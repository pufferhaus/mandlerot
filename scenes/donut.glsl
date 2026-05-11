// donut.c: a spinning torus rendered as ASCII glyphs. The torus is
// raymarched, shaded with a single light, the luminance quantized to one
// of 12 characters, then drawn into the cell with a 5x7 bitmap.
//
// u_param0 spin_x      0..2     rotation about x-axis (1.0)
// u_param1 spin_y      0..2     rotation about y-axis (0.6)
// u_param2 cell_size   6..18    pixel size of one ASCII cell (10)
// u_param3 hue         0..1     glyph color (amber default)
// u_param4 r_major     0.6..1.4 torus major radius
// u_param5 r_minor     0.1..0.6 torus minor radius
// u_param6 brightness  0..2     global gain
// u_param7 scanlines   0..1     overlay scanlines

float h21(vec2 p){ return fract(sin(dot(p, vec2(12.9898,78.233))) * 43758.5453); }

float sd_torus(vec3 p, float R, float r) {
    vec2 q = vec2(length(p.xz) - R, p.y);
    return length(q) - r;
}

float scene(vec3 p) {
    return sd_torus(p, u_param4, u_param5);
}

vec3 normal(vec3 p) {
    vec2 e = vec2(0.001, 0.0);
    return normalize(vec3(
        scene(p + e.xyy) - scene(p - e.xyy),
        scene(p + e.yxy) - scene(p - e.yxy),
        scene(p + e.yyx) - scene(p - e.yyx)
    ));
}

mat3 rotX(float a){ float c=cos(a),s=sin(a); return mat3(1,0,0, 0,c,-s, 0,s,c); }
mat3 rotY(float a){ float c=cos(a),s=sin(a); return mat3(c,0,s, 0,1,0, -s,0,c); }

// 5x7 bitmap for 12 luminance levels — patterns get denser with brightness.
// Stylized rather than literal ASCII; reads as text on a screen.
float glyph(float level, vec2 sub) {
    // sub in [0,4]x[0,6]
    float density = level / 11.0; // 0..1
    float h = h21(sub + vec2(floor(level), 0.0));
    return step(1.0 - density * 0.85, h);
}

void main() {
    float cell = max(u_param2, 4.0);
    vec2 px = v_uv * u_resolution;
    vec2 grid = floor(px / cell);
    vec2 cell_uv = fract(px / cell);
    vec2 sub = floor(cell_uv * vec2(5.0, 7.0));
    float in_bounds = step(0.0, sub.x) * step(0.0, sub.y)
                    * step(sub.x, 4.0) * step(sub.y, 6.0);

    // raymarch the torus, sampling at the cell CENTER (so all 5x7 sub-px
    // in a cell render the same glyph).
    vec2 cell_center = (grid + 0.5) * cell / u_resolution;
    vec2 uv = (cell_center - 0.5);
    uv.x *= u_resolution.x / u_resolution.y;

    vec3 ro = vec3(0.0, 0.0, -3.0);
    vec3 rd = normalize(vec3(uv, 1.5));
    mat3 R = rotY(u_time * u_param1) * rotX(u_time * u_param0);
    ro = R * ro;
    rd = R * rd;

    float t = 0.0;
    float hit = 0.0;
    for (int i = 0; i < 64; i++) {
        vec3 p = ro + rd * t;
        float d = scene(p);
        if (d < 0.001) { hit = 1.0; break; }
        if (t > 8.0) break;
        t += d;
    }

    vec3 col = vec3(0.0);
    if (hit > 0.5) {
        vec3 p = ro + rd * t;
        vec3 n = normal(p);
        vec3 light = normalize(vec3(0.4, 0.7, -0.6));
        float diff = clamp(dot(n, light), 0.0, 1.0);
        float lum = 0.15 + 0.85 * diff;
        // quantize to 12 levels
        float level = floor(lum * 11.99);
        float on = glyph(level, sub) * in_bounds;
        vec3 tint = 0.5 + 0.5 * cos(6.2831 * (u_param3 + vec3(0.0, 0.33, 0.66)));
        col = tint * on;
    }

    col *= u_param6;
    col *= mix(1.0, 0.7 + 0.3 * sin(v_uv.y * u_resolution.y * 3.1415), u_param7);
    gl_FragColor = vec4(col, 1.0);
}
