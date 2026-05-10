// u_param0  a_freq       (0.5..5) — X frequency ratio
// u_param1  b_freq       (0.5..5) — Y frequency ratio
// u_param2  phase        (0..1)   — phase offset (0.25 = pi/2)
// u_param3  glow_radius  (0.005..0.05) — phosphor blob radius [treble]
// u_param4  hue          (0..1)   — base hue
//
// Lissajous curve rendered as summed Gaussian globs over ~80 t samples.
// BPM drives a slow phase animation; treble tightens the glow radius.
void main() {
    float aspect = u_resolution.x / u_resolution.y;
    vec2 uv = (v_uv - 0.5) * vec2(aspect, 1.0);

    float bpm = u_bpm > 1.0 ? u_bpm : 120.0;
    float phase_anim = u_time * bpm / 60.0 * 0.25; // quarter-beat drift

    float a = u_param0;
    float b = u_param1;
    float phi = u_param2 * 6.2831 + phase_anim;
    float radius = u_param3;
    float hue = u_param4;

    // Accumulate glow from N curve samples
    float glow = 0.0;
    float N = 80.0;
    float r2 = radius * radius * aspect * aspect;
    for (int i = 0; i < 80; i++) {
        float t = 6.2831 * float(i) / N;
        vec2 p = vec2(sin(a * t + phi), sin(b * t)) * 0.45;
        vec2 d = uv - p;
        d.x /= aspect;
        glow += exp(-dot(d, d) / (radius * radius));
    }
    glow /= N;
    glow = clamp(glow * 4.0, 0.0, 1.0);

    vec3 col = 0.5 + 0.5 * cos(6.2831 * (hue + vec3(0.0, 0.33, 0.66)));
    // Phosphor: bright core, dim halo tinted green
    vec3 phosphor = mix(vec3(0.0), col + vec3(0.0, 0.2, 0.0) * glow, glow);
    gl_FragColor = vec4(phosphor, 1.0);
}
