# mandleROT Roadmap

Compact index. Full specs in `ROADMAP-SPECS.md`. Effects brainstorm in
`EFFECTS-CATALOG.md` (✅ shipped / 📋 queued / ☐ ideas). Done work archived
in `COMPLETED.md` once Recently Shipped exceeds 3 entries.

## Bugs / Blockers

_(none currently tracked)_

## Recently Shipped

- **2026-05-11** 17 new effects shipped: crt_collapse, vhs_tracking, datamosh, hex_rain, smoke, speed_lines, halftone, transform_rings, sparkle, pong, pond, boids, donut, voxel_terrain, bios_post, vectorscope, bayer.
- **2026-05-11** Preset → Look rename. `Mode::Look`, `LookStore`, `looks.json` on disk with one-shot migration from `presets.json`. Status panel reads "LOOK"/"LOOKS"; CLI accepts `--looks` (or `--presets` as alias).
- **2026-05-10** Audio settings screen + SettingsScreen root menu under F4. Slot Mapper nested. Active-layer header inverts on status panel. Double-tap Esc = panic-from-anywhere.

## Design Notes

- **Combined-state save = "Look"** (renamed from Preset). `LookStore` holds scene_a, scene_b, xfade, blend mode, params for both layers.
- **Active layer highlight** = header row inverted on the status panel (amber bg, black fg). Dim attr stripped inside the band.
- **User state** lives at `$MANDLEROT_STATE_DIR` (systemd) or `<exec>/.config/mandleROT/`. Bundle reinstalls leave it intact via the `ReadWritePaths` boundary.
- **Screen stack** (`src/ui/`) is the scalable home for all in-app menus. Adding a screen = one file + register MenuKind / push from parent.

## Execution Order

| ID | Step | Status | Key files |
|----|------|--------|-----------|
| 1  | Roadmap docs set up | ✅ | `.docs/ROADMAP.md`, `.docs/ROADMAP-SPECS.md` |
| 2  | Double-tap Esc → Panic from inside menu | ✅ | `src/input/double_tap.rs`, `src/main.rs` |
| 3  | Active-layer = solid inverted half | ✅ | `src/status/compose.rs` |
| 4  | F4 → SettingsScreen root; Slot Mapper becomes child | ✅ | `src/ui/screens/settings.rs`, `src/action.rs`, `keymap.toml` |
| 5  | AudioSettingsScreen + live-tunable noise floor / band gains | ✅ | `src/ui/screens/audio.rs`, `src/audio/{params,thread,envelope}.rs` |
| 6  | Rename Preset → Look (mode, struct, file, UI labels) | ✅ | `src/state.rs`, `src/preset/store.rs`, `src/status/compose.rs`, `src/apply.rs`, `src/main.rs` |
| 7  | Scene: CRT signal collapse | ✅ | `scenes/crt_collapse.{glsl,toml}` |
| 8  | Scene: VHS tracking | ✅ | `scenes/vhs_tracking.{glsl,toml}` |
| 9  | Scene: Datamosh blocks | ✅ | `scenes/datamosh.{glsl,toml}` |
| 10 | Scene: Hex/binary rain | ✅ | `scenes/hex_rain.{glsl,toml}` |
| 11 | Scene: Smoke / ink dispersion | ✅ | `scenes/smoke.{glsl,toml}` |
| 12 | Scene: Speed lines (anime) | ✅ | `scenes/speed_lines.{glsl,toml}` |
| 13 | Scene: Ben-Day halftone dots | ✅ | `scenes/halftone.{glsl,toml}` |
| 14 | Scene: Sailor-Moon transformation rings | ✅ | `scenes/transform_rings.{glsl,toml}` |
| 15 | Scene: Sparkle burst | ✅ | `scenes/sparkle.{glsl,toml}` |
| 16 | Scene: Self-playing Pong | ✅ | `scenes/pong.{glsl,toml}` |
| 17 | Scene: Pond ripples (wave eq) | ✅ | `scenes/pond.{glsl,toml}` |
| 18 | Scene: Flocking boids | ✅ | `scenes/boids.{glsl,toml}` |
| 19 | Scene: donut.c spinning ASCII donut | ✅ | `scenes/donut.{glsl,toml}` |
| 20 | Scene: Voxel terrain (Comanche flyover) | ✅ | `scenes/voxel_terrain.{glsl,toml}` |
| 21 | Scene: BIOS POST scroll | ✅ | `scenes/bios_post.{glsl,toml}` |
| 22 | Scene: Audio vectorscope | ✅ | `scenes/vectorscope.{glsl,toml}` |
| 23 | Scene: Bayer 1-bit dither | ✅ | `scenes/bayer.{glsl,toml}` |
| 24 | Composite video input (USB capture → live texture as a layer) | ☐ | `src/video/`, `src/render/pipeline.rs`, `src/state.rs` |
| 25 | Additional blend modes (Overlay, Soft/Hard Light, Dodge/Burn, Lighten/Darken, Exclusion, Subtract, HSL family) | ☐ | `shaders/blend.glsl`, `src/state.rs::BlendMode` |
| 26 | Post-processing FX pipeline (bloom, vignette, grain, CRT, dither, pixelate, LUT) | ☐ | `src/render/postfx.rs`, `src/render/pipeline.rs`, `src/ui/screens/postfx.rs` |
| 27 | Chromakey output mode (paint scene backgrounds with a key color for an external video mixer) | ☐ | `src/render/chromakey.rs`, `src/scene/meta.rs`, `shaders/blend.glsl`, `src/ui/screens/chromakey.rs` |

Active phase = first incomplete step. Mark `✅` and bump to Recently Shipped on completion.

## Backlog (post-current-phase)

- Pi smoke test (blocked until hardware in hand)
- Touch input on SPI panel (XPT2046 wired, software TBD)
- MIDI / OSC control surface
- Look workflow inside menu (rename slots, see saved-at, recall from menu)
- More demoscene effects (see `EFFECTS-CATALOG.md::Where to look next` — Fire, Tetris rain, NORAD radar, Sandpile/Lenia, dashboard cluster, Lorenz attractor)
