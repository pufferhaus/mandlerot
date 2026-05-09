void main() {
    float v = 0.05 + 0.05 * sin(u_time * 0.5);
    gl_FragColor = vec4(v, v * 0.6, 0.0, 1.0);
}
