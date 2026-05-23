// Gray-Scott reaction-diffusion. State stored in alpha (V concentration);
// U is approximated as 1 - V so RGB is free for palette display.
//
// Display strategy: render the gradient magnitude of V (pattern edges) as
// the visible image. Even when V saturates to high values across the screen,
// the EDGES between regions stay sharp and glow. This prevents the
// "fills up" failure mode where the whole panel looks one color.
//
// du/dt = Du*lap(u) - u*v² + f*(1-u)
// dv/dt = Dv*lap(v) + u*v² - (f+k)*v
//
// Default (f, k) = (0.029, 0.057) is the "stripes" regime — spatially
// extended labyrinth patterns that flow and reorganize forever.
//
// u_param0  feed_rate    (0.01..0.10,  0.029)
// u_param1  kill_rate    (0.04..0.07,  0.057) audio_route=bass
// u_param2  hue          (0..1,        0.55)  audio_route=himid
// u_param3  reseed_beats (4..64,       12.0)
// u_param4  perturbation (0..1,        0.3)   audio_route=treble

float hash21(vec2 p) {
    return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453);
}

float v_at(vec2 uv) {
    return texture(u_prev, uv).a;
}

float lap_v(vec2 uv, vec2 px) {
    float c  = v_at(uv);
    float n  = v_at(uv + vec2(0.0,  px.y));
    float s  = v_at(uv + vec2(0.0, -px.y));
    float e  = v_at(uv + vec2( px.x, 0.0));
    float w  = v_at(uv + vec2(-px.x, 0.0));
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

    // Sparse seed: 3 small blobs at hashed positions. Patterns spread from
    // localized sources rather than a uniform noise field, which gives a
    // more dramatic emergence and longer time-to-saturation.
    float seed = 0.0;
    for (int i = 0; i < 3; i++) {
        float fi = float(i);
        vec2 center = vec2(hash21(vec2(fi, 1.0)), hash21(vec2(fi, 2.0)));
        seed = max(seed, step(length(v_uv - center), 0.025));
    }

    float v0 = v_at(v_uv);
    float u0 = 1.0 - v0;

    v0 = mix(v0, seed,        seed_gate);
    u0 = mix(u0, 1.0 - seed,  seed_gate);

    // Sparse perturbation only on strong beats; less likely to flood.
    float beat_noise = hash21(v_uv * u_resolution + floor(beats * 4.0));
    float beat_inject = step(0.995, beat_noise) * step(0.6, u_beat) * u_param4 * 0.2;
    v0 = min(1.0, v0 + beat_inject * (1.0 - seed_gate));

    // Reaction-diffusion. dt=0.5 for stability + slower spread.
    float lap = lap_v(v_uv, px);
    float lap_u = -lap;
    float uvv = u0 * v0 * v0;
    float du = 1.0 * lap_u - uvv + f * (1.0 - u0);
    float dv = 0.5 * lap   + uvv - (f + k) * v0;
    float vn = clamp(v0 + dv * 0.5, 0.0, 1.0);

    // Edge-based display: gradient magnitude of V highlights pattern boundaries.
    // Sample V at neighbor pixels to compute |grad V|.
    float v_e = v_at(v_uv + vec2( px.x, 0.0));
    float v_w = v_at(v_uv + vec2(-px.x, 0.0));
    float v_n = v_at(v_uv + vec2(0.0,  px.y));
    float v_s = v_at(v_uv + vec2(0.0, -px.y));
    float dvx = (v_e - v_w) * 0.5;
    float dvy = (v_n - v_s) * 0.5;
    float grad = length(vec2(dvx, dvy));
    // Edge intensity: sharp at high gradient, dark in interior regions.
    float edge = smoothstep(0.003, 0.04, grad);

    float hue_drift = beats / 32.0;
    // Hue varies slightly with V level so different stripe densities show
    // different colors.
    float palette_t = u_param2 + hue_drift + vn * 0.3;
    vec3 color = 0.5 + 0.5 * cos(6.2831 * (palette_t + vec3(0.0, 0.33, 0.66)));
    // Mix: edges fully colored; interiors get a tiny ambient glow so the
    // pattern shape is hinted at even where the gradient is zero.
    color *= edge + 0.05 * vn;

    fragColor = vec4(color, vn);
}
