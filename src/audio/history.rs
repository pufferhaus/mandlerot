//! Ring buffer of recent audio band readings. Updated each render frame
//! (NOT each audio thread tick — sampling at the render rate matches what
//! the shader would have observed via u_prev scrolling).

use std::sync::{Arc, Mutex};

pub const HISTORY_LEN: usize = 320;

pub struct AudioHistory {
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    /// Ring of bands: each entry is [bass, lomid, himid, treble] in 0..=255.
    /// Stored as u8 for direct texture upload (RGBA8).
    buf: Vec<[u8; 4]>,
    head: usize, // next write index
    filled: usize,
}

impl AudioHistory {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                buf: vec![[0u8; 4]; HISTORY_LEN],
                head: 0,
                filled: 0,
            })),
        }
    }

    pub fn push(&self, bands: [f32; 4]) {
        let mut g = self.inner.lock().unwrap();
        let entry = [
            (bands[0].clamp(0.0, 1.0) * 255.0) as u8,
            (bands[1].clamp(0.0, 1.0) * 255.0) as u8,
            (bands[2].clamp(0.0, 1.0) * 255.0) as u8,
            (bands[3].clamp(0.0, 1.0) * 255.0) as u8,
        ];
        let head = g.head;
        g.buf[head] = entry;
        g.head = (head + 1) % HISTORY_LEN;
        g.filled = (g.filled + 1).min(HISTORY_LEN);
    }

    /// Snapshot the buffer in **time order** (oldest first, newest last).
    /// Returns RGBA8 byte vec ready for `tex_image_2d` (1 × 320 × 4 bytes).
    pub fn snapshot_rgba(&self) -> Vec<u8> {
        let mut out = vec![0u8; HISTORY_LEN * 4];
        self.snapshot_into(&mut out);
        out
    }

    /// Same as `snapshot_rgba` but writes into a caller-owned buffer. The
    /// render loop allocates a single 1280-byte scratch buffer at startup
    /// and reuses it every frame, dropping ~30 allocator round-trips per
    /// second on the Pi 3B+ where the global allocator competes with the
    /// audio worker thread for the same heap.
    pub fn snapshot_into(&self, dst: &mut [u8]) {
        debug_assert_eq!(dst.len(), HISTORY_LEN * 4);
        let g = self.inner.lock().unwrap();
        // Read from oldest to newest. With a partially-filled ring, `head`
        // points at the next write slot, which is also the oldest entry
        // once we've wrapped at least once. Before wrap, slots ahead of
        // `head` are still the zero-initialised values, which read
        // correctly as silence.
        let start = g.head;
        for i in 0..HISTORY_LEN {
            let idx = (start + i) % HISTORY_LEN;
            let e = g.buf[idx];
            let off = i * 4;
            dst[off..off + 4].copy_from_slice(&e);
        }
    }

    pub fn handle(&self) -> AudioHistoryHandle {
        AudioHistoryHandle {
            inner: self.inner.clone(),
        }
    }
}

#[derive(Clone)]
pub struct AudioHistoryHandle {
    inner: Arc<Mutex<Inner>>,
}

impl AudioHistoryHandle {
    pub fn snapshot_rgba(&self) -> Vec<u8> {
        let g = self.inner.lock().unwrap();
        let mut out = Vec::with_capacity(HISTORY_LEN * 4);
        let start = g.head;
        for i in 0..HISTORY_LEN {
            let idx = (start + i) % HISTORY_LEN;
            let e = g.buf[idx];
            out.extend_from_slice(&e);
        }
        out
    }
}

impl Default for AudioHistory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_buffer_is_all_zero() {
        let h = AudioHistory::new();
        let bytes = h.snapshot_rgba();
        assert_eq!(bytes.len(), HISTORY_LEN * 4);
        assert!(bytes.iter().all(|b| *b == 0));
    }

    #[test]
    fn push_then_snapshot_in_time_order() {
        let h = AudioHistory::new();
        // Push two distinct values.
        h.push([0.1, 0.2, 0.3, 0.4]);
        h.push([0.5, 0.6, 0.7, 0.8]);
        let bytes = h.snapshot_rgba();
        // Time order: oldest first → newest last. After two pushes with head
        // starting at 0, head=2, so snapshot reads slots [2,3,...,319,0,1].
        // Slot 0 holds the first push, slot 1 the second. They land at the
        // last two texels of the snapshot.
        let oldest_off = (HISTORY_LEN - 2) * 4;
        let newest_off = (HISTORY_LEN - 1) * 4;
        assert_eq!(bytes[oldest_off], (0.1 * 255.0) as u8);
        assert_eq!(bytes[newest_off], (0.5 * 255.0) as u8);
    }

    #[test]
    fn ring_overwrites_when_full() {
        let h = AudioHistory::new();
        for i in 0..(HISTORY_LEN + 5) {
            let v = (i % 256) as f32 / 255.0;
            h.push([v, v, v, v]);
        }
        let bytes = h.snapshot_rgba();
        // Newest (last 4 bytes) = (HISTORY_LEN+4) % 256
        let last = bytes[bytes.len() - 4];
        assert_eq!(last, ((HISTORY_LEN + 4) % 256) as u8);
    }

    #[test]
    fn handle_sees_same_data() {
        let h = AudioHistory::new();
        let handle = h.handle();
        h.push([1.0, 0.0, 0.0, 0.0]);
        let from_handle = handle.snapshot_rgba();
        let from_owner = h.snapshot_rgba();
        assert_eq!(from_handle, from_owner);
        // Newest texel R should be 255.
        assert_eq!(from_handle[from_handle.len() - 4], 255);
    }
}
