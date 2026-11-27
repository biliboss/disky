//! Find — and optionally remove — disk-hoggy paths discovered in a snapshot.
//!
//! Two kinds of rule live in one table, [`CATEGORIES`]:
//!
//! * **Folder-name rules** (`node_modules`, `target`, `__pycache__`, …) match
//!   any directory with that basename anywhere in the snapshot. They are
//!   filtered through [`ALWAYS_SKIP_SUBSTRINGS`].
//! * **Location rules** (system / app caches, temp, logs, the Trash, app
//!   leftovers, old downloads) match the direct children of fixed macOS
//!   locations. They bypass `ALWAYS_SKIP_SUBSTRINGS` (which deliberately
//!   blocks `/Library/Caches/` and `/Library/Application Support/` for
//!   folder-name rules) and carry their own per-category exclusions instead.
//!
//! Every hit carries a [`Safety`] class: `safe` (regenerable, nothing of
//! value is lost) or `review` (look before you delete).
//!
//! @implements REQ-DCLEAN-033

use anyhow::Result;
use duckdb::Connection;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// How confident the engine is that a hit can be removed without loss.
/// Deserialisation is strict: any value other than `safe` / `review` is a
/// contract violation and is rejected (REQ-DCLEAN-027.A2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Safety {
    /// Regenerable data (caches, scratch files). Removing it loses nothing.
    Safe,
    /// May hold something the user wants. Show it, but let them decide.
    Review,
}

impl Safety {
    pub fn as_str(self) -> &'static str {
        match self {
            Safety::Safe => "safe",
            Safety::Review => "review",
        }
    }
}

/// One location a [`Matcher::Location`] rule looks under. `pattern` is an
/// absolute path, or starts with `~/` (expanded once at runtime). A `*`
/// segment matches exactly one path segment. Each direct child of the
/// location becomes one hit.
#[derive(Debug, Clone, Copy)]
pub struct Location {
    pub pattern: &'static str,
    pub safety: Safety,
}

/// Extra predicate a location rule applies to its children.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocationFilter {
    /// Every child is a hit.
    All,
    /// Only children whose newest mtime (the child itself and everything
    /// under it) is older than N days.
    OlderThanDays(u32),
    /// Only children that match no installed app by bundle id or by name.
    AppLeftovers,
}

#[derive(Debug, Clone, Copy)]
pub enum Matcher {
    /// Any directory whose basename is one of these.
    Basename(&'static [&'static str]),
    /// The direct children of each location.
    Location {
        locations: &'static [Location],
        filter: LocationFilter,
        /// Child basename prefixes never reported for this category.
        skip_children: &'static [&'static str],
    },
}

/// One cleanup category. `safety` is the category's class; for location
/// rules it is the most cautious class among its locations (a location may
/// be more cautious than its sibling, e.g. `temp`).
#[derive(Debug, Clone, Copy)]
pub struct Category {
    /// Stable slug: the `--target` name, and `id` in the GUI contract.
    pub name: &'static str,
    /// Display name shown to the user (`name` in the GUI contract).
    pub label: &'static str,
    /// One-line explanation of what the category holds.
    pub description: &'static str,
    /// Lucide icon name the GUI draws for the category (e.g. `"app-window"`).
    /// The GUI resolves it by name, never by category id (REQ-DCLEAN-005).
    pub icon: &'static str,
    pub safety: Safety,
    pub matcher: Matcher,
    /// Caveat copied onto every hit of this category.
    pub note: Option<&'static str>,
}

impl Category {
    pub fn is_basename(&self) -> bool {
        matches!(self.matcher, Matcher::Basename(_))
    }
}

/// Name of the category that holds what is already in `~/.Trash`.
pub const TRASH_CATEGORY: &str = "trash";

/// Age after which a top-level item in `~/Downloads` counts as old.
pub const OLD_DOWNLOADS_DAYS: u32 = 90;

const TRASH_NOTE: &str = "Already in the Trash: cleaning deletes these permanently and cannot be \
     undone. Refused with --reversible; run --apply without --reversible to delete.";

/// Children of macOS-owned support folders that are never app leftovers.
const LEFTOVER_SKIP: &[&str] = &[
    "com.apple.",
    "Apple",
    "disky",
    "AddressBook",
    "CallHistoryDB",
    "CallHistoryTransactions",
    "CloudDocs",
    "CrashReporter",
    "DiskImages",
    "Dock",
    "FileProvider",
    "iCloud",
    "icdd",
    "Knowledge",
    "MobileSync",
    "networkserviceproxy",
    "SyncServices",
    "accountsd",
    "AppleUserHIDDrivers",
    "Animoji",
    "AskPermission",
    "ControlCenter",
    "default.store",
    "homeenergyd",
    "identityservicesd",
    "locationaccessstored",
    "ScreenTimeAgent",
    "Spotlight",
];

/// One location category's folders on one OS.
#[derive(Debug, Clone, Copy)]
pub struct LocationSet {
    pub locations: &'static [Location],
    /// Child basename prefixes never reported.
    pub skip_children: &'static [&'static str],
    /// The most cautious class among `locations`.
    pub safety: Safety,
}

impl LocationSet {
    const fn matcher(self, filter: LocationFilter) -> Matcher {
        Matcher::Location {
            locations: self.locations,
            filter,
            skip_children: self.skip_children,
        }
    }
}

/// The folders each location category targets on one OS (REQ-XPLAT-005).
/// The category names, labels and icons are the same everywhere; only the
/// folders differ.
#[derive(Debug, Clone, Copy)]
pub struct OsLocations {
    pub system_caches: LocationSet,
    pub app_caches: LocationSet,
    pub temp: LocationSet,
    pub logs: LocationSet,
    pub trash: LocationSet,
    pub app_leftovers: LocationSet,
}

const fn loc(pattern: &'static str, safety: Safety) -> Location {
    Location { pattern, safety }
}

const NONE: LocationSet = LocationSet {
    locations: &[],
    skip_children: &[],
    safety: Safety::Review,
};

/// macOS (the original table).
pub const MACOS_LOCATIONS: OsLocations = OsLocations {
    system_caches: LocationSet {
        locations: &[loc("/Library/Caches", Safety::Safe)],
        // Apple's own system caches are SIP-managed or rebuilt by daemons
        // that misbehave while they are missing.
        skip_children: &["com.apple."],
        safety: Safety::Safe,
    },
    app_caches: LocationSet {
        locations: &[loc("~/Library/Caches", Safety::Safe)],
        // iCloud sync state lives here; deleting it forces a full resync.
        skip_children: &[
            "CloudKit",
            "com.apple.bird",
            "com.apple.cloudd",
            "com.apple.nsurlsessiond",
            "FamilyCircle",
            "disky",
        ],
        safety: Safety::Safe,
    },
    temp: LocationSet {
        locations: &[
            loc("/private/tmp", Safety::Safe),
            // Per-user temp dirs hold live files of running apps.
            loc("/private/var/folders/*/*/T", Safety::Review),
        ],
        // launchd sockets and Finder's in-flight copies.
        skip_children: &["com.apple.", "TemporaryItems"],
        safety: Safety::Review,
    },
    logs: LocationSet {
        locations: &[
            loc("~/Library/Logs", Safety::Review),
            loc("/Library/Logs", Safety::Review),
        ],
        skip_children: &[],
        safety: Safety::Review,
    },
    trash: LocationSet {
        locations: &[loc("~/.Trash", Safety::Safe)],
        skip_children: &[],
        safety: Safety::Safe,
    },
    app_leftovers: LocationSet {
        locations: &[
            // Keyed by app name.
            loc("~/Library/Application Support", Safety::Review),
            // Keyed by bundle id.
            loc("~/Library/Containers", Safety::Review),
        ],
        skip_children: LEFTOVER_SKIP,
        safety: Safety::Review,
    },
};

/// Linux (freedesktop / FHS folders).
pub const LINUX_LOCATIONS: OsLocations = OsLocations {
    system_caches: LocationSet {
        locations: &[loc("/var/cache", Safety::Review)],
        // debconf holds the package configuration database, not a cache;
        // ldconfig's cache is rebuilt only by root.
        skip_children: &["debconf", "ldconfig"],
        safety: Safety::Review,
    },
    app_caches: LocationSet {
        locations: &[loc("~/.cache", Safety::Safe)],
        skip_children: &["disky"],
        safety: Safety::Safe,
    },
    temp: LocationSet {
        // Both hold live files of running programs.
        locations: &[loc("/tmp", Safety::Review), loc("/var/tmp", Safety::Review)],
        // Live sockets and per-service private tmp (dot-names, such as
        // `.X11-unix`, are never offered).
        skip_children: &[
            "systemd-private-",
            "ssh-",
            "tmux-",
            "snap-private-tmp",
            "pulse-",
        ],
        safety: Safety::Review,
    },
    logs: LocationSet {
        locations: &[loc("/var/log", Safety::Review)],
        skip_children: &[],
        safety: Safety::Review,
    },
    trash: LocationSet {
        locations: &[loc("~/.local/share/Trash/files", Safety::Safe)],
        skip_children: &[],
        safety: Safety::Safe,
    },
    // No installed-app list: when in doubt, don't report (REQ-XPLAT-005.A3).
    app_leftovers: NONE,
};

