//! Tap-tempo BPM derivation. Median of intervals between the last 8 taps,
//! with 30-second timeout that resets the buffer.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

const HISTORY_LEN: usize = 8;
const TAP_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct TapTempo {
    taps: VecDeque<Instant>,
}

impl TapTempo {
    pub fn new() -> Self {
        Self {
            taps: VecDeque::with_capacity(HISTORY_LEN),
        }
    }

    /// Register a tap. Returns the new BPM (or 0.0 if not yet derivable).
    pub fn tap(&mut self, now: Instant) -> f32 {
        // Reset if too long since last tap.
        if let Some(last) = self.taps.back() {
            if now.duration_since(*last) > TAP_TIMEOUT {
                self.taps.clear();
            }
        }
        if self.taps.len() >= HISTORY_LEN {
            self.taps.pop_front();
        }
        self.taps.push_back(now);

        if self.taps.len() < 2 {
            return 0.0;
        }
        let mut intervals: Vec<f32> = self
            .taps
            .iter()
            .zip(self.taps.iter().skip(1))
            .map(|(a, b)| b.duration_since(*a).as_secs_f32())
            .collect();
        intervals.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = intervals[intervals.len() / 2];
        if median <= 0.001 {
            return 0.0;
        }
        60.0 / median
    }
}

impl Default for TapTempo {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_tap_yields_zero_bpm() {
        let mut t = TapTempo::new();
        let bpm = t.tap(Instant::now());
        assert_eq!(bpm, 0.0);
    }

    #[test]
    fn even_taps_at_120bpm() {
        let mut t = TapTempo::new();
        let start = Instant::now();
        // 120 BPM = 0.5s interval
        for i in 0..5 {
            t.tap(start + Duration::from_millis(500 * i));
        }
        let bpm = t.tap(start + Duration::from_millis(2500));
        assert!((bpm - 120.0).abs() < 1.0);
    }

    #[test]
    fn timeout_clears_history() {
        let mut t = TapTempo::new();
        let start = Instant::now();
        t.tap(start);
        t.tap(start + Duration::from_millis(500));
        assert_eq!(t.taps.len(), 2);
        // Long pause
        t.tap(start + Duration::from_secs(60));
        assert_eq!(t.taps.len(), 1);
    }
}
