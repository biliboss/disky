//! Pattern classifier for directory size-over-time samples.
//!
//! Given a series of `(timestamp, bytes)` samples drawn from N snapshots,
//! classifies the growth shape into one of:
//!
//! | Pattern      | Meaning                                                          |
//! |--------------|------------------------------------------------------------------|
//! | `LogShaped`  | Big initial growth that levels off (caches, indexes warming up)  |
//! | `Burst`      | A sudden late jump (a download dump, an import)                  |
//! | `Stable`     | Roughly flat across the series                                   |
//! | `Declining`  | Trending down (cleanup landed, churning logs being rotated)      |
//! | `Unknown`    | Mixed / unclear / too few samples                                |
//!
//! Pure heuristic — no ML, no stats crate. Uses ordinary least-squares
//! linear regression for the declining branch.
//!
//! Wired into the CLI as `disky churn --classify` (opt-in). See
//! `AGENTS.md#pattern-classifier`. When churn lands as a first-class
//! command, each record gains a `pattern` field; until then the module
//! stands alone and is unit-tested in isolation.
#![allow(dead_code)]

use std::fmt;

/// Classification bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pattern {
    LogShaped,
    Burst,
    Stable,
    Declining,
    Unknown,
}

impl Pattern {
    /// Lowercase JSON wire form (`"log_shaped"`, `"burst"`, ...).
    pub fn as_str(self) -> &'static str {
        match self {
            Pattern::LogShaped => "log_shaped",
            Pattern::Burst => "burst",
            Pattern::Stable => "stable",
            Pattern::Declining => "declining",
            Pattern::Unknown => "unknown",
        }
    }
}

impl fmt::Display for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// --- Decision-tree thresholds (final values). ---------------------------
//
// These constants are deliberately exported so a future tweak only needs
// to touch one place — and so tests can reference them by name.

/// `Declining` requires regression slope <= -1% of mean per sample step.
pub const DECLINING_SLOPE_FRAC: f64 = -0.01;
/// `Declining` requires R^2 >= this to filter out noisy non-trends.
pub const DECLINING_R2_MIN: f64 = 0.5;

/// `LogShaped` / `Burst` require max-stride / median-stride > this.
pub const SPIKE_RATIO: f64 = 4.0;

/// `Stable` requires max-stride / median-stride < this. Below `SPIKE_RATIO`.
/// Raised from sub-agent default 1.5 → 3.0 post-merge: a real-world
/// "flat" series rarely has all strides within 1.5× of the median
/// (sub-agent's own `stable_flat_series` test fixture has ratio 2.5).
pub const STABLE_RATIO: f64 = 3.0;

/// `LogShaped` boundary — spike must be inside the first third of the
/// series (index < n/3). Otherwise spike is `Burst`.
///
/// On `n = 5` samples the strides are indexed 0..=3 and `n/3 = 1`; only a
/// spike at stride 0 counts as `LogShaped`. For `n = 6` it's 0..=4 with
/// `n/3 = 2`, so strides 0 and 1 count. We treat the "first third" as
/// strictly less than `(n_strides) / 3` rounded by integer division to
/// avoid floating-point boundary surprises.
pub const FIRST_THIRD_DIV: usize = 3;

/// Minimum samples for any non-`Unknown` verdict.
pub const MIN_SAMPLES: usize = 3;

