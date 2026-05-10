// Spectrogram waterfall: vertical scrolling FFT history.
// Reads the last 320 audio frames directly from u_audio_history (1×320 RGBA8,
// R=bass, G=lomid, B=himid, A=treble; v=0 oldest, v=1 newest). Top of screen
// = newest, bottom = oldest. No u_prev scrolling, so switching to this scene
// mid-session shows the audio history immediately rather than warming up
// over ~10 seconds.
//
// u_param0  hue_low      (0..1, 0.66) — palette anchor for low magnitude
// u_param1  hue_high     (0..1, 0.0)  — palette anchor for high magnitude
// u_param2  scroll_speed (unused; preserved for parameter compatibility)
// u_param3  brightness   (0..2, 1.0)
// u_param4  contrast     (0.5..3.0, 1.5) — power curve on amplitude

void main() {
    // y=1 at top → newest history; y=0 at bottom → oldest. The history
    // texture stores oldest-first at v=0 and newest-last at v=1, so a top-row
    // = newest mapping is t = v_uv.y.
    float t = v_uv.y;
    vec4 hist = texture2D(u_audio_history, vec2(0.5, t));
    // hist.rgba = (bass, lomid, himid, treble) for that historical frame.

    // Map x across the four bands. Linearly interpolate between adjacent
    // bands so we get a smooth left-to-right gradient instead of four hard
    // bars. fb in 0..3, i_low in {0,1,2}.
    float fb = v_uv.x * 3.0;
    float i_low = floor(fb);
    float frac = fb - i_low;
    // GLES 1.00 has no array indexing of vec4 components, so a ternary chain
    // selects the correct channel for the low/high band of the current x.
    float low_val =
        i_low < 0.5 ? hist.r :
        i_low < 1.5 ? hist.g :
        i_low < 2.5 ? hist.b : hist.a;
    float high_val =
        i_low < 0.5 ? hist.g :
        i_low < 1.5 ? hist.b :
                      hist.a; // i_low == 2 → high is treble
    float mag = mix(low_val, high_val, frac);

    // Match the original scene's curve: pow(amp, contrast). With default
    // contrast=1.5 this attenuates low magnitudes and lifts the visual
    // dynamic range, identical to the pre-rewrite behavior.
    mag = pow(clamp(mag, 0.0, 1.0), u_param4);
    mag *= u_param3;

    float hue = mix(u_param0, u_param1, clamp(mag, 0.0, 1.0));
    vec3 color = 0.5 + 0.5 * cos(6.2831 * (hue + vec3(0.0, 0.33, 0.66)));
    color *= clamp(mag, 0.0, 1.0);
    gl_FragColor = vec4(color, 1.0);
}
