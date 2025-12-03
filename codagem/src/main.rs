mod config;
mod consensus;
mod outage;
mod ping;
mod scheduler;
mod storage;
mod types;

use anyhow::Result;
use std::sync::Arc;
use tracing_subscriber; // ✅ Importação adicionada

#[tokio::main]
async fn main() -> Result<()> {
    // Inicializa o sistema de logging (tracing)
    tracing_subscriber::fmt::init();

    // Carrega a configuração da aplicação
    let config = Arc::new(config::Config::load()?);

    // Conecta ao banco de dados usando a nova implementação tokio_postgres
    let storage: Arc<storage::Storage> =
        Arc::new(storage::Storage::connect(&config.database_url).await?);

    // Busca todos os alvos monitorados
    let targets = storage.list_targets().await?;
    if targets.is_empty() {
        anyhow::bail!("Nenhum alvo registrado no banco de dados");
    }

    // Busca todos os probes cadastrados
    let probes = storage.list_probes().await?;
    if probes.is_empty() {
        anyhow::bail!("Nenhum probe registrado no banco de dados");
    }

    // Seleciona o primeiro probe como exemplo (ajuste conforme sua lógica)
    let probe = probes[0].clone();

    // Cria o estado de consenso
    let consensus = consensus::ConsensusState::new(config.fail_threshold, config.consensus);

    // Cria o gerenciador de outages
    let outage_manager = outage::OutageManager::new();

    // Executa o scheduler principal
    scheduler::run_scheduler(config, storage, consensus, outage_manager, probe, targets).await;

    Ok(())
}