/// Windows. `%SystemDrive%` expands to the system drive (`C:`).
pub const WINDOWS_LOCATIONS: OsLocations = OsLocations {
    system_caches: LocationSet {
        // Windows Update's download cache; it re-downloads what it needs.
        locations: &[loc(
            "%SystemDrive%/Windows/SoftwareDistribution/Download",
            Safety::Review,
        )],
        skip_children: &[],
        safety: Safety::Review,
    },
    app_caches: LocationSet {
        locations: &[loc(
            "~/AppData/Local/Microsoft/Windows/INetCache",
            Safety::Safe,
        )],
        skip_children: &["desktop.ini"],
        safety: Safety::Safe,
    },
    temp: LocationSet {
        locations: &[
            loc("~/AppData/Local/Temp", Safety::Review),
            loc("%SystemDrive%/Windows/Temp", Safety::Review),
        ],
        skip_children: &[],
        safety: Safety::Review,
    },
    logs: LocationSet {
        locations: &[loc("%SystemDrive%/Windows/Logs", Safety::Review)],
        skip_children: &[],
        safety: Safety::Review,
    },
    trash: LocationSet {
        // Each user's bin is a SID folder; `$I` records go with their `$R`
        // item, so they are not offered on their own.
        locations: &[loc("%SystemDrive%/$Recycle.Bin/*", Safety::Safe)],
        skip_children: &["$I", "desktop.ini"],
        safety: Safety::Safe,
    },
    app_leftovers: NONE,
};

impl OsLocations {
    /// The folders for location category `name`; `None` for a category
    /// whose folders are the same on every OS (`old_downloads`).
    pub fn set_for(&self, name: &str) -> Option<LocationSet> {
        Some(match name {
            "system_caches" => self.system_caches,
            "app_caches" => self.app_caches,
            "temp" => self.temp,
            "logs" => self.logs,
            TRASH_CATEGORY => self.trash,
            "app_leftovers" => self.app_leftovers,
            _ => return None,
        })
    }
}

impl Default for OsLocations {
    fn default() -> Self {
        OS_LOCATIONS
    }
}

/// This build's table.
pub const OS_LOCATIONS: OsLocations = match crate::platform::Os::current() {
    crate::platform::Os::MacOs => MACOS_LOCATIONS,
    crate::platform::Os::Linux => LINUX_LOCATIONS,
    crate::platform::Os::Windows => WINDOWS_LOCATIONS,
};

/// The single table of cleanup categories (REQ-DCLEAN-033.A2). The GUI and
/// `disky schema` read this; nothing else defines a category.
pub const CATEGORIES: &[Category] = &[
    // ── Developer artefacts (folder-name rules, all `review`) ──────────
    Category {
        name: "node_modules",
        label: "node_modules",
        icon: "code",
        description: "Installed JavaScript packages; npm install restores them",
        safety: Safety::Review,
        matcher: Matcher::Basename(&["node_modules"]),
        note: None,
    },
    Category {
        name: "target",
        label: "Cargo target dirs",
        icon: "code",
        description: "Rust build output; cargo build recreates it",
        safety: Safety::Review,
        matcher: Matcher::Basename(&["target"]),
        note: None,
    },
    Category {
        name: "pycache",
        label: "Python bytecode",
        icon: "code",
        description: "__pycache__ folders; Python regenerates them on import",
        safety: Safety::Review,
        matcher: Matcher::Basename(&["__pycache__"]),
        note: None,
    },
    Category {
        name: "next",
        label: "Next.js build caches",
        icon: "code",
        description: ".next folders; next build recreates them",
        safety: Safety::Review,
        matcher: Matcher::Basename(&[".next"]),
        note: None,
    },
    Category {
        name: "dist",
        label: "dist folders",
        icon: "code",
        description: "Build output; check it is not the only copy of a release",
        safety: Safety::Review,
        matcher: Matcher::Basename(&["dist"]),
        note: None,
    },
    Category {
        name: "build",
        label: "build folders",
        icon: "code",
        description: "Build output; check it is not hand-written source",
        safety: Safety::Review,
        matcher: Matcher::Basename(&["build"]),
        note: None,
    },
    Category {
        name: "venv",
        label: "Python virtualenvs",
        icon: "code",
        description: ".venv / venv folders; reinstall from requirements to restore",
        safety: Safety::Review,
        matcher: Matcher::Basename(&[".venv", "venv"]),
        note: None,
    },
    Category {
        name: "gradle",
        label: "Gradle caches",
        icon: "code",
        description: ".gradle folders; Gradle re-downloads and rebuilds them",
        safety: Safety::Review,
        matcher: Matcher::Basename(&[".gradle"]),
        note: None,
    },
    Category {
        name: "pytest",
        label: "pytest caches",
        icon: "code",
        description: ".pytest_cache folders; pytest recreates them",
        safety: Safety::Review,
        matcher: Matcher::Basename(&[".pytest_cache"]),
        note: None,
    },
    // ── System-level categories (location rules, per OS: REQ-XPLAT-005) ─
    Category {
        name: "system_caches",
        label: "System caches",
        icon: "box",
        description: "Rebuilt automatically when needed",
        safety: OS_LOCATIONS.system_caches.safety,
        matcher: OS_LOCATIONS.system_caches.matcher(LocationFilter::All),
        note: None,
    },
    Category {
        name: "app_caches",
        label: "App caches",
        icon: "app-window",
        description: "Rebuilt by each app the next time it runs",
        safety: OS_LOCATIONS.app_caches.safety,
        matcher: OS_LOCATIONS.app_caches.matcher(LocationFilter::All),
        note: None,
    },
    Category {
        name: "temp",
        label: "Temporary files",
        icon: "clock",
        description: "Left behind by installers and apps",
        safety: OS_LOCATIONS.temp.safety,
        matcher: OS_LOCATIONS.temp.matcher(LocationFilter::All),
        note: None,
    },
    Category {
        name: "logs",
        label: "Logs & crash reports",
        icon: "file-text",
        description: "App and system logs; kept only for troubleshooting",
        safety: OS_LOCATIONS.logs.safety,
        matcher: OS_LOCATIONS.logs.matcher(LocationFilter::All),
        note: None,
    },
    Category {
        name: TRASH_CATEGORY,
        label: "Trash",
        icon: "trash",
        description: "Items already in the Trash; cleaning empties them permanently",
        safety: OS_LOCATIONS.trash.safety,
        matcher: OS_LOCATIONS.trash.matcher(LocationFilter::All),
        note: Some(TRASH_NOTE),
    },
    Category {
        name: "app_leftovers",
        label: "App leftovers",
        icon: "package",
        description: "Support files of apps that are no longer installed",
        safety: OS_LOCATIONS.app_leftovers.safety,
        matcher: OS_LOCATIONS
            .app_leftovers
            .matcher(LocationFilter::AppLeftovers),
        note: None,
    },
    Category {
        name: "old_downloads",
        label: "Old downloads",
        icon: "download",
        description: "Downloads untouched for more than 90 days",
        safety: Safety::Review,
        matcher: Matcher::Location {
            locations: &[Location {
                pattern: "~/Downloads",
                safety: Safety::Review,
            }],
            filter: LocationFilter::OlderThanDays(OLD_DOWNLOADS_DAYS),
            skip_children: &[],
        },
        note: None,
    },
];

/// Hardcoded safety: paths matching ANY of these substrings are NEVER
/// returned by a **folder-name** rule, regardless of how their basename
/// matched. Location rules do not consult this list (it blocks the very
/// folders they target); they carry their own `skip_children`.
///
/// Why: `cleanup --target target` once matched
/// `~/.cargo/registry/src/index.crates.io-*/cc-1.2.62/src/target/` —
/// the `cc` Rust crate's source module dir, NOT a cargo build output.
/// Running that with `--apply` broke the host's build. This list is
/// the floor; `.diskyignore` extends it.
pub const ALWAYS_SKIP_SUBSTRINGS: &[&str] = &[
    // Rust / Cargo
    "/.cargo/registry/",
    "/.cargo/git/",
    "/.rustup/",
    // Node / npm / pnpm
    "/.npm/",
    "/.pnpm-store/",
    "/.yarn/cache/",
    // Python / pip
    "/site-packages/",
    "/.cache/pip/",
    "/.cache/uv/",
    // OS caches
    "/Library/Caches/",
    "/Library/Group Containers/",
    "/Library/Application Support/",
    // Plugin / extension sources
    "/.vscode/extensions/",
    "/.windsurf/extensions/",
    "/.antigravity/",
];

#[inline]
fn is_protected(path: &str) -> bool {
    ALWAYS_SKIP_SUBSTRINGS.iter().any(|s| path.contains(s))
}

/// Every category name, in table order. Used when the caller passes no
/// `--target` for a dry-run.
pub fn category_names() -> Vec<&'static str> {
    CATEGORIES.iter().map(|c| c.name).collect()
}

