//! Per-band attack/release envelope follower with rolling 95th-percentile
//! auto-gain.

use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct EnvelopeFollower {
    /// Current smoothed value
    pub value: f32,
    pub attack_a: f32,
    pub release_a: f32,
}

impl EnvelopeFollower {
    /// Construct with attack/release time constants in seconds at the
    /// given update rate.
    pub fn new(attack_secs: f32, release_secs: f32, rate_hz: f32) -> Self {
        let attack_a = (-1.0 / (attack_secs * rate_hz)).exp();
        let release_a = (-1.0 / (release_secs * rate_hz)).exp();
        Self {
            value: 0.0,
            attack_a,
            release_a,
        }
    }

    pub fn update(&mut self, x: f32) {
        let a = if x > self.value {
            self.attack_a
        } else {
            self.release_a
        };
        self.value = a * self.value + (1.0 - a) * x;
    }
}

/// Tracks 95th-percentile over a rolling window for normalization.
///
/// `min_reference` is the absolute minimum value used in the denominator.
/// Without it, a steady-state quiet signal (room hum, fan noise) sets P95
/// to the hum level itself, and `x / P95` outputs ~1.0 even for ambient.
/// With it, the denominator never falls below the noise floor, so quiet
/// rooms produce small output, and only signals louder than the floor
/// (i.e. actual music) auto-normalize.
#[derive(Debug, Clone)]
pub struct AutoGain {
    history: VecDeque<f32>,
    capacity: usize,
    min_reference: f32,
}

impl AutoGain {
    pub fn new(window_secs: f32, rate_hz: f32, min_reference: f32) -> Self {
        let capacity = (window_secs * rate_hz).ceil() as usize;
        Self {
            history: VecDeque::with_capacity(capacity),
            capacity: capacity.max(1),
            min_reference: min_reference.max(1e-6),
        }
    }

    /// Override the noise-floor minimum reference. Called from the audio
    /// thread each tick so the UI can tune it live.
    pub fn set_min_reference(&mut self, v: f32) {
        self.min_reference = v.max(1e-6);
    }

    pub fn observe(&mut self, x: f32) {
        if self.history.len() >= self.capacity {
            self.history.pop_front();
        }
        self.history.push_back(x);
    }

    /// Map raw input to normalized [0, 1] using max(P95, min_reference) as
    /// the "loud" reference. Falls back to identity (clamped) until the
    /// window has at least `min_samples` observations.
    pub fn normalize(&self, x: f32, min_samples: usize) -> f32 {
        if self.history.len() < min_samples {
            return x.clamp(0.0, 1.0);
        }
        let mut sorted: Vec<f32> = self.history.iter().copied().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let p95_idx = (sorted.len() as f32 * 0.95) as usize;
        let p95 = sorted[p95_idx.min(sorted.len() - 1)];
        let denom = p95.max(self.min_reference);
        (x / denom).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_rises_fast_falls_slow() {
        let mut e = EnvelopeFollower::new(0.005, 0.2, 100.0);
        // Pulse at t=0
        e.update(1.0);
        let after_attack = e.value;
        // Release: feed zero for many steps
        for _ in 0..5 {
            e.update(0.0);
        }
        let after_release = e.value;
        // Should still be substantially above zero after a short release period.
        assert!(after_release > 0.5 * after_attack);
        // Attack should produce a value > 0.5 in one update with these constants.
        assert!(after_attack > 0.5);
    }

    #[test]
    fn auto_gain_normalizes_to_p95() {
        // min_reference well below the data so P95 controls the denominator
        let mut g = AutoGain::new(1.0, 100.0, 0.01);
        for v in 0..100 {
            g.observe(v as f32 / 100.0); // 0.0 to 0.99
        }
        // p95 ≈ 0.95. Values at the top should normalize near 1.0.
        let n = g.normalize(0.95, 50);
        assert!((0.9..=1.0).contains(&n));
    }

    #[test]
    fn auto_gain_returns_clamp_when_window_too_short() {
        let g = AutoGain::new(1.0, 100.0, 5.0);
        let n = g.normalize(0.7, 50);
        assert_eq!(n, 0.7);
    }

    #[test]
    fn auto_gain_handles_silent_window() {
        let mut g = AutoGain::new(1.0, 100.0, 5.0);
        for _ in 0..100 {
            g.observe(0.0);
        }
        let n = g.normalize(0.5, 50);
        // P95 = 0, but denom = max(0, 5.0) = 5.0; 0.5/5.0 = 0.1
        assert!((n - 0.1).abs() < 1e-4);
    }

    #[test]
    fn auto_gain_floors_steady_state_quiet_signals() {
        // Simulates room hum: steady 2.0, well below min_reference of 8.0.
        // Without min_reference, P95 = 2.0 and x/P95 = 1.0 (the bug).
        // With min_reference, denom = max(2.0, 8.0) = 8.0; 2.0/8.0 = 0.25.
        let mut g = AutoGain::new(1.0, 100.0, 8.0);
        for _ in 0..100 {
            g.observe(2.0);
        }
        let n = g.normalize(2.0, 50);
        assert!(n < 0.3, "steady hum should not saturate (got {n})");
    }

    #[test]
    fn auto_gain_passes_loud_dynamic_signals() {
        // Music-like: P95 ends up at 50, above min_reference of 8.
        // x = 50 should normalize to ~1.0.
        let mut g = AutoGain::new(1.0, 100.0, 8.0);
        for v in 0..100 {
            g.observe(v as f32 * 0.5); // 0..49.5
        }
        let n = g.normalize(50.0, 50);
        assert!(n > 0.9, "loud signal should saturate (got {n})");
    }
}
