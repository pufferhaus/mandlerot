// u_param0  zoom_base    — center of the zoom oscillation (manual)
// u_param1  center_x
// u_param2  center_y
// u_param3  max iterations
// u_param4  palette offset (audio-routed: bass)
// u_param5  color_warp    (audio-routed: treble)
// u_param6  zoom_octaves  — how many doublings the breathing covers (manual)
// u_param7  zoom_beats    — beats per full breathe cycle (manual)
//
// Continuous breathing zoom: zoom oscillates around `zoom_base` between
// `zoom_base / 2^octaves` and `zoom_base * 2^octaves` over `zoom_beats`
// beats. BPM comes from `u_bpm`; if no tap-tempo has been registered,
// fall back to 120 BPM so the scene still moves on its own.
//
// Interior of the set is pure black for a clean silhouette. Escape-time
// pixels get a smoothed cosine palette tinted by audio.
void main() {
    float zoom_base = u_param0;
    vec2 center = vec2(u_param1, u_param2);
    float aspect = u_resolution.x / u_resolution.y;

    float bpm = u_bpm > 1.0 ? u_bpm : 120.0;
    float zoom_beats = u_param7 > 0.5 ? u_param7 : 16.0;
    float zoom_octaves = u_param6;
    float cycle_secs = 60.0 * zoom_beats / bpm;
    float phase = 6.2831 * (u_time / cycle_secs);
    float zoom_mul = exp2(sin(phase) * zoom_octaves);
    float zoom = zoom_base * zoom_mul;

    vec2 uv = (v_uv - 0.5) * vec2(aspect, 1.0) * (4.0 / zoom) + center;

    vec2 z = vec2(0.0);
    int max_iter = int(u_param3);
    int i;
    float smooth_count = 0.0;
    for (i = 0; i < 256; i++) {
        if (i >= max_iter) break;
        z = vec2(z.x * z.x - z.y * z.y, 2.0 * z.x * z.y) + uv;
        float r2 = dot(z, z);
        if (r2 > 4.0) {
            smooth_count = float(i) + 1.0 - log2(log(r2) * 0.5);
            break;
        }
    }

    if (i >= max_iter) {
        gl_FragColor = vec4(0.0, 0.0, 0.0, 1.0);
        return;
    }

    float t = smooth_count / float(max_iter);

    vec3 phase_rgb = vec3(0.0, 0.33, 0.66) + u_param5 * vec3(0.05, 0.10, 0.15);
    // Slow BPM-locked palette drift so colors evolve even with silent audio.
    float palette_drift = u_time * bpm / (60.0 * 32.0);
    vec3 color = 0.5 + 0.5 * cos(6.2831 * (t + u_param4 + palette_drift) + phase_rgb);
    color *= 0.85 + 0.30 * u_audio.x;

    gl_FragColor = vec4(color, 1.0);
}
