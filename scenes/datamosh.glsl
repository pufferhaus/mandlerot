// Datamosh: sample u_prev with per-macroblock motion-vector offsets so
// the previous frame drifts in 16x16 chunks. Drift is audio-driven; on
// every beat we briefly zero motion (key frame) so the image re-stabilizes
// before the codec "loses sync" again.
//
// u_param0 block_size      8..32   pixel size of a macroblock (16)
// u_param1 motion_bass     0..1    horizontal drift gain  [bass +]
// u_param2 motion_treble   0..1    vertical drift gain    [treble +]
// u_param3 intensity       0..1    mix between drifted u_prev and a fresh base
// u_param4 hue_drift       0..1    per-block hue rotation each cycle
// u_param5 corruption      0..1    sparse white "corrupted" macroblocks
// u_param6 base_speed      0..2    speed of the underlying base pattern
// u_param7 keyframe_decay  0..2    seconds the beat-keyframe lasts

float h21(vec2 p){ return fract(sin(dot(p, vec2(12.9898,78.233))) * 43758.5453); }

vec3 base(vec2 uv) {
    float t = u_time * (0.3 + u_param6);
    float v = sin(uv.x * 4.0 + t) + cos(uv.y * 5.0 - t * 0.7) + sin((uv.x+uv.y) * 3.0 + t * 0.5);
    v *= 0.33;
    float h = 0.6 + 0.1 * v + u_audio.x * 0.2;
    return 0.5 + 0.5 * cos(6.2831 * (h + vec3(0.0, 0.33, 0.66)));
}

void main() {
    vec2 uv = v_uv;
    float bs = max(u_param0, 4.0);
    vec2 px = uv * u_resolution;
    vec2 block = floor(px / bs);

    // per-block hash → motion vector
    float hx = h21(block + vec2(1.0, 0.0)) - 0.5;
    float hy = h21(block + vec2(0.0, 1.0)) - 0.5;

    // beat-driven keyframe: kf=1.0 right after beat, decays toward 0. When
    // kf is near 1, motion is suppressed so the frame stabilizes.
    float kf = exp(-mod(u_time, max(u_param7, 0.05)) * 2.0) * step(0.5, u_beat);
    float motion_scale = 1.0 - kf;

    vec2 mv = vec2(
        hx * u_param1 * 0.04 + (u_audio.x - 0.3) * 0.02,
        hy * u_param2 * 0.04 + (u_audio.w - 0.3) * 0.02
    ) * motion_scale;

    vec3 prev = texture2D(u_prev, fract(uv + mv)).rgb;

    // optional per-block hue rotation: shifts color over time independently
    float cycle = floor(u_time * 0.5);
    float hue_off = u_param4 * (h21(block + vec2(cycle, 0.0)) - 0.5) * 0.4;
    vec3 hsv_rot = 0.5 + 0.5 * cos(6.2831 * (hue_off + vec3(0.0, 0.33, 0.66)));
    prev *= mix(vec3(1.0), hsv_rot, u_param4 * 0.5);

    vec3 fresh = base(uv);
    vec3 col = mix(fresh, prev, u_param3);

    // corruption: a small fraction of blocks flash full white each frame
    float corrupt_seed = h21(block + vec2(floor(u_time * 12.0), 0.0));
    float corrupt = step(1.0 - u_param5 * 0.08, corrupt_seed);
    col = mix(col, vec3(1.0), corrupt);

    gl_FragColor = vec4(col, 1.0);
}
