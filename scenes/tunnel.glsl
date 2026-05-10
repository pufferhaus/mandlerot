// u_param0  speed           (0..2)   — forward speed multiplier
// u_param1  stripes         (4..32)  — stripe count (floored)
// u_param2  twist           (-2..2)  — twist per unit depth
// u_param3  hue             (0..1)   — base hue
// u_param4  brightness_pulse (0..1)  — bass brightness boost [bass]
//
// Raycast-style tunnel via polar UV mapping.
// Speed locks to BPM; bass pulses brightness.
void main() {
    float bpm = u_bpm > 1.0 ? u_bpm : 120.0;
    float beat_time = u_time * bpm / 60.0;

    float speed = u_param0;
    float stripes = floor(u_param1);
    float twist = u_param2;
    float hue = u_param3;
    float pulse = u_param4;

    float aspect = u_resolution.x / u_resolution.y;
    vec2 uv = (v_uv - 0.5) * vec2(aspect, 1.0);

    float radius = length(uv);
    float angle = atan(uv.y, uv.x); // -pi..pi

    // Tunnel UV: u = angle wrapped to 0..1, v = depth (1/radius + time)
    float tu = angle / 6.2831 + 0.5;                   // 0..1
    float tv = 1.0 / (radius + 0.001) * 0.3            // depth
               + beat_time * speed * (1.0 / 16.0);     // forward motion (BPM locked)

    // Twist: rotate angle with depth
    tu += twist * tv * 0.05;
    tu = fract(tu);

    // Checkerboard / stripe pattern
    float stripe = floor(tu * stripes);
    float depth_band = floor(tv * stripes);
    float check = mod(stripe + depth_band, 2.0);

    vec3 base_col = 0.5 + 0.5 * cos(6.2831 * (hue + check * 0.5 + vec3(0.0, 0.33, 0.66)));

    // Vignette: darker at edges, brighter center
    float vignette = 1.0 - clamp(radius * 1.5, 0.0, 0.85);
    // Bass brightness pulse
    float bright = 1.0 + pulse * u_audio.x * 0.8;

    vec3 col = base_col * vignette * bright;
    gl_FragColor = vec4(clamp(col, 0.0, 1.0), 1.0);
}
