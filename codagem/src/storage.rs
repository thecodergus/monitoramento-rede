use crate::types::{Cycle, MetricStatus, MetricType, OutageEvent, PingResult, Probe, Target};
use anyhow::Result;
use tokio_postgres::{Client, NoTls, Row};

/// Storage: Camada de persistência usando tokio_postgres
pub struct Storage {
    client: Client,
}

impl Storage {
    /// Conecta ao banco de dados PostgreSQL e retorna um Storage pronto para uso.
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
    pub async fn list_targets(&self) -> Result<Vec<Target>> {
        let rows = self
            .client
            .query(
                "SELECT id, name, address, asn, provider, type, region FROM monitoring_targets",
                &[],
            )
            .await?;
        Ok(rows.into_iter().map(Target::from).collect())
    }

    /// Lista todos os probes cadastrados.
    pub async fn list_probes(&self) -> Result<Vec<Probe>> {
        let rows = self.client
            .query("SELECT id, location, ip_address::text as ip_address, provider FROM monitoring_probes", &[])
            .await?;
        Ok(rows.into_iter().map(Probe::from).collect())
    }

    /// Insere um novo ciclo de monitoramento e retorna o id gerado.
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

    /// Insere um resultado de ping.
    pub async fn insert_ping_result(&self, result: &PingResult) -> Result<()> {
        self.client
            .execute(
                "INSERT INTO connectivity_metrics
                 (cycle_id, probe_id, target_id, timestamp, metric_type, status, response_time_ms, packet_loss_percent, error_message)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
                &[
                    &result.cycle_id,
                    &result.probe_id,
                    &result.target_id,
                    &result.timestamp,
                    &result.metric_type,
                    &result.status,
                    &result.response_time_ms,
                    &result.packet_loss_percent,
                    &result.error_message,
                ],
            )
            .await?;
        Ok(())
    }

    /// Insere um evento de outage.
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
}