/// Alias of [`category_names`], kept for existing callers.
pub fn default_target_names() -> Vec<&'static str> {
    category_names()
}

/// The developer-artefact (folder-name) categories only. `--apply` without
/// an explicit `--target` is limited to these, so a bare `--apply` never
/// reaches the system-level categories.
pub fn dev_category_names() -> Vec<&'static str> {
    CATEGORIES
        .iter()
        .filter(|c| c.is_basename())
        .map(|c| c.name)
        .collect()
}

pub fn category(name: &str) -> Option<&'static Category> {
    CATEGORIES.iter().find(|c| c.name == name)
}

#[derive(Debug, Clone, Serialize)]
pub struct CleanupHit {
    pub category: String,
    pub safety: Safety,
    pub path: String,
    /// Logical bytes (sum of `st_size`).
    pub bytes: u64,
    /// Physical bytes on disk (sum of `st_blocks * 512`, falling back to the
    /// logical size where a snapshot lacks it). What removing the hit frees.
    pub physical_bytes: u64,
    pub files: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Newest mtime (unix seconds) of the hit and everything beneath it, when
    /// known. Engine-internal: the Assistant words "since <month>" from it
    /// (REQ-DSCR-008.A2). Not part of the `cleanup` envelope.
    #[serde(skip)]
    pub newest_mtime: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CategorySummary {
    pub category: String,
    /// `review` if any hit in the category is `review`.
    pub safety: Safety,
    pub paths: u64,
    pub bytes: u64,
    pub physical_bytes: u64,
    pub files: u64,
}

/// Aggregate `CleanupHit`s by category. Sorted by total `bytes` desc so the
/// biggest reclamation opportunity is first.
pub fn summarise(hits: &[CleanupHit]) -> Vec<CategorySummary> {
    use std::collections::BTreeMap;
    let mut acc: BTreeMap<String, CategorySummary> = BTreeMap::new();
    for h in hits {
        let entry = acc
            .entry(h.category.clone())
            .or_insert_with(|| CategorySummary {
                category: h.category.clone(),
                safety: h.safety,
                paths: 0,
                bytes: 0,
                physical_bytes: 0,
                files: 0,
            });
        if h.safety == Safety::Review {
            entry.safety = Safety::Review;
        }
        entry.paths += 1;
        entry.bytes += h.bytes;
        entry.physical_bytes += h.physical_bytes;
        entry.files += h.files;
    }
    let mut out: Vec<CategorySummary> = acc.into_values().collect();
    out.sort_by_key(|s| std::cmp::Reverse(s.bytes));
    out
}

/// Resolve target names to categories, dropping unknown names and repeats.
fn categories_for(targets: &[String]) -> Vec<&'static Category> {
    let mut out: Vec<&'static Category> = Vec::new();
    for t in targets {
        if let Some(c) = category(t) {
            if !out.iter().any(|o| o.name == c.name) {
                out.push(c);
            }
        }
    }
    out
}

fn basenames_for(targets: &[String]) -> Vec<(&'static str, &'static str)> {
    let mut out = Vec::new();
    for c in categories_for(targets) {
        if let Matcher::Basename(basenames) = c.matcher {
            for b in basenames {
                out.push((c.name, *b));
            }
        }
    }
    out
}

/// True when `path` may be returned as a hit (REQ-DCLEAN-033.A3): it lies
/// inside `scan_root`, under none of the user-`excluded` folders, and no
/// segment below the root matches a `.diskyignore` pattern.
pub fn in_scope(
    path: &str,
    scan_root: Option<&str>,
    ignore_patterns: &[String],
    excluded: &[String],
) -> bool {
    let rel = match scan_root {
        Some(root) => {
            let root = root.trim_end_matches('/');
            if root.is_empty() {
                path
            } else if path == root {
                ""
            } else if let Some(rest) = path.strip_prefix(root).filter(|r| r.starts_with('/')) {
                rest
            } else {
                return false;
            }
        }
        None => path,
    };
    for e in excluded {
        let e = e.trim_end_matches('/');
        if e.is_empty() {
            return false; // excluding `/` excludes everything
        }
        if path == e || path.strip_prefix(e).is_some_and(|r| r.starts_with('/')) {
            return false;
        }
    }
    ignore_patterns.is_empty()
        || !rel
            .split('/')
            .filter(|s| !s.is_empty())
            .any(|seg| crate::ignore::should_skip(seg, ignore_patterns))
}

/// Apps installed on the machine, reduced to the lowercase names and bundle
/// ids a support folder could be keyed by.
#[derive(Debug, Clone, Default)]
pub struct InstalledApps {
    bundle_ids: HashSet<String>,
    names: HashSet<String>,
    tokens: HashSet<String>,
}

/// Words too generic to tie a folder to an app.
const GENERIC_TOKENS: &[&str] = &[
    "com", "org", "net", "io", "app", "apps", "dev", "co", "inc", "mac", "macos", "osx", "the",
    "helper", "desktop", "client", "support", "data", "plugin", "plugins",
];

fn words(s: &str) -> impl Iterator<Item = &str> {
    s.split(['.', ' ', '-', '_'])
        .filter(|w| w.len() >= 3)
        .filter(|w| !GENERIC_TOKENS.contains(&w.to_ascii_lowercase().as_str()))
}

impl InstalledApps {
    /// Build from (bundle id, names) pairs. Exposed for tests and callers
    /// that already know the app list.
    pub fn from_apps<I, S>(apps: I) -> Self
    where
        I: IntoIterator<Item = (Option<S>, Vec<S>)>,
        S: AsRef<str>,
    {
        let mut out = InstalledApps::default();
        for (bundle_id, names) in apps {
            if let Some(id) = bundle_id {
                out.add_bundle_id(id.as_ref());
            }
            for n in names {
                out.add_name(n.as_ref());
            }
        }
        out
    }

    fn add_bundle_id(&mut self, id: &str) {
        let id = id.trim().to_lowercase();
        if id.is_empty() {
            return;
        }
        for w in words(&id) {
            self.tokens.insert(w.to_string());
        }
        self.bundle_ids.insert(id);
    }

    fn add_name(&mut self, name: &str) {
        let name = name.trim().to_lowercase();
        if name.is_empty() {
            return;
        }
        for w in words(&name) {
            self.tokens.insert(w.to_string());
        }
        self.names.insert(name);
    }

    pub fn is_empty(&self) -> bool {
        self.bundle_ids.is_empty() && self.names.is_empty()
    }

    /// True when `child` (a folder under Application Support / Containers)
    /// plausibly belongs to an installed app. Deliberately generous: a
    /// false "belongs" only hides a leftover, a false "leftover" could
    /// cost the user their app data.
    pub fn owns(&self, child: &str) -> bool {
        let c = child.trim().to_lowercase();
        if self.names.contains(&c) || self.bundle_ids.contains(&c) {
            return true;
        }
        // `com.vendor.App.helper` → is any dotted prefix an installed id?
        let mut prefix = String::new();
        for seg in c.split('.') {
            if !prefix.is_empty() {
                prefix.push('.');
            }
            prefix.push_str(seg);
            if self.bundle_ids.contains(&prefix) {
                return true;
            }
        }
        // Installed id is a prefix of the child, or vice versa.
        if self
            .bundle_ids
            .iter()
            .any(|id| c.starts_with(&format!("{id}.")) || id.starts_with(&format!("{c}.")))
        {
            return true;
        }
        if words(&c).any(|w| self.tokens.contains(w)) {
            return true;
        }
        false
    }

    /// Scan the standard app folders for `*.app` bundles (two levels deep)
    /// and read each `Info.plist`.
    pub fn load(home: Option<&Path>) -> Self {
        let mut roots: Vec<PathBuf> = vec![
            PathBuf::from("/Applications"),
            PathBuf::from("/System/Applications"),
            PathBuf::from("/System/Library/CoreServices"),
        ];
        if let Some(h) = home {
            roots.push(h.join("Applications"));
        }
        let mut out = InstalledApps::default();
        for root in roots {
            out.collect_dir(&root, 0);
        }
        out
    }

    fn collect_dir(&mut self, dir: &Path, depth: u32) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "app") {
                self.collect_app(&p);
            } else if depth < 1 && e.file_type().is_ok_and(|t| t.is_dir()) {
                self.collect_dir(&p, depth + 1);
            }
        }
    }

    fn collect_app(&mut self, app: &Path) {
        if let Some(stem) = app.file_stem() {
            self.add_name(&stem.to_string_lossy());
        }
        let Ok(plist::Value::Dictionary(d)) =
            plist::Value::from_file(app.join("Contents/Info.plist"))
        else {
            return;
        };
        if let Some(id) = d.get("CFBundleIdentifier").and_then(|v| v.as_string()) {
            self.add_bundle_id(id);
        }
        for key in ["CFBundleName", "CFBundleDisplayName", "CFBundleExecutable"] {
            if let Some(n) = d.get(key).and_then(|v| v.as_string()) {
                self.add_name(n);
            }
        }
    }
}

