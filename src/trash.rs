//! The OS Trash (REQ-XPLAT-006): where a reversible clean puts each path,
//! in the layout the OS's own file manager reads, so the user can restore
//! it from there as well as from `disky restore`.
//!
//! - macOS: `~/.Trash/<name>-<stamp>`.
//! - Linux: the freedesktop.org Trash, `files/<name>` plus
//!   `info/<name>.trashinfo`.
//! - Windows: the drive's Recycle Bin, `$R<id><ext>` plus the `$I<id><ext>`
//!   record Explorer reads.
//!
//! Every move is one `rename`: a path on another volume fails and stays
//! where it is (A5). Nothing is copied and then deleted.
//!
//! @implements REQ-XPLAT-006

use anyhow::{Context, Result};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::platform::Os;

/// One Trash, by layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bin {
    /// macOS: `<root>/<name>-<secs>-<nanos>`.
    Flat(PathBuf),
    /// freedesktop.org: `<root>/files` and `<root>/info`.
    FreeDesktop(PathBuf),
    /// Windows: one user's Recycle Bin folder on one drive,
    /// `<drive>/$Recycle.Bin/<SID>`.
    Recycle(PathBuf),
}

/// The Recycle Bin folder name on every NTFS/FAT drive.
pub const RECYCLE_DIR: &str = "$Recycle.Bin";

impl Bin {
    /// The layout `os` uses for a Trash rooted at `root`.
    pub fn for_os(os: Os, root: &Path) -> Bin {
        match os {
            Os::MacOs => Bin::Flat(root.to_path_buf()),
            Os::Linux => Bin::FreeDesktop(root.to_path_buf()),
            Os::Windows => Bin::Recycle(root.to_path_buf()),
        }
    }

    /// [`Bin::for_os`] for this OS.
    pub fn for_root(root: &Path) -> Bin {
        Bin::for_os(Os::current(), root)
    }

    /// The folder trashed items land in. For a Recycle Bin, an item on
    /// another drive goes to that drive's bin (same SID), so the move stays
    /// a rename.
    fn dir_for(&self, src: &Path) -> PathBuf {
        match self {
            Bin::Flat(root) => root.clone(),
            Bin::FreeDesktop(root) => root.join("files"),
            Bin::Recycle(dir) => recycle_dir_for(dir, src),
        }
    }

    /// True when `path` already sits in this Trash (its contents can only
    /// be deleted permanently).
    pub fn holds(&self, path: &Path) -> bool {
        match self {
            Bin::Flat(root) | Bin::FreeDesktop(root) => {
                !root.as_os_str().is_empty() && path.starts_with(root)
            }
            Bin::Recycle(dir) => {
                (!dir.as_os_str().is_empty() && path.starts_with(dir))
                    || path
                        .components()
                        .any(|c| c.as_os_str().eq_ignore_ascii_case(RECYCLE_DIR))
            }
        }
    }

