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
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// How confident the engine is that a hit can be removed without loss.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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
    pub name: &'static str,
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

/// The single table of cleanup categories (REQ-DCLEAN-033.A2). The GUI and
/// `disky schema` read this; nothing else defines a category.
pub const CATEGORIES: &[Category] = &[
    // ── Developer artefacts (folder-name rules, all `review`) ──────────
    Category {
        name: "node_modules",
        safety: Safety::Review,
        matcher: Matcher::Basename(&["node_modules"]),
        note: None,
    },
    Category {
        name: "target",
        safety: Safety::Review,
        matcher: Matcher::Basename(&["target"]),
        note: None,
    },
    Category {
        name: "pycache",
        safety: Safety::Review,
        matcher: Matcher::Basename(&["__pycache__"]),
        note: None,
    },
    Category {
        name: "next",
        safety: Safety::Review,
        matcher: Matcher::Basename(&[".next"]),
        note: None,
    },
    Category {
        name: "dist",
        safety: Safety::Review,
        matcher: Matcher::Basename(&["dist"]),
        note: None,
    },
    Category {
        name: "build",
        safety: Safety::Review,
        matcher: Matcher::Basename(&["build"]),
        note: None,
    },
    Category {
        name: "venv",
        safety: Safety::Review,
        matcher: Matcher::Basename(&[".venv", "venv"]),
        note: None,
    },
    Category {
        name: "gradle",
        safety: Safety::Review,
        matcher: Matcher::Basename(&[".gradle"]),
        note: None,
    },
    Category {
        name: "pytest",
        safety: Safety::Review,
        matcher: Matcher::Basename(&[".pytest_cache"]),
        note: None,
    },
    // ── System-level categories (location rules) ───────────────────────
    Category {
        name: "system_caches",
        safety: Safety::Safe,
        matcher: Matcher::Location {
            locations: &[Location {
                pattern: "/Library/Caches",
                safety: Safety::Safe,
            }],
            filter: LocationFilter::All,
            // Apple's own system caches are SIP-managed or rebuilt by
            // daemons that misbehave while they are missing.
            skip_children: &["com.apple."],
        },
        note: None,
    },
    Category {
        name: "app_caches",
        safety: Safety::Safe,
        matcher: Matcher::Location {
            locations: &[Location {
                pattern: "~/Library/Caches",
                safety: Safety::Safe,
            }],
            filter: LocationFilter::All,
            // iCloud sync state lives here; deleting it forces a full resync.
            skip_children: &[
                "CloudKit",
                "com.apple.bird",
                "com.apple.cloudd",
                "com.apple.nsurlsessiond",
                "FamilyCircle",
                "disky",
            ],
        },
        note: None,
    },
    Category {
        name: "temp",
        safety: Safety::Review,
        matcher: Matcher::Location {
            locations: &[
                Location {
                    pattern: "/private/tmp",
                    safety: Safety::Safe,
                },
                // Per-user temp dirs hold live files of running apps.
                Location {
                    pattern: "/private/var/folders/*/*/T",
                    safety: Safety::Review,
                },
            ],
            filter: LocationFilter::All,
            // launchd sockets and Finder's in-flight copies.
            skip_children: &["com.apple.", "TemporaryItems"],
        },
        note: None,
    },
    Category {
        name: "logs",
        safety: Safety::Review,
        matcher: Matcher::Location {
            locations: &[
                Location {
                    pattern: "~/Library/Logs",
                    safety: Safety::Review,
                },
                Location {
                    pattern: "/Library/Logs",
                    safety: Safety::Review,
                },
            ],
            filter: LocationFilter::All,
            skip_children: &[],
        },
        note: None,
    },
    Category {
        name: TRASH_CATEGORY,
        safety: Safety::Safe,
        matcher: Matcher::Location {
            locations: &[Location {
                pattern: "~/.Trash",
                safety: Safety::Safe,
            }],
            filter: LocationFilter::All,
            skip_children: &[],
        },
        note: Some(TRASH_NOTE),
    },
    Category {
        name: "app_leftovers",
        safety: Safety::Review,
        matcher: Matcher::Location {
            locations: &[
                // Keyed by app name.
                Location {
                    pattern: "~/Library/Application Support",
                    safety: Safety::Review,
                },
                // Keyed by bundle id.
                Location {
                    pattern: "~/Library/Containers",
                    safety: Safety::Review,
                },
            ],
            filter: LocationFilter::AppLeftovers,
            skip_children: LEFTOVER_SKIP,
        },
        note: None,
    },
    Category {
        name: "old_downloads",
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
    pub bytes: u64,
    pub files: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CategorySummary {
    pub category: String,
    /// `review` if any hit in the category is `review`.
    pub safety: Safety,
    pub paths: u64,
    pub bytes: u64,
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
                files: 0,
            });
        if h.safety == Safety::Review {
            entry.safety = Safety::Review;
        }
        entry.paths += 1;
        entry.bytes += h.bytes;
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
    /// Absolute folders the user excluded. Hits inside them are dropped.
    pub excluded: Vec<String>,
    /// "Now" in unix seconds, for age filters.
    pub now_secs: i64,
    /// Installed apps for the leftover check. `None` → read from disk the
    /// first time `app_leftovers` needs it.
    pub installed_apps: Option<InstalledApps>,
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
            home: dirs::home_dir(),
            scan_root,
            ignore,
            excluded: Vec::new(),
            now_secs,
            installed_apps: None,
        }
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
        match pattern.strip_prefix("~/") {
            Some(rest) => {
                let home = self.home.as_ref()?.to_string_lossy();
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
    is_dir: bool,
    self_mtime: Option<i64>,
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
    for (path, name, is_dir, mtime) in rows {
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
            is_dir,
            self_mtime: mtime,
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
            is_dir: true,
            self_mtime: mtime,
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
    if roots.is_empty() {
        return Ok(vec![]);
    }

    // History (kept for context):
    //   v1: `LEFT JOIN files f ON f.path LIKE (t.path || '/%')` — 79 s on a
    //       1.77 M-row snapshot. DuckDB planned a nested-loop join with
    //       per-row LIKE evaluation.
    //   v2: materialise target dirs in Rust, then issue ONE aggregation
    //       query per target with a path-range predicate. Better than v1
    //       but still O(N_targets × N_files / zone_maps): on real macOS
    //       homes with thousands of `target` / `node_modules` dirs the
    //       per-query overhead (DuckDB statement dispatch, FFI round-trip)
    //       dominates and the whole call lands at 79–250 s.
    //   v3: one grouped query. Stash target dirs in a temporary table
    //       (DuckDB then computes its zone maps) and let the planner fuse
    //       them with `files` via a single range-join + GROUP BY.
    //       Brings 79 s → < 1 s on a 1 M-row synthetic snapshot.
    //   v4 (this): same single join; location-rule roots join the same
    //       TEMP table. A root that is a file uses the range
    //       [path, path || '\x01'), which matches exactly itself.
    //
    // We materialise roots into a TEMP table (instead of an inline VALUES
    // list) because DuckDB can build zone maps on it, which the IEJoin
    // planner exploits; all filtering happens in Rust beforehand.
    conn.execute_batch(
        "DROP TABLE IF EXISTS _cleanup_targets;
         CREATE TEMP TABLE _cleanup_targets (
             idx  BIGINT NOT NULL,
             lo   TEXT NOT NULL,
             hi   TEXT NOT NULL
         );",
    )?;
    {
        let mut app = conn.appender("_cleanup_targets")?;
        for (i, r) in roots.iter().enumerate() {
            let (lo, hi) = if r.is_dir {
                (format!("{}/", r.path), format!("{}0", r.path))
            } else {
                (r.path.clone(), format!("{}\u{1}", r.path))
            };
            app.append_row(duckdb::params![i as i64, lo, hi])?;
        }
        app.flush()?;
    }

    // One grouped range-join. DuckDB plans this as an IEJoin (inequality
    // join), which is roughly linear in N_files + N_targets.
    let mut grouped = conn.prepare(
        "SELECT t.idx,
                COALESCE(SUM(f.size), 0) AS bytes,
                COUNT(f.path)             AS files,
                MAX(f.mtime)              AS newest
         FROM _cleanup_targets t
         LEFT JOIN files f
           ON f.is_dir = false
          AND f.path >= t.lo
          AND f.path <  t.hi
         GROUP BY t.idx",
    )?;
    let rows = grouped.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, Option<i64>>(3)?,
        ))
    })?;

    let mut out: Vec<CleanupHit> = Vec::with_capacity(roots.len());
    for (idx, bytes, files, newest) in rows.flatten() {
        let Some(r) = roots.get(idx as usize) else {
            continue;
        };
        if let Matcher::Location {
            filter: LocationFilter::OlderThanDays(days),
            ..
        } = r.category.matcher
        {
            let cutoff = ctx.now_secs - i64::from(days) * 86_400;
            let newest = match (newest, r.self_mtime) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (a, b) => a.or(b),
            };
            // Unknown age → not provably old → not reported.
            if !matches!(newest, Some(m) if m < cutoff) {
                continue;
            }
        }
        out.push(CleanupHit {
            category: r.category.name.to_string(),
            safety: r.safety,
            path: r.path.clone(),
            bytes: bytes as u64,
            files: files as u64,
            note: r.category.note.map(String::from),
        });
    }
    drop(grouped);
    let _ = conn.execute_batch("DROP TABLE IF EXISTS _cleanup_targets;");

    let mut out = drop_nested(out);
    out.sort_by_key(|h| std::cmp::Reverse(h.bytes));
    out.truncate(limit);
    Ok(out)
}

