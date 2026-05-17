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
}

impl DiskyError {
    pub fn new(code: ExitCode, title: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            title,
            detail: detail.into(),
            retryable: matches!(code, ExitCode::LockHeld | ExitCode::Io),
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

/// Classify an `anyhow::Error` into a `DiskyError`. Best-effort — falls back to
/// `Generic` so behaviour matches today, but specific underlying errors get
/// mapped to stable codes.
pub fn classify(err: anyhow::Error) -> DiskyError {
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