    /// Move `src` into the Trash and write the layout's record. Returns
    /// where it went (what `history` stores as `trash_path`). `bytes` is
    /// the size the Recycle Bin record shows.
    pub fn put(&self, src: &Path, bytes: u64) -> Result<PathBuf> {
        let dir = self.dir_for(src);
        fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        let name = src
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "disky-trashed".into());
        match self {
            Bin::Flat(_) => {
                let now = now_since_epoch();
                let stamp = format!("{}-{}", now.as_secs(), now.subsec_nanos());
                let mut dest = dir.join(format!("{name}-{stamp}"));
                let mut n = 0u32;
                while dest.symlink_metadata().is_ok() {
                    n += 1;
                    dest = dir.join(format!("{name}-{stamp}-{n}"));
                }
                fs::rename(src, &dest)?;
                Ok(dest)
            }
            Bin::FreeDesktop(root) => {
                let info_dir = root.join("info");
                fs::create_dir_all(&info_dir)?;
                // Reserve the name by creating its .trashinfo exclusively
                // (the spec's own collision rule), then move the item.
                let body = trashinfo(
                    &crate::platform::to_slash(src),
                    &chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
                );
                let mut n = 1u32;
                loop {
                    let candidate = if n == 1 {
                        name.clone()
                    } else {
                        format!("{name}.{n}")
                    };
                    let dest = dir.join(&candidate);
                    let info = info_dir.join(format!("{candidate}.trashinfo"));
                    if dest.symlink_metadata().is_ok() {
                        n += 1;
                        continue;
                    }
                    match fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&info)
                    {
                        Ok(mut f) => {
                            f.write_all(body.as_bytes())?;
                            drop(f);
                            if let Err(e) = fs::rename(src, &dest) {
                                let _ = fs::remove_file(&info);
                                return Err(e.into());
                            }
                            return Ok(dest);
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => n += 1,
                        Err(e) => return Err(e.into()),
                    }
                }
            }
            Bin::Recycle(_) => {
                let ext = Path::new(&name)
                    .extension()
                    .map(|e| format!(".{}", e.to_string_lossy()))
                    .unwrap_or_default();
                let original = crate::platform::to_native(&crate::platform::to_slash(src));
                let record = recycle_record(&original, bytes, filetime_now());
                let mut seed = now_since_epoch().as_nanos() as u64 ^ u64::from(std::process::id());
                loop {
                    let id = recycle_id(&mut seed);
                    let dest = dir.join(format!("$R{id}{ext}"));
                    let info = dir.join(format!("$I{id}{ext}"));
                    if dest.symlink_metadata().is_ok() {
                        continue;
                    }
                    match fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&info)
                    {
                        Ok(mut f) => {
                            f.write_all(&record)?;
                            drop(f);
                            if let Err(e) = fs::rename(src, &dest) {
                                let _ = fs::remove_file(&info);
                                return Err(e.into());
                            }
                            return Ok(dest);
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                        Err(e) => return Err(e.into()),
                    }
                }
            }
        }
    }
}

/// The record a Trash keeps beside a trashed item: `info/<name>.trashinfo`
/// for `files/<name>`, `$I<id>` for `$R<id>`. `None` for a macOS item.
pub fn record_of(trash_path: &Path) -> Option<PathBuf> {
    let name = trash_path.file_name()?.to_string_lossy().into_owned();
    let parent = trash_path.parent()?;
    if parent.file_name().is_some_and(|n| n == "files") {
        return Some(
            parent
                .parent()?
                .join("info")
                .join(format!("{name}.trashinfo")),
        );
    }
    name.strip_prefix("$R")
        .map(|rest| parent.join(format!("$I{rest}")))
}

/// Remove the record of an item that left the Trash (restored, or deleted
/// for good). A missing record is fine: there is nothing to forget.
pub fn forget_record(trash_path: &Path) {
    if let Some(r) = record_of(trash_path) {
        let _ = fs::remove_file(r);
    }
}

/// A freedesktop.org `.trashinfo` body: the original path, URI-escaped,
/// and the local deletion time `YYYY-MM-DDThh:mm:ss`.
pub fn trashinfo(original: &str, deleted_at: &str) -> String {
    let mut path = String::new();
    for b in original.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) {
            path.push(b as char);
        } else {
            path.push_str(&format!("%{b:02X}"));
        }
    }
    format!("[Trash Info]\nPath={path}\nDeletionDate={deleted_at}\n")
}

