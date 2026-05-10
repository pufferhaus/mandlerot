// Gray-Scott reaction-diffusion. U+V stored in red/green channels.
// du/dt = Du*lap(u) - u*v^2 + f*(1-u)
// dv/dt = Dv*lap(v) + u*v^2 - (f+k)*v
// Two substeps per frame for stability.
//
// u_param0  feed_rate    (0.01..0.10,  0.055)
// u_param1  kill_rate    (0.04..0.07,  0.062) audio_route=bass
// u_param2  hue          (0..1,        0.45)
// u_param3  reseed_beats (4..64,       32)    full reset cadence
// u_param4  perturbation (0..1,        0.3)   noise injected on beats

float hash21(vec2 p) {
    return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453);
}

vec2 laplacian(vec2 uv, vec2 px) {
    vec2 c = texture2D(u_prev, uv).rg;
    vec2 n = texture2D(u_prev, uv + vec2(0.0,  px.y)).rg;
    vec2 s = texture2D(u_prev, uv + vec2(0.0, -px.y)).rg;
    vec2 e = texture2D(u_prev, uv + vec2( px.x, 0.0)).rg;
    vec2 w = texture2D(u_prev, uv + vec2(-px.x, 0.0)).rg;
    return (n + s + e + w) - 4.0 * c;
}

void main() {
    vec2 px = 1.0 / u_resolution;

    float f = u_param0;
    float k = u_param1;
    float Du = 1.0;
    float Dv = 0.5;

    float bpm  = u_bpm > 1.0 ? u_bpm : 120.0;
    float beats = u_time * bpm / 60.0;

    // Wipe gate: true for ~50 ms each cycle
    float in_wipe = step(mod(beats, u_param3), 0.1);
    float seed_gate = max(step(u_time, 0.5), in_wipe);

    // Blob mask for seeding: small circle at center
    vec2 c = v_uv - 0.5;
    float blob = step(length(c), 0.05);

    // Current state
    vec2 uv0 = texture2D(u_prev, v_uv).rg;
    float u0 = uv0.r;
    float v0 = uv0.g;

    // Seed: U=1 in bulk, V=1 in blob
    u0 = mix(u0, 1.0 - blob, seed_gate);
    v0 = mix(v0, blob,        seed_gate);

    // Beat perturbation: inject small V noise on beats
    float beat_noise = hash21(v_uv * u_resolution + floor(beats)) * u_param4;
    v0 = min(1.0, v0 + beat_noise * u_beat * (1.0 - seed_gate));

    // Substep 1
    vec2 lap = laplacian(v_uv, px);
    float uvv = u0 * v0 * v0;
    float du = Du * lap.r - uvv + f * (1.0 - u0);
    float dv = Dv * lap.g + uvv - (f + k) * v0;
    float u1 = clamp(u0 + du * 0.5, 0.0, 1.0);
    float v1 = clamp(v0 + dv * 0.5, 0.0, 1.0);

    // Substep 2
    uvv = u1 * v1 * v1;
    du = Du * lap.r - uvv + f * (1.0 - u1);
    dv = Dv * lap.g + uvv - (f + k) * v1;
    float un = clamp(u1 + du * 0.5, 0.0, 1.0);
    float vn = clamp(v1 + dv * 0.5, 0.0, 1.0);

    // Display: map V concentration to palette
    float t = vn;
    vec3 tint = 0.5 + 0.5 * cos(6.2831 * (u_param2 + t * 0.5 + vec3(0.0, 0.33, 0.66)));

    gl_FragColor = vec4(un, vn, dot(tint, vec3(0.333)), 1.0);
}
