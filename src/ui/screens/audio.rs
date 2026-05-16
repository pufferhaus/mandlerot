//! Audio settings. Five knobs:
//!   noise floor, gain bass, gain lomid, gain himid, gain treble.
//! ↑↓ select knob, ←→ nudge by step, `r` reset to default. Changes are
//! immediately visible (audio thread reads on each tick) and persisted to
//! `<state_dir>/audio.toml` on every nudge.

use crate::audio::params::{DEFAULT_GAIN, DEFAULT_NOISE_FLOOR};
use crate::status::{Cell, TextScreen, ATTR_BRIGHT, ATTR_DIM, ATTR_INVERSE, ATTR_NORMAL};
use crate::ui::{RenderCtx, Screen, ScreenCtx, ScreenResult};

#[derive(Clone, Copy)]
struct Knob {
    label: &'static str,
    min: f32,
    max: f32,
    default: f32,
    step: f32,
}

const KNOBS: &[Knob] = &[
    Knob {
        label: "Noise Floor",
        min: 0.0,
        max: 50.0,
        default: DEFAULT_NOISE_FLOOR,
        step: 0.5,
    },
    Knob {
        label: "Gain Bass",
        min: 0.0,
        max: 4.0,
        default: DEFAULT_GAIN,
        step: 0.05,
    },
    Knob {
        label: "Gain Lo-Mid",
        min: 0.0,
        max: 4.0,
        default: DEFAULT_GAIN,
        step: 0.05,
    },
    Knob {
        label: "Gain Hi-Mid",
        min: 0.0,
        max: 4.0,
        default: DEFAULT_GAIN,
        step: 0.05,
    },
    Knob {
        label: "Gain Treble",
        min: 0.0,
        max: 4.0,
        default: DEFAULT_GAIN,
        step: 0.05,
    },
    Knob {
        label: "Gain Mid",
        min: 0.0,
        max: 4.0,
        default: DEFAULT_GAIN,
        step: 0.05,
    },
];

pub struct AudioSettingsScreen {
    cursor: u8,
}

impl Default for AudioSettingsScreen {
    fn default() -> Self {
        Self { cursor: 0 }
    }
}

impl AudioSettingsScreen {
    pub fn new() -> Self {
        Self::default()
    }
}

fn read(ctx_audio: &crate::audio::params::AudioParams, i: usize) -> f32 {
    if i == 0 {
        ctx_audio.noise_floor()
    } else {
        ctx_audio.gain(i - 1)
    }
}

fn write(ctx_audio: &crate::audio::params::AudioParams, i: usize, v: f32) {
    if i == 0 {
        ctx_audio.set_noise_floor(v);
    } else {
        ctx_audio.set_gain(i - 1, v);
    }
}

impl Screen for AudioSettingsScreen {
    fn render(&self, g: &mut TextScreen, ctx: &RenderCtx) {
        draw_border(g, "AUDIO");
        let start_row = 4;
        for (i, k) in KNOBS.iter().enumerate() {
            let row = start_row + i * 2;
            let is_cursor = self.cursor as usize == i;
            let attr = if is_cursor { ATTR_INVERSE } else { ATTR_NORMAL };
            let marker = if is_cursor { '>' } else { ' ' };
            g.set(row, 3, Cell::new(marker, ATTR_BRIGHT));
            g.write(row, 5, attr, k.label);

            let v = read(ctx.audio, i);
            let frac = ((v - k.min) / (k.max - k.min)).clamp(0.0, 1.0);
            let bar_w = 30usize;
            let filled = (frac * bar_w as f32).round() as usize;
            g.set(row, 22, Cell::new('[', ATTR_DIM));
            for c in 0..bar_w {
                let ch = if c < filled { '█' } else { '·' };
                let bar_attr = if c < filled {
                    ATTR_BRIGHT
                } else {
                    ATTR_DIM
                };
                g.set(row, 23 + c, Cell::new(ch, bar_attr));
            }
            g.set(row, 23 + bar_w, Cell::new(']', ATTR_DIM));
            let val_str = format_val(v);
            g.write(row, 23 + bar_w + 2, ATTR_NORMAL, &val_str);
        }
        draw_footer(g, "^v select   <> adjust   r reset   Esc back");
    }

