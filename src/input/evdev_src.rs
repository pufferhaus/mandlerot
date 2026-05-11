//! Pi USB-HID input via `/dev/input/event*` using the `evdev` crate.

#![cfg(all(feature = "pi", target_os = "linux"))]

use evdev::{Device, EventType, Key};

use super::keymap::{Modifier, RawKey};
use crate::error::{Error, Result};

pub struct EvdevInput {
    devices: Vec<Device>,
    /// Across all devices: which modifier keys are currently held.
    shift_held: bool,
    numlock_held: bool,
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
                Ok(d) => {
                    if d.supported_keys().is_some() {
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
            numlock_held: false,
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
                let key = Key::new(evt.code());
                let down = evt.value() == 1; // 1=down, 0=up, 2=repeat
                let repeat = evt.value() == 2;
                if !down && !repeat {
                    // key release
                    match key {
                        Key::KEY_LEFTSHIFT | Key::KEY_RIGHTSHIFT => self.shift_held = false,
                        Key::KEY_NUMLOCK => self.numlock_held = false,
                        _ => {}
                    }
                    continue;
                }
                // press or repeat
                match key {
                    Key::KEY_LEFTSHIFT | Key::KEY_RIGHTSHIFT => {
                        self.shift_held = true;
                        continue;
                    }
                    Key::KEY_NUMLOCK => {
                        self.numlock_held = true;
                        continue;
                    }
                    _ => {}
                }
                if let Some(raw) = key_to_raw(key) {
                    let m = if self.shift_held {
                        Modifier::Shift
                    } else if self.numlock_held {
                        Modifier::NumLock
                    } else {
                        Modifier::None
                    };
                    out.push((raw, m));
                }
            }
        }
        out
    }
}

fn key_to_raw(k: Key) -> Option<RawKey> {
    Some(match k {
        Key::KEY_TAB => "Tab".into(),
        Key::KEY_ESC => "Esc".into(),
        Key::KEY_ENTER => "Enter".into(),
        Key::KEY_KPENTER => "NumpadEnter".into(),
        Key::KEY_BACKSPACE => "Backspace".into(),
        Key::KEY_SPACE => "Space".into(),
        Key::KEY_BACKSLASH => "Backslash".into(),
        Key::KEY_MINUS => "Minus".into(),
        Key::KEY_EQUAL => "Equal".into(),
        Key::KEY_LEFTBRACE => "BracketLeft".into(),
        Key::KEY_RIGHTBRACE => "BracketRight".into(),
        Key::KEY_KP0 => "Numpad0".into(),
        Key::KEY_KP1 => "Numpad1".into(),
        Key::KEY_KP2 => "Numpad2".into(),
        Key::KEY_KP3 => "Numpad3".into(),
        Key::KEY_KP4 => "Numpad4".into(),
        Key::KEY_KP5 => "Numpad5".into(),
        Key::KEY_KP6 => "Numpad6".into(),
        Key::KEY_KP7 => "Numpad7".into(),
        Key::KEY_KP8 => "Numpad8".into(),
        Key::KEY_KP9 => "Numpad9".into(),
        Key::KEY_KPPLUS => "NumpadAdd".into(),
        Key::KEY_KPMINUS => "NumpadSubtract".into(),
        Key::KEY_KPASTERISK => "NumpadMultiply".into(),
        Key::KEY_KPSLASH => "NumpadDivide".into(),
        Key::KEY_KPDOT => "NumpadDecimal".into(),
        Key::KEY_F1 => "F1".into(),
        Key::KEY_F2 => "F2".into(),
        Key::KEY_F3 => "F3".into(),
        Key::KEY_F4 => "F4".into(),
        Key::KEY_F5 => "F5".into(),
        Key::KEY_1 => "1".into(),
        Key::KEY_2 => "2".into(),
        Key::KEY_3 => "3".into(),
        Key::KEY_4 => "4".into(),
        Key::KEY_5 => "5".into(),
        Key::KEY_6 => "6".into(),
        Key::KEY_7 => "7".into(),
        Key::KEY_8 => "8".into(),
        Key::KEY_9 => "9".into(),
        Key::KEY_0 => "0".into(),
        Key::KEY_F => "F".into(),
        Key::KEY_G => "G".into(),
        Key::KEY_L => "L".into(),
        Key::KEY_M => "M".into(),
        Key::KEY_N => "N".into(),
        Key::KEY_UP => "Up".into(),
        Key::KEY_DOWN => "Down".into(),
        Key::KEY_LEFT => "Left".into(),
        Key::KEY_RIGHT => "Right".into(),
        Key::KEY_PAGEUP => "PageUp".into(),
        Key::KEY_PAGEDOWN => "PageDown".into(),
        Key::KEY_HOME => "Home".into(),
        Key::KEY_END => "End".into(),
        _ => return None,
    })
}
