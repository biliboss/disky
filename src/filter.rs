//! Predicate engine for `disky filter`. Tiny hand-rolled tokenizer +
//! evaluator over a small DSL: `<field> <op> <literal>` connected by
//! optional `AND`.
//!
//! Supported fields: `size` (u64), `ext` (string), `name` (string),
//! `path` (string).
//! Supported ops: `=`, `!=`, `>`, `<`, `>=`, `<=`, `LIKE`.
//! Supported literals: bare integer, integer with size suffix
//! (`1KB`/`1MB`/`1GB`/`1TB`, both binary and decimal accepted —
//! we use binary `1024^n` to match `humansize`'s defaults), quoted string.

use anyhow::{anyhow, bail, Result};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
enum Op {
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
    Like,
}

#[derive(Debug, Clone)]
enum Lit {
    Int(i64),
    Str(String),
}

#[derive(Debug, Clone)]
struct Pred {
    field: String,
    op: Op,
    lit: Lit,
}

#[derive(Debug, Clone)]
pub struct Predicate {
    parts: Vec<Pred>,
}

impl Predicate {
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();
        if s.is_empty() {
            return Ok(Predicate { parts: vec![] });
        }
        let mut parts = Vec::new();
        // Split on case-insensitive " AND " — naive but adequate.
        for chunk in split_and(s) {
            parts.push(parse_pred(chunk.trim())?);
        }
        Ok(Predicate { parts })
    }

    /// Evaluate against a JSON record. Missing fields evaluate false
    /// (except `!=` which becomes true) — same SQL three-valued logic
    /// is too heavy for a CLI filter.
    pub fn matches(&self, rec: &Value) -> bool {
        self.parts.iter().all(|p| eval_pred(p, rec))
    }
}

fn split_and(s: &str) -> Vec<&str> {
    // Find " AND " / " and " boundaries that aren't inside quotes.
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut in_quote = false;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c == '"' || c == '\'' {
            in_quote = !in_quote;
            i += 1;
            continue;
        }
        if !in_quote && i + 5 <= bytes.len() {
            let win = &s[i..i + 5];
            if win.eq_ignore_ascii_case(" and ") {
                out.push(&s[start..i]);
                start = i + 5;
                i += 5;
                continue;
            }
        }
        i += 1;
    }
    out.push(&s[start..]);
    out
}

fn parse_pred(s: &str) -> Result<Pred> {
    // Try operators in order of length (multi-char first).
    let (field, op, rest) = if let Some(idx) = find_op(s, ">=") {
        (s[..idx].trim().to_string(), Op::Ge, &s[idx + 2..])
    } else if let Some(idx) = find_op(s, "<=") {
        (s[..idx].trim().to_string(), Op::Le, &s[idx + 2..])
    } else if let Some(idx) = find_op(s, "!=") {
        (s[..idx].trim().to_string(), Op::Ne, &s[idx + 2..])
    } else if let Some(idx) = find_op_case_insensitive(s, " LIKE ") {
        (s[..idx].trim().to_string(), Op::Like, &s[idx + 6..])
    } else if let Some(idx) = find_op(s, "=") {
        (s[..idx].trim().to_string(), Op::Eq, &s[idx + 1..])
    } else if let Some(idx) = find_op(s, ">") {
        (s[..idx].trim().to_string(), Op::Gt, &s[idx + 1..])
    } else if let Some(idx) = find_op(s, "<") {
        (s[..idx].trim().to_string(), Op::Lt, &s[idx + 1..])
    } else {
        bail!("predicate '{}' has no operator", s);
    };
    if field.is_empty() {
        bail!("predicate '{}' has no field", s);
    }
    let lit = parse_lit(rest.trim())?;
    Ok(Pred { field, op, lit })
}

fn find_op(s: &str, op: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut in_quote = false;
    for i in 0..bytes.len() {
        let c = bytes[i] as char;
        if c == '"' || c == '\'' {
            in_quote = !in_quote;
            continue;
        }
        if !in_quote && s[i..].starts_with(op) {
            return Some(i);
        }
    }
    None
}

