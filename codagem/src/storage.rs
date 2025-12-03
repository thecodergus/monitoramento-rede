use crate::types::{Cycle, MetricStatus, MetricType, OutageEvent, PingResult, Probe, Target};
use anyhow::Result;
use sqlx::{PgPool, postgres::PgPoolOptions};

pub struct Storage {
    pool: PgPool,
}

impl Storage {
    pub async fn connect(database_url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await?;
        Ok(Self { pool })
    }

    pub async fn list_targets(&self) -> Result<Vec<Target>> {
        let rows = sqlx::query_as!(
            Target,
            r#"SELECT id, name, address, asn, provider, type as "type_", region FROM monitoring_targets"#
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn list_probes(&self) -> Result<Vec<Probe>> {
        let rows = sqlx::query_as!(
            Probe,
            r#"SELECT id, location, ip_address::text as ip_address, provider FROM monitoring_probes"#
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn insert_cycle(&self, cycle: &Cycle) -> Result<i64> {
        let rec = sqlx::query!(
            r#"
            INSERT INTO monitoring_cycles (started_at, ended_at, cycle_number, probe_count)
            VALUES ($1, $2, $3, $4)
            RETURNING id
            "#,
            cycle.started_at,
            cycle.ended_at,
            cycle.cycle_number,
            cycle.probe_count,
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(rec.id)
    }

    pub async fn insert_ping_result(&self, result: &PingResult) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO connectivity_metrics
            (cycle_id, probe_id, target_id, timestamp, metric_type, status, response_time_ms, packet_loss_percent, error_message)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
            result.cycle_id,
            result.probe_id,
            result.target_id,
            result.timestamp,
            result.metric_type.clone() as MetricType,
            result.status.clone() as MetricStatus,
            result.response_time_ms,
            result.packet_loss_percent,
            result.error_message,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn insert_outage_event(&self, event: &OutageEvent) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO outage_events
            (start_time, end_time, duration_seconds, reason, affected_targets, affected_probes, consensus_level, details)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
            event.start_time,
            event.end_time,
            event.duration_seconds,
            &event.reason,
            &event.affected_targets,
            event.affected_probes.as_deref(),
            event.consensus_level,
            &event.details,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
