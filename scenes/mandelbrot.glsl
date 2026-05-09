void main() {
    float zoom = u_param0;
    vec2 center = vec2(u_param1, u_param2);
    float aspect = u_resolution.x / u_resolution.y;
    vec2 uv = (v_uv - 0.5) * vec2(aspect, 1.0) * (4.0 / zoom) + center;
    vec2 z = vec2(0.0);
    int max_iter = int(u_param3);
    int i;
    for (i = 0; i < 256; i++) {
        if (i >= max_iter) break;
        z = vec2(z.x*z.x - z.y*z.y, 2.0 * z.x * z.y) + uv;
        if (dot(z, z) > 4.0) break;
    }
    float t = float(i) / float(max_iter);
    vec3 color = 0.5 + 0.5 * cos(6.2831 * (t + u_param4 + vec3(0.0, 0.33, 0.66)));
    if (i >= max_iter) color = vec3(0.0);
    gl_FragColor = vec4(color, 1.0);
}