/// Drop hits that sit inside another hit, so bytes are never counted twice
/// and `apply` never removes a path twice (e.g. a `node_modules` inside an
/// old download).
fn drop_nested(mut hits: Vec<CleanupHit>) -> Vec<CleanupHit> {
    hits.sort_by(|a, b| a.path.cmp(&b.path));
    let mut out: Vec<CleanupHit> = Vec::with_capacity(hits.len());
    for h in hits {
        let nested = out.last().is_some_and(|k| {
            h.path
                .strip_prefix(k.path.as_str())
                .is_some_and(|r| r.starts_with('/'))
        });
        if !nested {
            out.push(h);
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

/// What [`apply`] did: the paths it handled and the history entry it
/// recorded (REQ-DCLEAN-034). `entry` is `None` when nothing was handled.
#[derive(Debug, Clone)]
pub struct ApplyOutcome {
    pub handled: Vec<String>,
    pub entry: Option<crate::history::Entry>,
}

/// `~/.Trash`, the Trash used outside tests.
fn user_trash() -> Result<PathBuf> {
    Ok(dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("home dir unavailable"))?
        .join(".Trash"))
}

/// Remove or trash the listed paths and, when `log` is given and anything
/// was handled, append one entry to the clean-history log.
///
/// In [`ApplyMode::Trash`], hits that are already in the Trash (the `trash`
/// category) are refused: moving them would be a no-op, and emptying them
/// would not be reversible.
pub fn apply(hits: &[CleanupHit], mode: ApplyMode, log: Option<&Path>) -> Result<ApplyOutcome> {
    // Delete mode never touches the Trash, so it works without a home dir.
    let trash = match mode {
        ApplyMode::Trash => user_trash()?,
        ApplyMode::Delete => user_trash().unwrap_or_default(),
    };
    apply_with(hits, mode, log, &trash)
}

/// [`apply`] with an explicit Trash root, so tests never touch `~/.Trash`.
pub(crate) fn apply_with(
    hits: &[CleanupHit],
    mode: ApplyMode,
    log: Option<&Path>,
    trash_root: &Path,
) -> Result<ApplyOutcome> {
    use crate::history::{Entry, Item, ItemStatus, Mode};

    let mut handled = Vec::new();
    let mut items = Vec::new();
    let mut categories: Vec<String> = Vec::new();
    for h in hits {
        let trash_path = match mode {
            ApplyMode::Delete => {
                let p = Path::new(&h.path);
                let is_dir = p.symlink_metadata().is_ok_and(|m| m.is_dir());
                let res = if is_dir {
                    std::fs::remove_dir_all(p)
                } else {
                    std::fs::remove_file(p)
                };
                match res {
                    Ok(()) => None,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(e) => return Err(e.into()),
                }
            }
            ApplyMode::Trash => {
                let in_trash = !trash_root.as_os_str().is_empty()
                    && Path::new(&h.path).starts_with(trash_root);
                if h.category == TRASH_CATEGORY || in_trash {
                    eprintln!(
                        "cleanup: refused {} — it is already in the Trash and cannot be \
                         removed reversibly; rerun with --apply without --reversible to \
                         delete it permanently",
                        h.path
                    );
                    continue;
                }
                match move_to_trash(&h.path, trash_root) {
                    Ok(dest) => Some(dest.to_string_lossy().into_owned()),
                    Err(_) => {
                        if Path::new(&h.path).exists() {
                            eprintln!("cleanup: skip {} (move-to-trash failed)", h.path);
                        }
                        continue;
                    }
                }
            }
        };
        handled.push(h.path.clone());
        if !categories.contains(&h.category) {
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
            bytes: h.bytes,
        });
    }

    let entry = match log {
        Some(log) if !items.is_empty() => {
            let mode = match mode {
                ApplyMode::Trash => Mode::Trash,
                ApplyMode::Delete => Mode::Delete,
            };
            let entry =
                crate::history::append(log, Entry::new(mode, categories, items)).map_err(|e| {
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
    Ok(ApplyOutcome { handled, entry })
}

fn move_to_trash(path: &str, trash: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(trash)?;
    let src = Path::new(path);
    let name = src
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "disky-trashed".into());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let stamp = format!("{}-{}", now.as_secs(), now.subsec_nanos());
    let mut dest = trash.join(format!("{}-{}", name, stamp));
    let mut n = 0u32;
    while dest.exists() {
        n += 1;
        dest = trash.join(format!("{}-{}-{}", name, stamp, n));
    }
    std::fs::rename(src, &dest)?;
    Ok(dest)
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
            files,
            note: None,
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

    #[test]
    fn location_safety_classes_match_contract() {
        let loc_safety = |cat: &str, pat: &str| -> Safety {
            let Matcher::Location { locations, .. } = category(cat).unwrap().matcher else {
                panic!("{cat} is not a location rule");
            };
            locations.iter().find(|l| l.pattern == pat).unwrap().safety
        };
        assert_eq!(loc_safety("system_caches", "/Library/Caches"), Safety::Safe);
        assert_eq!(loc_safety("app_caches", "~/Library/Caches"), Safety::Safe);
        assert_eq!(loc_safety("temp", "/private/tmp"), Safety::Safe);
        assert_eq!(
            loc_safety("temp", "/private/var/folders/*/*/T"),
            Safety::Review
        );
        assert_eq!(loc_safety("trash", "~/.Trash"), Safety::Safe);
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
            assert_eq!(item.bytes, hit.bytes);
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
}