    fn handle_key(&mut self, key: &str, ctx: &mut ScreenCtx) -> ScreenResult {
        let i = self.cursor as usize;
        match key {
            "Esc" | "Backspace" => ScreenResult::Pop,
            "Up" => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                ScreenResult::Continue
            }
            "Down" => {
                if (self.cursor as usize) + 1 < KNOBS.len() {
                    self.cursor += 1;
                }
                ScreenResult::Continue
            }
            "Left" => {
                nudge(ctx, i, -1.0);
                ScreenResult::Continue
            }
            "Right" => {
                nudge(ctx, i, 1.0);
                ScreenResult::Continue
            }
            "r" | "R" => {
                write(ctx.audio, i, KNOBS[i].default);
                let _ = ctx.audio.save(ctx.state_dir);
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }
}

fn nudge(ctx: &mut ScreenCtx, i: usize, dir: f32) {
    let k = KNOBS[i];
    let cur = read(ctx.audio, i);
    let next = (cur + dir * k.step).clamp(k.min, k.max);
    write(ctx.audio, i, next);
    if let Err(e) = ctx.audio.save(ctx.state_dir) {
        tracing::warn!("save audio.toml: {e}");
    }
}

fn format_val(v: f32) -> String {
    if v.abs() >= 10.0 {
        format!("{v:>5.1}")
    } else {
        format!("{v:>5.2}")
    }
}

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

    fn s_ctx<'a>(
        scenes: &'a [String],
        bindings: &'a mut SlotBindings,
        dir: &'a std::path::Path,
        audio: &'a std::sync::Arc<AudioParams>,
    ) -> ScreenCtx<'a> {
        ScreenCtx {
            scenes,
            bindings,
            state_dir: dir,
            audio,
            postfx: None,
            video_status: crate::video::VideoStatus::NoDevice,
        }
    }

    #[test]
    fn right_arrow_nudges_focused_knob_up() {
        let mut s = AudioSettingsScreen::new();
        let scenes: Vec<String> = vec![];
        let mut b = SlotBindings::default();
        let dir = tempfile::tempdir().unwrap();
        let audio = AudioParams::new();
        let before = audio.noise_floor();
        let _ = s.handle_key("Right", &mut s_ctx(&scenes, &mut b, dir.path(), &audio));
        assert!(audio.noise_floor() > before);
    }

    #[test]
    fn left_arrow_nudges_down_clamped_at_min() {
        let mut s = AudioSettingsScreen::new();
        let scenes: Vec<String> = vec![];
        let mut b = SlotBindings::default();
        let dir = tempfile::tempdir().unwrap();
        let audio = AudioParams::new();
        audio.set_noise_floor(0.0);
        for _ in 0..5 {
            let _ = s.handle_key("Left", &mut s_ctx(&scenes, &mut b, dir.path(), &audio));
        }
        assert_eq!(audio.noise_floor(), 0.0);
    }

    #[test]
    fn r_resets_to_default() {
        let mut s = AudioSettingsScreen::new();
        let scenes: Vec<String> = vec![];
        let mut b = SlotBindings::default();
        let dir = tempfile::tempdir().unwrap();
        let audio = AudioParams::new();
        audio.set_noise_floor(20.0);
        let _ = s.handle_key("r", &mut s_ctx(&scenes, &mut b, dir.path(), &audio));
        assert_eq!(audio.noise_floor(), DEFAULT_NOISE_FLOOR);
    }

    #[test]
    fn down_arrow_moves_cursor_to_band_gain() {
        let mut s = AudioSettingsScreen::new();
        let scenes: Vec<String> = vec![];
        let mut b = SlotBindings::default();
        let dir = std::path::PathBuf::from("/tmp");
        let audio = AudioParams::new();
        let _ = s.handle_key("Down", &mut s_ctx(&scenes, &mut b, &dir, &audio));
        let before = audio.gain(0);
        let _ = s.handle_key("Right", &mut s_ctx(&scenes, &mut b, &dir, &audio));
        assert!(audio.gain(0) > before);
        assert_eq!(audio.noise_floor(), DEFAULT_NOISE_FLOOR);
    }

    #[test]
    fn esc_pops() {
        let mut s = AudioSettingsScreen::new();
        let scenes: Vec<String> = vec![];
        let mut b = SlotBindings::default();
        let dir = std::path::PathBuf::from("/tmp");
        let audio = AudioParams::new();
        let r = s.handle_key("Esc", &mut s_ctx(&scenes, &mut b, &dir, &audio));
        assert!(matches!(r, ScreenResult::Pop));
    }
}
