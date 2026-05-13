//! Post-FX list + per-pass param drawer.
//!
//! `PostFxScreen` lists every pass loaded from `postfx/`. The user toggles
//! `enabled` with Space/Enter, and Right (or Tab) dives into
//! `PostFxParamScreen` for the highlighted pass to nudge its 8 params.
//!
//! Every mutation is persisted to `<state_dir>/postfx.toml` immediately so
//! the on-disk chain state always reflects the live render. Mirrors the
//! eager-save pattern from `AudioSettingsScreen` and `SlotsScreen`.

use crate::status::{Cell, TextScreen, ATTR_BRIGHT, ATTR_DIM, ATTR_INVERSE, ATTR_NORMAL};
use crate::ui::{RenderCtx, Screen, ScreenCtx, ScreenResult};

/// Top-level pass list. Cursor selects a pass.
pub struct PostFxScreen {
    cursor: usize,
}

impl Default for PostFxScreen {
    fn default() -> Self {
        Self { cursor: 0 }
    }
}

impl PostFxScreen {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Screen for PostFxScreen {
    fn render(&self, g: &mut TextScreen, ctx: &RenderCtx) {
        draw_border(g, "POST-FX");
        let Some(pfx) = ctx.postfx else {
            g.write(4, 3, ATTR_DIM, "(post-fx unavailable in this context)");
            draw_footer(g, "Esc back");
            return;
        };
        let passes = pfx.passes();
        if passes.is_empty() {
            g.write(4, 3, ATTR_DIM, "No post-FX passes loaded.");
            g.write(5, 3, ATTR_DIM, "Drop `<name>.{glsl,toml}` pairs into `postfx/`.");
            draw_footer(g, "Esc back");
            return;
        }
        g.write(3, 3, ATTR_DIM, "PASS              ENABLED   PARAMS");
        let start_row = 5;
        for (i, p) in passes.iter().enumerate() {
            let row = start_row + i;
            if row > 21 {
                break;
            }
            let is_cursor = self.cursor == i;
            let attr = if is_cursor { ATTR_INVERSE } else { ATTR_NORMAL };
            let marker = if is_cursor { '>' } else { ' ' };
            g.set(row, 2, Cell::new(marker, ATTR_BRIGHT));
            // Name (padded to 16) + on/off + first param summary.
            let display = p
                .meta
                .display_name
                .clone()
                .unwrap_or_else(|| p.name.clone());
            g.write(row, 4, attr, &format!("{:<16}", truncate(&display, 16)));
            let on_str = if p.enabled { "[x]" } else { "[ ]" };
            let on_attr = if p.enabled { ATTR_BRIGHT } else { ATTR_DIM };
            g.write(row, 22, on_attr, on_str);
            // Summarise the first 3 params so the user can see what's set at
            // a glance without diving in.
            let mut col = 28usize;
            for (slot_i, d) in p.params.defs().iter().enumerate().take(3) {
                if col + 14 > 78 {
                    break;
                }
                if let Some(v) = p.params.get(&d.name) {
                    let name = truncate(&d.name, 6);
                    let summary = format!("{name}={}", short_val(v));
                    g.write(row, col, ATTR_DIM, &summary);
                    col += summary.chars().count() + 2;
                }
                let _ = slot_i;
            }
        }
        draw_footer(g, "^v select   Spc toggle   > tune   Esc back");
    }

    fn handle_key(&mut self, key: &str, ctx: &mut ScreenCtx) -> ScreenResult {
        let Some(pfx) = ctx.postfx.as_deref_mut() else {
            return ScreenResult::Pop;
        };
        let n = pfx.passes().len();
        if n == 0 {
            return match key {
                "Esc" | "Backspace" => ScreenResult::Pop,
                _ => ScreenResult::Continue,
            };
        }
        match key {
            "Esc" | "Backspace" => ScreenResult::Pop,
            "Up" => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                ScreenResult::Continue
            }
            "Down" => {
                if self.cursor + 1 < n {
                    self.cursor += 1;
                }
                ScreenResult::Continue
            }
            "Space" | "Enter" | "NumpadEnter" => {
                pfx.toggle(self.cursor);
                if let Err(e) = pfx.save_state(ctx.state_dir) {
                    tracing::warn!("save postfx.toml: {e}");
                }
                ScreenResult::Continue
            }
            "Right" | "Tab" => ScreenResult::Push(Box::new(PostFxParamScreen::new(self.cursor))),
            _ => ScreenResult::Continue,
        }
    }
}

/// Per-pass param drawer. Up/Down picks a param; Left/Right nudges its
/// value by 2% of `(max - min)`; `r` resets to the default.
pub struct PostFxParamScreen {
    pass_idx: usize,
    cursor: usize,
}

impl PostFxParamScreen {
    pub fn new(pass_idx: usize) -> Self {
        Self {
            pass_idx,
            cursor: 0,
        }
    }
}