/// A Windows Recycle Bin `$I` record, format 2 (Windows 10+), all
/// little-endian: version (i64 = 2), size (i64), deletion time (FILETIME),
/// path length in UTF-16 units including the NUL (u32), the path.
pub fn recycle_record(original_native: &str, bytes: u64, filetime: u64) -> Vec<u8> {
    let wide: Vec<u16> = original_native.encode_utf16().chain([0]).collect();
    let mut out = Vec::with_capacity(28 + wide.len() * 2);
    out.extend_from_slice(&2i64.to_le_bytes());
    out.extend_from_slice(&(bytes as i64).to_le_bytes());
    out.extend_from_slice(&filetime.to_le_bytes());
    out.extend_from_slice(&(wide.len() as u32).to_le_bytes());
    for w in wide {
        out.extend_from_slice(&w.to_le_bytes());
    }
    out
}

/// Now as a Windows FILETIME: 100 ns ticks since 1601-01-01 UTC.
fn filetime_now() -> u64 {
    const EPOCH_DIFF_SECS: u64 = 11_644_473_600;
    let d = now_since_epoch();
    (d.as_secs() + EPOCH_DIFF_SECS) * 10_000_000 + u64::from(d.subsec_nanos() / 100)
}

fn now_since_epoch() -> std::time::Duration {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
}

/// Six upper-case alphanumerics, as Explorer names its items.
fn recycle_id(seed: &mut u64) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut s = String::with_capacity(6);
    for _ in 0..6 {
        // xorshift64
        *seed ^= *seed << 13;
        *seed ^= *seed >> 7;
        *seed ^= *seed << 17;
        s.push(ALPHABET[(*seed % ALPHABET.len() as u64) as usize] as char);
    }
    s
}

/// The bin for `src`: `dir` is `<drive>/$Recycle.Bin/<SID>`; an item on
/// another drive uses that drive's bin. A `dir` of any other shape (tests)
/// is used as is.
fn recycle_dir_for(dir: &Path, src: &Path) -> PathBuf {
    let d = crate::platform::to_slash(dir);
    let s = crate::platform::to_slash(src);
    let drive = |p: &str| {
        let b = p.as_bytes();
        (b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':').then(|| p[..2].to_string())
    };
    match (drive(&d), drive(&s)) {
        (Some(dd), Some(sd))
            if !dd.eq_ignore_ascii_case(&sd)
                && d[2..].trim_start_matches('/').starts_with(RECYCLE_DIR) =>
        {
            PathBuf::from(format!("{sd}{}", &d[2..]))
        }
        _ => dir.to_path_buf(),
    }
}

/// The current user's Trash root on this OS (REQ-XPLAT-006): `~/.Trash`
/// (macOS), `$XDG_DATA_HOME/Trash` or `~/.local/share/Trash` (Linux),
/// `<system drive>/$Recycle.Bin/<SID>` (Windows).
///
/// `DISKY_TRASH_ROOT`, when set, replaces it (same layout). The CLI tests
/// use it on Windows, where the Recycle Bin is on the drive rather than in
/// the temp home, so no test ever touches the real one.
pub fn user_root() -> Result<PathBuf> {
    if let Some(d) = std::env::var_os("DISKY_TRASH_ROOT").filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(d));
    }
    let home =
        || crate::platform::home_dir().ok_or_else(|| anyhow::anyhow!("home dir unavailable"));
    match Os::current() {
        Os::MacOs => Ok(home()?.join(".Trash")),
        Os::Linux => Ok(
            match std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
                Some(d) => PathBuf::from(d).join("Trash"),
                None => home()?.join(".local/share/Trash"),
            },
        ),
        Os::Windows => {
            let root = crate::platform::default_root();
            Ok(PathBuf::from(root).join(RECYCLE_DIR).join(user_sid()?))
        }
    }
}

/// The current user's SID (`S-1-5-21-…`), which names their Recycle Bin
/// folder. Read once from the process token, so no helper program is needed.
fn user_sid() -> Result<String> {
    static SID: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    SID.get_or_init(token_user_sid)
        .clone()
        .ok_or_else(|| anyhow::anyhow!("could not read the user's SID for the Recycle Bin"))
}

