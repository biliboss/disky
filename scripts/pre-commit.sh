#!/usr/bin/env bash
# disky pre-commit hook — fmt + clippy gate on staged Rust changes.
# Install via: bash scripts/install-hooks.sh
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

# Only check Rust changes
if ! git diff --cached --name-only | grep -qE '\.(rs|toml)$'; then
    exit 0
fi

echo "→ cargo fmt --check"
cargo fmt --check

echo "→ cargo clippy --all-targets -- -D warnings"
cargo clippy --all-targets -- -D warnings

echo "✓ pre-commit gates passed"
