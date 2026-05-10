//! Scripted input source for headless replay tests.
//!
//! Format (one event per line):
//!
//! ```text
//! 0.000  press 1
//! 0.500  press [ hold 2.0
//! 3.000  press Tab
//! 3.100  press 5
//! 3.200  press = repeat 10
//! ```
//!
//! Comments start with `#`. Blank lines ignored. Times are absolute seconds
//! from script start.

use std::time::Duration;

use crate::error::{Error, Result};

use super::keymap::{Modifier, RawKey};

#[derive(Debug, Clone, PartialEq)]
pub struct ScriptedEvent {
    pub at: Duration,
    pub key: RawKey,
    pub modifier: Modifier,
    pub repeat: u32,
}

#[derive(Debug, Default)]
pub struct MockInput {
    events: Vec<ScriptedEvent>,
    cursor: usize,
}

impl MockInput {
    pub fn from_script(s: &str) -> Result<Self> {
        let mut events = Vec::new();
        for (lineno, line) in s.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let evt = parse_line(line, lineno + 1)?;
            events.push(evt);
        }
        Ok(Self { events, cursor: 0 })
    }

    /// Drain events whose `at` ≤ `now`. Each `repeat` count expands into
    /// multiple identical RawKey emissions.
    pub fn drain_until(&mut self, now: Duration) -> Vec<(RawKey, Modifier)> {
        let mut out = Vec::new();
        while let Some(evt) = self.events.get(self.cursor) {
            if evt.at > now {
                break;
            }
            for _ in 0..evt.repeat.max(1) {
                out.push((evt.key.clone(), evt.modifier));
            }
            self.cursor += 1;
        }
        out
    }

    pub fn finished(&self) -> bool {
        self.cursor >= self.events.len()
    }
}

fn parse_line(s: &str, lineno: usize) -> Result<ScriptedEvent> {
    let mut tokens = s.split_whitespace();
    let at_str = tokens
        .next()
        .ok_or_else(|| Error::Backend(format!("line {lineno}: empty")))?;
    let at_secs: f64 = at_str
        .parse()
        .map_err(|_| Error::Backend(format!("line {lineno}: bad time '{at_str}'")))?;
    let verb = tokens
        .next()
        .ok_or_else(|| Error::Backend(format!("line {lineno}: missing verb")))?;
    if verb != "press" {
        return Err(Error::Backend(format!("line {lineno}: only 'press' supported, got '{verb}'")));
    }
    let key = tokens
        .next()
        .ok_or_else(|| Error::Backend(format!("line {lineno}: missing key")))?
        .to_string();

    let mut modifier = Modifier::None;
    let mut repeat = 1u32;

    while let Some(tok) = tokens.next() {
        match tok {
            "shift" => modifier = Modifier::Shift,
            "numlock" => modifier = Modifier::NumLock,
            "repeat" => {
                let n_str = tokens
                    .next()
                    .ok_or_else(|| Error::Backend(format!("line {lineno}: 'repeat' needs count")))?;
                repeat = n_str.parse().map_err(|_| {
                    Error::Backend(format!("line {lineno}: bad repeat count '{n_str}'"))
                })?;
            }
            "hold" => {
                // For Plan 2 mock, treat 'hold N' as 'repeat ceil(N*10)' to
                // simulate ~10 Hz repeat-while-held. Real continuous hold
                // behavior is platform input-source territory.
                let dur_str = tokens
                    .next()
                    .ok_or_else(|| Error::Backend(format!("line {lineno}: 'hold' needs seconds")))?;
                let dur: f64 = dur_str.parse().map_err(|_| {
                    Error::Backend(format!("line {lineno}: bad hold seconds '{dur_str}'"))
                })?;
                repeat = (dur * 10.0).ceil() as u32;
            }
            other => {
                return Err(Error::Backend(format!(
                    "line {lineno}: unexpected token '{other}'"
                )))
            }
        }
    }

    Ok(ScriptedEvent {
        at: Duration::from_secs_f64(at_secs),
        key,
        modifier,
        repeat,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_script() {
        let s = "
            # comment
            0.0  press 1
            0.5  press Tab
        ";
        let m = MockInput::from_script(s).unwrap();
        assert_eq!(m.events.len(), 2);
        assert_eq!(m.events[0].key, "1");
        assert_eq!(m.events[1].key, "Tab");
    }

    #[test]
    fn drain_returns_events_at_or_before_now() {
        let s = "0.0 press a\n1.0 press b\n2.0 press c";
        let mut m = MockInput::from_script(s).unwrap();
        let drained = m.drain_until(Duration::from_secs(1));
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].0, "a");
        assert_eq!(drained[1].0, "b");
        assert!(!m.finished());
        let rest = m.drain_until(Duration::from_secs(10));
        assert_eq!(rest.len(), 1);
        assert!(m.finished());
    }

    #[test]
    fn repeat_expands() {
        let s = "0.0 press = repeat 3";
        let mut m = MockInput::from_script(s).unwrap();
        let drained = m.drain_until(Duration::from_secs(1));
        assert_eq!(drained.len(), 3);
        assert!(drained.iter().all(|e| e.0 == "="));
    }

    #[test]
    fn shift_modifier_parsed() {
        let s = "0.0 press 1 shift";
        let mut m = MockInput::from_script(s).unwrap();
        let drained = m.drain_until(Duration::from_secs(1));
        assert_eq!(drained[0].1, Modifier::Shift);
    }

    #[test]
    fn hold_expands_at_ten_hz() {
        let s = "0.0 press [ hold 2.0";
        let mut m = MockInput::from_script(s).unwrap();
        let drained = m.drain_until(Duration::from_secs(1));
        assert_eq!(drained.len(), 20); // 2.0s * 10 Hz
    }

    #[test]
    fn empty_script_finishes_immediately() {
        let m = MockInput::from_script("").unwrap();
        assert!(m.finished());
    }
}