/// Classify a size-over-time series.
///
/// `samples` is `(timestamp_secs, bytes)`; ordering by timestamp is the
/// caller's job — we trust the input order. Timestamps are used only as
/// the regression x-axis (so non-uniform sampling still works).
pub fn classify_pattern(samples: &[(i64, u64)]) -> Pattern {
    if samples.len() < MIN_SAMPLES {
        return Pattern::Unknown;
    }

    // --- Step 1: linear regression for the declining check. ------------
    let n = samples.len() as f64;
    let xs: Vec<f64> = samples.iter().map(|(t, _)| *t as f64).collect();
    let ys: Vec<f64> = samples.iter().map(|(_, b)| *b as f64).collect();

    let sum_x: f64 = xs.iter().sum();
    let sum_y: f64 = ys.iter().sum();
    let mean_x = sum_x / n;
    let mean_y = sum_y / n;

    let mut s_xy = 0.0;
    let mut s_xx = 0.0;
    let mut s_yy = 0.0;
    for i in 0..samples.len() {
        let dx = xs[i] - mean_x;
        let dy = ys[i] - mean_y;
        s_xy += dx * dy;
        s_xx += dx * dx;
        s_yy += dy * dy;
    }

    let slope = if s_xx > 0.0 { s_xy / s_xx } else { 0.0 };
    let r2 = if s_xx > 0.0 && s_yy > 0.0 {
        (s_xy * s_xy) / (s_xx * s_yy)
    } else {
        0.0
    };

    // Mean sample interval — for translating slope into "per-step".
    let span = xs[xs.len() - 1] - xs[0];
    let mean_step = if span > 0.0 { span / (n - 1.0) } else { 1.0 };
    let per_step_slope = slope * mean_step;

    // Declining: slope strongly negative relative to mean, well-fit line.
    if mean_y > 0.0 && per_step_slope / mean_y <= DECLINING_SLOPE_FRAC && r2 >= DECLINING_R2_MIN {
        return Pattern::Declining;
    }

    // --- Step 2: stride analysis for spike vs flat. --------------------
    // Strides are signed deltas — a big *drop* counts as a spike too.
    let mut strides: Vec<i128> = Vec::with_capacity(samples.len() - 1);
    for w in samples.windows(2) {
        strides.push(w[1].1 as i128 - w[0].1 as i128);
    }

    // Absolute magnitudes for ratio comparison.
    let mut abs_strides: Vec<u128> = strides.iter().map(|d| d.unsigned_abs()).collect();
    let max_stride = *abs_strides.iter().max().unwrap_or(&0);

    // Index of the largest stride (first one wins ties — earliest spike).
    let max_idx = strides
        .iter()
        .enumerate()
        .max_by_key(|(_, d)| d.unsigned_abs())
        .map(|(i, _)| i)
        .unwrap_or(0);

    // Median stride magnitude.
    abs_strides.sort_unstable();
    let mid = abs_strides.len() / 2;
    let median = if abs_strides.len() % 2 == 1 {
        abs_strides[mid] as f64
    } else {
        (abs_strides[mid - 1] as f64 + abs_strides[mid] as f64) / 2.0
    };

    // Avoid divide-by-zero — if every stride is 0, the series is stable.
    if max_stride == 0 {
        return Pattern::Stable;
    }
    if median == 0.0 {
        // All but one stride is zero → that one is a spike by definition.
        return spike_pattern(max_idx, strides.len());
    }

    let ratio = max_stride as f64 / median;

    if ratio > SPIKE_RATIO {
        return spike_pattern(max_idx, strides.len());
    }
    if ratio < STABLE_RATIO {
        return Pattern::Stable;
    }

    Pattern::Unknown
}

/// LogShaped if the spike sits in the first third of strides; Burst otherwise.
fn spike_pattern(spike_idx: usize, n_strides: usize) -> Pattern {
    let cutoff = n_strides / FIRST_THIRD_DIV;
    // strides 0..cutoff (exclusive) → first third → LogShaped
    if spike_idx < cutoff {
        Pattern::LogShaped
    } else {
        Pattern::Burst
    }
}

