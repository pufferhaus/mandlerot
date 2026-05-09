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

## Status

Plan 1 implemented: rendering, scenes, hot-reload. Plans 2 (input + audio +
presets) and 3 (status display + deploy) pending.
