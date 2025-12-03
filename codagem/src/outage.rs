use crate::types::OutageEvent;
use chrono::Utc;

/// Gerencia início/fim de outages, calcula duração e monta detalhes.
#[derive(Debug, Clone)]
pub struct OutageManager {
    current: Option<OutageEvent>,
}

impl OutageManager {
    pub fn new() -> Self {
        Self { current: None }
    }

    pub fn handle_cycle(
        &mut self,
        maybe_outage: Option<OutageEvent>,
        timestamp: chrono::DateTime<Utc>,
    ) -> Option<OutageEvent> {
        match (&self.current, maybe_outage) {
            (None, Some(event)) => {
                self.current = Some(event.clone());
                Some(event)
            }
            (Some(current), None) => {
                let mut finished = current.clone();
                finished.end_time = Some(timestamp);
                finished.duration_seconds = finished
                    .end_time
                    .map(|end| (end - finished.start_time).num_seconds() as i32);
                self.current = None;
                Some(finished)
            }
            _ => None,
        }
    }
}
