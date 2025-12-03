mod config;
mod consensus;
mod outage;
mod ping;
mod scheduler;
mod storage;
mod types;

use anyhow::Result;
use std::sync::Arc;
use tokio::task;
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Inicializa o sistema de logging (tracing)
    tracing_subscriber::fmt::init();

    // Carrega a configuração da aplicação (Arc para compartilhamento seguro)
    println!("{:?}", config::Config::load());
    let config: Arc<config::Config> = Arc::new(config::Config::load()?);
    println!("Configuração feita");

    // Conecta ao banco de dados usando tokio_postgres (Arc para tasks)
    let storage: Arc<storage::Storage> =
        Arc::new(storage::Storage::connect(&config.database_url).await?);
    println!("Banco de dados conectado");

    // Busca todos os alvos monitorados
    let targets: Vec<types::Target> = storage.list_targets().await?;
    if targets.is_empty() {
        anyhow::bail!("Nenhum alvo registrado no banco de dados");
    }
    println!("Alvos recuperados do banco de dados");

    // Busca todos os probes cadastrados
    let probes: Vec<types::Probe> = storage.list_probes().await?;
    if probes.is_empty() {
        anyhow::bail!("Nenhum probe registrado no banco de dados");
    }
    println!("Probes recuperados do banco de dados");

    // Cria o estado de consenso e o gerenciador de outages (precisam ser Clone)
    let consensus: consensus::ConsensusState =
        consensus::ConsensusState::new(config.fail_threshold, config.consensus.clone());
    let outage_manager: outage::OutageManager = outage::OutageManager::new();

    println!("Spawmando as tasks para cada probe");
    // Spawna uma task para cada probe, clonando consensus/outage_manager para cada uma
    let mut handles: Vec<task::JoinHandle<()>> = Vec::new();
    for probe in probes {
        let config: Arc<config::Config> = Arc::clone(&config);
        let storage: Arc<storage::Storage> = Arc::clone(&storage);
        let consensus: consensus::ConsensusState = consensus.clone(); // Clone para ownership exclusivo
        let outage_manager: outage::OutageManager = outage_manager.clone(); // Clone para ownership exclusivo
        let targets: Vec<types::Target> = targets.clone();

        let handle: task::JoinHandle<()> = task::spawn(async move {
            scheduler::run_scheduler(config, storage, consensus, outage_manager, probe, targets)
                .await
        });
        handles.push(handle);
    }

    println!("Iniciando os trabalhos");
    // Aguarda todos os schedulers finalizarem
    for handle in handles {
        if let Err(e) = handle.await {
            eprintln!("Scheduler error: {:?}", e);
        }
    }

    Ok(())
}
