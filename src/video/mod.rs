//! Video capture subsystem. Pulls live frames from a USB capture device
//! (V4L2 on Linux, AVFoundation on macOS, MediaFoundation on Windows)
//! into a lock-free `ArcSwap<Arc<VideoFrame>>`. See
//! `docs/superpowers/specs/2026-05-16-video-input-design.md`.

pub mod frame;

#[cfg(feature = "video")]
pub mod capture;

pub use frame::VideoFrame;

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;

/// Capture state surfaced to the UI / status panel. Packed into an
/// `AtomicU8` so reads from the render thread are lock-free.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoStatus {
    /// No device opened yet, or last open failed. UI shows `VID:--`.
    NoDevice,
    /// Frames flowing within the freshness window. UI shows `VID:OK WxH`.
    Active { width: u32, height: u32 },
    /// Last frame is older than the freshness window. UI shows `VID:STALE`.
    Stale,
    /// Capture thread encountered repeated decode failures. UI shows `VID:ERR`.
    Error,
}

impl VideoStatus {
    fn pack(self) -> u8 {
        match self {
            VideoStatus::NoDevice => 0,
            VideoStatus::Active { .. } => 1,
            VideoStatus::Stale => 2,
            VideoStatus::Error => 3,
        }
    }

    fn unpack(byte: u8, w: u32, h: u32) -> Self {
        match byte {
            1 => VideoStatus::Active {
                width: w,
                height: h,
            },
            2 => VideoStatus::Stale,
            3 => VideoStatus::Error,
            _ => VideoStatus::NoDevice,
        }
    }

    /// Compact 6-char chip for the top status bar. Dimensions are dropped
    /// here (they live in the log line instead) so the chip aligns cleanly
    /// alongside the existing MODE/BLEND/POST/BPM/AUD chips.
    pub fn as_chip(&self) -> &'static str {
        match self {
            VideoStatus::NoDevice => "VID:--",
            VideoStatus::Active { .. } => "VID:OK",
            VideoStatus::Stale => "VID:ST",
            VideoStatus::Error => "VID:ER",
        }
    }

    /// Verbose form for tracing logs / debug dumps.
    pub fn as_log_str(&self) -> String {
        match self {
            VideoStatus::NoDevice => "VID:--".into(),
            VideoStatus::Active { width, height } => format!("VID:OK {}x{}", width, height),
            VideoStatus::Stale => "VID:STALE".into(),
            VideoStatus::Error => "VID:ERR".into(),
        }
    }
}

/// Stale-frame threshold. Frames older than this flip the status to
/// `Stale` even if the capture thread hasn't reported an error.
pub const STALE_AFTER: Duration = Duration::from_millis(1000);

/// Caller-supplied capture preferences. `device = None` = autodetect.
#[derive(Debug, Clone, Default)]
pub struct VideoPrefs {
    pub device: Option<String>,
    /// Target capture width / height. Real source dims may differ — the
    /// pipeline uses `u_video_uv_scale` to sample only the populated rect.
    pub target_width: u32,
    pub target_height: u32,
}

impl VideoPrefs {
    pub fn default_pi() -> Self {
        Self {
            device: None,
            target_width: 720,
            target_height: 480,
        }
    }
}

/// Handle passed to the render thread. Lock-free reads.
pub struct VideoHandle {
    swap: Arc<ArcSwap<VideoFrame>>,
    status: Arc<AtomicU8>,
    last_dims: Arc<std::sync::atomic::AtomicU64>, // packed (w<<32)|h
}

impl VideoHandle {
    /// Construct a handle wired to the given shared state. Used by both
    /// the real capture thread and tests.
    pub fn new(
        swap: Arc<ArcSwap<VideoFrame>>,
        status: Arc<AtomicU8>,
        last_dims: Arc<std::sync::atomic::AtomicU64>,
    ) -> Self {
        Self {
            swap,
            status,
            last_dims,
        }
    }

    /// Build a permanently-NoDevice handle. Used when the `video` feature
    /// is off or capture-thread spawn failed.
    pub fn stub() -> Self {
        let swap = Arc::new(ArcSwap::from_pointee(VideoFrame::black_placeholder(0)));
        let status = Arc::new(AtomicU8::new(VideoStatus::NoDevice.pack()));
        let last_dims = Arc::new(std::sync::atomic::AtomicU64::new(0));
        Self::new(swap, status, last_dims)
    }

    pub fn latest_frame(&self) -> Arc<VideoFrame> {
        self.swap.load_full()
    }

    pub fn status(&self) -> VideoStatus {
        let byte = self.status.load(Ordering::Relaxed);
        let packed = self.last_dims.load(Ordering::Relaxed);
        let w = (packed >> 32) as u32;
        let h = (packed & 0xFFFF_FFFF) as u32;
        VideoStatus::unpack(byte, w, h)
    }

    /// Called by the render thread once per frame: if the most recently
    /// published frame is older than `STALE_AFTER`, promote the status
    /// from `Active` to `Stale`. Cheap (one atomic load + Instant compare).
    /// The capture thread will overwrite `Stale` → `Active` automatically
    /// the next time a frame arrives.
    pub fn refresh_stale(&self) {
        let frame = self.swap.load();
        let cur = self.status.load(Ordering::Relaxed);
        let is_active = cur == VideoStatus::Active { width: 0, height: 0 }.pack();
        if is_active && frame.ts.elapsed() > STALE_AFTER {
            self.status
                .store(VideoStatus::Stale.pack(), Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_packs_and_unpacks_cleanly() {
        for s in [
            VideoStatus::NoDevice,
            VideoStatus::Active {
                width: 720,
                height: 480,
            },
            VideoStatus::Stale,
            VideoStatus::Error,
        ] {
            let packed = s.pack();
            let (w, h) = match s {
                VideoStatus::Active { width, height } => (width, height),
                _ => (0, 0),
            };
            assert_eq!(VideoStatus::unpack(packed, w, h), s);
        }
    }

    #[test]
    fn stub_handle_reads_as_no_device_with_one_by_one_black() {
        let h = VideoHandle::stub();
        assert_eq!(h.status(), VideoStatus::NoDevice);
        let f = h.latest_frame();
        assert_eq!(f.width, 1);
        assert_eq!(f.height, 1);
    }

    #[test]
    fn status_chip_strings_are_six_chars() {
        // Top-bar chip is fixed-width to align with the other 6-char chips.
        for s in [
            VideoStatus::NoDevice,
            VideoStatus::Active { width: 720, height: 480 },
            VideoStatus::Stale,
            VideoStatus::Error,
        ] {
            assert_eq!(s.as_chip().len(), 6, "as_chip() must be 6 chars: {:?}", s);
        }
        assert_eq!(VideoStatus::NoDevice.as_chip(), "VID:--");
        assert_eq!(VideoStatus::Stale.as_chip(), "VID:ST");
        assert_eq!(VideoStatus::Error.as_chip(), "VID:ER");
        assert_eq!(
            VideoStatus::Active { width: 720, height: 480 }.as_chip(),
            "VID:OK"
        );
        // Verbose form keeps dims for logs.
        assert_eq!(
            VideoStatus::Active { width: 720, height: 480 }.as_log_str(),
            "VID:OK 720x480"
        );
    }
}
