//! Now playing (decision 0008): the music Windows reports, ranked into the peek
//! beside pushed activities, with a media view opened by a click on the cover. It is its own
//! core input and never enters the activity store.

use std::time::{Duration, Instant};

use crate::store::Key;
use crate::view::MusicView;
use crate::{Core, Effect};

/// The fixed key of the music item: the peek swap and the button presses name it.
pub const SOURCE: &str = "media:";
pub const ID: &str = "now-playing";

/// The buttons the music view sends back.
const ACTIONS: [&str; 3] = ["media.play_pause", "media.next", "media.previous"];

/// A moving position differs less than this from what the UI was last told: not worth an update.
const DRIFT: Duration = Duration::from_secs(2);

pub(crate) fn media_key() -> Key {
    (SOURCE.into(), ID.into())
}

pub(crate) fn is_media_key(key: &Key) -> bool {
    key.0 == SOURCE && key.1 == ID
}

/// Where a song is, as read from the player.
#[derive(Debug, Clone, PartialEq)]
pub struct Timeline {
    pub position: Duration,
    pub length: Duration,
    /// When `position` was read; a playing song has moved on since.
    pub read_at: Instant,
    /// Playback speed: 1.0 is normal. Zero or less means the song isn't moving.
    pub rate: f64,
}

/// What the media provider knows about the current session.
#[derive(Debug, Clone, PartialEq)]
pub struct MediaState {
    /// The player's name.
    pub app: String,
    pub title: String,
    pub artist: String,
    /// A URL on the `qapsule-art` scheme.
    pub art: Option<String>,
    pub playing: bool,
    pub timeline: Option<Timeline>,
}

/// How fast the position moves: the rate when it is a sane positive number, else not at all.
fn speed(rate: f64) -> f64 {
    if rate.is_finite() && rate > 0.0 {
        rate.min(8.0)
    } else {
        0.0
    }
}

/// The position at `now`: the one read, plus the time since at the playback rate while
/// playing, within the length.
fn position_at(timeline: &Timeline, playing: bool, now: Instant) -> Duration {
    let rate = if playing { speed(timeline.rate) } else { 0.0 };
    let moved = now
        .saturating_duration_since(timeline.read_at)
        .min(Duration::from_secs(7 * 24 * 3600))
        .mul_f64(rate);
    (timeline.position + moved).min(timeline.length)
}

/// The music as the peek shows it. With details off the song is hidden: the title
/// is generic, the artist line names the player, and the cover (which gives the
/// song away) is left out.
pub(crate) fn music_view(
    media: &MediaState,
    controls: bool,
    details: bool,
    now: Instant,
) -> MusicView {
    // A zero length is a live stream: no line.
    let timeline = media.timeline.as_ref().filter(|t| !t.length.is_zero());
    MusicView {
        source: SOURCE.into(),
        id: ID.into(),
        app: media.app.clone(),
        title: if details {
            media.title.clone()
        } else {
            "Now playing".into()
        },
        artist: if details {
            media.artist.clone()
        } else {
            media.app.clone()
        },
        art: media.art.clone().filter(|_| details),
        playing: media.playing,
        position_ms: timeline.map(|t| position_at(t, media.playing, now).as_millis() as u64),
        length_ms: timeline.map(|t| t.length.as_millis() as u64),
        rate: timeline
            .filter(|_| media.playing)
            .map_or(0.0, |t| speed(t.rate)),
        controls,
    }
}

/// Whether `new` shows the same as `old` right now: the same text and cover, the same
/// speed, and a position within [`DRIFT`] of where `old` has it by now.
fn same_view(old: &MediaState, new: &MediaState, now: Instant) -> bool {
    old.app == new.app
        && old.title == new.title
        && old.artist == new.artist
        && old.art == new.art
        && old.playing == new.playing
        && match (&old.timeline, &new.timeline) {
            (None, None) => true,
            (Some(a), Some(b)) => {
                a.length == b.length
                    && speed(a.rate) == speed(b.rate)
                    && position_at(a, old.playing, now).abs_diff(position_at(b, new.playing, now))
                        < DRIFT
            }
            _ => false,
        }
}

impl Core {
    /// The media provider has a new state, or nothing is playing (`None`).
    pub fn set_media(&mut self, media: Option<MediaState>, now: Instant) {
        if self.media == media {
            return;
        }
        let Some(new) = media else {
            self.media = None;
            self.media_shown = None;
            self.media_open = false;
            self.media_close_at = None;
            self.coalescer.mark(now);
            return;
        };

        // Compared with what the UI was last told, so slow drift can't pile up unseen.
        let quiet = self
            .media_shown
            .as_ref()
            .is_some_and(|shown| same_view(shown, &new, now));
        if !quiet {
            self.media_shown = Some(new.clone());
            self.coalescer.mark(now);
        }
        self.media = Some(new);
    }

    /// The cover was clicked: opens the media view, or closes it. It opens only with music
    /// playing, the expanded view closed and no pop or undo bar on screen.
    pub fn media_click(&mut self, now: Instant) {
        if self.media_open {
            self.close_media_view(now);
        } else if self.media.is_some() && !self.expanded.is_open() && !self.pop_or_undo_showing() {
            self.media_open = true;
            self.media_close_at = None;
            self.coalescer.mark(now);
        }
    }

    /// Closes the media view, if open.
    pub(crate) fn close_media_view(&mut self, now: Instant) {
        self.media_close_at = None;
        if self.media_open {
            self.media_open = false;
            self.coalescer.mark(now);
        }
    }

    /// A music button was pressed: the media provider carries it out.
    pub(crate) fn press_media(&self, action_id: &str) -> Option<Effect> {
        self.media.as_ref()?;
        ACTIONS.contains(&action_id).then(|| Effect::Provider {
            source: SOURCE.into(),
            id: ID.into(),
            action: action_id.into(),
        })
    }
}
