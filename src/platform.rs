//! Per-OS conventions: path separators, home and data folders, the default
//! scan root, and how a path is opened or revealed in the file manager.
//!
//! Everything the engine stores or compares is a `/`-separated string
//! (REQ-XPLAT-001), so the folder-prefix logic in `map`, `cleanup` and
//! `prefs` is one code path on every OS.
//!
//! @implements REQ-XPLAT-001
//! @implements REQ-XPLAT-002
//! @implements REQ-XPLAT-008

use anyhow::Result;
use std::path::{Path, PathBuf};

/// The operating systems disky knows how to talk to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    MacOs,
    Linux,
    Windows,
}

impl Os {
    /// The OS this binary was built for. Any other Unix is treated as Linux
    /// (freedesktop conventions).
    pub const fn current() -> Os {
        if cfg!(target_os = "macos") {
            Os::MacOs
        } else if cfg!(windows) {
            Os::Windows
        } else {
            Os::Linux
        }
    }
}

/// `path` as the engine stores it: `/` separators (REQ-XPLAT-001).
pub fn to_slash(path: &Path) -> String {
    to_slash_str(&path.to_string_lossy())
}

/// [`to_slash`] for a string. On Unix a `\` is an ordinary file-name byte,
/// so only Windows paths are rewritten.
pub fn to_slash_str(s: &str) -> String {
    to_slash_for(Os::current(), s)
}

fn to_slash_for(os: Os, s: &str) -> String {
    if os != Os::Windows {
        return s.to_string();
    }
    // `canonicalize` yields verbatim paths (`\\?\C:\x`, `\\?\UNC\srv\x`).
    let s = match s.strip_prefix(r"\\?\UNC\") {
        Some(rest) => format!(r"\\{rest}"),
        None => s.strip_prefix(r"\\?\").unwrap_or(s).to_string(),
    };
    s.replace('\\', "/")
}

/// `path` with the OS's own separator, for handing to another program
/// (Explorer wants `\`).
pub fn to_native(path: &str) -> String {
    to_native_for(Os::current(), path)
}

fn to_native_for(os: Os, path: &str) -> String {
    if os == Os::Windows {
        path.replace('/', "\\")
    } else {
        path.to_string()
    }
}

/// The user's home: `$HOME` on Unix, `%USERPROFILE%` on Windows, read from
/// the environment first so tests can point it at a temp folder
/// (REQ-XPLAT-002.A2).
pub fn home_dir() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(dirs::home_dir)
}

/// [`home_dir`] as a `/`-separated string.
pub fn home_str() -> Option<String> {
    home_dir().map(|h| to_slash(&h))
}

/// The per-user data folder disky's own folder lives in
/// (REQ-XPLAT-002.A3): `~/Library/Application Support` (macOS),
/// `$XDG_DATA_HOME` or `~/.local/share` (Linux), `%LOCALAPPDATA%`
/// (Windows). The environment wins on Windows too.
pub fn data_local_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        if let Some(d) = std::env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty()) {
            return Some(PathBuf::from(d));
        }
    }
    if cfg!(all(unix, not(target_os = "macos"))) {
        if let Some(d) = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
            return Some(PathBuf::from(d));
        }
        return home_dir().map(|h| h.join(".local/share"));
    }
    if cfg!(target_os = "macos") {
        return home_dir().map(|h| h.join("Library/Application Support"));
    }
    dirs::data_local_dir()
}

/// What `disky scan` scans with no path (REQ-XPLAT-002.A1): `/`, or the
/// system drive's root on Windows.
pub fn default_root() -> String {
    if cfg!(windows) {
        let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        format!("{}/", drive.trim_end_matches(['/', '\\']))
    } else {
        "/".to_string()
    }
}

/// True for an absolute `/`-separated path: `/…`, or `C:/…` / `C:` on
/// Windows (REQ-XPLAT-001).
pub fn is_absolute_str(s: &str) -> bool {
    is_absolute_for(Os::current(), s)
}

fn is_absolute_for(os: Os, s: &str) -> bool {
    if s.starts_with('/') {
        return true;
    }
    let b = s.as_bytes();
    os == Os::Windows
        && b.len() >= 2
        && b[0].is_ascii_alphabetic()
        && b[1] == b':'
        && (b.len() == 2 || b[2] == b'/')
}

