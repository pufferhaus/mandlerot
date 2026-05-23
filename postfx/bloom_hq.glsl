// Wide single-pass bloom for Pi4+. 13 taps: center + 4 inner cardinal +
// 4 diagonal + 4 outer cardinal. More diffuse halo than bloom.glsl (8 taps).
// p0 threshold  — luma cutoff for bright-pass extract
// p1 intensity  — additive blend strength
// p2 radius     — inner ring radius (outer ring = 2.3x)

vec3 bright_pass(vec4 c, float t) {
    float lum = dot(c.rgb, vec3(0.299, 0.587, 0.114));
    return c.rgb * max(lum - t, 0.0);
}

void main() {
    float threshold = u_param0;
    float intensity = u_param1;
    float r1 = max(u_param2, 1e-4);
    float r2 = r1 * 2.3;
    float d1 = r1 * 0.7071;

    vec4 c = texture2D(u_input, v_uv);

    // Center (0.15) + inner cardinal x4 (0.10) + diagonal x4 (0.075) + outer cardinal x4 (0.05)
    vec3 bloom = bright_pass(c, threshold) * 0.15;
    bloom += bright_pass(texture2D(u_input, v_uv + vec2( r1, 0.0)), threshold) * 0.10;
    bloom += bright_pass(texture2D(u_input, v_uv + vec2(-r1, 0.0)), threshold) * 0.10;
    bloom += bright_pass(texture2D(u_input, v_uv + vec2(0.0,  r1)), threshold) * 0.10;
    bloom += bright_pass(texture2D(u_input, v_uv + vec2(0.0, -r1)), threshold) * 0.10;
    bloom += bright_pass(texture2D(u_input, v_uv + vec2( d1,  d1)), threshold) * 0.075;
    bloom += bright_pass(texture2D(u_input, v_uv + vec2(-d1,  d1)), threshold) * 0.075;
    bloom += bright_pass(texture2D(u_input, v_uv + vec2( d1, -d1)), threshold) * 0.075;
    bloom += bright_pass(texture2D(u_input, v_uv + vec2(-d1, -d1)), threshold) * 0.075;
    bloom += bright_pass(texture2D(u_input, v_uv + vec2( r2, 0.0)), threshold) * 0.05;
    bloom += bright_pass(texture2D(u_input, v_uv + vec2(-r2, 0.0)), threshold) * 0.05;
    bloom += bright_pass(texture2D(u_input, v_uv + vec2(0.0,  r2)), threshold) * 0.05;
    bloom += bright_pass(texture2D(u_input, v_uv + vec2(0.0, -r2)), threshold) * 0.05;

    bloom *= intensity;
    gl_FragColor = vec4(c.rgb + bloom, c.a);
}
