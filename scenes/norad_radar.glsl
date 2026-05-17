// NORAD Radar — rotating phosphor sweep with decaying blip contacts.
//
// Dark green phosphor aesthetic. Sweep rotates clockwise.
// UV convention: v_uv.y=0 = top, so we negate y for standard math angles.
//
// u_param0  speed       (0.2..3.0, default 0.6) — sweep radians/sec
// u_param1  trail       (0.1..3.0, default 0.9) — angular trail falloff (larger = longer trail)
// u_param2  glow        (0.005..0.12, default 0.035) — blip softness radius

void main() {
    vec2 center = vec2(0.5, 0.5);
    vec2 uv = v_uv - center;
    // Aspect-correct: map to square space
    float aspect = u_resolution.x / u_resolution.y;
    uv.x *= aspect;

    float r = length(uv);

    // Fragment angle using standard math (CCW from +x axis)
    // Negate y because v_uv.y=0 is top of screen
    float frag_angle = atan(-uv.y, uv.x);

    // Clockwise sweep: angle decreases with time
    float speed = u_param0;
    float sweep_angle = mod(-u_time * speed, 6.28318530718);

    // Angular distance from sweep (how far behind the sweep arm this fragment is)
    float da = mod(frag_angle - sweep_angle + 6.28318530718 * 2.0, 6.28318530718);
    // da = 0 means right on the sweep arm; da near 2pi means just ahead of it
    // Trail: bright at da=0, decays as da increases
    float trail_falloff = u_param1;
    float trail = exp(-da * trail_falloff) * step(r, 0.46);

    // Sweep arm: the leading edge gets a bright line
    float arm = exp(-da * 12.0) * step(r, 0.46);

    // Outer ring
    float ring = step(0.44, r) * step(r, 0.46);
    // Inner tick marks at cardinal and ordinal angles
    float tick_r = step(0.42, r) * step(r, 0.46);
    float a_mod = mod(frag_angle, 0.785398); // pi/4 = 8 ticks
    float tick = tick_r * step(a_mod, 0.04);

    // Center dot
    float dot_glow = exp(-r * 80.0);

    // Hardcoded blip contacts (polar: angle_seed, radius)
    // 8 contacts scattered around the radar
    float blip_brightness = 0.0;
    float glow_r = u_param2;
    float bass_boost = 1.0 + u_audio.x * 0.4;

    // Contact 0
    {
        float ba = 0.4;
        float br = 0.28;
        vec2 bp = vec2(cos(ba) * br, -sin(ba) * br);
        float bd = length(uv - bp);
        float angle_to_blip = atan(-bp.y, bp.x);
        float blip_da = mod(angle_to_blip - sweep_angle + 6.28318530718 * 2.0, 6.28318530718);
        float blip_decay = exp(-blip_da * 2.0);
        blip_brightness += exp(-bd * bd / (2.0 * glow_r * glow_r)) * blip_decay * bass_boost;
    }
    // Contact 1
    {
        float ba = 1.1;
        float br = 0.35;
        vec2 bp = vec2(cos(ba) * br, -sin(ba) * br);
        float bd = length(uv - bp);
        float angle_to_blip = atan(-bp.y, bp.x);
        float blip_da = mod(angle_to_blip - sweep_angle + 6.28318530718 * 2.0, 6.28318530718);
        float blip_decay = exp(-blip_da * 2.0);
        blip_brightness += exp(-bd * bd / (2.0 * glow_r * glow_r)) * blip_decay * bass_boost;
    }
    // Contact 2
    {
        float ba = 2.0;
        float br = 0.18;
        vec2 bp = vec2(cos(ba) * br, -sin(ba) * br);
        float bd = length(uv - bp);
        float angle_to_blip = atan(-bp.y, bp.x);
        float blip_da = mod(angle_to_blip - sweep_angle + 6.28318530718 * 2.0, 6.28318530718);
        float blip_decay = exp(-blip_da * 2.0);
        blip_brightness += exp(-bd * bd / (2.0 * glow_r * glow_r)) * blip_decay * bass_boost;
    }
    // Contact 3
    {
        float ba = 2.8;
        float br = 0.38;
        vec2 bp = vec2(cos(ba) * br, -sin(ba) * br);
        float bd = length(uv - bp);
        float angle_to_blip = atan(-bp.y, bp.x);
        float blip_da = mod(angle_to_blip - sweep_angle + 6.28318530718 * 2.0, 6.28318530718);
        float blip_decay = exp(-blip_da * 2.0);
        blip_brightness += exp(-bd * bd / (2.0 * glow_r * glow_r)) * blip_decay * bass_boost;
    }
    // Contact 4
    {
        float ba = 3.7;
        float br = 0.22;
        vec2 bp = vec2(cos(ba) * br, -sin(ba) * br);
        float bd = length(uv - bp);
        float angle_to_blip = atan(-bp.y, bp.x);
        float blip_da = mod(angle_to_blip - sweep_angle + 6.28318530718 * 2.0, 6.28318530718);
        float blip_decay = exp(-blip_da * 2.0);
        blip_brightness += exp(-bd * bd / (2.0 * glow_r * glow_r)) * blip_decay * bass_boost;
    }
    // Contact 5
    {
        float ba = 4.5;
        float br = 0.31;
        vec2 bp = vec2(cos(ba) * br, -sin(ba) * br);
        float bd = length(uv - bp);
        float angle_to_blip = atan(-bp.y, bp.x);
        float blip_da = mod(angle_to_blip - sweep_angle + 6.28318530718 * 2.0, 6.28318530718);
        float blip_decay = exp(-blip_da * 2.0);
        blip_brightness += exp(-bd * bd / (2.0 * glow_r * glow_r)) * blip_decay * bass_boost;
    }
    // Contact 6
    {
        float ba = 5.2;
        float br = 0.40;
        vec2 bp = vec2(cos(ba) * br, -sin(ba) * br);
        float bd = length(uv - bp);
        float angle_to_blip = atan(-bp.y, bp.x);
        float blip_da = mod(angle_to_blip - sweep_angle + 6.28318530718 * 2.0, 6.28318530718);
        float blip_decay = exp(-blip_da * 2.0);
        blip_brightness += exp(-bd * bd / (2.0 * glow_r * glow_r)) * blip_decay * bass_boost;
    }
    // Contact 7
    {
        float ba = 5.9;
        float br = 0.15;
        vec2 bp = vec2(cos(ba) * br, -sin(ba) * br);
        float bd = length(uv - bp);
        float angle_to_blip = atan(-bp.y, bp.x);
        float blip_da = mod(angle_to_blip - sweep_angle + 6.28318530718 * 2.0, 6.28318530718);
        float blip_decay = exp(-blip_da * 2.0);
        blip_brightness += exp(-bd * bd / (2.0 * glow_r * glow_r)) * blip_decay * bass_boost;
    }

    // Combine: background tint + sweep trail + blips + ring + dot
    float luminance = clamp(
        trail * 0.4 + arm * 0.9 + blip_brightness + ring * 0.25 + tick * 0.15 + dot_glow * 0.8,
        0.0, 1.0
    );

    // Dark green phosphor (#001800 base)
    vec3 phosphor = vec3(0.1, 1.0, 0.15);
    vec3 bg = vec3(0.0, 0.094, 0.0);
    gl_FragColor = vec4(bg + phosphor * luminance, 1.0);
}
