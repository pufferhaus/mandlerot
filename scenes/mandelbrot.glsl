// u_param0  zoom_base    — initial zoom at start of each cycle
// u_param1  center_x     — fractal coord to zoom toward
// u_param2  center_y
// u_param3  iterations   — base iteration count (auto-scaled with zoom)
// u_param4  palette      — palette offset (audio-routed: bass)
// u_param5  color_warp   — palette phase warp (audio-routed: treble)
// u_param6  zoom_rate    — doublings per second
// u_param7  cycle_secs   — seconds before zoom resets back to zoom_base
//
// Monotonic exponential infinite zoom: zoom = zoom_base * 2^(t * zoom_rate).
// At cycle_secs seconds the zoom resets to zoom_base and restarts — fp32
// precision dies past ~20 doublings, so cycling avoids degenerate pixel
// soup. Critical math uses highp explicitly; on hardware without true highp
// (e.g. Pi VC4 default mediump=fp16) zoom depth is limited.
//
// Iteration cap auto-scales with zoom depth so deep zooms keep boundary
// detail without burning iterations at shallow levels.
void main() {
    highp float zoom_base = u_param0;
    highp vec2  center    = vec2(u_param1, u_param2);
    float aspect = u_resolution.x / u_resolution.y;

    float bpm        = u_bpm > 1.0 ? u_bpm : 120.0;
    float zoom_rate  = u_param6;
    float cycle_secs = u_param7 > 0.5 ? u_param7 : 90.0;

    float t_loop = mod(u_time, cycle_secs);
    highp float zoom_mul = exp2(t_loop * zoom_rate);
    highp float zoom = zoom_base * zoom_mul;

    highp vec2 uv = (v_uv - 0.5) * vec2(aspect, 1.0) * (4.0 / zoom) + center;

    // base iter count + extra iterations as zoom deepens, clamped to loop bound
    int base_iter = int(u_param3);
    int extra = int(log2(max(zoom, 1.0)) * 12.0);
    int max_iter = base_iter + extra;
    if (max_iter < 16) max_iter = 16;
    if (max_iter > 256) max_iter = 256;

    highp vec2 z = vec2(0.0);
    int i;
    float smooth_count = 0.0;
    for (i = 0; i < 256; i++) {
        if (i >= max_iter) break;
        z = vec2(z.x * z.x - z.y * z.y, 2.0 * z.x * z.y) + uv;
        highp float r2 = dot(z, z);
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
    float palette_drift = u_time * bpm / (60.0 * 32.0);
    vec3 color = 0.5 + 0.5 * cos(6.2831 * (t + u_param4 + palette_drift) + phase_rgb);
    color *= 0.85 + 0.30 * u_audio.x;

    gl_FragColor = vec4(color, 1.0);
}
