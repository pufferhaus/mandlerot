// u_param0  triad_brightness (0..2, 1.0)  — overall brightness scale
// u_param1  scanline_strength (0..1, 0.5) — CRT scanline darkening
// u_param2  phosphor_glow    (0..1, 0.3)  — RGB bleed between triads
// u_param3  source_pattern   (0..1, 0.5)  — 0=pure audio, 1=mix plasma base
// u_param4  saturation       (0..2, 1.5)  — color saturation
//
// RGB-triad CRT phosphor: each pixel column mod 3 is R, G, or B.
// Audio bands drive brightness per triad. Scanline darkening adds CRT feel.
void main() {
    float bpm = u_bpm > 1.0 ? u_bpm : 120.0;

    float triad_bright  = u_param0;
    float scanline_str  = u_param1;
    float glow          = u_param2;
    float src_mix       = u_param3;
    float saturation    = u_param4;

    float px = floor(v_uv.x * u_resolution.x);
    float py = floor(v_uv.y * u_resolution.y);
    float triad = mod(px, 3.0);

    // Audio band per sub-pixel: bass=R, (lomid+himid)*0.5=G, treble=B
    float r_band = u_audio.x;
    float g_band = u_audio.y * 0.6 + u_audio.z * 0.4;
    float b_band = u_audio.w;

    // Plasma-like base for src_mix>0
    vec2 p = v_uv * 2.0 - 1.0;
    float t = u_time * 0.3;
    float plasma = 0.5 + 0.5 * sin(p.x * 3.0 + t) * sin(p.y * 2.0 - t * 0.7);

    // Sub-pixel brightness: select band by triad column
    float r_src = mix(r_band, plasma, src_mix);
    float g_src = mix(g_band, plasma * 0.9, src_mix);
    float b_src = mix(b_band, plasma * 1.1, src_mix);

    // Select brightness for this column; glow bleeds neighbors
    float r_val = (triad < 0.5) ? r_src : r_src * glow;
    float g_val = (triad > 0.5 && triad < 1.5) ? g_src : g_src * glow;
    float b_val = (triad > 1.5) ? b_src : b_src * glow;

    vec3 col = vec3(r_val, g_val, b_val);

    // Scanline darkening: every other pixel row
    float scanline = 1.0 - scanline_str * 0.4 * mod(floor(py * 0.5), 2.0);
    col *= scanline;

    // Beat flash: horizontal bright bar sweeps on beat
    float bar_y = fract(u_time * bpm / 60.0 * 0.25);
    float bar = smoothstep(0.01, 0.0, abs(v_uv.y - bar_y)) * u_beat * 0.3;
    col += bar * vec3(0.8, 0.9, 1.0);

    // Saturation
    float luma = dot(col, vec3(0.2126, 0.7152, 0.0722));
    col = mix(vec3(luma), col, saturation);

    col *= triad_bright;
    col = clamp(col, 0.0, 1.0);
    gl_FragColor = vec4(col, 1.0);
}
