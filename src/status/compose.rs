//! Pure function: `&PanelSnapshot → TextScreen`.
//!
//! Layout matches the spec's 80×26 grid. Functions are split by row group so
//! each is small enough to follow in isolation. The input is a lean
//! `PanelSnapshot` (see `crate::status::snapshot`) rather than a full
//! `SharedState` clone — this drops 18+ String allocations per frame on
//! the cross-thread transport.

use crate::state::{Layer, Mode};
use crate::status::snapshot::{LayerSnapshot, PanelSnapshot};
use crate::status::sysmon::SysMon;

use super::grid::{Cell, TextScreen, ATTR_BRIGHT, ATTR_DIM, ATTR_INVERSE, ATTR_NORMAL};

pub fn state_to_grid(
    snap: &PanelSnapshot,
    postfx_summary: &str,
    sysmon: &SysMon,
    fps: Option<f32>,
) -> TextScreen {
    let mut g = TextScreen::new();
    fill_borders(&mut g);
    write_top_bar(&mut g, snap, postfx_summary);
    write_layer_headers(&mut g, snap);
    write_layer_params(&mut g, snap);
    write_xfade(&mut g, snap);
    write_audio_looks_last(&mut g, snap);
    write_hotkeys(&mut g);
    write_sysmon(&mut g, sysmon);
    write_fps(&mut g, fps);
    apply_active_layer_invert(&mut g, snap);
    g
}

/// Highlight the active layer by inverting just the header row of its
/// card. Param rows stay normal so the readout still reads cleanly. Inside
/// the inverted band, ATTR_DIM is stripped — a brown bg muddied the look,
/// especially under the row of `─` filler chars between the layer label
/// and the scene name. Stripping dim makes that band a clean amber bar.
fn apply_active_layer_invert(g: &mut TextScreen, snap: &PanelSnapshot) {
    let (col_lo, col_hi_excl) = match snap.layer {
        Layer::A => (1usize, 40usize),
        Layer::B => (41usize, 79usize),
    };
    for col in col_lo..col_hi_excl {
        let mut c = g.at(1, col);
        c.attr ^= ATTR_INVERSE;
        c.attr &= !ATTR_DIM;
        g.set(1, col, c);
    }
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
    // Layer row separator at column 40 for the 9 param rows (rows 2..11).
    for r in 2..11 {
        g.set(r, 40, Cell::new('│', ATTR_DIM));
    }
    // Audio/Looks 50/50 split — single vertical separator at col 40 from
    // just below the section divider down to just above the hotkeys
    // divider, matching the layer A/B split above.
    for r in 16..21 {
        g.set(r, 40, Cell::new('│', ATTR_DIM));
    }
}

fn write_top_bar(g: &mut TextScreen, snap: &PanelSnapshot, postfx_summary: &str) {
    let mode_str = match snap.mode {
        Mode::Scene => "SCENE",
        Mode::Param => "PARAM",
        Mode::Look => "LOOK",
    };
    let blend_str = match snap.blend_mode {
        crate::state::BlendMode::Mix => "mix",
        crate::state::BlendMode::Add => "add",
        crate::state::BlendMode::Multiply => "mult",
        crate::state::BlendMode::Screen => "screen",
        crate::state::BlendMode::Difference => "diff",
        crate::state::BlendMode::Overlay => "overly",
        crate::state::BlendMode::HardLight => "hardlt",
        crate::state::BlendMode::Lighten => "lgt",
        crate::state::BlendMode::Darken => "drk",
        crate::state::BlendMode::Exclusion => "excl",
        crate::state::BlendMode::Subtract => "sub",
        crate::state::BlendMode::LinearBurn => "linbn",
        crate::state::BlendMode::SoftLight => "softlt",
        crate::state::BlendMode::ColorDodge => "coldg",
        crate::state::BlendMode::ColorBurn => "colbn",
        crate::state::BlendMode::Hue => "hue",
        crate::state::BlendMode::Saturation => "sat",
        crate::state::BlendMode::Color => "color",
        crate::state::BlendMode::Luminosity => "lumin",
    };
    let aud = if snap.audio_bypass { "OFF" } else { "ON" };
    // Uniform bright text across the top bar — labels and values share one
    // attribute so the row reads as a single colour band. The active layer
    // is already conveyed by the inverted header row of the A/B card, so
    // the redundant "LYR:" indicator is gone.
    g.write(0, 3, ATTR_BRIGHT, "MODE:");
    g.write(0, 8, ATTR_BRIGHT, mode_str);
    g.write(0, 15, ATTR_BRIGHT, "BLEND:");
    g.write(0, 21, ATTR_BRIGHT, blend_str);
    g.write(0, 30, ATTR_BRIGHT, "POST:");
    let post = if postfx_summary.is_empty() {
        "off".to_string()
    } else {
        postfx_summary.to_string()
    };
    // Cap the value to 11 chars (cols 35..=45) so the `VID:xx` chip can
    // land at col 47 with 3 cols of breathing room before BPM at col 60.
    let post_trim: String = post.chars().take(11).collect();
    g.write(0, 35, ATTR_BRIGHT, &post_trim);
    // 6-char video-capture status chip: `VID:--` / `VID:OK` / `VID:ST` /
    // `VID:ER`. Ends at col 52, leaving cols 53..59 blank before BPM.
    g.write(0, 47, ATTR_BRIGHT, snap.video_status.as_chip());
    let bpm = format!("BPM:{:>3.0}", snap.bpm);
    g.write(0, 60, ATTR_BRIGHT, &bpm);
    let aud_lbl = format!("AUD:{}", aud);
    g.write(0, 70, ATTR_BRIGHT, &aud_lbl);
}

