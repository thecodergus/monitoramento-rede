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

use crate::types::{
    ConnectivityMetric, Cycle, MetricStatus, MetricType, Probe, SchedulerState, Target,
    TargetWarmupState,
};
use crate::{config::Config, ping, storage::Storage};
use chrono::Utc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use tokio::net::TcpStream;
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
) {
    let mut state = SchedulerState::WaitingForInternet;
    let mut warmup = TargetWarmupState::new(3); // Exemplo: 3 ciclos de sucesso para "aquecimento"
    let mut cycle_number = 0;

    let mut ticker = interval(Duration::from_secs(config.cycle_interval_secs));
    loop {
        ticker.tick().await;
        let now = Utc::now();

        match state {
            SchedulerState::WaitingForInternet => {
                info!(
                    "[PROBE {}] Aguardando conectividade de internet...",
                    probe.location
                );
                if check_connectivity_resilient(&targets, &probe, &config).await {
                    info!(
                        "[PROBE {}] Internet detectada, iniciando monitoramento.",
                        probe.location
                    );
                    state = SchedulerState::Monitoring;
                } else {
                    continue;
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
                // Persiste ciclo e obtém id
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

                // Coleta métricas de ping concorrente
                let metrics = ping::ping_targets(
                    &targets,
                    &probe,
                    config.ping_count,
                    config.timeout_secs,
                    cycle_id,
                )
                .await;

                // Persiste métricas
                for metric in &metrics {
                    if let Err(e) = storage.insert_connectivity_metric(metric).await {
                        error!(
                            "[PROBE {}] Falha ao persistir métrica: {:?} (target_id: {})",
                            probe.location, e, metric.target_id
                        );
                    }
                }

                // Atualiza warmup e status dos targets
                for metric in &metrics {
                    let is_success = metric.status == MetricStatus::Up;
                    let warmed = warmup.update(metric.target_id, is_success);
                    debug!(
                        "[PROBE {}] Target {} warmup: {} (status: {:?})",
                        probe.location, metric.target_id, warmed, metric.status
                    );
                    // Atualiza status persistido
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

                // TODO: Integrar lógica de consenso/outage se necessário

                // Verifica se perdeu conectividade geral
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
