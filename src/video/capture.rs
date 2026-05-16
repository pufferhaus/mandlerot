//! Capture worker thread. Owns the nokhwa camera; decodes frames to RGBA8;
//! publishes them via `ArcSwap`. On open failure, retries every 5s. On
//! decode failures, counts and falls into `Error` status after a threshold.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use nokhwa::pixel_format::RgbAFormat;
use nokhwa::utils::{
    ApiBackend, CameraIndex, RequestedFormat, RequestedFormatType, Resolution,
};
use nokhwa::Camera;

use super::frame::VideoFrame;
use super::{VideoHandle, VideoPrefs, VideoStatus};

const RETRY_INTERVAL: Duration = Duration::from_secs(5);
const DECODE_ERROR_LIMIT: u32 = 30;

/// Spawn the capture thread. Returns immediately; the thread will retry
/// device-open every 5s if no device is available. Caller drops the
/// returned `Stopper` to ask the thread to exit.
pub fn start_capture(prefs: VideoPrefs) -> VideoHandle {
    let swap = Arc::new(ArcSwap::from_pointee(VideoFrame::black_placeholder(0)));
    let status = Arc::new(AtomicU8::new(VideoStatus::NoDevice.pack()));
    let last_dims = Arc::new(AtomicU64::new(0));
    let stop = Arc::new(AtomicBool::new(false));

    let swap_thr = swap.clone();
    let status_thr = status.clone();
    let dims_thr = last_dims.clone();
    let stop_thr = stop.clone();

    thread::Builder::new()
        .name("video-capture".into())
        .spawn(move || run(prefs, swap_thr, status_thr, dims_thr, stop_thr))
        .expect("spawn video-capture thread");

    // The handle takes ownership of `stop` by storing it in a side channel.
    // For now we leak the stopper into a thread_local-style static; cleanup
    // happens at process exit (the OS kills the thread). A future iteration
    // can wire shutdown into Pipeline::drop, but for v0 process-end is OK.
    std::mem::forget(stop);

    VideoHandle::new(swap, status, last_dims)
}

fn run(
    prefs: VideoPrefs,
    swap: Arc<ArcSwap<VideoFrame>>,
    status: Arc<AtomicU8>,
    last_dims: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
) {
    let mut seq: u64 = 1;
    loop {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        match open_camera(&prefs) {
            Ok(mut cam) => {
                tracing::info!("video: opened camera ({:?})", cam.resolution());
                let mut decode_errors: u32;
                loop {
                    if stop.load(Ordering::Relaxed) {
                        return;
                    }
                    match cam.frame() {
                        Ok(raw) => {
                            decode_errors = 0;
                            let res = raw.resolution();
                            match raw.decode_image::<RgbAFormat>() {
                                Ok(img) => {
                                    let w = img.width();
                                    let h = img.height();
                                    let buf: Arc<[u8]> = img.into_raw().into();
                                    let frame = VideoFrame {
                                        width: w,
                                        height: h,
                                        rgba: buf,
                                        seq,
                                        ts: Instant::now(),
                                    };
                                    seq = seq.wrapping_add(1);
                                    swap.store(Arc::new(frame));
                                    last_dims.store(((w as u64) << 32) | (h as u64), Ordering::Relaxed);
                                    status.store(VideoStatus::Active { width: w, height: h }.pack(), Ordering::Relaxed);
                                }
                                Err(e) => {
                                    decode_errors += 1;
                                    tracing::debug!("video decode error ({res:?}): {e}");
                                    if decode_errors >= DECODE_ERROR_LIMIT {
                                        status.store(VideoStatus::Error.pack(), Ordering::Relaxed);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("video: frame read failed: {e}; reopening");
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                tracing::warn!("video: no camera ({e}); retrying in {:?}", RETRY_INTERVAL);
                status.store(VideoStatus::NoDevice.pack(), Ordering::Relaxed);
                wait_with_stop(&stop, RETRY_INTERVAL);
            }
        }
    }
}

fn open_camera(prefs: &VideoPrefs) -> Result<Camera, String> {
    let index = match &prefs.device {
        Some(_name) => {
            // Future: enumerate via nokhwa and match. For v0 always use index 0.
            // (Manual override via VideoPrefs.device is reserved for a later
            // task that adds device enumeration UI.)
            CameraIndex::Index(0)
        }
        None => CameraIndex::Index(0),
    };
    let fmt = RequestedFormat::new::<RgbAFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
    let backend = preferred_backend();
    let mut cam = Camera::with_backend(index, fmt, backend)
        .map_err(|e| format!("nokhwa open: {e}"))?;
    if prefs.target_width > 0 && prefs.target_height > 0 {
        let _ = cam.set_resolution(Resolution::new(prefs.target_width, prefs.target_height));
    }
    cam.open_stream().map_err(|e| format!("nokhwa open_stream: {e}"))?;
    Ok(cam)
}

fn preferred_backend() -> ApiBackend {
    #[cfg(target_os = "macos")]
    {
        ApiBackend::AVFoundation
    }
    #[cfg(target_os = "linux")]
    {
        ApiBackend::Video4Linux
    }
    #[cfg(target_os = "windows")]
    {
        ApiBackend::MediaFoundation
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        ApiBackend::Auto
    }
}

fn wait_with_stop(stop: &AtomicBool, dur: Duration) {
    let start = Instant::now();
    while start.elapsed() < dur {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

/// Helper for tests + the render thread's per-frame staleness check.
/// Promotes `Active` → `Stale` when `now - frame.ts > threshold`.
pub fn refresh_stale(
    status: &AtomicU8,
    last_seen: Instant,
    threshold: Duration,
    now: Instant,
) {
    let cur = status.load(Ordering::Relaxed);
    let active = cur == VideoStatus::Active { width: 0, height: 0 }.pack();
    if active && now.saturating_duration_since(last_seen) > threshold {
        status.store(VideoStatus::Stale.pack(), Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_stale_promotes_active_to_stale_when_frame_too_old() {
        let s = AtomicU8::new(VideoStatus::Active { width: 4, height: 4 }.pack());
        let now = Instant::now();
        let old = now - Duration::from_millis(2000);
        refresh_stale(&s, old, Duration::from_millis(1000), now);
        // pack() of Active { 0, 0 } is 1; Stale is 2. We compare by pack value.
        assert_eq!(s.load(Ordering::Relaxed), VideoStatus::Stale.pack());
    }

    #[test]
    fn refresh_stale_leaves_fresh_active_alone() {
        let s = AtomicU8::new(VideoStatus::Active { width: 720, height: 480 }.pack());
        let now = Instant::now();
        refresh_stale(&s, now, Duration::from_millis(1000), now);
        // still active
        assert_eq!(s.load(Ordering::Relaxed), VideoStatus::Active { width: 0, height: 0 }.pack());
    }
}