fn write_layer_headers(g: &mut TextScreen, snap: &PanelSnapshot) {
    g.fill(1, 0, 80, '─', ATTR_DIM);
    g.set(1, 0, Cell::new('├', ATTR_DIM));
    g.set(1, 40, Cell::new('┬', ATTR_DIM));
    g.set(1, 79, Cell::new('┤', ATTR_DIM));
    g.write(1, 3, ATTR_BRIGHT, "A:");
    g.write(1, 6, ATTR_NORMAL, &truncate(&snap.layer_a.scene_name, 14));
    g.write(1, 43, ATTR_BRIGHT, "B:");
    g.write(1, 46, ATTR_NORMAL, &truncate(&snap.layer_b.scene_name, 14));
}

fn write_layer_params(g: &mut TextScreen, snap: &PanelSnapshot) {
    for slot in 0..9 {
        let row = 2 + slot as usize;
        write_one_param_row(
            g,
            row,
            &snap.layer_a,
            slot,
            snap.selected_a == slot,
            snap.layer == Layer::A,
            0,
        );
        write_one_param_row(
            g,
            row,
            &snap.layer_b,
            slot,
            snap.selected_b == slot,
            snap.layer == Layer::B,
            41,
        );
    }
}

fn write_one_param_row(
    g: &mut TextScreen,
    row: usize,
    layer: &LayerSnapshot,
    slot: u8,
    is_selected: bool,
    layer_active: bool,
    col_off: usize,
) {
    let param = &layer.params[slot as usize];
    // Display 1-based to match the keymap (key `1` selects slot index 0).
    let slot_ch = std::char::from_digit((slot + 1) as u32, 10).unwrap_or('?');
    g.set(row, col_off + 1, Cell::new(slot_ch, ATTR_NORMAL));

    let (name, route, val) = if param.present {
        let r = match param.route {
            crate::scene::AudioRoute::None => "..",
            crate::scene::AudioRoute::Bass => "Bs",
            crate::scene::AudioRoute::Lomid => "Lo",
            crate::scene::AudioRoute::Himid => "Hi",
            crate::scene::AudioRoute::Treble => "Tr",
            crate::scene::AudioRoute::Beat => "Bt",
            crate::scene::AudioRoute::Mid => "Md",
        };
        (param.name(), r, param.value)
    } else {
        ("--", "..", 0.0)
    };
    // Name is already ≤8 bytes (truncation happens at snapshot build time),
    // so we just left-pad to the column width here.
    let name_attr = if is_selected && layer_active {
        ATTR_INVERSE
    } else {
        ATTR_NORMAL
    };
    g.write(row, col_off + 3, name_attr, &format!("{:<8}", name));
    g.write(row, col_off + 12, ATTR_DIM, route);

    g.set(row, col_off + 15, Cell::new('[', ATTR_DIM));
    g.set(row, col_off + 30, Cell::new(']', ATTR_DIM));
    if param.present {
        let span = (param.max - param.min).max(1e-6);
        let frac = ((val - param.min) / span).clamp(0.0, 1.0);
        let pos = (frac * 13.0).round() as usize;
        let bar_attr = if is_selected && layer_active {
            ATTR_BRIGHT
        } else {
            ATTR_NORMAL
        };
        g.set(row, col_off + 16 + pos, Cell::new('█', bar_attr));
    }
    let val_str = format_value(val);
    g.write(row, col_off + 32, ATTR_NORMAL, &format!("{:>5}", val_str));
}

