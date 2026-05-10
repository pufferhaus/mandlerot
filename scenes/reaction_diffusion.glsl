// Gray-Scott reaction-diffusion. State stored in alpha (V concentration);
// U is approximated as 1 - V so the RGB channels are free for palette display.
// This prevents the "always orange" failure mode where U=0.5, V=0.3 in R/G
// would dominate the visible color regardless of hue.
//
// du/dt = Du*lap(u) - u*v² + f*(1-u)
// dv/dt = Dv*lap(v) + u*v² - (f+k)*v
//
// Default (f, k) = (0.037, 0.06) is the "stripes / labyrinth" regime —
// patterns never settle to static spots; they flow and reorganize forever.
//
// u_param0  feed_rate    (0.01..0.10,  0.037)
// u_param1  kill_rate    (0.04..0.07,  0.060) audio_route=bass
// u_param2  hue          (0..1,        0.55)  audio_route=himid
// u_param3  reseed_beats (4..64,       12.0)  full reset cadence
// u_param4  perturbation (0..1,        0.4)   noise injection on beats / treble

float hash21(vec2 p) {
    return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453);
}

float v_at(vec2 uv) {
    return texture2D(u_prev, uv).a;
}

float lap_v(vec2 uv, vec2 px) {
    float c = v_at(uv);
    float n = v_at(uv + vec2(0.0,  px.y));
    float s = v_at(uv + vec2(0.0, -px.y));
    float e = v_at(uv + vec2( px.x, 0.0));
    float w = v_at(uv + vec2(-px.x, 0.0));
    // Diagonals at lower weight for a 9-tap Laplacian — sharper edges than 5-tap.
    float ne = v_at(uv + vec2( px.x,  px.y));
    float nw = v_at(uv + vec2(-px.x,  px.y));
    float se = v_at(uv + vec2( px.x, -px.y));
    float sw = v_at(uv + vec2(-px.x, -px.y));
    return 0.2 * (n + s + e + w) + 0.05 * (ne + nw + se + sw) - 1.0 * c;
}

void main() {
    vec2 px = 1.0 / u_resolution;

    float f = u_param0;
    float k = u_param1;

    float bpm   = u_bpm > 1.0 ? u_bpm : 120.0;
    float beats = u_time * bpm / 60.0;

    float in_wipe   = step(mod(beats, u_param3), 0.15);
    float seed_gate = max(step(u_time, 0.5), in_wipe);

    // Multi-blob seed: 7 blobs at hashed positions across the screen so the
    // reaction has multiple foci and the pattern emerges with spatial variety.
    float seed = 0.0;
    for (int i = 0; i < 7; i++) {
        float fi = float(i);
        vec2 center = vec2(hash21(vec2(fi, 1.0)), hash21(vec2(fi, 2.0)));
        seed = max(seed, step(length(v_uv - center), 0.04));
    }

    float v0 = v_at(v_uv);
    float u0 = 1.0 - v0; // U≈1-V approximation; lets RGB carry palette display

    // Apply seed during seed_gate
    v0 = mix(v0, seed,        seed_gate);
    u0 = mix(u0, 1.0 - seed,  seed_gate);

    // Beat perturbation: inject sparse V noise so patterns keep reorganizing.
    float beat_noise = hash21(v_uv * u_resolution + floor(beats * 4.0));
    float beat_inject = step(0.97, beat_noise) * u_beat * u_param4 * 0.3;
    v0 = min(1.0, v0 + beat_inject * (1.0 - seed_gate));

    // Reaction-diffusion step (single full step; Du=1.0, Dv=0.5, dt=1.0)
    float lap = lap_v(v_uv, px);
    // For U we use the inverse approximation, but its laplacian is just -lap_v.
    float lap_u = -lap;
    float uvv = u0 * v0 * v0;
    float du = 1.0 * lap_u - uvv + f * (1.0 - u0);
    float dv = 0.5 * lap   + uvv - (f + k) * v0;
    float vn = clamp(v0 + dv, 0.0, 1.0);
    // un is implied by 1 - vn for display purposes; we don't need to store it.

    // Display: palette of V with BPM-locked hue drift, audio-modulated.
    float hue_drift = beats / 48.0;
    float palette_t = u_param2 + hue_drift + vn * 0.6;
    vec3 color = 0.5 + 0.5 * cos(6.2831 * (palette_t + vec3(0.0, 0.33, 0.66)));
    // Modulate brightness by V so empty regions are dark, populated regions glow.
    color *= 0.15 + 0.85 * smoothstep(0.05, 0.55, vn);

    // RGB = display color; alpha = state for next frame
    gl_FragColor = vec4(color, vn);
}
