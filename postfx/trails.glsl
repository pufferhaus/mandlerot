// Phosphor-style trails: keep the brighter of the current pixel or last
// frame's pixel scaled by `decay`. Combined with an optional uv smear, this
// gives motion-blur / persistence-of-vision look.
// p0 decay  — 0..0.99, how much of last frame survives into this one (long trails near 1)
// p1 smear  — uv shift applied when sampling `u_prev` (0 = no smear, 0.02 ≈ 5px at 240w)
// p2 mix    — 0..1 blend between "max" (phosphor) and "linear mix" (smooth crossfade)
void main() {
    vec4 cur = texture2D(u_input, v_uv);
    float decay  = clamp(u_param0, 0.0, 0.99);
    vec2  smear  = vec2(u_param1 * 0.5, u_param1 * 0.5);
    float blend  = clamp(u_param2, 0.0, 1.0);
    vec4 prev = texture2D(u_prev, v_uv - smear);
    vec3 prev_dim = prev.rgb * decay;
    vec3 phosphor = max(cur.rgb, prev_dim);
    vec3 linear   = mix(cur.rgb, prev_dim, decay);
    vec3 out_rgb  = mix(phosphor, linear, blend);
    gl_FragColor = vec4(out_rgb, 1.0);
}