/// A user-typed path as the engine stores it: `~` expanded against `home`
/// and separators normalized. Relative paths stay relative.
pub fn expand_user(p: &str, home: Option<&Path>) -> String {
    let p = to_slash_str(p);
    match (p.strip_prefix("~/").or((p == "~").then_some("")), home) {
        (Some(rest), Some(h)) => {
            let h = to_slash(h);
            let h = h.trim_end_matches('/');
            if rest.is_empty() {
                h.to_string()
            } else {
                format!("{h}/{rest}")
            }
        }
        _ => p,
    }
}

/// One program to run. `exit_code_meaningful` is false for Explorer, which
/// exits 1 even when it did what was asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub program: &'static str,
    /// Arguments as the program must see them. On Windows they are passed
    /// verbatim (already quoted), because Explorer parses its own command
    /// line.
    pub args: Vec<String>,
    pub exit_code_meaningful: bool,
}

/// How `os` opens `path` (a file or a folder) in its default app.
pub fn open_command(os: Os, path: &str) -> Launch {
    match os {
        Os::MacOs => Launch {
            program: "open",
            args: vec![path.to_string()],
            exit_code_meaningful: true,
        },
        Os::Linux => Launch {
            program: "xdg-open",
            args: vec![path.to_string()],
            exit_code_meaningful: true,
        },
        Os::Windows => Launch {
            program: "explorer",
            args: vec![format!("\"{}\"", to_native_for(os, path))],
            exit_code_meaningful: false,
        },
    }
}

/// How `os` shows `path` selected in its file manager, in the order to try
/// (REQ-XPLAT-008). Linux asks the file manager over D-Bus and falls back
/// to opening the parent folder.
pub fn reveal_commands(os: Os, path: &str) -> Vec<Launch> {
    match os {
        Os::MacOs => vec![Launch {
            program: "open",
            args: vec!["-R".into(), path.to_string()],
            exit_code_meaningful: true,
        }],
        Os::Windows => vec![Launch {
            program: "explorer",
            args: vec![format!("/select,\"{}\"", to_native_for(os, path))],
            exit_code_meaningful: false,
        }],
        Os::Linux => {
            let parent = Path::new(path)
                .parent()
                .map_or_else(|| "/".to_string(), |p| p.to_string_lossy().into_owned());
            vec![
                Launch {
                    program: "dbus-send",
                    args: vec![
                        "--session".into(),
                        "--print-reply".into(),
                        "--dest=org.freedesktop.FileManager1".into(),
                        "--type=method_call".into(),
                        "/org/freedesktop/FileManager1".into(),
                        "org.freedesktop.FileManager1.ShowItems".into(),
                        format!("array:string:{}", file_uri(path)),
                        "string:".into(),
                    ],
                    exit_code_meaningful: true,
                },
                open_command(os, &parent),
            ]
        }
    }
}

