// src/scheduler.rs

use crate::types::{Cycle, Probe, Target};
use crate::{config::Config, ping, storage::Storage};
use chrono::Utc;
use std::{sync::Arc, time::Duration, time::Instant};
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// Enum de estados do scheduler: aguardando internet ou monitorando.
enum SchedulerState {
    WaitingForInternet,
    Monitoring,
}

/// Função auxiliar para checar conectividade.
/// Retorna true se a internet está disponível, false caso contrário.
/// Aqui, faz ping no primeiro target como referência.
async fn check_connectivity(targets: &[Target], probe: &Probe, config: &Config) -> bool {
    let reference_target = targets.get(0);
    if let Some(target) = reference_target {
        info!(
            "[PROBE {}] Verificando conectividade inicial com target de referência: {}",
            probe.location, target.name
        );
        let result = ping::ping_targets(
            &[target.clone()],
            probe,
            config.ping_count,
            config.timeout_secs,
            0, // cycle_id fictício
        )
        .await;
        let is_up = result
            .iter()
            .any(|r| r.status == crate::types::MetricStatus::Up);
        if is_up {
            info!(
                "[PROBE {}] Conectividade inicial confirmada com target {}.",
                probe.location, target.name
            );
        } else {
            warn!(
                "[PROBE {}] Falha na verificação inicial de conectividade com target {}.",
                probe.location, target.name
            );
        }
        is_up
    } else {
        error!(
            "[PROBE {}] Nenhum target disponível para verificação de conectividade!",
            probe.location
        );
        false
    }
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
                if check_connectivity(&targets, &probe, &config).await {
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
