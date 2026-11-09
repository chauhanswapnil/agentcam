//! Watch the agents' log folders and re-index after they change.

use std::sync::Arc;
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::Duration;

use notify::{EventKind, RecursiveMode, Watcher};

use crate::AppState;

/// Wait this long after the last change before re-indexing, so an agent
/// writing many lines in a burst causes one run, not dozens.
const SETTLE: Duration = Duration::from_millis(750);
/// An agent that never pauses still gets indexed this often, so the live
/// view keeps up.
const MAX_WAIT: Duration = Duration::from_secs(3);
/// Project files change without the logs changing (you edit code by
/// hand), so re-check which agent lines are still there this often.
const RECHECK: Duration = Duration::from_secs(120);
/// How often to look for a log folder that didn't exist at start.
const LOOK_FOR_FOLDERS: Duration = Duration::from_secs(5);

pub fn start(state: Arc<AppState>) -> bool {
    let (tx, rx) = channel();
    let mut watcher =
        match notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(ev) = res {
                let relevant = matches!(ev.kind, EventKind::Create(_) | EventKind::Modify(_))
                    && ev
                        .paths
                        .iter()
                        .any(|p| p.extension().is_some_and(|e| e == "jsonl"));
                if relevant {
                    let _ = tx.send(());
                }
            }
        }) {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!("can't watch log folders: {e}");
                return false;
            }
        };
    // Folders that don't exist yet (an agent never run on this machine)
    // are checked every few seconds and watched once they appear.
    let mut pending: Vec<std::path::PathBuf> = Vec::new();
    for src in state.sources() {
        if !src.path.is_dir() {
            pending.push(src.path.clone());
            continue;
        }
        if let Err(e) = watcher.watch(&src.path, RecursiveMode::Recursive) {
            tracing::warn!("can't watch {}: {e}", src.path.display());
        }
    }

    let handle = tokio::runtime::Handle::current();
    std::thread::Builder::new()
        .name("agentcam-watch".into())
        .spawn(move || {
            let mut watcher = watcher; // keep it alive for the thread's life
            let _rt = handle.enter();
            let mut last_recheck = std::time::Instant::now();
            loop {
                let wait = if pending.is_empty() {
                    RECHECK
                } else {
                    LOOK_FOR_FOLDERS
                };
                match rx.recv_timeout(wait) {
                    Ok(()) => {}
                    Err(RecvTimeoutError::Timeout) => {
                        let before = pending.len();
                        pending.retain(|p| {
                            if !p.is_dir() {
                                return true;
                            }
                            match watcher.watch(p, RecursiveMode::Recursive) {
                                Ok(()) => {
                                    tracing::info!("now watching {}", p.display());
                                    false
                                }
                                Err(_) => true,
                            }
                        });
                        if pending.len() < before || last_recheck.elapsed() >= RECHECK {
                            last_recheck = std::time::Instant::now();
                            state.spawn_index(false);
                        }
                        continue;
                    }
                    Err(RecvTimeoutError::Disconnected) => return,
                }
                // Let the burst settle.
                let start = std::time::Instant::now();
                while start.elapsed() < MAX_WAIT {
                    match rx.recv_timeout(SETTLE) {
                        Ok(()) => continue,
                        Err(RecvTimeoutError::Timeout) => break,
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                }
                // A run may already be going; wait for it, then run again so
                // this change isn't missed.
                while !state.spawn_index(false) {
                    std::thread::sleep(Duration::from_millis(300));
                }
            }
        })
        .is_ok()
}
