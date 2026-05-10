//! Desktop keyboard input via winit `KeyEvent`. Translates winit logical
//! keys to our `RawKey` strings and tracks Shift state.
//!
//! Module-level `#[cfg(feature = "desktop")]` is applied at the parent
//! `mod input` declaration site, so we don't repeat it here.

use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{Key, NamedKey, PhysicalKey};

use super::keymap::{Modifier, RawKey};

#[derive(Debug, Default)]
pub struct WinitInputState {
    shift_held: bool,
}

impl WinitInputState {
    pub fn handle(&mut self, event: &KeyEvent) -> Option<(RawKey, Modifier)> {
        // Track shift but don't emit it as an action key.
        if let Key::Named(NamedKey::Shift) = event.logical_key {
            self.shift_held = matches!(event.state, ElementState::Pressed);
            return None;
        }
        if event.state != ElementState::Pressed {
            return None;
        }

        let raw = key_to_raw(&event.logical_key, event.physical_key)?;
        let modifier = if self.shift_held {
            Modifier::Shift
        } else {
            Modifier::None
        };
        Some((raw, modifier))
    }
}

fn key_to_raw(logical: &Key, physical: PhysicalKey) -> Option<RawKey> {
    if let Key::Named(named) = logical {
        return Some(match named {
            NamedKey::Tab => "Tab".into(),
            NamedKey::Enter => "Enter".into(),
            NamedKey::Escape => "Esc".into(),
            NamedKey::Backspace => "Backspace".into(),
            NamedKey::Space => "Space".into(),
            NamedKey::F1 => "F1".into(),
            NamedKey::F2 => "F2".into(),
            NamedKey::F3 => "F3".into(),
            NamedKey::F4 => "F4".into(),
            NamedKey::F5 => "F5".into(),
            _ => return None,
        });
    }
    if let Key::Character(s) = logical {
        let s = s.as_str();
        return Some(match s {
            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "0" => s.into(),
            "-" | "_" => "Minus".into(),
            "=" | "+" => "Equal".into(),
            "[" | "{" => "BracketLeft".into(),
            "]" | "}" => "BracketRight".into(),
            "\\" | "|" => "Backslash".into(),
            // Letter keys — uppercase shorthand
            other if other.len() == 1 => other.to_ascii_uppercase(),
            _ => return None,
        });
    }
    // Use physical-key fallback for things like `KeyN` when logical_key didn't help.
    use winit::keyboard::KeyCode;
    if let PhysicalKey::Code(code) = physical {
        return Some(match code {
            KeyCode::KeyN => "N".into(),
            KeyCode::KeyM => "M".into(),
            KeyCode::KeyG => "G".into(),
            KeyCode::KeyL => "L".into(),
            KeyCode::KeyF => "F".into(),
            _ => return None,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // NOTE: winit 0.30 `KeyEvent` has a `pub(crate) platform_specific` field
    // (`platform_impl::KeyEventExtra`) that cannot be constructed from outside
    // the winit crate. Struct-literal construction of `KeyEvent` is therefore
    // impossible in external code. The three tests below are marked `#[ignore]`
    // to document the intended behaviour without failing the build.
    //
    // Integration coverage comes from the live desktop loop via `WinitGlTarget::
    // drain_key_events()` (Task 6 Step 2).

    #[test]
    #[ignore = "winit::event::KeyEvent cannot be constructed externally (platform_specific is pub(crate))"]
    fn shift_press_sets_state_returns_none() {
        // Would test: pressing Shift sets shift_held=true and returns None.
        let _ = WinitInputState::default();
    }

    #[test]
    #[ignore = "winit::event::KeyEvent cannot be constructed externally (platform_specific is pub(crate))"]
    fn tab_pressed_returns_tab_no_modifier() {
        // Would test: pressing Tab with no shift held returns ("Tab", Modifier::None).
        let _ = WinitInputState::default();
    }

    #[test]
    #[ignore = "winit::event::KeyEvent cannot be constructed externally (platform_specific is pub(crate))"]
    fn key_release_ignored() {
        // Would test: a Released event returns None regardless of key.
        let _ = WinitInputState::default();
    }
}
