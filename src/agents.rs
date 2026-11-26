//! Which agent CLIs are installed (REQ-DSCR-007).
//!
//! The Assistant names the agent the user picked (`prefs.agent`), and its
//! side panel lists the three it knows: Claude Code (`claude`), Codex CLI
//! (`codex`) and Gemini CLI (`gemini`), in that order. Detection looks for an
//! executable file named after each CLI in `PATH`, then in `~/.local/bin`,
//! `/opt/homebrew/bin` and `/usr/local/bin` (A1). It only checks the file's
//! executable bit: it never runs the CLI (A2).
//!
//! `disky agents --format json` prints the same records the GUI shows.
//!
//! @implements REQ-DSCR-007

use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// One agent CLI the Assistant can name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentDef {
    /// Store id (`prefs.agent`).
    pub id: &'static str,
    /// Display name, as the GUI shows it.
    pub name: &'static str,
    /// Executable looked up on disk.
    pub bin: &'static str,
}

/// The agents, in display order (A1). `prefs::AGENT_IDS` lists the same ids.
pub const AGENTS: &[AgentDef] = &[
    AgentDef {
        id: "claude",
        name: "Claude Code",
        bin: "claude",
    },
    AgentDef {
        id: "codex",
        name: "Codex CLI",
        bin: "codex",
    },
    AgentDef {
        id: "gemini",
        name: "Gemini CLI",
        bin: "gemini",
    },
];

/// Folders searched after `PATH` (A1). `~` expands to the home directory.
pub const EXTRA_DIRS: &[&str] = &["~/.local/bin", "/opt/homebrew/bin", "/usr/local/bin"];

/// The Windows equivalent of [`EXTRA_DIRS`] (REQ-XPLAT-009): npm's global
/// launchers (`%APPDATA%/npm`), then `~/.local/bin`.
pub const WINDOWS_EXTRA_DIRS: &[&str] = &["%APPDATA%/npm", "~/.local/bin"];

/// The file names an agent's CLI may have on `os` (REQ-XPLAT-009): the bare
/// name on Unix; `.exe`, `.cmd` and `.bat` launchers on Windows.
pub fn candidates(bin: &str, os: crate::platform::Os) -> Vec<String> {
    if os == crate::platform::Os::Windows {
        ["exe", "cmd", "bat"]
            .iter()
            .map(|e| format!("{bin}.{e}"))
            .collect()
    } else {
        vec![bin.to_string()]
    }
}

/// One agent and whether it is installed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub found: bool,
    /// Absolute path of the executable found, or null.
    pub path: Option<String>,
    /// `path` with the home directory shown as `~` ("Found · <path>").
    pub display_path: Option<String>,
}

/// The display name for a store id: "Claude Code" for `claude`, …; an id
/// the table doesn't know is shown as itself.
pub fn display_name(id: &str) -> String {
    AGENTS
        .iter()
        .find(|a| a.id == id)
        .map_or_else(|| id.to_string(), |a| a.name.to_string())
}

/// The folders searched, in order: each `PATH` entry, then [`EXTRA_DIRS`]
/// (`~` against `home`; skipped with no home). Duplicates are dropped.
pub fn search_dirs(path_var: Option<&OsStr>, home: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = path_var
        .map(|p| std::env::split_paths(p).collect())
        .unwrap_or_default();
    let extras = if cfg!(windows) {
        WINDOWS_EXTRA_DIRS
    } else {
        EXTRA_DIRS
    };
    for extra in extras {
        if let Some(rest) = extra.strip_prefix("~/") {
            if let Some(h) = home {
                dirs.push(h.join(rest));
            }
        } else if let Some(rest) = extra.strip_prefix("%APPDATA%/") {
            if let Some(a) = std::env::var_os("APPDATA").filter(|v| !v.is_empty()) {
                dirs.push(PathBuf::from(a).join(rest));
            }
        } else {
            dirs.push(PathBuf::from(extra));
        }
    }
    let mut seen = std::collections::HashSet::new();
    dirs.retain(|d| !d.as_os_str().is_empty() && seen.insert(d.clone()));
    dirs
}

/// True for a regular file (symlinks followed) with any executable bit set.
/// Reads metadata only: the file is never opened or run (A2).
fn is_executable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.is_file() && meta.permissions().mode() & 0o111 != 0
    }
    // No executable bit: the launcher's extension (`.exe`, `.cmd`) is what
    // makes it runnable (REQ-XPLAT-009).
    #[cfg(not(unix))]
    {
        meta.is_file()
    }
}

/// Each agent in [`AGENTS`] order, found in the first of `dirs` that holds
/// an executable of its name.
pub fn detect_in(dirs: &[PathBuf], home: Option<&Path>) -> Vec<Agent> {
    detect_for(crate::platform::Os::current(), dirs, home)
}

fn detect_for(os: crate::platform::Os, dirs: &[PathBuf], home: Option<&Path>) -> Vec<Agent> {
    AGENTS
        .iter()
        .map(|a| {
            let names = candidates(a.bin, os);
            let hit = dirs
                .iter()
                .flat_map(|d| names.iter().map(move |n| d.join(n)))
                .find(|p| is_executable(p));
            Agent {
                id: a.id.to_string(),
                name: a.name.to_string(),
                found: hit.is_some(),
                display_path: hit
                    .as_deref()
                    .map(|p| crate::prefs::abbreviate_home(p, home)),
                path: hit.map(|p| crate::platform::to_slash(&p)),
            }
        })
        .collect()
}

