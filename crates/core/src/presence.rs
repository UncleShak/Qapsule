//! When the webview exists. It lives while the capsule is on screen and is
//! destroyed 10 seconds after the capsule tucks away or hides behind a
//! fullscreen app, so a hidden Qapsule is just the Rust process. The native
//! strip at the top edge takes over the job of bringing the capsule back.
//!
//! `Presence` watches the core's `shown` flag and says what the window must do
//! about it. These are window commands, not UI updates: they skip the coalescer
//! so a capsule that should appear isn't held back by the update rate limit.
//! Like everything in core it takes `now`, so tests drive time by hand.

use std::time::{Duration, Instant};

/// How long a hidden capsule keeps its webview, in case it comes straight back.
pub const DESTROY_AFTER: Duration = Duration::from_secs(10);
/// How long after hiding Rust empties the click region itself. The longest CSS
/// transition is 280 ms, so the UI has finished tucking by then; Rust only
/// guarantees "empty when hidden" for a webview that is slow or broken.
pub const CLEAR_AFTER: Duration = Duration::from_secs(1);

/// The least time between two attempts to create the webview, so a window that
/// keeps failing or closing doesn't turn into a busy loop.
pub const CREATE_RETRY: Duration = Duration::from_secs(1);

/// Something the window must do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresenceEffect {
    CreateWebview,
    DestroyWebview,
    /// Show (`true`) or hide the native strip at the top edge.
    EdgeStrip(bool),
    /// Make nothing in the window clickable.
    ClearRegion,
}

#[derive(Debug, Default)]
pub struct Presence {
    /// Whether the capsule was on screen at the last look; `None` before the first.
    shown: Option<bool>,
    /// A create has been asked for and no destroy since.
    webview: bool,
    destroy_at: Option<Instant>,
    clear_at: Option<Instant>,
    /// When the last `CreateWebview` was emitted.
    last_create: Option<Instant>,
    /// A create that was asked for too soon after the last one.
    create_at: Option<Instant>,
}

impl Presence {
    pub fn new() -> Self {
        Self::default()
    }

    /// Looks at whether the capsule is on screen. Does nothing if that hasn't changed.
    pub fn observe(&mut self, shown: bool, now: Instant) -> Vec<PresenceEffect> {
        if self.shown == Some(shown) {
            return Vec::new();
        }
        self.shown = Some(shown);
        if shown {
            self.destroy_at = None;
            self.clear_at = None;
            let mut effects = vec![PresenceEffect::EdgeStrip(false)];
            self.create_at = None;
            if !self.webview {
                self.webview = true;
                self.last_create = Some(now);
                effects.push(PresenceEffect::CreateWebview);
            }
            effects
        } else {
            self.create_at = None;
            if self.webview {
                self.clear_at = Some(now + CLEAR_AFTER);
                self.destroy_at = Some(now + DESTROY_AFTER);
            }
            vec![PresenceEffect::EdgeStrip(true)]
        }
    }

    /// The window reports that the webview is gone (a failed create, Alt+F4, a
    /// crash). If the capsule should be on screen, asks for a new one, but not
    /// more than once per [`CREATE_RETRY`].
    pub fn webview_gone(&mut self, now: Instant) -> Vec<PresenceEffect> {
        self.webview = false;
        self.destroy_at = None;
        self.clear_at = None;
        if self.shown != Some(true) || self.create_at.is_some() {
            return Vec::new();
        }
        match self.last_create {
            Some(last) if now < last + CREATE_RETRY => {
                self.create_at = Some(last + CREATE_RETRY);
                Vec::new()
            }
            _ => {
                self.webview = true;
                self.last_create = Some(now);
                vec![PresenceEffect::CreateWebview]
            }
        }
    }

    /// Effects whose time has come.
    pub fn tick(&mut self, now: Instant) -> Vec<PresenceEffect> {
        let mut effects = Vec::new();
        if self.create_at.is_some_and(|at| at <= now) {
            self.create_at = None;
            if self.shown == Some(true) && !self.webview {
                self.webview = true;
                self.last_create = Some(now);
                effects.push(PresenceEffect::CreateWebview);
            }
        }
        if self.clear_at.is_some_and(|at| at <= now) {
            self.clear_at = None;
            effects.push(PresenceEffect::ClearRegion);
        }
        if self.destroy_at.is_some_and(|at| at <= now) {
            self.destroy_at = None;
            self.webview = false;
            effects.push(PresenceEffect::DestroyWebview);
        }
        effects
    }

    /// The next moment [`Presence::tick`] has work to do.
    pub fn next_deadline(&self) -> Option<Instant> {
        [self.clear_at, self.destroy_at, self.create_at]
            .into_iter()
            .flatten()
            .min()
    }
}

#[cfg(test)]
mod tests {
    use super::PresenceEffect::*;
    use super::*;

    fn secs(start: Instant, s: u64) -> Instant {
        start + Duration::from_secs(s)
    }

    #[test]
    fn first_hidden_shows_the_strip_without_a_webview() {
        let t = Instant::now();
        let mut p = Presence::new();
        assert_eq!(p.observe(false, t), vec![EdgeStrip(true)]);
        assert_eq!(p.next_deadline(), None);
        assert!(p.tick(secs(t, 60)).is_empty());
    }