/// Runtime inputs to [`scan_with`]. [`ScanContext::from_snapshot`] fills it
/// from the machine and the snapshot; tests build it by hand.
#[derive(Debug, Clone, Default)]
pub struct ScanContext {
    /// Expansion of `~` in location patterns. `None` → `~/…` rules are off.
    pub home: Option<PathBuf>,
    /// Root of the scan. Hits outside it are dropped.
    pub scan_root: Option<String>,
    /// `.diskyignore` patterns (substring match per path segment).
    pub ignore: Vec<String>,
    /// Absolute folders the user excluded. Hits inside them are dropped
    /// (REQ-DSCR-002.A1).
    pub excluded: Vec<String>,
    /// Skip-recently-used cutoff (unix seconds): a hit whose newest mtime is
    /// at or after it — or unknown — is dropped (REQ-DSCR-002.A2). `None`
    /// keeps recent items.
    pub recent_cutoff: Option<i64>,
    /// "Now" in unix seconds, for age filters.
    pub now_secs: i64,
    /// Installed apps for the leftover check. `None` → read from disk the
    /// first time `app_leftovers` needs it.
    pub installed_apps: Option<InstalledApps>,
    /// The location table to match: this OS's ([`OS_LOCATIONS`]); tests
    /// pick another OS's to check its folders anywhere.
    pub locations: OsLocations,
}

impl ScanContext {
    pub fn from_snapshot(conn: &Connection) -> Self {
        let scan_root = crate::db::read_scan_meta(conn).map(|m| m.root);
        let ignore = scan_root
            .as_deref()
            .map(|r| crate::ignore::load_diskyignore_chain(Path::new(r)))
            .unwrap_or_default();
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        ScanContext {
            home: crate::platform::home_dir(),
            scan_root,
            ignore,
            excluded: Vec::new(),
            recent_cutoff: None,
            now_secs,
            installed_apps: None,
            locations: OS_LOCATIONS,
        }
    }

    /// Apply the user's preferences (REQ-DSCR-002): excluded folders, with
    /// `~` expanded against [`ScanContext::home`], and the skip-recently-used
    /// cutoff. Folders that don't exist simply match nothing (A3).
    ///
    /// @implements REQ-DSCR-002
    pub fn with_prefs(mut self, prefs: &crate::prefs::Prefs) -> Self {
        self.excluded = prefs.expanded_excluded(self.home.as_deref());
        self.recent_cutoff = prefs.recent_cutoff(self.now_secs);
        self
    }

    fn in_scope(&self, path: &str) -> bool {
        in_scope(
            path,
            self.scan_root.as_deref(),
            &self.ignore,
            &self.excluded,
        )
    }

    /// Expand a location pattern; `None` if it needs `~` and home is unknown.
    fn expand(&self, pattern: &str) -> Option<String> {
        if let Some(rest) = pattern.strip_prefix("%SystemDrive%") {
            let root = crate::platform::default_root();
            return Some(format!("{}{rest}", root.trim_end_matches('/')));
        }
        match pattern.strip_prefix("~/") {
            Some(rest) => {
                let home = crate::platform::to_slash(self.home.as_ref()?);
                Some(format!("{}/{}", home.trim_end_matches('/'), rest))
            }
            None => Some(pattern.to_string()),
        }
    }
}

/// A directory (or file) that becomes one hit if it survives the join.
struct Root {
    path: String,
    category: &'static Category,
    safety: Safety,
    self_mtime: Option<i64>,
    /// Found by a location rule (a direct child of a known folder) rather
    /// than by a directory basename; decides how its bytes are summed.
    by_location: bool,
}

/// True when `path` is a direct child of a location `pattern` (segments of
/// `*` match any one segment).
fn is_child_of(pattern: &str, path: &str) -> bool {
    let pat: Vec<&str> = pattern.split('/').collect();
    let segs: Vec<&str> = path.split('/').collect();
    segs.len() == pat.len() + 1
        && segs.last().is_some_and(|s| !s.is_empty())
        && pat
            .iter()
            .zip(&segs)
            .all(|(p, s)| if *p == "*" { !s.is_empty() } else { p == s })
}

