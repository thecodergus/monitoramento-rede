//! consensus.rs — Estado de consenso multi-ciclo robusto para detecção de outages

use crate::types::{ConnectivityMetric, MetricStatus, OutageEvent};
use chrono::{DateTime, Utc};
use serde_json::json;
use std::collections::{HashMap, VecDeque};

/// Estado do consenso multi-ciclo
#[derive(Debug, Clone)]
pub struct ConsensusState {
    pub history: VecDeque<Vec<ConnectivityMetric>>,
    fail_threshold: usize,
    consensus: usize,
    current_outage: Option<OutageEvent>,
    /// ID da probe local (necessário para affected_probes)
    probe_id: Option<i32>,
}

impl ConsensusState {
    /// Cria um novo estado de consenso
    pub fn new(fail_threshold: usize, consensus: usize, probe_id: Option<i32>) -> Self {
        Self {
            history: VecDeque::with_capacity(fail_threshold),
            fail_threshold,
            consensus,
            current_outage: None,
            probe_id,
        }
    }

    /// Valida os parâmetros de consenso em relação ao número de targets monitorados.
    pub fn validate_params(&self, num_targets: usize) -> Result<(), String> {
        if self.fail_threshold == 0 {
            return Err("fail_threshold deve ser maior que zero".into());
        }
        if self.consensus == 0 {
            return Err("consensus deve ser maior que zero".into());
        }
        if self.consensus > num_targets {
            return Err(format!(
                "consensus ({}) não pode ser maior que o número de targets monitorados ({})",
                self.consensus, num_targets
            ));
        }
        Ok(())
    }

    /// Atualiza o estado de consenso com os resultados de um novo ciclo
    pub fn update(
        &mut self,
        cycle_results: Vec<ConnectivityMetric>,
        cycle_timestamp: DateTime<Utc>,
    ) -> Option<OutageEvent> {
        // Mantém o histórico limitado ao fail_threshold
        if self.history.len() == self.fail_threshold {
            self.history.pop_front();
        }
        self.history.push_back(cycle_results.clone());

        // Conta quantos ciclos cada target ficou Down ou Timeout
        let mut down_counts: HashMap<i32, usize> = HashMap::new();
        for cycle in self.history.iter() {
            for metric in cycle.iter() {
                if metric.status == MetricStatus::Down || metric.status == MetricStatus::Timeout {
                    *down_counts.entry(metric.target_id).or_insert(0) += 1;
                }
            }
        }

        // Targets que ficaram Down/Timeout em todos os ciclos do histórico
        let majority_down: Vec<i32> = down_counts
            .iter()
            .filter(|(_, count)| **count == self.fail_threshold)
            .map(|(&target_id, _)| target_id)
            .collect();

        // Logging detalhado para auditoria
        println!(
            "[CONSENSUS DEBUG] Histórico: {} ciclos, Down/Timeout por target: {:?}, majority_down: {:?}, consensus: {}, fail_threshold: {}",
            self.history.len(),
            down_counts,
            majority_down,
            self.consensus,
            self.fail_threshold
        );

        // Se atingiu consenso de falha, dispara outage se ainda não houver um aberto
        if majority_down.len() >= self.consensus {
            if self.current_outage.is_none() {
                let event = OutageEvent {
                    id: 0,
                    start_time: cycle_timestamp,
                    end_time: None,
                    duration_seconds: None,
                    reason: Some("consensus_reached".to_string()),
                    affected_targets: majority_down.clone(),
                    affected_probes: match self.probe_id {
                        Some(n) => Some(vec![n]),
                        None => None,
                    }, // Adapte para multi-probe se necessário
                    consensus_level: Some(majority_down.len() as i32),
                    details: Some(json!({
                        "fail_threshold": self.fail_threshold,
                        "consensus": self.consensus,
                        "history_len": self.history.len(),
                        "down_counts": down_counts,
                    })),
                };
                self.current_outage = Some(event.clone());
                println!(
                    "[CONSENSUS INFO] Outage detectado! Atingido consenso de {} targets Down/Timeout.",
                    self.consensus
                );
                return Some(event);
            }
        } else {
            // Se consenso foi perdido, encerra outage aberto
            if let Some(mut event) = self.current_outage.take() {
                event.end_time = Some(cycle_timestamp);
                event.duration_seconds = event
                    .end_time
                    .map(|end| (end - event.start_time).num_seconds() as i32);
                println!(
                    "[CONSENSUS INFO] Outage encerrado. Duração: {:?} segundos.",
                    event.duration_seconds
                );
                return Some(event);
            }
        }
        None
    }
}
