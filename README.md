# mandleROT

Generative video VJ tool for Raspberry Pi composite output. Single Rust
binary, GLSL scenes hot-reloaded from disk, two-layer A/B mix with crossfade,
audio reactivity from a USB mic, and an SPI text-grid status display.

## Quickstart (desktop dev)

```
cargo run
```

Opens a 720×480 window. Edit `scenes/plasma.glsl` and save — the change
applies live within ~250 ms.

```
cargo run -- --smoke-frames 2     # boot the pipeline, render 2 frames, exit
cargo run -- --status-window      # also open a preview of the SPI panel
cargo test --lib                  # 164 unit tests
```

## Layout

| Path | What |
|---|---|
| `scenes/foo.glsl`        | Fragment shader (the scene body — prelude is auto-injected) |
| `scenes/foo.toml`        | Scene metadata: display name, 8 params, audio routing |
| `shaders/prelude.glsl`   | Uniforms shared by every scene |
| `shaders/blend.glsl`     | Fixed A/B blend pass (5 modes) |
| `shaders/safe_scene.glsl`| SMPTE test pattern shown during PANIC / compile faults |
| `config.toml`            | Initial scene names, xfade, blend mode, render size, FPS |
| `keymap.toml`            | Key → Action bindings |
| `.docs/`                 | Roadmap, full specs, effects catalog |
| `docs/superpowers/`      | Historical design spec + implementation plans (archival) |
| `CLAUDE.md`              | Project-specific guidance for AI coding agents |

User state (slots, audio settings, looks) lives at
`$MANDLEROT_STATE_DIR` (set to `/var/lib/mandlerot` by the systemd unit)
or `<exec>/.config/mandleROT/` for desktop dev.

## Scene library

58 scenes as of 2026-05-11, grouped by aesthetic in
[`.docs/EFFECTS-CATALOG.md`](.docs/EFFECTS-CATALOG.md):

- **Fractals**: mandelbrot (∞ zoom), mandelbulb, mandelbox, juliabulb, menger_sponge, sierpinski_3d, apollonian, kleinian
- **Gritty / digital**: crt_collapse, vhs_tracking, datamosh, hex_rain, ascii_rain, glitch, static, bayer, phosphor_crt
- **Demoscene classics**: plasma, tunnel, starfield, metaballs, cube_wireframe, voxel_terrain, donut
- **Audio scopes**: spectrogram_waterfall, spectrum_bars, waveform_line, vectorscope, strobe, shockwave, vinyl
- **Geometric / vector**: synthwave_grid, kaleidoscope, lissajous, voronoi, hex_grid, truchet, pulse_grid
- **Organic**: caustics, curl_noise, reaction_diffusion, conway, smoke, pond
- **Pop / anime**: speed_lines, halftone, transform_rings, sparkle
- **Game refs**: pipes_3d, maze_3d, pong, boids
- **Cyberpunk**: bios_post
- **Experimental**: slit_scan, echo, mirror_delay

## Build matrix

| Target | Command |
|--------|---------|
| Desktop dev | `cargo run` |
| Desktop perf | `cargo run --release` |
| Pi cross-compile | `make build-pi` |
| Smoke tests | `make smoke` |
| Deploy to Pi | `make deploy HOST=mandlerot.local` |

## Controls

Modal scheme. **Tab** cycles modes: SCENE → PARAM → LOOK → SCENE.

### Always active
- **Esc / Backspace**: PANIC (both layers → safe-scene, audio bypass on)
- **Esc twice within 400 ms**: PANIC + close any open menu (escape hatch)
- **F4**: open Settings menu (Audio, Slot Mapper, Preferences)
- **Backslash / NumpadEnter**: toggle active layer (A ↔ B)
- **Tab** *(or NumLock+NumpadEnter)*: advance mode
- **N / NumpadDivide**: trigger pulse
- **M / NumpadMultiply**: cycle blend mode
- **F** *(or NumLock+Numpad0)*: freeze (pause `u_time`)
- **L / Space / Numpad0**: tap-tempo
- **G / NumpadDecimal**: toggle audio bypass
- **F4** *(or NumLock+NumpadDecimal)*: open Settings menu

Holding `NumLock` on a USB numpad acts as a sticky-free shift: the three
numpad keys above gain their second meaning while NumLock is held, so the
numpad alone can advance modes, freeze, and open menus. NumLock is also the
"other layer" modifier for digit keys (mirrors keyboard Shift).