/// Turn every requested location rule into one root per direct child of the
/// location, using the rows already in `files`. A location with no rows
/// (missing on this machine, or outside the scan) yields nothing
/// (REQ-DCLEAN-033.A5). All locations are resolved in one query.
fn location_roots(
    conn: &Connection,
    cats: &[&'static Category],
    ctx: &ScanContext,
) -> Result<Vec<Root>> {
    // (category, location safety, expanded pattern)
    let mut locs: Vec<(&'static Category, Safety, String)> = Vec::new();
    for c in cats {
        if let Matcher::Location { locations, .. } = c.matcher {
            let locations = ctx
                .locations
                .set_for(c.name)
                .map_or(locations, |s| s.locations);
            for l in locations {
                if let Some(p) = ctx.expand(l.pattern) {
                    locs.push((c, l.safety, p.trim_end_matches('/').to_string()));
                }
            }
        }
    }
    if locs.is_empty() {
        return Ok(vec![]);
    }

    // One predicate per location: a path range on the literal prefix (zone
    // maps prune it) plus the slash count of a direct child.
    let mut clauses: Vec<String> = Vec::new();
    let mut params: Vec<duckdb::types::Value> = Vec::new();
    for (_, _, pattern) in &locs {
        let literal = match pattern.find("/*") {
            Some(i) => &pattern[..i],
            None => pattern.as_str(),
        };
        let slashes = pattern.matches('/').count() as i64 + 1;
        clauses.push(
            "(path >= ? AND path < ? AND length(path) - length(replace(path, '/', '')) = ?)".into(),
        );
        params.push(duckdb::types::Value::Text(format!("{literal}/")));
        params.push(duckdb::types::Value::Text(format!("{literal}0")));
        params.push(duckdb::types::Value::BigInt(slashes));
    }
    let sql = format!(
        "SELECT path, name, is_dir, mtime FROM files WHERE {}",
        clauses.join(" OR ")
    );
    let refs: Vec<&dyn duckdb::ToSql> = params.iter().map(|v| v as &dyn duckdb::ToSql).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows: Vec<(String, String, bool, Option<i64>)> = stmt
        .query_map(refs.as_slice(), |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?
        .flatten()
        .collect();
    drop(stmt);

    let mut apps: Option<InstalledApps> = ctx.installed_apps.clone();
    let mut out = Vec::new();
    for (path, name, _is_dir, mtime) in rows {
        let Some((cat, safety)) = locs
            .iter()
            .find(|(_, _, p)| is_child_of(p, &path))
            .map(|(c, s, _)| (*c, *s))
        else {
            continue;
        };
        let Matcher::Location {
            filter,
            skip_children,
            ..
        } = cat.matcher
        else {
            continue;
        };
        let skip_children = ctx
            .locations
            .set_for(cat.name)
            .map_or(skip_children, |s| s.skip_children);
        // Hidden entries (`.DS_Store`, `.localized`) are never reported.
        if name.starts_with('.') || skip_children.iter().any(|s| name.starts_with(s)) {
            continue;
        }
        if !ctx.in_scope(&path) {
            continue;
        }
        if filter == LocationFilter::AppLeftovers {
            let apps = apps.get_or_insert_with(|| InstalledApps::load(ctx.home.as_deref()));
            // No app list (not macOS, unreadable /Applications) → no basis
            // for calling anything a leftover.
            if apps.is_empty() || apps.owns(&name) {
                continue;
            }
        }
        out.push(Root {
            path,
            category: cat,
            safety,
            self_mtime: mtime,
            by_location: true,
        });
    }
    Ok(out)
}

/// Roots from folder-name rules, filtered by `ALWAYS_SKIP_SUBSTRINGS`.
fn basename_roots(
    conn: &Connection,
    cats: &[&'static Category],
    ctx: &ScanContext,
) -> Result<Vec<Root>> {
    let names: Vec<String> = cats.iter().map(|c| c.name.to_string()).collect();
    let by_basename: HashMap<&'static str, &'static Category> = basenames_for(&names)
        .into_iter()
        .filter_map(|(cat, base)| category(cat).map(|c| (base, c)))
        .collect();
    if by_basename.is_empty() {
        return Ok(vec![]);
    }
    let basenames: Vec<&str> = by_basename.keys().copied().collect();
    let placeholders = vec!["?"; basenames.len()].join(",");
    let sql =
        format!("SELECT path, name, mtime FROM files WHERE is_dir AND name IN ({placeholders})");
    let params: Vec<duckdb::types::Value> = basenames
        .iter()
        .map(|b| duckdb::types::Value::Text((*b).to_string()))
        .collect();
    let refs: Vec<&dyn duckdb::ToSql> = params.iter().map(|v| v as &dyn duckdb::ToSql).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(refs.as_slice(), |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<i64>>(2)?,
        ))
    })?;
    let mut out = Vec::new();
    for (path, name, mtime) in rows.flatten() {
        // Drop protected paths (cargo registry, node module sources, OS
        // caches, etc.). See ALWAYS_SKIP_SUBSTRINGS for the rationale.
        if is_protected(&path) || !ctx.in_scope(&path) {
            continue;
        }
        let Some(&cat) = by_basename.get(name.as_str()) else {
            continue;
        };
        out.push(Root {
            path,
            category: cat,
            safety: cat.safety,
            self_mtime: mtime,
            by_location: false,
        });
    }
    Ok(out)
}

/// Find cleanup hits for `targets`, reading the scan root, `.diskyignore`
/// and home directory from the snapshot and the machine.
pub fn scan(conn: &Connection, targets: &[String], limit: usize) -> Result<Vec<CleanupHit>> {
    let ctx = ScanContext::from_snapshot(conn);
    scan_with(conn, targets, limit, &ctx)
}

/// [`scan`] with explicit runtime inputs.
pub fn scan_with(
    conn: &Connection,
    targets: &[String],
    limit: usize,
    ctx: &ScanContext,
) -> Result<Vec<CleanupHit>> {
    let cats = categories_for(targets);
    if cats.is_empty() {
        return Ok(vec![]);
    }

    let mut roots = basename_roots(conn, &cats, ctx)?;
    roots.extend(location_roots(conn, &cats, ctx)?);
    // A path matched by two rules is reported once (first rule wins).
    let mut seen: HashSet<String> = HashSet::new();
    roots.retain(|r| seen.insert(r.path.clone()));
    // Outermost wins before the join, not only after it: a home with
    // thousands of `node_modules` nested inside other `node_modules` (31 k
    // dirs on a 10.9 M-row snapshot) otherwise joins every one of them, and
    // the view takes seconds. A nested root's bytes are inside its
    // ancestor's, and a nested root under a recently used one is never
    // reported on its own (deleting it would break a project in use).
    let roots = outermost(roots, |r| r.path.as_str());
    if roots.is_empty() {
        return Ok(vec![]);
    }

    // History (kept for context):
    //   v1: `LEFT JOIN files f ON f.path LIKE (t.path || '/%')` — 79 s on a
    //       1.77 M-row snapshot (nested-loop join, per-row LIKE).
    //   v2: one aggregation query per target with a path-range predicate —
    //       79–250 s on homes with thousands of targets (dispatch overhead).
    //   v3/v4: one grouped range-join against a TEMP table of targets
    //       (IEJoin). < 1 s on 1 M rows, but the IEJoin sorts every path:
    //       8–17 s on a real 10.9 M-row home, which made the GUI's review
    //       look dead.
    //   v5 (this): no join and no sort. Each file is keyed by the root it
    //       belongs to, computed from its own path, and summed with a hash
    //       GROUP BY — one linear pass for the basename rules (the outermost
    //       `/<basename>/` segment, by regex) and one range scan per
    //       location parent (the child name after `<parent>/`). Roots look
    //       their sums up by path.
    let sums = root_sums(conn, &roots)?;

    let mut out: Vec<CleanupHit> = Vec::with_capacity(roots.len());
    for r in &roots {
        let (bytes, files, newest, physical) = sums.get(&r.path).copied().unwrap_or_default();
        // Newest mtime of the root and everything beneath it.
        let newest = match (newest, r.self_mtime) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
        if let Matcher::Location {
            filter: LocationFilter::OlderThanDays(days),
            ..
        } = r.category.matcher
        {
            let cutoff = ctx.now_secs - i64::from(days) * 86_400;
            // Unknown age → not provably old → not reported.
            if !matches!(newest, Some(m) if m < cutoff) {
                continue;
            }
        }
        // Skip recently used (REQ-DSCR-002.A2): a review-category hit touched
        // since the cutoff stays. Unknown age → not provably unused → not
        // reported. Safe categories (caches, temp, logs, Trash) are rebuilt
        // automatically and are touched constantly, so they are exempt —
        // otherwise the default would hide nearly all of them.
        if let (Some(cutoff), Safety::Review) = (ctx.recent_cutoff, r.safety) {
            if !matches!(newest, Some(m) if m < cutoff) {
                continue;
            }
        }
        out.push(CleanupHit {
            category: r.category.name.to_string(),
            safety: r.safety,
            path: r.path.clone(),
            bytes: bytes as u64,
            physical_bytes: physical as u64,
            files: files as u64,
            note: r.category.note.map(String::from),
            newest_mtime: newest,
        });
    }
    let mut out = drop_nested(out);
    out.sort_by_key(|h| std::cmp::Reverse(h.bytes));
    out.truncate(limit);
    Ok(out)
}

/// `(logical bytes, files, newest mtime, physical bytes)` of the regular
/// files beneath a root (or of the root itself, for a file root).
type Sum = (i64, i64, Option<i64>, i64);

/// Sum every root's files in linear passes (no join, no sort).
///
/// Basename roots: a file belongs to the outermost `/<basename>/` segment of
/// its path (roots are outermost-pruned, and a filter that drops an outer
/// match drops everything beneath it too, so the outermost match is the
/// root). Location roots: a file under `<parent>/` belongs to
/// `<parent>/<first segment>`; a file root keys to itself.
fn root_sums(conn: &Connection, roots: &[Root]) -> Result<HashMap<String, Sum>> {
    let mut sums: HashMap<String, Sum> = HashMap::new();
    let mut collect = |sql: &str, params: Vec<duckdb::types::Value>| -> Result<()> {
        let refs: Vec<&dyn duckdb::ToSql> =
            params.iter().map(|v| v as &dyn duckdb::ToSql).collect();
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(refs.as_slice(), |row| {
            Ok((
                row.get::<_, String>(0)?,
                (
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                    row.get::<_, i64>(4)?,
                ),
            ))
        })?;
        for (k, v) in rows.flatten() {
            sums.insert(k, v);
        }
        Ok(())
    };
    const AGG: &str =
        "sum(size)::BIGINT, count(*), max(mtime), sum(coalesce(physical_size, size))::BIGINT";

    let mut basenames: Vec<String> = roots
        .iter()
        .filter(|r| !r.by_location)
        .filter_map(|r| r.path.rsplit('/').next().map(str::to_string))
        .collect();
    basenames.sort();
    basenames.dedup();
    if !basenames.is_empty() {
        // The outermost `/<b>/` segment is the earliest one in the path:
        // for each basename, the root's end is strpos + len('/<b>') - 1, and
        // the key is the prefix up to the smallest such end. `contains`
        // prefilters (a plain substring scan, far cheaper than a regex).
        let mut ends: Vec<String> = Vec::new();
        let mut filters: Vec<String> = Vec::new();
        let mut params: Vec<duckdb::types::Value> = Vec::new();
        for b in &basenames {
            let seg = format!("/{b}/");
            let tail = b.chars().count() as i64; // len('/<b>') - 1
            ends.push(format!(
                "CASE WHEN strpos(path, ?) > 0 THEN strpos(path, ?) + {tail} END"
            ));
            params.push(seg.clone().into());
            params.push(seg.into());
        }
        for b in &basenames {
            filters.push("contains(path, ?)".to_string());
            params.push(format!("/{b}/").into());
        }
        let end = if ends.len() == 1 {
            ends.remove(0)
        } else {
            format!("least({})", ends.join(", "))
        };
        collect(
            &format!(
                "SELECT k, {AGG} FROM (
                   SELECT substr(path, 1, {end}) AS k, size, mtime, physical_size
                   FROM files WHERE NOT is_dir AND ({}))
                 GROUP BY k",
                filters.join(" OR ")
            ),
            params,
        )?;
    }

    let mut parents: Vec<&str> = roots
        .iter()
        .filter(|r| r.by_location)
        .filter_map(|r| r.path.rsplit_once('/').map(|(p, _)| p))
        .collect();
    parents.sort();
    parents.dedup();
    for p in parents {
        let lo = format!("{p}/");
        // DuckDB's substr counts characters, not bytes.
        let start = lo.chars().count() as i64 + 1;
        collect(
            &format!(
                "SELECT ? || split_part(substr(path, ?), '/', 1) AS k, {AGG}
                 FROM files WHERE NOT is_dir AND path >= ? AND path < ?
                 GROUP BY k"
            ),
            vec![
                lo.clone().into(),
                duckdb::types::Value::BigInt(start),
                lo.into(),
                format!("{p}0").into(),
            ],
        )?;
    }
    Ok(sums)
}

/// Drop hits that sit inside another hit, so bytes are never counted twice
/// and `apply` never removes a path twice (e.g. a `node_modules` inside an
/// old download).
fn drop_nested(hits: Vec<CleanupHit>) -> Vec<CleanupHit> {
    outermost(hits, |h| h.path.as_str())
}

/// Keep only items not inside another item's path (outermost wins).
///
/// Sorted by path *component*, so every descendant directly follows its
/// ancestor. A plain string sort would put `/a/b c` between `/a/b` and
/// `/a/b/x` (space sorts before `/`), and `/a/b/x` would escape as "not
/// nested".
fn outermost<T>(mut items: Vec<T>, path: impl Fn(&T) -> &str) -> Vec<T> {
    items.sort_by(|a, b| path(a).split('/').cmp(path(b).split('/')));
    let mut out: Vec<T> = Vec::with_capacity(items.len());
    for it in items {
        let nested = out.last().is_some_and(|k| {
            path(&it)
                .strip_prefix(path(k))
                .is_some_and(|r| r.starts_with('/'))
        });
        if !nested {
            out.push(it);
        }
    }
    out
}

