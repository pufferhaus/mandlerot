// Bayer 4×4 ordered dither: quantise the input to `levels` steps per channel
// with a tiled threshold matrix nudging the rounding boundary up or down by
// 1/levels at each pixel. Classic 1-bit look at levels=2, 16-colour at
// levels=4, etc.
// p0 levels   — number of discrete steps per channel
// p1 strength — 0 = no dither, 1 = full bayer offset
//
// GLSL ES 1.00 has no integer bit-ops and no array indexing on uniforms.
// The 4×4 Bayer threshold matrix is laid out as four `vec4` rows that are
// selected with `step + mix` rather than an `if/else` cascade — VC4 has no
// branch predictor, so the prior 15-arm conditional cost ~6 ms/frame at
// 240×160 while this branchless form is a handful of ALU ops.
float bayer_t(float ix, float iy) {
    vec4 r0 = vec4( 0.0,  8.0,  2.0, 10.0);
    vec4 r1 = vec4(12.0,  4.0, 14.0,  6.0);
    vec4 r2 = vec4( 3.0, 11.0,  1.0,  9.0);
    vec4 r3 = vec4(15.0,  7.0, 13.0,  5.0);
    vec4 row01 = mix(r0, r1, step(0.5, iy));
    vec4 row23 = mix(r2, r3, step(2.5, iy));
    vec4 row   = mix(row01, row23, step(1.5, iy));
    vec2 c01 = mix(row.xy, row.zw, step(1.5, ix));
    return mix(c01.x, c01.y, step(0.5, mod(ix, 2.0)));
}

void main() {
    vec4 c = texture2D(u_input, v_uv);
    float levels = max(u_param0, 2.0);
    float strength = clamp(u_param1, 0.0, 1.0);
    vec2 px = floor(v_uv * u_resolution);
    float ix = mod(px.x, 4.0);
    float iy = mod(px.y, 4.0);
    float t = bayer_t(ix, iy);
    // Map t (0..15) to a signed bias around the quantisation step.
    float bias = (t / 16.0 - 0.5) / levels * strength;
    vec3 rgb = c.rgb + bias;
    rgb = floor(rgb * (levels - 1.0) + 0.5) / (levels - 1.0);
    gl_FragColor = vec4(rgb, c.a);
}
