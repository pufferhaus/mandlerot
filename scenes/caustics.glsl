// u_param0  frequency    (3..20, 8)    — wave frequency [treble 0.5]
// u_param1  anim_speed   (0..2, 0.4)   — animation speed
// u_param2  hue          (0..1, 0.55)  — base hue (blue/cyan default)
// u_param3  threshold    (0..1, 0.5)   — bright spot threshold
// u_param4  brightness   (0..2, 1.2)   — output brightness [bass 0.4]
//
// Pool-floor caustics: layered sine interference produces bright refractive patches.
void main() {
    float bpm = u_bpm > 1.0 ? u_bpm : 120.0;

    float freq      = u_param0;
    float spd       = u_param1;
    float hue       = u_param2;
    float threshold = u_param3;
    float bright    = u_param4;

    // Aspect-correct UV
    vec2 p = (v_uv * 2.0 - 1.0) * vec2(u_resolution.x / u_resolution.y, 1.0);
    float t = u_time * spd;

    // Layer 1: horizontal + nested vertical
    float c1 = sin(p.x * freq       + sin(p.y * freq * 0.7 + t * 1.1) * 1.2 + t);
    // Layer 2: vertical + nested horizontal
    float c2 = sin(p.y * freq * 1.2 + sin(p.x * freq * 0.9 - t * 0.8) * 1.0 - t * 1.3);
    // Layer 3: diagonal phase
    float c3 = sin((p.x + p.y) * freq * 0.6 + t * 0.7);

    float caustic = abs(c1 + c2 + c3) * 0.33;

    // Bright spots where caustic > threshold
    float spot = smoothstep(threshold * 0.8, threshold, caustic);

    // Palette: cycle hue with caustic value
    float h = hue + caustic * 0.2 + u_time * 0.03;
    vec3 base_col = 0.5 + 0.5 * cos(6.2831 * (h + vec3(0.0, 0.33, 0.66)));

    // Hot-white caustic flares on top
    vec3 col = mix(base_col * 0.3, base_col + vec3(0.4) * spot, spot);
    col *= bright;

    // Beat flash: brief brightness boost
    col += u_beat * 0.15 * vec3(0.8, 0.9, 1.0);

    col = clamp(col, 0.0, 1.0);
    gl_FragColor = vec4(col, 1.0);
}
