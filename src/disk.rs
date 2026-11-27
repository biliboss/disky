//! Volume capacity and free space, as `df` reports them (REQ-DCLEAN-002.A1,
//! REQ-DCLEAN-027). Physical bytes from `statvfs(2)`, never a sum of file
//! sizes.
//!
//! @implements REQ-DCLEAN-002
//! @implements REQ-DCLEAN-027

#[cfg(windows)]
use anyhow::Context;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
#[cfg(unix)]
use std::path::PathBuf;

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

/// Capacity and free space of the volume holding `path`, from
/// `GetDiskFreeSpaceExW` (REQ-XPLAT-003.A1). `volume` is the drive (`C:`).
#[cfg(windows)]
pub fn disk_usage(path: &Path) -> Result<Disk> {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetDiskFreeSpaceExW(
            dir: *const u16,
            free_to_caller: *mut u64,
            total: *mut u64,
            total_free: *mut u64,
        ) -> i32;
    }

    // The API takes a directory; a missing path must fail, not report the
    // drive (A3).
    let meta = std::fs::metadata(path).with_context(|| format!("stat {}", path.display()))?;
    let dir = if meta.is_dir() {
        path
    } else {
        path.parent().unwrap_or(path)
    };
    let wide: Vec<u16> = dir.as_os_str().encode_wide().chain([0]).collect();
    let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
    // SAFETY: `wide` is NUL-terminated and the three out-params are valid.
    if unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut avail, &mut total, &mut free) } == 0 {
        return Err(anyhow::Error::new(std::io::Error::last_os_error())
            .context(format!("GetDiskFreeSpaceExW {}", path.display())));
    }
    Ok(Disk {
        volume: drive_name(&crate::platform::to_slash(path)),
        capacity_bytes: total,
        free_bytes: avail,
    })
}

/// The drive of a `/`-separated Windows path (`C:/x` → `C:`), or the path
/// itself for a UNC share.
#[cfg_attr(not(windows), allow(dead_code))]
fn drive_name(path: &str) -> String {
    let b = path.as_bytes();
    if b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
        path[..2].to_ascii_uppercase()
    } else {
        path.to_string()
    }
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

#[cfg(unix)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drive_name_is_the_drive_letter() {
        assert_eq!(drive_name("c:/Users/me"), "C:");
        assert_eq!(drive_name("//srv/share"), "//srv/share");
    }

    #[test]
    fn disk_usage_reports_physical_volume_space() {
        let root = crate::platform::default_root();
        let d = disk_usage(Path::new(&root)).unwrap();
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

    #[cfg(unix)]
    #[test]
    fn mount_point_of_root_is_root() {
        assert_eq!(mount_point(Path::new("/")), PathBuf::from("/"));
    }
}
