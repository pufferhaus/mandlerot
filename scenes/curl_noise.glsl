// u_param0  flow_speed     (0..2, 0.5)  — advection strength [lomid 0.4]
// u_param1  turbulence     (1..6, 3)    — sin layer count (integer-ish)
// u_param2  hue            (0..1, 0.5)  — base palette hue
// u_param3  decay          (0.5..0.99, 0.95) — frame decay
// u_param4  inject_density (0..1, 0.3)  — fresh content per frame
//
// Divergence-free curl flow: flow = vec2(dP/dy, -dP/dx) via finite diff.
// Advects u_prev and injects audio-modulated dots.
float potential(vec2 p, float t, int layers) {
    float v = 0.0;
    float amp = 1.0;
    for (int i = 0; i < 6; i++) {
        if (i >= layers) break;
        float fi = float(i) + 1.0;
        v += amp * sin(p.x * fi * 1.3 + t * 0.7 + p.y * fi * 0.9)
                 + amp * sin(p.y * fi * 1.1 - t * 0.5 + p.x * fi * 1.2);
        amp *= 0.6;
    }
    return v;
}

void main() {
    float bpm = u_bpm > 1.0 ? u_bpm : 120.0;

    float flow_speed   = u_param0;
    int   layers       = int(floor(u_param1 + 0.5));
    float hue          = u_param2;
    float decay        = u_param3;
    float inject       = u_param4;

    float t = u_time * 0.4;
    float eps = 0.005;

    // Curl of potential P: flow = (dP/dy, -dP/dx)
    float p0  = potential(v_uv, t, layers);
    float px  = potential(v_uv + vec2(eps, 0.0), t, layers);
    float py  = potential(v_uv + vec2(0.0, eps), t, layers);
    float dpx = (px - p0) / eps;
    float dpy = (py - p0) / eps;
    vec2 flow = vec2(dpy, -dpx);

    // Advect previous frame
    float dt = flow_speed * 0.003;
    vec2 advect_uv = v_uv - flow * dt;
    advect_uv = clamp(advect_uv, 0.001, 0.999);
    vec3 prev = texture2D(u_prev, advect_uv).rgb * decay;

    // Fresh injection: audio-modulated colored dots from potential
    float bright_spot = smoothstep(0.6, 1.0, abs(p0) * 0.3 + u_audio.y * 0.5);
    vec3 fresh_col = 0.5 + 0.5 * cos(6.2831 * (hue + bright_spot * 0.3 + vec3(0.0, 0.33, 0.66)));
    float fresh = bright_spot * inject * (0.5 + u_beat * 0.5);

    vec3 col = mix(prev, fresh_col, fresh);
    col = clamp(col, 0.0, 1.0);
    gl_FragColor = vec4(col, 1.0);
}