/// `OpenProcessToken` → `GetTokenInformation(TokenUser)` →
/// `ConvertSidToStringSidW`.
#[cfg(windows)]
fn token_user_sid() -> Option<String> {
    use std::ffi::c_void;
    type Handle = *mut c_void;
    const TOKEN_QUERY: u32 = 0x0008;
    const TOKEN_USER: u32 = 1;
    #[link(name = "advapi32")]
    extern "system" {
        fn OpenProcessToken(process: Handle, access: u32, token: *mut Handle) -> i32;
        fn GetTokenInformation(
            token: Handle,
            class: u32,
            info: *mut c_void,
            len: u32,
            ret: *mut u32,
        ) -> i32;
        fn ConvertSidToStringSidW(sid: *mut c_void, out: *mut *mut u16) -> i32;
    }
    extern "system" {
        fn GetCurrentProcess() -> Handle;
        fn CloseHandle(h: Handle) -> i32;
        fn LocalFree(mem: *mut c_void) -> *mut c_void;
    }
    // SAFETY: each call gets valid out-pointers; the token handle is closed
    // and the string LocalFree'd before returning. The TOKEN_USER buffer is
    // u64-aligned and as large as GetTokenInformation asked for.
    unsafe {
        let mut token: Handle = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return None;
        }
        let mut len = 0u32;
        GetTokenInformation(token, TOKEN_USER, std::ptr::null_mut(), 0, &mut len);
        let mut buf = vec![0u64; (len as usize).div_ceil(8).max(1)];
        let ok = GetTokenInformation(token, TOKEN_USER, buf.as_mut_ptr().cast(), len, &mut len);
        CloseHandle(token);
        if ok == 0 {
            return None;
        }
        // TOKEN_USER starts with SID_AND_ATTRIBUTES, whose first field is the SID.
        let sid = *buf.as_ptr().cast::<*mut c_void>();
        let mut wide: *mut u16 = std::ptr::null_mut();
        if ConvertSidToStringSidW(sid, &mut wide) == 0 || wide.is_null() {
            return None;
        }
        let n = (0..).take_while(|&i| *wide.add(i) != 0).count();
        let s = String::from_utf16_lossy(std::slice::from_raw_parts(wide, n));
        LocalFree(wide.cast());
        s.starts_with("S-").then_some(s)
    }
}

