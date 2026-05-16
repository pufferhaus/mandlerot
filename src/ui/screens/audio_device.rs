//! F4 → Audio → Device picker. Lists CPAL input devices including the
//! USB capture dongle's audio interface when present. Selecting one
//! writes `audio.toml::device` (empty = host default) and the main loop
//! respawns the audio worker thread.

use cpal::traits::{DeviceTrait, HostTrait};

use crate::status::{Cell, TextScreen, ATTR_BRIGHT, ATTR_DIM, ATTR_INVERSE, ATTR_NORMAL};
use crate::ui::{RenderCtx, Screen, ScreenCtx, ScreenResult};

const VISIBLE_ROWS: usize = 18;

pub struct AudioDeviceScreen {
    devices: Vec<String>,
    cursor: usize,
    scroll: usize,
}

impl AudioDeviceScreen {
    pub fn new() -> Self {
        let host = cpal::default_host();
        let mut devices: Vec<String> = vec![String::new()]; // "" = host default
        if let Ok(iter) = host.input_devices() {
            for d in iter {
                if let Ok(name) = d.name() {
                    devices.push(name);
                }
            }
        }
        Self {
            devices,
            cursor: 0,
            scroll: 0,
        }
    }

    fn label_for(&self, i: usize) -> String {
        let raw = &self.devices[i];
        if raw.is_empty() {
            "<host default>".to_string()
        } else {
            raw.clone()
        }
    }
}

impl Default for AudioDeviceScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl Screen for AudioDeviceScreen {
    fn render(&self, g: &mut TextScreen, _ctx: &RenderCtx) {
        // Border
        g.fill(0, 0, 80, '─', ATTR_DIM);
        g.set(0, 0, Cell::new('┌', ATTR_DIM));
        g.set(0, 79, Cell::new('┐', ATTR_DIM));
        g.write(0, 3, ATTR_BRIGHT, " AUDIO DEVICE ");
        for r in 1..25 {
            g.set(r, 0, Cell::new('│', ATTR_DIM));
            g.set(r, 79, Cell::new('│', ATTR_DIM));
        }
        g.fill(25, 0, 80, '─', ATTR_DIM);

        let scroll = clamp_scroll(self.scroll, self.cursor, self.devices.len());
        for row_idx in 0..VISIBLE_ROWS {
            let i = scroll + row_idx;
            if i >= self.devices.len() {
                break;
            }
            let row = 4 + row_idx;
            let is_cursor = i == self.cursor;
            let attr = if is_cursor { ATTR_INVERSE } else { ATTR_NORMAL };
            let marker = if is_cursor { '>' } else { ' ' };
            g.set(row, 3, Cell::new(marker, ATTR_BRIGHT));
            g.write(row, 5, attr, &truncate(&self.label_for(i), 70));
        }

        g.fill(23, 0, 80, '─', ATTR_DIM);
        g.write(24, 2, ATTR_DIM, "^v select   Enter pick   Esc back");
    }

    fn handle_key(&mut self, key: &str, ctx: &mut ScreenCtx) -> ScreenResult {
        let n = self.devices.len();
        if n == 0 {
            return ScreenResult::Pop;
        }
        match key {
            "Esc" | "Backspace" => ScreenResult::Pop,
            "Up" => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                self.scroll = clamp_scroll(self.scroll, self.cursor, n);
                ScreenResult::Continue
            }
            "Down" => {
                if self.cursor + 1 < n {
                    self.cursor += 1;
                }
                self.scroll = clamp_scroll(self.scroll, self.cursor, n);
                ScreenResult::Continue
            }
            "Enter" | "NumpadEnter" => {
                let pick = self.devices[self.cursor].clone();
                ctx.audio.set_device(pick);
                if let Err(e) = ctx.audio.save(ctx.state_dir) {
                    tracing::warn!("save audio.toml: {e}");
                }
                ScreenResult::Pop
            }
            _ => ScreenResult::Continue,
        }
    }
}

fn clamp_scroll(scroll: usize, cursor: usize, total: usize) -> usize {
    let mut s = scroll;
    if cursor < s {
        s = cursor;
    } else if cursor >= s + VISIBLE_ROWS {
        s = cursor + 1 - VISIBLE_ROWS;
    }
    let max_scroll = total.saturating_sub(VISIBLE_ROWS);
    s.min(max_scroll)
}

fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_constructs_with_at_least_host_default() {
        let s = AudioDeviceScreen::new();
        // The empty-string "host default" entry is always present.
        assert!(s.devices.iter().any(|d| d.is_empty()));
    }

    #[test]
    fn label_for_empty_string_shows_friendly_name() {
        let s = AudioDeviceScreen {
            devices: vec!["".to_string(), "Real Mic".to_string()],
            cursor: 0,
            scroll: 0,
        };
        assert_eq!(s.label_for(0), "<host default>");
        assert_eq!(s.label_for(1), "Real Mic");
    }
}
