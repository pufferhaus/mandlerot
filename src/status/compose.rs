//! Pure function: `&SharedState → TextScreen`.
//!
//! Layout matches the spec's 80×26 grid. Functions are split by row group so
//! each is small enough to follow in isolation.

use crate::scene::SceneLibrary;
use crate::state::{Layer, Mode, SharedState};

use super::grid::{Cell, TextScreen, ATTR_BRIGHT, ATTR_DIM, ATTR_INVERSE, ATTR_NORMAL};

pub fn state_to_grid(state: &SharedState, _lib: &SceneLibrary) -> TextScreen {
    let mut g = TextScreen::new();
    fill_borders(&mut g);
    write_top_bar(&mut g, state);
    write_layer_headers(&mut g, state);
    write_layer_params(&mut g, state);
    write_xfade(&mut g, state);
    write_audio_presets_last(&mut g, state);
    write_hotkeys(&mut g);
    g
}

/// Outer rectangle border + horizontal row dividers at fixed rows.
fn fill_borders(g: &mut TextScreen) {
    // Row 0  top border
    g.fill(0, 0, 80, '─', ATTR_DIM);
    g.set(0, 0, Cell::new('┌', ATTR_DIM));
    g.set(0, 79, Cell::new('┐', ATTR_DIM));
    // Row 25 bottom border
    g.fill(25, 0, 80, '─', ATTR_DIM);
    g.set(25, 0, Cell::new('└', ATTR_DIM));
    g.set(25, 79, Cell::new('┘', ATTR_DIM));
    // Vertical sides on every interior row
    for r in 1..25 {
        g.set(r, 0, Cell::new('│', ATTR_DIM));
        g.set(r, 79, Cell::new('│', ATTR_DIM));
    }
    // Layer row separator at column 40 for the 8 param rows (rows 2..10)
    for r in 2..10 {
        g.set(r, 40, Cell::new('│', ATTR_DIM));
    }
    // Audio/Presets/Last separators (rows 13..17)
    for r in 13..17 {
        g.set(r, 11, Cell::new('│', ATTR_DIM));
        g.set(r, 46, Cell::new('│', ATTR_DIM));
    }
}

fn write_top_bar(g: &mut TextScreen, state: &SharedState) {
    let mode_str = match state.active_mode {
        Mode::Scene => "SCENE",
        Mode::Param => "PARAM",
        Mode::Preset => "PRESET",
    };
    let layer_str = match state.active_layer {
        Layer::A => "A",
        Layer::B => "B",
    };
    let blend_str = match state.blend_mode {
        crate::state::BlendMode::Mix => "mix",
        crate::state::BlendMode::Add => "add",
        crate::state::BlendMode::Multiply => "mult",
        crate::state::BlendMode::Screen => "screen",
        crate::state::BlendMode::Difference => "diff",
    };
    let aud = if state.audio_bypass { "OFF" } else { "ON" };
    g.write(0, 3, ATTR_BRIGHT, "MODE:");
    g.write(0, 8, ATTR_INVERSE, mode_str);
    g.write(0, 14, ATTR_BRIGHT, "LYR:");
    g.write(0, 18, ATTR_INVERSE, layer_str);
    g.write(0, 21, ATTR_NORMAL, "BLEND:");
    g.write(0, 27, ATTR_NORMAL, blend_str);
    let bpm = format!("BPM:{:>3.0}", state.tap_tempo_bpm);
    g.write(0, 60, ATTR_BRIGHT, &bpm);
    let aud_lbl = format!("AUD:{}", aud);
    g.write(0, 70, ATTR_BRIGHT, &aud_lbl);
}

fn write_layer_headers(g: &mut TextScreen, state: &SharedState) {
    g.fill(1, 0, 80, '─', ATTR_DIM);
    g.set(1, 0, Cell::new('├', ATTR_DIM));
    g.set(1, 40, Cell::new('┬', ATTR_DIM));
    g.set(1, 79, Cell::new('┤', ATTR_DIM));
    g.write(1, 3, ATTR_BRIGHT, "A:");
    g.write(1, 6, ATTR_NORMAL, &truncate(&state.layer_a.scene_name, 14));
    g.write(1, 43, ATTR_BRIGHT, "B:");
    g.write(1, 46, ATTR_NORMAL, &truncate(&state.layer_b.scene_name, 14));
}

fn write_layer_params(g: &mut TextScreen, state: &SharedState) {
    for slot in 0..8 {
        let row = 2 + slot as usize;
        write_one_param_row(
            g,
            row,
            &state.layer_a,
            slot,
            state.selected_param_a == slot,
            state.active_layer == Layer::A,
            0,
        );
        write_one_param_row(
            g,
            row,
            &state.layer_b,
            slot,
            state.selected_param_b == slot,
            state.active_layer == Layer::B,
            41,
        );
    }
}

