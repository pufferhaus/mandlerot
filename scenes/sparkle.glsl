// Sparkle burst: scattered 4-point twinkle stars on a dark backdrop. Each
// sparkle has independent lifecycle; bass spikes spawn extra bright ones.
//
// u_param0 count        16..256   number of sparkles tracked at once (96)
// u_param1 lifetime     0.3..3    seconds for one sparkle cycle (1.2)
// u_param2 size         0.005..0.08 base spike length
// u_param3 hue          0..1      sparkle color
// u_param4 bg_glow      0..1      backdrop hue brightness
// u_param5 motion       0..1      jitter motion (positions drift)
// u_param6 beat_boost   0..1      extra brightness on bass hit
// u_param7 brightness   0..2      global gain

float h1(float x){ return fract(sin(x * 12.9898) * 43758.5453); }

void main() {
    vec2 uv = v_uv - 0.5;
    uv.x *= u_resolution.x / u_resolution.y;

    vec3 col = vec3(0.0);
    vec3 tint = 0.5 + 0.5 * cos(6.2831 * (u_param3 + vec3(0.0, 0.33, 0.66)));

    // subtle backdrop
    float r = length(uv);
    col += tint * u_param4 * 0.06 * (1.0 - smoothstep(0.0, 0.9, r));

    float n = max(u_param0, 4.0);
    float life = max(u_param1, 0.1);
    for (int i = 0; i < 256; i++) {
        if (float(i) >= n) break;
        float fi = float(i);
        // each sparkle's phase is offset so spawns are spread out
        float t_local = u_time / life + fi * 0.137;
        float cycle = floor(t_local);
        float phase = fract(t_local);

        // randomize position per cycle (so the sparkle "moves" between births)
        float drift = u_param5;
        vec2 pos = vec2(
            (h1(fi * 1.7 + cycle) - 0.5) * 1.7,
            (h1(fi * 3.1 + cycle + 0.5) - 0.5) * 1.2
        );
        pos += drift * 0.1 * vec2(sin(u_time + fi), cos(u_time + fi * 2.0));

        // bell-shaped intensity profile over lifetime
        float intensity = sin(phase * 3.1415);
        intensity *= 0.7 + 0.3 * h1(fi * 5.3 + cycle); // per-sparkle scale jitter
        intensity = max(intensity, 0.0);

        vec2 d = uv - pos;
        float dist = length(d);
        float core = smoothstep(0.01, 0.0, dist);

        // 4-point star: two thin perpendicular bars
        float bar_w = u_param2 * 0.25;
        float bar_l = u_param2 * (1.0 + intensity * 0.5);
        float h_bar = smoothstep(bar_l, 0.0, abs(d.x))
                    * smoothstep(bar_w, 0.0, abs(d.y));
        float v_bar = smoothstep(bar_l, 0.0, abs(d.y))
                    * smoothstep(bar_w, 0.0, abs(d.x));

        col += tint * intensity * (core * 1.5 + (h_bar + v_bar) * 0.8);
    }

    col += tint * u_param6 * u_beat * 0.3;
    col *= u_param7;
    gl_FragColor = vec4(clamp(col, 0.0, 1.5), 1.0);
}
