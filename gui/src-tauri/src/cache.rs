//! A one-entry cache keyed by what a view depends on (`contract::view_key`:
//! the snapshot and the preferences). On a large home the cleanup view takes
//! seconds to build and the Duplicates result tens of seconds (it hashes
//! every candidate's content), so screens reuse the last one instead of
//! rebuilding it on every visit. Anything that changes files (clean, trash,
//! restore) clears it; a new scan or a prefs change is a new key; and an
//! entry older than its time-to-live is rebuilt, which catches changes made
//! outside the app.

use std::time::{Duration, Instant};

pub(crate) struct Keyed<T> {
    entry: Option<(String, Instant, T)>,
}

impl<T> Default for Keyed<T> {
    fn default() -> Self {
        Self { entry: None }
    }
}

impl<T: Clone> Keyed<T> {
    /// The cached value when it was stored under `key` less than `ttl`
    /// before `now`.
    pub(crate) fn get(&self, key: &str, ttl: Duration, now: Instant) -> Option<T> {
        match &self.entry {
            Some((k, at, v)) if k == key && now.saturating_duration_since(*at) < ttl => {
                Some(v.clone())
            }
            _ => None,
        }
    }

    pub(crate) fn put(&mut self, key: String, at: Instant, value: T) {
        self.entry = Some((key, at, value));
    }

    pub(crate) fn clear(&mut self) {
        self.entry = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_key_within_ttl_is_reused() {
        let t0 = Instant::now();
        let mut c = Keyed::default();
        c.put("snap|1|prefs".into(), t0, 7);
        let ttl = Duration::from_secs(60);
        assert_eq!(
            c.get("snap|1|prefs", ttl, t0 + Duration::from_secs(59)),
            Some(7)
        );
    }

    #[test]
    fn new_key_expiry_or_clear_rebuilds() {
        let t0 = Instant::now();
        let ttl = Duration::from_secs(60);
        let mut c = Keyed::default();
        c.put("snap|1|prefs".into(), t0, 7);
        // A new scan (mtime) or a prefs change is a different key.
        assert_eq!(c.get("snap|2|prefs", ttl, t0), None);
        // Too old: changes made outside the app are picked up.
        assert_eq!(c.get("snap|1|prefs", ttl, t0 + ttl), None);
        // A clean, trash or restore clears it.
        c.clear();
        assert_eq!(c.get("snap|1|prefs", ttl, t0), None);
    }
}
