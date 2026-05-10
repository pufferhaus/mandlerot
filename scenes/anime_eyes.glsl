// Two cartoon-style anime eyes that look around and blink.
// Pupil position drifts on a slow time-driven path with audio jitter on top.
// Blinks fire on a random subset of beats with a smooth squash animation.
//
// u_param0  pupil_size    (0.02..0.10, 0.045) audio_route=bass amount=0.2  pupil dilates on bass
// u_param1  look_speed    (0..1.5, 0.5)       rate of look-around motion
// u_param2  iris_hue      (0..1, 0.55)        audio_route=himid amount=0.4
// u_param3  blink_chance  (0..1, 0.35)        probability of blink per 4-beat
// u_param4  eye_size      (0.15..0.45, 0.30)
// u_param5  jitter        (0..0.06, 0.012)    audio_route=treble amount=0.5 darting eyes
// u_param6  iris_radius   (0.3..0.7, 0.55)    iris-to-eye ratio

float hash11(float p) {
    return fract(sin(p * 12.9898) * 43758.5453);
}

void main() {
    vec2 aspect = vec2(u_resolution.x / u_resolution.y, 1.0);
    vec2 uv = (v_uv * 2.0 - 1.0) * aspect;

    float bpm   = u_bpm > 1.0 ? u_bpm : 120.0;
    float beats = u_time * bpm / 60.0;

    float eye_sz   = u_param4;
    float iris_r   = eye_sz * u_param6;
    float pupil_r  = u_param0;
    float look_amp = eye_sz * 0.35;

    // Two eye centers, separated horizontally; slightly above screen center.
    vec2 lc = vec2(-eye_sz * 1.25, eye_sz * 0.15);
    vec2 rc = vec2( eye_sz * 1.25, eye_sz * 0.15);

    // Look direction: a slow Lissajous around the eye + treble micro-jitter.
    float lt = u_time * u_param1;
    vec2 look = vec2(sin(lt * 0.7), sin(lt * 0.5 + 1.7)) * 0.55;
    float jt = u_time * 18.0;
    look += vec2(sin(jt), cos(jt * 1.3)) * u_param5 * 6.0;
    look = clamp(look, vec2(-1.0), vec2(1.0));
    vec2 look_offset = look * look_amp;

    // Blink: every 4 beats, with `blink_chance` probability, lasting ~0.25 beat.
    float beat_period = 4.0;
    float blink_seed = floor(beats / beat_period);
    float blink_fire = step(1.0 - u_param3, hash11(blink_seed));
    float beat_phase = mod(beats, beat_period);
    float blink_win  = step(beat_phase, 0.25);
    // Smooth open-close-open over the window using sin(pi * phase/window).
    float blink_curve = blink_fire * blink_win
                        * sin(3.14159 * clamp(beat_phase / 0.25, 0.0, 1.0));
    float lid_close = blink_curve * 0.95;

    // Render two eyes
    float eye_mask = 0.0;
    float eye_edge = 0.0;
    float iris_mask = 0.0;
    float iris_edge = 0.0;
    float pupil_mask = 0.0;
    float highlight_mask = 0.0;
    float lash_mask = 0.0;

    for (int i = 0; i < 2; i++) {
        vec2 c = (i == 0) ? lc : rc;
        vec2 local = uv - c;

        // Almond eye outline: y squashed by lid_close to animate blink.
        float ey = eye_sz * (1.0 - lid_close);
        float eye_d = length(local / vec2(eye_sz, ey * 0.55)) - 1.0;
        float e_m = step(eye_d, 0.0);
        eye_mask = max(eye_mask, e_m);
        eye_edge = max(eye_edge, smoothstep(0.04, 0.0, abs(eye_d)) * e_m);

        // Iris (offset by look) — clamped to stay inside the eye almond.
        vec2 iris_local = local - look_offset;
        float iris_d = length(iris_local) - iris_r;
        float i_m = step(iris_d, 0.0) * e_m;
        iris_mask = max(iris_mask, i_m);
        iris_edge = max(iris_edge, smoothstep(0.015, 0.0, abs(iris_d)) * e_m);

        // Pupil
        float pupil_d = length(iris_local) - pupil_r;
        float p_m = step(pupil_d, 0.0) * e_m;
        pupil_mask = max(pupil_mask, p_m);

        // Highlight: small bright circle upper-left of iris, hidden when blinking.
        vec2 hl_pos = iris_local - vec2(-iris_r * 0.38, iris_r * 0.38);
        float hl_d = length(hl_pos) - iris_r * 0.18;
        highlight_mask = max(highlight_mask,
                             step(hl_d, 0.0) * e_m * (1.0 - blink_curve));

        // Upper eyelash: thick arc above the eye, draws even when open.
        // Distance to top of almond ellipse approximated as a thin band.
        float upper_d = -eye_d; // negative inside, but we want a band along top
        float top_y = local.y;
        float lash_band = step(top_y, eye_sz * (1.0 - lid_close) * 0.55)
                          * step(eye_sz * (1.0 - lid_close) * 0.45 - 0.015, top_y);
        // Constrain horizontally to inside eye
        float lash_h = step(abs(local.x), eye_sz * 0.95);
        lash_mask = max(lash_mask, lash_band * lash_h);
    }

    // Compose colors
    vec3 bg          = vec3(0.06, 0.05, 0.07);
    vec3 sclera      = vec3(0.97, 0.95, 0.93);
    float hue_drift  = beats / 32.0;
    vec3 iris_color  = 0.5 + 0.5 * cos(
        6.2831 * (u_param2 + hue_drift + vec3(0.0, 0.33, 0.66))
    );
    // Iris has radial gradient — darker at edge, lighter near pupil.
    // We don't have the per-pixel iris_local here outside the loop, but the
    // bare iris_mask is enough; tint is uniform per eye for simplicity.
    vec3 pupil_color = vec3(0.05, 0.04, 0.08);
    vec3 lash_color  = vec3(0.04, 0.03, 0.05);
    vec3 outline     = vec3(0.10, 0.08, 0.10);

    vec3 col = bg;
    col = mix(col, sclera,      eye_mask);
    col = mix(col, iris_color,  iris_mask);
    col = mix(col, iris_color * 0.6, iris_edge); // darker iris rim
    col = mix(col, pupil_color, pupil_mask);
    col = mix(col, vec3(1.0),   highlight_mask);
    col = mix(col, lash_color,  lash_mask);
    col = mix(col, outline,     eye_edge * (1.0 - iris_mask));

    gl_FragColor = vec4(col, 1.0);
}