### SCENE mode (default)
- **1-9** (top row or numpad): select scene N for active layer.
  Resolves via `slots.toml` (bind any scene to any digit via F4 → Slot Mapper);
  unbound digits fall back to alphabetical Nth.
- **Shift+1-9**: select for the *other* layer
- **[ ] / NumpadAdd / NumpadSubtract / -/=** : crossfade

### PARAM mode (Tab once)
- **1-8**: select param slot for active layer
- **9**: reset selected param
- **-/=**: decrement / increment selected param

### LOOK mode (Tab again)
A *Look* is a combined A+B state: both scenes, xfade, blend, both param maps.
- **1-8**: recall Look slot
- **Shift+1-8**: save current state to slot
- **9**: reset all params on active layer

### In-menu (when F4 menu is open)
- **↑↓ / PgUp/Dn / Home/End**: navigate lists
- **Enter / 1-3**: open the selected entry
- **0**: clear binding (Slot Mapper)
- **← / →**: nudge value (Audio settings)
- **r**: reset focused knob to default (Audio settings)
- **Esc**: close one level. **Esc twice quickly** → PANIC.
- **Numpad `-` + `+` + `Enter` within 400 ms (any order)** → PANIC. Works
  even with a menu open, mirroring the double-tap-Esc escape hatch.

### Dev keys
- **F1**: toggle top-of-screen overlay
- **F2 / F3**: cycle scene A / B alphabetically
- **F5**: force-reload all scenes

## Status panel

A 480×320 amber-phosphor SPI TFT (Hosyond 3.5") shows operator state:
mode, active layer (header-row inverted on the active side), scene names,
8-row param readout per layer, audio bands, xfade bar, Look slots 1-8,
last action label, and a hotkeys cheat-sheet.

Menus replace the normal compose grid while open. Live render is unaffected.

Desktop dev modes:
- Default: dumps a PNG snapshot of the panel to `target/status.png` each frame.
- `--status-window`: opens a second window with a live preview of the panel.

## Audio

Default capture device, 1024-sample windows at ~100 Hz, four log-binned
frequency bands (bass / lo-mid / hi-mid / treble), per-band attack/release
envelope, P95 auto-gain with absolute noise floor, beat detection on the
spectral magnitude.

Both the noise floor *and* per-band gain are **live-tunable** via
F4 → Audio. Values persist to `audio.toml`. The `MANDLEROT_NOISE_FLOOR`
env var still works as a one-shot override.

Scenes consume audio via `u_audio.x` (bass) through `u_audio.w` (treble),
the `u_beat` trigger pulse, `u_bpm`, and `u_audio_history` (1×320 RGBA
ring with the last ~10 s of band values).

## Looks (formerly "Presets")

Up to 8 saved combined A+B states. Stored as JSON at
`$MANDLEROT_STATE_DIR/looks.json`. Each Look captures:

- Scene names for A and B
- xfade, blend mode, audio bypass
- Full param maps for both layers (keyed by param name, not slot index — survives scene refactors)

A legacy `presets.json` is migrated to `looks.json` on first run.

## Roadmap

See [`.docs/ROADMAP.md`](.docs/ROADMAP.md). Active queue at the time of
writing (items 24-27):

- **Composite video input** — USB v4l2 capture → live layer texture
- **Additional blend modes** — Overlay, Soft/Hard Light, Dodge/Burn, HSL family, etc.
- **Post-FX pipeline** — Bloom, vignette, grain, CRT overlay, LUT grading, dither
- **Chromakey output** — paint scene backgrounds with a key color for external hardware mixers

Full specs in [`.docs/ROADMAP-SPECS.md`](.docs/ROADMAP-SPECS.md).

## Deploying to a Pi

```
make build-pi                          # cross-compile to armv7
make install-pi HOST=mandlerot.local   # one-time provision + deploy
make deploy-restart HOST=mandlerot.local
```

`install-pi` creates a service user, drops the systemd unit, edits
`config.txt` for composite + SPI, and copies the binary + scene library.
The Pi reboots once during provisioning to activate the composite output.

Subsequent updates are `make deploy-restart`. User state in
`/var/lib/mandlerot/` (slots, audio settings, looks) is preserved across
reinstalls because the unit's `ProtectSystem=strict` + `ReadWritePaths`
boundary excludes it from the install path.
