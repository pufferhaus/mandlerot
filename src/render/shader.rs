//! Shader source assembly. Pure string operations — no GL calls.

pub const PRELUDE: &str = include_str!("../../shaders/prelude.glsl");
pub const POSTFX_PRELUDE: &str = include_str!("../../shaders/postfx_prelude.glsl");
pub const QUAD_VERT: &str = include_str!("../../shaders/quad.vert");
pub const BLEND_FRAG: &str = include_str!("../../shaders/blend.glsl");
pub const SAFE_SCENE: &str = include_str!("../../shaders/safe_scene.glsl");

/// Combine the prelude with a user scene fragment shader body.
pub fn assemble_scene_fragment(user_body: &str) -> String {
    let mut s = String::with_capacity(PRELUDE.len() + user_body.len() + 1);
    s.push_str(PRELUDE);
    if !PRELUDE.ends_with('\n') {
        s.push('\n');
    }
    s.push_str(user_body);
    s
}

/// Combine the post-FX prelude (smaller — no `u_audio`/`u_prev`) with a user
/// pass body. Mirrors `assemble_scene_fragment` so the rest of the pipeline
/// stays type-symmetric.
pub fn assemble_postfx_fragment(user_body: &str) -> String {
    let mut s = String::with_capacity(POSTFX_PRELUDE.len() + user_body.len() + 1);
    s.push_str(POSTFX_PRELUDE);
    if !POSTFX_PRELUDE.ends_with('\n') {
        s.push('\n');
    }
    s.push_str(user_body);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prelude_declares_required_uniforms() {
        assert!(PRELUDE.contains("uniform float u_time;"));
        assert!(PRELUDE.contains("uniform vec4  u_audio;"));
        assert!(PRELUDE.contains("varying vec2 v_uv;"));
        for slot in 0..9 {
            assert!(
                PRELUDE.contains(&format!("uniform float u_param{};", slot)),
                "missing u_param{}",
                slot
            );
        }
    }

    #[test]
    fn assembled_scene_starts_with_version() {
        let body = "void main() { gl_FragColor = vec4(1.0); }";
        let s = assemble_scene_fragment(body);
        assert!(s.starts_with("#version 100"));
        assert!(s.contains(body));
    }

    #[test]
    fn safe_scene_uses_uniforms_from_prelude() {
        // Ensure baked-in safe_scene compiles when assembled (textually checked here;
        // GL compile happens in pipeline tests).
        let s = assemble_scene_fragment(SAFE_SCENE);
        assert!(s.contains("u_time"));
    }

    #[test]
    fn blend_shader_has_all_modes() {
        // Every mode 0..=18 is dispatched via explicit `u_blend_mode == N`.
        // The final `else` is a safe fallback, not a numbered mode.
        for mode in 0..=18 {
            assert!(
                BLEND_FRAG.contains(&format!("u_blend_mode == {}", mode)),
                "blend mode {} dispatch missing",
                mode
            );
        }
    }
}