    #[test]
    fn first_shown_creates_the_webview() {
        let mut p = Presence::new();
        assert_eq!(
            p.observe(true, Instant::now()),
            vec![EdgeStrip(false), CreateWebview]
        );
    }

    #[test]
    fn unchanged_observations_do_nothing() {
        let t = Instant::now();
        let mut p = Presence::new();
        p.observe(true, t);
        assert!(p.observe(true, secs(t, 1)).is_empty());
    }

    #[test]
    fn hide_for_ten_seconds_clears_then_destroys() {
        let t = Instant::now();
        let mut p = Presence::new();
        p.observe(true, t);
        assert_eq!(p.observe(false, t), vec![EdgeStrip(true)]);
        assert!(p.tick(t + Duration::from_millis(999)).is_empty());
        assert_eq!(p.tick(secs(t, 1)), vec![ClearRegion]);
        assert!(p.tick(secs(t, 9)).is_empty());
        assert_eq!(p.tick(secs(t, 10)), vec![DestroyWebview]);
        assert_eq!(p.next_deadline(), None);
    }

    #[test]
    fn a_late_tick_clears_before_destroying() {
        let t = Instant::now();
        let mut p = Presence::new();
        p.observe(true, t);
        p.observe(false, t);
        assert_eq!(p.tick(secs(t, 30)), vec![ClearRegion, DestroyWebview]);
    }

    #[test]
    fn showing_within_ten_seconds_keeps_the_webview() {
        let t = Instant::now();
        let mut p = Presence::new();
        p.observe(true, t);
        p.observe(false, secs(t, 1));
        assert_eq!(p.observe(true, secs(t, 5)), vec![EdgeStrip(false)]);
        assert!(p.tick(secs(t, 60)).is_empty());
        assert_eq!(p.next_deadline(), None);
    }

    #[test]
    fn clearing_is_cancelled_by_showing_within_a_second() {
        let t = Instant::now();
        let mut p = Presence::new();
        p.observe(true, t);
        p.observe(false, t);
        p.observe(true, t + Duration::from_millis(500));
        assert!(p.tick(secs(t, 2)).is_empty());
    }

    #[test]
    fn show_hide_show_before_destroy_creates_once() {
        let t = Instant::now();
        let mut p = Presence::new();
        let mut creates = 0;
        for (i, shown) in [true, false, true, false, true].into_iter().enumerate() {
            let effects = p.observe(shown, secs(t, i as u64));
            creates += effects.iter().filter(|e| **e == CreateWebview).count();
        }
        assert_eq!(creates, 1);
    }

    #[test]
    fn after_a_destroy_the_next_show_creates_again() {
        let t = Instant::now();
        let mut p = Presence::new();
        p.observe(true, t);
        p.observe(false, t);
        p.tick(secs(t, 10));
        assert_eq!(
            p.observe(true, secs(t, 11)),
            vec![EdgeStrip(false), CreateWebview]
        );
    }

    #[test]
    fn next_deadline_follows_the_pending_timers() {
        let t = Instant::now();
        let mut p = Presence::new();
        p.observe(true, t);
        assert_eq!(p.next_deadline(), None);
        p.observe(false, t);
        assert_eq!(p.next_deadline(), Some(secs(t, 1)));
        p.tick(secs(t, 1));
        assert_eq!(p.next_deadline(), Some(secs(t, 10)));
        p.observe(true, secs(t, 2));
        assert_eq!(p.next_deadline(), None);
    }

    #[test]
    fn webview_gone_while_shown_creates_at_once() {
        let t = Instant::now();
        let mut p = Presence::new();
        p.observe(true, t);
        assert_eq!(p.webview_gone(secs(t, 5)), vec![CreateWebview]);
        // A duplicate report while a create is already believed under way does nothing
        // harmful: it is rate limited.
        assert!(p.webview_gone(secs(t, 5)).is_empty());
    }

    #[test]
    fn a_second_gone_within_a_second_waits_for_the_retry() {
        let t = Instant::now();
        let mut p = Presence::new();
        p.observe(true, t);
        assert!(p.webview_gone(t + Duration::from_millis(300)).is_empty());
        assert_eq!(p.next_deadline(), Some(secs(t, 1)));
        assert!(p.tick(t + Duration::from_millis(999)).is_empty());
        assert_eq!(p.tick(secs(t, 1)), vec![CreateWebview]);
        assert_eq!(p.next_deadline(), None);
    }

    #[test]
    fn webview_gone_while_hidden_does_not_create() {
        let t = Instant::now();
        let mut p = Presence::new();
        p.observe(true, t);
        p.observe(false, secs(t, 1));
        assert!(p.webview_gone(secs(t, 2)).is_empty());
        assert_eq!(p.next_deadline(), None);
        assert!(p.tick(secs(t, 60)).is_empty());
    }

    #[test]
    fn hiding_clears_a_pending_create() {
        let t = Instant::now();
        let mut p = Presence::new();
        p.observe(true, t);
        p.webview_gone(t + Duration::from_millis(100));
        p.observe(false, t + Duration::from_millis(200));
        assert_eq!(p.next_deadline(), None);
        assert!(p.tick(secs(t, 5)).is_empty());
    }
}
