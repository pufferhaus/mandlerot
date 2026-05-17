//! F4 → Chromakey: enabled / color preset / luma threshold / soft edge / spill.

use crate::status::{Cell, TextScreen, ATTR_BRIGHT, ATTR_DIM, ATTR_INVERSE, ATTR_NORMAL};
use crate::ui::{RenderCtx, Screen, ScreenCtx, ScreenResult};

const ROWS: usize = 5;
const PRESETS: &[([f32; 3], &str)] = &[
    ([0.0, 1.0, 0.0], "green"),
    ([1.0, 0.0, 1.0], "magenta"),
    ([0.0, 0.0, 1.0], "blue"),
    ([1.0, 1.0, 0.0], "yellow"),
];

pub struct ChromakeyScreen {
    cursor: usize,
}

impl Default for ChromakeyScreen {
    fn default() -> Self {
        Self { cursor: 0 }
    }
}

impl ChromakeyScreen {
    pub fn new() -> Self {
        Self::default()
    }
}

fn preset_label(rgb: [f32; 3]) -> &'static str {
    for (c, l) in PRESETS {
        if (c[0] - rgb[0]).abs() < 1e-3
            && (c[1] - rgb[1]).abs() < 1e-3
            && (c[2] - rgb[2]).abs() < 1e-3
        {
            return l;
        }
    }
    "custom"
}

fn cycle_preset(cur: [f32; 3]) -> [f32; 3] {
    let idx = PRESETS
        .iter()
        .position(|(c, _)| {
            (c[0] - cur[0]).abs() < 1e-3
                && (c[1] - cur[1]).abs() < 1e-3
                && (c[2] - cur[2]).abs() < 1e-3
        })
        .unwrap_or(usize::MAX);
    let next = idx.wrapping_add(1) % PRESETS.len();
    PRESETS[next].0
}

impl Screen for ChromakeyScreen {
    fn render(&self, g: &mut TextScreen, ctx: &RenderCtx) {
        draw_border(g, "CHROMAKEY");
        let Some(s) = ctx.chromakey else {
            g.write(4, 3, ATTR_DIM, "(chromakey unavailable in this context)");
            draw_footer(g, "Esc back");
            return;
        };
        let rows: [(&str, String); ROWS] = [
            ("Enabled",        if s.enabled { "[x]".into() } else { "[ ]".into() }),
            ("Color",          preset_label(s.key_color).to_string()),
            ("Luma",           format!("{:.3}", s.luma_threshold)),
            ("Soft edge",      format!("{:.3}", s.edge_soft)),
            ("Spill suppress", if s.spill_suppress { "[x]".into() } else { "[ ]".into() }),
        ];
        for (i, (label, value)) in rows.iter().enumerate() {
            let row = 4 + i * 2;
            let is_cursor = self.cursor == i;
            let attr = if is_cursor { ATTR_INVERSE } else { ATTR_NORMAL };
            let marker = if is_cursor { '>' } else { ' ' };
            g.set(row, 3, Cell::new(marker, ATTR_BRIGHT));
            g.write(row, 5, attr, &format!("{label:<16}"));
            g.write(row, 24, ATTR_BRIGHT, value);
        }
        draw_footer(g, "^v select   Spc toggle/cycle   <> tune   Esc back");
    }

