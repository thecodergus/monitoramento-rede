// src/scheduler.rs

//! Scheduler de monitoramento de rede — compatível com types.rs moderno
//!
//! Orquestra ciclos de monitoramento, verifica conectividade, coleta métricas
//! e persiste resultados usando tipos granulares e idiomáticos.
//!
//! - Usa ConnectivityMetric (não mais PingResult)
//! - Endereços IP são IpAddr (não String)
//! - Enum MetricType granular (PingIpv4/PingIpv6)
//! - Lógica funcional, concorrente e auditável

use crate::consensus::ConsensusState;
use crate::types::{
    ConnectivityMetric, Cycle, MetricStatus, MetricType, OutageEvent, Probe, SchedulerState,
    Target, TargetWarmupState,
};
use crate::{config::Config, ping, storage::Storage};
use chrono::Utc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::sync::{Mutex, MutexGuard};
use tokio::time::interval;
use tracing::{debug, error, info, warn};
use trust_dns_resolver::TokioAsyncResolver;

/// Verificação multi-método de conectividade.
/// Tenta TCP connect, resolução DNS e ICMP/ping (fallback).
/// Loga detalhadamente cada tentativa e motivo de falha.
/// Retorna true se qualquer método/alvo responder.
async fn check_connectivity_resilient(targets: &[Target], probe: &Probe, config: &Config) -> bool {
    // 1. TCP connect para portas 53, 80, 443 em todos os targets
    let tcp_ports = [53u16, 80, 443];
    for target in targets {
        for &port in &tcp_ports {
            let addr = format!("{}:{}", target.address, port);
            match tokio::time::timeout(Duration::from_secs(3), TcpStream::connect(&addr)).await {
                Ok(Ok(_)) => {
                    info!(
                        "[PROBE {}] TCP connect OK em {}:{} (target: {})",
                        probe.location, target.address, port, target.name
                    );
                    return true;
                }
                Ok(Err(e)) => {
                    warn!(
                        "[PROBE {}] Falha TCP connect em {}:{} (target: {}): {:?}",
                        probe.location, target.address, port, target.name, e
                    );
                }
                Err(_) => {
                    warn!(
                        "[PROBE {}] Timeout TCP connect em {}:{} (target: {})",
                        probe.location, target.address, port, target.name
                    );
                }
            }
        }
    }

    // 2. Resolução DNS (usando trust-dns)
    if let Ok(resolver) = TokioAsyncResolver::tokio_from_system_conf() {
        for target in targets {
            // Tenta resolver o nome reverso do IP
            match resolver.reverse_lookup(target.address).await {
                Ok(response) if response.iter().next().is_some() => {
                    // resposta DNS reversa não vazia
                    info!(
                        "[PROBE {}] DNS reverso OK para {} (target: {})",
                        probe.location, target.address, target.name
                    );
                    return true;
                }
                Ok(_) | Err(_) => {
                    warn!(
                        "[PROBE {}] Falha DNS reverso para {} (target: {})",
                        probe.location, target.address, target.name
                    );
                }
            }
        }
    }

    // 3. ICMP/ping (fallback)
    let ping_results = ping::ping_targets(
        targets, probe, 1, // apenas 1 tentativa rápida
        2, // timeout curto
        0, // ciclo fictício
    )
    .await;
    if ping_results.iter().any(|m| m.status == MetricStatus::Up) {
        info!(
            "[PROBE {}] ICMP ping OK para pelo menos um target",
            probe.location
        );
        return true;
    }

    warn!(
        "[PROBE {}] Nenhum método de conectividade teve sucesso",
        probe.location
    );
    false
}

/// Loop principal do scheduler para um probe.
/// Executa ciclos de monitoramento, coleta métricas e persiste resultados.
/// - Aguarda internet antes de iniciar ciclos
/// - Usa TargetWarmupState para evitar falsos positivos
/// - Integra com storage, ping e consensus

