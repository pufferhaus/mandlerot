// Sailor-Moon transformation rings: concentric expanding rings from the
// center with sparkle particles inside, magical-girl palette.
//
// u_param0 ring_count     2..8       how many simultaneous rings (4)
// u_param1 expand_rate    0.1..2     how fast rings grow
// u_param2 ring_width     0.005..0.08 thickness of a ring
// u_param3 sparkle_count  4..64      sparkles per visible ring
// u_param4 palette        0..1       0=pink/cyan, 1=gold/violet
// u_param5 glow           0..1       extra inner glow
// u_param6 beat_trigger   0..1       rings reset to 0 on beat
// u_param7 brightness     0..2       global gain

float h(vec2 p){ return fract(sin(dot(p, vec2(12.9898,78.233))) * 43758.5453); }
float h1(float x){ return fract(sin(x * 12.9898) * 43758.5453); }

void main() {
    vec2 uv = v_uv - 0.5;
    uv.x *= u_resolution.x / u_resolution.y;
    float r = length(uv);
    float a = atan(uv.y, uv.x);

    vec3 col = vec3(0.0);

    // palette
    vec3 c1 = mix(vec3(1.0, 0.45, 0.75), vec3(1.0, 0.80, 0.30), u_param4); // pink → gold
    vec3 c2 = mix(vec3(0.55, 0.85, 1.0), vec3(0.70, 0.45, 1.0), u_param4); // cyan → violet

    // background gradient
    col = mix(c2 * 0.15, c1 * 0.25, smoothstep(0.0, 0.7, r));

    // rings: each ring has its own phase, expanding outward and fading
    float n = max(u_param0, 1.0);
    for (int i = 0; i < 8; i++) {
        if (float(i) >= n) break;
        float phase = mod(u_time * u_param1 + float(i) / n, 1.0);
        // reset toward 0 on beat: pull phase toward 0 proportional to u_beat
        phase = mix(phase, 0.0, u_param6 * u_beat * 0.6);
        float radius = phase * 0.75;
        float ring = smoothstep(u_param2, 0.0, abs(r - radius));
        float fade = 1.0 - phase; // older rings fade
        vec3 ring_color = mix(c1, c2, fract(float(i) * 0.5));
        col += ring_color * ring * fade * 0.9;
    }

    // sparkles: hash-randomized points inside the largest ring
    float sparkle_n = max(u_param3, 4.0);
    for (int j = 0; j < 64; j++) {
        if (float(j) >= sparkle_n) break;
        float jf = float(j);
        vec2 pos = vec2(
            (h1(jf * 1.7 + floor(u_time * 2.0)) - 0.5) * 1.6,
            (h1(jf * 3.1 + floor(u_time * 2.0) + 0.5) - 0.5) * 1.2
        );
        float life = fract(u_time * 1.5 + jf * 0.13);
        float pulse = sin(life * 3.1415);
        float d = length(uv - pos);
        col += c1 * pulse * smoothstep(0.012, 0.0, d) * 1.6;
        // 4-point star arms
        float arm = max(smoothstep(0.04, 0.0, abs(uv.x - pos.x)) * smoothstep(0.0025, 0.0, abs(uv.y - pos.y)),
                        smoothstep(0.04, 0.0, abs(uv.y - pos.y)) * smoothstep(0.0025, 0.0, abs(uv.x - pos.x)));
        col += c2 * pulse * arm * 0.8;
    }

    // inner glow
    col += c2 * u_param5 * (1.0 - smoothstep(0.0, 0.4, r));

    // angle-based sparkle haze
    col += 0.08 * c1 * (0.5 + 0.5 * sin(a * 12.0 + u_time));

    col *= u_param7;
    gl_FragColor = vec4(clamp(col, 0.0, 1.5), 1.0);
}
