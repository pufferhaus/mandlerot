// u_param0  ball_count    (2..10, 6)    — number of metaballs (integer-ish, max 10)
// u_param1  radius_scale  (0.05..0.3, 0.12) — blob size [bass 0.4]
// u_param2  hue           (0..1, 0.7)   — base hue
// u_param3  threshold     (0.5..3.0, 1.5) — field strength for surface isoline
// u_param4  motion_speed  (0..2, 0.6)   — metaball orbit speed
//
// 6-10 metaballs summing 1/r^2 field; threshold creates smooth merged blobs.
// Color shifts with total field strength above threshold.
void main() {
    float bpm = u_bpm > 1.0 ? u_bpm : 120.0;

    int   count    = int(u_param0);
    float rscale   = u_param1; // toml routes bass to radius_scale
    float hue      = u_param2 + u_time * bpm / (60.0 * 32.0); // BPM-locked drift
    float thresh   = u_param3;
    float speed    = u_param4;

    // Metaball centers in [0,1] space — unique (a,b,p) per ball via constants
    // Work in [-0.5,0.5] normalized space
    vec2 uv = v_uv - 0.5;

    float field = 0.0;

    // Ball 0
    if (0 < count) {
        vec2 c = vec2(sin(u_time * speed * 1.00 + 0.0), cos(u_time * speed * 0.77 + 0.0)) * 0.35;
        float r2 = dot(uv - c, uv - c);
        field += rscale * rscale / max(r2, 1e-5);
    }
    // Ball 1
    if (1 < count) {
        vec2 c = vec2(sin(u_time * speed * 0.83 + 1.1), cos(u_time * speed * 1.23 + 2.2)) * 0.35;
        float r2 = dot(uv - c, uv - c);
        field += rscale * rscale / max(r2, 1e-5);
    }
    // Ball 2
    if (2 < count) {
        vec2 c = vec2(sin(u_time * speed * 1.37 + 2.3), cos(u_time * speed * 0.61 + 1.5)) * 0.35;
        float r2 = dot(uv - c, uv - c);
        field += rscale * rscale / max(r2, 1e-5);
    }
    // Ball 3
    if (3 < count) {
        vec2 c = vec2(sin(u_time * speed * 0.71 + 3.7), cos(u_time * speed * 1.51 + 0.9)) * 0.35;
        float r2 = dot(uv - c, uv - c);
        field += rscale * rscale / max(r2, 1e-5);
    }
    // Ball 4
    if (4 < count) {
        vec2 c = vec2(sin(u_time * speed * 1.17 + 5.0), cos(u_time * speed * 0.93 + 3.1)) * 0.35;
        float r2 = dot(uv - c, uv - c);
        field += rscale * rscale / max(r2, 1e-5);
    }
    // Ball 5
    if (5 < count) {
        vec2 c = vec2(sin(u_time * speed * 0.59 + 1.9), cos(u_time * speed * 1.43 + 4.7)) * 0.35;
        float r2 = dot(uv - c, uv - c);
        field += rscale * rscale / max(r2, 1e-5);
    }
    // Ball 6
    if (6 < count) {
        vec2 c = vec2(sin(u_time * speed * 1.29 + 0.7), cos(u_time * speed * 0.47 + 2.6)) * 0.35;
        float r2 = dot(uv - c, uv - c);
        field += rscale * rscale / max(r2, 1e-5);
    }
    // Ball 7
    if (7 < count) {
        vec2 c = vec2(sin(u_time * speed * 0.91 + 4.2), cos(u_time * speed * 1.11 + 1.3)) * 0.35;
        float r2 = dot(uv - c, uv - c);
        field += rscale * rscale / max(r2, 1e-5);
    }
    // Ball 8
    if (8 < count) {
        vec2 c = vec2(sin(u_time * speed * 1.53 + 3.0), cos(u_time * speed * 0.67 + 5.5)) * 0.35;
        float r2 = dot(uv - c, uv - c);
        field += rscale * rscale / max(r2, 1e-5);
    }
    // Ball 9
    if (9 < count) {
        vec2 c = vec2(sin(u_time * speed * 0.79 + 6.1), cos(u_time * speed * 1.33 + 2.9)) * 0.35;
        float r2 = dot(uv - c, uv - c);
        field += rscale * rscale / max(r2, 1e-5);
    }

    // Smooth threshold: field >= thresh → inside blob
    float blob = smoothstep(thresh * 0.6, thresh, field);
    // Color shifts with field excess
    float excess = clamp((field - thresh) / thresh, 0.0, 1.0);
    float col_hue = hue + excess * 0.2 + u_beat * 0.05;
    vec3 col = 0.5 + 0.5 * cos(6.2831 * (col_hue + vec3(0.0, 0.33, 0.66)));

    gl_FragColor = vec4(col * blob, 1.0);
}
