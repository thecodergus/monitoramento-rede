// src/scheduler.rs

use crate::types::{Cycle, Probe, Target};
use crate::{config::Config, ping, storage::Storage};
use chrono::Utc;
use std::net::Ipv4Addr;
use std::{sync::Arc, time::Duration, time::Instant};
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use tokio::net::TcpStream;
use trust_dns_resolver::TokioAsyncResolver;

/// Enum de estados do scheduler: aguardando internet ou monitorando.
enum SchedulerState {
    WaitingForInternet,
    Monitoring,
}

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

    // 2. Resolução DNS para domínios conhecidos
    let resolver = match TokioAsyncResolver::tokio_from_system_conf() {
        Ok(r) => r,
        Err(e) => {
            error!(
                "[PROBE {}] Falha ao inicializar resolver DNS: {:?}",
                probe.location, e
            );
            // Não retorna, tenta ICMP/ping
            return false;
        }
    };
    let dns_domains = ["www.google.com", "cloudflare.com"];

    for domain in &dns_domains {
        match tokio::time::timeout(Duration::from_secs(3), resolver.lookup_ip(domain)).await {
            Ok(Ok(lookup)) if lookup.iter().next().is_some() => {
                info!(
                    "[PROBE {}] Resolução DNS OK para {}",
                    probe.location, domain
                );
                return true;
            }
            Ok(Ok(_)) => {
                warn!(
                    "[PROBE {}] Resolução DNS para {} não retornou IPs",
                    probe.location, domain
                );
            }
            Ok(Err(e)) => {
                warn!(
                    "[PROBE {}] Falha na resolução DNS para {}: {:?}",
                    probe.location, domain, e
                );
            }
            Err(_) => {
                warn!(
                    "[PROBE {}] Timeout na resolução DNS para {}",
                    probe.location, domain
                );
            }
        }
    }

    // 3. ICMP/ping como fallback final
    let result = ping::ping_targets(
        targets,
        probe,
        config.ping_count,
        config.timeout_secs,
        0, // cycle_id fictício
    )
    .await;
    if result
        .iter()
        .any(|r| r.status == crate::types::MetricStatus::Up)
    {
        info!("[PROBE {}] ICMP/ping OK em algum target", probe.location);
        return true;
    } else {
        warn!(
            "[PROBE {}] Falha em todas as tentativas de ICMP/ping.",
            probe.location
        );
    }

    error!(
        "[PROBE {}] Nenhum método de verificação de conectividade teve sucesso.",
        probe.location
    );
    false
}

/// Scheduler principal: verifica internet uma vez, depois executa pings infinitamente.
pub async fn run_scheduler(
    config: Arc<Config>,
    storage: Arc<Storage>,
    probe: Probe,
    targets: Vec<Target>,
) {
    let mut state = SchedulerState::WaitingForInternet;

    loop {
        match state {
            SchedulerState::WaitingForInternet => {
                info!(
                    "[ESTADO: AGUARDANDO INTERNET] [PROBE {}] Aguardando conectividade...",
                    probe.location
                );
                if check_connectivity_resilient(&targets, &probe, &config).await {
                    info!(
                        "[ESTADO: MONITORAMENTO ATIVO] [PROBE {}] Internet detectada, iniciando monitoramento contínuo.",
                        probe.location
                    );
                    state = SchedulerState::Monitoring;
                } else {
                    warn!(
                        "[PROBE {}] Internet não detectada. Nova tentativa em 10 segundos...",
                        probe.location
                    );
                    tokio::time::sleep(Duration::from_secs(10)).await;
                }
            }
            SchedulerState::Monitoring => {
                let mut ticker = interval(Duration::from_secs(config.cycle_interval_secs));
                let mut cycle_number = 0;

                loop {
                    let cycle_start = Instant::now();
                    ticker.tick().await;
                    cycle_number += 1;
                    let started_at = Utc::now();

                    info!(
                        "[PROBE {}][CICLO {}] Iniciando ciclo em {}.",
                        probe.location, cycle_number, started_at
                    );

                    // 1. Persistência do ciclo
                    let cycle = Cycle {
                        id: 0,
                        started_at,
                        ended_at: None,
                        cycle_number,
                        probe_count: 1,
                    };
                    let cycle_id = match storage.insert_cycle(&cycle).await {
                        Ok(id) => {
                            debug!(
                                "[PROBE {}][CICLO {}] Ciclo persistido com id {}.",
                                probe.location, cycle_number, id
                            );
                            id
                        }
                        Err(e) => {
                            error!(
                                "[PROBE {}][CICLO {}] Erro ao persistir ciclo: {:?}",
                                probe.location, cycle_number, e
                            );
                            continue;
                        }
                    };

                    // 2. Execução dos pings
                    info!(
                        "[PROBE {}][CICLO {}] Executando pings para {} targets...",
                        probe.location,
                        cycle_number,
                        targets.len()
                    );
                    let ping_start = Instant::now();
                    let results = ping::ping_targets(
                        &targets,
                        &probe,
                        config.ping_count,
                        config.timeout_secs,
                        cycle_id,
                    )
                    .await;
                    let ping_duration = ping_start.elapsed();
                    info!(
                        "[PROBE {}][CICLO {}] Pings finalizados em {:?}.",
                        probe.location, cycle_number, ping_duration
                    );

                    // 3. Persistência dos resultados de ping
                    for res in &results {
                        debug!(
                            "[PROBE {}][CICLO {}] Ping target {}: status={:?}",
                            probe.location, cycle_number, res.target_id, res.status
                        );
                        match storage.insert_ping_result(res).await {
                            Ok(_) => debug!(
                                "[PROBE {}][CICLO {}] Métrica persistida para target {}.",
                                probe.location, cycle_number, res.target_id
                            ),
                            Err(e) => error!(
                                "[PROBE {}][CICLO {}] Erro ao persistir métrica para target {}: {:?}",
                                probe.location, cycle_number, res.target_id, e
                            ),
                        }
                    }

                    let cycle_duration = cycle_start.elapsed();
                    info!(
                        "[PROBE {}][CICLO {}] Fim do ciclo. Duração: {:?}\n",
                        probe.location, cycle_number, cycle_duration
                    );
                }
            }
        }
    }
}