/// How destructive `apply` should be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyMode {
    /// Permanently `remove_dir_all` each path.
    Delete,
    /// Move each path to `~/.Trash/<name>-<unix-secs>` so the user can undo.
    Trash,
}

/// Reason recorded when a path that no longer exists is skipped
/// (REQ-DCLEAN-029.A1).
pub const FAILURE_MISSING: &str = "missing";
/// Reason recorded when a Trash item is refused in Trash mode.
pub const FAILURE_IN_TRASH: &str = "already in Trash";
/// Reason recorded when moving an existing path to the Trash failed.
pub const FAILURE_TRASH_MOVE: &str = "move to Trash failed";
/// Reason recorded when permanently deleting an existing path failed (for
/// example, permission denied). The rest of the clean proceeds
/// (REQ-DCLEAN-018.A3).
pub const FAILURE_DELETE: &str = "delete failed";

/// One path [`apply`] did not handle, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Failure {
    pub path: String,
    pub reason: String,
}

/// What [`apply`] did: the paths it handled, the paths it skipped and the
/// history entry it recorded (REQ-DCLEAN-034). `entry` is `None` when
/// nothing was handled.
#[derive(Debug, Clone)]
pub struct ApplyOutcome {
    pub handled: Vec<String>,
    pub failed: Vec<Failure>,
    pub entry: Option<crate::history::Entry>,
}

/// The OS Trash used outside tests (REQ-XPLAT-006): `~/.Trash`, the
/// freedesktop Trash, or the Recycle Bin.
fn user_trash() -> Result<PathBuf> {
    crate::trash::user_root()
}

/// The Trash root [`apply`] uses for `mode`. Delete mode never touches the
/// Trash, so it works without a home dir.
pub(crate) fn default_trash(mode: ApplyMode) -> Result<PathBuf> {
    match mode {
        ApplyMode::Trash => user_trash(),
        ApplyMode::Delete => Ok(user_trash().unwrap_or_default()),
    }
}

/// Remove or trash the listed paths and, when `log` is given and anything
/// was handled, append one entry to the clean-history log.
///
/// In [`ApplyMode::Trash`], hits that are already in the Trash (the `trash`
/// category) are refused: moving them would be a no-op, and emptying them
/// would not be reversible.
pub fn apply(hits: &[CleanupHit], mode: ApplyMode, log: Option<&Path>) -> Result<ApplyOutcome> {
    apply_with(hits, mode, log, &default_trash(mode)?)
}

/// [`apply`] with an explicit Trash root, so tests never touch `~/.Trash`.
pub(crate) fn apply_with(
    hits: &[CleanupHit],
    mode: ApplyMode,
    log: Option<&Path>,
    trash_root: &Path,
) -> Result<ApplyOutcome> {
    apply_observed(
        hits,
        mode,
        log,
        trash_root,
        crate::history::TITLE_CLI,
        &mut |_, _| {},
    )
}

/// [`apply_with`] that calls `on_item(hit, index)` once per hit, in order,
/// right after that hit was handled or failed (REQ-DCLEAN-008). Still one
/// history entry for the whole clean, titled `title` (REQ-DCLEAN-009.A5).
pub(crate) fn apply_observed(
    hits: &[CleanupHit],
    mode: ApplyMode,
    log: Option<&Path>,
    trash_root: &Path,
    title: &str,
    on_item: &mut dyn FnMut(&CleanupHit, usize),
) -> Result<ApplyOutcome> {
    use crate::history::{Entry, Item, ItemStatus, Mode};

    let mut handled = Vec::new();
    let mut failed: Vec<Failure> = Vec::new();
    let mut items = Vec::new();
    let mut categories: Vec<String> = Vec::new();
    for (i, h) in hits.iter().enumerate() {
        match apply_one(h, mode, trash_root) {
            Ok(trash_path) => {
                handled.push(h.path.clone());
                // Reviewed files (Large files, Duplicates) carry no category.
                if !h.category.is_empty() && !categories.contains(&h.category) {
                    categories.push(h.category.clone());
                }
                items.push(Item {
                    original_path: h.path.clone(),
                    status: if trash_path.is_some() {
                        ItemStatus::InTrash
                    } else {
                        ItemStatus::Deleted
                    },
                    trash_path,
                    // Physical: what the clean actually freed (REQ-DCLEAN-009.A5).
                    bytes: h.physical_bytes,
                });
            }
            Err(reason) => failed.push(Failure {
                path: h.path.clone(),
                reason: reason.to_string(),
            }),
        }
        on_item(h, i);
    }

    let entry = match log {
        Some(log) if !items.is_empty() => {
            let mode = match mode {
                ApplyMode::Trash => Mode::Trash,
                ApplyMode::Delete => Mode::Delete,
            };
            let entry = crate::history::append(log, Entry::new(title, mode, categories, items))
                .map_err(|e| {
                    e.context(format!(
                        "cleanup handled {} path(s) but could not record them in the \
                         history log {}",
                        handled.len(),
                        log.display()
                    ))
                })?;
            Some(entry)
        }
        _ => None,
    };
    Ok(ApplyOutcome {
        handled,
        failed,
        entry,
    })
}

