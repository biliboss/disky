//! Live volume free-space probe.
//!
//! A snapshot cannot carry this number: `stats` reports what a scan measured
//! at scan time, while a cleanup guard needs what the filesystem says *right
//! now*. It is also the missing input for [`crate::predict`] and
//! `growth --over-n`, whose fill-by projections are inert without a byte
//! budget to extrapolate against — before this existed the caller had to
//! shell out to `df` and do the unit arithmetic by hand.

use anyhow::{anyhow, Result};
use serde::Serialize;
use std::ffi::CString;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct VolumeRecord {
    /// Path that was probed (not the mount point — the volume is whichever
    /// one holds this path).
    pub path: String,
    pub total_bytes: u64,
    /// Blocks available to an unprivileged process. Smaller than the
    /// root-only free space on filesystems that reserve a margin, which is
    /// the honest number for "can I still write files".
    pub free_bytes: u64,
    pub used_bytes: u64,
    /// 0..100, computed as `used / total`. Reads 1–2 points *above* `df`'s
    /// Capacity column, which divides by `used + available` and so hides the
    /// root-reserved margin. Neither is wrong; this one is the fraction of the
    /// whole volume.
    pub used_pct: f64,
}

/// `statvfs(2)` on the volume holding `path`.
pub fn probe(path: &Path) -> Result<VolumeRecord> {
    let c_path = CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|_| anyhow!("path contains an interior NUL byte: {}", path.display()))?;

    // SAFETY: c_path outlives the call; statvfs only reads it. The buffer is
    // fully written by the syscall on success and never read on failure.
    let stat = unsafe {
        let mut buf: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c_path.as_ptr(), &mut buf) != 0 {
            return Err(std::io::Error::last_os_error())
                .map_err(|e| anyhow!("statvfs({}) failed: {}", path.display(), e));
        }
        buf
    };

    // f_frsize is the fragment size the block counts are denominated in;
    // f_bsize is the preferred I/O size and is the wrong multiplier here.
    let unit = stat.f_frsize as u64;
    let total_bytes = (stat.f_blocks as u64).saturating_mul(unit);
    let free_bytes = (stat.f_bavail as u64).saturating_mul(unit);
    let used_bytes = total_bytes.saturating_sub(free_bytes);
    let used_pct = if total_bytes == 0 {
        0.0
    } else {
        used_bytes as f64 / total_bytes as f64 * 100.0
    };

    Ok(VolumeRecord {
        path: path.display().to_string(),
        total_bytes,
        free_bytes,
        used_bytes,
        used_pct,
    })
}

/// Free bytes on the volume holding `path`, or the `$HOME` volume when
/// `None`. Returns `None` on any failure — callers must treat that as
/// "unknown" and never as zero, or a probe error reads as a full disk.
pub fn free_bytes(path: Option<&Path>) -> Option<u64> {
    let owned: Option<PathBuf> = match path {
        Some(p) => Some(p.to_path_buf()),
        None => dirs::home_dir(),
    };
    probe(owned?.as_path()).ok().map(|v| v.free_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_root_is_internally_consistent() {
        let v = probe(Path::new("/")).expect("root volume must be probeable");
        assert!(v.total_bytes > 0, "total must be non-zero");
        assert!(v.free_bytes <= v.total_bytes);
        assert_eq!(v.used_bytes, v.total_bytes - v.free_bytes);
        assert!((0.0..=100.0).contains(&v.used_pct));
    }

    #[test]
    fn probe_missing_path_errors() {
        // Negative control: a bad path must fail loudly, not report 0 free.
        assert!(probe(Path::new("/nonexistent-disky-probe-target")).is_err());
    }

    #[test]
    fn free_bytes_defaults_to_home_volume() {
        assert!(free_bytes(None).unwrap_or(0) > 0);
    }
}
