//! Watch the agents' log folders and re-index after they change.

use std::sync::Arc;
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::Duration;

use notify::{EventKind, RecursiveMode, Watcher};

use crate::AppState;

/// Wait this long after the last change before re-indexing, so an agent
/// writing many lines in a burst causes one run, not dozens.
const SETTLE: Duration = Duration::from_millis(750);

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
    let mut watching = 0;
    for src in state.sources().iter().filter(|s| s.exists) {
        match watcher.watch(&src.path, RecursiveMode::Recursive) {
            Ok(()) => watching += 1,
            Err(e) => tracing::warn!("can't watch {}: {e}", src.path.display()),
        }
    }
    if watching == 0 {
        return false;
    }

    let handle = tokio::runtime::Handle::current();
    std::thread::Builder::new()
        .name("agentcam-watch".into())
        .spawn(move || {
            let _watcher = watcher; // keep it alive for the thread's life
            let _rt = handle.enter();
            while rx.recv().is_ok() {
                // Let the burst settle.
                loop {
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