pub async fn run_scheduler(
    probe: Probe,
    targets: Vec<Target>,
    config: Arc<Config>,
    storage: Arc<Storage>,
    consensus_state: Arc<Mutex<ConsensusState>>,
) {
    let mut state: SchedulerState = SchedulerState::WaitingForInternet;
    let mut warmup: TargetWarmupState = TargetWarmupState::new(3);
    let mut cycle_number = 0;

    let mut ticker: tokio::time::Interval =
        interval(Duration::from_secs(config.cycle_interval_secs));
    loop {
        ticker.tick().await;
        let now = Utc::now();

        match state {
            SchedulerState::WaitingForInternet => {
                info!(
                    "[PROBE {}] Aguardando conectividade de internet...",
                    probe.location
                );

                // Coleta métricas (todas Down) mesmo sem internet
                let metrics = ping::ping_targets(
                    &targets,
                    &probe,
                    config.ping_count,
                    config.timeout_secs,
                    0, // ciclo fictício
                )
                .await;

                let now: chrono::DateTime<Utc> = Utc::now();

                // Atualiza o consenso e loga o histórico
                let outage_event_opt: Option<OutageEvent> = {
                    let mut consensus: MutexGuard<'_, ConsensusState> =
                        consensus_state.lock().await;
                    debug!(
                        "[CONSENSUS {}] [WAITING] Lock ConsensusState OK, histórico: {} ciclos",
                        probe.location,
                        consensus.history.len()
                    );
                    let result: Option<OutageEvent> = consensus.update(metrics.clone(), now);
                    debug!(
                        "[CONSENSUS {}] [WAITING] ConsensusState::update = {:?} | Histórico: {} ciclos",
                        probe.location,
                        result,
                        consensus.history.len()
                    );
                    result
                };

                if let Some(outage_event) = outage_event_opt {
                    info!(
                        "[CONSENSUS {}] [WAITING] Outage detectado/encerrado: {:?}",
                        probe.location, outage_event
                    );
                    if let Err(e) = storage.insert_outage_event(&outage_event).await {
                        error!(
                            "[CONSENSUS {}] [WAITING] Falha ao persistir outage: {:?}",
                            probe.location, e
                        );
                    }
                } else {
                    debug!(
                        "[CONSENSUS {}] [WAITING] Sem outages detectados neste ciclo (sem internet)",
                        probe.location
                    );
                }

                // Checa se a internet voltou
                if check_connectivity_resilient(&targets, &probe, &config).await {
                    info!(
                        "[PROBE {}] Internet detectada, iniciando monitoramento.",
                        probe.location
                    );
                    state = SchedulerState::Monitoring;
                }
            }

            SchedulerState::Monitoring => {
                cycle_number += 1;
                let cycle = Cycle {
                    id: 0,
                    started_at: now,
                    ended_at: None,
                    cycle_number,
                    probe_count: 1,
                };
                let cycle_id = match storage.insert_cycle(&cycle).await {
                    Ok(id) => id,
                    Err(e) => {
                        error!(
                            "[PROBE {}] Falha ao inserir ciclo no banco: {:?}",
                            probe.location, e
                        );
                        continue;
                    }
                };

                let metrics: Vec<ConnectivityMetric> = ping::ping_targets(
                    &targets,
                    &probe,
                    config.ping_count,
                    config.timeout_secs,
                    cycle_id,
                )
                .await;

                for metric in &metrics {
                    if let Err(e) = storage.insert_connectivity_metric(metric).await {
                        error!(
                            "[PROBE {}] Falha ao persistir métrica: {:?} (target_id: {})",
                            probe.location, e, metric.target_id
                        );
                    }
                }

                for metric in &metrics {
                    let is_success: bool = metric.status == MetricStatus::Up;
                    let warmed: bool = warmup.update(metric.target_id, is_success);
                    debug!(
                        "[PROBE {}] Target {} warmup: {} (status: {:?})",
                        probe.location, metric.target_id, warmed, metric.status
                    );
                    if let Err(e) = storage
                        .set_target_status(metric.target_id, &metric.status)
                        .await
                    {
                        warn!(
                            "[PROBE {}] Falha ao atualizar status do target {}: {:?}",
                            probe.location, metric.target_id, e
                        );
                    }
                }

                // 3️⃣ INTEGRAÇÃO DO CONSENSO: Atualiza ConsensusState e persiste outages
                let mut consensus: MutexGuard<'_, ConsensusState> = consensus_state.lock().await;
                let now: chrono::DateTime<Utc> = Utc::now();

                if let Some(outage_event) = consensus.update(metrics.clone(), now) {
                    info!(
                        "[CONSENSO {}] Outage detectado: {:?}",
                        probe.location, outage_event
                    );
                    if let Err(e) = storage.insert_outage_event(&outage_event).await {
                        error!(
                            "[CONSENSO {}] Falha ao persistir outage: {:?}",
                            probe.location, e
                        );
                    }
                } else {
                    info!(
                        "[CONSENSO {}] Sem outages detectados neste ciclo",
                        probe.location
                    );
                }
                drop(consensus);

                if !check_connectivity_resilient(&targets, &probe, &config).await {
                    warn!(
                        "[PROBE {}] Perda de conectividade detectada, retornando para WAITING_FOR_INTERNET.",
                        probe.location
                    );
                    state = SchedulerState::WaitingForInternet;
                }
            }
        }
    }
}
