//! The named-pipe server: accepts clients, enforces the size and rate limits,
//! decodes messages and hands them to the engine.

use std::io;
use std::time::Instant;

use qapsule_core::RateLimiter;
use qapsule_protocol::limits::{MAX_MESSAGE_BYTES, MAX_MESSAGES_PER_SECOND};
use qapsule_protocol::{
    ActionKind, ClientMessage, ErrorCode, ProtocolError, ServerMessage, decode_client, encode,
};
use tokio::io::AsyncWriteExt;
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use tokio::sync::mpsc;

use crate::engine::Engine;
use crate::framing::{Frame, LineReader};
use crate::security::PipeSecurity;
use crate::system;

pub struct Listener {
    name: String,
    security: PipeSecurity,
    next: NamedPipeServer,
}

impl Listener {
    /// Creates the pipe. Fails if another Qapsule already owns the name.
    /// Must be called inside a Tokio runtime.
    pub fn bind(name: &str) -> io::Result<Listener> {
        let security = PipeSecurity::current_user_only()?;
        let next = create(name, &security, true)?;
        Ok(Listener {
            name: name.to_string(),
            security,
            next,
        })
    }

    /// Serves clients until the runtime shuts down.
    pub async fn run(mut self, engine: Engine) -> io::Result<()> {
        loop {
            self.next.connect().await?;
            let client =
                std::mem::replace(&mut self.next, create(&self.name, &self.security, false)?);
            tokio::spawn(serve(client, engine.clone()));
        }
    }
}

fn create(name: &str, security: &PipeSecurity, first: bool) -> io::Result<NamedPipeServer> {
    let mut attributes = security.attributes();
    let mut options = ServerOptions::new();
    options
        .first_pipe_instance(first)
        .reject_remote_clients(true);
    // SAFETY: `attributes` points at a descriptor that `security` keeps alive.
    unsafe { options.create_with_security_attributes_raw(name, (&raw mut attributes).cast()) }
}

/// Points `focus_window` buttons at the process that owns the window, while the
/// pushing process and its parents are still alive to be walked.
fn resolve_focus_pids(message: &mut ClientMessage) {
    let ClientMessage::Upsert { activity } = message else {
        return;
    };
    for action in &mut activity.actions {
        if let ActionKind::FocusWindow { pid } = &mut action.kind
            && let Some(owner) = system::window_owner(*pid)
        {
            *pid = owner;
        }
    }
}

async fn serve(pipe: NamedPipeServer, engine: Engine) {
    let (read, mut write) = tokio::io::split(pipe);
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<ServerMessage>();
    let conn = engine.connect(out_tx.clone());

    let writer = tokio::spawn(async move {
        while let Some(message) = out_rx.recv().await {
            let mut line = encode(&message);
            line.push('\n');
            if write.write_all(line.as_bytes()).await.is_err() {
                break;
            }
        }
    });

    let mut reader = LineReader::new(read, MAX_MESSAGE_BYTES);
    let mut limiter = RateLimiter::new(MAX_MESSAGES_PER_SECOND);
    loop {
        let frame = match reader.next().await {
            Ok(Frame::Eof) | Err(_) => break,
            Ok(frame) => frame,
        };
        let error = if !limiter.allow(Instant::now()) {
            ProtocolError::new(
                ErrorCode::RateLimited,
                format!(
                    "at most {MAX_MESSAGES_PER_SECOND} messages a second; this one was dropped"
                ),
            )
        } else {
            match frame {
                Frame::Line(line) => match decode_client(&line) {
                    Ok(mut message) => {
                        resolve_focus_pids(&mut message);
                        engine.message(conn, message);
                        continue;
                    }
                    Err(e) => e,
                },
                _ => ProtocolError::new(
                    ErrorCode::TooLarge,
                    format!("messages are limited to {MAX_MESSAGE_BYTES} bytes of UTF-8"),
                ),
            }
        };
        let _ = out_tx.send(error.into());
    }

    engine.disconnect(conn);
    drop(out_tx);
    let _ = writer.await;
}
