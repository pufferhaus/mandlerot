// Pond ripples. Approximates a 2D wave equation by summing a handful of
// expanding circular wavefronts. Each wave has its own birth time + center
// hash-spawned on a slow tick or on beat. Refraction warps an underlying
// gradient so the surface looks watery.
//
// u_param0 drop_rate     0..2     wavefronts per second (1.0)
// u_param1 wave_speed    0.1..2   how fast rings expand (0.6)
// u_param2 wave_freq     5..40    spatial frequency of each ring (20)
// u_param3 damping       0.1..3   how fast amplitude decays with radius (1.2)
// u_param4 hue           0..1     water tint
// u_param5 refract       0..0.3   how much the surface bends light
// u_param6 beat_drops    0..1     extra drops on beat (0.6)
// u_param7 brightness    0..2     global gain

float h(vec2 p){ return fract(sin(dot(p, vec2(12.9898,78.233))) * 43758.5453); }

void main() {
    vec2 uv = (v_uv - 0.5) * vec2(u_resolution.x / u_resolution.y, 1.0);

    // accumulate wave displacement
    float disp = 0.0;
    const int N_WAVES = 8;
    for (int i = 0; i < N_WAVES; i++) {
        float fi = float(i);
        // each wave has its own emission time, cycled
        float period = 1.0 / max(u_param0, 0.1);
        float t_local = u_time + fi * period * 0.13 + u_param6 * u_beat * 0.2;
        float cycle = floor(t_local / period);
        float age = mod(t_local, period);
        // center spawned per cycle
        vec2 c = vec2(
            (h(vec2(fi, cycle)) - 0.5) * 1.6,
            (h(vec2(fi + 11.0, cycle)) - 0.5) * 1.0
        );
        float r = length(uv - c);
        float front = age * u_param1;
        // ring profile: cos within an annulus near `front`, fading with age and radius
        float profile = sin((r - front) * u_param2);
        float fade = exp(-u_param3 * r) * exp(-age * 2.0);
        // only contribute when the wave hasn't passed beyond r yet
        float gate = smoothstep(0.0, 0.05, front - r) * step(r, front);
        disp += profile * fade * gate;
    }

    // refract a base gradient: warp UV by gradient of disp (approximate)
    vec2 du = vec2(0.005, 0.0);
    float dx = disp; // sample at current
    vec2 warped_uv = v_uv + vec2(disp, disp * 0.7) * u_param5;

    // base gradient: subtle blue-cyan radial
    vec3 tint = 0.5 + 0.5 * cos(6.2831 * (u_param4 + vec3(0.0, 0.33, 0.66)));
    float r0 = length(warped_uv - 0.5);
    vec3 base = tint * (0.5 - 0.4 * r0);

    // highlights where disp is positive
    vec3 col = base + tint * 0.6 * max(disp, 0.0);
    col += vec3(1.0) * pow(max(disp, 0.0), 4.0) * 0.4; // specular sparkle
    col *= u_param7;
    gl_FragColor = vec4(clamp(col, 0.0, 1.5), 1.0);
}
