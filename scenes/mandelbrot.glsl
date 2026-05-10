// u_param0  zoom
// u_param1  center_x
// u_param2  center_y
// u_param3  max iterations
// u_param4  palette offset (audio-routed: bass)
// u_param5  color_warp    (audio-routed: treble)
//
// Interior of the set renders pure black so the background reads as a clean
// silhouette. Escape-time pixels get a cosine palette tinted by audio.
void main() {
    float zoom = u_param0;
    vec2 center = vec2(u_param1, u_param2);
    float aspect = u_resolution.x / u_resolution.y;
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
            // Smoothed escape count for banding-free coloring.
            smooth_count = float(i) + 1.0 - log2(log(r2) * 0.5);
            break;
        }
    }

    if (i >= max_iter) {
        // Inside the set: solid black background.
        gl_FragColor = vec4(0.0, 0.0, 0.0, 1.0);
        return;
    }

    float t = smooth_count / float(max_iter);

    // Palette offset (bass-driven via u_param4) shifts hue.
    // color_warp (treble-driven via u_param5) adds a fast micro-shift to the
    // RGB phase so transients flicker the color.
    vec3 phase = vec3(0.0, 0.33, 0.66) + u_param5 * vec3(0.05, 0.10, 0.15);
    vec3 color = 0.5 + 0.5 * cos(6.2831 * (t + u_param4) + phase);

    // Bass also lifts saturation/brightness slightly so the kick is felt
    // without overpowering the structure.
    color *= 0.85 + 0.30 * u_audio.x;

    gl_FragColor = vec4(color, 1.0);
}
