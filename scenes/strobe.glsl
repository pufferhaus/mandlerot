// u_param0  flash_intensity (0..1)   — beat brightness amount
// u_param1  background      (0..0.2) — non-flash floor brightness
// u_param2  hue_speed       (0..2)   — hue cycle rate
// u_param3  hue_bass_kick   (0..1)   — bass adds to flash
// u_param4  chroma          (0..1)   — 0=mono white flash, 1=full color
//
// Hard strobe: full-screen color flash on beat. Black otherwise.
void main() {
    float bpm = u_bpm > 1.0 ? u_bpm : 120.0;

    float flash_intensity = u_param0;
    float background      = u_param1;
    float hue_speed       = u_param2;
    float hue_bass_kick   = u_param3;
    float chroma          = u_param4;

    float hue = fract(u_time * hue_speed * 0.1);

    // Beat flash with bass kick layered in
    float beat_flash = u_beat * flash_intensity;
    float bass_flash = u_audio.x * hue_bass_kick;
    float brightness = clamp(beat_flash + bass_flash, 0.0, 1.0);

    // Color: cosine palette for full-chroma, white for mono
    vec3 color_hue = 0.5 + 0.5 * cos(6.2831 * (hue + vec3(0.0, 0.33, 0.66)));
    vec3 flash_color = mix(vec3(1.0), color_hue, chroma);

    // Combine flash with background floor (tinted by current hue, not gray).
    vec3 bg_col = vec3(background) * mix(vec3(1.0), color_hue, chroma);
    vec3 col = mix(bg_col, flash_color, brightness);

    gl_FragColor = vec4(col, 1.0);
}
