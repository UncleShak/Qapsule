//! The activity store: every live activity, its owner, its expiry and its rank.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use qapsule_protocol::limits::{FINISHED_LIFETIME_SECS, MAX_ACTIVITIES_PER_SOURCE};
use qapsule_protocol::{Activity, ErrorCode, ProtocolError, State};

use crate::ConnId;

/// An activity's identity: its source, then its id.
pub type Key = (String, String);

pub fn key_of(activity: &Activity) -> Key {
    (activity.source.clone(), activity.id.clone())
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub activity: Activity,
    /// The connection that last pushed it; callbacks go there.
    pub owner: Option<ConnId>,
    /// Insertion order, used to break ties in favour of the newest.
    pub seq: u64,
    /// When the current run started, for "Working · 6 min".
    pub started: Instant,
    pub expires_at: Instant,
}

#[derive(Debug, Default)]
pub struct Store {
    entries: HashMap<Key, Entry>,
    seq: u64,
}

/// Lower ranks show first: needs input, failed, running with progress, running, done, info.
pub fn rank(activity: &Activity) -> u8 {
    match activity.state {
        State::NeedsInput => 0,
        State::Failed => 1,
        State::Running if activity.progress.is_some() => 2,
        State::Running => 3,
        State::Succeeded => 4,
        State::Info => 5,
    }
}

impl Store {
    /// Adds or replaces an activity. Returns its previous state, if it existed.
    pub fn upsert(
        &mut self,
        activity: Activity,
        owner: Option<ConnId>,
        now: Instant,
    ) -> Result<Option<State>, ProtocolError> {
        let key = key_of(&activity);
        let expires_at = now + Duration::from_secs(activity.lifetime_secs());
        if let Some(entry) = self.entries.get_mut(&key) {
            let previous = entry.activity.state;
            if previous.is_finished() && !activity.state.is_finished() {
                entry.started = now;
            }
            entry.activity = activity;
            entry.owner = owner;
            entry.expires_at = expires_at;
            return Ok(Some(previous));
        }
        let in_source = self.entries.keys().filter(|(s, _)| *s == key.0).count();
        if in_source >= MAX_ACTIVITIES_PER_SOURCE {
            return Err(ProtocolError::new(
                ErrorCode::TooManyActivities,
                format!("a source may have at most {MAX_ACTIVITIES_PER_SOURCE} live activities"),
            ));
        }
        self.seq += 1;
        self.entries.insert(
            key,
            Entry {
                activity,
                owner,
                seq: self.seq,
                started: now,
                expires_at,
            },
        );
        Ok(None)
    }

    /// Marks an activity finished. Returns its previous state.
    pub fn end(
        &mut self,
        key: &Key,
        state: State,
        title: Option<String>,
        subtitle: Option<String>,
        now: Instant,
    ) -> Result<State, ProtocolError> {
        let Some(entry) = self.entries.get_mut(key) else {
            return Err(ProtocolError::new(
                ErrorCode::NotFound,
                format!("no activity `{}` from `{}`", key.1, key.0),
            ));
        };
        let previous = entry.activity.state;
        let activity = &mut entry.activity;
        activity.state = state;
        activity.progress = None;
        activity.expires_in = None;
        if let Some(title) = title {
            activity.title = title;
        }
        if subtitle.is_some() {
            activity.subtitle = subtitle;
        }
        entry.expires_at = now + Duration::from_secs(FINISHED_LIFETIME_SECS);
        Ok(previous)
    }

    /// The key of the oldest activity (lowest `seq`) in a source.
    pub fn oldest_in(&self, source: &str) -> Option<Key> {
        self.entries
            .iter()
            .filter(|((s, _), _)| s == source)
            .min_by_key(|(_, e)| e.seq)
            .map(|(k, _)| k.clone())
    }

    pub fn get(&self, key: &Key) -> Option<&Entry> {
        self.entries.get(key)
    }

    pub fn remove(&mut self, key: &Key) -> Option<Entry> {
        self.entries.remove(key)
    }

    /// Puts back an entry taken by [`Store::remove`], unless the client has replaced it since.
    pub fn restore(&mut self, key: Key, entry: Entry) {
        self.entries.entry(key).or_insert(entry);
    }

    /// Removes one activity, a whole source, or everything. Returns the removed keys.
    pub fn clear(&mut self, source: Option<&str>, id: Option<&str>) -> Vec<Key> {
        let removed: Vec<Key> = self
            .entries
            .keys()
            .filter(|(s, i)| source.is_none_or(|x| x == s) && id.is_none_or(|x| x == i))
            .cloned()
            .collect();
        for key in &removed {
            self.entries.remove(key);
        }
        removed
    }

    /// Removes everything that has expired by `now`. Returns the removed keys.
    pub fn expire(&mut self, now: Instant) -> Vec<Key> {
        let expired: Vec<Key> = self
            .entries
            .iter()
            .filter(|(_, e)| e.expires_at <= now)
            .map(|(k, _)| k.clone())
            .collect();
        for key in &expired {
            self.entries.remove(key);
        }
        expired
    }

    pub fn next_expiry(&self) -> Option<Instant> {
        self.entries.values().map(|e| e.expires_at).min()
    }

    /// Every activity, most important first; ties go to the newest.
    pub fn ranked(&self) -> Vec<(&Key, &Entry)> {
        let mut all: Vec<_> = self.entries.iter().collect();
        all.sort_by_key(|(_, e)| (rank(&e.activity), std::cmp::Reverse(e.seq)));
        all
    }

    /// The live activities, with `expires_in` set to the seconds left.
    pub fn list(&self, source: Option<&str>, now: Instant) -> Vec<Activity> {
        self.ranked()
            .into_iter()
            .filter(|((s, _), _)| source.is_none_or(|x| x == s))
            .map(|(_, e)| {
                let left = e.expires_at.saturating_duration_since(now);
                Activity {
                    expires_in: Some(left.as_secs_f64().ceil().max(1.0) as u64),
                    ..e.activity.clone()
                }
            })
            .collect()
    }

    /// A client disconnected; its activities stay, but nothing can call it back.
    pub fn forget_owner(&mut self, conn: ConnId) {
        for entry in self.entries.values_mut() {
            if entry.owner == Some(conn) {
                entry.owner = None;
            }
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
