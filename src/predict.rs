//! Linear extrapolation of disk fill-by date.
//!
//! Reads every snapshot in the data dir, pulls (started_at, total_bytes)
//! per snapshot, fits a line, projects when usage hits the volume's free
//! ceiling. Useful for "you will run out of disk in 12 days at the
//! current growth rate" agent suggestions.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::{db, query, snapshots};

#[derive(Debug, Clone, Serialize)]
pub struct PredictRecord {
    /// Latest measured bytes (logical by default; physical when `physical=true`).
    pub current_bytes: u64,
    /// Linear-fit slope in bytes/day. Negative when usage is shrinking.
    pub growth_rate_bytes_per_day: f64,
    /// Free bytes on the volume at prediction time. Pass-through from the caller.
    pub free_bytes_now: Option<u64>,
    /// RFC 3339 predicted moment the volume hits zero free space.
    /// `None` when slope <= 0 (never fills) or history insufficient.
    pub fill_at: Option<String>,
    /// Days until fill_at. Mirrors `fill_at` with the same null semantics.
    pub days_until_fill: Option<f64>,
    /// Pearson r² of the linear fit (0..1). Closer to 1 = trust the slope.
    /// 1.0 when only 2 points are available (always a perfect line).
    pub confidence_r2: f64,
    /// Number of snapshots used in the fit.
    pub samples: usize,
    /// Reason if `fill_at` is None.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

pub fn predict(physical: bool, free_bytes_now: Option<u64>) -> Result<PredictRecord> {
    // Collect (timestamp, total_bytes) per snapshot in data dir.
    let snaps = snapshots::list_snapshots();
    if snaps.is_empty() {
        return Ok(empty("no snapshots found — run `disky scan` first"));
    }
    let mut samples: Vec<(f64, f64)> = Vec::new();
    for (path, _) in &snaps {
        let id = match snapshots::id_for(path) {
            Some(id) => id,
            None => continue,
        };
        let dt = match snapshots::parse_id(&id) {
            Some(d) => d,
            None => continue,
        };
        let conn = match db::open(path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let stats = if physical {
            query::stats_physical(&conn)
        } else {
            query::stats(&conn)
        };
        let total = match stats {
            Ok(s) => s.total_bytes as f64,
            Err(_) => continue,
        };
        samples.push((dt.timestamp() as f64, total));
    }
    if samples.len() < 2 {
        return Ok(empty(
            "insufficient history — need >= 2 parseable snapshots",
        ));
    }
    samples.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

    let (slope_per_sec, intercept, r2) = linfit(&samples);
    let slope_per_day = slope_per_sec * 86400.0;
    let latest = samples.last().unwrap();
    let current_bytes = latest.1 as u64;

    let (fill_at, days_until, reason) = match (slope_per_day, free_bytes_now) {
        (s, _) if s <= 0.0 => (
            None,
            None,
            Some("slope is non-positive — usage is flat or shrinking".to_string()),
        ),
        (s, Some(free)) => {
            let days = free as f64 / s;
            let secs = days * 86400.0;
            let fill_ts = latest.0 + secs;
            let fill_dt = DateTime::<Utc>::from_timestamp(fill_ts as i64, 0)
                .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
            (fill_dt, Some(days), None)
        }
        (_, None) => (
            None,
            None,
            Some("provide --free-bytes to get a fill-by date".to_string()),
        ),
    };

    let _ = intercept; // emitted for transparency in v0.11 (e.g. when slope was first non-zero)
    Ok(PredictRecord {
        current_bytes,
        growth_rate_bytes_per_day: slope_per_day,
        free_bytes_now,
        fill_at,
        days_until_fill: days_until,
        confidence_r2: r2,
        samples: samples.len(),
        reason,
    })
}

fn empty(reason: &str) -> PredictRecord {
    PredictRecord {
        current_bytes: 0,
        growth_rate_bytes_per_day: 0.0,
        free_bytes_now: None,
        fill_at: None,
        days_until_fill: None,
        confidence_r2: 0.0,
        samples: 0,
        reason: Some(reason.to_string()),
    }
}

/// Ordinary least squares. Returns (slope, intercept, r²).
fn linfit(points: &[(f64, f64)]) -> (f64, f64, f64) {
    let n = points.len() as f64;
    let mean_x: f64 = points.iter().map(|p| p.0).sum::<f64>() / n;
    let mean_y: f64 = points.iter().map(|p| p.1).sum::<f64>() / n;
    let mut num = 0.0;
    let mut den_x = 0.0;
    let mut den_y = 0.0;
    for (x, y) in points {
        let dx = x - mean_x;
        let dy = y - mean_y;
        num += dx * dy;
        den_x += dx * dx;
        den_y += dy * dy;
    }
    let slope = if den_x == 0.0 { 0.0 } else { num / den_x };
    let intercept = mean_y - slope * mean_x;
    let r2 = if den_x == 0.0 || den_y == 0.0 {
        // Only one distinct x or y — define as 1.0 when both are constant,
        // else 0.0. Two-point fits always pass through both points → 1.0.
        if points.len() == 2 {
            1.0
        } else {
            0.0
        }
    } else {
        (num * num) / (den_x * den_y)
    };
    (slope, intercept, r2)
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;

    #[test]
    fn linfit_perfect_line_yields_r2_one() {
        let pts = vec![(0.0, 0.0), (1.0, 1.0), (2.0, 2.0), (3.0, 3.0)];
        let (slope, intercept, r2) = linfit(&pts);
        assert!((slope - 1.0).abs() < 1e-9);
        assert!(intercept.abs() < 1e-9);
        assert!((r2 - 1.0).abs() < 1e-9);
    }

    #[test]
    fn linfit_two_points_perfect() {
        let pts = vec![(0.0, 5.0), (1.0, 10.0)];
        let (slope, intercept, r2) = linfit(&pts);
        assert!((slope - 5.0).abs() < 1e-9);
        assert!((intercept - 5.0).abs() < 1e-9);
        assert_eq!(r2, 1.0);
    }

    #[test]
    fn linfit_flat_data_has_zero_slope() {
        let pts = vec![(0.0, 10.0), (1.0, 10.0), (2.0, 10.0)];
        let (slope, _, _) = linfit(&pts);
        assert_eq!(slope, 0.0);
    }

    #[test]
    fn empty_returns_zero_with_reason() {
        let r = empty("nothing");
        assert_eq!(r.samples, 0);
        assert!(r.fill_at.is_none());
        assert_eq!(r.reason.as_deref(), Some("nothing"));
    }
}
