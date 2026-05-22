//! Tiny duration parser. Accepts shapes like `30d`, `2w`, `6mo`, `1y`,
//! `48h`, `90m`. No mixed units; whole numbers only.

use anyhow::{anyhow, Result};

pub fn parse_seconds(spec: &str) -> Result<i64> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Err(anyhow!("empty duration"));
    }
    // Split into numeric prefix + unit suffix.
    let split_at = spec
        .find(|c: char| !c.is_ascii_digit())
        .ok_or_else(|| anyhow!("missing unit in duration '{}'", spec))?;
    let (num_s, unit) = spec.split_at(split_at);
    let n: i64 = num_s
        .parse()
        .map_err(|_| anyhow!("invalid number in duration '{}'", spec))?;
    let secs_per_unit = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        "d" => 86400,
        "w" => 7 * 86400,
        "mo" => 30 * 86400,
        "y" => 365 * 86400,
        u => {
            return Err(anyhow!(
                "unknown duration unit '{}' (try s/m/h/d/w/mo/y)",
                u
            ))
        }
    };
    Ok(n * secs_per_unit)
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_forms() {
        assert_eq!(parse_seconds("30s").unwrap(), 30);
        assert_eq!(parse_seconds("5m").unwrap(), 300);
        assert_eq!(parse_seconds("2h").unwrap(), 7200);
        assert_eq!(parse_seconds("7d").unwrap(), 604800);
        assert_eq!(parse_seconds("1w").unwrap(), 604800);
        assert_eq!(parse_seconds("6mo").unwrap(), 6 * 30 * 86400);
        assert_eq!(parse_seconds("1y").unwrap(), 365 * 86400);
    }

    #[test]
    fn rejects_invalid_input() {
        assert!(parse_seconds("").is_err());
        assert!(parse_seconds("abc").is_err());
        assert!(parse_seconds("30").is_err()); // no unit
        assert!(parse_seconds("30x").is_err()); // bad unit
        assert!(parse_seconds("30 days").is_err()); // word units not supported
    }

    #[test]
    fn handles_whitespace() {
        assert_eq!(parse_seconds("  7d  ").unwrap(), 604800);
    }
}
