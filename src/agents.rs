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
    for extra in EXTRA_DIRS {
        match extra.strip_prefix("~/") {
            Some(rest) => {
                if let Some(h) = home {
                    dirs.push(h.join(rest));
                }
            }
            None => dirs.push(PathBuf::from(extra)),
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
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Each agent in [`AGENTS`] order, found in the first of `dirs` that holds
/// an executable of its name.
pub fn detect_in(dirs: &[PathBuf], home: Option<&Path>) -> Vec<Agent> {
    AGENTS
        .iter()
        .map(|a| {
            let hit = dirs
                .iter()
                .map(|d| d.join(a.bin))
                .find(|p| is_executable(p));
            Agent {
                id: a.id.to_string(),
                name: a.name.to_string(),
                found: hit.is_some(),
                display_path: hit
                    .as_deref()
                    .map(|p| crate::prefs::abbreviate_home(p, home)),
                path: hit.map(|p| p.to_string_lossy().into_owned()),
            }
        })
        .collect()
}

/// [`detect_in`] over this process's `PATH` and `HOME`.
pub fn detect() -> Vec<Agent> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
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
