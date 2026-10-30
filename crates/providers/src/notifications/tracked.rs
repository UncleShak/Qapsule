//! What the provider remembers about live toasts: id to (source, AUMID).
//! Holds no notification text. Pure, so it is tested without WinRT.

use std::collections::{HashMap, VecDeque};

use super::toast::valid_aumid;

pub(crate) const MAX_TRACKED: usize = 500;

#[derive(Default)]
pub(crate) struct Tracked {
    map: HashMap<String, (String, Option<String>)>,
    order: VecDeque<String>,
}

impl Tracked {
    pub(crate) fn insert(&mut self, id: String, value: (String, Option<String>)) {
        if self.map.insert(id.clone(), value).is_none() {
            self.order.push_back(id);
        }
        while self.map.len() > MAX_TRACKED {
            match self.order.pop_front() {
                Some(old) => {
                    self.map.remove(&old);
                }
                None => break,
            }
        }
    }

    pub(crate) fn remove(&mut self, id: &str) -> Option<(String, Option<String>)> {
        let value = self.map.remove(id)?;
        self.order.retain(|o| o != id);
        Some(value)
    }
}

/// The AUMID to open for a button press, or `None` if the press must be ignored.
pub(crate) fn aumid_to_open(
    tracked: &Tracked,
    source: &str,
    id: &str,
    action: &str,
) -> Option<String> {
    if action != "open" {
        return None;
    }
    let (tracked_source, aumid) = tracked.map.get(id)?;
    if tracked_source != source {
        return None;
    }
    aumid.clone().filter(|a| valid_aumid(a))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(source: &str, aumid: Option<&str>) -> (String, Option<String>) {
        (source.into(), aumid.map(String::from))
    }

    #[test]
    fn eviction_drops_the_oldest() {
        let mut t = Tracked::default();
        for i in 0..MAX_TRACKED + 3 {
            t.insert(i.to_string(), entry("windows:A", None));
        }
        assert_eq!(t.map.len(), MAX_TRACKED);
        assert_eq!(t.order.len(), MAX_TRACKED);
        for i in 0..3 {
            assert!(!t.map.contains_key(&i.to_string()));
        }
        assert!(t.map.contains_key("3"));
    }

    #[test]
    fn reinserting_does_not_duplicate_order() {
        let mut t = Tracked::default();
        t.insert("1".into(), entry("windows:A", None));
        t.insert("1".into(), entry("windows:B", Some("x")));
        assert_eq!(t.order.len(), 1);
        assert_eq!(t.map["1"].0, "windows:B");
    }

    #[test]
    fn remove_keeps_order_in_sync() {
        let mut t = Tracked::default();
        t.insert("1".into(), entry("windows:A", None));
        t.insert("2".into(), entry("windows:A", None));
        assert!(t.remove("1").is_some());
        assert!(t.remove("1").is_none());
        assert_eq!(t.order, ["2"]);
        assert_eq!(t.map.len(), 1);
    }

    #[test]
    fn aumid_to_open_rules() {
        let mut t = Tracked::default();
        t.insert("1".into(), entry("windows:A", Some("Pkg!App")));
        t.insert("2".into(), entry("windows:A", None));
        t.insert("3".into(), entry("windows:A", Some("bad|id")));
        let open = |s, i, a| aumid_to_open(&t, s, i, a);
        assert_eq!(open("windows:A", "1", "open").as_deref(), Some("Pkg!App"));
        assert_eq!(open("windows:A", "1", "close"), None);
        assert_eq!(open("windows:B", "1", "open"), None);
        assert_eq!(open("windows:A", "9", "open"), None);
        assert_eq!(open("windows:A", "2", "open"), None);
        assert_eq!(open("windows:A", "3", "open"), None);
    }
}
