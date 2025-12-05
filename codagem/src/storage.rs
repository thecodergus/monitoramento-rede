use crate::types::{
    ConnectivityMetric, Cycle, MetricStatus, MetricType, OutageEvent, Probe, Target, TargetStatus,
};
use anyhow::Result;
use tokio_postgres::{Client, NoTls, Row};

/// Storage: Camada de persistência usando tokio_postgres
///
/// Esta estrutura fornece uma interface idiomática para interações com PostgreSQL,
/// utilizando tipos seguros e padrões funcionais do Rust. Todos os métodos são
/// assíncronos e retornam Result<T> para tratamento robusto de erros.
pub struct Storage {
    client: Client,
}

impl Storage {
    /// Conecta ao banco de dados PostgreSQL e retorna um Storage pronto para uso.
    ///
    /// # Arguments
    /// * `database_url` - URL de conexão PostgreSQL (formato: postgresql://user:pass@host:port/db)
    ///
    /// # Returns
    /// * `Result<Self>` - Instância de Storage ou erro de conexão
    ///
    /// # Example
    /// ```
    /// let storage = Storage::connect("postgresql://localhost/monitoring").await?;
    /// ```
    pub async fn connect(database_url: &str) -> Result<Self> {
        let (client, connection) = tokio_postgres::connect(database_url, NoTls).await?;
        // Spawn a task to drive the connection
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("Postgres connection error: {}", e);
            }
        });
        Ok(Self { client })
    }

    /// Lista todos os targets monitorados.
    ///
    /// # Returns
    /// * `Result<Vec<Target>>` - Lista de targets ou erro de consulta
    pub async fn list_targets(&self) -> Result<Vec<Target>> {
        let rows = self
            .client
            .query(
                "SELECT id, name, address, asn, provider, type, region, created_at FROM monitoring_targets ORDER BY id",
                &[],
            )
            .await?;
        Ok(rows.into_iter().map(Target::from).collect())
    }

    /// Lista todos os probes cadastrados.
    ///
    /// # Returns
    /// * `Result<Vec<Probe>>` - Lista de probes ou erro de consulta
    pub async fn list_probes(&self) -> Result<Vec<Probe>> {
        let rows = self
            .client
            .query(
                "SELECT id, location, ip_address, provider, created_at FROM monitoring_probes ORDER BY id",
                &[],
            )
            .await?;
        Ok(rows.into_iter().map(Probe::from).collect())
    }

    /// Insere um novo ciclo de monitoramento e retorna o id gerado.
    ///
    /// # Returns
    /// * `Result<i64>` - ID do ciclo inserido ou erro de inserção
    pub async fn insert_cycle(&self, cycle: &Cycle) -> Result<i64> {
        let row = self
            .client
            .query_one(
                "INSERT INTO monitoring_cycles (started_at, ended_at, cycle_number, probe_count)
                 VALUES ($1, $2, $3, $4) RETURNING id",
                &[
                    &cycle.started_at,
                    &cycle.ended_at,
                    &cycle.cycle_number,
                    &cycle.probe_count,
                ],
            )
            .await?;
        Ok(row.get("id"))
    }

    /// Insere uma métrica de conectividade (ping, tcp, http, dns).
    ///
    /// # Returns
    /// * `Result<()>` - Sucesso ou erro de inserção
    pub async fn insert_connectivity_metric(&self, metric: &ConnectivityMetric) -> Result<()> {
        self.client
            .execute(
                "INSERT INTO connectivity_metrics
                 (cycle_id, probe_id, target_id, timestamp, metric_type, status, response_time_ms, packet_loss_percent, error_message)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
                &[
                    &metric.cycle_id,
                    &metric.probe_id,
                    &metric.target_id,
                    &metric.timestamp,
                    &metric.metric_type,
                    &metric.status,
                    &metric.response_time_ms,
                    &metric.packet_loss_percent,
                    &metric.error_message,
                ],
            )
            .await?;
        Ok(())
    }

    /// Insere um evento de outage.
    ///
    /// # Returns
    /// * `Result<()>` - Sucesso ou erro de inserção
    pub async fn insert_outage_event(&self, event: &OutageEvent) -> Result<()> {
        self.client
            .execute(
                "INSERT INTO outage_events
                 (start_time, end_time, duration_seconds, reason, affected_targets, affected_probes, consensus_level, details)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                &[
                    &event.start_time,
                    &event.end_time,
                    &event.duration_seconds,
                    &event.reason,
                    &event.affected_targets,
                    &event.affected_probes,
                    &event.consensus_level,
                    &event.details,
                ],
            )
            .await?;
        Ok(())
    }

    /// Recupera o último status persistido do target.
    ///
    /// # Returns
    /// * `Result<Option<MetricStatus>>` - Status ou None se não encontrado
    pub async fn get_target_status(&self, target_id: i32) -> Result<Option<MetricStatus>> {
        let row = self
            .client
            .query_opt(
                "SELECT last_status FROM target_status WHERE target_id = $1",
                &[&target_id],
            )
            .await?;
        Ok(row.map(|r| r.get("last_status")))
    }

    /// Atualiza o status persistido do target.
    ///
    /// # Returns
    /// * `Result<()>` - Sucesso ou erro de atualização
    pub async fn set_target_status(&self, target_id: i32, status: &MetricStatus) -> Result<()> {
        self.client
            .execute(
                "INSERT INTO target_status (target_id, last_status, last_change)
                 VALUES ($1, $2, NOW())
                 ON CONFLICT (target_id) DO UPDATE SET last_status = $2, last_change = NOW()",
                &[&target_id, status],
            )
            .await?;
        Ok(())
    }

    /// Lista métricas de conectividade de um ciclo específico.
    ///
    /// # Returns
    /// * `Result<Vec<ConnectivityMetric>>` - Lista de métricas do ciclo
    pub async fn list_connectivity_metrics_by_cycle(
        &self,
        cycle_id: i64,
    ) -> Result<Vec<ConnectivityMetric>> {
        let rows = self
            .client
            .query(
                "SELECT id, cycle_id, probe_id, target_id, timestamp, metric_type, status, response_time_ms, packet_loss_percent, error_message
                 FROM connectivity_metrics
                 WHERE cycle_id = $1
                 ORDER BY timestamp",
                &[&cycle_id],
            )
            .await?;
        Ok(rows.into_iter().map(ConnectivityMetric::from).collect())
    }

    /// Lista status de todos os targets.
    ///
    /// # Returns
    /// * `Result<Vec<TargetStatus>>` - Lista de status dos targets
    pub async fn list_all_target_status(&self) -> Result<Vec<TargetStatus>> {
        let rows = self
            .client
            .query(
                "SELECT target_id, last_status, last_change FROM target_status ORDER BY target_id",
                &[],
            )
            .await?;
        Ok(rows.into_iter().map(TargetStatus::from).collect())
    }
}
