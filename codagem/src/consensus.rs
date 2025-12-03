use crate::types::{MetricStatus, OutageEvent, PingResult};
use chrono::{DateTime, Utc};
use serde_json::json;
use std::collections::VecDeque;

/// Estado do consenso multi-ciclo
#[derive(Debug, Clone)]
pub struct ConsensusState {
    history: VecDeque<Vec<PingResult>>,
    fail_threshold: usize,
    consensus: usize,
    current_outage: Option<OutageEvent>,
}

impl ConsensusState {
    pub fn new(fail_threshold: usize, consensus: usize) -> Self {
        Self {
            history: VecDeque::with_capacity(fail_threshold),
            fail_threshold,
            consensus,
            current_outage: None,
        }
    }

    pub fn update(
        &mut self,
        cycle_results: Vec<PingResult>,
        cycle_timestamp: DateTime<Utc>,
    ) -> Option<OutageEvent> {
        if self.history.len() == self.fail_threshold {
            self.history.pop_front();
        }
        self.history.push_back(cycle_results.clone());

        let mut down_counts: std::collections::HashMap<i32, usize> =
            std::collections::HashMap::new();
        for cycle in self.history.iter() {
            for res in cycle.iter() {
                if res.status == MetricStatus::Down {
                    *down_counts.entry(res.target_id).or_insert(0) += 1;
                }
            }
        }
        let majority_down: Vec<i32> = down_counts
            .iter()
            .filter(|(_, c)| **c == self.fail_threshold)
            .map(|(&id, _)| id)
            .collect();

        if majority_down.len() >= self.consensus {
            if self.current_outage.is_none() {
                let event: OutageEvent = OutageEvent {
                    id: 0,
                    start_time: cycle_timestamp,
                    end_time: None,
                    duration_seconds: None,
                    reason: "consensus_loss".to_string(),
                    affected_targets: majority_down.clone(),
                    affected_probes: None,
                    consensus_level: self.consensus as i32,
                    details: json!({
                        "down_targets": majority_down,
                        "cycles": self.fail_threshold,
                    }),
                };
                self.current_outage = Some(event.clone());
                return Some(event);
            }
        } else if let Some(mut event) = self.current_outage.take() {
            event.end_time = Some(cycle_timestamp);
            event.duration_seconds = event
                .end_time
                .map(|end: DateTime<Utc>| (end - event.start_time).num_seconds() as i32);
            return Some(event);
        }
        None
    }
}
