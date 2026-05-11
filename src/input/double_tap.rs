//! Double-tap detector. Used to give the user an "always-panic" escape
//! hatch even when a menu screen has captured the Esc key.

use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct DoubleTap {
    window: Duration,
    last: Option<Instant>,
}

impl DoubleTap {
    pub fn new(window_ms: u64) -> Self {
        Self {
            window: Duration::from_millis(window_ms),
            last: None,
        }
    }

    /// Register a tap at `now`. Returns `true` if this tap arrived within
    /// the configured window of a previous tap (i.e. a double-tap).
    ///
    /// On a successful double-tap the internal state resets so a third tap
    /// in quick succession does NOT also register as a double — you must
    /// see two fresh taps.
    pub fn tap(&mut self, now: Instant) -> bool {
        let hit = self
            .last
            .map(|t| now.duration_since(t) <= self.window)
            .unwrap_or(false);
        self.last = if hit { None } else { Some(now) };
        hit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_tap_does_not_fire() {
        let mut d = DoubleTap::new(400);
        let t0 = Instant::now();
        assert!(!d.tap(t0));
    }

    #[test]
    fn two_taps_inside_window_fire() {
        let mut d = DoubleTap::new(400);
        let t0 = Instant::now();
        assert!(!d.tap(t0));
        assert!(d.tap(t0 + Duration::from_millis(200)));
    }

    #[test]
    fn two_taps_outside_window_dont_fire() {
        let mut d = DoubleTap::new(400);
        let t0 = Instant::now();
        assert!(!d.tap(t0));
        assert!(!d.tap(t0 + Duration::from_millis(600)));
    }

    #[test]
    fn third_tap_needs_fresh_pair() {
        let mut d = DoubleTap::new(400);
        let t0 = Instant::now();
        assert!(!d.tap(t0));
        assert!(d.tap(t0 + Duration::from_millis(100)));
        // After firing, state resets — next tap is a fresh "first".
        assert!(!d.tap(t0 + Duration::from_millis(200)));
        assert!(d.tap(t0 + Duration::from_millis(300)));
    }
}
