// u_param0  power        (3..16, 8)   — locked at 8 inside shader (UI readonly)
// u_param1  orbit_speed  (0..2, 0.3)  — camera orbit speed (BPM-locked)
// u_param2  cam_distance (1.5..4, 2.5) — camera distance [bass 0.5]
// u_param3  hue          (0..1, 0.6)  — base palette hue
// u_param4  detail       (16..96, 48) — sphere-march step count (int-ish)
//
// Power-8 Mandelbulb raymarcher. Camera orbits around the fractal,
// BPM-locked. Bass drives camera distance breathing.

float DE(vec3 p) {
    vec3 z = p;
    float dr = 1.0;
    float r = 0.0;
    for (int i = 0; i < 8; i++) {
        r = length(z);
        if (r > 2.0) break;
        // Polar conversion — clamp for GLES precision safety
        float theta = acos(clamp(z.z / max(r, 1e-6), -1.0, 1.0));
        float phi = atan(z.y, z.x);
        dr = pow(r, 7.0) * 8.0 * dr + 1.0;
        float zr = pow(r, 8.0);
        theta *= 8.0;
        phi   *= 8.0;
        z = zr * vec3(sin(theta) * cos(phi),
                      sin(theta) * sin(phi),
                      cos(theta));
        z += p;
    }
    float safe_r = max(r, 1e-6);
    return 0.5 * log(safe_r) * safe_r / dr;
}

void main() {
    float bpm   = u_bpm > 1.0 ? u_bpm : 120.0;
    float orbit = u_param1;
    float dist  = u_param2;
    float hue   = u_param3;
    int   steps = int(u_param4);

    float aspect = u_resolution.x / u_resolution.y;
    // Camera orbit angle — BPM-locked
    float angle = u_time * orbit * bpm / 60.0 * 0.1;

    vec3 ro = vec3(cos(angle) * dist, 0.4, sin(angle) * dist);
    vec3 target = vec3(0.0);
    vec3 fwd = normalize(target - ro);
    vec3 right = normalize(cross(fwd, vec3(0.0, 1.0, 0.0)));
    vec3 up = cross(right, fwd);

    // Ray direction from screen UV
    vec2 ndc = (v_uv - 0.5) * vec2(aspect, 1.0);
    vec3 rd = normalize(fwd + ndc.x * right + ndc.y * up);

    // Sphere march
    float t = 0.0;
    float hit = 0.0;
    float escape = 0.0;
    for (int j = 0; j < 64; j++) {
        if (j >= steps) break;
        vec3 p = ro + rd * t;
        float d = DE(p);
        if (d < 0.001) {
            hit = 1.0;
            escape = float(j) / float(steps);
            break;
        }
        if (t > 6.0) break;
        t += max(d, 0.001);
    }

    if (hit < 0.5) {
        gl_FragColor = vec4(0.0, 0.0, 0.0, 1.0);
        return;
    }

    // Palette: escape count + audio hue drift + slow BPM-locked drift
    float audio_drift = u_audio.x * 0.15;
    float bpm_drift = u_time * bpm / (60.0 * 32.0);
    float col_t = escape + hue + audio_drift + bpm_drift + u_beat * 0.05;
    vec3 col = 0.5 + 0.5 * cos(6.2831 * (col_t + vec3(0.0, 0.33, 0.66)));
    // Ambient shading: farther hits are dimmer
    float shade = 1.0 - clamp(t / 6.0, 0.0, 0.7);
    col *= shade;

    gl_FragColor = vec4(clamp(col, 0.0, 1.0), 1.0);
}
