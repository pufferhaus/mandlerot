//! Pi USB-HID input via `/dev/input/event*` using the `evdev` crate.

#![cfg(all(feature = "pi", target_os = "linux"))]

use std::time::{Duration, Instant};

use evdev::{Device, EventType, KeyCode};

use super::keymap::{Modifier, RawKey};
use crate::error::{Error, Result};

/// Three `KEY_KP0` presses within this window classify as a `000` key tap
/// (the firmware-issued triple-shot). Production unit emits the burst with
/// up to 50 ms between presses, so the window has to span ~100 ms+ to keep
/// the first event from aging out before the third arrives. A genuine
/// double-time TapTempo press (e.g. 600 BPM = 100 ms between taps) is the
/// theoretical false-positive ceiling — well above any musical use case.
const KP0_BURST_WINDOW: Duration = Duration::from_millis(120);

/// After a `000` burst classifies, the `Numpad000` modifier stays armed for
/// this long so the operator's follow-up key (e.g. `000`+`Enter` for audio
/// bypass) lands as a combo. Re-armed by every subsequent burst when the
/// key is physically held. The modifier is consumed by the first non-KP0
/// keypress.
const NUMPAD000_HOLD: Duration = Duration::from_millis(600);

/// The cheap pad's firmware wraps the leftmost-column digit keys AND the
/// `000` key with a NumLock press both before and after, so the digit
/// always reports its number-form scancode regardless of OS NumLock state.
/// Observed wrap spacing on the production unit is up to **200 ms** between
/// the digit release and the trailing NumLock. The window has to cover
/// that worst-case gap so the trailing wrap never escapes as a spurious
/// scene-cycle action.
const NUMLOCK_WRAP_WINDOW: Duration = Duration::from_millis(250);

pub struct EvdevInput {
    devices: Vec<Device>,
    /// Across all devices: which modifier keys are currently held.
    shift_held: bool,
    /// Legacy "000" key path: some firmware revs send `KEY_KPCOMMA` /
    /// `KEY_KPPLUSMINUS` as a real held key. We still honour that. The
    /// 3×`KEY_KP0`-burst path below is the dominant case on the unit
    /// in production but this keeps the older variant working.
    numpad000_held: bool,
    /// Buffered `KEY_KP0` press timestamps awaiting burst classification.
    /// Three within `KP0_BURST_WINDOW` = `000` key; a single one that
    /// ages out of the window = plain `0` key (TapTempo).
    kp0_pending: Vec<Instant>,
    /// Deadline for the burst-engaged `Numpad000` modifier. `Some(t)` while
    /// active; reset to `None` either on timeout, on consumption by the
    /// next non-KP0 keypress, or after a TapTempo flush of a stale KP0
    /// (so `000` doesn't ride on a TapTempo press by accident).
    numpad000_until: Option<Instant>,
    /// When the most recent numpad-digit keypress arrived. A NumLock press
    /// arriving within `NUMLOCK_WRAP_WINDOW` AFTER this is the trailing
    /// half of an auto-wrap and gets dropped.
    last_digit_press: Option<Instant>,
    /// Buffered NumLock press waiting for classification. If a digit
    /// arrives within `NUMLOCK_WRAP_WINDOW`, this was the leading half of
    /// an auto-wrap and is dropped. Otherwise it ages out and fires the
    /// bound `NumLock` action (scene cycle previous).
    numlock_pending: Option<Instant>,
}

