# Resume — feat/video-input

**Snapshot:** 2026-05-16, after Task 14 + final-review fixes. Branch is implementation-complete; only finishing-a-branch (PR/merge) remains.

## Where things stand

- **Branch:** `feat/video-input` (checked out, working tree clean).
- **HEAD:** `4fb68ad` `render/pipeline: skip oversized video frames + opaque-black texture init`.
- **Ahead of `main`:** 18 commits.
- **`cargo test --lib`:** 232 passed, 0 failed, 9 ignored.
- **`cargo run -- --smoke-frames 2`:** exit 0 on macOS dev box (FaceTime camera opens at 1920×1080 — oversized-frame guard fires, no panic).
- **`cargo` binary path:** `~/.cargo/bin/cargo` (system PATH doesn't have it).

## What's already shipped on this branch

Two features landed together because video-input was authored against 28a's `min_pi_gen` field:

1. **Item 28a** — Pi-gen detect + per-scene caps (commit `6e2b2e9`).
2. **Item 24** — Video input (commits `8c127a5` through `4fb68ad`).

Both already marked `✅` in `.docs/ROADMAP.md`. Recently Shipped already updated.

### Commits (oldest → newest, on this branch)

```
f60fbbd Docs: refresh NUMPAD-MANUAL with current chord layout
6e2b2e9 Ship 28a: Pi-gen detect + per-scene caps
8c127a5 Add nokhwa + arc-swap deps behind video feature
3a036f5 video: VideoFrame type with Arc<[u8]> shared pixel buffer
5baccd3 video: VideoStatus + VideoHandle with lock-free atomic packing
b4ca3c8 video: nokhwa-backed capture thread with retry on NoDevice
3e6503a video: silence unused_assignments warning on decode_errors
2e87482 shaders/prelude: declare u_video + u_video_uv_scale
14a3a13 scene: bake __video__ alongside __safe__
e824741 render/pipeline: u_video texture on TU3 + seq-gated upload
0012a97 scenes: add video_glitch demo (horizontal-tear datamosh on u_video)
0db8199 status: VID: chip + thread VideoStatus through RenderCtx + PanelSnapshot
7d3b7b7 main: start_capture + attach VideoHandle + thread VideoStatus
ad05dcd audio: add device field to audio.toml + open_with_device variant
0706283 audio: respawn worker thread when audio.toml device changes
a005c59 ui: F4 → Audio → Device picker for CPAL input device selection
95ecae6 Roadmap: item 24 (video input) shipped
4fb68ad render/pipeline: skip oversized video frames + opaque-black texture init
```

## What remains

**One step:** invoke the `superpowers:finishing-a-development-branch` skill to decide how to integrate. Options (the skill will ask):

- Open PR against `main` via `gh pr create`.
- Local merge / fast-forward.
- Other cleanup.

Suggested PR title: `Video input (item 24) + Pi-gen detect (item 28a)`

Suggested PR body (paste into `gh pr create --body`):

```
## Summary

- **Item 24 — Video input:** live USB capture as a u_video sampler available to every scene; baked `__video__` layer scene + demo `video_glitch`; F4 → Audio → Device picker; `VID:OK/--/ST/ER` chip on the top bar.
- **Item 28a — Pi-gen detect + per-scene caps** (preparatory): `MANDLEROT_PI_GEN`/`MANDLEROT_RENDER_SCALE` env overrides; per-scene `internal_resolution` ignored on Pi 5 / Unknown; `min_pi_gen` scene filter; install-time `pi-gen.conf` systemd drop-in.

## Specs

- docs/superpowers/specs/2026-05-16-video-input-design.md
- .docs/ROADMAP-SPECS.md (28a section)

## Test plan

- [x] `cargo test --lib` — 232 green (added 14 new tests across video + audio modules)
- [x] `cargo run -- --smoke-frames 2` — exit 0 on macOS dev box
- [ ] Pi 3B+ field test with EasyCap dongle (deferred — needs hardware)
- [ ] Pi 5 field test (deferred — hardware blocked)

## Known v0 debt (documented in spec)

- `mem::forget(stop)` on capture thread → process-lifetime cleanup only
- No udev hot-plug push (5s NoDevice retry covers it)
- No GLES-side YUYV (CPU decode via nokhwa is fine at 1280×720 / 30 fps)
- Single capture device at a time
```

## How to resume immediately

The first message after `/clear` should be:

> Read `.docs/RESUME-video-input.md` for context. Branch `feat/video-input` is implementation-complete; please invoke `superpowers:finishing-a-development-branch` to integrate.

That single instruction is enough — this doc is the durable handoff.

## Useful one-liners

```bash
# State check
git status && git log --oneline main..HEAD | head -5

# Verify nothing regressed
~/.cargo/bin/cargo test --lib
~/.cargo/bin/cargo run -- --smoke-frames 2

# See the full feature diff vs main
git diff --stat main..feat/video-input

# Eyeball the spec + plan
ls docs/superpowers/{specs,plans}/2026-05-16-video-input-*

# Confirm baked __video__ + demo scene
ls scenes/video_glitch.* scenes/__video__* 2>&1 | grep -v "No such"
```

## Open follow-ups (post-merge, not blocking)

- **`refresh_stale` byte-cmp opacity** (Task 3 reviewer minor) — clarify with a comment or extract `is_status_active()` helper.
- **`VideoHandle::new()` visibility** (Task 3 reviewer minor) — could be `pub(crate)`.
- **`__video__` pre-compile parity** (Task 6 reviewer) — done in Task 10's wire-up, ✅.
- **Pi field validation** — both items 24 and 28a have manual-test plans gated on real hardware.
- **Item 28 (deferred)** — Pi 4+ shader headroom: `#version 300 es` prelude variant + real Gaussian bloom postfx tier. Hardware-blocked.

## Code-review feedback rolled up

Across all 14 tasks, two-stage review (spec compliance + code quality) produced these aggregate findings:

- **Critical:** none.
- **Important (fixed inline):** decode_errors unused-assignment warning (commit `3e6503a`); oversized-frame upload garbling (commit `4fb68ad`); transparent-vs-opaque texture init (commit `4fb68ad`).
- **Minor (deferred to post-merge):** see "Open follow-ups" above.