fn find_op_case_insensitive(s: &str, op: &str) -> Option<usize> {
    let lower = s.to_ascii_lowercase();
    let op_lower = op.to_ascii_lowercase();
    lower.find(&op_lower)
}

fn parse_lit(s: &str) -> Result<Lit> {
    if s.is_empty() {
        bail!("empty literal");
    }
    // Quoted string
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        if s.len() < 2 {
            bail!("malformed string literal");
        }
        return Ok(Lit::Str(s[1..s.len() - 1].to_string()));
    }
    // Size suffix? 1KB, 1MB, 1GB, 1TB, 1KiB, 1MiB...
    let upper = s.to_uppercase();
    let multipliers = [
        ("TIB", 1024u64.pow(4)),
        ("GIB", 1024u64.pow(3)),
        ("MIB", 1024u64.pow(2)),
        ("KIB", 1024),
        ("TB", 1024u64.pow(4)),
        ("GB", 1024u64.pow(3)),
        ("MB", 1024u64.pow(2)),
        ("KB", 1024),
        ("B", 1),
    ];
    for (suf, mult) in multipliers.iter() {
        if let Some(num_s) = upper.strip_suffix(suf) {
            let n: i64 = num_s
                .trim()
                .parse()
                .map_err(|_| anyhow!("bad size literal '{}'", s))?;
            return Ok(Lit::Int(n.saturating_mul(*mult as i64)));
        }
    }
    // Plain integer
    if let Ok(n) = s.parse::<i64>() {
        return Ok(Lit::Int(n));
    }
    // Unquoted bare identifier — treat as string
    Ok(Lit::Str(s.to_string()))
}

fn eval_pred(p: &Pred, rec: &Value) -> bool {
    let field_value = rec.get(&p.field);
    match (&p.lit, field_value) {
        (Lit::Int(want), Some(v)) => {
            let got = match v {
                Value::Number(n) => n.as_i64().unwrap_or(0),
                _ => return p.op == Op::Ne,
            };
            cmp_int(&p.op, got, *want)
        }
        (Lit::Str(want), Some(v)) => {
            let got = match v {
                Value::String(s) => s.as_str(),
                _ => return p.op == Op::Ne,
            };
            cmp_str(&p.op, got, want)
        }
        (_, None) => p.op == Op::Ne,
    }
}

fn cmp_int(op: &Op, got: i64, want: i64) -> bool {
    match op {
        Op::Eq => got == want,
        Op::Ne => got != want,
        Op::Gt => got > want,
        Op::Lt => got < want,
        Op::Ge => got >= want,
        Op::Le => got <= want,
        Op::Like => false,
    }
}

fn cmp_str(op: &Op, got: &str, want: &str) -> bool {
    match op {
        Op::Eq => got == want,
        Op::Ne => got != want,
        Op::Like => like_match(got, want),
        _ => false,
    }
}

/// SQL LIKE-style: `%` = any chars, `_` = single char. Anchored at both ends.
fn like_match(haystack: &str, pattern: &str) -> bool {
    let mut regex = String::from("^");
    for c in pattern.chars() {
        match c {
            '%' => regex.push_str(".*"),
            '_' => regex.push('.'),
            '.' | '+' | '*' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '^' | '$' | '\\' => {
                regex.push('\\');
                regex.push(c);
            }
            _ => regex.push(c),
        }
    }
    regex.push('$');
    // No regex crate dep — fall back to simple wildcard match for now.
    // For correctness on patterns without `_`, split on `%` and check
    // prefix/contains/suffix sequence.
    if !pattern.contains('_') {
        let parts: Vec<&str> = pattern.split('%').collect();
        return like_wildcard(haystack, &parts);
    }
    // Fallback for patterns with `_`: char-by-char match.
    like_char_match(haystack, pattern)
}

