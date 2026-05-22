# disky dev recipes — invoke via `just <recipe>`.
# Fast loop: `just test` runs unit + lib_integration only (<10s target).

default: ci

check:
    cargo check --all-targets

fmt:
    cargo fmt

fmt-check:
    cargo fmt --check

lint:
    cargo clippy --all-targets -- -D warnings

# Fast tier — no binary build, no shell-out. Sub-10s target.
test:
    cargo nextest run --lib --test lib_integration

test-full:
    cargo nextest run

# CLI/MCP tier — requires release binary build.
test-cli:
    cargo build --release
    cargo nextest run --test agentic --test mcp_protocol

bench:
    cargo bench

bench-cmp:
    bash scripts/bench-competitors.sh

mcp:
    cargo run --bin disky-mcp

install-fast:
    cargo install --path . --debug --bins

ci: fmt-check lint test test-cli

# Append a metrics line to metrics/build-timings.jsonl (~10s)
metrics:
    bash scripts/collect-metrics.sh

# Cold-build metrics (cargo clean + release rebuild). ~3min. Tag-time only.
metrics-cold:
    bash scripts/collect-metrics.sh --cold

# Competitor benchmark on 10k-file synthetic tree. Updates metrics/competitors-latest.json.
bench-cmp-10k:
    bash scripts/bench-competitors.sh 10000

# Reset baseline to current measurements. Commits the JSON deliberately.
metrics-reseed-baseline:
    bash scripts/collect-metrics.sh
    cp metrics/latest.json metrics/baseline.json
    @echo "→ baseline updated. Review with: jq . metrics/baseline.json"
