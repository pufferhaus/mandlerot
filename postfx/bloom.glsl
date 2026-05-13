// Single-pass approximated bloom: bright-pass extract + 8-tap radial blur,
// added back over the source. Cheap on VC4 — 9 texture fetches per pixel at
// 240×160 = ~350k taps/frame which sits well under budget.
// p0 threshold — luma cutoff above which a pixel contributes to bloom
// p1 intensity — how much of the bloom is summed back over the source
// p2 radius    — sample radius in uv-space (0.015 ≈ 4px at 240w)

vec3 bright_pass(vec4 c, float threshold) {
    float lum = dot(c.rgb, vec3(0.299, 0.587, 0.114));
    float mask = max(lum - threshold, 0.0);
    return c.rgb * mask;
}

void main() {
    float threshold = u_param0;
    float intensity = u_param1;
    float radius    = max(u_param2, 1e-4);

    vec4 c = texture2D(u_input, v_uv);

    vec3 bloom = vec3(0.0);
    // Cardinal taps at full radius
    bloom += bright_pass(texture2D(u_input, v_uv + vec2( radius, 0.0)), threshold);
    bloom += bright_pass(texture2D(u_input, v_uv + vec2(-radius, 0.0)), threshold);
    bloom += bright_pass(texture2D(u_input, v_uv + vec2(0.0,  radius)), threshold);
    bloom += bright_pass(texture2D(u_input, v_uv + vec2(0.0, -radius)), threshold);
    // Diagonal taps at ~0.7 × radius so the ring stays a circle, not a square
    float d = radius * 0.7071;
    bloom += bright_pass(texture2D(u_input, v_uv + vec2( d,  d)), threshold);
    bloom += bright_pass(texture2D(u_input, v_uv + vec2(-d,  d)), threshold);
    bloom += bright_pass(texture2D(u_input, v_uv + vec2( d, -d)), threshold);
    bloom += bright_pass(texture2D(u_input, v_uv + vec2(-d, -d)), threshold);
    bloom *= intensity / 8.0;

    gl_FragColor = vec4(c.rgb + bloom, c.a);
}
