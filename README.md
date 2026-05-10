# mandleROT

Generative video VJ tool for Raspberry Pi composite output. Single Rust binary,
GLSL scenes hot-reloaded from disk, two-layer A/B mix with crossfade.

## Quickstart (desktop dev)

```
cargo run
```

Opens a 720×480 window. Edit `scenes/plasma.glsl` and save — the change
applies live within ~250 ms.

## Layout

- `scenes/foo.glsl` — fragment shader (the scene)
- `scenes/foo.toml` — metadata (display name, params, audio routing)
- `shaders/prelude.glsl` — uniforms injected before user shaders
- `shaders/blend.glsl` — fixed blend pass (5 modes)
- `config.toml` — initial scene names, xfade, blend mode
- `docs/superpowers/specs/` — full design spec
- `docs/superpowers/plans/` — implementation plans

## Build matrix

| Target | Command |
|--------|---------|
| Desktop dev | `cargo run` |
| Desktop perf | `cargo run --release` |
| Pi cross-compile | `make build-pi` |
| Smoke tests | `make smoke` |
| Deploy to Pi | `make deploy HOST=mandlerot.local` |

## Controls (Plan 2)

Modal scheme. **Tab** cycles modes: SCENE → PARAM → PRESET → SCENE.

### Always active
- **Esc / Backspace**: PANIC (both layers → safe-scene, audio bypass on)
- **Backslash / NumpadEnter**: toggle active layer (A ↔ B)
- **Tab**: advance mode
- **N / NumpadDivide**: trigger pulse
- **M / NumpadMultiply**: cycle blend mode
- **F**: freeze (pause `u_time`)
- **L / Space / Numpad0**: tap-tempo
- **G / NumpadDecimal**: toggle audio bypass

### SCENE mode (default)
- **1-9** (top row or numpad): select scene N for active layer
- **Shift+1-9**: select for the other layer
- **-/= / [ ] / Numpad± **: crossfade

### PARAM mode (Tab once)
- **1-8**: select param slot for active layer
- **9**: reset selected param
- **-/=**: decrement / increment selected param

### PRESET mode (Tab again)
- **1-8**: recall preset slot
- **Shift+1-8**: save current state to slot
- **9**: reset all params on active layer

### Dev keys (debug feature)
- **F1**: toggle overlay (Plan 3)
- **F2 / F3**: cycle scene A / B
- **F5**: force-reload all scenes

## Status

- Plan 1: Foundation, scenes, hot-reload — done.
- Plan 2: Input, audio FFT, presets — done.
- Plan 3: Status display, supervisor, deploy — done.

## Status panel

A 480×320 amber-phosphor SPI TFT (Hosyond 3.5") shows operator state:
mode, active layer, scene names, params, audio levels, presets, last
action, hotkeys cheat-sheet.

Desktop dev mode dumps a PNG snapshot of the panel to `target/status.png`
each frame so you can verify the layout from your laptop without any SPI
hardware.

## Deploying to a Pi

```
make build-pi                    # cross-compile to armv7
make install-pi HOST=mandlerot.local  # one-time install + deploy
make deploy-restart HOST=mandlerot.local
```

`install-pi` provisions the user, drops the systemd unit, edits
`config.txt` for composite + SPI, and copies the binary + scene library.
The Pi reboots once during provisioning to activate the composite output.
