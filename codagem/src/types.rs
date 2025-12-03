use std::collections::HashMap;

use chrono::{DateTime, Utc};
use postgres_types::{FromSql, ToSql};
use serde::{Deserialize, Serialize};
use tokio_postgres::Row;

/// Enum para status da métrica (PostgreSQL)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSql, FromSql)]
#[postgres(name = "metric_status", rename_all = "lowercase")]
pub enum MetricStatus {
    Up,
    Down,
    Degraded,
    Timeout,
}

/// Enum para tipo de métrica (PostgreSQL)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSql, FromSql)]
#[postgres(name = "metric_type", rename_all = "lowercase")]
pub enum MetricType {
    Ping,
    Http,
    Dns,
    Tcp,
}

/// Struct de alvo monitorado (normalizado)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    pub id: i32,
    pub name: String,
    pub address: String,
    pub asn: Option<i32>,
    pub provider: Option<String>,
    pub type_: String,
    pub region: Option<String>,
}

impl From<Row> for Target {
    fn from(row: Row) -> Self {
        Self {
            id: row.get("id"),
            name: row.get("name"),
            address: row.get("address"),
            asn: row.get("asn"),
            provider: row.get("provider"),
            type_: row.get("type"),
            region: row.get("region"),
        }
    }
}

/// Struct de probe (multi-localização)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Probe {
    pub id: i32,
    pub location: String,
    pub ip_address: Option<String>,
    pub provider: Option<String>,
}

impl From<Row> for Probe {
    fn from(row: Row) -> Self {
        Self {
            id: row.get("id"),
            location: row.get("location"),
            ip_address: row.get("ip_address"),
            provider: row.get("provider"),
        }
    }
}

/// Struct de ciclo de monitoramento
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cycle {
    pub id: i64,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub cycle_number: i32,
    pub probe_count: i32,
}

/// Struct de resultado de ping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingResult {
    pub cycle_id: i64,
    pub probe_id: i32,
    pub target_id: i32,
    pub timestamp: DateTime<Utc>,
    pub metric_type: MetricType,
    pub status: MetricStatus,
    pub response_time_ms: Option<f64>,
    pub packet_loss_percent: Option<i16>,
    pub error_message: Option<String>,
}

/// Struct de evento de outage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutageEvent {
    pub id: i64,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub duration_seconds: Option<i32>,
    pub reason: String,
    pub affected_targets: Vec<i32>,
    pub affected_probes: Option<Vec<i32>>,
    pub consensus_level: i32,
    pub details: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct TargetWarmupState {
    pub success_streak: HashMap<i32, usize>, // target_id -> contagem de ciclos de sucesso
    pub required_streak: usize,              // N ciclos necessários
}

impl TargetWarmupState {
    pub fn new(required_streak: usize) -> Self {
        Self {
            success_streak: HashMap::new(),
            required_streak,
        }
    }

    /// Atualiza o streak de sucesso para um target.
    /// Retorna true se o target já está "aquecido" (pronto para registrar outages).
    pub fn update(&mut self, target_id: i32, is_success: bool) -> bool {
        if is_success {
            let streak = self.success_streak.entry(target_id).or_insert(0);
            *streak += 1;
        } else {
            self.success_streak.insert(target_id, 0);
        }
        self.success_streak[&target_id] >= self.required_streak
    }
}
