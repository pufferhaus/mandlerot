// Analog TV "no signal" snow.
// u_param0  noise_density   (0.5..1.0, default 0.9) — fraction of pixels lit per frame
// u_param1  scanline_strength (0..1, default 0.4) — horizontal scanline darkening
// u_param2  chroma_tint     (-1..1, default 0.05) — red/blue color tint
// u_param3  roll_amount     (0..1, default 0.3) — vertical sync-roll on beats
// u_param4  hum_freq        (10..200, default 60) — horizontal banding "60Hz hum"
// u_param5  brightness      (0..1, default 0.7)

float hash(vec2 p) {
    return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453);
}

void main() {
    vec2 uv = v_uv;

    // Sync-roll: on beat, vertical offset the whole image
    float roll = u_beat * u_param3 * 0.3;
    uv.y = fract(uv.y + roll);

    // Noise per pixel per frame
    float t = floor(u_time * 30.0); // 30 Hz update
    float n = hash(uv * u_resolution + vec2(t));
    float lit = step(1.0 - u_param0, n);
    float bright = lit * (0.6 + 0.4 * hash(uv * u_resolution + vec2(t + 1.0)));

    // 60 Hz horizontal hum
    float hum = sin(uv.y * u_param4 * 2.0) * 0.05;
    bright += hum;

    // Scanlines (pixel-level horizontal bands darker)
    float scan = 1.0 - u_param1 * step(0.5, mod(uv.y * u_resolution.y * 0.5, 1.0));
    bright *= scan;

    // Subtle chroma tint shifted by audio
    float r_tint = 1.0 + u_param2 + 0.2 * u_audio.x;
    float b_tint = 1.0 - u_param2 + 0.2 * u_audio.w;
    vec3 color = vec3(bright * r_tint, bright, bright * b_tint);

    // Bass thump = momentary brightness boost
    color *= u_param5 * (1.0 + 0.3 * u_audio.x);

    gl_FragColor = vec4(color, 1.0);
}
