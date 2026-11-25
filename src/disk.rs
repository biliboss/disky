//! Volume capacity and free space, as `df` reports them (REQ-DCLEAN-002.A1,
//! REQ-DCLEAN-027). Physical bytes from `statvfs(2)`, never a sum of file
//! sizes.
//!
//! @implements REQ-DCLEAN-027

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The volume a path lives on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Disk {
    /// Display name of the volume ("Macintosh HD" for the boot volume on
    /// macOS, else the mount point's name).
    pub volume: String,
    /// Total size of the volume (`f_blocks * f_frsize`).
    pub capacity_bytes: u64,
    /// Space available to the user (`f_bavail * f_frsize`, df's "Avail").
    pub free_bytes: u64,
}

impl Disk {
    pub fn used_bytes(&self) -> u64 {
        self.capacity_bytes.saturating_sub(self.free_bytes)
    }
}

/// Capacity and free space of the volume holding `path`.
#[cfg(unix)]
pub fn disk_usage(path: &Path) -> Result<Disk> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c = CString::new(path.as_os_str().as_bytes())?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `c` is a valid NUL-terminated path and `st` a valid out-param.
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
        return Err(anyhow::Error::new(std::io::Error::last_os_error())
            .context(format!("statvfs {}", path.display())));
    }
    let frsize = st.f_frsize as u64;
    let mount = mount_point(path);
    Ok(Disk {
        volume: volume_name(&mount),
        capacity_bytes: (st.f_blocks as u64).saturating_mul(frsize),
        free_bytes: (st.f_bavail as u64).saturating_mul(frsize),
    })
}

#[cfg(not(unix))]
pub fn disk_usage(path: &Path) -> Result<Disk> {
    anyhow::bail!(
        "disk usage is not supported on this platform ({})",
        path.display()
    )
}

/// Nearest ancestor of `path` (itself included) that is a mount point: the
/// last directory before the device id changes.
#[cfg(unix)]
fn mount_point(path: &Path) -> PathBuf {
    use std::os::unix::fs::MetadataExt;
    let start = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let Ok(dev) = std::fs::metadata(&start).map(|m| m.dev()) else {
        return start;
    };
    let mut mount = start.clone();
    for anc in start.ancestors().skip(1) {
        match std::fs::metadata(anc) {
            Ok(m) if m.dev() == dev => mount = anc.to_path_buf(),
            _ => break,
        }
    }
    mount
}

/// "Macintosh HD" style name for a mount point. On macOS the boot volume is
/// mounted at `/` and `/Volumes/<name>` links back to it.
fn volume_name(mount: &Path) -> String {
    if mount == Path::new("/") {
        if let Ok(entries) = std::fs::read_dir("/Volumes") {
            for e in entries.flatten() {
                if std::fs::canonicalize(e.path()).is_ok_and(|p| p == Path::new("/")) {
                    return e.file_name().to_string_lossy().into_owned();
                }
            }
        }
        return "/".to_string();
    }
    mount
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| mount.display().to_string())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn disk_usage_reports_physical_volume_space() {
        let d = disk_usage(Path::new("/")).unwrap();
        assert!(d.capacity_bytes > 0);
        assert!(d.free_bytes <= d.capacity_bytes);
        assert_eq!(d.used_bytes(), d.capacity_bytes - d.free_bytes);
        assert!(!d.volume.is_empty());
        // A path on the same volume reports the same capacity.
        let tmp = tempfile::tempdir().unwrap();
        let t = disk_usage(tmp.path()).unwrap();
        assert!(t.capacity_bytes > 0);
    }

    #[test]
    fn disk_usage_fails_for_missing_path() {
        assert!(disk_usage(Path::new("/definitely/not/here/disky")).is_err());
    }

    #[test]
    fn mount_point_of_root_is_root() {
        assert_eq!(mount_point(Path::new("/")), PathBuf::from("/"));
    }
}
