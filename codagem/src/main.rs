// src/main.rs
mod config;
mod consensus;
mod outage;
mod ping;
mod scheduler;
mod storage;
mod types;

use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::Duration;
use tokio::task;
use tokio::time::timeout;
use tracing::{debug, error, info, warn};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().init();

    info!("🚀 Iniciando aplicação de monitoramento de rede...");

    // Carregando configuração
    info!("🔧 Carregando configuração...");
    let config: Arc<config::Config> =
        Arc::new(config::Config::load().context("Falha ao carregar configuração")?);
    debug!("Configuração carregada: {:?}", config);

    // Conectando ao banco de dados com timeout
    info!("🗄️  Conectando ao banco de dados...");
    let storage: Arc<storage::Storage> = Arc::new(
        timeout(
            Duration::from_secs(10),
            storage::Storage::connect(&config.database_url),
        )
        .await
        .context("Timeout ao conectar ao banco de dados")??,
    );
    info!("✅ Conexão ao banco de dados estabelecida.");

    // Listando targets
    info!("🎯 Consultando targets...");
    let targets: Vec<types::Target> = timeout(Duration::from_secs(8), storage.list_targets())
        .await
        .context("Timeout ao consultar targets")??;
    info!("Targets encontrados: {}", targets.len());
    if targets.is_empty() {
        error!("Nenhum alvo registrado no banco de dados");
        anyhow::bail!("Nenhum alvo registrado no banco de dados");
    }

    // Listando probes
    info!("📡 Consultando probes...");
    let probes: Vec<types::Probe> = timeout(Duration::from_secs(8), storage.list_probes())
        .await
        .context("Timeout ao consultar probes")??;
    info!("Probes encontrados: {}", probes.len());
    if probes.is_empty() {
        error!("Nenhum probe registrado no banco de dados");
        anyhow::bail!("Nenhum probe registrado no banco de dados");
    }

    // Spawn de schedulers para cada probe
    let mut handles: Vec<task::JoinHandle<()>> = Vec::new();
    for probe in probes {
        let config = Arc::clone(&config);
        let storage = Arc::clone(&storage);
        let targets = targets.clone();

        info!("🟢 Spawnando scheduler para probe: {:?}", probe);

        let handle =
            task::spawn(
                async move { scheduler::run_scheduler(config, storage, probe, targets).await },
            );
        handles.push(handle);
    }

    // Pattern matching idiomático para tratar panics e erros via JoinHandle
    let mut panic_count = 0;
    let mut error_count = 0;

    for (index, handle) in handles.into_iter().enumerate() {
        match handle.await {
            Ok(_) => {
                info!("✅ Scheduler {} finalizado com sucesso", index + 1);
            }
            Err(join_err) if join_err.is_panic() => {
                panic_count += 1;
                error!("💥 Task {} panicked: {:?}", index + 1, join_err);
            }
            Err(join_err) if join_err.is_cancelled() => {
                warn!("🚫 Task {} foi cancelada: {:?}", index + 1, join_err);
            }
            Err(join_err) => {
                error_count += 1;
                error!("❌ Scheduler {} error: {:?}", index + 1, join_err);
            }
        }
    }

    if panic_count > 0 || error_count > 0 {
        warn!(
            "Aplicação finalizada com {} panics e {} erros",
            panic_count, error_count
        );
    } else {
        info!("🏁 Aplicação finalizada com sucesso - todos os schedulers terminaram normalmente.");
    }

    Ok(())
}
