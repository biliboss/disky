//! Layered configuration: built-in defaults → file → env → CLI.
//!
//! Hand-rolled over `toml` to avoid pulling figment/config and their
//! transitive crates. The schema is intentionally small — every later feature
//! that wants a persistent default reads from here.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

/// Top-level config loaded from `~/.config/disky/config.toml`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub defaults: Defaults,
    pub scan: ScanConfig,
    pub output: OutputConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Defaults {
    /// Default `--format` when none is given. "text" | "json" | "ndjson".
    pub format: Option<String>,
    /// Default `--snapshot` for query commands. `@latest` is the implicit value.
    pub snapshot: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ScanConfig {
    /// 0 = num_cpus. Future: wire into rayon pool size.
    pub threads: Option<usize>,
    /// "parallel" | "sequential" | "adaptive"
    pub strategy: Option<String>,
    pub respect_gitignore: Option<bool>,
    pub cross_device: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct OutputConfig {
    /// "auto" | "always" | "never". Overridden by NO_COLOR env.
    pub color: Option<String>,
}

impl Config {
    /// Path resolution: $DISKY_CONFIG_PATH wins; otherwise
    /// `dirs::config_dir()/disky/config.toml`.
    pub fn default_path() -> Option<PathBuf> {
        if let Ok(p) = std::env::var("DISKY_CONFIG_PATH") {
            return Some(PathBuf::from(p));
        }
        dirs::config_dir().map(|d| d.join("disky").join("config.toml"))
    }

    /// Load from `default_path()`. Missing file → empty config (not an error).
    /// Malformed file → `DiskyError::usage`-compatible error message.
    pub fn load() -> Result<Self> {
        match Self::default_path() {
            Some(p) if p.exists() => Self::load_from(&p),
            _ => Ok(Self::default()),
        }
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        let text =
            fs::read_to_string(path).with_context(|| format!("read config {}", path.display()))?;
        let cfg: Config =
            toml::from_str(&text).with_context(|| format!("parse config {}", path.display()))?;
        Ok(cfg.merged_with_env())
    }

    /// Apply env-var overrides on top of the parsed file.
    pub fn merged_with_env(mut self) -> Self {
        if let Ok(f) = std::env::var("DISKY_FORMAT") {
            self.defaults.format = Some(f);
        }
        if let Ok(s) = std::env::var("DISKY_SNAPSHOT") {
            self.defaults.snapshot = Some(s);
        }
        self
    }

    /// Resolve the default `--format` flag value as a string suitable for
    /// passing through clap's `value_parser`. Returns `None` when the user
    /// hasn't configured one (caller keeps clap's auto-pick behaviour).
    pub fn format(&self) -> Option<&str> {
        self.defaults.format.as_deref()
    }

    pub fn snapshot(&self) -> Option<&str> {
        self.defaults.snapshot.as_deref()
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn empty_config_returns_defaults() {
        let c = Config::default();
        assert!(c.format().is_none());
        assert!(c.snapshot().is_none());
    }

    #[test]
    fn parses_minimal_toml() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            tmp.as_file(),
            "[defaults]\nformat = \"json\"\nsnapshot = \"@latest\""
        )
        .unwrap();
        let cfg = Config::load_from(tmp.path()).unwrap();
        assert_eq!(cfg.format(), Some("json"));
        assert_eq!(cfg.snapshot(), Some("@latest"));
    }

    #[test]
    fn ignores_unknown_sections() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(
            tmp.as_file(),
            "[defaults]\nformat = \"ndjson\"\n[totally_new_section]\nkey = 1"
        )
        .unwrap();
        let cfg = Config::load_from(tmp.path()).unwrap();
        assert_eq!(cfg.format(), Some("ndjson"));
    }

    #[test]
    fn malformed_toml_returns_error_with_path() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp.as_file(), "this is not toml = =").unwrap();
        let err = Config::load_from(tmp.path()).unwrap_err();
        let s = format!("{:#}", err);
        assert!(s.to_lowercase().contains("parse"));
    }

    #[test]
    fn env_overrides_file_values() {
        // Run in serial via a guard — env vars are process-global.
        let tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp.as_file(), "[defaults]\nformat = \"text\"").unwrap();
        // SAFETY: tests are single-threaded for env mutation via nextest's
        // per-test process model when run with `--test-threads=1`. nextest
        // runs each test in its own process by default, so env mutation is
        // isolated. If this becomes flaky, gate with `#[serial]`.
        std::env::set_var("DISKY_FORMAT", "json");
        let cfg = Config::load_from(tmp.path()).unwrap();
        std::env::remove_var("DISKY_FORMAT");
        assert_eq!(cfg.format(), Some("json"));
    }

    #[test]
    fn default_path_honours_env_override() {
        std::env::set_var("DISKY_CONFIG_PATH", "/tmp/custom-disky.toml");
        let p = Config::default_path().unwrap();
        std::env::remove_var("DISKY_CONFIG_PATH");
        assert_eq!(p, PathBuf::from("/tmp/custom-disky.toml"));
    }
}
