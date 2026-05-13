//! Status panel worker thread. Owns the framebuffer + a previous-grid for
//! diffing. Pulls `StateSnapshot` values via mpsc; the main loop sends
//! once per frame (drop-old strategy).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::compose::state_to_grid;
use super::grid::TextScreen;
use super::render::{Fb, PANEL_H, PANEL_W};
use super::snapshot::PanelSnapshot;
use super::sysmon::SysMon;
use super::Backend;

/// Cheap, fixed-shape snapshot sent from the render thread to the status
/// worker every frame. Was previously a `SharedState` clone (~30 string
/// allocations per frame) — `PanelSnapshot` strips it down to ~2 (the two
/// scene names).
pub struct StateSnapshot {
    pub panel: PanelSnapshot,
    /// If a menu screen is open on the main thread, the pre-rendered grid
    /// is sent here. The worker thread blits this directly instead of
    /// composing from `panel` — so menu rendering doesn't need to know
    /// about scene library plumbing.
    pub menu_grid: Option<super::grid::TextScreen>,
    /// Pre-rendered post-FX summary tag (e.g. `vig+grn`). Built on the main
    /// thread because the live `PostFx` lives in the pipeline and isn't
    /// `Send`. Empty string = no passes / "off".
    pub postfx_summary: String,
    /// Smoothed render fps measured on the main thread. `None` for the first
    /// few frames before a usable sample exists.
    pub fps: Option<f32>,
}

pub struct StatusHandle {
    tx: SyncSender<StateSnapshot>,
    pub stop: Arc<AtomicBool>,
}

impl StatusHandle {
    pub fn try_send(&self, snap: StateSnapshot) {
        // Drop-old: a slow panel must not back-pressure the render thread.
        match self.tx.try_send(snap) {
            Ok(_) | Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {}
        }
    }
}

pub fn spawn(mut backend: Box<dyn Backend>) -> (StatusHandle, std::thread::JoinHandle<()>) {
    let (tx, rx) = std::sync::mpsc::sync_channel::<StateSnapshot>(1);
    let stop = Arc::new(AtomicBool::new(false));
    let stop_t = stop.clone();
    let handle = std::thread::spawn(move || {
        let mut prev = TextScreen::new();
        let mut fb = Fb::new(PANEL_W, PANEL_H);
        // System stats sampled at 1 Hz inside the worker. Cost on a Pi is
        // ~50 µs of tmpfs reads per sample — invisible compared to the SPI
        // flush further down.
        let mut sysmon = SysMon::new();
        // Initial full clear push so the panel boots into the amber bg
        // *and* primes the mmap with a known baseline so subsequent
        // `flush_runs` calls only touch changed regions.
        if let Err(e) = backend.flush_full(&fb) {
            tracing::warn!("status initial flush: {e}");
        }
        let dt = Duration::from_millis(100); // ~10 Hz
        let mut last = Instant::now();
        while !stop_t.load(Ordering::Relaxed) {
            // Drain channel, keep newest.
            let mut latest: Option<StateSnapshot> = None;
            while let Ok(snap) = rx.try_recv() {
                latest = Some(snap);
            }
            if let Some(snap) = latest {
                sysmon.maybe_sample(Instant::now());
                let next = match snap.menu_grid {
                    Some(g) => g,
                    None => state_to_grid(
                        &snap.panel,
                        &snap.postfx_summary,
                        &sysmon,
                        snap.fps,
                    ),
                };
                let runs = next.diff_runs(&prev);
                if !runs.is_empty() {
                    super::render::render_runs(&next, &runs, &mut fb);
                    // Reverted from `flush_runs` → `flush_full` after the
                    // sysmon line was reported to "wrap around" between
                    // ticks on the actual MPI3501 panel. The text content
                    // is provably width-stable (see compose tests), so
                    // the artefact lived in the partial-update path:
                    // probably the ILI9486 + fbtft dirty-page tracking
                    // gets confused by sparse non-contiguous mmap writes.
                    // Full-frame push is ~76 ms over 32 MHz SPI which is
                    // still inside the 100 ms worker tick budget, and the
                    // panel updates correctly. If we ever need partial
                    // updates back, do it by row (480×12 pixels at a
                    // time) rather than by cell.
                    if let Err(e) = backend.flush_full(&fb) {
                        tracing::warn!("status flush_full: {e}");
                    }
                }
                prev = next;
            }
            let elapsed = last.elapsed();
            if elapsed < dt {
                std::thread::sleep(dt - elapsed);
            }
            last = Instant::now();
        }
    });
    (StatusHandle { tx, stop }, handle)
}
