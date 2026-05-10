//! Spectral flux onset detector with adaptive median threshold.

use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct BeatDetector {
    prev_mags: Vec<f32>,
    flux_history: VecDeque<f32>,
    threshold_window: usize,
    threshold_mult: f32,
    /// Decaying value visible to scenes via `u_beat`.
    pub trigger: f32,
    /// Time-weighted decay rate applied per `update` call.
    pub decay_per_update: f32,
}

impl BeatDetector {
    pub fn new(rate_hz: f32) -> Self {
        Self {
            prev_mags: Vec::new(),
            flux_history: VecDeque::with_capacity(43),
            threshold_window: 43, // ~430 ms at 100 Hz
            threshold_mult: 1.5,
            trigger: 0.0,
            decay_per_update: (-1.0 / (0.15 * rate_hz)).exp(),
        }
    }

    /// Update with the latest FFT magnitudes. Returns true on beat.
    pub fn update(&mut self, mags: &[f32]) -> bool {
        let mut beat = false;
        if !self.prev_mags.is_empty() && self.prev_mags.len() == mags.len() {
            let flux: f32 = mags
                .iter()
                .zip(self.prev_mags.iter())
                .map(|(c, p)| (c - p).max(0.0))
                .sum();
            if self.flux_history.len() >= self.threshold_window {
                let mut sorted: Vec<f32> = self.flux_history.iter().copied().collect();
                sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
                let median = sorted[sorted.len() / 2];
                if flux > median * self.threshold_mult && flux > 1e-3 {
                    beat = true;
                }
            }
            if self.flux_history.len() >= self.threshold_window {
                self.flux_history.pop_front();
            }
            self.flux_history.push_back(flux);
        }
        self.prev_mags = mags.to_vec();
        self.trigger = self.trigger * self.decay_per_update;
        if beat {
            self.trigger = 1.0;
        }
        beat
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_input_produces_no_beat() {
        let mut d = BeatDetector::new(100.0);
        let mags = vec![1.0; 32];
        let mut beats = 0;
        for _ in 0..200 {
            if d.update(&mags) {
                beats += 1;
            }
        }
        assert_eq!(beats, 0);
    }

    #[test]
    fn sudden_spike_after_quiet_triggers_beat() {
        let mut d = BeatDetector::new(100.0);
        let quiet = vec![0.01; 32];
        let loud = vec![5.0; 32];
        // Build threshold from quiet baseline
        for _ in 0..100 {
            d.update(&quiet);
        }
        let beat = d.update(&loud);
        assert!(beat);
    }

    #[test]
    fn trigger_decays_over_time() {
        let mut d = BeatDetector::new(100.0);
        d.trigger = 1.0;
        let mags = vec![0.0; 32];
        for _ in 0..50 {
            d.update(&mags);
        }
        assert!(d.trigger < 0.1);
    }
}
