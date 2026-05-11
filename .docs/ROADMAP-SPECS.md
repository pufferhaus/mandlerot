# mandleROT Roadmap Specs

Full feature specifications. One section per numbered item in
`ROADMAP.md::Execution Order`. The roadmap index stays compact; details
live here.

---

## 2. Double-tap Esc → Panic from inside menu

**Problem.** Menu screens swallow `Esc` (pop). With a menu open the user
has no way to fire `Action::Panic` without first closing the menu manually.

**Design.**

- A 400ms double-tap detector at the top of the main-loop input dispatch.
- Track `last_esc_at: Option<Instant>` next to `ui_stack` in `main.rs`.
- When an `Esc` (or `Backspace`) is observed:
  - If `last_esc_at` set within 400ms → fire `Action::Panic` *and* call
    `ui_stack.close_all()`. Clear `last_esc_at`.
  - Else stamp `last_esc_at` and route to stack-first / keymap as today.
- Single Esc still pops one screen (menu open) or fires Panic (menu closed)
  — that path is unchanged.

**Test plan.** Two synthetic Esc events 200ms apart with menu open →
state ends up at SAFE_SCENE, stack empty. Two events 600ms apart with menu
open → first pops, second pops again (or panic if stack empty).

---

## 3. Active-layer: solid inverted half

**Problem.** With both layer headers + param rows side-by-side, the
selected layer is only signalled by the `>` cursor in one of two columns.
Easy to miss at a glance.

**Design.**

- After `state_to_grid` paints normally, walk the active layer's column
  range and OR `ATTR_INVERSE` into every cell.
- Active = `Layer::A` → cols `1..=39` rows `1..=10` (header + 8 param rows
  + xfade label row stops at 9). Skip the vertical separator at col 40.
- Active = `Layer::B` → cols `41..=78`.
- Keep the `>` cursor logic; inverted-on-inverted reads as a normal
  bright `>` on amber.

**Test plan.** Render state with `active_layer = A` and assert col 5 row 2
has `ATTR_INVERSE` bit set; col 45 row 2 does not. Flip and re-assert.

---

## 4. F4 → SettingsScreen root

**Problem.** F4 currently jumps straight to SLOTS. Future settings
(audio, preferences, key rebinder) need a home — a flat top-level menu
doesn't scale.

**Design.**

- New `src/ui/screens/settings.rs` with `SettingsScreen` listing three
  entries:
  1. Preferences (stub: "Coming soon")
  2. Audio
  3. Slot Mapper
- Enter on an entry → `ScreenResult::Push(...)`.
- `MenuKind::Settings` replaces `MenuKind::Slots` as the F4 target.
  `MenuKind::Slots` removed; entry into SLOTS only via the settings root.
- Selection cursor + arrow keys + 1/2/3 digit shortcuts.

**File changes.** `src/action.rs`, `keymap.toml`, `src/main.rs` (open
`SettingsScreen` instead of `SlotsScreen`), `src/ui/screens/{mod,settings}.rs`.

---

## 5. AudioSettingsScreen + live-tunable audio params

**Problem.** Noise floor is currently env-var-only (`MANDLEROT_NOISE_FLOOR`).
Per-band gain has no knob at all. Tuning requires editing the unit file +
restart — useless mid-show.

**Design.**

- New `AudioParams` shared struct (Arc-of-AtomicF32 for each field) owned
  by main, cloned into the audio thread on spawn:
  ```rust
  struct AudioParams {
      noise_floor: AtomicF32,
      gain_bass: AtomicF32,
      gain_lomid: AtomicF32,
      gain_himid: AtomicF32,
      gain_treble: AtomicF32,
  }
  ```
- `AudioGain::normalize` reads `noise_floor` from the shared struct each
  call (current field becomes `Arc<AudioParams>`).
- Per-band gain multiplied onto the post-normalize value before push to
  `AtomicAudio`.
- `AudioSettingsScreen` renders 5 horizontal sliders. ↑↓ select knob, ←→
  adjust. `r` resets the focused knob to its default. Persist to
  `<state_dir>/audio.toml` on every change.

**File changes.** `src/audio/{envelope,thread,params}.rs`, `src/ui/screens/audio.rs`.

