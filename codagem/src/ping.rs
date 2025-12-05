//! ping.rs — Execução concorrente de pings ICMP nativos com granularidade IPv4/IPv6
//!
//! Implementação idiomática, funcional e auditável usando surge-ping.
//! Compatível com types.rs moderno: usa ConnectivityMetric, MetricType granular, status robusto.

use crate::types::{ConnectivityMetric, MetricStatus, MetricType, Probe, Target};
use chrono::Utc;
use std::net::IpAddr;
use std::sync::Arc;
use surge_ping::{Client, Config, PingIdentifier, PingSequence};
use tokio::time::Duration;

/// Executa pings concorrentes a múltiplos alvos, retornando métricas detalhadas.
///
/// - Determina automaticamente se o alvo é IPv4 ou IPv6.
/// - Usa surge-ping para ICMP nativo, async, auditável.
/// - Status: Up, Degraded, Down, Timeout.
/// - Retorna vetor de `ConnectivityMetric` pronto para persistência.
///
/// # Parâmetros
/// - `targets`: fatia de alvos monitorados
/// - `probe`: probe executor
/// - `ping_count`: número de tentativas por alvo
/// - `timeout_secs`: timeout por tentativa
/// - `cycle_id`: ciclo de monitoramento
///
/// # Retorno
/// - `Vec<ConnectivityMetric>`: resultados detalhados por alvo
pub async fn ping_targets(
    targets: &[Target],
    probe: &Probe,
    ping_count: usize,
    timeout_secs: u64,
    cycle_id: i64,
) -> Vec<ConnectivityMetric> {
    let config = Config::default();
    let client = Arc::new(Client::new(&config).expect("Falha ao criar Client surge-ping"));

    let mut handles = Vec::with_capacity(targets.len());

    for (i, target) in targets.iter().cloned().enumerate() {
        let probe = probe.clone();
        let client = client.clone();
        let handle = tokio::spawn(async move {
            let mut success = 0;
            let mut total_time = 0.0;
            let mut last_error = None;
            let mut timeout_count = 0;

            let metric_type = match target.address {
                IpAddr::V4(_) => MetricType::PingIpv4,
                IpAddr::V6(_) => MetricType::PingIpv6,
            };

            // Criação síncrona do pinger
            let mut pinger = client.pinger(target.address, PingIdentifier(i as u16));
            pinger.await.timeout(Duration::from_secs(timeout_secs));
            let payload = [0u8; 32]; // Payload padrão de 32 bytes

            for seq in 0..ping_count {
                pinger = client.pinger(target.address, PingIdentifier(i as u16));
                match pinger.await.ping(PingSequence(seq as u16), &payload).await {
                    Ok((_reply, dur)) => {
                        success += 1;
                        total_time += dur.as_secs_f64() * 1000.0; // ms
                    }
                    Err(e) => {
                        if e.to_string().contains("timeout") {
                            timeout_count += 1;
                            last_error = Some("timeout".to_string());
                        } else {
                            last_error = Some(e.to_string());
                        }
                    }
                }
            }

            let status = if timeout_count == ping_count {
                MetricStatus::Timeout
            } else if success == ping_count {
                MetricStatus::Up
            } else if success > 0 {
                MetricStatus::Degraded
            } else {
                MetricStatus::Down
            };

            let avg_time = if success > 0 {
                Some(total_time / success as f64)
            } else {
                None
            };

            ConnectivityMetric {
                id: 0, // será preenchido pelo banco
                cycle_id,
                probe_id: probe.id,
                target_id: target.id,
                timestamp: Utc::now(),
                metric_type,
                status,
                response_time_ms: avg_time,
                packet_loss_percent: Some(100 - ((success * 100) / ping_count) as i16),
                error_message: last_error,
            }
        });
        handles.push(handle);
    }

    let mut results = Vec::with_capacity(handles.len());
    for h in handles {
        if let Ok(res) = h.await {
            results.push(res);
        }
    }
    results
}
