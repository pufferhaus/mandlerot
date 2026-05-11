// u_param0  c_x         (-1..1, -0.2)  — Julia constant x  [bass +]
// u_param1  c_y         (-1..1,  0.4)  — Julia constant y  [lomid +]
// u_param2  c_z         (-1..1,  0.3)  — Julia constant z  [himid +]
// u_param3  hue         (0..1, 0.7)
// u_param4  detail      (16..96, 48)
// u_param5  orbit_speed (0..2, 0.25)
// u_param6  cam_distance(1.5..4, 2.6)
// u_param7  power       (3..12, 8)     — exponent (locked-ish)
//
// Juliabulb: same iteration as Mandelbulb but z is updated as
// z = z^power + c (constant), not z + p. Different c → completely different
// organic shape. Audio drives c on three axes → shape morphs continuously.

float DE(vec3 p, vec3 c, float power, int iters){
    vec3 z = p;
    float dr = 1.0;
    float r = 0.0;
    for (int i = 0; i < 12; i++){
        if (i >= iters) break;
        r = length(z);
        if (r > 2.0) break;
        float theta = acos(clamp(z.z / max(r, 1e-6), -1.0, 1.0));
        float phi   = atan(z.y, z.x);
        dr = pow(r, power - 1.0) * power * dr + 1.0;
        float zr = pow(r, power);
        theta *= power;
        phi   *= power;
        z = zr * vec3(sin(theta) * cos(phi),
                      sin(theta) * sin(phi),
                      cos(theta));
        z += c;
    }
    float safe_r = max(r, 1e-6);
    return 0.5 * log(safe_r) * safe_r / dr;
}

void main(){
    float bpm = u_bpm > 1.0 ? u_bpm : 120.0;
    vec3  c    = vec3(u_param0, u_param1, u_param2);
    float hue  = u_param3;
    int   steps = int(u_param4);
    float orbit = u_param5;
    float dist  = u_param6;
    float power = u_param7;

    float aspect = u_resolution.x / u_resolution.y;
    float angle = u_time * orbit * bpm / 60.0 * 0.1;

    vec3 ro = vec3(cos(angle) * dist, 0.4 + sin(u_time * 0.17) * 0.3, sin(angle) * dist);
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
        float d = DE(p, c, power, 8);
        if (d < 0.001){ hit = 1.0; escape = float(j) / float(steps); break; }
        if (t > 6.0) break;
        t += max(d, 0.001);
    }

    if (hit < 0.5){ gl_FragColor = vec4(0.0, 0.0, 0.0, 1.0); return; }

    float audio_drift = u_audio.x * 0.15;
    float bpm_drift = u_time * bpm / (60.0 * 32.0);
    float col_t = escape + hue + audio_drift + bpm_drift + u_beat * 0.05;
    vec3 col = 0.5 + 0.5 * cos(6.2831 * (col_t + vec3(0.0, 0.33, 0.66)));
    float shade = 1.0 - clamp(t / 6.0, 0.0, 0.7);
    col *= shade;

    gl_FragColor = vec4(clamp(col, 0.0, 1.0), 1.0);
}
