//! `.diskyignore` loader — gitignore-style ergonomics, additive only.
//!
//! Patterns are **substring matches** against directory basenames, consistent
//! with the built-in skip list. Globs (`*`, `?`, `**`) and negation (`!`) are
//! deliberately deferred for v1.

use std::fs;
use std::path::{Path, PathBuf};

/// Built-in skip list — these directory basenames are filtered out of every
/// scan regardless of `.diskyignore` contents.
pub fn default_skip_substrings() -> Vec<String> {
    [
        "node_modules",
        "target",
        "__pycache__",
        ".next",
        "dist",
        "build",
        ".venv",
        "venv",
        ".gradle",
        ".pytest_cache",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Walk from `scan_root` up to the user's HOME (or `/`), reading any
/// `.diskyignore` files. Returns the merged additive pattern list.
///
/// The walk is bounded: it stops at HOME (when set) or at the filesystem root,
/// whichever comes first. Ancestors **above** HOME are not consulted.
pub fn load_diskyignore_chain(scan_root: &Path) -> Vec<String> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut patterns: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Canonicalize when possible — falls back to the literal path otherwise so
    // tests using non-existent prefixes still work.
    let mut cursor: PathBuf = scan_root
        .canonicalize()
        .unwrap_or_else(|_| scan_root.to_path_buf());

    loop {
        let candidate = cursor.join(".diskyignore");
        if let Ok(contents) = fs::read_to_string(&candidate) {
            for pat in parse(&contents) {
                if seen.insert(pat.clone()) {
                    patterns.push(pat);
                }
            }
        }

        // Stop at HOME boundary.
        if let Some(h) = home.as_ref() {
            if cursor == *h {
                break;
            }
        }

        match cursor.parent() {
            Some(p) if p != cursor => cursor = p.to_path_buf(),
            _ => break,
        }
    }

    patterns
}

/// Parse a `.diskyignore` file body. Lenient — malformed lines are dropped
/// rather than producing errors.
fn parse(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in body.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // v1: substring only. Ignore lines containing glob meta chars to make
        // a future `*`/`?` upgrade non-breaking — for now we drop them so
        // users get the loudest possible "not implemented yet" signal (no
        // accidental literal-`*` matches).
        if line.contains('*') || line.contains('?') {
            continue;
        }
        // Negation deferred — drop `!`-prefixed lines.
        if line.starts_with('!') {
            continue;
        }
        out.push(line.to_string());
    }
    out
}

/// True if `basename` should be skipped given the merged pattern list.
pub fn should_skip(basename: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|p| basename.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn empty_dir_returns_no_patterns() {
        let dir = tempdir().unwrap();
        // Set HOME to the tempdir so the walk stops immediately.
        std::env::set_var("HOME", dir.path());
        let patterns = load_diskyignore_chain(dir.path());
        assert!(patterns.is_empty());
    }

    #[test]
    fn single_file_parsed() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join(".diskyignore"), "foo\nbar\n").unwrap();
        std::env::set_var("HOME", dir.path());
        let patterns = load_diskyignore_chain(dir.path());
        assert!(patterns.contains(&"foo".to_string()));
        assert!(patterns.contains(&"bar".to_string()));
    }

    #[test]
    fn comments_and_blanks_skipped() {
        let dir = tempdir().unwrap();
        let body = "# header\n\nfoo\n   \n# trailing\nbar\n";
        fs::write(dir.path().join(".diskyignore"), body).unwrap();
        std::env::set_var("HOME", dir.path());
        let patterns = load_diskyignore_chain(dir.path());
        assert_eq!(patterns, vec!["foo".to_string(), "bar".to_string()]);
    }

    #[test]
    fn malformed_lines_dropped_leniently() {
        // Globs / negation are not yet supported; the parser must not panic.
        let dir = tempdir().unwrap();
        let body = "*.log\n!keepme\n? maybe\nok_pattern\n";
        fs::write(dir.path().join(".diskyignore"), body).unwrap();
        std::env::set_var("HOME", dir.path());
        let patterns = load_diskyignore_chain(dir.path());
        assert_eq!(patterns, vec!["ok_pattern".to_string()]);
    }

    #[test]
    fn chain_walks_ancestors() {
        let root = tempdir().unwrap();
        let parent = root.path().join("parent");
        let child = parent.join("child");
        fs::create_dir_all(&child).unwrap();
        fs::write(parent.join(".diskyignore"), "parent_pat\n").unwrap();
        fs::write(child.join(".diskyignore"), "child_pat\n").unwrap();
        std::env::set_var("HOME", root.path());

        let patterns = load_diskyignore_chain(&child);
        assert!(patterns.contains(&"child_pat".to_string()));
        assert!(patterns.contains(&"parent_pat".to_string()));
    }

    #[test]
    fn should_skip_substring_match() {
        let patterns = vec!["foo".to_string()];
        assert!(should_skip("foo", &patterns));
        assert!(should_skip("myfoodir", &patterns));
        assert!(!should_skip("bar", &patterns));
    }
}
