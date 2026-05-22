//! restic-style snapshot retention policy.
//!
//! Given a list of [`SnapshotMeta`] and a [`Policy`], compute which snapshots
//! survive and which are pruned. Pure function — caller decides dry-run vs
//! apply.

use chrono::{DateTime, Datelike, Local, NaiveDate};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Default)]
pub struct Policy {
    pub keep_last: Option<usize>,
    pub keep_daily: Option<usize>,
    pub keep_weekly: Option<usize>,
    pub keep_monthly: Option<usize>,
    pub keep_yearly: Option<usize>,
}

impl Policy {
    pub fn is_empty(&self) -> bool {
        self.keep_last.is_none()
            && self.keep_daily.is_none()
            && self.keep_weekly.is_none()
            && self.keep_monthly.is_none()
            && self.keep_yearly.is_none()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SnapshotMeta {
    pub id: String,
    pub path: String,
    pub bytes: u64,
    /// RFC 3339 string for envelope output. Resolved from `id` via
    /// `snapshots::parse_id`; absent for user-renamed files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct KeptSnapshot {
    pub id: String,
    pub path: String,
    pub bytes: u64,
    /// Sorted, deduplicated list of reasons the snapshot was kept (e.g.
    /// `["last", "daily"]`). Useful UX: "kept because it's both the newest
    /// and the only Monday this month".
    pub reasons: Vec<&'static str>,
}

#[derive(Debug, Default, Serialize)]
pub struct Plan {
    pub kept: Vec<KeptSnapshot>,
    pub removed: Vec<SnapshotMeta>,
    pub skipped_unparseable: Vec<String>,
    pub total_removed_bytes: u64,
}

/// Compute the retention plan. Snapshots whose ID doesn't parse to a
/// datetime end up in `skipped_unparseable` — they are NEVER removed.
pub fn apply_policy(snaps: &[SnapshotMeta], policy: &Policy) -> Plan {
    // Resolve parseable snapshots with their datetime.
    let mut parsed: Vec<(DateTime<Local>, &SnapshotMeta)> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    for s in snaps {
        match crate::snapshots::parse_id(&s.id) {
            Some(dt) => parsed.push((dt, s)),
            None => skipped.push(s.id.clone()),
        }
    }
    // Newest first.
    parsed.sort_by_key(|(dt, _)| std::cmp::Reverse(*dt));

    let mut reasons: HashMap<String, Vec<&'static str>> = HashMap::new();
    let mark = |reasons: &mut HashMap<String, Vec<&'static str>>, id: &str, why: &'static str| {
        let entry = reasons.entry(id.to_string()).or_default();
        if !entry.contains(&why) {
            entry.push(why);
        }
    };

    if let Some(n) = policy.keep_last {
        for (_, s) in parsed.iter().take(n) {
            mark(&mut reasons, &s.id, "last");
        }
    }
    if let Some(n) = policy.keep_daily {
        bucket_keep(&parsed, n, &mut reasons, "daily", |dt| dt.date_naive());
    }
    if let Some(n) = policy.keep_weekly {
        bucket_keep(&parsed, n, &mut reasons, "weekly", |dt| iso_week_key(&dt));
    }
    if let Some(n) = policy.keep_monthly {
        bucket_keep(&parsed, n, &mut reasons, "monthly", |dt| {
            NaiveDate::from_ymd_opt(dt.year(), dt.month(), 1).unwrap()
        });
    }
    if let Some(n) = policy.keep_yearly {
        bucket_keep(&parsed, n, &mut reasons, "yearly", |dt| {
            NaiveDate::from_ymd_opt(dt.year(), 1, 1).unwrap()
        });
    }

    let mut kept: Vec<KeptSnapshot> = Vec::new();
    let mut removed: Vec<SnapshotMeta> = Vec::new();
    let mut total_removed_bytes: u64 = 0;
    for (_, s) in &parsed {
        if let Some(why) = reasons.get(&s.id) {
            let mut r = why.clone();
            r.sort();
            r.dedup();
            kept.push(KeptSnapshot {
                id: s.id.clone(),
                path: s.path.clone(),
                bytes: s.bytes,
                reasons: r,
            });
        } else {
            removed.push((*s).clone());
            total_removed_bytes += s.bytes;
        }
    }

    Plan {
        kept,
        removed,
        skipped_unparseable: skipped,
        total_removed_bytes,
    }
}

fn iso_week_key(dt: &DateTime<Local>) -> NaiveDate {
    let iso = dt.iso_week();
    NaiveDate::from_isoywd_opt(iso.year(), iso.week(), chrono::Weekday::Mon).unwrap_or_else(|| {
        // Fallback impossible in practice for valid dates.
        NaiveDate::from_ymd_opt(dt.year(), 1, 1).unwrap()
    })
}

fn bucket_keep<F, K>(
    parsed: &[(DateTime<Local>, &SnapshotMeta)],
    n: usize,
    reasons: &mut HashMap<String, Vec<&'static str>>,
    label: &'static str,
    key_for: F,
) where
    F: Fn(DateTime<Local>) -> K,
    K: Ord,
{
    // For each bucket key, keep the NEWEST snapshot. parsed is already sorted
    // newest first, so the first occurrence of each key is the keeper.
    let mut taken: BTreeMap<usize, ()> = BTreeMap::new(); // bucket index → kept once
    let mut seen_keys: Vec<K> = Vec::new();
    for (dt, s) in parsed.iter() {
        if seen_keys.len() >= n && !seen_keys.iter().any(|k| *k == key_for(*dt)) {
            break;
        }
        let k = key_for(*dt);
        if let Some(idx) = seen_keys.iter().position(|x| *x == k) {
            taken.entry(idx).or_insert(());
        } else if seen_keys.len() < n {
            seen_keys.push(k);
            let idx = seen_keys.len() - 1;
            taken.entry(idx).or_insert(());
            let entry = reasons.entry(s.id.clone()).or_default();
            if !entry.contains(&label) {
                entry.push(label);
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;

    fn snap(id: &str, bytes: u64) -> SnapshotMeta {
        SnapshotMeta {
            id: id.to_string(),
            path: format!("/tmp/{}.db", id),
            bytes,
            created: None,
        }
    }

    #[test]
    fn empty_policy_keeps_nothing() {
        let snaps = vec![snap("2026-05-15_11-56", 100)];
        let plan = apply_policy(&snaps, &Policy::default());
        assert_eq!(plan.kept.len(), 0);
        assert_eq!(plan.removed.len(), 1);
        assert_eq!(plan.total_removed_bytes, 100);
    }

    #[test]
    fn keep_last_holds_newest_n() {
        let snaps = vec![
            snap("2026-05-15_11-00", 100),
            snap("2026-05-15_12-00", 200),
            snap("2026-05-15_13-00", 300),
        ];
        let plan = apply_policy(
            &snaps,
            &Policy {
                keep_last: Some(2),
                ..Default::default()
            },
        );
        let kept_ids: Vec<&str> = plan.kept.iter().map(|k| k.id.as_str()).collect();
        assert_eq!(kept_ids, vec!["2026-05-15_13-00", "2026-05-15_12-00"]);
        assert_eq!(plan.removed.len(), 1);
        assert_eq!(plan.removed[0].id, "2026-05-15_11-00");
    }

    #[test]
    fn keep_daily_takes_one_per_local_date() {
        // Three snapshots on three different days; keep_daily=2 keeps the two
        // newest distinct dates.
        let snaps = vec![
            snap("2026-05-13_10-00", 10),
            snap("2026-05-14_10-00", 20),
            snap("2026-05-14_15-00", 25),
            snap("2026-05-15_10-00", 30),
        ];
        let plan = apply_policy(
            &snaps,
            &Policy {
                keep_daily: Some(2),
                ..Default::default()
            },
        );
        let kept: Vec<&str> = plan.kept.iter().map(|k| k.id.as_str()).collect();
        assert!(kept.contains(&"2026-05-15_10-00"));
        assert!(kept.contains(&"2026-05-14_15-00"));
        assert!(!kept.contains(&"2026-05-13_10-00"));
        assert!(!kept.contains(&"2026-05-14_10-00"));
    }

    #[test]
    fn unparseable_id_is_skipped_never_removed() {
        let snaps = vec![
            snap("2026-05-15_11-00", 100),
            snap("my-manual-snapshot", 200),
        ];
        let plan = apply_policy(
            &snaps,
            &Policy {
                keep_last: Some(1),
                ..Default::default()
            },
        );
        assert_eq!(plan.skipped_unparseable, vec!["my-manual-snapshot"]);
        assert_eq!(plan.kept.len(), 1);
        assert_eq!(plan.removed.len(), 0);
    }

    #[test]
    fn snapshot_kept_by_multiple_buckets_lists_all_reasons() {
        let snaps = vec![snap("2026-05-15_11-00", 100)];
        let plan = apply_policy(
            &snaps,
            &Policy {
                keep_last: Some(1),
                keep_daily: Some(1),
                ..Default::default()
            },
        );
        assert_eq!(plan.kept.len(), 1);
        assert!(plan.kept[0].reasons.contains(&"last"));
        assert!(plan.kept[0].reasons.contains(&"daily"));
    }

    #[test]
    fn total_removed_bytes_sums_correctly() {
        let snaps = vec![
            snap("2026-05-13_10-00", 10),
            snap("2026-05-14_10-00", 20),
            snap("2026-05-15_10-00", 30),
        ];
        let plan = apply_policy(
            &snaps,
            &Policy {
                keep_last: Some(1),
                ..Default::default()
            },
        );
        assert_eq!(plan.total_removed_bytes, 30); // 10 + 20
    }
}
