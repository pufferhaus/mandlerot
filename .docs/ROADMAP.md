# mandleROT Roadmap

Compact index. Full specs in `ROADMAP-SPECS.md`. Effects brainstorm in
`EFFECTS-CATALOG.md` (✅ shipped / 📋 queued / ☐ ideas). Done work archived
in `COMPLETED.md` once Recently Shipped exceeds 3 entries.

## Bugs / Blockers

_(none currently tracked)_

## Recently Shipped

- **2026-05-16** Video input (item 24) — new `src/video/` (nokhwa-backed capture thread, `ArcSwap<Arc<VideoFrame>>` handoff), `u_video` + `u_video_uv_scale` in the GLSL prelude (TU3), baked `__video__` scene, demo `video_glitch` scene, `VID:` status chip, F4 → Audio → Device picker for routing dongle audio. Continuous capture with 5s NoDevice retry + 1s stale threshold. Spec at `docs/superpowers/specs/2026-05-16-video-input-design.md`.
- **2026-05-16** Pi-gen detect + per-scene caps (28a) — new `src/platform.rs` (`PiGen::detect`, env override), `min_pi_gen` filter on `SceneLibrary::load_dir_for_gen`, pipeline drops per-scene `internal_resolution` caps on Pi 5 / Unknown so previously down-scaled scenes scale up to native, `install.sh` writes per-gen `MANDLEROT_RENDER_SCALE` + skips `composite=1` on Pi 4/5. Scene-list menu surfaces "N visible / M hidden on PiX". 218 tests green.
- **2026-05-15** Numpad menu arrows + `000+±` param chord — rotated pad now navigates menus via centre cross (`6/4/8/2` → U/D/L/R) and steps params with `000`-held xfade keys. Slots screen drops numpad digit-jump (was misaligned with rotation).

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
| 24 | Composite video input (USB capture → live texture as a layer) | ✅ | `src/video/`, `src/render/pipeline.rs`, `src/state.rs` |
| 25 | Additional blend modes — tier 1 (Overlay, HardLight, Lighten, Darken, Exclusion, Subtract, LinearBurn) | ✅ | `shaders/blend.glsl`, `src/state.rs::BlendMode` |
| 25b | Blend modes tier 2 (SoftLight, ColorDodge, ColorBurn, Hue, Saturation, Color, Luminosity) | ✅ | `shaders/blend.glsl`, `src/state.rs::BlendMode` |
| 26a | Post-FX phase 1: chain skeleton + Vignette/Grain/Pixelate passes (no UI, no persistence) | ✅ | `src/render/postfx.rs`, `src/render/pipeline.rs`, `shaders/postfx_prelude.glsl`, `postfx/*.{glsl,toml}` |
| 26b | Post-FX phase 2: UI (F4→Post-FX), `postfx.toml` persistence, Chromatic Aberration + Bayer Dither | ✅ | `src/ui/screens/postfx.rs`, `src/render/postfx.rs`, `postfx/{chromatic,dither}.{glsl,toml}` |
| 26b' | Post-FX phase 2b: LUT colour grade (needs PNG loader + aux texture) + hot-reload of `postfx/` dir | ☐ | `src/render/postfx.rs`, `src/hot_reload.rs`, `postfx/lut.{glsl,toml}` |
| 26c | Post-FX phase 3: Bloom (half-res blur) + CRT overlay | ☐ | `src/render/postfx.rs`, `postfx/{bloom,crt}.{glsl,toml}` |
| 26d | Post-FX phase 4: per-Look post-FX (Look schema bump to v2) | ☐ | `src/preset/store.rs`, `src/apply.rs` |
| 27 | Chromakey output mode (paint scene backgrounds with a key color for an external video mixer) | ☐ | `src/render/chromakey.rs`, `src/scene/meta.rs`, `shaders/blend.glsl`, `src/ui/screens/chromakey.rs` |
| 28a | Pi-gen detect + per-scene caps (PiGen runtime detect, `min_pi_gen` filter, ignore per-scene `internal_resolution` on Pi 5, install-time `render_scale` per gen, skip composite overlay on Pi 5) | ✅ | `src/platform.rs` (new), `src/scene/{meta,library}.rs`, `src/render/pipeline.rs`, `src/ui/screens/scene_list.rs`, `deploy/install.sh`, `src/main.rs` |
| 28  | Pi 4+ shader headroom — opt-in `#version 300 es` prelude variant (`glsl_version` scene field auto-marks `min_pi_gen = Pi4`) + real Gaussian bloom postfx tier. Blocked on Pi 4 + Pi 5 hardware in hand. | ☐ | `src/render/shader.rs`, `src/render/postfx.rs`, `postfx/bloom.{glsl,toml}`, `.docs/bench-pi{4,5}.md` |

Active phase = first incomplete step. Mark `✅` and bump to Recently Shipped on completion.

## Backlog (post-current-phase)

- Pi smoke test (blocked until hardware in hand)
- MPI3501 status panel: characterize the actual color response (R-channel weak, G→B bleed) and write a real Rgb565→fb conversion in `status::pi`. Current values in `status::theme` are empirical workarounds — amber is approximate, "dim" is just pure red because reducing R kills the hue.
- Touch input on SPI panel (XPT2046 wired, software TBD)
- MIDI / OSC control surface
- Look workflow inside menu (rename slots, see saved-at, recall from menu)
- More demoscene effects (see `EFFECTS-CATALOG.md::Where to look next` — Fire, Tetris rain, NORAD radar, Sandpile/Lenia, dashboard cluster, Lorenz attractor)