fn format_value(v: f32) -> String {
    if v.abs() >= 100.0 {
        format!("{:>5.0}", v)
    } else if v.abs() >= 10.0 {
        format!("{:>5.1}", v)
    } else {
        format!("{:>5.2}", v)
    }
}

fn write_xfade(g: &mut TextScreen, snap: &PanelSnapshot) {
    g.fill(11, 0, 80, '─', ATTR_DIM);
    g.set(11, 0, Cell::new('├', ATTR_DIM));
    g.set(11, 40, Cell::new('┴', ATTR_DIM));
    g.set(11, 79, Cell::new('┤', ATTR_DIM));
    g.write(11, 3, ATTR_BRIGHT, "XFADE");
    g.write(11, 70, ATTR_NORMAL, &format!("{:.2}", snap.xfade));

    // Bar spans rows 12-14 (3 rows tall) but only the middle row carries
    // the A───B track. Rows 12 and 14 show just the marker column, giving
    // a cross / plus-sign silhouette:
    //
    //         █
    //   A─────█─────B
    //         █
    //
    // Bar fill runs cols 2..78 (76 cells, indices 2..=77) so the marker
    // can reach all the way to the B label on max xfade.
    let bar_lo = 2usize;
    let bar_hi = 77usize;
    let span = (bar_hi - bar_lo) as f32;
    let pos = bar_lo + (snap.xfade.clamp(0.0, 1.0) * span).round() as usize;
    let pos = pos.min(bar_hi);
    // Middle row: full track with labels.
    g.set(13, 1, Cell::new('A', ATTR_BRIGHT));
    g.fill(13, bar_lo, bar_hi - bar_lo + 1, '─', ATTR_DIM);
    g.set(13, 78, Cell::new('B', ATTR_BRIGHT));
    g.set(13, pos, Cell::new('█', ATTR_BRIGHT));
    // Outer rows: just the marker column, so the bar reads as a cross.
    g.set(12, pos, Cell::new('█', ATTR_BRIGHT));
    g.set(14, pos, Cell::new('█', ATTR_BRIGHT));
}

fn write_audio_looks_last(g: &mut TextScreen, snap: &PanelSnapshot) {
    // Header divider at row 15, content rows 16..20 (5 audio bands now),
    // hotkeys divider at row 21 below.
    g.fill(15, 0, 80, '─', ATTR_DIM);
    g.set(15, 0, Cell::new('├', ATTR_DIM));
    g.set(15, 40, Cell::new('┬', ATTR_DIM));
    g.set(15, 79, Cell::new('┤', ATTR_DIM));
    g.write(15, 3, ATTR_BRIGHT, "AUDIO");
    g.write(15, 43, ATTR_BRIGHT, "LOOKS");

    // Audio bands: 5 rows tall (Bs / Lo / MD / Hi / Tr). `MD` is a
    // UI-only roll-up of the existing lo-mid + hi-mid bands — gives the
    // operator a single "mids" reading without requiring a 5th band in
    // the audio thread / shader interface. Each bar is wrapped in `[ ]`
    // with `·` dots filling the empty portion so the operator can read
    // the bar's saturation at a glance:
    //
    //   `Bs [████████····················]`
    //
    // Layout per row:
    //   col 2-3   : 2-char label
    //   col 4     : `[`
    //   col 5..35 : 30-cell bar (`█` = filled, `·` = empty)
    //   col 35    : `]`
    const BAR_W: usize = 30;
    // Logical frequency order on screen: bass → lo-mid → mid → hi-mid →
    // treble. Internal storage keeps `mid` at index 4 (see
    // `audio::bands::BAND_MID`) so legacy `u_audio.xyzw` mapping stays
    // intact for existing scenes.
    let bs = snap.audio_bands[0];
    let lo = snap.audio_bands[1];
    let hi = snap.audio_bands[2];
    let tr = snap.audio_bands[3];
    let md = snap.audio_bands[4];
    let bars: [(&str, f32); 5] =
        [("Bs", bs), ("Lo", lo), ("MD", md), ("Hi", hi), ("Tr", tr)];
    for (i, (label, v)) in bars.iter().enumerate() {
        let row = 16 + i;
        g.write(row, 2, ATTR_NORMAL, label);
        g.set(row, 4, Cell::new('[', ATTR_DIM));
        let frac = v.clamp(0.0, 1.0);
        let filled = (frac * BAR_W as f32).round() as usize;
        for c in 0..BAR_W {
            let (ch, attr) = if c < filled {
                ('█', ATTR_BRIGHT)
            } else {
                ('·', ATTR_DIM)
            };
            g.set(row, 5 + c, Cell::new(ch, attr));
        }
        g.set(row, 5 + BAR_W, Cell::new(']', ATTR_DIM));
    }

    // Looks slots — 8 cells, centred in the right half. Each cell is `[N]`
    // (3 chars) with 1 space, so 8 slots fit in 31 cols. Place starting at
    // col 44 (1-cell margin past the separator + "LOOKS" label region).
    let mut col = 44;
    for slot in 1..=8 {
        let active = snap.active_look_slot == Some(slot);
        let attr = if active { ATTR_INVERSE } else { ATTR_DIM };
        g.set(16, col, Cell::new('[', attr));
        g.set(
            16,
            col + 1,
            Cell::new(std::char::from_digit(slot as u32, 10).unwrap(), attr),
        );
        g.set(16, col + 2, Cell::new(']', attr));
        if active && snap.look_postfx_bound {
            g.set(16, col + 3, Cell::new('*', ATTR_BRIGHT));
        }
        col += 4;
    }
}

