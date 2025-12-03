use crate::{
    config::Config,
    consensus::ConsensusState,
    outage::OutageManager,
    ping,
    storage::Storage,
    types::{Cycle, Probe, Target},
};
use chrono::Utc;
use std::sync::Arc;
use tokio::time::{Duration, interval};
use tracing::{error, info};

pub async fn run_scheduler(
    config: Arc<Config>,
    storage: Arc<Storage>,
    mut consensus: ConsensusState,
    mut outage: OutageManager,
    probe: Probe,
    targets: Vec<Target>,
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
        let cycle_id = storage.insert_cycle(&cycle).await.unwrap();

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
                error!("Erro ao persistir métrica: {:?}", e);
            }
        }
        let maybe_outage = consensus.update(results.clone(), started_at);
        if let Some(event) = outage.handle_cycle(maybe_outage, started_at) {
            if let Err(e) = storage.insert_outage_event(&event).await {
                error!("Erro ao persistir outage: {:?}", e);
            } else {
                info!("Evento de outage registrado: {:?}", event);
            }
        }
    }
}