    fn handle_key(&mut self, key: &str, ctx: &mut ScreenCtx) -> ScreenResult {
        let Some(s) = ctx.chromakey.as_deref_mut() else { return ScreenResult::Pop };
        let state_dir = ctx.state_dir;
        let mut mutated = false;
        let result = match key {
            "Esc" | "Backspace" => return ScreenResult::Pop,
            "Up" => {
                if self.cursor > 0 { self.cursor -= 1; }
                ScreenResult::Continue
            }
            "Down" => {
                if self.cursor + 1 < ROWS { self.cursor += 1; }
                ScreenResult::Continue
            }
            "Space" | "Enter" | "NumpadEnter" => {
                match self.cursor {
                    0 => { s.enabled = !s.enabled; mutated = true; }
                    1 => { s.key_color = cycle_preset(s.key_color); mutated = true; }
                    4 => { s.spill_suppress = !s.spill_suppress; mutated = true; }
                    _ => {}
                }
                ScreenResult::Continue
            }
            "Left" => {
                match self.cursor {
                    2 => { s.luma_threshold = (s.luma_threshold - 0.005).max(0.0); mutated = true; }
                    3 => { s.edge_soft      = (s.edge_soft - 0.005).max(0.0);      mutated = true; }
                    _ => {}
                }
                ScreenResult::Continue
            }
            "Right" => {
                match self.cursor {
                    2 => { s.luma_threshold = (s.luma_threshold + 0.005).min(0.5); mutated = true; }
                    3 => { s.edge_soft      = (s.edge_soft + 0.005).min(0.2);      mutated = true; }
                    _ => {}
                }
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        };
        if mutated {
            if let Err(e) = s.save(state_dir) {
                tracing::warn!("save chromakey.toml: {e}");
            }
        }
        result
    }
}

// Mirrors the helpers in src/ui/screens/postfx.rs and settings.rs. These are
// near-duplicates of what those files declare — keep them in sync if you
// ever refactor border/footer drawing into a shared util.
fn draw_border(g: &mut TextScreen, title: &str) {
    g.fill(0, 0, 80, '─', ATTR_DIM);
    g.set(0, 0, Cell::new('┌', ATTR_DIM));
    g.set(0, 79, Cell::new('┐', ATTR_DIM));
    g.write(0, 3, ATTR_BRIGHT, &format!(" {title} "));
    for r in 1..25 {
        g.set(r, 0, Cell::new('│', ATTR_DIM));
        g.set(r, 79, Cell::new('│', ATTR_DIM));
    }
    g.fill(25, 0, 80, '─', ATTR_DIM);
    g.set(25, 0, Cell::new('└', ATTR_DIM));
    g.set(25, 79, Cell::new('┘', ATTR_DIM));
}

fn draw_footer(g: &mut TextScreen, hint: &str) {
    g.fill(23, 0, 80, '─', ATTR_DIM);
    g.set(23, 0, Cell::new('├', ATTR_DIM));
    g.set(23, 79, Cell::new('┤', ATTR_DIM));
    g.write(24, 2, ATTR_DIM, hint);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::params::AudioParams;
    use crate::preset::SlotBindings;
    use crate::render::chromakey::ChromakeyState;
    use std::sync::Arc;

    fn audio() -> Arc<AudioParams> { AudioParams::new() }

    #[test]
    fn space_on_enabled_row_toggles_and_saves() {
        let tmp = tempfile::tempdir().unwrap();
        let mut st = ChromakeyState::default();
        let mut bindings = SlotBindings::default();
        let a = audio();
        let scenes: Vec<String> = vec![];
        let mut screen = ChromakeyScreen::new();
        {
            let mut ctx = ScreenCtx {
                scenes: &scenes,
                bindings: &mut bindings,
                state_dir: tmp.path(),
                audio: &a,
                postfx: None,
                chromakey: Some(&mut st),
                video_status: crate::video::VideoStatus::NoDevice,
                active_look_slot: None,
                looks: None,
            };
            screen.handle_key("Space", &mut ctx);
        }
        assert!(st.enabled);
        let reloaded = ChromakeyState::load_or_default(tmp.path());
        assert!(reloaded.enabled);
    }

    #[test]
    fn right_arrow_on_luma_row_nudges_up() {
        let tmp = tempfile::tempdir().unwrap();
        let mut st = ChromakeyState::default();
        let before = st.luma_threshold;
        let mut bindings = SlotBindings::default();
        let a = audio();
        let scenes: Vec<String> = vec![];
        let mut screen = ChromakeyScreen::new();
        let mut ctx = ScreenCtx {
            scenes: &scenes,
            bindings: &mut bindings,
            state_dir: tmp.path(),
            audio: &a,
            postfx: None,
            chromakey: Some(&mut st),
            video_status: crate::video::VideoStatus::NoDevice,
            active_look_slot: None,
            looks: None,
        };
        // Move cursor down to luma row (index 2).
        screen.handle_key("Down", &mut ctx);
        screen.handle_key("Down", &mut ctx);
        screen.handle_key("Right", &mut ctx);
        assert!(st.luma_threshold > before);
    }
}