/// Trash or delete one hit. `Ok(trash_path)` when handled (`None` in delete
/// mode); `Err(reason)` when the path was left as it was.
fn apply_one(
    h: &CleanupHit,
    mode: ApplyMode,
    trash_root: &Path,
) -> Result<Option<String>, &'static str> {
    let p = Path::new(&h.path);
    let bin = crate::trash::Bin::for_root(trash_root);
    match mode {
        ApplyMode::Delete => {
            let is_dir = p.symlink_metadata().is_ok_and(|m| m.is_dir());
            let res = if is_dir {
                std::fs::remove_dir_all(p)
            } else {
                std::fs::remove_file(p)
            };
            match res {
                Ok(()) => {
                    // Emptied for good: its .trashinfo / $I record goes too.
                    if bin.holds(p) {
                        crate::trash::forget_record(p);
                    }
                    Ok(None)
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(FAILURE_MISSING),
                Err(e) => {
                    eprintln!("cleanup: skip {} (delete failed: {e})", h.path);
                    Err(FAILURE_DELETE)
                }
            }
        }
        ApplyMode::Trash => {
            if h.category == TRASH_CATEGORY || bin.holds(p) {
                eprintln!(
                    "cleanup: refused {} — it is already in the Trash and cannot be \
                     removed reversibly; rerun with --apply without --reversible to \
                     delete it permanently",
                    h.path
                );
                return Err(FAILURE_IN_TRASH);
            }
            if p.symlink_metadata().is_err() {
                return Err(FAILURE_MISSING);
            }
            match bin.put(p, h.bytes) {
                Ok(dest) => Ok(Some(crate::platform::to_slash(&dest))),
                Err(_) if p.symlink_metadata().is_ok() => {
                    eprintln!("cleanup: skip {} (move-to-trash failed)", h.path);
                    Err(FAILURE_TRASH_MOVE)
                }
                Err(_) => Err(FAILURE_MISSING),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(category: &str, bytes: u64, files: u64) -> CleanupHit {
        CleanupHit {
            category: category.to_string(),
            safety: Safety::Review,
            path: format!("/tmp/{}-{}", category, bytes),
            bytes,
            physical_bytes: bytes,
            files,
            note: None,
            newest_mtime: None,
        }
    }

    /// REQ-DCLEAN-005: every category names a Lucide icon in kebab case.
    #[test]
    fn every_category_names_a_lucide_icon() {
        for c in CATEGORIES {
            assert!(!c.icon.is_empty(), "{} has no icon", c.name);
            assert!(
                c.icon
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
                "{}: icon {:?} is not a kebab-case Lucide name",
                c.name,
                c.icon
            );
        }
    }

    #[test]
    fn default_target_names_covers_all_categories() {
        let names = default_target_names();
        assert_eq!(names.len(), CATEGORIES.len());
        assert!(names.contains(&"node_modules"));
        assert!(names.contains(&"target"));
        for n in [
            "system_caches",
            "app_caches",
            "temp",
            "logs",
            "trash",
            "app_leftovers",
            "old_downloads",
        ] {
            assert!(names.contains(&n), "missing category {n}");
        }
    }

    #[test]
    fn category_names_are_unique() {
        let names = category_names();
        let set: HashSet<_> = names.iter().collect();
        assert_eq!(set.len(), names.len());
    }

    #[test]
    fn dev_categories_keep_names_and_are_review() {
        // REQ-DCLEAN-033.A1
        let dev = dev_category_names();
        assert_eq!(
            dev,
            vec![
                "node_modules",
                "target",
                "pycache",
                "next",
                "dist",
                "build",
                "venv",
                "gradle",
                "pytest"
            ]
        );
        for c in CATEGORIES.iter().filter(|c| c.is_basename()) {
            assert_eq!(c.safety, Safety::Review, "{} must be review", c.name);
        }
    }

    /// Same names everywhere; each set's safety is its most cautious
    /// location (REQ-XPLAT-005.A1).
    #[test]
    fn every_os_table_names_the_same_categories() {
        let names = [
            "system_caches",
            "app_caches",
            "temp",
            "logs",
            TRASH_CATEGORY,
            "app_leftovers",
        ];
        for table in [MACOS_LOCATIONS, LINUX_LOCATIONS, WINDOWS_LOCATIONS] {
            for n in names {
                let set = table.set_for(n).unwrap();
                for l in set.locations {
                    assert!(
                        l.safety == set.safety || set.safety == Safety::Review,
                        "{n}: {} is more cautious than its set",
                        l.pattern
                    );
                    assert!(
                        l.pattern.starts_with('/')
                            || l.pattern.starts_with("~/")
                            || l.pattern.starts_with("%SystemDrive%/"),
                        "{n}: {} must be absolute",
                        l.pattern
                    );
                }
            }
            assert_eq!(table.set_for("old_downloads").map(|_| ()), None);
            assert!(CATEGORIES.iter().any(|c| c.name == "old_downloads"));
        }
        // No installed-app list outside macOS: no leftovers (A3).
        assert!(LINUX_LOCATIONS.app_leftovers.locations.is_empty());
        assert!(WINDOWS_LOCATIONS.app_leftovers.locations.is_empty());
        assert_eq!(
            LINUX_LOCATIONS.trash.locations[0].pattern,
            "~/.local/share/Trash/files"
        );
    }

    /// Linux `/tmp` holds live sockets; none of them is offered (A2).
    #[test]
    fn linux_temp_skips_live_sockets() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::create_schema(&conn).unwrap();
        let rows = [
            ("/tmp", true, 0),
            ("/tmp/.X11-unix", true, 0),
            ("/tmp/.X11-unix/X0", false, 1),
            ("/tmp/systemd-private-abc-colord.service-x", true, 0),
            ("/tmp/ssh-XXXX", true, 0),
            ("/tmp/tmux-1000", true, 0),
            ("/tmp/old-installer", true, 0),
            ("/tmp/old-installer/a.bin", false, 500),
            ("/var/tmp/cache.bin", false, 70),
        ];
        for (p, d, b) in rows {
            let name = p.rsplit('/').next().unwrap();
            conn.execute(
                "INSERT INTO files (path, name, ext, size, physical_size, mtime, is_dir, depth)
                 VALUES (?, ?, NULL, ?, ?, 0, ?, 0)",
                duckdb::params![p, name, b, b, d],
            )
            .unwrap();
        }
        let ctx = ScanContext {
            scan_root: Some("/".into()),
            locations: LINUX_LOCATIONS,
            now_secs: 1_000_000_000,
            ..Default::default()
        };
        let hits = scan_with(&conn, &["temp".to_string()], 100, &ctx).unwrap();
        let paths: Vec<&str> = hits.iter().map(|h| h.path.as_str()).collect();
        assert_eq!(paths, ["/tmp/old-installer", "/var/tmp/cache.bin"]);
        assert!(hits.iter().all(|h| h.safety == Safety::Review));
    }

    #[test]
    fn location_safety_classes_match_contract() {
        let loc_safety = |cat: &str, pat: &str| -> Safety {
            let set = MACOS_LOCATIONS.set_for(cat).unwrap();
            set.locations
                .iter()
                .find(|l| l.pattern == pat)
                .unwrap()
                .safety
        };
        assert_eq!(loc_safety("system_caches", "/Library/Caches"), Safety::Safe);
        assert_eq!(loc_safety("app_caches", "~/Library/Caches"), Safety::Safe);
        assert_eq!(loc_safety("temp", "/private/tmp"), Safety::Safe);
        assert_eq!(
            loc_safety("temp", "/private/var/folders/*/*/T"),
            Safety::Review
        );
        assert_eq!(loc_safety("trash", "~/.Trash"), Safety::Safe);
        assert_eq!(MACOS_LOCATIONS.logs.safety, Safety::Review);
        assert_eq!(category("logs").unwrap().safety, Safety::Review);
        assert_eq!(category("app_leftovers").unwrap().safety, Safety::Review);
        assert_eq!(category("old_downloads").unwrap().safety, Safety::Review);
        assert!(category("trash").unwrap().note.is_some());
    }

    #[test]
    fn safety_serialises_lowercase() {
        assert_eq!(serde_json::to_string(&Safety::Safe).unwrap(), "\"safe\"");
        assert_eq!(
            serde_json::to_string(&Safety::Review).unwrap(),
            "\"review\""
        );
    }

    #[test]
    fn summarise_aggregates_and_sorts_by_bytes_desc() {
        let hits = vec![
            hit("node_modules", 1024, 5),
            hit("node_modules", 2048, 10),
            hit("target", 8192, 3),
        ];
        let summary = summarise(&hits);
        assert_eq!(summary.len(), 2);
        assert_eq!(summary[0].category, "target");
        assert_eq!(summary[0].bytes, 8192);
        assert_eq!(summary[1].category, "node_modules");
        assert_eq!(summary[1].bytes, 3072);
        assert_eq!(summary[1].paths, 2);
        assert_eq!(summary[1].files, 15);
    }

    #[test]
    fn summarise_takes_most_cautious_safety() {
        let mut a = hit("temp", 10, 1);
        a.safety = Safety::Safe;
        let b = hit("temp", 20, 1);
        let summary = summarise(&[a.clone(), b]);
        assert_eq!(summary[0].safety, Safety::Review);
        assert_eq!(summarise(&[a])[0].safety, Safety::Safe);
    }

    #[test]
    fn summarise_empty_input_returns_empty() {
        let summary = summarise(&[]);
        assert!(summary.is_empty());
    }

    #[test]
    fn basenames_for_skips_unknown_targets() {
        let targets = vec!["node_modules".to_string(), "totally-fake".to_string()];
        let pairs = basenames_for(&targets);
        assert!(pairs.iter().all(|(n, _)| *n == "node_modules"));
        assert_eq!(pairs.len(), 1);
    }

    #[test]
    fn basenames_for_expands_multi_basename_categories() {
        let targets = vec!["venv".to_string()];
        let pairs = basenames_for(&targets);
        let names: Vec<_> = pairs.iter().map(|(_, b)| *b).collect();
        assert!(names.contains(&".venv"));
        assert!(names.contains(&"venv"));
    }

    #[test]
    fn is_protected_catches_cargo_registry() {
        // The bug that started the ignore list: cc-1.2.62/src/target inside
        // ~/.cargo/registry was matched as a `target` cleanup candidate.
        assert!(is_protected(
            "/Users/me/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/cc-1.2.62/src/target"
        ));
        assert!(is_protected("/Users/me/.cargo/git/checkouts/foo"));
        assert!(is_protected(
            "/Users/me/.rustup/toolchains/stable-aarch64-apple-darwin/lib"
        ));
    }

    #[test]
    fn is_protected_catches_python_site_packages() {
        // venv installs live under site-packages/<pkg>/. We don't want to
        // accidentally clean a `build` subdir of an installed package.
        assert!(is_protected(
            "/Users/me/venv/lib/python3.13/site-packages/foo/build"
        ));
    }

    #[test]
    fn is_protected_catches_os_caches() {
        assert!(is_protected("/Users/me/Library/Caches/Slack/build"));
        assert!(is_protected(
            "/Users/me/Library/Application Support/disky/build"
        ));
        assert!(is_protected(
            "/Users/me/Library/Group Containers/HUAQ24HBR6.dev.orbstack/data"
        ));
    }

    #[test]
    fn is_protected_passes_real_dev_dirs() {
        // These are the common-case targets we DO want to clean.
        assert!(!is_protected("/Users/me/src/myapp/target"));
        assert!(!is_protected("/Users/me/src/myapp/node_modules"));
        assert!(!is_protected("/Users/me/src/myapp/.venv"));
        assert!(!is_protected("/Users/me/src/myapp/build"));
        assert!(!is_protected("/Users/me/src/myapp/dist"));
    }

    #[test]
    fn in_scope_rejects_paths_outside_root() {
        let root = Some("/Users/me/src");
        assert!(in_scope("/Users/me/src/app/target", root, &[], &[]));
        assert!(in_scope("/Users/me/src", root, &[], &[]));
        assert!(!in_scope("/Users/me/srcfoo/target", root, &[], &[]));
        assert!(!in_scope("/Users/me/.Trash/x", root, &[], &[]));
        // Root `/` admits everything.
        assert!(in_scope("/private/tmp/x", Some("/"), &[], &[]));
        assert!(in_scope("/private/tmp/x", None, &[], &[]));
    }

    #[test]
    fn in_scope_rejects_ignored_and_excluded_paths() {
        let ignore = vec!["keepme".to_string()];
        let root = Some("/Users/me");
        assert!(!in_scope(
            "/Users/me/keepme-proj/node_modules",
            root,
            &ignore,
            &[]
        ));
        assert!(in_scope("/Users/me/proj/node_modules", root, &ignore, &[]));
        // Patterns only apply below the root, never to the root's own path.
        assert!(in_scope(
            "/Users/keepme/proj/target",
            Some("/Users/keepme"),
            &ignore,
            &[]
        ));
        let excluded = vec!["/Users/me/work/".to_string()];
        assert!(!in_scope("/Users/me/work/a/target", root, &[], &excluded));
        assert!(!in_scope("/Users/me/work", root, &[], &excluded));
        assert!(in_scope("/Users/me/workshop/target", root, &[], &excluded));
    }

    #[test]
    fn is_child_of_matches_one_level_and_wildcards() {
        assert!(is_child_of("/Library/Caches", "/Library/Caches/foo"));
        assert!(!is_child_of("/Library/Caches", "/Library/Caches/foo/bar"));
        assert!(!is_child_of("/Library/Caches", "/Library/Caches"));
        assert!(is_child_of(
            "/private/var/folders/*/*/T",
            "/private/var/folders/xy/abc123/T/tmp.1"
        ));
        assert!(!is_child_of(
            "/private/var/folders/*/*/T",
            "/private/var/folders/xy/abc123/C/tmp.1"
        ));
    }

    #[test]
    fn installed_apps_match_by_bundle_id_and_name() {
        let apps = InstalledApps::from_apps([
            (
                Some("com.microsoft.VSCode"),
                vec!["Visual Studio Code", "Code"],
            ),
            (Some("com.google.Chrome"), vec!["Google Chrome"]),
            (Some("com.tinyspeck.slackmacgap"), vec!["Slack"]),
        ]);
        // Application Support is keyed by name.
        assert!(apps.owns("Code"));
        assert!(apps.owns("Google"));
        assert!(apps.owns("Slack"));
        // Containers are keyed by bundle id.
        assert!(apps.owns("com.microsoft.VSCode"));
        assert!(apps.owns("com.google.Chrome.helper"));
        assert!(apps.owns("com.tinyspeck.slackmacgap"));
        // Unrelated → leftover.
        assert!(!apps.owns("OldThing"));
        assert!(!apps.owns("com.defunct.Widget"));
    }

    #[test]
    fn drop_nested_keeps_outermost() {
        let mk = |p: &str| CleanupHit {
            path: p.into(),
            ..hit("x", 1, 1)
        };
        let out = drop_nested(vec![
            mk("/a/b/node_modules"),
            mk("/a/b"),
            mk("/a/bc"),
            mk("/z"),
        ]);
        let paths: Vec<_> = out.iter().map(|h| h.path.as_str()).collect();
        assert_eq!(paths, vec!["/a/b", "/a/bc", "/z"]);
    }

    #[test]
    fn drop_nested_sees_through_a_sibling_with_a_space() {
        // "/a/b c" sorts between "/a/b" and "/a/b/x" as plain strings.
        let mk = |p: &str| CleanupHit {
            path: p.into(),
            ..hit("x", 1, 1)
        };
        let out = drop_nested(vec![mk("/a/b/x"), mk("/a/b c"), mk("/a/b")]);
        let paths: Vec<_> = out.iter().map(|h| h.path.as_str()).collect();
        assert_eq!(paths, vec!["/a/b", "/a/b c"]);
    }

    fn real_hit(category: &str, path: &Path, bytes: u64) -> CleanupHit {
        CleanupHit {
            path: path.to_string_lossy().into_owned(),
            ..hit(category, bytes, 1)
        }
    }

    #[test]
    fn apply_trash_records_one_history_entry() {
        use crate::history::{ItemStatus, Mode};
        let tmp = tempfile::tempdir().unwrap();
        let trash = tmp.path().join("Trash");
        let log = tmp.path().join("history.json");
        let nm = tmp.path().join("proj/node_modules");
        let tg = tmp.path().join("proj/target");
        std::fs::create_dir_all(&nm).unwrap();
        std::fs::create_dir_all(&tg).unwrap();
        std::fs::write(nm.join("x.js"), b"x").unwrap();
        let hits = [
            real_hit("node_modules", &nm, 100),
            real_hit("target", &tg, 50),
        ];

        let out = apply_with(&hits, ApplyMode::Trash, Some(&log), &trash).unwrap();
        assert_eq!(out.handled.len(), 2);
        let entries = crate::history::load(&log).unwrap();
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(Some(e), out.entry.as_ref());
        assert_eq!(e.mode, Mode::Trash);
        assert_eq!(e.bytes, 150);
        assert_eq!(e.categories, ["node_modules", "target"]);
        for (item, hit) in e.items.iter().zip(&hits) {
            assert_eq!(item.original_path, hit.path);
            assert_eq!(item.bytes, hit.physical_bytes);
            assert_eq!(item.status, ItemStatus::InTrash);
            let t = Path::new(item.trash_path.as_deref().unwrap());
            assert!(t.starts_with(&trash) && t.exists(), "{t:?}");
        }
        assert!(!nm.exists());
        assert!(nm_contents_in_trash(&e.items[0]));
    }

    fn nm_contents_in_trash(item: &crate::history::Item) -> bool {
        Path::new(item.trash_path.as_deref().unwrap())
            .join("x.js")
            .exists()
    }

    #[test]
    fn apply_delete_records_entry_without_trash_paths() {
        use crate::history::{ItemStatus, Mode};
        let tmp = tempfile::tempdir().unwrap();
        let log = tmp.path().join("history.json");
        let f = tmp.path().join("build");
        std::fs::create_dir_all(&f).unwrap();
        let hits = [real_hit("build", &f, 7)];
        let out = apply_with(&hits, ApplyMode::Delete, Some(&log), &tmp.path().join("T")).unwrap();
        assert_eq!(out.handled.len(), 1);
        let e = &crate::history::load(&log).unwrap()[0];
        assert_eq!(e.mode, Mode::Delete);
        assert_eq!(e.items[0].trash_path, None);
        assert_eq!(e.items[0].status, ItemStatus::Deleted);
        assert!(!f.exists());
    }

    #[test]
    fn apply_with_nothing_handled_writes_no_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let log = tmp.path().join("history.json");
        let gone = real_hit("build", &tmp.path().join("missing"), 1);
        let out = apply_with(&[gone], ApplyMode::Delete, Some(&log), tmp.path()).unwrap();
        assert!(out.handled.is_empty() && out.entry.is_none());
        assert!(!log.exists());
    }

    #[test]
    fn apply_observed_reports_each_hit_in_order() {
        let tmp = tempfile::tempdir().unwrap();
        let trash = tmp.path().join("Trash");
        let log = tmp.path().join("history.json");
        let a = tmp.path().join("a/node_modules");
        let b = tmp.path().join("b/target");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let gone = tmp.path().join("gone/build");
        let hits = [
            real_hit("node_modules", &a, 3),
            real_hit("build", &gone, 4),
            real_hit("target", &b, 5),
        ];
        let mut seen = Vec::new();
        let out = apply_observed(
            &hits,
            ApplyMode::Trash,
            Some(&log),
            &trash,
            "Smart Clean",
            &mut |h, i| seen.push((i, h.category.clone())),
        )
        .unwrap();
        assert_eq!(
            seen,
            [
                (0, "node_modules".to_string()),
                (1, "build".to_string()),
                (2, "target".to_string())
            ],
            "once per hit, failures included, in order"
        );
        assert_eq!(out.handled.len(), 2);
        let entries = crate::history::load(&log).unwrap();
        assert_eq!(entries.len(), 1, "one history entry per clean");
        assert_eq!(entries[0].title, "Smart Clean");
        assert_eq!(entries[0].bytes, 8, "physical bytes actually freed");
    }

    #[cfg(unix)]
    #[test]
    fn apply_delete_failure_leaves_path_and_proceeds() {
        use std::os::unix::fs::PermissionsExt;
        // Root reads and deletes regardless of mode bits (containers, CI
        // images): there is no permission failure to observe.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let log = tmp.path().join("history.json");
        // A file inside a read-only dir cannot be unlinked.
        let locked_dir = tmp.path().join("locked");
        std::fs::create_dir_all(&locked_dir).unwrap();
        let locked = locked_dir.join("build");
        std::fs::write(&locked, b"x").unwrap();
        std::fs::set_permissions(&locked_dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        let ok = tmp.path().join("ok/target");
        std::fs::create_dir_all(&ok).unwrap();
        let hits = [real_hit("build", &locked, 1), real_hit("target", &ok, 2)];

        let out = apply_with(&hits, ApplyMode::Delete, Some(&log), &tmp.path().join("T"));
        std::fs::set_permissions(&locked_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        let out = out.unwrap();
        assert_eq!(out.handled, [hits[1].path.clone()]);
        assert_eq!(
            out.failed,
            [Failure {
                path: hits[0].path.clone(),
                reason: FAILURE_DELETE.into()
            }]
        );
        assert!(locked.exists(), "failed path is untouched");
        assert!(!ok.exists());
        assert_eq!(crate::history::load(&log).unwrap().len(), 1);
    }
}
