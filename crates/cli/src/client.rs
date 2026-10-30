//! A blocking pipe client: one request, one reply.

use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::thread;
use std::time::Duration;

use qapsule_protocol::{ClientMessage, ServerMessage, decode_server, encode};

const ERROR_FILE_NOT_FOUND: i32 = 2;
const ERROR_PIPE_BUSY: i32 = 231;

pub struct Client {
    reader: BufReader<File>,
    writer: File,
}

/// The pipe to use: `QAPSULE_PIPE` if set, else the real one.
pub fn pipe_name() -> String {
    std::env::var("QAPSULE_PIPE").unwrap_or_else(|_| qapsule_protocol::PIPE_NAME.to_string())
}

impl Client {
    pub fn connect() -> io::Result<Client> {
        let name = pipe_name();
        let mut attempts = 0;
        let file = loop {
            match OpenOptions::new().read(true).write(true).open(&name) {
                Ok(file) => break file,
                // Every instance is momentarily taken; the server makes a new one at once.
                Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY) && attempts < 40 => {
                    attempts += 1;
                    thread::sleep(Duration::from_millis(25));
                }
                Err(e) if e.raw_os_error() == Some(ERROR_FILE_NOT_FOUND) => {
                    return Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        "Qapsule isn't running (no pipe at ".to_string() + &name + ")",
                    ));
                }
                Err(e) => return Err(e),
            }
        };
        Ok(Client {
            reader: BufReader::new(file.try_clone()?),
            writer: file,
        })
    }

    pub fn send(&mut self, message: &ClientMessage) -> io::Result<()> {
        let mut line = encode(message);
        line.push('\n');
        self.writer.write_all(line.as_bytes())
    }

    /// The next message from Qapsule: a reply, or a callback.
    pub fn receive(&mut self) -> io::Result<ServerMessage> {
        let mut line = String::new();
        if self.reader.read_line(&mut line)? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Qapsule closed the pipe",
            ));
        }
        decode_server(line.trim_end()).map_err(io::Error::other)
    }

    pub fn request(&mut self, message: &ClientMessage) -> io::Result<ServerMessage> {
        self.send(message)?;
        self.receive()
    }
}