impl Screen for PostFxParamScreen {
    fn render(&self, g: &mut TextScreen, ctx: &RenderCtx) {
        let title = ctx
            .postfx
            .and_then(|p| p.passes().get(self.pass_idx))
            .map(|p| {
                p.meta
                    .display_name
                    .clone()
                    .unwrap_or_else(|| p.name.clone())
            })
            .unwrap_or_else(|| "POST-FX PARAM".to_string());
        draw_border(g, &format!("POST-FX · {title}"));
        let Some(pfx) = ctx.postfx else {
            g.write(4, 3, ATTR_DIM, "(post-fx unavailable)");
            draw_footer(g, "Esc back");
            return;
        };
        let Some(p) = pfx.passes().get(self.pass_idx) else {
            g.write(4, 3, ATTR_DIM, "(pass missing)");
            draw_footer(g, "Esc back");
            return;
        };
        let defs = p.params.defs();
        if defs.is_empty() {
            g.write(4, 3, ATTR_DIM, "This pass has no params.");
            draw_footer(g, "Esc back");
            return;
        }
        let start_row = 4;
        for (i, d) in defs.iter().enumerate() {
            let row = start_row + i * 2;
            if row > 22 {
                break;
            }
            let is_cursor = self.cursor == i;
            let attr = if is_cursor { ATTR_INVERSE } else { ATTR_NORMAL };
            let marker = if is_cursor { '>' } else { ' ' };
            g.set(row, 3, Cell::new(marker, ATTR_BRIGHT));
            let name = truncate(&d.name, 14);
            g.write(row, 5, attr, &format!("{name:<14}"));
            let v = p.params.get(&d.name).unwrap_or(d.default);
            let frac = ((v - d.min) / (d.max - d.min)).clamp(0.0, 1.0);
            let bar_w = 28usize;
            let filled = (frac * bar_w as f32).round() as usize;
            g.set(row, 22, Cell::new('[', ATTR_DIM));
            for c in 0..bar_w {
                let ch = if c < filled { '█' } else { '·' };
                let a = if c < filled { ATTR_BRIGHT } else { ATTR_DIM };
                g.set(row, 23 + c, Cell::new(ch, a));
            }
            g.set(row, 23 + bar_w, Cell::new(']', ATTR_DIM));
            g.write(row, 23 + bar_w + 2, ATTR_NORMAL, &format_val(v));
        }
        draw_footer(g, "^v param   <> adjust   r reset   Esc back");
    }

    fn handle_key(&mut self, key: &str, ctx: &mut ScreenCtx) -> ScreenResult {
        let Some(pfx) = ctx.postfx.as_deref_mut() else {
            return ScreenResult::Pop;
        };
        let defs: Vec<crate::scene::ParamDef> = pfx
            .passes()
            .get(self.pass_idx)
            .map(|p| p.params.defs().to_vec())
            .unwrap_or_default();
        if defs.is_empty() {
            return match key {
                "Esc" | "Backspace" => ScreenResult::Pop,
                _ => ScreenResult::Continue,
            };
        }
        match key {
            "Esc" | "Backspace" => ScreenResult::Pop,
            "Up" => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                ScreenResult::Continue
            }
            "Down" => {
                if self.cursor + 1 < defs.len() {
                    self.cursor += 1;
                }
                ScreenResult::Continue
            }
            "Left" => {
                nudge(pfx, self.pass_idx, &defs[self.cursor], -1.0);
                let _ = pfx.save_state(ctx.state_dir);
                ScreenResult::Continue
            }
            "Right" => {
                nudge(pfx, self.pass_idx, &defs[self.cursor], 1.0);
                let _ = pfx.save_state(ctx.state_dir);
                ScreenResult::Continue
            }
            "r" | "R" => {
                if let Some(pm) = pfx.pass_params_mut(self.pass_idx) {
                    let d = &defs[self.cursor];
                    pm.set(&d.name, d.default);
                }
                let _ = pfx.save_state(ctx.state_dir);
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }
}

fn nudge(
    pfx: &mut crate::render::postfx::PostFx,
    pass_idx: usize,
    d: &crate::scene::ParamDef,
    dir: f32,
) {
    if let Some(pm) = pfx.pass_params_mut(pass_idx) {
        let cur = pm.get(&d.name).unwrap_or(d.default);
        let step = (d.max - d.min) * 0.02;
        let next = (cur + dir * step).clamp(d.min, d.max);
        pm.set(&d.name, next);
    }
}

fn format_val(v: f32) -> String {
    if v.abs() >= 100.0 {
        format!("{v:>5.0}")
    } else if v.abs() >= 10.0 {
        format!("{v:>5.1}")
    } else {
        format!("{v:>5.2}")
    }
}

fn short_val(v: f32) -> String {
    if v.abs() >= 10.0 {
        format!("{v:.1}")
    } else {
        format!("{v:.2}")
    }
}

fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
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
        }
    }

    #[test]
    fn esc_pops() {
        let mut s = PostFxScreen::new();
        let scenes: Vec<String> = vec![];
        let mut b = SlotBindings::default();
        let dir = std::path::PathBuf::from("/tmp");
        let audio = AudioParams::new();
        let r = s.handle_key("Esc", &mut s_ctx(&scenes, &mut b, &dir, &audio));
        assert!(matches!(r, ScreenResult::Pop));
    }

    #[test]
    fn empty_chain_renders_placeholder() {
        let s = PostFxScreen::new();
        let mut g = TextScreen::new();
        let b = SlotBindings::default();
        let scenes: Vec<String> = vec![];
        let audio = AudioParams::new();
        let rctx = RenderCtx {
            scenes: &scenes,
            bindings: &b,
            audio: &audio,
            postfx: None,
        };
        s.render(&mut g, &rctx);
        // The placeholder for "no postfx" starts at row 4 col 3.
        let row4: String = (3..20).map(|c| g.at(4, c).ch).collect();
        assert!(row4.starts_with("(post-fx"));
    }

    #[test]
    fn param_screen_with_missing_pass_renders_placeholder() {
        let s = PostFxParamScreen::new(99);
        let mut g = TextScreen::new();
        let b = SlotBindings::default();
        let scenes: Vec<String> = vec![];
        let audio = AudioParams::new();
        let rctx = RenderCtx {
            scenes: &scenes,
            bindings: &b,
            audio: &audio,
            postfx: None,
        };
        s.render(&mut g, &rctx);
        let row4: String = (3..30).map(|c| g.at(4, c).ch).collect();
        assert!(row4.contains("post-fx") || row4.contains("pass"));
    }
}