fn write_hotkeys(g: &mut TextScreen) {
    // Hotkeys block shifted down 1 row from the old position to absorb the
    // empty row that used to sit below it; the freed row above went into the
    // taller XFADE bar.
    g.fill(21, 0, 80, '─', ATTR_DIM);
    g.set(21, 0, Cell::new('├', ATTR_DIM));
    g.set(21, 40, Cell::new('┴', ATTR_DIM));
    g.set(21, 79, Cell::new('┤', ATTR_DIM));
    g.write(21, 3, ATTR_BRIGHT, "HOTKEYS");
    g.write(
        22,
        2,
        ATTR_NORMAL,
        "Tab mode  Shift other-lyr  1-9 slot  -/= xfade/param  N trig  M blend",
    );
    g.write(
        23,
        2,
        ATTR_NORMAL,
        "Esc PANIC  G audio-byp  L/Spc tap  F1 ovl  F2/F3 next  Shift-/= aud-rt",
    );
}

/// Bottom-right system stats embedded in the bottom border row (25).
/// Width is hard-fixed at 20 cells — `CPU NNN% MEM NNN% NNNC`. The
/// numeric fields use **integer** formatting (`{:>3}`) over a clamped
/// `u32`, which is byte-stable in a way that float `{:.0}` is not: no
/// rounding edge cases, no precision-driven width drift, no NaN
/// surprises. The visible bug this fixes is the line shifting around
/// (and visually "wrapping") between samples when a previous run wrote
/// a wider value to a cell the new line doesn't cover.
///
///   `CPU  5% MEM 24% 52C`
///
/// Sampled at 1 Hz by the worker (see `crate::status::sysmon::SysMon`).
/// Each field falls back to `---` when its source is missing.
fn write_sysmon(g: &mut TextScreen, sysmon: &SysMon) {
    // Convert each Option<f32> → 3-char field string of fixed width.
    // `clamp(0.0, 999.0) as u32` is finite and ≤ 999 by construction, so
    // `{:>3}` always emits exactly 3 chars (`  5`, ` 50`, `100`, `999`).
    let cpu = match sysmon.cpu_pct {
        Some(v) => format!("CPU{:>3}%", v.clamp(0.0, 999.0) as u32),
        None => "CPU---%".to_string(),
    };
    let mem = match sysmon.mem_pct {
        Some(v) => format!("MEM{:>3}%", v.clamp(0.0, 999.0) as u32),
        None => "MEM---%".to_string(),
    };
    let temp = match sysmon.temp_c {
        Some(v) => format!("{:>3}C", v.clamp(0.0, 999.0) as u32),
        None => "---C".to_string(),
    };
    // 7 + 1 + 7 + 1 + 4 = 20. Each sub-field is independently width-
    // stable; the whole-line invariant is checked below.
    let line = format!("{} {} {}", cpu, mem, temp);
    debug_assert_eq!(
        line.chars().count(),
        20,
        "sysmon line width drifted: {line:?}"
    );
    // Fixed start column so the line position can't drift even if the
    // width invariant is ever broken in a future refactor.
    const SYSMON_START_COL: usize = 59;
    g.write(25, SYSMON_START_COL, ATTR_NORMAL, &line);
}

