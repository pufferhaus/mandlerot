# mandleROT — GLSL ES 3.0 HQ Variant System (roadmap item 28)

**Date:** 2026-05-22  
**Status:** Approved, pending implementation

---

## Overview

Add GLSL ES 3.0 support to mandleROT via a `.hq.glsl` file-suffix convention. Pi4+ loads the uncapped HQ variant; Pi3 loads the existing ES 1.00 base. Every capped raymarch/compute scene gets a HQ variant that removes artificial loop bounds and uses 300-es features. A higher-quality bloom postfx pass and PiGen benchmark labeling round out the item.

Platform detection (28a) is already shipped. This item builds on top of it.

---

## 1. Variant Convention

**Rule:** if `scenes/foo.hq.glsl` exists alongside `scenes/foo.glsl`, Pi4+ loads the HQ variant; Pi3 loads the base. If only `.hq.glsl` exists (no Pi3 fallback), Pi3 filters the scene out via the existing `min_pi_gen` mechanism.

- The `.hq` suffix implies `glsl_version = "300es"` and `min_pi_gen >= Pi4` — no new TOML field required; convention drives it.
- One `.toml` file covers both variants. No duplicate metadata.
- Pi3 behavior for all existing scenes is unchanged.

**Loader change:** `SceneLibrary::load_dir_for_gen` (already Pi-aware from 28a) checks for `foo.hq.glsl` after finding `foo.toml` + `foo.glsl`. If found and `gen >= Pi4`, loads the HQ body and tags it for 300-es assembly. Same scene name, same params, same slot — transparent to the rest of the app.

---

## 2. GLSL ES 3.0 Prelude

**New file:** `shaders/prelude_300es.glsl`

Differs from `shaders/prelude.glsl` in exactly three ways:
- `#version 300 es` header (replaces `#version 100`)
- `in vec2 v_uv;` (replaces `varying vec2 v_uv;`)
- `out vec4 fragColor;` added (300-es drops the `gl_FragColor` builtin)

All `uniform` declarations are identical to the ES 1.00 prelude — no duplication of the uniform block.

**`src/render/shader.rs` changes:**

```rust
pub enum GlslVersion { Es100, Es300 }

pub fn assemble_scene_fragment(user_body: &str, version: GlslVersion) -> String
```

`assemble_scene_fragment` selects `PRELUDE` or `PRELUDE_300ES` based on `version`. Existing callers pass `GlslVersion::Es100` — no behavior change.

**Postfx prelude stays `#version 100`.** `bloom_hq` is Pi4-gated for performance reasons (sample count), not GLSL version — no postfx prelude variant needed.

---

## 3. HQ Scene Bodies

`.hq.glsl` files use:
- `fragColor` instead of `gl_FragColor`
- `texture()` instead of `texture2D()`
- Dynamic loop bounds (no compile-time cap workarounds)
- `switch` instead of `if/else if` integer dispatch
- Integer types where appropriate

Base `.glsl` files are untouched.

**Scenes receiving `.hq.glsl` variants:**

| Scene | Cap lifted |
|---|---|
| `mandelbulb` | Raymarch step cap removed, dynamic early-exit |
| `mandelbox` | Same |
| `juliabulb` | Same |
| `menger_sponge` | Iteration depth |
| `apollonian` | Fold iteration count |
| `kleinian` | Same |
| `reaction_diffusion` | Simulation steps per frame |
| `maze_3d` | Raymarch steps |
| `pipes_3d` | Raymarch steps |

No new params, no TOML changes for any of these.

---

## 4. bloom_hq.glsl

`postfx/bloom_hq.toml` already exists with `min_pi_gen = "Pi4"` and `enabled_by_default = false`. This item writes the missing `postfx/bloom_hq.glsl`.

Stays `#version 100` (Pi4-gated for perf, not GLSL version). Implements a wider separable Gaussian approximation: ~13-tap kernel vs the existing `bloom.glsl` ~5-tap. Single-pass (samples in both axes in one draw). No postfx architecture changes — true multi-pass bloom is a separate future item.

Params (from existing TOML): `threshold` (0–1), `intensity` (0–3), `radius` (0.5–3).

---

## 5. Benchmark Output

`--benchmark N` gets detected `PiGen` in the header line so `.docs/bench-pi5.md` dumps are self-labeling. One-liner addition in `main.rs`.

---

## Files Touched

| File | Change |
|---|---|
| `shaders/prelude_300es.glsl` | New — 300-es uniform block + `in`/`out` |
| `src/render/shader.rs` | `GlslVersion` enum, `assemble_scene_fragment` version param |
| `src/scene/library.rs` | `.hq.glsl` probe in `load_dir_for_gen` |
| `scenes/mandelbulb.hq.glsl` | New HQ variant |
| `scenes/mandelbox.hq.glsl` | New HQ variant |
| `scenes/juliabulb.hq.glsl` | New HQ variant |
| `scenes/menger_sponge.hq.glsl` | New HQ variant |
| `scenes/apollonian.hq.glsl` | New HQ variant |
| `scenes/kleinian.hq.glsl` | New HQ variant |
| `scenes/reaction_diffusion.hq.glsl` | New HQ variant |
| `scenes/maze_3d.hq.glsl` | New HQ variant |
| `scenes/pipes_3d.hq.glsl` | New HQ variant |
| `postfx/bloom_hq.glsl` | New wide-kernel bloom |
| `src/main.rs` | PiGen in benchmark header |

---

## Out of Scope

- Postfx prelude 300-es variant — not needed for any current postfx pass
- True multi-pass bloom (separate FBO management in postfx.rs) — future item
- `glsl_version` TOML field — replaced entirely by filename convention
- Adaptive auto-scale (cancelled in 28a)
- Per-gen resolution table (cancelled in 28a)
