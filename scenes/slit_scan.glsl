// Slit scan: each row samples u_prev shifted up 1px + horizontal warp.
// Bottom inject_height strip: fresh audio waveform.
//
// u_param0  row_amp      (0..0.05, 0.01) audio_route=treble — warp magnitude
// u_param1  pattern_freq (1..30, 8)
// u_param2  pattern_speed (0..2, 0.5)
// u_param3  hue           (0..1, 0.4)
// u_param4  inject_height (0..0.1, 0.02) — bottom inject strip thickness

void main() {
    vec2 px = 1.0 / u_resolution;
    float inject = u_param4;

    if (v_uv.y < inject) {
        // Bottom inject strip: synthesize waveform from u_audio bands
        float t = v_uv.x;
        float amp;
        if (t < 0.25) {
            amp = mix(u_audio.x, u_audio.y, t * 4.0);
        } else if (t < 0.5) {
            amp = mix(u_audio.y, u_audio.z, (t - 0.25) * 4.0);
        } else if (t < 0.75) {
            amp = mix(u_audio.z, u_audio.w, (t - 0.5) * 4.0);
        } else {
            amp = u_audio.w;
        }
        float h = u_param3;
        vec3 col = 0.5 + 0.5 * cos(6.2831 * (h + amp * 0.3 + vec3(0.0, 0.33, 0.66)));
        gl_FragColor = vec4(col * amp, 1.0);
    } else {
        // Warp: row-dependent horizontal offset + scroll 1px up
        float row_offset = sin(v_uv.y * u_param1 + u_time * u_param2) * u_param0;
        vec2 sample_uv = v_uv + vec2(row_offset, -px.y);
        // Clamp to avoid wrap-around artifacts at edges
        sample_uv.x = clamp(sample_uv.x, 0.0, 1.0);
        sample_uv.y = clamp(sample_uv.y, 0.0, 1.0);
        gl_FragColor = texture2D(u_prev, sample_uv);
    }
}