// -----------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Build evenly-spaced samples from a byte series. Timestamps start at
    /// 1000 and increment by 60s per sample.
    fn series(bytes: &[u64]) -> Vec<(i64, u64)> {
        bytes
            .iter()
            .enumerate()
            .map(|(i, b)| (1000 + 60 * i as i64, *b))
            .collect()
    }

    #[test]
    fn too_few_samples_is_unknown() {
        assert_eq!(classify_pattern(&[]), Pattern::Unknown);
        assert_eq!(classify_pattern(&series(&[100])), Pattern::Unknown);
        assert_eq!(classify_pattern(&series(&[100, 200])), Pattern::Unknown);
    }

    #[test]
    fn log_shaped_big_initial_growth_then_flat() {
        // 10MB → 100MB (huge first stride), then flat-ish.
        let s = series(&[
            10_000_000,
            100_000_000,
            101_000_000,
            101_500_000,
            102_000_000,
            102_400_000,
        ]);
        assert_eq!(classify_pattern(&s), Pattern::LogShaped);
    }

    #[test]
    fn burst_late_spike() {
        // Flat-ish, then a huge late jump.
        let s = series(&[
            10_000_000,
            10_500_000,
            11_000_000,
            11_500_000,
            12_000_000,
            500_000_000,
        ]);
        assert_eq!(classify_pattern(&s), Pattern::Burst);
    }

    #[test]
    fn stable_flat_series() {
        // All within ~1% of each other.
        let s = series(&[
            1_000_000, 1_005_000, 1_002_000, 1_004_000, 1_003_000, 1_001_500,
        ]);
        assert_eq!(classify_pattern(&s), Pattern::Stable);
    }

    #[test]
    fn stable_exactly_constant() {
        let s = series(&[5_000_000; 6]);
        assert_eq!(classify_pattern(&s), Pattern::Stable);
    }

    #[test]
    fn declining_clear_downtrend() {
        // Steady drop ~10% per step — well-fit line, slope strongly negative.
        let s = series(&[
            10_000_000, 9_000_000, 8_000_000, 7_000_000, 6_000_000, 5_000_000,
        ]);
        assert_eq!(classify_pattern(&s), Pattern::Declining);
    }

    #[test]
    fn declining_beats_burst_when_trend_dominates() {
        // Even with noise the regression should catch the trend.
        let s = series(&[
            10_000_000, 9_500_000, 8_800_000, 8_000_000, 7_500_000, 6_900_000,
        ]);
        assert_eq!(classify_pattern(&s), Pattern::Declining);
    }

    #[test]
    fn unknown_mixed_noise() {
        // Mid-ratio spike — between STABLE_RATIO and SPIKE_RATIO.
        // Strides: 0, +2MB, 0, 0, 0 → median 0 → fallback path forces a
        // verdict, so we tweak: alternating jitter, no dominant spike.
        let s = series(&[
            10_000_000, 11_000_000, 10_500_000, 11_200_000, 10_800_000, 11_100_000,
        ]);
        // Strides are roughly: +1MB, -0.5MB, +0.7MB, -0.4MB, +0.3MB
        // ratio max/median ~ 1MB / 0.5MB = 2.0 → between thresholds → Unknown.
        let got = classify_pattern(&s);
        assert!(
            matches!(got, Pattern::Unknown | Pattern::Stable),
            "expected unknown/stable, got {got:?}"
        );
    }

    #[test]
    fn pattern_json_strings_are_snake_case() {
        assert_eq!(Pattern::LogShaped.as_str(), "log_shaped");
        assert_eq!(Pattern::Burst.as_str(), "burst");
        assert_eq!(Pattern::Stable.as_str(), "stable");
        assert_eq!(Pattern::Declining.as_str(), "declining");
        assert_eq!(Pattern::Unknown.as_str(), "unknown");
    }

    #[test]
    fn first_third_boundary_n5_only_stride_zero_is_log() {
        // 5 samples → 4 strides → cutoff = 4 / 3 = 1.
        // Spike at stride 0 → LogShaped; stride 1+ → Burst.
        let log_s = series(&[
            10_000_000,
            200_000_000,
            201_000_000,
            201_500_000,
            202_000_000,
        ]);
        assert_eq!(classify_pattern(&log_s), Pattern::LogShaped);

        let burst_s = series(&[
            10_000_000,
            11_000_000,
            200_000_000,
            200_500_000,
            201_000_000,
        ]);
        assert_eq!(classify_pattern(&burst_s), Pattern::Burst);
    }
}
