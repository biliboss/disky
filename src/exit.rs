use std::fmt;

/// Stable exit-code taxonomy. Documented in AGENTS.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
#[allow(dead_code)] // Usage emitted by clap directly; PartialScan reserved for F6.
pub enum ExitCode {
    Ok = 0,
    Generic = 1,
    Usage = 2,
    Io = 3,
    NotFound = 4,
    PartialScan = 5,
    LockHeld = 6,
}

impl ExitCode {
    /// Stable identifier for RFC 9457 `type` URIs and JSON error payloads.
    pub fn slug(self) -> &'static str {
        match self {
            ExitCode::Ok => "ok",
            ExitCode::Generic => "generic",
            ExitCode::Usage => "usage",
            ExitCode::Io => "io",
            ExitCode::NotFound => "not-found",
            ExitCode::PartialScan => "partial-scan",
            ExitCode::LockHeld => "lock-held",
        }
    }
}

/// Typed error surface. Wraps an underlying `anyhow::Error` plus a stable code
/// so the agent can dispatch without string-matching messages.
#[derive(Debug)]
pub struct DiskyError {
    pub code: ExitCode,
    pub title: &'static str,
    pub detail: String,
    pub retryable: bool,
    /// Per-error instance identifier (RFC 9457 `instance` field) so agents can
    /// correlate stderr payloads with logs.
    pub instance: String,
}

fn fresh_instance() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| (d.as_secs(), d.subsec_nanos()))
        .unwrap_or((0, 0));
    format!("disky-{}-{:09}-{}", t.0, t.1, n)
}

impl DiskyError {
    pub fn new(code: ExitCode, title: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            title,
            detail: detail.into(),
            retryable: matches!(code, ExitCode::LockHeld | ExitCode::Io),
            instance: fresh_instance(),
        }
    }

    pub fn not_found(detail: impl Into<String>) -> Self {
        Self::new(ExitCode::NotFound, "snapshot not found", detail)
    }

    pub fn io(detail: impl Into<String>) -> Self {
        Self::new(ExitCode::Io, "i/o error", detail)
    }

    pub fn lock_held(detail: impl Into<String>) -> Self {
        Self::new(ExitCode::LockHeld, "snapshot locked", detail)
    }

    pub fn generic(detail: impl Into<String>) -> Self {
        Self::new(ExitCode::Generic, "error", detail)
    }
}

impl fmt::Display for DiskyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.title, self.detail)
    }
}

impl std::error::Error for DiskyError {}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;

    #[test]
    fn slug_table_is_stable() {
        // Each code must keep its slug — RFC 9457 type URIs are a public contract.
        assert_eq!(ExitCode::Ok.slug(), "ok");
        assert_eq!(ExitCode::Generic.slug(), "generic");
        assert_eq!(ExitCode::Usage.slug(), "usage");
        assert_eq!(ExitCode::Io.slug(), "io");
        assert_eq!(ExitCode::NotFound.slug(), "not-found");
        assert_eq!(ExitCode::PartialScan.slug(), "partial-scan");
        assert_eq!(ExitCode::LockHeld.slug(), "lock-held");
    }

    #[test]
    fn retryable_flag_set_only_for_transient_codes() {
        assert!(DiskyError::lock_held("x").retryable);
        assert!(DiskyError::io("x").retryable);
        assert!(!DiskyError::not_found("x").retryable);
        assert!(!DiskyError::generic("x").retryable);
    }

    #[test]
    fn classify_passes_through_existing_disky_error() {
        let original = anyhow::Error::from(DiskyError::lock_held("held"));
        let classified = classify(original);
        assert_eq!(classified.code, ExitCode::LockHeld);
        assert!(classified.retryable);
    }

    #[test]
    fn classify_io_not_found() {
        let io = std::io::Error::new(std::io::ErrorKind::NotFound, "boom");
        let classified = classify(anyhow::Error::from(io));
        assert_eq!(classified.code, ExitCode::NotFound);
    }

    #[test]
    fn classify_io_permission_denied_maps_to_io() {
        let io = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let classified = classify(anyhow::Error::from(io));
        assert_eq!(classified.code, ExitCode::Io);
    }

    #[test]
    fn classify_lock_held_via_message() {
        let err = anyhow::anyhow!("Could not set lock on file");
        assert_eq!(classify(err).code, ExitCode::LockHeld);
    }

    #[test]
    fn classify_generic_fallback() {
        let err = anyhow::anyhow!("something unexpected");
        assert_eq!(classify(err).code, ExitCode::Generic);
    }

    #[test]
    fn instance_is_unique_per_error() {
        let a = DiskyError::generic("x");
        let b = DiskyError::generic("y");
        assert_ne!(a.instance, b.instance);
        assert!(a.instance.starts_with("disky-"));
    }

    #[test]
    fn display_includes_title_and_detail() {
        let e = DiskyError::not_found("/missing.db");
        let s = format!("{}", e);
        assert!(s.contains("snapshot not found"));
        assert!(s.contains("/missing.db"));
    }
}

/// Classify an `anyhow::Error` into a `DiskyError`. Best-effort — falls back to
/// `Generic` so behaviour matches today, but specific underlying errors get
/// mapped to stable codes.
pub fn classify(err: anyhow::Error) -> DiskyError {
    if err.downcast_ref::<DiskyError>().is_some() {
        return err.downcast::<DiskyError>().unwrap();
    }

    let msg = format!("{:#}", err);
    let lower = msg.to_lowercase();

    if let Some(io) = err.downcast_ref::<std::io::Error>() {
        return match io.kind() {
            std::io::ErrorKind::NotFound => DiskyError::not_found(msg),
            std::io::ErrorKind::PermissionDenied => DiskyError::io(msg),
            _ => DiskyError::io(msg),
        };
    }

    if lower.contains("could not set lock")
        || lower.contains("database is locked")
        || lower.contains("conflicting lock")
    {
        DiskyError::lock_held(msg)
    } else if lower.contains("no such file") || lower.contains("not found") {
        DiskyError::not_found(msg)
    } else if lower.contains("permission denied")
        || lower.contains("read-only file system")
        || lower.contains("cannot open file")
        || lower.contains("io error")
    {
        DiskyError::io(msg)
    } else {
        DiskyError::generic(msg)
    }
}