/// `file://` URI for an absolute path. Everything but unreserved bytes and
/// `/` is percent-encoded, which also keeps `dbus-send` from splitting the
/// array on a `,` in the name.
fn file_uri(path: &str) -> String {
    let mut out = String::from("file://");
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Swap `tmp` in for `dest` with one rename. Windows refuses to replace a
/// file another process (a reader, an indexer, antivirus) holds open for a
/// moment, so there a denied replace is retried for up to ~0.5 s before the
/// error is returned (REQ-XPLAT-004.A2).
pub fn replace_file(tmp: &Path, dest: &Path) -> std::io::Result<()> {
    let mut tries = 0;
    loop {
        match std::fs::rename(tmp, dest) {
            Err(e)
                if cfg!(windows)
                    && e.kind() == std::io::ErrorKind::PermissionDenied
                    && tries < 50 =>
            {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            r => return r,
        }
    }
}

/// Open `path` in its default app (the report, a folder).
pub fn open_path(path: &str) -> Result<()> {
    run_first(&[open_command(Os::current(), path)], path)
}

/// Show `path` selected in the file manager.
pub fn reveal_path(path: &str) -> Result<()> {
    run_first(&reveal_commands(Os::current(), path), path)
}

/// Run each launch in turn until one succeeds.
fn run_first(launches: &[Launch], path: &str) -> Result<()> {
    let mut last = None;
    for l in launches {
        let mut cmd = std::process::Command::new(l.program);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            for a in &l.args {
                cmd.raw_arg(a);
            }
        }
        #[cfg(not(windows))]
        cmd.args(&l.args);
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        match cmd.status() {
            Ok(s) if s.success() || !l.exit_code_meaningful => return Ok(()),
            Ok(s) => last = Some(format!("{} exited with {s}", l.program)),
            Err(e) => last = Some(format!("{}: {e}", l.program)),
        }
    }
    anyhow::bail!(
        "could not open {path}: {}",
        last.unwrap_or_else(|| "no file manager".into())
    )
}

/// What the file manager is called, for messages ("Finder could not …").
pub fn file_manager_name() -> &'static str {
    match Os::current() {
        Os::MacOs => "Finder",
        Os::Windows => "Explorer",
        Os::Linux => "The file manager",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_paths_become_forward_slashes() {
        let w = |s| to_slash_for(Os::Windows, s);
        assert_eq!(w(r"C:\Users\me\a b\x.txt"), "C:/Users/me/a b/x.txt");
        assert_eq!(w(r"\\?\C:\Users\me"), "C:/Users/me");
        assert_eq!(w(r"\\?\UNC\srv\share\x"), "//srv/share/x");
        assert_eq!(w("C:/already/fine"), "C:/already/fine");
        assert_eq!(to_native_for(Os::Windows, "C:/a/b"), r"C:\a\b");
        assert!(is_absolute_for(Os::Windows, "C:/Users"));
        assert!(is_absolute_for(Os::Windows, "d:"));
        assert!(!is_absolute_for(Os::Windows, "C:rel"));
        assert!(!is_absolute_for(Os::Linux, "C:/Users"));
    }

    #[test]
    fn unix_paths_are_unchanged() {
        for os in [Os::MacOs, Os::Linux] {
            assert_eq!(to_slash_for(os, r"/tmp/odd\name"), r"/tmp/odd\name");
            assert_eq!(to_native_for(os, "/a/b"), "/a/b");
        }
    }

    #[test]
    fn home_prefers_the_environment() {
        // The test runner's own home: whatever the variable says wins.
        let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
        if let Some(v) = std::env::var_os(var).filter(|v| !v.is_empty()) {
            assert_eq!(home_dir(), Some(PathBuf::from(v)));
        }
        let h = Path::new("/home/me");
        assert_eq!(expand_user("~/x/y", Some(h)), "/home/me/x/y");
        assert_eq!(expand_user("~", Some(h)), "/home/me");
        assert_eq!(expand_user("/abs", Some(h)), "/abs");
        assert_eq!(expand_user("~/x", None), "~/x");
        assert!(default_root().ends_with('/'));
    }

    #[test]
    fn open_and_reveal_commands_per_os() {
        let mac = reveal_commands(Os::MacOs, "/Users/me/x.iso");
        assert_eq!(mac[0].program, "open");
        assert_eq!(mac[0].args, ["-R", "/Users/me/x.iso"]);

        let win = reveal_commands(Os::Windows, "C:/Users/me/a b/x.iso");
        assert_eq!(win[0].program, "explorer");
        assert_eq!(win[0].args, [r#"/select,"C:\Users\me\a b\x.iso""#]);
        assert!(!win[0].exit_code_meaningful, "explorer exits 1 on success");
        assert_eq!(
            open_command(Os::Windows, "C:/Users").args,
            [r#""C:\Users""#]
        );

        let lin = reveal_commands(Os::Linux, "/home/me/a b,c.iso");
        assert_eq!(lin[0].program, "dbus-send");
        assert!(lin[0]
            .args
            .contains(&"array:string:file:///home/me/a%20b%2Cc.iso".to_string()));
        assert_eq!(lin[1].program, "xdg-open");
        assert_eq!(lin[1].args, ["/home/me"]);
        assert_eq!(open_command(Os::Linux, "/x").program, "xdg-open");
    }
}
