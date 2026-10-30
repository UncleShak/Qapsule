//! The engine: one task that owns the [`Core`] and feeds it real time. It
//! sleeps until the next thing is due (an expiry, a pop ending, the next
//! minute or an owed UI update), so it costs nothing while idle.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use qapsule_core::{ConnId, Core, Effect, PresenceEffect, Settings, UiState};
use qapsule_protocol::{ClientMessage, ServerMessage};
use tokio::sync::{mpsc, oneshot};

use crate::system;

/// Where the engine's output goes: the window, and the action router.
pub trait Shell: Send + 'static {
    /// The UI should show this. Called at most 4 times a second.
    fn update(&mut self, state: UiState);
    /// Open a link or bring a window forward.
    fn run(&mut self, effect: Effect);
    /// The window must create or destroy the webview, switch the edge strip or empty
    /// its click region. Called as soon as the core's `shown` changes or a timer is
    /// due, not rate limited.
    fn presence(&mut self, effect: PresenceEffect);
}

/// The real action router.
pub fn run_effect(effect: Effect) {
    match effect {
        Effect::OpenUrl(url) => {
            system::open_url(&url);
        }
        Effect::FocusWindow(pid) => {
            system::focus_process_window(pid);
        }
        // Callbacks never reach the shell; the engine sends them down the pipe.
        Effect::Callback { .. } => {}
    }
}

enum Command {
    Connected(ConnId, mpsc::UnboundedSender<ServerMessage>),
    Message(ConnId, ClientMessage),
    Disconnected(ConnId),
    Press {
        source: String,
        id: String,
        action: String,
    },
    DismissPop,
    Undo,
    SwapPeek,
    Pointer(bool),
    TogglePinned,
    EdgeHover,
    Fullscreen(bool),
    Settings(Settings),
    WebviewGone,
    Snapshot(oneshot::Sender<UiState>),
}

/// A cheap, cloneable handle to the engine task.
#[derive(Clone)]
pub struct Engine {
    tx: mpsc::UnboundedSender<Command>,
    next_conn: Arc<AtomicU64>,
}

impl Engine {
    /// Starts the engine on the current Tokio runtime.
    pub fn spawn(settings: Settings, shell: impl Shell) -> Engine {
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(run(Core::new(settings, Instant::now()), rx, shell));
        Engine {
            tx,
            next_conn: Arc::new(AtomicU64::new(1)),
        }
    }

    fn send(&self, command: Command) {
        // Fails only after shutdown, when nothing is listening anyway.
        let _ = self.tx.send(command);
    }

    /// Registers a pipe connection; replies and callbacks go to `out`.
    pub fn connect(&self, out: mpsc::UnboundedSender<ServerMessage>) -> ConnId {
        let conn = self.next_conn.fetch_add(1, Ordering::Relaxed);
        self.send(Command::Connected(conn, out));
        conn
    }

    pub fn message(&self, conn: ConnId, message: ClientMessage) {
        self.send(Command::Message(conn, message));
    }

    pub fn disconnect(&self, conn: ConnId) {
        self.send(Command::Disconnected(conn));
    }

    pub fn press(&self, source: String, id: String, action: String) {
        self.send(Command::Press { source, id, action });
    }

    pub fn dismiss_pop(&self) {
        self.send(Command::DismissPop);
    }

    pub fn undo(&self) {
        self.send(Command::Undo);
    }

    pub fn swap_peek(&self) {
        self.send(Command::SwapPeek);
    }

    pub fn pointer(&self, over: bool) {
        self.send(Command::Pointer(over));
    }

    pub fn toggle_pinned(&self) {
        self.send(Command::TogglePinned);
    }

    /// The pointer touched the strip at the top edge where a tucked capsule hides.
    pub fn edge_hover(&self) {
        self.send(Command::EdgeHover);
    }

    pub fn fullscreen(&self, fullscreen: bool) {
        self.send(Command::Fullscreen(fullscreen));
    }

    pub fn settings(&self, settings: Settings) {
        self.send(Command::Settings(settings));
    }

    /// The webview no longer exists (it closed or failed to start).
    pub fn webview_gone(&self) {
        self.send(Command::WebviewGone);
    }

    /// The full UI state, for a UI that has just started.
    pub async fn snapshot(&self) -> Option<UiState> {
        let (tx, rx) = oneshot::channel();
        self.send(Command::Snapshot(tx));
        rx.await.ok()
    }
}

async fn run(mut core: Core, mut rx: mpsc::UnboundedReceiver<Command>, mut shell: impl Shell) {
    let mut conns: HashMap<ConnId, mpsc::UnboundedSender<ServerMessage>> = HashMap::new();
    let mut next_minute = Instant::now();
    loop {
        let now = Instant::now();
        if now >= next_minute {
            let (clock, until) = system::local_clock();
            core.set_clock(clock, now);
            next_minute = now + until;
        }
        core.tick(now);
        // Presence effects are window commands, not webview updates, so AGENTS.md rule 3
        // (the coalescer) doesn't apply: never route them through it. Every command
        // `continue`s or falls through to the top of this loop, so a change to `shown`
        // is acted on at once, not at the coalescer's next deadline.
        for effect in core.take_presence(now) {
            shell.presence(effect);
        }
        if let Some(state) = core.take_update(now) {
            shell.update(state);
        }
        let deadline = core
            .next_deadline()
            .map_or(next_minute, |d| d.min(next_minute));

        let command = tokio::select! {
            command = rx.recv() => match command {
                Some(command) => command,
                None => return,
            },
            _ = tokio::time::sleep_until(deadline.into()) => continue,
        };
        let now = Instant::now();
        match command {
            Command::Connected(conn, out) => {
                conns.insert(conn, out);
            }
            Command::Message(conn, message) => {
                let reply = core.handle(conn, message, now);
                if let Some(out) = conns.get(&conn) {
                    let _ = out.send(reply);
                }
            }
            Command::Disconnected(conn) => {
                conns.remove(&conn);
                core.connection_closed(conn);
            }
            Command::Press { source, id, action } => match core.press(&source, &id, &action, now) {
                Some(Effect::Callback { conn, message }) => {
                    if let Some(out) = conns.get(&conn) {
                        let _ = out.send(message);
                    }
                }
                Some(effect) => shell.run(effect),
                None => {}
            },
            Command::DismissPop => core.dismiss_pop(now),
            Command::Undo => core.undo(now),
            Command::SwapPeek => core.swap_peek(now),
            Command::Pointer(over) => core.set_pointer(over, now),
            Command::TogglePinned => core.toggle_pinned(now),
            Command::EdgeHover => core.reveal_from_edge(now),
            Command::Fullscreen(fullscreen) => core.set_fullscreen(fullscreen, now),
            Command::Settings(settings) => core.set_settings(settings, now),
            Command::WebviewGone => {
                for effect in core.webview_gone(now) {
                    shell.presence(effect);
                }
            }
            Command::Snapshot(reply) => {
                let _ = reply.send(core.snapshot(now));
            }
        }
    }
}