impl EvdevInput {
    /// Open every keyboard-capable device under `/dev/input`.
    pub fn open_all() -> Result<Self> {
        let mut devices = Vec::new();
        for entry in std::fs::read_dir("/dev/input")? {
            let entry = entry?;
            let path = entry.path();
            if !path
                .file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.starts_with("event"))
                .unwrap_or(false)
            {
                continue;
            }
            match Device::open(&path) {
                Ok(mut d) => {
                    if d.supported_keys().is_some() {
                        // Non-blocking fd: fetch_events returns WouldBlock
                        // instead of stalling the render loop.
                        let _ = d.set_nonblocking(true);
                        let _ = d.grab(); // best-effort exclusive grab
                        devices.push(d);
                    }
                }
                Err(e) => tracing::debug!("skip {path:?}: {e}"),
            }
        }
        if devices.is_empty() {
            return Err(Error::Backend("no input devices found".into()));
        }
        Ok(Self {
            devices,
            shift_held: false,
            numpad000_held: false,
            kp0_pending: Vec::new(),
            numpad000_until: None,
            last_digit_press: None,
            numlock_pending: None,
        })
    }

    /// Non-blocking poll. Returns translated `(RawKey, Modifier)` pairs.
    pub fn poll(&mut self) -> Vec<(RawKey, Modifier)> {
        let mut out = Vec::new();
        let now = Instant::now();
        for dev in &mut self.devices {
            // fetch_events drains pending events without blocking when
            // FETCH_EVENTS_NONBLOCK is set on the device fd.
            let events: Vec<_> = match dev.fetch_events() {
                Ok(it) => it.collect(),
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::WouldBlock {
                        continue;
                    }
                    tracing::warn!("evdev fetch: {e}");
                    continue;
                }
            };
            for evt in events {
                if evt.event_type() != EventType::KEY {
                    continue;
                }
                let key = KeyCode::new(evt.code());
                let down = evt.value() == 1; // 1=down, 0=up, 2=repeat
                let repeat = evt.value() == 2;
                if !down && !repeat {
                    // key release
                    match key {
                        KeyCode::KEY_LEFTSHIFT | KeyCode::KEY_RIGHTSHIFT => self.shift_held = false,
                        KeyCode::KEY_KPCOMMA | KeyCode::KEY_KPPLUSMINUS => {
                            self.numpad000_held = false;
                        }
                        _ => {}
                    }
                    continue;
                }
                // press or repeat
                match key {
                    KeyCode::KEY_LEFTSHIFT | KeyCode::KEY_RIGHTSHIFT => {
                        self.shift_held = true;
                        continue;
                    }
                    KeyCode::KEY_KPCOMMA | KeyCode::KEY_KPPLUSMINUS => {
                        self.numpad000_held = true;
                        continue;
                    }
                    KeyCode::KEY_NUMLOCK => {
                        let trailing = self
                            .last_digit_press
                            .map(|t| now.duration_since(t) <= NUMLOCK_WRAP_WINDOW)
                            .unwrap_or(false);
                        tracing::debug!(
                            "evdev: NUMLOCK press trailing={trailing} last_digit_age_ms={:?}",
                            self.last_digit_press.map(|t| now.duration_since(t).as_millis()),
                        );
                        if trailing {
                            self.numlock_pending = None;
                        } else {
                            self.numlock_pending = Some(now);
                        }
                        continue;
                    }
                    KeyCode::KEY_KP0 => {
                        // Defer classification: this might be the first of
                        // three `000` burst presses, or a lone TapTempo
                        // tap. Drop entries older than the burst window so
                        // a previous unrelated KP0 doesn't poison the
                        // count.
                        let cutoff = now.checked_sub(KP0_BURST_WINDOW);
                        self.kp0_pending
                            .retain(|t| cutoff.map(|c| *t >= c).unwrap_or(true));
                        self.kp0_pending.push(now);
                        // This digit press cancels any pending leading-wrap
                        // NumLock and arms trailing-wrap suppression for
                        // the next ~30 ms.
                        self.numlock_pending = None;
                        self.last_digit_press = Some(now);
                        tracing::debug!(
                            "evdev: KP0 press, pending_len={}",
                            self.kp0_pending.len()
                        );
                        if self.kp0_pending.len() >= 3 {
                            tracing::debug!("evdev: KP0 burst → Numpad000 armed");
                            self.numpad000_until = Some(now + NUMPAD000_HOLD);
                            self.kp0_pending.clear();
                        }
                        continue;
                    }
                    KeyCode::KEY_KP1
                    | KeyCode::KEY_KP2
                    | KeyCode::KEY_KP3
                    | KeyCode::KEY_KP4
                    | KeyCode::KEY_KP5
                    | KeyCode::KEY_KP6
                    | KeyCode::KEY_KP7
                    | KeyCode::KEY_KP8
                    | KeyCode::KEY_KP9
                    | KeyCode::KEY_KPDOT => {
                        // Same wrap-suppression handshake as KP0: cancel
                        // any leading-wrap NumLock and arm trailing-wrap
                        // suppression. Then fall through to the normal
                        // key_to_raw emit path.
                        self.numlock_pending = None;
                        self.last_digit_press = Some(now);
                    }
                    _ => {}
                }
                if let Some(raw) = key_to_raw(key) {
                    // Any non-KP0 press flushes the pending KP0 buffer first
                    // — those KP0s are unambiguously single-taps (the burst
                    // would have completed by now). Same for the deferred
                    // NumLock: by the time we reach a non-digit, non-NumLock
                    // key the wrap-pair classification window is moot, so
                    // anything still pending was a genuine NumLock press.
                    flush_pending_kp0(&mut self.kp0_pending, &mut self.numpad000_until, &mut out);
                    flush_pending_numlock(&mut self.numlock_pending, &mut out);
                    let burst_modifier = self
                        .numpad000_until
                        .map(|t| t > now)
                        .unwrap_or(false);
                    // Shift wins over the pad's `000` modifier if both are
                    // somehow held. NumLock used to be a third modifier but
                    // is now a regular key (SceneCycleActive previous).
                    let m = if self.shift_held {
                        Modifier::Shift
                    } else if self.numpad000_held || burst_modifier {
                        if burst_modifier {
                            // Burst-engaged modifier is sticky for one
                            // press; consume it so subsequent keys land as
                            // unmodified unless another burst rearms it.
                            self.numpad000_until = None;
                        }
                        Modifier::Numpad000
                    } else {
                        Modifier::None
                    };
                    out.push((raw, m));
                } else {
                    // Unknown evdev scancode — useful when characterising a
                    // new HID device. DEBUG level so log spam from a stuck
                    // key on a misconfigured pad doesn't fill the journal.
                    tracing::debug!("evdev: unmapped key {:?} (code {})", key, evt.code());
                }
            }
        }
        // After the events loop, flush any KP0 entries that aged out of
        // the burst window without becoming a `000` burst — those were
        // genuine single-tap TapTempo presses.
        let flush_cutoff = now.checked_sub(KP0_BURST_WINDOW);
        let aged: Vec<Instant> = self
            .kp0_pending
            .iter()
            .copied()
            .take_while(|t| flush_cutoff.map(|c| *t < c).unwrap_or(false))
            .collect();
        let to_drain = aged.len();
        for _ in 0..to_drain {
            // Stale KP0 = bare TapTempo; don't ride on a stale `000`
            // modifier even if one happens to still be armed.
            self.numpad000_until = None;
            out.push(("Numpad0".into(), Modifier::None));
        }
        self.kp0_pending.drain(..to_drain);

        // Same age-out for the deferred NumLock: if it's older than the
        // wrap window without a paired digit arriving, the press was a
        // real NumLock tap and the bound action (scene cycle previous)
        // should fire.
        let numlock_cutoff = now.checked_sub(NUMLOCK_WRAP_WINDOW);
        if let Some(t) = self.numlock_pending {
            if numlock_cutoff.map(|c| t < c).unwrap_or(false) {
                out.push(("NumLock".into(), Modifier::None));
                self.numlock_pending = None;
            }
        }
        out
    }
}