#[cfg(not(windows))]
fn token_user_sid() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freedesktop_put_writes_files_and_info() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("p/node_modules");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("x.js"), b"x").unwrap();
        let bin = Bin::FreeDesktop(tmp.path().join("Trash"));
        let dest = bin.put(&src, 1).unwrap();
        assert_eq!(dest, tmp.path().join("Trash/files/node_modules"));
        assert!(dest.join("x.js").exists() && !src.exists());
        let info =
            fs::read_to_string(tmp.path().join("Trash/info/node_modules.trashinfo")).unwrap();
        let want = format!(
            "Path={}\n",
            crate::platform::to_slash(&src).replace(' ', "%20")
        );
        assert!(info.starts_with("[Trash Info]\n"), "{info}");
        // The freedesktop Trash is Linux's; a `C:` path percent-encodes its colon.
        assert!(!cfg!(unix) || info.contains(&want), "{info}");
        let date = info
            .lines()
            .find_map(|l| l.strip_prefix("DeletionDate="))
            .unwrap();
        assert!(chrono::NaiveDateTime::parse_from_str(date, "%Y-%m-%dT%H:%M:%S").is_ok());
        assert!(bin.holds(&dest));
    }

    #[test]
    fn freedesktop_names_never_collide() {
        let tmp = tempfile::tempdir().unwrap();
        let bin = Bin::FreeDesktop(tmp.path().join("Trash"));
        let mut got = Vec::new();
        for i in 0..3 {
            let src = tmp.path().join(format!("{i}/node_modules"));
            fs::create_dir_all(&src).unwrap();
            fs::write(src.join("v"), i.to_string()).unwrap();
            got.push(bin.put(&src, 1).unwrap());
        }
        let names: Vec<_> = got
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["node_modules", "node_modules.2", "node_modules.3"]);
        for (i, p) in got.iter().enumerate() {
            assert_eq!(fs::read_to_string(p.join("v")).unwrap(), i.to_string());
            assert!(record_of(p).unwrap().exists());
        }
    }

    #[test]
    fn recycle_put_writes_r_and_i_files() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("movie.iso");
        fs::write(&src, b"data").unwrap();
        let bin = Bin::Recycle(tmp.path().join("bin"));
        let dest = bin.put(&src, 4).unwrap();
        let name = dest.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            name.starts_with("$R") && name.ends_with(".iso") && name.len() == 12,
            "{name}"
        );
        assert_eq!(fs::read(&dest).unwrap(), b"data");
        let rec = fs::read(record_of(&dest).unwrap()).unwrap();
        assert_eq!(&rec[..8], &2i64.to_le_bytes());
        assert_eq!(&rec[8..16], &4i64.to_le_bytes());
        assert!(bin.holds(&dest));
    }

    #[test]
    fn recycle_record_is_format_2() {
        let rec = recycle_record(r"C:\a\é.txt", 0x0102, 0x0A0B);
        assert_eq!(&rec[0..8], &[2, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&rec[8..16], &[2, 1, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&rec[16..24], &[0x0B, 0x0A, 0, 0, 0, 0, 0, 0]);
        let chars = "C:\\a\\é.txt".encode_utf16().count() as u32 + 1;
        assert_eq!(&rec[24..28], &chars.to_le_bytes());
        assert_eq!(rec.len(), 28 + chars as usize * 2);
        assert_eq!(&rec[rec.len() - 2..], &[0, 0], "NUL-terminated");
        assert_eq!(&rec[28..30], &[b'C', 0]);
    }

    #[test]
    fn forget_record_removes_the_sidecar() {
        let tmp = tempfile::tempdir().unwrap();
        let fd = Bin::FreeDesktop(tmp.path().join("T"));
        let rb = Bin::Recycle(tmp.path().join("R"));
        for bin in [fd, rb] {
            let src = tmp.path().join("f.txt");
            fs::write(&src, b"x").unwrap();
            let dest = bin.put(&src, 1).unwrap();
            let rec = record_of(&dest).unwrap();
            assert!(rec.exists());
            forget_record(&dest);
            assert!(!rec.exists(), "{bin:?}");
        }
        assert_eq!(record_of(Path::new("/Users/me/.Trash/x-1-2")), None);
    }

    #[test]
    fn another_drive_uses_its_own_bin() {
        let sid = "C:/$Recycle.Bin/S-1-5-21-1";
        assert_eq!(
            crate::platform::to_slash(&recycle_dir_for(Path::new(sid), Path::new("D:/x/y.iso"))),
            "D:/$Recycle.Bin/S-1-5-21-1"
        );
        assert_eq!(
            recycle_dir_for(Path::new(sid), Path::new("C:/x")),
            PathBuf::from(sid)
        );
    }

    /// The Recycle Bin is named by the process's own SID (REQ-XPLAT-006.A6).
    #[test]
    fn the_user_sid_comes_from_the_process_token() {
        let sid = user_sid();
        if cfg!(windows) {
            let sid = sid.expect("Windows must read its own SID");
            assert!(sid.starts_with("S-1-"), "{sid}");
        } else {
            assert!(sid.is_err(), "only Windows names a Recycle Bin by SID");
        }
    }

    #[test]
    fn flat_put_keeps_the_macos_layout() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("a");
        fs::write(&src, b"x").unwrap();
        let dest = Bin::Flat(tmp.path().join(".Trash")).put(&src, 1).unwrap();
        let name = dest.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("a-"), "{name}");
        assert_eq!(record_of(&dest), None);
    }
}
