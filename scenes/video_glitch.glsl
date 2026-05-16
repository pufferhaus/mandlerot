// Horizontal-tear datamosh on the live feed. Reads u_video, displaces each
// scanline by a noisy offset modulated by u_param0 (tear amount) and
// u_audio.x (bass kicks tearing harder). u_param1 controls vertical
// banding density.

float hash(float x) { return fract(sin(x * 91.3458) * 47453.5453); }

void main() {
    float tear = u_param0;
    float bands = max(u_param1, 1.0);
    float bass = u_audio.x;
    float row = floor(v_uv.y * bands) / bands;
    float jitter = (hash(row + floor(u_time * 8.0)) - 0.5) * tear * (0.5 + bass);
    vec2 sampled = vec2(v_uv.x + jitter, v_uv.y) * u_video_uv_scale;
    sampled.x = clamp(sampled.x, 0.0, u_video_uv_scale.x);
    sampled.y = clamp(sampled.y, 0.0, u_video_uv_scale.y);
    vec3 col = texture2D(u_video, sampled).rgb;
    // chroma offset on the red channel for extra glitch
    float r_off = (hash(row * 3.7) - 0.5) * tear * 0.4;
    col.r = texture2D(u_video, sampled + vec2(r_off, 0.0)).r;
    gl_FragColor = vec4(col, 1.0);
}
