// Smoke / ink dispersion. Layered fbm noise advected by a curl field; the
// flow direction is driven by audio so the smoke breathes with the music.
// No persistent state — pure procedural so it always animates cleanly.
//
// u_param0 density       0..1   how much of the field is "smoke"
// u_param1 swirl         0..2   curl noise rotation strength
// u_param2 flow_speed    0..2   overall drift speed
// u_param3 hue           0..1   ink color
// u_param4 contrast      0.5..3 detail contrast
// u_param5 dissolve      0..1   audio-modulated decay  [treble +]
// u_param6 scale         0.5..3 noise frequency
// u_param7 brightness    0..2   global gain

float h(vec2 p){ return fract(sin(dot(p, vec2(12.9898,78.233))) * 43758.5453); }

float value_noise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    f = f*f*(3.0 - 2.0*f);
    float a = h(i);
    float b = h(i + vec2(1.0, 0.0));
    float c = h(i + vec2(0.0, 1.0));
    float d = h(i + vec2(1.0, 1.0));
    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

float fbm(vec2 p) {
    float v = 0.0;
    float a = 0.5;
    for (int i = 0; i < 5; i++) {
        v += a * value_noise(p);
        p *= 2.03;
        a *= 0.5;
    }
    return v;
}

void main() {
    vec2 uv = v_uv;
    float t = u_time * u_param2 * 0.15;

    // pseudo-curl: rotate UV by a gradient angle from a coarser fbm
    float ang = fbm(uv * 1.7 + vec2(t, -t)) * 6.2831 * u_param1;
    vec2 dir = vec2(cos(ang), sin(ang));
    vec2 warped = uv * u_param6 + dir * 0.2 + vec2(t * 0.3, -t * 0.4);

    float f = fbm(warped);
    // Apply density threshold + contrast
    float smoke = smoothstep(1.0 - u_param0, 1.0, f);
    smoke = pow(smoke, u_param4);

    // dissolve when treble spikes: chop the lowest-density regions
    smoke *= 1.0 - u_param5 * 0.6 * (1.0 - smoke);

    vec3 tint = 0.5 + 0.5 * cos(6.2831 * (u_param3 + vec3(0.0, 0.33, 0.66)));
    vec3 col = tint * smoke * u_param7;
    gl_FragColor = vec4(col, 1.0);
}
