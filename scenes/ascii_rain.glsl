// Matrix-style "ASCII rain". The screen is divided into a coarse cell
// grid; each column has a falling rain head whose brightness fades up
// the column. Per-cell glyphs are faked via a hash that flips on/off,
// giving a flickering character-grid look without a font atlas.
float hash11(float x) {
    return fract(sin(x * 12.9898) * 43758.5453);
}
float hash21(vec2 p) {
    return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453);
}

void main() {
    float cell = 8.0;
    vec2 grid = floor(v_uv * u_resolution / cell);
    float col = grid.x;

    // Each column gets its own speed so the field rains at mixed cadences.
    float rows = u_resolution.y / cell;
    float speed = (1.0 + 2.0 * hash11(col)) * u_param0;
    // Head position (flipped: y=0 is bottom in UV, but rain falls down).
    float head_y = mod(u_time * speed * rows + hash11(col + 17.0) * rows, rows);
    // Distance from head, working in "cells below head" coordinates.
    // Since v_uv.y=0 is bottom, the head is "above" cells whose grid.y
    // is less than head_y; flip the sense so dist >= 0 means "in trail".
    float dist_from_head = head_y - grid.y;

    // Wrap distance for the trail to handle the modular head wrap-around.
    float dist_wrapped = mod(dist_from_head, rows);

    // Trail brightness fades exponentially up the column; trail_decay
    // controls the fade rate. Negative dist (above head) is dark.
    float fade_rate = max(u_param3, 0.01);
    float bright = exp(-dist_wrapped * fade_rate);
    // Suppress the cell directly above the head so the trail looks falling.
    bright *= step(0.0, dist_from_head + 0.0001) * step(dist_wrapped, rows * 0.6);

    // Per-cell glyph: a 0/1 pattern that flickers on a coarse time slice
    // so it looks like characters cycling. Density param controls how
    // many cells are "lit" inside a column.
    float ch = hash21(grid + floor(u_time * 8.0));
    float density = u_param1;
    ch = step(1.0 - density, ch);

    // Head cell is brighter / whiter; the rest takes the hue tint.
    float at_head = step(dist_wrapped, 0.5);
    float hue = u_param2;
    vec3 tint = 0.5 + 0.5 * cos(6.2831 * (hue + vec3(0.0, 0.33, 0.66)));
    vec3 trail_color = tint * bright * ch;
    vec3 head_color = mix(tint, vec3(1.0), 0.7) * ch;
    vec3 color = mix(trail_color, head_color, at_head);

    gl_FragColor = vec4(color, 1.0);
}
