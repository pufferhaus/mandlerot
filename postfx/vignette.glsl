// Radial darkening at the edges of the frame.
// p0 strength  — 0..1 (0 = invisible, 1 = corners go black)
// p1 radius    — distance from centre where the darkening starts
// p2 softness  — width of the falloff band (1 = wide gradient, 0 = hard edge)
void main() {
    vec4 c = texture2D(u_input, v_uv);
    float strength = u_param0;
    float radius   = u_param1;
    float soft     = max(u_param2, 1e-3);
    // 2-D distance from centre, normalised so x and y both span [-1, 1].
    vec2 d = v_uv - 0.5;
    float r = length(d) * 2.0;
    // 0 in centre, 1 past the radius+softness ring.
    float mask = smoothstep(radius, radius + soft, r) * strength;
    gl_FragColor = vec4(c.rgb * (1.0 - mask), c.a);
}
