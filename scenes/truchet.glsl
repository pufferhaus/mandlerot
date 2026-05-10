// u_param0  scale           (4..40, 16)   — tile density (integer-ish)
// u_param1  line_thickness  (0.005..0.05, 0.02) — arc line width
// u_param2  hue             (0..1, 0.7)   — base palette hue
// u_param3  anim_speed      (0..2, 0.4)   — per-cell color drift speed
// u_param4  thickness_pulse (0..1, 0.4)   — thickness audio modulation [bass 1.0]
//
// Truchet tiles: each grid cell gets a hash-selected quarter-circle pair.
// Even hash: arcs at NW+SE corners. Odd hash: arcs at NE+SW corners.
float trHash(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

float arcDist(vec2 local, vec2 corner) {
    return abs(length(local - corner) - 0.5);
}

void main() {
    float scale     = floor(u_param0 + 0.5);
    float thickness = u_param1 + u_param4 * 0.03;
    float hue       = u_param2;
    float anim_spd  = u_param3;

    vec2 uv = v_uv * scale;
    vec2 cell_id  = floor(uv);
    vec2 local    = fract(uv); // [0,1) within cell

    float h = trHash(cell_id);
    // Orientation: even=NW+SE arcs, odd=NE+SW arcs
    float on_arc;
    if (h < 0.5) {
        float d1 = arcDist(local, vec2(0.0, 0.0)); // NW corner
        float d2 = arcDist(local, vec2(1.0, 1.0)); // SE corner
        on_arc = min(d1, d2);
    } else {
        float d1 = arcDist(local, vec2(1.0, 0.0)); // NE corner
        float d2 = arcDist(local, vec2(0.0, 1.0)); // SW corner
        on_arc = min(d1, d2);
    }

    float line = 1.0 - smoothstep(thickness - 0.003, thickness + 0.003, on_arc);

    // Per-cell color with time drift
    float cell_hue = hue + trHash(cell_id + 0.5) * 0.4 + u_time * anim_spd * 0.05;
    vec3 fg = 0.5 + 0.5 * cos(6.2831 * (cell_hue + vec3(0.0, 0.33, 0.66)));
    vec3 bg = vec3(0.04);

    vec3 col = mix(bg, fg, line);
    gl_FragColor = vec4(col, 1.0);
}
