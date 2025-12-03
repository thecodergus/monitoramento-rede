mod config;
mod consensus;
mod outage;
mod ping;
mod scheduler;
mod storage;
mod types;

use std::sync::Arc;
use tracing_subscriber;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let config: Arc<config::Config> = Arc::new(config::Config::load()?);
    let storage: Arc<storage::Storage> =
        Arc::new(storage::Storage::connect(&config.database_url).await?);

    // Carrega todos os targets e probes do banco
    let targets: Vec<types::Target> = storage.list_targets().await?;
    if targets.is_empty() {
        panic!("Nenhum target cadastrado no banco!");
    }
    let probes: Vec<types::Probe> = storage.list_probes().await?;
    if probes.is_empty() {
        panic!("Nenhum probe cadastrado no banco!");
    }
    // Exemplo: usa o primeiro probe cadastrado
    let probe: types::Probe = probes[0].clone();

    let consensus: consensus::ConsensusState =
        consensus::ConsensusState::new(config.fail_threshold, config.consensus);
    let outage: outage::OutageManager = outage::OutageManager::new();
    scheduler::run_scheduler(config, storage, consensus, outage, probe, targets).await;
    Ok(())
}
