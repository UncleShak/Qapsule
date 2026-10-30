//! Splits a byte stream into lines, refusing any line over the size limit
//! without ever holding more than the limit in memory.

use std::io;

use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};

#[derive(Debug, PartialEq, Eq)]
pub enum Frame {
    Line(String),
    /// A line over the limit (or not UTF-8); it was skipped.
    Rejected,
    Eof,
}

pub struct LineReader<R> {
    inner: BufReader<R>,
    buf: Vec<u8>,
    max: usize,
}

impl<R: AsyncRead + Unpin> LineReader<R> {
    pub fn new(inner: R, max: usize) -> Self {
        Self {
            inner: BufReader::new(inner),
            buf: Vec::new(),
            max,
        }
    }

    /// The next non-empty line, without its line ending.
    pub async fn next(&mut self) -> io::Result<Frame> {
        loop {
            match self.next_raw().await? {
                Frame::Line(line) if line.trim().is_empty() => continue,
                frame => return Ok(frame),
            }
        }
    }

    async fn next_raw(&mut self) -> io::Result<Frame> {
        self.buf.clear();
        let mut over = false;
        loop {
            let available = self.inner.fill_buf().await?;
            if available.is_empty() {
                if over {
                    return Ok(Frame::Rejected);
                }
                if self.buf.is_empty() {
                    return Ok(Frame::Eof);
                }
                break;
            }
            let (chunk, used, done) = match available.iter().position(|&b| b == b'\n') {
                Some(i) => (&available[..i], i + 1, true),
                None => (available, available.len(), false),
            };
            if !over {
                if self.buf.len() + chunk.len() > self.max + 1 {
                    over = true;
                    self.buf.clear();
                } else {
                    self.buf.extend_from_slice(chunk);
                }
            }
            self.inner.consume(used);
            if done {
                break;
            }
        }
        if self.buf.last() == Some(&b'\r') {
            self.buf.pop();
        }
        if over || self.buf.len() > self.max {
            return Ok(Frame::Rejected);
        }
        Ok(match String::from_utf8(std::mem::take(&mut self.buf)) {
            Ok(line) => Frame::Line(line),
            Err(_) => Frame::Rejected,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn frames(input: &[u8], max: usize) -> Vec<Frame> {
        let mut reader = LineReader::new(input, max);
        let mut out = Vec::new();
        loop {
            let frame = reader.next().await.unwrap();
            let eof = frame == Frame::Eof;
            out.push(frame);
            if eof {
                return out;
            }
        }
    }

    fn line(s: &str) -> Frame {
        Frame::Line(s.into())
    }

    #[tokio::test]
    async fn splits_lines_and_skips_blank_ones() {
        let out = frames(b"one\r\n\ntwo\nthree", 10).await;
        assert_eq!(out, [line("one"), line("two"), line("three"), Frame::Eof]);
    }

    #[tokio::test]
    async fn rejects_long_lines_and_carries_on() {
        let out = frames(b"12345\n123456\nok\n", 5).await;
        assert_eq!(
            out,
            [line("12345"), Frame::Rejected, line("ok"), Frame::Eof]
        );
    }

    #[tokio::test]
    async fn rejects_long_lines_bigger_than_the_buffer() {
        let mut input = vec![b'x'; 100_000];
        input.extend_from_slice(b"\nok\n");
        let out = frames(&input, 64 * 1024).await;
        assert_eq!(out, [Frame::Rejected, line("ok"), Frame::Eof]);
    }

    #[tokio::test]
    async fn rejects_invalid_utf8() {
        let out = frames(b"\xff\xfe\nok\n", 10).await;
        assert_eq!(out, [Frame::Rejected, line("ok"), Frame::Eof]);
    }
}
