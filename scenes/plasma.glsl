void main() {
    vec2 p = (v_uv * 2.0 - 1.0) * vec2(u_resolution.x / u_resolution.y, 1.0);
    float t = u_time * (0.3 + u_param2);
    float v = 0.0;
    v += sin(p.x * (3.0 * u_param0) + t);
    v += sin(p.y * (3.0 * u_param0) + t * 1.3);
    v += sin((p.x + p.y) * 2.0 * u_param0 + t * 0.7);
    v += sin(length(p) * 4.0 * u_param0 - t);
    v *= 0.25;
    float h = u_param1 + 0.1 * v;
    vec3 color = 0.5 + 0.5 * cos(6.2831 * (h + vec3(0.0, 0.33, 0.66)));
    gl_FragColor = vec4(color, 1.0);
}
