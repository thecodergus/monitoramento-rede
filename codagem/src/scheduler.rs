// src/scheduler.rs
use crate::types::{Cycle, Probe, Target, TargetWarmupState};
use crate::{
    config::Config, consensus::ConsensusState, outage::OutageManager, ping, storage::Storage,
};
use chrono::Utc;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::time::interval;
use tracing::{error, info, warn};

pub async fn run_scheduler(
    config: Arc<Config>,
    storage: Arc<Storage>,
    mut consensus: ConsensusState,
    mut outage: OutageManager,
    probe: Probe,
    targets: Vec<Target>,
    main_start_time: Instant,
    grace_period_duration: Duration,
    mut warmup_state: TargetWarmupState,
) {
    let mut ticker = interval(Duration::from_secs(config.cycle_interval_secs));
    let mut cycle_number = 0;

    loop {
        ticker.tick().await;
        cycle_number += 1;
        let started_at = Utc::now();
        let cycle = Cycle {
            id: 0,
            started_at,
            ended_at: None,
            cycle_number,
            probe_count: 1,
        };

        let cycle_id = match storage.insert_cycle(&cycle).await {
            Ok(id) => id,
            Err(e) => {
                error!("Erro ao persistir ciclo: {:?}", e);
                continue;
            }
        };

        let results = ping::ping_targets(
            &targets,
            &probe,
            config.ping_count,
            config.timeout_secs,
            cycle_id,
        )
        .await;

        for res in &results {
            if let Err(e) = storage.insert_ping_result(res).await {
                error!(
                    "Erro ao persistir métrica para target {}: {:?}",
                    res.target_id, e
                );
            }
        }

        let current_time = Instant::now();

        for target in &targets {
            let is_target_up = results.iter().any(|r| r.target_id == target.id);

            // 1. Grace Period: Ignora falhas se ainda estiver no período de carência
            if current_time.duration_since(main_start_time) < grace_period_duration {
                if !is_target_up {
                    warn!(
                        "Grace period ativo para probe {} e target {}: ignorando falha temporária",
                        probe.location, target.name
                    );
                }
                continue;
            }

            // 3. Warmup: Só considera o target "aquecido" após N ciclos de sucesso
            let is_warmed_up = warmup_state.update(target.id, is_target_up);
            if !is_warmed_up {
                info!(
                    "Target {} ainda em warmup para probe {}: ignorando falhas até atingir {} ciclos de sucesso",
                    target.name, probe.location, warmup_state.required_streak
                );
                continue;
            }

            // 5. Persistência de Estado: Só registra outage se houver transição real de UP para DOWN
            let last_status_option =
                storage
                    .get_target_status(target.id)
                    .await
                    .unwrap_or_else(|e| {
                        error!(
                            "Erro ao obter status anterior do target {}: {:?}",
                            target.name, e
                        );
                        None
                    });
            let last_status = last_status_option.unwrap_or_else(|| "unknown".to_string());

            if last_status == "up" && !is_target_up {
                info!(
                    "Transição de UP para DOWN detectada para target {} (probe {}). Registrando outage.",
                    target.name, probe.location
                );
                let target_results: Vec<_> = results
                    .iter()
                    .filter(|r| r.target_id == target.id)
                    .cloned()
                    .collect();
                let maybe_outage = consensus.update(target_results, started_at);
                if let Some(event) = outage.handle_cycle(maybe_outage, started_at) {
                    if let Err(e) = storage.insert_outage_event(&event).await {
                        error!(
                            "Erro ao persistir outage para target {}: {:?}",
                            target.name, e
                        );
                    } else {
                        info!(
                            "Evento de outage registrado para target {}: {:?}",
                            target.name, event
                        );
                    }
                }
                if let Err(e) = storage.set_target_status(target.id, "down").await {
                    error!(
                        "Erro ao persistir status 'down' para target {}: {:?}",
                        target.name, e
                    );
                }
            } else if is_target_up && last_status != "up" {
                info!(
                    "Transição para UP detectada para target {} (probe {}). Atualizando status.",
                    target.name, probe.location
                );
                if let Err(e) = storage.set_target_status(target.id, "up").await {
                    error!(
                        "Erro ao persistir status 'up' para target {}: {:?}",
                        target.name, e
                    );
                }
            } else {
                let new_status = if is_target_up { "up" } else { "down" };
                if last_status != new_status {
                    if let Err(e) = storage.set_target_status(target.id, new_status).await {
                        error!(
                            "Erro ao persistir status '{}' para target {}: {:?}",
                            new_status, target.name, e
                        );
                    }
                }
            }
        }
    }
}
