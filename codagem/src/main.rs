// src/main.rs
mod config;
mod consensus;
mod outage;
mod ping;
mod scheduler;
mod storage;
mod types;

use anyhow::Result;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::task;
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let config: Arc<config::Config> = Arc::new(config::Config::load()?);
    let storage: Arc<storage::Storage> =
        Arc::new(storage::Storage::connect(&config.database_url).await?);

    let targets: Vec<types::Target> = storage.list_targets().await?;
    if targets.is_empty() {
        anyhow::bail!("Nenhum alvo registrado no banco de dados");
    }

    let probes: Vec<types::Probe> = storage.list_probes().await?;
    if probes.is_empty() {
        anyhow::bail!("Nenhum probe registrado no banco de dados");
    }

    let consensus: consensus::ConsensusState =
        consensus::ConsensusState::new(config.fail_threshold, config.consensus.clone());
    let outage_manager: outage::OutageManager = outage::OutageManager::new();

    // --- Melhorias: Grace Period, Warmup State ---
    let start_time = Instant::now();
    let grace_period = Duration::from_secs(30); // Ajuste conforme necessário
    let initial_warmup_state = types::TargetWarmupState::new(3); // Exemplo: 3 ciclos de sucesso

    let mut handles: Vec<task::JoinHandle<()>> = Vec::new();
    for probe in probes {
        let config = Arc::clone(&config);
        let storage = Arc::clone(&storage);
        let consensus = consensus.clone();
        let outage_manager = outage_manager.clone();
        let targets = targets.clone();
        let current_warmup_state = initial_warmup_state.clone();

        let handle = task::spawn(async move {
            scheduler::run_scheduler(
                config,
                storage,
                consensus,
                outage_manager,
                probe,
                targets,
                start_time,
                grace_period,
                current_warmup_state,
            )
            .await
        });
        handles.push(handle);
    }

    for handle in handles {
        if let Err(e) = handle.await {
            eprintln!("Scheduler error: {:?}", e);
        }
    }

    Ok(())
}