/// Emit every queued KP0 as an unmodified `Numpad0` (TapTempo) and clear
/// the buffer. Called when a non-KP0 key arrives — by that point the
/// queued KP0s couldn't have been part of an incoming burst, so they're
/// unambiguously single taps. Also clears any burst-engaged `Numpad000`
/// modifier so a TapTempo press doesn't accidentally consume it.
fn flush_pending_kp0(
    pending: &mut Vec<Instant>,
    numpad000_until: &mut Option<Instant>,
    out: &mut Vec<(RawKey, Modifier)>,
) {
    if pending.is_empty() {
        return;
    }
    for _ in 0..pending.len() {
        out.push(("Numpad0".into(), Modifier::None));
    }
    pending.clear();
    *numpad000_until = None;
}

/// Emit a deferred NumLock press as the bound `NumLock` action. Called
/// when a non-digit, non-NumLock key arrives — at that point the wrap-
/// classification window is irrelevant, the pending press was genuine.
fn flush_pending_numlock(
    pending: &mut Option<Instant>,
    out: &mut Vec<(RawKey, Modifier)>,
) {
    if pending.take().is_some() {
        out.push(("NumLock".into(), Modifier::None));
    }
}

fn key_to_raw(k: KeyCode) -> Option<RawKey> {
    Some(match k {
        KeyCode::KEY_TAB => "Tab".into(),
        KeyCode::KEY_ESC => "Esc".into(),
        KeyCode::KEY_ENTER => "Enter".into(),
        KeyCode::KEY_KPENTER => "NumpadEnter".into(),
        KeyCode::KEY_BACKSPACE => "Backspace".into(),
        KeyCode::KEY_SPACE => "Space".into(),
        KeyCode::KEY_BACKSLASH => "Backslash".into(),
        KeyCode::KEY_MINUS => "Minus".into(),
        KeyCode::KEY_EQUAL => "Equal".into(),
        KeyCode::KEY_LEFTBRACE => "BracketLeft".into(),
        KeyCode::KEY_RIGHTBRACE => "BracketRight".into(),
        KeyCode::KEY_KP0 => "Numpad0".into(),
        KeyCode::KEY_KP1 => "Numpad1".into(),
        KeyCode::KEY_KP2 => "Numpad2".into(),
        KeyCode::KEY_KP3 => "Numpad3".into(),
        KeyCode::KEY_KP4 => "Numpad4".into(),
        KeyCode::KEY_KP5 => "Numpad5".into(),
        KeyCode::KEY_KP6 => "Numpad6".into(),
        KeyCode::KEY_KP7 => "Numpad7".into(),
        KeyCode::KEY_KP8 => "Numpad8".into(),
        KeyCode::KEY_KP9 => "Numpad9".into(),
        KeyCode::KEY_KPPLUS => "NumpadAdd".into(),
        KeyCode::KEY_KPMINUS => "NumpadSubtract".into(),
        KeyCode::KEY_KPASTERISK => "NumpadMultiply".into(),
        KeyCode::KEY_KPSLASH => "NumpadDivide".into(),
        KeyCode::KEY_KPDOT => "NumpadDecimal".into(),
        // NumLock is no longer treated as a held modifier — on the rotated
        // cheap pad it sits in the operator's primary-key zone, so it's a
        // regular key that fires `SceneCycleActive { dir: -1 }`.
        KeyCode::KEY_NUMLOCK => "NumLock".into(),
        KeyCode::KEY_F1 => "F1".into(),
        KeyCode::KEY_F2 => "F2".into(),
        KeyCode::KEY_F3 => "F3".into(),
        KeyCode::KEY_F4 => "F4".into(),
        KeyCode::KEY_F5 => "F5".into(),
        KeyCode::KEY_1 => "1".into(),
        KeyCode::KEY_2 => "2".into(),
        KeyCode::KEY_3 => "3".into(),
        KeyCode::KEY_4 => "4".into(),
        KeyCode::KEY_5 => "5".into(),
        KeyCode::KEY_6 => "6".into(),
        KeyCode::KEY_7 => "7".into(),
        KeyCode::KEY_8 => "8".into(),
        KeyCode::KEY_9 => "9".into(),
        KeyCode::KEY_0 => "0".into(),
        KeyCode::KEY_F => "F".into(),
        KeyCode::KEY_G => "G".into(),
        KeyCode::KEY_L => "L".into(),
        KeyCode::KEY_M => "M".into(),
        KeyCode::KEY_N => "N".into(),
        KeyCode::KEY_UP => "Up".into(),
        KeyCode::KEY_DOWN => "Down".into(),
        KeyCode::KEY_LEFT => "Left".into(),
        KeyCode::KEY_RIGHT => "Right".into(),
        KeyCode::KEY_PAGEUP => "PageUp".into(),
        KeyCode::KEY_PAGEDOWN => "PageDown".into(),
        KeyCode::KEY_HOME => "Home".into(),
        KeyCode::KEY_END => "End".into(),
        _ => return None,
    })
}
