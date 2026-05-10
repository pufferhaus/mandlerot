// u_param0  scale        (4..40, 12)  — cell density (integer-ish)
// u_param1  cell_jitter  (0..1, 0.8)  — 0=regular grid, 1=fully random offset
// u_param2  hue          (0..1, 0.55) — base palette hue [bass 0.2]
// u_param3  edge_intensity(0..1, 0.5) — brighten near cell boundaries
// u_param4  cell_anim    (0..2, 0.3)  — per-cell color cycle speed [lomid 0.4]
//
// Worley/Voronoi: 3x3 neighborhood search, color each pixel by nearest cell.
vec2 vrHash2(vec2 p) {
    p = vec2(dot(p, vec2(127.1, 311.7)), dot(p, vec2(269.5, 183.3)));
    return fract(sin(p) * 43758.5453);
}

void main() {
    float bpm = u_bpm > 1.0 ? u_bpm : 120.0;

    float scale    = u_param0;
    float jitter   = u_param1;
    float hue      = u_param2 + u_audio.x * 0.2;
    float edge_int = u_param3;
    float anim_spd = u_param4 + u_audio.y * 0.4;

    vec2 uv = v_uv * scale;
    vec2 cell_base = floor(uv);
    vec2 cell_frac = fract(uv);

    float minDist1 = 1e9;
    float minDist2 = 1e9;
    vec2  nearCell = vec2(0.0);

    for (int ny = -1; ny <= 1; ny++) {
        for (int nx = -1; nx <= 1; nx++) {
            vec2 neighbor = vec2(float(nx), float(ny));
            vec2 cell_id  = cell_base + neighbor;
            // Perturbed center in [0,1] within cell
            vec2 center = vrHash2(cell_id) * jitter + (1.0 - jitter) * 0.5;
            vec2 to_center = neighbor + center - cell_frac;
            float d = length(to_center);
            if (d < minDist1) {
                minDist2 = minDist1;
                minDist1 = d;
                nearCell = cell_id;
            } else if (d < minDist2) {
                minDist2 = d;
            }
        }
    }

    // Cell color: hash of nearest cell + time animation
    float time_bucket = u_time * anim_spd * 0.1;
    float h = fract(vrHash2(nearCell).x + time_bucket);
    float cell_hue = hue + h * 0.5;
    vec3 col = 0.5 + 0.5 * cos(6.2831 * (cell_hue + vec3(0.0, 0.33, 0.66)));

    // Edge intensity: bright near cell boundaries (where dist1 ~= dist2)
    float edge = 1.0 - smoothstep(0.0, 0.1, minDist2 - minDist1);
    col = mix(col, col + edge_int, edge * edge_int);

    // Distance darkening: slightly darker away from center
    float center_fade = 1.0 - minDist1 * 0.4;
    col *= clamp(center_fade, 0.3, 1.0);

    col = clamp(col, 0.0, 1.0);
    gl_FragColor = vec4(col, 1.0);
}
