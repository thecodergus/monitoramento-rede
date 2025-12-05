//! types.rs — Modelos de dados para monitoramento de rede
//!
//! Representação fiel e idiomática do schema PostgreSQL atualizado.
//! Inclui enums para metric_type e metric_status, além de structs para targets, probes, ciclos e métricas.
//!
//! # Exemplo de uso
//! ```rust
//! use crate::types::{MetricType, MetricStatus};
//! let t: MetricType = "ping_ipv4".parse().unwrap();
//! assert_eq!(t.to_string(), "ping_ipv4");
//! ```

use chrono::{DateTime, Utc};
use postgres_types::{FromSql, ToSql};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;
use tokio_postgres::Row;

/// Estado do scheduler (não persistido)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedulerState {
    WaitingForInternet,
    Monitoring,
}

/// Enum para status da métrica (PostgreSQL)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSql, FromSql)]
#[postgres(name = "metric_status", rename_all = "lowercase")]
pub enum MetricStatus {
    Up,
    Down,
    Degraded,
    Timeout,
}

/// Enum para tipo de métrica (PostgreSQL), granular por protocolo e pilha
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSql, FromSql)]
#[postgres(name = "metric_type", rename_all = "snake_case")]
pub enum MetricType {
    PingIpv4,
    PingIpv6,
    TcpIpv4,
    TcpIpv6,
    HttpIpv4,
    HttpIpv6,
    DnsIpv4,
    DnsIpv6,
}

/// Struct de alvo monitorado (monitoring_targets)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    pub id: i32,
    pub name: String,
    pub address: IpAddr,
    pub asn: Option<i32>,
    pub provider: Option<String>,
    pub type_: String, // Pode ser refinado para MetricType se o banco garantir ENUM
    pub region: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
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
            created_at: row.try_get("created_at").ok(),
        }
    }
}

/// Struct de probe (monitoring_probes)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Probe {
    pub id: i32,
    pub location: String,
    pub ip_address: Option<IpAddr>,
    pub provider: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}

impl From<Row> for Probe {
    fn from(row: Row) -> Self {
        Self {
            id: row.get("id"),
            location: row.get("location"),
            ip_address: row.get("ip_address"),
            provider: row.get("provider"),
            created_at: row.try_get("created_at").ok(),
        }
    }
}

/// Struct de ciclo de monitoramento (monitoring_cycles)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cycle {
    pub id: i64,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub cycle_number: i32,
    pub probe_count: i32,
}

impl From<Row> for Cycle {
    fn from(row: Row) -> Self {
        Self {
            id: row.get("id"),
            started_at: row.get("started_at"),
            ended_at: row.get("ended_at"),
            cycle_number: row.get("cycle_number"),
            probe_count: row.get("probe_count"),
        }
    }
}

/// Struct de métrica de conectividade (connectivity_metrics)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectivityMetric {
    pub id: i64,
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

impl From<Row> for ConnectivityMetric {
    fn from(row: Row) -> Self {
        Self {
            id: row.get("id"),
            cycle_id: row.get("cycle_id"),
            probe_id: row.get("probe_id"),
            target_id: row.get("target_id"),
            timestamp: row.get("timestamp"),
            metric_type: row.get("metric_type"),
            status: row.get("status"),
            response_time_ms: row.get("response_time_ms"),
            packet_loss_percent: row.get("packet_loss_percent"),
            error_message: row.get("error_message"),
        }
    }
}

/// Struct de evento de outage (outage_events)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutageEvent {
    pub id: i64,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub duration_seconds: Option<i32>,
    pub reason: Option<String>,
    pub affected_targets: Vec<i32>,
    pub affected_probes: Option<Vec<i32>>,
    pub consensus_level: Option<i32>,
    pub details: Option<serde_json::Value>,
}

impl From<Row> for OutageEvent {
    fn from(row: Row) -> Self {
        Self {
            id: row.get("id"),
            start_time: row.get("start_time"),
            end_time: row.get("end_time"),
            duration_seconds: row.get("duration_seconds"),
            reason: row.get("reason"),
            affected_targets: row.get("affected_targets"),
            affected_probes: row.get("affected_probes"),
            consensus_level: row.get("consensus_level"),
            details: row.get("details"),
        }
    }
}

/// Struct para status do alvo (target_status)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetStatus {
    pub target_id: i32,
    pub last_status: MetricStatus,
    pub last_change: DateTime<Utc>,
}

impl From<Row> for TargetStatus {
    fn from(row: Row) -> Self {
        Self {
            target_id: row.get("target_id"),
            last_status: row.get("last_status"),
            last_change: row.get("last_change"),
        }
    }
}

/// Estado de aquecimento de target (lógica de streaks)
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
