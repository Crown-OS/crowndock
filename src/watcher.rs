//! Filesystem watcher feeding [`WatchEvent`]s into the calloop event loop.

use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use calloop::channel as cl_channel;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};

use crate::config;
use crate::error::Result;

#[derive(Debug, Clone, Copy)]
pub enum WatchEvent {
    PinnedChanged,
    ConfigChanged,
}

/// Start watching the user's crowndock directories. Returns a calloop channel
/// to insert into the application's event loop.
pub fn start() -> Result<cl_channel::Channel<WatchEvent>> {
    let (cl_tx, cl_rx) = cl_channel::channel();

    let pinned = config::pinned_dir();
    let cfg_path = config::config_path();
    let cfg_dir = config::config_dir();

    thread::spawn(move || run_watcher(cl_tx, pinned, cfg_path, cfg_dir));

    Ok(cl_rx)
}

fn run_watcher(
    tx: cl_channel::Sender<WatchEvent>,
    pinned: PathBuf,
    cfg_path: PathBuf,
    cfg_dir: PathBuf,
) {
    let (notify_tx, notify_rx) = mpsc::channel::<notify::Result<Event>>();
    let mut watcher = match RecommendedWatcher::new(
        move |res| {
            let _ = notify_tx.send(res);
        },
        notify::Config::default().with_poll_interval(Duration::from_secs(2)),
    ) {
        Ok(w) => w,
        Err(err) => {
            log::warn!("watcher init failed: {err}");
            return;
        }
    };

    if let Err(err) = watcher.watch(&pinned, RecursiveMode::NonRecursive) {
        log::warn!("watching {} failed: {err}", pinned.display());
    }
    if let Err(err) = watcher.watch(&cfg_dir, RecursiveMode::NonRecursive) {
        log::warn!("watching {} failed: {err}", cfg_dir.display());
    }

    while let Ok(res) = notify_rx.recv() {
        let Ok(ev) = res else { continue };
        let mut sent_pinned = false;
        let mut sent_cfg = false;
        for p in ev.paths {
            if p.starts_with(&pinned) && !sent_pinned {
                let _ = tx.send(WatchEvent::PinnedChanged);
                sent_pinned = true;
            }
            if p == cfg_path && !sent_cfg {
                let _ = tx.send(WatchEvent::ConfigChanged);
                sent_cfg = true;
            }
        }
    }
}
