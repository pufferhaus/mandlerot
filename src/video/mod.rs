//! Video capture subsystem. Pulls live frames from a USB capture device
//! (V4L2 on Linux, AVFoundation on macOS, MediaFoundation on Windows)
//! into a lock-free `ArcSwap<Arc<VideoFrame>>`. See
//! `docs/superpowers/specs/2026-05-16-video-input-design.md`.

pub mod frame;

pub use frame::VideoFrame;
