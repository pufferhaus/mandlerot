//! Status panel worker thread. Owns the framebuffer + a previous-grid for
//! diffing. Pulls `StateSnapshot` values via mpsc; the main loop sends
//! once per frame (drop-old strategy).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::scene::SceneLibrary;
use crate::state::SharedState;

use super::compose::state_to_grid;
use super::grid::TextScreen;
use super::render::{Fb, PANEL_H, PANEL_W};
use super::Backend;

/// Cheap copy of state for cross-thread send. Avoids cloning the full library.
pub struct StateSnapshot {
    pub state: SharedState,
    /// If a menu screen is open on the main thread, the pre-rendered grid
    /// is sent here. The worker thread blits this directly instead of
    /// composing from `state` — so menu rendering doesn't need to know
    /// about scene library plumbing.
    pub menu_grid: Option<super::grid::TextScreen>,
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

pub fn spawn(
    mut backend: Box<dyn Backend>,
    library: SceneLibrary,
) -> (StatusHandle, std::thread::JoinHandle<()>) {
    let (tx, rx) = std::sync::mpsc::sync_channel::<StateSnapshot>(1);
    let stop = Arc::new(AtomicBool::new(false));
    let stop_t = stop.clone();
    let handle = std::thread::spawn(move || {
        let mut prev = TextScreen::new();
        let mut fb = Fb::new(PANEL_W, PANEL_H);
        // Initial full clear push so the panel boots into the amber bg.
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
                let next = match snap.menu_grid {
                    Some(g) => g,
                    None => state_to_grid(&snap.state, &library),
                };
                let runs = next.diff_runs(&prev);
                if !runs.is_empty() {
                    super::render::render_runs(&next, &runs, &mut fb);
                    if let Err(e) = backend.flush_runs(&fb, &runs) {
                        tracing::warn!("status flush_runs: {e}");
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
