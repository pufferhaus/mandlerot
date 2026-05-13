//! Pi USB-HID input via `/dev/input/event*` using the `evdev` crate.

#![cfg(all(feature = "pi", target_os = "linux"))]

use evdev::{Device, EventType, KeyCode};

use super::keymap::{Modifier, RawKey};
use crate::error::{Error, Result};

pub struct EvdevInput {
    devices: Vec<Device>,
    /// Across all devices: which modifier keys are currently held.
    shift_held: bool,
    /// "000" key on the cheap USB numpad acts as a held modifier. The
    /// underlying scancode is `KEY_KPCOMMA` or `KEY_KPPLUSMINUS` depending
    /// on firmware; both map to `RawKey::Numpad000` for the keymap.
    numpad000_held: bool,
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
        })
    }

    /// Non-blocking poll. Returns translated `(RawKey, Modifier)` pairs.
    pub fn poll(&mut self) -> Vec<(RawKey, Modifier)> {
        let mut out = Vec::new();
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
                    _ => {}
                }
                if let Some(raw) = key_to_raw(key) {
                    // Shift wins over the pad's `000` modifier if both are
                    // somehow held. NumLock used to be a third modifier but
                    // is now a regular key (SceneCycleActive previous).
                    let m = if self.shift_held {
                        Modifier::Shift
                    } else if self.numpad000_held {
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
        out
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
