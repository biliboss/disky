# disky — Disk Analyzer

macOS disk analyzer CLI in Rust.

## Stack

- **Traversal**: `jwalk` — parallel, uses `getattrlistbulk` syscall (fastest on macOS)
- **Storage**: `duckdb` embedded — columnar, analytical queries, persistent
- **Threading**: `rayon` — batch insert pipeline

## Architecture

```
jwalk (parallel) → channel → batch (1000 entries) → DuckDB INSERT
```

## Cargo deps

```toml
jwalk = "0.8"
duckdb = "1.1"
rayon = "1.10"
```

## Next steps

- [ ] Scaffold `cargo new disky`
- [ ] Define DuckDB schema (path, name, ext, size, mtime, kind)
- [ ] Traversal + batch insert core
- [ ] CLI queries (top files, by ext, by dir)
