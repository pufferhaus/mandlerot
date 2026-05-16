//! Captured video frame. Shared between the capture thread (writer) and
//! the render thread (reader) via `Arc<VideoFrame>`. The pixel buffer is
//! itself an `Arc<[u8]>` so frame clones never copy pixel data.

use std::sync::Arc;
use std::time::Instant;

#[derive(Clone)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<[u8]>,
    pub seq: u64,
    pub ts: Instant,
}

impl VideoFrame {
    /// Construct a single-colour frame at the given dims. Used by the
    /// capture thread's startup placeholder and the unit tests.
    pub fn solid(width: u32, height: u32, rgba: [u8; 4], seq: u64) -> Self {
        let pixels = (width as usize) * (height as usize);
        let mut buf = vec![0u8; pixels * 4];
        for chunk in buf.chunks_exact_mut(4) {
            chunk.copy_from_slice(&rgba);
        }
        Self {
            width,
            height,
            rgba: buf.into(),
            seq,
            ts: Instant::now(),
        }
    }

    /// 1×1 opaque black. The startup default before any capture frame.
    pub fn black_placeholder(seq: u64) -> Self {
        Self::solid(1, 1, [0, 0, 0, 255], seq)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_frame_has_expected_buffer_length() {
        let f = VideoFrame::solid(4, 2, [10, 20, 30, 40], 0);
        assert_eq!(f.rgba.len(), 4 * 2 * 4);
        assert_eq!(f.rgba[0], 10);
        assert_eq!(f.rgba[1], 20);
        assert_eq!(f.rgba[2], 30);
        assert_eq!(f.rgba[3], 40);
        // last pixel still matches
        assert_eq!(f.rgba[4 * 2 * 4 - 1], 40);
    }

    #[test]
    fn cloning_a_frame_does_not_copy_pixels() {
        let f = VideoFrame::solid(64, 64, [255, 0, 0, 255], 7);
        let g = f.clone();
        // Arc::ptr_eq on the slice — same allocation, no copy.
        assert!(Arc::ptr_eq(&f.rgba, &g.rgba));
        assert_eq!(g.seq, 7);
    }

    #[test]
    fn black_placeholder_is_one_by_one() {
        let f = VideoFrame::black_placeholder(0);
        assert_eq!(f.width, 1);
        assert_eq!(f.height, 1);
        assert_eq!(f.rgba.as_ref(), &[0, 0, 0, 255]);
    }
}
