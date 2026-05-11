// u_param0  scale        (1.5..3.5, 2.0) — box fold scale [bass +]
// u_param1  orbit_speed  (0..2, 0.3)
// u_param2  cam_distance (2.5..8, 4.5)
// u_param3  hue          (0..1, 0.55)
// u_param4  detail       (16..96, 48)    — march steps
// u_param5  min_radius   (0.3..0.8, 0.5) — sphere-fold inner radius
// u_param6  fixed_radius (0.9..1.5, 1.0) — sphere-fold outer radius
// u_param7  fold_iters   (4..12, 8)      — fractal iteration depth
//
// Mandelbox: alternating box fold + sphere fold scaled by `scale`. Crystalline
// alien architecture. Bass drives scale → structure pulses.

void boxFold(inout vec3 p){ p = clamp(p, -1.0, 1.0) * 2.0 - p; }

void sphereFold(inout vec3 p, inout float dr, float minR, float fixR){
    float r2 = dot(p, p);
    if (r2 < minR*minR){
        float t = (fixR*fixR) / (minR*minR);
        p *= t; dr *= t;
    } else if (r2 < fixR*fixR){
        float t = (fixR*fixR) / r2;
        p *= t; dr *= t;
    }
}

float DE(vec3 p, float scale, float minR, float fixR, int iters){
    vec3 offset = p;
    float dr = 1.0;
    for (int n = 0; n < 12; n++){
        if (n >= iters) break;
        boxFold(p);
        sphereFold(p, dr, minR, fixR);
        p = scale * p + offset;
        dr = dr * abs(scale) + 1.0;
    }
    return length(p) / abs(dr);
}

void main(){
    float bpm = u_bpm > 1.0 ? u_bpm : 120.0;
    float scale = u_param0;
    float orbit = u_param1;
    float dist  = u_param2;
    float hue   = u_param3;
    int   steps = int(u_param4);
    float minR  = u_param5;
    float fixR  = u_param6;
    int   iters = int(u_param7);

    float aspect = u_resolution.x / u_resolution.y;
    float angle = u_time * orbit * bpm / 60.0 * 0.1;

    vec3 ro = vec3(cos(angle) * dist, sin(u_time * 0.13) * 1.0, sin(angle) * dist);
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
        float d = DE(p, scale, minR, fixR, iters);
        if (d < 0.001){ hit = 1.0; escape = float(j) / float(steps); break; }
        if (t > 12.0) break;
        t += max(d, 0.001);
    }

    if (hit < 0.5){ gl_FragColor = vec4(0.0, 0.0, 0.0, 1.0); return; }

    float audio_drift = u_audio.x * 0.12;
    float bpm_drift = u_time * bpm / (60.0 * 32.0);
    float col_t = escape + hue + audio_drift + bpm_drift + u_beat * 0.05;
    vec3 col = 0.5 + 0.5 * cos(6.2831 * (col_t + vec3(0.0, 0.33, 0.66)));
    float shade = 1.0 - clamp(t / 12.0, 0.0, 0.7);
    col *= shade;

    gl_FragColor = vec4(clamp(col, 0.0, 1.0), 1.0);
}
