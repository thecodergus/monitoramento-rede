use config as config_crate;
use serde::Deserialize;

/// Configuração operacional do sistema.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Quantidade de tentativas de ping por ciclo.
    pub ping_count: usize,
    /// Timeout em segundos para cada ping.
    pub timeout_secs: u64,
    /// Threshold de falhas para acionar consenso.
    pub fail_threshold: usize,
    /// Nível de consenso para considerar outage.
    pub consensus: usize,
    /// Intervalo entre ciclos em segundos.
    pub cycle_interval_secs: u64,
    /// URL de conexão com o banco PostgreSQL.
    pub database_url: String,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let settings = config_crate::Config::builder()
            .add_source(config_crate::File::with_name("config"))
            .build()?;
        let config: Config = settings.try_deserialize()?; // CORRETO!
        Ok(config)
    }
    /// Validação customizada (opcional)
    pub fn validate(&self) -> Result<(), String> {
        if self.ping_count == 0 {
            return Err("ping_count deve ser maior que zero".into());
        }
        if self.timeout_secs == 0 {
            return Err("timeout_secs deve ser maior que zero".into());
        }
        Ok(())
    }
}
