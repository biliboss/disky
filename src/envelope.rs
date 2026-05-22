//! Parse a disky JSON envelope from any reader (stdin, file, byte slice).
//!
//! Keeps `--json-input` chain mode honest: every command that consumes
//! prior output validates `schema_version` + `kind` whitelist before
//! transforming records.

use anyhow::{anyhow, bail, Result};
use serde_json::Value;
use std::io::Read;

use crate::query::SCHEMA_VERSION;

#[derive(Debug, Clone)]
pub struct Envelope {
    pub schema_version: u32,
    pub kind: String,
    pub records: Vec<Value>,
}

/// Read a full JSON envelope (`{schema_version, kind, records}`) from a
/// reader. Buffered — input must fit in memory.
pub fn parse_json<R: Read>(mut r: R) -> Result<Envelope> {
    let mut buf = String::new();
    r.read_to_string(&mut buf)?;
    let v: Value =
        serde_json::from_str(buf.trim()).map_err(|e| anyhow!("stdin is not valid JSON: {}", e))?;
    parse_value(v)
}

pub fn parse_value(v: Value) -> Result<Envelope> {
    let obj = v
        .as_object()
        .ok_or_else(|| anyhow!("envelope must be a JSON object"))?;
    let schema_version = obj
        .get("schema_version")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("missing 'schema_version' (u32)"))? as u32;
    if schema_version != SCHEMA_VERSION {
        bail!(
            "envelope schema_version={} is incompatible (expected {})",
            schema_version,
            SCHEMA_VERSION
        );
    }
    let kind = obj
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing 'kind' (string)"))?
        .to_string();
    let records = obj
        .get("records")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(Envelope {
        schema_version,
        kind,
        records,
    })
}

/// Reject envelopes whose `kind` isn't in the whitelist.
pub fn require_kind(env: &Envelope, allowed: &[&str]) -> Result<()> {
    if allowed.contains(&env.kind.as_str()) {
        Ok(())
    } else {
        Err(anyhow!(
            "envelope kind '{}' not accepted here (expected one of: {})",
            env.kind,
            allowed.join(", ")
        ))
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn make(kind: &str, records: Value) -> String {
        format!(
            r#"{{"schema_version":1,"kind":"{}","records":{}}}"#,
            kind, records
        )
    }

    #[test]
    fn parses_minimal_envelope() {
        let env = parse_json(Cursor::new(make("top", serde_json::json!([])))).unwrap();
        assert_eq!(env.schema_version, 1);
        assert_eq!(env.kind, "top");
        assert!(env.records.is_empty());
    }

    #[test]
    fn carries_records_through() {
        let env = parse_json(Cursor::new(make(
            "top",
            serde_json::json!([{"path":"/a","size":1024}]),
        )))
        .unwrap();
        assert_eq!(env.records.len(), 1);
        assert_eq!(env.records[0]["path"], "/a");
    }

    #[test]
    fn rejects_mismatched_schema_version() {
        let s = r#"{"schema_version":999,"kind":"top","records":[]}"#;
        let err = parse_json(Cursor::new(s)).unwrap_err();
        assert!(format!("{:#}", err).contains("incompatible"));
    }

    #[test]
    fn rejects_non_object_input() {
        let err = parse_json(Cursor::new("[]")).unwrap_err();
        assert!(format!("{:#}", err).contains("object"));
    }

    #[test]
    fn rejects_invalid_json() {
        let err = parse_json(Cursor::new("not json")).unwrap_err();
        assert!(format!("{:#}", err).contains("not valid JSON"));
    }

    #[test]
    fn require_kind_accepts_whitelisted() {
        let env = parse_json(Cursor::new(make("top", serde_json::json!([])))).unwrap();
        assert!(require_kind(&env, &["top", "find"]).is_ok());
        assert!(require_kind(&env, &["dirs", "ext"]).is_err());
    }
}
