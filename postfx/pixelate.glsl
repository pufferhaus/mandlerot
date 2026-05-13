// Chunky pixelation by floor-snapping the sample UV to a coarser grid.
// p0 cell — size of each block in pixels (clamped to >=1 so the shader can't
//           divide by zero when the user drags the param all the way down).
void main() {
    float cell = max(u_param0, 1.0);
    vec2 grid = u_resolution / cell;
    // Snap to the centre of each cell so sampling is stable across frames.
    vec2 snapped = (floor(v_uv * grid) + 0.5) / grid;
    gl_FragColor = texture2D(u_input, snapped);
}
