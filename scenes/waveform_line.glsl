// u_param0  amplitude    (0..0.5)    — waveform height
// u_param1  thickness    (0.001..0.02) — line width [treble 0.4]
// u_param2  hue          (0..1)      — base hue
// u_param3  wave_density (1..20)     — peaks/valleys count
// u_param4  trail        (0..0.95)   — mix in u_prev for trail glow
//
// Single horizontal waveform synthesized from audio bands; glow falloff.
void main() {
    float amplitude    = u_param0;
    float thickness    = u_param1;
    float hue          = u_param2;
    float wave_density = u_param3;
    float trail        = u_param4;

    vec2 uv = v_uv;
    float x  = uv.x;

    // Synthesize waveform from 4 audio bands varied by screen X
    float pi2 = 6.2831;
    float bass   = u_audio.x;
    float lomid  = u_audio.y;
    float himid  = u_audio.z;
    float treble = u_audio.w;

    float wave = 0.0;
    wave += bass   * sin(pi2 * x * wave_density * 0.25);
    wave += lomid  * sin(pi2 * x * wave_density * 0.5 + 1.0);
    wave += himid  * sin(pi2 * x * wave_density * 0.75 + 2.0);
    wave += treble * sin(pi2 * x * wave_density + 3.0);
    wave *= amplitude * 0.5;

    // Centered on y=0.5
    float line_y = 0.5 + wave;
    float dist = abs(uv.y - line_y);
    float glow = exp(-dist * dist / (thickness * thickness));
    glow = clamp(glow, 0.0, 1.0);

    vec3 col = 0.5 + 0.5 * cos(pi2 * (hue + vec3(0.0, 0.33, 0.66)));
    vec3 pixel = col * glow;

    // Trail: blend with previous frame
    vec3 prev = texture2D(u_prev, uv).rgb;
    pixel = mix(pixel, prev * trail, trail * (1.0 - glow));

    gl_FragColor = vec4(pixel, 1.0);
}
