//! ping.rs — Execução concorrente de pings com granularidade IPv4/IPv6
//!
//! Compatível com types.rs moderno: usa ConnectivityMetric, MetricType granular, status robusto.
//! Tipagem estática rigorosa, concorrência segura, pattern matching idiomático.

use crate::types::{ConnectivityMetric, MetricStatus, MetricType, Probe, Target};
use chrono::Utc;
use std::net::IpAddr;
use tokio::process::Command;
use tokio::time::{Duration, timeout};

/// Executa pings concorrentes a múltiplos alvos, retornando métricas detalhadas.
///
/// - Determina automaticamente se o alvo é IPv4 ou IPv6.
/// - Usa `ping -4` ou `ping -6` conforme o tipo de IP.
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
    let mut handles = Vec::with_capacity(targets.len());

    for target in targets.iter().cloned() {
        let probe = probe.clone();
        let handle = tokio::spawn(async move {
            let mut success = 0;
            let mut total_time = 0.0;
            let mut last_error = None;
            let mut timeout_count = 0;

            // Determina o tipo de métrica granular (IPv4/IPv6)
            let metric_type = match target.address {
                IpAddr::V4(_) => MetricType::PingIpv4,
                IpAddr::V6(_) => MetricType::PingIpv6,
            };

            for _ in 0..ping_count {
                let start = Utc::now();
                // Seleciona comando e argumentos conforme o tipo de IP
                let mut cmd = Command::new("ping");
                if target.address.is_ipv6() {
                    cmd.arg("-6");
                } else {
                    cmd.arg("-4");
                }
                cmd.arg("-c").arg("1").arg(target.address.to_string());

                let res = timeout(Duration::from_secs(timeout_secs), cmd.output()).await;

                match res {
                    Ok(Ok(output)) if output.status.success() => {
                        success += 1;
                        let elapsed = (Utc::now() - start).num_milliseconds() as f64;
                        total_time += elapsed;
                    }
                    Ok(Ok(output)) => {
                        last_error = Some(String::from_utf8_lossy(&output.stderr).to_string());
                    }
                    Ok(Err(e)) => {
                        last_error = Some(e.to_string());
                    }
                    Err(e) => {
                        // Timeout explícito
                        last_error = Some(e.to_string());
                        timeout_count += 1;
                    }
                }
            }

            // Determina status conforme sucesso, falha parcial ou timeout total
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
