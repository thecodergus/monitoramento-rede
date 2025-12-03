use crate::types::{MetricStatus, MetricType, PingResult, Probe, Target};
use chrono::Utc;
use tokio::process::Command;
use tokio::time::{Duration, timeout};

/// Executa pings concorrentes a múltiplos alvos, retorna resultados
pub async fn ping_targets(
    targets: &[Target],
    probe: &Probe,
    ping_count: usize,
    timeout_secs: u64,
    cycle_id: i64,
) -> Vec<PingResult> {
    let mut handles = Vec::new();
    for target in targets.iter().cloned() {
        let probe = probe.clone();
        let handle = tokio::spawn(async move {
            let mut success = 0;
            let mut total_time = 0.0;
            let mut last_error = None;
            for _ in 0..ping_count {
                let start = Utc::now();
                let res = timeout(
                    Duration::from_secs(timeout_secs),
                    Command::new("ping")
                        .arg("-c")
                        .arg("1")
                        .arg(&target.address)
                        .output(),
                )
                .await;

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
                        last_error = Some(e.to_string());
                    }
                }
            }
            let status = if success == ping_count {
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
            PingResult {
                cycle_id,
                probe_id: probe.id,
                target_id: target.id,
                timestamp: Utc::now(),
                metric_type: MetricType::Ping,
                status,
                response_time_ms: avg_time,
                packet_loss_percent: Some(100 - ((success * 100) / ping_count) as i16),
                error_message: last_error,
            }
        });
        handles.push(handle);
    }
    let mut results = Vec::new();
    for h in handles {
        if let Ok(res) = h.await {
            results.push(res);
        }
    }
    results
}
