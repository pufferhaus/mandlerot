// Conway's Game of Life — strict binary rules.
//
// Texture filtering on the FBO is LINEAR, so neighbor samples must be
// thresholded to binary or the rule arithmetic gets fuzzy and cells fail to
// die from overpopulation. Trail rendering is a *display-only* effect on the
// output; the canonical state lives in the red channel as a hard 0 or 1.
//
// To keep the simulation moving without saturating:
//   - Sparse continuous seeding (small per-frame drip, not full-screen
//     noise on every beat).
//   - Periodic wipes wipe_beats beats (default 8) — short and clean.
//   - No glider stamping on bass: it dumps too many cells into a region
//     that's already alive, accelerating overpopulation collapse.
//
// u_param0  seed_density   (0..1, default 0.25) — initial + per-wipe noise
// u_param1  drip_rate      (0..0.001, default 0.0001) — per-frame fresh cells
// u_param2  hue            (0..1, default 0.3)
// u_param3  trail_alpha    (0..0.95, default 0.6) — display-only fade for dying cells
// u_param4  wipe_beats     (4..64, default 8) — full reset cadence

float hash21(vec2 p) {
    return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453);
}

float n_at(vec2 uv) {
    // Threshold to binary so LINEAR texture filtering doesn't smear the rules.
    return step(0.5, texture2D(u_prev, uv).r);
}

void main() {
    vec2 px = 1.0 / u_resolution;
    vec2 gp = floor(v_uv * u_resolution);

    float bpm = u_bpm > 1.0 ? u_bpm : 120.0;
    float beats = u_time * bpm / 60.0;
    // ~50ms wipe window per cycle (in_wipe = 1 inside, 0 outside)
    float in_wipe = step(mod(beats, u_param4), 0.1);

    // 8-neighbour binary sum
    float n = 0.0;
    n += n_at(v_uv + vec2(-px.x, -px.y)) + n_at(v_uv + vec2(0.0, -px.y));
    n += n_at(v_uv + vec2( px.x, -px.y)) + n_at(v_uv + vec2(-px.x, 0.0));
    n += n_at(v_uv + vec2( px.x,  0.0)) + n_at(v_uv + vec2(-px.x, px.y));
    n += n_at(v_uv + vec2( 0.0,   px.y)) + n_at(v_uv + vec2( px.x, px.y));

    float self_alive = n_at(v_uv);

    // During wipe: force everything dead so the seed-gate fully redraws.
    n          *= (1.0 - in_wipe);
    self_alive *= (1.0 - in_wipe);

    // Strict Conway: born on exactly 3, survive on 2 or 3.
    float born    = step(2.5, n) * step(n, 3.5);
    float survive = step(1.5, n) * step(n, 3.5);
    float next_alive = self_alive * survive + (1.0 - self_alive) * born;
    next_alive = step(0.5, next_alive); // hard binary

    // Seed phase (first second) and every wipe re-injects density noise.
    float t_seed = step(u_time, 1.0);
    float seed_gate = max(t_seed, in_wipe);
    float seed = step(1.0 - u_param0, hash21(gp + floor(u_time * 60.0)));
    next_alive = max(next_alive, seed * seed_gate);

    // Sparse continuous drip: tiny rate of fresh births anywhere on the board
    // so the population never reaches zero between wipes. drip_rate is
    // per-pixel-per-frame probability.
    float drip = step(1.0 - u_param1, hash21(gp + vec2(u_time, u_time * 1.7)));
    next_alive = max(next_alive, drip * (1.0 - in_wipe));

    // Beat pulse: a small (~0.5%) extra drip on beats so the rhythm shows
    // up as visible new births without flooding.
    float beat_drip = step(1.0 - 0.005 * u_beat, hash21(gp + vec2(u_time * 3.0, u_time)));
    next_alive = max(next_alive, beat_drip * (1.0 - in_wipe));

    next_alive = step(0.5, next_alive);

    // Display: render living cells full-bright; render dying cells (was alive
    // last frame, dead this frame) faded by trail_alpha. Trail is display-only;
    // the red-channel-as-state we write back must remain binary.
    float was_alive = self_alive;
    float dying = was_alive * (1.0 - next_alive);
    float display = max(next_alive, dying * u_param3);

    vec3 tint = 0.5 + 0.5 * cos(6.2831 * (u_param2 + vec3(0.0, 0.33, 0.66)));
    // Red channel = canonical state for next frame's neighbour counts.
    // Green/blue = visual tint, can hold display fade.
    gl_FragColor = vec4(next_alive, tint.g * display, tint.b * display, 1.0);
}
