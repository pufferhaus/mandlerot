//! Multi-key chord watcher. Fires once when every watched key has been
//! pressed inside the configured window. Used for the numpad Panic combo
//! (`- + ENTER`) — three deliberate presses that no normal play pattern
//! would touch in rapid succession, so accidental triggering is unlikely.
//!
//! Order-independent on purpose: a stressed VJ mashing the bottom-right
//! corner of a numpad shouldn't have to remember a sequence. Any order
//! works as long as all three keys land within `window_ms` of each other.

use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct ChordWatcher {
    window: Duration,
    keys: Vec<String>,
    /// Parallel to `keys`. `None` if that key hasn't been seen yet (or has
    /// rolled out of the window); `Some(t)` for the last observed press.
    seen: Vec<Option<Instant>>,
}

impl ChordWatcher {
    pub fn new(keys: &[&str], window_ms: u64) -> Self {
        Self {
            window: Duration::from_millis(window_ms),
            keys: keys.iter().map(|s| s.to_string()).collect(),
            seen: vec![None; keys.len()],
        }
    }

    /// Record a key press. Returns `true` exactly once when all watched
    /// keys have a timestamp within the configured window of `now`. On a
    /// successful fire the internal state resets so a stuck-key event
    /// doesn't replay Panic on every subsequent press.
    pub fn observe(&mut self, key: &str, now: Instant) -> bool {
        if let Some(idx) = self.keys.iter().position(|k| k == key) {
            self.seen[idx] = Some(now);
        }
        let all_recent = self
            .seen
            .iter()
            .all(|s| s.map(|t| now.duration_since(t) <= self.window).unwrap_or(false));
        if all_recent {
            for s in &mut self.seen {
                *s = None;
            }
        }
        all_recent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_key_does_not_fire() {
        let mut c = ChordWatcher::new(&["A", "B", "C"], 400);
        let t = Instant::now();
        assert!(!c.observe("A", t));
    }

    #[test]
    fn unrelated_key_is_ignored() {
        let mut c = ChordWatcher::new(&["A", "B", "C"], 400);
        let t = Instant::now();
        assert!(!c.observe("Q", t));
        assert!(!c.observe("A", t));
        assert!(!c.observe("B", t));
        // "Q" never landed in the chord; needs all three real keys.
        assert!(c.observe("C", t));
    }

    #[test]
    fn all_three_inside_window_fire_once() {
        let mut c = ChordWatcher::new(&["A", "B", "C"], 400);
        let t = Instant::now();
        assert!(!c.observe("A", t));
        assert!(!c.observe("B", t + Duration::from_millis(100)));
        assert!(c.observe("C", t + Duration::from_millis(200)));
        // After firing, a 4th press of any key should NOT immediately
        // re-fire — the window resets.
        assert!(!c.observe("A", t + Duration::from_millis(250)));
    }

    #[test]
    fn order_does_not_matter() {
        let mut c = ChordWatcher::new(&["A", "B", "C"], 400);
        let t = Instant::now();
        assert!(!c.observe("C", t));
        assert!(!c.observe("A", t + Duration::from_millis(100)));
        assert!(c.observe("B", t + Duration::from_millis(200)));
    }

    #[test]
    fn stale_press_outside_window_does_not_count() {
        let mut c = ChordWatcher::new(&["A", "B", "C"], 400);
        let t = Instant::now();
        assert!(!c.observe("A", t));
        // Long delay puts A outside the window from the perspective of C.
        assert!(!c.observe("B", t + Duration::from_millis(900)));
        assert!(!c.observe("C", t + Duration::from_millis(950)));
        // Now re-press A inside the new window; this should complete the
        // chord because B and C are now both recent.
        assert!(c.observe("A", t + Duration::from_millis(1000)));
    }
}
