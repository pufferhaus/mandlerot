//! Filesystem watcher → `ReloadEvent` channel.
//!
//! Wraps `notify` so the rest of the program just polls a channel.

use std::path::Path;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReloadEvent {
    /// `<stem>.glsl` or `<stem>.toml` was created or modified.
    SceneTouched { stem: String },
    /// `<stem>.glsl` or `<stem>.toml` was removed.
    SceneRemoved { stem: String },
}

pub struct HotReloader {
    _watcher: RecommendedWatcher,
    rx: Receiver<ReloadEvent>,
}

impl HotReloader {
    pub fn watch(dir: &Path) -> Result<Self> {
        let (tx, rx) = std::sync::mpsc::channel::<ReloadEvent>();
        let dir_buf = dir.to_path_buf();
        let mut watcher =
            notify::recommended_watcher(move |res: notify::Result<notify::Event>| match res {
                Ok(event) => emit_for_event(&dir_buf, &event, &tx),
                Err(e) => tracing::warn!("notify error: {e}"),
            })
            .map_err(|e| Error::Backend(format!("notify watcher: {e}")))?;
        watcher
            .watch(dir, RecursiveMode::NonRecursive)
            .map_err(|e| Error::Backend(format!("notify watch: {e}")))?;
        Ok(Self {
            _watcher: watcher,
            rx,
        })
    }

    pub fn try_recv(&self) -> Option<ReloadEvent> {
        self.rx.try_recv().ok()
    }

    pub fn recv_timeout(&self, dur: Duration) -> Option<ReloadEvent> {
        self.rx.recv_timeout(dur).ok()
    }
}

fn emit_for_event(_dir: &Path, event: &notify::Event, tx: &Sender<ReloadEvent>) {
    let stems = event
        .paths
        .iter()
        .filter(|p| matches_scene_extension(p))
        .filter_map(|p| {
            p.file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
        });
    for stem in stems {
        let evt = match event.kind {
            EventKind::Remove(_) => ReloadEvent::SceneRemoved { stem },
            _ => ReloadEvent::SceneTouched { stem },
        };
        let _ = tx.send(evt);
    }
}

fn matches_scene_extension(p: &Path) -> bool {
    matches!(
        p.extension().and_then(|s| s.to_str()),
        Some("glsl") | Some("toml")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn detects_glsl_write() {
        let tmp = tempfile::tempdir().unwrap();
        let watcher = HotReloader::watch(tmp.path()).unwrap();
        // Some platforms emit nothing for the create-and-write done in one go;
        // do a write, then a follow-up modify, to be robust.
        let path = tmp.path().join("my.glsl");
        std::fs::write(&path, "void main() {}").unwrap();
        std::fs::write(&path, "void main() { gl_FragColor = vec4(1.0); }").unwrap();
        let mut got = None;
        for _ in 0..20 {
            if let Some(e) = watcher.recv_timeout(Duration::from_millis(200)) {
                if matches!(e, ReloadEvent::SceneTouched { .. }) {
                    got = Some(e);
                    break;
                }
            }
        }
        assert!(matches!(got, Some(ReloadEvent::SceneTouched { stem }) if stem == "my"));
    }
}
