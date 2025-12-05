//! consensus.rs — Estado e lógica de consenso multi-ciclo para detecção de outages
//!
//! Compatível com types.rs moderno: usa ConnectivityMetric, enums granulares e lógica auditável.
//! Mantém histórico de ciclos, detecta outages por maioria e gera eventos OutageEvent idiomáticos.

use crate::types::{ConnectivityMetric, MetricStatus, OutageEvent};
use chrono::{DateTime, Utc};
use serde_json::json;
use std::collections::{HashMap, VecDeque};

/// Estado do consenso multi-ciclo
///
/// Mantém histórico dos resultados de múltiplos ciclos de monitoramento,
/// detectando outages por maioria de targets em estado Down.
#[derive(Debug, Clone)]
pub struct ConsensusState {
    /// Histórico dos últimos N ciclos (cada ciclo: vetor de métricas de conectividade)
    pub history: VecDeque<Vec<ConnectivityMetric>>,
    /// Quantidade de ciclos consecutivos necessários para considerar falha
    fail_threshold: usize,
    /// Quantidade mínima de targets em falha para acionar consenso
    consensus: usize,
    /// Evento de outage atualmente em aberto (se houver)
    current_outage: Option<OutageEvent>,
}

impl ConsensusState {
    /// Cria um novo estado de consenso
    ///
    /// # Parâmetros
    /// - `fail_threshold`: número de ciclos consecutivos para considerar falha
    /// - `consensus`: número mínimo de targets em Down para acionar outage
    pub fn new(fail_threshold: usize, consensus: usize) -> Self {
        Self {
            history: VecDeque::with_capacity(fail_threshold),
            fail_threshold,
            consensus,
            current_outage: None,
        }
    }

    /// Atualiza o estado de consenso com os resultados de um novo ciclo
    ///
    /// # Parâmetros
    /// - `cycle_results`: vetor de métricas de conectividade do ciclo atual
    /// - `cycle_timestamp`: timestamp do ciclo
    ///
    /// # Retorno
    /// - `Some(OutageEvent)`: se um novo outage foi detectado ou encerrado
    /// - `None`: se não houve mudança de estado
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

        // Conta quantos ciclos cada target ficou Down
        let mut down_counts: HashMap<i32, usize> = HashMap::new();
        for cycle in self.history.iter() {
            for metric in cycle.iter() {
                if metric.status == MetricStatus::Down {
                    *down_counts.entry(metric.target_id).or_insert(0) += 1;
                }
            }
        }

        // Targets que ficaram Down em todos os ciclos do histórico
        let majority_down: Vec<i32> = down_counts
            .iter()
            .filter(|(_, count)| **count == self.fail_threshold)
            .map(|(&target_id, _)| target_id)
            .collect();

        // Se atingiu consenso de falha, dispara outage se ainda não houver um aberto
        if majority_down.len() >= self.consensus {
            if self.current_outage.is_none() {
                let event = OutageEvent {
                    id: 0,
                    start_time: cycle_timestamp,
                    end_time: None,
                    duration_seconds: None,
                    reason: Some("consensus_loss".to_string()),
                    affected_targets: majority_down.clone(),
                    affected_probes: None,
                    consensus_level: Some(self.consensus as i32),
                    details: Some(json!({
                        "down_targets": majority_down,
                        "cycles": self.fail_threshold,
                    })),
                };
                self.current_outage = Some(event.clone());
                return Some(event);
            }
        } else if let Some(mut event) = self.current_outage.take() {
            // Se o consenso foi perdido, encerra o outage aberto
            event.end_time = Some(cycle_timestamp);
            event.duration_seconds = event
                .end_time
                .map(|end| (end - event.start_time).num_seconds() as i32);
            return Some(event);
        }
        None
    }
}
