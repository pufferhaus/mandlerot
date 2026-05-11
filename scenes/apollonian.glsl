// u_param0  orbit_speed  (0..2, 0.20)
// u_param1  cam_distance (1.5..5, 2.6)
// u_param2  hue          (0..1, 0.78)
// u_param3  detail       (16..96, 56)
// u_param4  iterations   (4..10, 7)
// u_param5  fold_scale   (1.5..4.0, 2.5) — Apollonian scale s [bass +]
// u_param6  ground_dim   (0..1, 0.25)    — abs(p.y) factor in DE
// u_param7  hue_warp     (0..1, 0.4)     — escape→hue coupling [treble +]
//
// Apollonian Gasket 3D: classic IFS over the unit cell. Packs spheres of
// varying sizes recursively. Soft organic feel — looks like grouped pearls
// of all scales.

float DE(vec3 p, int iters, float s, float groundDim){
    float scale = 1.0;
    for (int i = 0; i < 10; i++){
        if (i >= iters) break;
        p = -1.0 + 2.0 * fract(0.5 * p + 0.5);
        float r2 = dot(p, p);
        float k  = s / max(r2, 1e-4);
        p *= k;
        scale *= k;
    }
    return groundDim * abs(p.y) / max(scale, 1e-4);
}

vec3 calcNormal(vec3 p, int iters, float s, float groundDim){
    vec2 e = vec2(0.0015, 0.0);
    return normalize(vec3(
        DE(p + e.xyy, iters, s, groundDim) - DE(p - e.xyy, iters, s, groundDim),
        DE(p + e.yxy, iters, s, groundDim) - DE(p - e.yxy, iters, s, groundDim),
        DE(p + e.yyx, iters, s, groundDim) - DE(p - e.yyx, iters, s, groundDim)
    ));
}

void main(){
    float bpm = u_bpm > 1.0 ? u_bpm : 120.0;
    float orbit = u_param0;
    float dist  = u_param1;
    float hue   = u_param2;
    int   steps = int(u_param3);
    int   iters = int(u_param4);
    float s     = u_param5;
    float gnd   = u_param6;
    float hw    = u_param7;

    float aspect = u_resolution.x / u_resolution.y;
    float angle = u_time * orbit * bpm / 60.0 * 0.1;

    vec3 ro = vec3(cos(angle) * dist, sin(u_time * 0.21) * 0.6 + 0.2, sin(angle) * dist);
    vec3 fwd = normalize(-ro);
    vec3 right = normalize(cross(fwd, vec3(0.0, 1.0, 0.0)));
    vec3 up = cross(right, fwd);

    vec2 ndc = (v_uv - 0.5) * vec2(aspect, 1.0);
    vec3 rd = normalize(fwd + ndc.x * right + ndc.y * up);

    float t = 0.0;
    float hit = 0.0;
    float escape = 0.0;
    for (int j = 0; j < 96; j++){
        if (j >= steps) break;
        vec3 p = ro + rd * t;
        float d = DE(p, iters, s, gnd);
        if (d < 0.001){ hit = 1.0; escape = float(j) / float(steps); break; }
        if (t > 8.0) break;
        t += max(d, 0.001);
    }

    if (hit < 0.5){ gl_FragColor = vec4(0.0, 0.0, 0.0, 1.0); return; }

    vec3 p   = ro + rd * t;
    vec3 n   = calcNormal(p, iters, s, gnd);
    vec3 ldir = normalize(vec3(0.5, 0.8, 0.3));
    float diff = clamp(dot(n, ldir), 0.0, 1.0);

    float audio_drift = u_audio.x * 0.10;
    float bpm_drift = u_time * bpm / (60.0 * 32.0);
    float col_t = escape * (1.0 + hw) + hue + audio_drift + bpm_drift;
    vec3 col = 0.5 + 0.5 * cos(6.2831 * (col_t + vec3(0.0, 0.33, 0.66)));
    // Lambert + ambient: surface variation gives detail even at close range,
    // so the screen doesn't flood with a single color when camera is near.
    col *= (0.25 + 0.75 * diff);
    // mild distance fog
    col *= 1.0 - clamp(t / 10.0, 0.0, 0.55);

    gl_FragColor = vec4(clamp(col, 0.0, 1.0), 1.0);
}