**Persistence layout.**
```toml
noise_floor = 8.0
gain_bass = 1.0
gain_lomid = 1.0
gain_himid = 1.0
gain_treble = 1.0
```

---

## 6. Rename Preset → Look

**Problem.** "Preset" reads as per-layer in VJ idiom, but our `Preset`
already holds both scenes + xfade + blend + params — a full combined
state. Misleading.

**Design.**

- `Mode::Preset` → `Mode::Look`.
- `PresetStore` → `LookStore`; `Preset` → `Look`; `PresetFile` → `LooksFile`.
- `presets.json` → `looks.json`. On first load: if `looks.json` missing
  but `presets.json` exists, rename in place (one-time migration log).
- Status panel: "PST" → "LK", "PRESETS" header → "LOOKS".
- CLI: `--presets` → `--looks` (keep `--presets` as deprecated alias for
  one release).
- README + design doc references.

**File changes.** broad but mechanical: `state.rs`, `apply.rs`,
`preset/store.rs` → rename file + types, `status/compose.rs`,
`status/overlay.rs`, `main.rs`, `keymap.toml` comments, README.

---

## 7. Scene: CRT signal collapse

**Problem.** No "broken hardware" aesthetic in the scene library.

**Design.**

- Base layer: gradient or color bars derived from `u_audio` (auto-stays
  visually loud).
- Effects pipeline (all in fragment shader):
  - **Scanlines**: `sin(uv.y * resolution.y * pi)` modulation, 70% depth.
  - **Chromatic aberration**: sample R/G/B at `uv ± k`. `k` scales with
    `u_audio.treble`.
  - **Vertical-hold roll**: triggered by `u_beat`. After a beat, for next
    ~200ms, shift `uv.y` by `fract(u_time * 3.0)`. Reads as a screen
    rolling.
  - **Static snow** band at random y rows (hash-gated, treble-driven).

**Params (slots 0..7).** `scan_depth`, `aberration`, `roll_rate`, `snow_amount`, `hue`, `brightness`, `roll_speed`, `tint_warp`.

---

## 8. Scene: VHS tracking

**Design.**

- Chroma offset: sample Y at `uv`, U/V at `uv + vec2(0.01, 0)`. Bleeds
  color right of luma.
- **Head-switch noise band**: a single 4px-tall band of TV-snow that
  rolls upward at ~0.3px/frame. Use `mod(u_time * speed, 1.0)` for y.
- **Dropout tears**: at random y rows (hash-gated by time), shift uv.x
  hard by ±0.1 for a single scanline.
- **Color bleed**: blur U/V channels horizontally with a 5-tap kernel.
- Audio: treble drives dropout count, bass drives chroma offset width.

**Params.** `chroma_offset`, `head_band_y_rate`, `dropout_rate`, `bleed`, `hue_shift`, `saturation`, `desync`, `brightness`.

---

## 9. Scene: Datamosh blocks

**Design.**

- Sample `u_prev` (previous frame texture) as the base "I-frame".
- Per-16x16-block motion vector computed from audio band derivative
  (bass = horizontal, lomid = vertical). Each block's `uv` is offset by
  its motion vector before sampling u_prev.
- Beat = forced keyframe reset (zero motion that frame; allows the
  visual to re-stabilize, then drift again).
- Optional: per-block hue-shift driven by hash(block_idx, time_bucket).

**Params.** `block_size`, `motion_bass`, `motion_lomid`, `mosh_intensity`, `hue_drift`, `keyframe_decay`, `chroma_corruption`, `noise_floor`.

---

## 10. Scene: Hex/binary rain

**Design.**

- Reuse `ascii_rain` skeleton (column hash, per-cell sub-pixel glyph,
  head/trail).
- Glyph set: 16 chars (0..9, A..F). Sub-pixel bitmaps are 5x7 like the
  Matrix glyphs.
- Slower fall, denser columns (~50% active vs. ASCII's ~25%), tighter
  trail to read as "data console" not "rain".
- Palette: amber (BIOS) by default, green via `palette_shift` param.

**Params.** `speed`, `glyph_rate`, `density`, `trail_decay`, `palette_shift` (0=amber, 1=green, blends), `head_brightness`, `noise_amount`, `binary_mix` (0=hex, 1=binary, blends).