fn write_one_param_row(
    g: &mut TextScreen,
    row: usize,
    layer: &crate::state::LayerState,
    slot: u8,
    is_selected: bool,
    layer_active: bool,
    col_off: usize,
) {
    let def = layer.params.defs().iter().find(|d| d.slot == slot);
    let cursor = if is_selected && layer_active {
        '>'
    } else {
        ' '
    };
    g.set(row, col_off + 1, Cell::new(' ', ATTR_NORMAL));
    g.set(row, col_off + 2, Cell::new(cursor, ATTR_BRIGHT));
    let slot_ch = std::char::from_digit(slot as u32, 10).unwrap_or('?');
    g.set(row, col_off + 3, Cell::new(slot_ch, ATTR_NORMAL));

    let (name, route, base, val) = if let Some(d) = def {
        let v = layer.params.get(&d.name).unwrap_or(d.default);
        let r = match d.audio_route {
            crate::scene::AudioRoute::None => "..",
            crate::scene::AudioRoute::Bass => "Bs",
            crate::scene::AudioRoute::Lomid => "Lo",
            crate::scene::AudioRoute::Himid => "Hi",
            crate::scene::AudioRoute::Treble => "Tr",
            crate::scene::AudioRoute::Beat => "Bt",
        };
        (d.name.clone(), r, d.min, v)
    } else {
        ("--".to_string(), "..", 0.0, 0.0)
    };
    let name_trunc = truncate(&name, 8);
    g.write(row, col_off + 5, ATTR_NORMAL, &format!("{:<8}", name_trunc));
    g.write(row, col_off + 14, ATTR_DIM, route);

    g.set(row, col_off + 17, Cell::new('[', ATTR_DIM));
    g.set(row, col_off + 32, Cell::new(']', ATTR_DIM));
    if def.is_some() {
        let frac = if let Some(d) = def {
            ((val - d.min) / (d.max - d.min)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let pos = (frac * 13.0).round() as usize;
        let bar_attr = if is_selected && layer_active {
            ATTR_BRIGHT
        } else {
            ATTR_NORMAL
        };
        g.set(row, col_off + 18 + pos, Cell::new('█', bar_attr));
    }
    let val_str = format_value(val, base);
    g.write(row, col_off + 34, ATTR_NORMAL, &format!("{:>5}", val_str));
}

fn format_value(v: f32, _min: f32) -> String {
    if v.abs() >= 100.0 {
        format!("{:>5.0}", v)
    } else if v.abs() >= 10.0 {
        format!("{:>5.1}", v)
    } else {
        format!("{:>5.2}", v)
    }
}

fn write_xfade(g: &mut TextScreen, state: &SharedState) {
    g.fill(10, 0, 80, '─', ATTR_DIM);
    g.set(10, 0, Cell::new('├', ATTR_DIM));
    g.set(10, 40, Cell::new('┴', ATTR_DIM));
    g.set(10, 79, Cell::new('┤', ATTR_DIM));
    g.write(10, 3, ATTR_BRIGHT, "XFADE");
    g.write(10, 60, ATTR_NORMAL, &format!("{:.2}", state.xfade));

    // Row 11 = the bar
    g.set(11, 1, Cell::new('A', ATTR_BRIGHT));
    g.set(11, 2, Cell::new('├', ATTR_DIM));
    g.set(11, 60, Cell::new('┤', ATTR_DIM));
    g.set(11, 61, Cell::new('B', ATTR_BRIGHT));
    g.fill(11, 3, 60, '─', ATTR_DIM);
    let pos = 3 + (state.xfade.clamp(0.0, 1.0) * 57.0).round() as usize;
    g.set(11, pos.min(59), Cell::new('█', ATTR_BRIGHT));
}

fn write_audio_presets_last(g: &mut TextScreen, state: &SharedState) {
    g.fill(12, 0, 80, '─', ATTR_DIM);
    g.set(12, 0, Cell::new('├', ATTR_DIM));
    g.set(12, 11, Cell::new('┬', ATTR_DIM));
    g.set(12, 46, Cell::new('┬', ATTR_DIM));
    g.set(12, 79, Cell::new('┤', ATTR_DIM));
    g.write(12, 3, ATTR_BRIGHT, "AUDIO");
    g.write(12, 14, ATTR_BRIGHT, "PRESETS");
    g.write(12, 49, ATTR_BRIGHT, "LAST");

    let labels = ["Bs", "Lo", "Hi", "Tr"];
    for (i, label) in labels.iter().enumerate() {
        let row = 13 + i;
        g.write(row, 2, ATTR_NORMAL, label);
        let v = state.audio_bands[i].clamp(0.0, 1.0);
        let bars = (v * 6.0).round() as usize;
        for c in 0..bars.min(6) {
            g.set(row, 5 + c, Cell::new('█', ATTR_BRIGHT));
        }
    }

    // Presets row 13 — 8 cells
    let mut col = 13;
    for slot in 1..=8 {
        let active = state.active_preset_slot == Some(slot);
        let attr = if active { ATTR_INVERSE } else { ATTR_DIM };
        g.set(13, col, Cell::new('[', attr));
        g.set(
            13,
            col + 1,
            Cell::new(std::char::from_digit(slot as u32, 10).unwrap(), attr),
        );
        g.set(13, col + 2, Cell::new(']', attr));
        col += 4;
    }

    // Last action footer
    let last = truncate(&state.last_action_label, 30);
    g.write(13, 48, ATTR_NORMAL, &last);
}

fn write_hotkeys(g: &mut TextScreen) {
    g.fill(20, 0, 80, '─', ATTR_DIM);
    g.set(20, 0, Cell::new('├', ATTR_DIM));
    g.set(20, 11, Cell::new('┴', ATTR_DIM));
    g.set(20, 46, Cell::new('┴', ATTR_DIM));
    g.set(20, 79, Cell::new('┤', ATTR_DIM));
    g.write(20, 3, ATTR_BRIGHT, "HOTKEYS");
    g.write(
        21,
        2,
        ATTR_NORMAL,
        "Tab mode  Shift other-lyr  1-9 slot  -/= xfade/param  N trig  M blend",
    );
    g.write(
        22,
        2,
        ATTR_NORMAL,
        "Esc PANIC  G audio-byp  L/Spc tap  F1 ovl  F2/F3 next  F5 reload-all",
    );
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        s[..max].to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{LoadedScene, SceneLibrary, SceneMeta};
    use crate::state::{BlendMode, SharedState};

    fn lib() -> SceneLibrary {
        let mut lib = SceneLibrary::default();
        let meta = SceneMeta::parse(
            "name = \"plasma\"\n[[params]]\nslot = 0\nname = \"hue\"\nmin = 0.0\nmax = 1.0\ndefault = 0.5\n",
            "x",
        )
        .unwrap();
        lib.upsert(
            "plasma",
            LoadedScene {
                meta,
                fragment_body: "void main() {}".into(),
                source_path: std::path::PathBuf::from("inline"),
            },
        );
        let meta2 = SceneMeta::parse(
            "name = \"solid\"\n[[params]]\nslot = 0\nname = \"red\"\nmin = 0.0\nmax = 1.0\ndefault = 1.0\n",
            "x",
        )
        .unwrap();
        lib.upsert(
            "solid",
            LoadedScene {
                meta: meta2,
                fragment_body: "void main() {}".into(),
                source_path: std::path::PathBuf::from("inline"),
            },
        );
        lib
    }

    fn state() -> SharedState {
        SharedState::from_initial(&lib(), "plasma", "solid", 0.0, BlendMode::Mix).unwrap()
    }

    #[test]
    fn corners_are_correct() {
        let g = state_to_grid(&state(), &lib());
        assert_eq!(g.at(0, 0).ch, '┌');
        assert_eq!(g.at(0, 79).ch, '┐');
        assert_eq!(g.at(25, 0).ch, '└');
        assert_eq!(g.at(25, 79).ch, '┘');
    }

    #[test]
    fn mode_label_uses_inverse() {
        let g = state_to_grid(&state(), &lib());
        // "SCENE" starts at col 8
        assert_eq!(g.at(0, 8).ch, 'S');
        assert!(g.at(0, 8).attr & ATTR_INVERSE != 0);
    }

    #[test]
    fn layer_a_scene_name_appears_at_row_1() {
        let g = state_to_grid(&state(), &lib());
        // "plasma" in cells 6..12
        let s: String = (6..12).map(|c| g.at(1, c).ch).collect();
        assert_eq!(s, "plasma");
    }

    #[test]
    fn vertical_separator_at_col_40_in_param_rows() {
        let g = state_to_grid(&state(), &lib());
        for row in 2..10 {
            assert_eq!(g.at(row, 40).ch, '│');
        }
    }

    #[test]
    fn xfade_marker_at_left_when_xfade_zero() {
        let g = state_to_grid(&state(), &lib());
        // xfade = 0.0 → bar position = col 3
        assert_eq!(g.at(11, 3).ch, '█');
    }
}
