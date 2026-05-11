# CLAUDE.md — mandleROT

Project-specific instructions for Claude. Read this before editing.

## Hardware target

- **Primary**: Raspberry Pi 3 B+ (Quad A53 1.4 GHz, 1 GB RAM, VideoCore IV, USB 2.0, native composite-out via TRRS jack).
- **Same binary** runs on Pi 4 + Pi 5 with the appropriate composite cable.
- Macros to remember:
  - No native HDMI / composite *input* on any Pi — capture is USB v4l2 only.
  - USB-2 bus on 3B+ shares silicon with Ethernet — heavy capture starves the network.
  - GLES 2.0 / EGL on the Pi side. Desktop dev uses glow + winit + glutin.

## GLSL constraints (ES 1.00)

Scene shaders must compile under `#version 100`. Common traps:

- `precision mediump float;` is already in the prelude — don't redeclare.
- No `switch` on `int`. Use `if/else if` chains.
- No implicit int↔float conversion. Always `float(i)`, never `i / 2.0`.
- Loop bounds must be compile-time constants for many drivers. Use `for (int i = 0; i < 64; i++) { if (float(i) >= n) break; ... }`.
- `texture2D`, not `texture()`. `gl_FragColor`, not `out vec4`.
- All uniforms come from `shaders/prelude.glsl` (injected automatically). The full set:
  - `u_time`, `u_resolution`, `u_audio.xyzw` (bass / lomid / himid / treble), `u_beat`, `u_bpm`, `u_trigger`
  - `u_param0..u_param7` — 8 scene params
  - `u_prev` — last rendered frame (sampler2D)
  - `u_audio_history` — 1×320 RGBA ring (sampler2D, `v=1` newest, channels match `u_audio`)
  - `v_uv` — fragment UV in [0,1]

## Scene contract

Every scene = one `.glsl` + one `.toml` in `scenes/`. Sourced examples: `plasma`, `ascii_rain`, `donut`.

```toml
name = "my_scene"
display_name = "My Scene"
keyable = false           # optional, defaults false. true = dark pixels are background (chromakey item 27)

[[params]]
slot = 0                  # 0..7 only
name = "intensity"
min = 0.0
max = 1.0
default = 0.5
curve = "linear"
audio_route = "bass"      # optional: bass/lomid/himid/treble/beat
audio_amount = 0.2        # 0..1, how much audio modulates the param
audio_polarity = 1.0      # +1 = louder = higher, -1 = inverted
```

Scene library auto-discovers paired files under `scenes/` at startup *and* on file-watch (hot-reload). Names that begin with `__` are reserved (e.g. `__safe__`).

## User state paths

State the user can edit lives outside the binary install. Resolved via `crate::config::user_state_dir()` with this order:

1. `$MANDLEROT_STATE_DIR` (set by the systemd unit on Pi → `/var/lib/mandlerot`)
2. `<exec_dir>/.config/mandleROT/`
3. `./.config/mandleROT/`

Files in that directory:

| File | Purpose |
|---|---|
| `slots.toml` | Slot 1..9 → scene name mapping (`SlotBindings`) |
| `audio.toml` | Live-tunable noise floor + per-band gains (`AudioParams`) |
| `looks.json` | Saved Looks 1..8 (combined A+B state) — migrated from legacy `presets.json` on first run |
| `chromakey.toml` | (planned, item 27) Chromakey output config |
| `postfx.toml` | (planned, item 26) Post-FX chain config |

`ProtectSystem=strict` in the systemd unit makes `/opt/mandlerot` read-only on Pi; only `/var/lib/mandlerot` is writable. Bundle reinstalls leave user state intact.

## Code layout

```
src/
  action.rs          — Action enum + MenuKind
  apply.rs           — Action → SharedState mutations
  audio/             — capture, FFT, bands, envelopes, beat, AudioParams (live-tunable)
  config.rs          — Config (TOML) + user_state_dir()
  input/             — keymap, winit_src, evdev_src, mock, double_tap
  preset/            — Look (renamed from Preset) save/recall + SlotBindings
  render/            — Pipeline, FBO, desktop / pi targets, shader assembly
  scene/             — Library, meta, ParamMap, audio routing
  state.rs           — SharedState + Mode (Scene/Param/Look) + Layer (A/B)
  status/            — 80×26 amber text grid (compose, render, theme, backends)
  ui/                — Screen trait + ScreenStack + screens/ (settings, slots, scene_list, audio)
  main.rs            — render loop, input dispatch, supervisor
shaders/
  prelude.glsl       — uniform declarations injected before every scene
  blend.glsl         — fixed A/B blend pass, 5 modes (Mix/Add/Mult/Screen/Diff)
  safe_scene.glsl    — baked SMPTE test pattern (PANIC fallback)
  quad.vert          — single shared vertex shader
scenes/              — 58 scene pairs as of 2026-05-11
.docs/               — ROADMAP.md, ROADMAP-SPECS.md, EFFECTS-CATALOG.md
docs/superpowers/    — historical design + plan docs (don't rewrite, reference only)
deploy/              — systemd unit, install.sh, boot-config-additions
```