/// [`detect_in`] over this process's `PATH` and `HOME`.
pub fn detect() -> Vec<Agent> {
    let home = crate::platform::home_dir();
    let path = std::env::var_os("PATH");
    detect_in(
        &search_dirs(path.as_deref(), home.as_deref()),
        home.as_deref(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[cfg(unix)]
    fn write_exe(dir: &Path, name: &str, mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        fs::create_dir_all(dir).unwrap();
        let p = dir.join(name);
        // A script that would leave a marker if anything ever ran it.
        let marker = dir.join(format!("{name}.ran"));
        fs::write(&p, format!("#!/bin/sh\ntouch '{}'\n", marker.display())).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(mode)).unwrap();
    }

    #[test]
    fn agents_are_listed_in_order_with_display_names() {
        let ids: Vec<&str> = AGENTS.iter().map(|a| a.id).collect();
        assert_eq!(ids, ["claude", "codex", "gemini"]);
        assert_eq!(ids, crate::prefs::AGENT_IDS);
        assert_eq!(display_name("claude"), "Claude Code");
        assert_eq!(display_name("codex"), "Codex CLI");
        assert_eq!(display_name("gemini"), "Gemini CLI");
        assert_eq!(display_name("other"), "other");
    }

    #[test]
    fn windows_candidates_include_cmd_launchers() {
        use crate::platform::Os;
        assert_eq!(
            candidates("claude", Os::Windows),
            ["claude.exe", "claude.cmd", "claude.bat"]
        );
        assert_eq!(candidates("claude", Os::Linux), ["claude"]);
        // npm's global launcher folder holds `claude.cmd` (A1).
        let tmp = tempfile::tempdir().unwrap();
        let npm = tmp.path().join("AppData/Roaming/npm");
        fs::create_dir_all(&npm).unwrap();
        let cmd = npm.join("claude.cmd");
        fs::write(&cmd, "@echo off\r\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&cmd, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let got = detect_for(Os::Windows, &[npm], None);
        assert!(got[0].found);
        assert_eq!(
            got[0].path.as_deref(),
            Some(crate::platform::to_slash(&cmd).as_str())
        );
        assert!(!got[1].found && !got[2].found);
    }

    #[cfg(unix)]
    #[test]
    fn search_covers_path_then_the_extra_folders() {
        let home = Path::new("/Users/me");
        let path = std::env::join_paths(["/a/bin", "/usr/local/bin", "/b/bin"]).unwrap();
        let dirs = search_dirs(Some(&path), Some(home));
        let shown: Vec<String> = dirs.iter().map(|d| d.display().to_string()).collect();
        assert_eq!(
            shown,
            [
                "/a/bin",
                "/usr/local/bin",
                "/b/bin",
                "/Users/me/.local/bin",
                "/opt/homebrew/bin"
            ]
        );
        // No home: the ~ folder is skipped, the others stay.
        let dirs = search_dirs(None, None);
        assert_eq!(
            dirs,
            [
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/usr/local/bin")
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn detect_finds_executables_and_never_runs_them() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let local = home.join(".local/bin");
        let brew = tmp.path().join("brew");
        write_exe(&local, "claude", 0o755);
        write_exe(&brew, "codex", 0o755);
        // Not executable: present but not found.
        write_exe(&brew, "gemini", 0o644);
        let dirs = vec![brew.clone(), local.clone()];
        let got = detect_in(&dirs, Some(&home));
        assert_eq!(got.len(), 3);
        assert_eq!(got[0].id, "claude");
        assert!(got[0].found);
        assert_eq!(
            got[0].path.as_deref(),
            Some(local.join("claude").to_str().unwrap())
        );
        assert_eq!(got[0].display_path.as_deref(), Some("~/.local/bin/claude"));
        assert!(got[1].found);
        assert_eq!(
            got[1].path.as_deref(),
            Some(brew.join("codex").to_str().unwrap())
        );
        assert!(!got[2].found);
        assert_eq!(got[2].path, None);
        assert_eq!(got[2].display_path, None);
        // A2: detection only read metadata; no script ran.
        for d in [&local, &brew] {
            for e in fs::read_dir(d).unwrap() {
                let name = e.unwrap().file_name();
                assert!(!name.to_string_lossy().ends_with(".ran"), "{name:?} ran");
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn the_first_folder_holding_the_cli_wins_and_folders_are_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let first = tmp.path().join("first");
        let second = tmp.path().join("second");
        write_exe(&second, "claude", 0o755);
        // A directory named like the CLI is not an executable file.
        fs::create_dir_all(first.join("claude")).unwrap();
        let got = detect_in(&[first, second.clone()], None);
        assert_eq!(
            got[0].path.as_deref(),
            Some(second.join("claude").to_str().unwrap())
        );
    }

    #[test]
    fn json_shape_is_snake_case() {
        let a = Agent {
            id: "claude".into(),
            name: "Claude Code".into(),
            found: false,
            path: None,
            display_path: None,
        };
        let v = serde_json::to_value(&a).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"id":"claude","name":"Claude Code","found":false,"path":null,"display_path":null})
        );
    }
}