fn like_wildcard(s: &str, parts: &[&str]) -> bool {
    let n = parts.len();
    if n == 1 {
        return s == parts[0];
    }
    // Must start with parts[0] (unless empty), end with parts[n-1] (unless empty),
    // and contain middle parts in order.
    let mut s = s;
    if !parts[0].is_empty() {
        if !s.starts_with(parts[0]) {
            return false;
        }
        s = &s[parts[0].len()..];
    }
    if !parts[n - 1].is_empty() && !s.ends_with(parts[n - 1]) {
        return false;
    }
    if !parts[n - 1].is_empty() {
        s = &s[..s.len() - parts[n - 1].len()];
    }
    for mid in &parts[1..n - 1] {
        if mid.is_empty() {
            continue;
        }
        match s.find(mid) {
            Some(idx) => s = &s[idx + mid.len()..],
            None => return false,
        }
    }
    true
}

fn like_char_match(s: &str, pat: &str) -> bool {
    let sc: Vec<char> = s.chars().collect();
    let pc: Vec<char> = pat.chars().collect();
    fn go(sc: &[char], pc: &[char]) -> bool {
        if pc.is_empty() {
            return sc.is_empty();
        }
        match pc[0] {
            '%' => (0..=sc.len()).any(|i| go(&sc[i..], &pc[1..])),
            '_' => !sc.is_empty() && go(&sc[1..], &pc[1..]),
            c => !sc.is_empty() && sc[0] == c && go(&sc[1..], &pc[1..]),
        }
    }
    go(&sc, &pc)
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rec(path: &str, size: i64, ext: &str) -> Value {
        json!({"path": path, "size": size, "ext": ext})
    }

    #[test]
    fn parses_simple_int_predicate() {
        let p = Predicate::parse("size > 1024").unwrap();
        assert!(p.matches(&rec("/a", 2048, "bin")));
        assert!(!p.matches(&rec("/a", 100, "bin")));
    }

    #[test]
    fn parses_size_suffix() {
        let p = Predicate::parse("size > 1GB").unwrap();
        assert!(p.matches(&rec("/a", 2 * 1024 * 1024 * 1024, "bin")));
        assert!(!p.matches(&rec("/a", 500, "bin")));
    }

    #[test]
    fn parses_string_equality() {
        let p = Predicate::parse("ext = 'log'").unwrap();
        assert!(p.matches(&rec("/a", 100, "log")));
        assert!(!p.matches(&rec("/a", 100, "bin")));
    }

    #[test]
    fn ne_returns_true_on_missing_field() {
        let p = Predicate::parse("missing != 'x'").unwrap();
        assert!(p.matches(&rec("/a", 100, "log")));
    }

    #[test]
    fn parses_and_chain() {
        let p = Predicate::parse("size > 1KB AND ext = 'log'").unwrap();
        assert!(p.matches(&rec("/big.log", 4096, "log")));
        assert!(!p.matches(&rec("/big.bin", 4096, "bin")));
        assert!(!p.matches(&rec("/small.log", 100, "log")));
    }

    #[test]
    fn like_with_percent_wildcards() {
        let p = Predicate::parse("path LIKE '%.log'").unwrap();
        assert!(p.matches(&rec("/var/foo.log", 0, "log")));
        assert!(!p.matches(&rec("/var/foo.bin", 0, "bin")));

        let p2 = Predicate::parse("path LIKE '/Users/%/Library/%'").unwrap();
        assert!(p2.matches(&rec("/Users/me/Library/Caches", 0, "")));
        assert!(!p2.matches(&rec("/etc/hosts", 0, "")));
    }

    #[test]
    fn empty_predicate_matches_everything() {
        let p = Predicate::parse("").unwrap();
        assert!(p.matches(&rec("/a", 100, "log")));
    }

    #[test]
    fn rejects_predicate_without_operator() {
        assert!(Predicate::parse("just words").is_err());
    }

    #[test]
    fn parses_kib_alias() {
        let p = Predicate::parse("size >= 1KiB").unwrap();
        assert!(p.matches(&rec("/a", 1024, "bin")));
        assert!(!p.matches(&rec("/a", 1023, "bin")));
    }
}