/// Bottom-left smoothed render fps on row 25, shares the line with sysmon
/// (sysmon starts at col 59 so we have cols 2..58 free). Format is
/// `FPS NN` — exactly 6 chars to stay width-stable. The smoothed value is
/// produced on the render thread (EMA over the last few frames) and threaded
/// through `StateSnapshot.fps`.
fn write_fps(g: &mut TextScreen, fps: Option<f32>) {
    let line = match fps {
        Some(v) => format!("FPS {:>2}", v.clamp(0.0, 99.0) as u32),
        None => "FPS --".to_string(),
    };
    debug_assert_eq!(
        line.chars().count(),
        6,
        "fps line width drifted: {line:?}"
    );
    g.write(25, 2, ATTR_NORMAL, &line);
}

fn truncate(s: &str, max: usize) -> String {
    // Char-aware: byte-slicing on a UTF-8 boundary mid-codepoint panics.
    // Counting chars also caps visual width better than byte length for
    // multibyte glyphs that may sneak into scene names or action labels.
    s.chars().take(max).collect()
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
        let g = state_to_grid(&PanelSnapshot::from_state(&state()), "", &SysMon::new(), None);
        assert_eq!(g.at(0, 0).ch, '┌');
        assert_eq!(g.at(0, 79).ch, '┐');
        assert_eq!(g.at(25, 0).ch, '└');
        assert_eq!(g.at(25, 79).ch, '┘');
    }

    #[test]
    fn mode_label_is_not_inverted() {
        let g = state_to_grid(&PanelSnapshot::from_state(&state()), "", &SysMon::new(), None);
        // Default-mode start is Param so the label at col 8 is "PARAM".
        // The inverted band was removed — it competed visually with the
        // layer-header invert band below.
        assert_eq!(g.at(0, 8).ch, 'P');
        assert_eq!(g.at(0, 8).attr & ATTR_INVERSE, 0);
        assert!(g.at(0, 8).attr & ATTR_BRIGHT != 0);
    }

    #[test]
    fn post_tag_reads_off_when_summary_empty() {
        let g = state_to_grid(&PanelSnapshot::from_state(&state()), "", &SysMon::new(), None);
        // "POST:" prefix lands at col 30, value starts at col 35.
        let prefix: String = (30..35).map(|c| g.at(0, c).ch).collect();
        assert_eq!(prefix, "POST:");
        let value: String = (35..38).map(|c| g.at(0, c).ch).collect();
        assert_eq!(value, "off");
    }

    #[test]
    fn post_tag_renders_summary_when_set() {
        let g = state_to_grid(&PanelSnapshot::from_state(&state()), "vig+grn", &SysMon::new(), None);
        let value: String = (35..42).map(|c| g.at(0, c).ch).collect();
        assert_eq!(value, "vig+grn");
    }

    #[test]
    fn top_bar_includes_video_chip() {
        // Default snapshot from a SharedState has VideoStatus::NoDevice, so
        // the 6-char chip text at cols 47..53 should read `VID:--`.
        let g = state_to_grid(&PanelSnapshot::from_state(&state()), "", &SysMon::new(), None);
        let chip: String = (47..53).map(|c| g.at(0, c).ch).collect();
        assert_eq!(chip, "VID:--");
    }

    #[test]
    fn top_bar_has_no_lyr_label() {
        let g = state_to_grid(&PanelSnapshot::from_state(&state()), "", &SysMon::new(), None);
        // The whole row must not contain the redundant "LYR:" badge.
        let row: String = (0..80).map(|c| g.at(0, c).ch).collect();
        assert!(!row.contains("LYR:"), "stale LYR label in: {row:?}");
    }

    #[test]
    fn top_bar_labels_and_values_are_all_bright() {
        let g = state_to_grid(&PanelSnapshot::from_state(&state()), "", &SysMon::new(), None);
        // Sample a representative cell from each label/value pair. All must
        // carry ATTR_BRIGHT — this is the contract for the uniform colour
        // scheme on the top bar.
        for col in [3, 8, 15, 21, 30, 35, 60, 70] {
            assert!(
                g.at(0, col).attr & ATTR_BRIGHT != 0,
                "col {col} ({}) missing BRIGHT",
                g.at(0, col).ch
            );
        }
    }

    #[test]
    fn layer_a_scene_name_appears_at_row_1() {
        let g = state_to_grid(&PanelSnapshot::from_state(&state()), "", &SysMon::new(), None);
        // "plasma" in cells 6..12
        let s: String = (6..12).map(|c| g.at(1, c).ch).collect();
        assert_eq!(s, "plasma");
    }

    #[test]
    fn vertical_separator_at_col_40_in_param_rows() {
        let g = state_to_grid(&PanelSnapshot::from_state(&state()), "", &SysMon::new(), None);
        for row in 2..11 {
            assert_eq!(g.at(row, 40).ch, '│');
        }
    }

    #[test]
    fn active_layer_a_inverts_header_left() {
        let g = state_to_grid(&PanelSnapshot::from_state(&state()), "", &SysMon::new(), None);
        // active = A by default → row 1 (header), left side, is inverted
        assert!(g.at(1, 5).attr & ATTR_INVERSE != 0);
        // ... non-selected param rows stay normal (slot 1, row 3)
        assert!(g.at(3, 5).attr & ATTR_INVERSE == 0);
        // ... right side header is not inverted
        assert!(g.at(1, 45).attr & ATTR_INVERSE == 0);
    }

    #[test]
    fn selected_param_name_is_inverted() {
        let g = state_to_grid(&PanelSnapshot::from_state(&state()), "", &SysMon::new(), None);
        // Selected param (slot 0, row 2) on active layer A has its name
        // cells inverted — name starts at col 3 (col_off=0, +3 offset).
        assert!(g.at(2, 4).attr & ATTR_INVERSE != 0);
        // The B-side equivalent isn't selected (layer A active), so its
        // name (row 2, col 41+3=44) stays normal.
        assert!(g.at(2, 44).attr & ATTR_INVERSE == 0);
    }

    #[test]
    fn active_layer_b_inverts_header_right() {
        let mut s = state();
        s.active_layer = Layer::B;
        let g = state_to_grid(&PanelSnapshot::from_state(&s), "", &SysMon::new(), None);
        assert!(g.at(1, 45).attr & ATTR_INVERSE != 0);
        assert!(g.at(1, 5).attr & ATTR_INVERSE == 0);
    }

    #[test]
    fn inverted_band_strips_dim() {
        let g = state_to_grid(&PanelSnapshot::from_state(&state()), "", &SysMon::new(), None);
        // The `─` filler around the active "A:" label was ATTR_DIM; after
        // invert it should no longer have the dim bit set.
        for col in 1..40 {
            assert_eq!(
                g.at(1, col).attr & ATTR_DIM,
                0,
                "col {col} still dim inside inverted band"
            );
        }
    }

    #[test]
    fn xfade_marker_at_left_when_xfade_zero() {
        let g = state_to_grid(&PanelSnapshot::from_state(&state()), "", &SysMon::new(), None);
        // xfade = 0.0 → marker sits at the bar's left edge (col 2, just past
        // the A label) on all 3 bar rows (12..=14 with 9 param rows above).
        assert_eq!(g.at(12, 2).ch, '█');
        assert_eq!(g.at(14, 2).ch, '█');
    }

    #[test]
    fn xfade_marker_reaches_b_label_when_xfade_one() {
        let mut s = state();
        s.xfade = 1.0;
        let g = state_to_grid(&PanelSnapshot::from_state(&s), "", &SysMon::new(), None);
        // Middle row carries A/B labels and the full track. Bar's right
        // edge is col 77, immediately left of the B label at 78.
        assert_eq!(g.at(13, 77).ch, '█');
        assert_eq!(g.at(13, 78).ch, 'B');
        // Marker also appears on outer rows (the cross silhouette).
        assert_eq!(g.at(12, 77).ch, '█');
        assert_eq!(g.at(14, 77).ch, '█');
    }

    #[test]
    fn audio_looks_split_at_col_40() {
        let g = state_to_grid(&PanelSnapshot::from_state(&state()), "", &SysMon::new(), None);
        // Section divider row 15 has the ┬ on col 40.
        assert_eq!(g.at(15, 40).ch, '┬');
        // Audio bands occupy left half (col 2 label) on row 16.
        assert_eq!(g.at(16, 2).ch, 'B');
        // Looks slot cells live in the right half (col 44 = first `[`).
        assert_eq!(g.at(16, 44).ch, '[');
    }

    #[test]
    fn five_audio_bands_with_brackets_render_rows_16_through_20() {
        let g = state_to_grid(&PanelSnapshot::from_state(&state()), "", &SysMon::new(), None);
        // Labels in the expected order.
        let labels = ["Bs", "Lo", "MD", "Hi", "Tr"];
        for (i, label) in labels.iter().enumerate() {
            let row = 16 + i;
            let a = g.at(row, 2).ch;
            let b = g.at(row, 3).ch;
            assert_eq!(
                format!("{a}{b}"),
                *label,
                "row {row} label should be {label}"
            );
            // Open bracket, bar fills with `·` dots when band level is 0,
            // close bracket lands at col 35.
            assert_eq!(g.at(row, 4).ch, '[');
            assert_eq!(g.at(row, 5).ch, '·');
            assert_eq!(g.at(row, 34).ch, '·');
            assert_eq!(g.at(row, 35).ch, ']');
        }
    }

    #[test]
    fn ninth_param_row_renders_at_row_10() {
        // Row 10 is the 9th param slot (slot index 8 = display digit 9).
        let g = state_to_grid(&PanelSnapshot::from_state(&state()), "", &SysMon::new(), None);
        assert_eq!(g.at(10, 1).ch, '9');
    }

    #[test]
    fn sysmon_line_is_exactly_20_chars_wide_in_all_states() {
        // Pin the width invariant the on-panel layout relies on. If a future
        // refactor breaks this (a different format string, a missing clamp),
        // the test fails BEFORE a wandering line ships to the Pi.
        let mut sm = crate::status::sysmon::SysMon::new();
        let snap = PanelSnapshot::from_state(&state());
        // 1) All None → placeholders.
        let g = state_to_grid(&snap, "", &sm, None);
        let line: String = (59..79).map(|c| g.at(25, c).ch).collect();
        assert_eq!(line.chars().count(), 20);
        assert!(line.starts_with("CPU---%"));
        assert!(line.ends_with("---C"));
        // 2) Small (1-digit) values.
        sm.cpu_pct = Some(4.0);
        sm.mem_pct = Some(8.0);
        sm.temp_c = Some(5.0);
        let g = state_to_grid(&snap, "", &sm, None);
        let line: String = (59..79).map(|c| g.at(25, c).ch).collect();
        assert_eq!(line.chars().count(), 20);
        // 3) Triple-digit clamps.
        sm.cpu_pct = Some(123.0);
        sm.mem_pct = Some(800.0);
        sm.temp_c = Some(105.0);
        let g = state_to_grid(&snap, "", &sm, None);
        let line: String = (59..79).map(|c| g.at(25, c).ch).collect();
        assert_eq!(line.chars().count(), 20);
        // 4) Out-of-range / NaN → clamped to 999 / 0.
        sm.cpu_pct = Some(f32::NAN);
        sm.mem_pct = Some(f32::INFINITY);
        sm.temp_c = Some(-50.0);
        let g = state_to_grid(&snap, "", &sm, None);
        let line: String = (59..79).map(|c| g.at(25, c).ch).collect();
        assert_eq!(line.chars().count(), 20);
    }

    #[test]
    fn bound_look_renders_star_suffix() {
        let mut snap = PanelSnapshot::from_state(&state());
        snap.active_look_slot = Some(3);
        snap.look_postfx_bound = true;
        let g = state_to_grid(&snap, "", &SysMon::new(), None);
        // Slot 3 lives at base col 44 + (3-1)*4 = 52; cells [3] at 52..=54,
        // suffix '*' at col 55.
        assert_eq!(g.at(16, 53).ch, '3');
        assert_eq!(g.at(16, 55).ch, '*');
        assert!(g.at(16, 55).attr & ATTR_BRIGHT != 0);
    }

    #[test]
    fn unbound_look_renders_no_star() {
        let mut snap = PanelSnapshot::from_state(&state());
        snap.active_look_slot = Some(3);
        snap.look_postfx_bound = false;
        let g = state_to_grid(&snap, "", &SysMon::new(), None);
        // Gap cell at col 55 stays blank when not bound.
        assert_ne!(g.at(16, 55).ch, '*');
    }
}
