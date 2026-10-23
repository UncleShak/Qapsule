//! When the webview exists. It lives while the capsule is on screen and is
//! destroyed 10 seconds after the capsule tucks away or hides behind a
//! fullscreen app, so a hidden Qapsule is just the Rust process. The native
//! strip at the top edge takes over the job of bringing the capsule back.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::{watcher, window};

/// How long a hidden capsule keeps its webview, in case it comes straight back.
pub const DESTROY_AFTER: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub struct Lifecycle {
    app: AppHandle,
    inner: Arc<Mutex<Inner>>,
    /// Bumped on every change; a pending destroy only runs if nothing changed since.
    epoch: Arc<AtomicU64>,
}

struct Inner {
    /// Whether the core says the capsule is on screen; `None` before the first update.
    shown: Option<bool>,
    /// False until the app has finished starting and commands can be answered.
    started: bool,
}

impl Lifecycle {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            inner: Arc::new(Mutex::new(Inner {
                shown: None,
                started: false,
            })),
            epoch: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Called once the app is ready: acts on whatever the core has said so far.
    pub fn start(&self) {
        let shown = {
            let mut inner = self.inner.lock().unwrap();
            inner.started = true;
            inner.shown
        };
        if let Some(shown) = shown {
            self.apply(shown);
        }
    }

    /// Called with every UI update from the core.
    pub fn observe(&self, shown: bool) {
        let started = {
            let mut inner = self.inner.lock().unwrap();
            if inner.shown == Some(shown) {
                return;
            }
            inner.shown = Some(shown);
            inner.started
        };
        if started {
            self.apply(shown);
        }
    }

    fn apply(&self, shown: bool) {
        let epoch = self.epoch.fetch_add(1, Ordering::SeqCst) + 1;
        watcher::set_edge_strip(!shown);
        let app = self.app.clone();
        if shown {
            if app.get_webview_window("main").is_none() {
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = window::create(&app) {
                        eprintln!("qapsule: couldn't create the window: {e}");
                    }
                });
            }
        } else {
            let current = self.epoch.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(DESTROY_AFTER).await;
                if current.load(Ordering::SeqCst) == epoch
                    && let Some(main) = app.get_webview_window("main")
                {
                    let _ = main.destroy();
                }
            });
        }
    }
}
