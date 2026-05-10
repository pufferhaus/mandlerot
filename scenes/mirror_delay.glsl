// Mirror delay: fresh rotating shape + mirrored, decayed u_prev echo.
//
// u_param0  mirror_decay (0.5..0.99, 0.92) — echo persistence
// u_param1  shape_size   (0.05..0.4, 0.15)
// u_param2  shape_speed  (-2..2, 0.3) — rotation rate
// u_param3  hue          (0..1, 0.55) audio_route=lomid
// u_param4  mirror_axis  (0..1, 0.0) — 0=horizontal, 1=vertical

// Signed distance to a regular polygon (n sides)
float poly_sdf(vec2 p, float n, float r) {
    float angle = atan(p.y, p.x);
    float slice = 6.2831 / n;
    float a = mod(angle, slice) - slice * 0.5;
    return length(p) * cos(a) - r;
}

void main() {
    vec2 c = v_uv - 0.5;

    // Mirror sample: blend horizontal and vertical flip
    float axis = u_param4;
    vec2 flip_h = vec2(1.0 - v_uv.x, v_uv.y);
    vec2 flip_v = vec2(v_uv.x, 1.0 - v_uv.y);
    vec2 mirror_uv = mix(flip_h, flip_v, axis);
    vec3 echo = texture2D(u_prev, mirror_uv).rgb * u_param0;

    // Fresh rotating polygon
    float angle = u_time * u_param2;
    mat2 rot = mat2(cos(angle), -sin(angle), sin(angle), cos(angle));
    vec2 pc = rot * c;
    float sides = 6.0 + floor(u_audio.x * 4.0); // bass modulates sides
    float dist = poly_sdf(pc, sides, u_param1);
    float shape = 1.0 - smoothstep(-0.005, 0.005, dist);

    // Palette from hue + time drift
    float h = u_param3 + u_time * 0.05;
    vec3 tint = 0.5 + 0.5 * cos(6.2831 * (h + vec3(0.0, 0.33, 0.66)));
    float brightness = 1.0 + 2.0 * u_beat;
    vec3 fresh = shape * tint * brightness;

    gl_FragColor = vec4(echo + fresh, 1.0);
}
