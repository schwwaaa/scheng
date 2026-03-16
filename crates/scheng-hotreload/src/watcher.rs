//! `watcher.rs` — cross-platform file system event watcher.
//!
//! Uses `notify` v6: FSEvents on macOS, inotify on Linux, ReadDirectoryChangesW on Windows.
//! Events are sent over a channel to the main thread. The render loop drains
//! the channel each frame — no blocking.

use std::{
    path::{Path, PathBuf},
    sync::mpsc::{channel, Receiver, TryRecvError},
};

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::HotReloadError;

/// A change event reported by the watcher.
#[derive(Debug, Clone)]
pub struct FileChange {
    /// Absolute path of the changed file.
    pub path: PathBuf,
    /// The kind of change.
    pub kind: ChangeKind,
}

/// Kind of file system change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeKind {
    /// File was modified (write or truncation).
    Modified,
    /// File was created.
    Created,
    /// File was removed.
    Removed,
}

/// File system watcher over an `assets/` directory.
pub struct AssetWatcher {
    /// The notify watcher — must stay alive for the lifetime of the subscription.
    _watcher: RecommendedWatcher,
    receiver: Receiver<FileChange>,
}

impl AssetWatcher {
    /// Start watching `assets_dir` recursively.
    pub fn new(assets_dir: &str) -> Result<Self, HotReloadError> {
        let (tx, rx) = channel::<FileChange>();

        let mut watcher = RecommendedWatcher::new(
            move |result: notify::Result<Event>| {
                let Ok(event) = result else { return };
                let kind = match event.kind {
                    EventKind::Modify(_) => ChangeKind::Modified,
                    EventKind::Create(_) => ChangeKind::Created,
                    EventKind::Remove(_) => ChangeKind::Removed,
                    _ => return,
                };
                for path in event.paths {
                    let _ = tx.send(FileChange { path, kind: kind.clone() });
                }
            },
            Config::default(),
        )
        .map_err(|e| HotReloadError::Watch {
            path:   assets_dir.into(),
            source: e,
        })?;

        watcher.watch(Path::new(assets_dir), RecursiveMode::Recursive)
            .map_err(|e| HotReloadError::Watch {
                path:   assets_dir.into(),
                source: e,
            })?;

        log::info!("Hot-reload watching: {}", assets_dir);

        Ok(Self { _watcher: watcher, receiver: rx })
    }

    /// Drain all pending change events. Non-blocking.
    pub fn drain(&self) -> Vec<FileChange> {
        let mut events = Vec::new();
        loop {
            match self.receiver.try_recv() {
                Ok(e)                          => events.push(e),
                Err(TryRecvError::Empty)       => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }
        events
    }
}
