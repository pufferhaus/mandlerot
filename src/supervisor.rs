//! Tracks per-scene fault counts so a repeatedly-broken scene gets
//! automatically substituted with `__safe__` until next manual select or
//! restart.

use std::collections::HashMap;
use std::time::{Duration, Instant};

const FAULT_WINDOW: Duration = Duration::from_secs(60);
const MAX_FAULTS: u32 = 3;

#[derive(Debug, Default)]
pub struct Supervisor {
    faults: HashMap<String, Vec<Instant>>,
    disabled: HashMap<String, Instant>,
}

impl Supervisor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a fault for `scene`. Returns `true` if the scene has now
    /// been disabled (fault count exceeded threshold).
    pub fn record_fault(&mut self, scene: &str) -> bool {
        let now = Instant::now();
        let entry = self.faults.entry(scene.to_string()).or_default();
        entry.retain(|t| now.duration_since(*t) <= FAULT_WINDOW);
        entry.push(now);
        if entry.len() as u32 >= MAX_FAULTS {
            self.disabled.insert(scene.to_string(), now);
            true
        } else {
            false
        }
    }

    /// Returns true if the scene is currently disabled.
    pub fn is_disabled(&self, scene: &str) -> bool {
        self.disabled.contains_key(scene)
    }

    /// Manually re-enable a scene (e.g. when the user explicitly selects it
    /// again — implies they accepted the risk).
    pub fn enable(&mut self, scene: &str) {
        self.disabled.remove(scene);
        self.faults.remove(scene);
    }

    /// Substitute a scene name with `__safe__` if it's been disabled.
    pub fn resolve<'a>(&self, scene: &'a str) -> &'a str {
        if self.is_disabled(scene) {
            "__safe__"
        } else {
            scene
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_faults_disables() {
        let mut s = Supervisor::new();
        assert!(!s.record_fault("foo"));
        assert!(!s.record_fault("foo"));
        assert!(s.record_fault("foo"));
        assert!(s.is_disabled("foo"));
    }

    #[test]
    fn enable_clears_state() {
        let mut s = Supervisor::new();
        for _ in 0..3 {
            s.record_fault("foo");
        }
        assert!(s.is_disabled("foo"));
        s.enable("foo");
        assert!(!s.is_disabled("foo"));
    }

    #[test]
    fn resolve_substitutes_safe() {
        let mut s = Supervisor::new();
        for _ in 0..3 {
            s.record_fault("foo");
        }
        assert_eq!(s.resolve("foo"), "__safe__");
        assert_eq!(s.resolve("bar"), "bar");
    }

    #[test]
    fn faults_for_different_scenes_dont_trigger_each_other() {
        let mut s = Supervisor::new();
        s.record_fault("foo");
        s.record_fault("foo");
        s.record_fault("bar");
        assert!(!s.is_disabled("foo"));
        assert!(!s.is_disabled("bar"));
    }
}
