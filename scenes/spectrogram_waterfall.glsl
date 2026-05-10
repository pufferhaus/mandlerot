// Scrolling FFT history. Each frame: shift u_prev down by scroll_speed rows,
// write fresh audio spectrum at the top.
//
// u_audio: x=bass, y=lomid, z=himid, w=treble
//
// u_param0  hue_low      (0..1, 0.66)  blue
// u_param1  hue_high     (0..1, 0.0)   red
// u_param2  scroll_speed (0.5..3.0, 1.0) rows shifted per frame
// u_param3  brightness   (0..2, 1.0)
// u_param4  contrast     (0.5..3.0, 1.5) power curve on amplitude

// Map a palette: low amplitude → hue_low, high → hue_high
vec3 palette(float t) {
    float h = mix(u_param0, u_param1, t);
    return 0.5 + 0.5 * cos(6.2831 * (h + vec3(0.0, 0.33, 0.66)));
}

void main() {
    vec2 px = 1.0 / u_resolution;
    float scroll = max(1.0, floor(u_param2));
    float top_rows = scroll * px.y;

    if (v_uv.y > 1.0 - top_rows) {
        // Top strip: synthesize from current u_audio bands
        float t = v_uv.x;
        float amp;
        if (t < 0.25) {
            // bass → lomid
            amp = mix(u_audio.x, u_audio.y, t * 4.0);
        } else if (t < 0.5) {
            // lomid → himid
            amp = mix(u_audio.y, u_audio.z, (t - 0.25) * 4.0);
        } else if (t < 0.75) {
            // himid → treble
            amp = mix(u_audio.z, u_audio.w, (t - 0.5) * 4.0);
        } else {
            amp = u_audio.w;
        }
        amp = pow(clamp(amp, 0.0, 1.0), u_param4) * u_param3;
        gl_FragColor = vec4(palette(amp) * amp, 1.0);
    } else {
        // Scroll: sample previous frame shifted down by scroll pixels
        vec2 shifted = v_uv + vec2(0.0, scroll * px.y);
        gl_FragColor = texture2D(u_prev, shifted);
    }
}
