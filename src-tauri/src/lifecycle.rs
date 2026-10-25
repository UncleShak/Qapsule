//! Applies the core's presence effects to the real window: the webview, the
//! edge strip and the click region. The decisions (and the timers) live in
//! `qapsule_core::presence`; this is only the Win32 and Tauri side.
//!
//! Effects go into a channel from the first moment and one task runs them in
//! order, each to completion, so a create and a destroy can never overlap. The
//! task starts once the app is ready and commands can be answered.

use std::sync::{Arc, Mutex};

use qapsule_core::PresenceEffect;
use tauri::{AppHandle, Manager};
use tokio::sync::mpsc;

use crate::{watcher, window};

#[derive(Clone)]
pub struct Lifecycle {
    app: AppHandle,
    tx: mpsc::UnboundedSender<PresenceEffect>,
    /// Taken by `start`.
    rx: Arc<Mutex<Option<mpsc::UnboundedReceiver<PresenceEffect>>>>,
}

impl Lifecycle {
    pub fn new(app: AppHandle) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            app,
            tx,
            rx: Arc::new(Mutex::new(Some(rx))),
        }
    }

    /// Called once the app is ready: starts acting on whatever the core has said so far.
    pub fn start(&self) {
        let Some(mut rx) = self.rx.lock().unwrap().take() else {
            return;
        };
        let app = self.app.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(effect) = rx.recv().await {
                run(&app, effect);
            }
        });
    }

    /// Queues `effect`; effects run strictly in the order they arrive.
    pub fn apply(&self, effect: PresenceEffect) {
        let _ = self.tx.send(effect);
    }
}

fn run(app: &AppHandle, effect: PresenceEffect) {
    match effect {
        PresenceEffect::CreateWebview => {
            // A window that is still closing makes this skip; its Destroyed event
            // tells the core, which asks again.
            if app.get_webview_window("main").is_some() {
                return;
            }
            if let Err(e) = window::create(app) {
                eprintln!("qapsule: couldn't create the window: {e}");
                crate::webview_gone(app);
            }
        }
        PresenceEffect::DestroyWebview => {
            if let Some(main) = app.get_webview_window("main") {
                let _ = main.destroy();
            }
        }
        PresenceEffect::EdgeStrip(visible) => {
            if !visible {
                window::set_cleared(false);
            }
            watcher::set_edge_strip(visible);
        }
        PresenceEffect::ClearRegion => {
            window::set_cleared(true);
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || {
                if let Some(main) = handle.get_webview_window("main") {
                    window::set_region(&main, &[]);
                }
            });
        }
    }
}