## Build / test workflow

```bash
cargo build                 # verify Rust compiles
cargo test --lib            # 164 unit tests (no GL needed)
cargo run -- --smoke-frames 2   # opens window, renders 2 frames, exits — catches GLSL compile errors
```

To smoke a single scene, edit `config.toml::initial.scene_a` then `--smoke-frames 2`. The integration test `every_scene_renders_60_frames` exists in `tests/integration_pipeline.rs` but is `--ignored` on macOS because winit's `EventLoop` can't be re-created within one process.

## Roadmap pattern

Two-file pattern at `.docs/`:

- `ROADMAP.md` — compact index. `## Bugs / Blockers`, `## Recently Shipped` (max 3), `## Execution Order` table, `## Backlog`.
- `ROADMAP-SPECS.md` — one section per execution-order item. Full design.
- `EFFECTS-CATALOG.md` — long catalogue of all generative-effect ideas (✅ shipped / 📋 queued / ☐ idea).

**Active phase** = lowest-numbered incomplete row in the Execution Order table.

## UI / menu system

`src/ui/` is the framework for in-app menus. Every menu = one `impl Screen`. Adding a new one:

1. New file in `src/ui/screens/`.
2. Register in `src/ui/screens/mod.rs`.
3. Reachable either from `MenuKind` (F4 root) or from a parent screen via `ScreenResult::Push(Box::new(...))`.

While the stack is non-empty, *all* key input is routed to the top screen (the keymap is bypassed). The status panel renders the top screen's grid in place of the normal compose grid. Render is done on the main thread (where bindings + scene names are live), then shipped to the status worker thread.

Exception: **double-tap Esc/Backspace within 400 ms** fires `Action::Panic` and closes the stack — the always-available escape hatch.

## Conventions

- **No Co-Authored-By trailers** in commit messages (user-wide rule). Tight subject + bullet body, see `git log --oneline` for tone.
- **Don't rewrite historical docs** in `docs/superpowers/`. They're the original design + plans, kept as-is for archaeology. Update `.docs/ROADMAP*.md` and `README.md` instead.
- **Don't add Co-Authored-By, emojis, or marketing-tone commentary** to code or commits unless explicitly asked.
- **Scene file naming**: short snake_case (e.g. `crt_collapse`, not `CRTCollapse` or `crt-collapse`).
- **Test placement**: per-module `#[cfg(test)] mod tests` blocks. Avoid `tests/` integration tests unless they truly need cross-module coverage — they're awkward on macOS due to winit limits.
- **GLSL changes**: assume nothing about which scene is in flight. Use the smoke-test harness (edit config.toml, run with `--smoke-frames 2`) to verify a shader compiles before declaring done.

## Active-runtime notes

- **PANIC** (`Esc`/`Backspace` once with no menu, or double-tap from anywhere): both layers → `__safe__` (SMPTE bars), xfade → 0.5, audio bypass on, mode → Scene. Doesn't touch BPM / freeze / blend / looks.
- **Safe scene is baked into the binary** (`shaders/safe_scene.glsl` → `include_str!`). Changes require a rebuild, not a hot-reload.
- **Audio noise floor + per-band gain** are live-tunable via F4 → Audio. Persists to `audio.toml`. The `MANDLEROT_NOISE_FLOOR` env var still works as a one-shot override.

## When asked to ship a feature

1. Find the matching item in `.docs/ROADMAP.md`. If not there, add it (and a spec stanza in `ROADMAP-SPECS.md`) before coding.
2. Build the minimal change. Don't add abstractions for hypothetical future variants.
3. `cargo test --lib` green + `cargo run -- --smoke-frames 2` green = ship.
4. Update the `✅` mark in the roadmap table and roll the entry into `## Recently Shipped` (keep that section to 3 lines; older entries don't need explicit archival yet — the table mark is authoritative).
