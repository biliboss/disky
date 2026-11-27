#!/usr/bin/env bash
# The full regression suite: every check CI runs, in one command.
#
#   scripts/regression.sh          # everything
#   scripts/regression.sh engine   # Rust engine + CLI only
#   scripts/regression.sh gui      # desktop GUI only
#
# Engine: rustfmt, clippy, unit + integration + CLI (agentic) tests.
# GUI:    vitest (screens, flows, contract), token/boundary/control lints,
#         tsc, Playwright (pixel match vs the design snapshot, axe, keyboard,
#         every screen's flows), and the Tauri crate's fmt, clippy and tests.
#
# Exits non-zero on the first failing stage, naming it. Tests never touch the
# real ~/.Trash, history or preferences (they run under temp HOMEs).
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
what="${1:-all}"

stage() { printf '\n== %s\n' "$1"; }

engine() {
  cd "$root"
  stage "engine: fmt";    cargo fmt --check
  stage "engine: clippy"; cargo clippy --all-targets -- -D warnings
  stage "engine: tests";  cargo test --release
}

gui() {
  cd "$root/gui"
  [ -d node_modules ] || npm ci
  stage "gui: unit + flow tests (vitest)"; npm test
  stage "gui: lints (tokens, boundary, controls)"; npm run lint:tokens
  stage "gui: types (tsc)"; npx tsc --noEmit -p .
  stage "gui: visual + a11y + keyboard (playwright)"; npx playwright test
  cd "$root/gui/src-tauri"
  stage "tauri: fmt";    cargo fmt --check
  stage "tauri: clippy"; cargo clippy --all-targets -- -D warnings
  stage "tauri: tests";  cargo test
}

case "$what" in
  engine) engine ;;
  gui) gui ;;
  all) engine; gui ;;
  *) echo "usage: $0 [all|engine|gui]" >&2; exit 2 ;;
esac
printf '\nregression suite passed (%s)\n' "$what"
