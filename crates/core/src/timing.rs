//! Small time-driven helpers: the UI update coalescer and the per-connection rate limiter.

use std::time::{Duration, Instant};

/// Lets at most one UI update through per interval, and never drops the last one.
#[derive(Debug)]
pub struct Coalescer {
    interval: Duration,
    last: Option<Instant>,
    dirty_since: Option<Instant>,
}

impl Coalescer {
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            last: None,
            dirty_since: None,
        }
    }

    /// Something changed; an update is owed.
    pub fn mark(&mut self, now: Instant) {
        self.dirty_since.get_or_insert(now);
    }

    /// When the owed update may be sent, if one is owed.
    pub fn due(&self) -> Option<Instant> {
        let dirty = self.dirty_since?;
        Some(match self.last {
            Some(last) => dirty.max(last + self.interval),
            None => dirty,
        })
    }

    /// True if an update should be sent now. Resets the owed update.
    pub fn take(&mut self, now: Instant) -> bool {
        match self.due() {
            Some(due) if now >= due => {
                self.last = Some(now);
                self.dirty_since = None;
                true
            }
            _ => false,
        }
    }
}

/// A fixed one-second window allowing `limit` messages.
#[derive(Debug)]
pub struct RateLimiter {
    limit: u32,
    window_start: Option<Instant>,
    count: u32,
}

impl RateLimiter {
    pub fn new(limit: u32) -> Self {
        Self {
            limit,
            window_start: None,
            count: 0,
        }
    }

    /// Counts a message; false if it is over the limit.
    pub fn allow(&mut self, now: Instant) -> bool {
        match self.window_start {
            Some(start) if now.duration_since(start) < Duration::from_secs(1) => {}
            _ => {
                self.window_start = Some(now);
                self.count = 0;
            }
        }
        self.count += 1;
        self.count <= self.limit
    }
}
