use std::time::{Duration, Instant};

/// Time quantizer used for deterministic scene switching.
#[derive(Debug, Clone, Copy)]
pub struct Quantizer {
    quantum: Duration,
    origin: Instant,
}

impl Quantizer {
    pub fn new(quantum: Duration, origin: Instant) -> Self {
        Self { quantum, origin }
    }

    pub fn slot(&self) -> u64 {
        let dt = self.origin.elapsed().as_millis() as u64;
        let q = self.quantum.as_millis().max(1) as u64;
        dt / q
    }

    pub fn is_boundary(&self, last_slot: &mut u64) -> bool {
        let s = self.slot();
        if s != *last_slot {
            *last_slot = s;
            true
        } else {
            false
        }
    }
}
