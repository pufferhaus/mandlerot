//! Fullscreen triangle/quad geometry shared by every draw pass.
//!
//! Two-triangle quad over NDC [-1, 1] × [-1, 1]. Vertex shader maps
//! `a_pos * 0.5 + 0.5` to `v_uv`.

pub const QUAD_POSITIONS: &[f32] = &[
    -1.0, -1.0, // bottom-left
    3.0, -1.0, // bottom-right (extends past, big triangle)
    -1.0, 3.0, // top-left (extends past)
];

pub const VERTEX_COUNT: i32 = 3;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quad_has_three_vertices() {
        assert_eq!(QUAD_POSITIONS.len(), 6); // 3 verts × 2 components
        assert_eq!(VERTEX_COUNT, 3);
    }

    #[test]
    fn quad_covers_ndc() {
        // The big triangle covers NDC [-1,1] × [-1,1] entirely after clipping.
        // Bottom-left at (-1, -1), opposite corner at (3, 3) ensures the
        // hypotenuse passes outside the (1, 1) corner.
        let bl = (QUAD_POSITIONS[0], QUAD_POSITIONS[1]);
        let br = (QUAD_POSITIONS[2], QUAD_POSITIONS[3]);
        let tl = (QUAD_POSITIONS[4], QUAD_POSITIONS[5]);
        assert_eq!(bl, (-1.0, -1.0));
        assert!(br.0 >= 1.0);
        assert!(tl.1 >= 1.0);
    }
}
