#!/usr/bin/env bash
# Fast metrics collector — runs cheap timings, captures git + system state,
# appends one JSONL line to metrics/build-timings.jsonl.
#
# Usage:
#   bash scripts/collect-metrics.sh         # fast tier only (~10s)
#   bash scripts/collect-metrics.sh --cold  # includes cargo clean + release build (~3min)
#
# Schema is intentionally additive: add new fields without bumping any
# version. Consumers read fields they know, ignore unknowns.

set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
mkdir -p metrics

COLD=0
if [ "${1:-}" = "--cold" ]; then COLD=1; fi

ts() { date -u +"%Y-%m-%dT%H:%M:%SZ"; }
now_ms() { python3 -c 'import time;print(int(time.time()*1000))'; }
elapsed_s() {
    local start_ms=$1
    local end_ms
    end_ms=$(now_ms)
    awk -v a="$start_ms" -v b="$end_ms" 'BEGIN { printf "%.3f", (b-a)/1000.0 }'
}

# ── git state ────────────────────────────────────────────────────────────
GIT_COMMIT=$(git rev-parse --short HEAD 2>/dev/null || echo "no-git")
GIT_COMMIT_FULL=$(git rev-parse HEAD 2>/dev/null || echo "no-git")
GIT_BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "no-git")
GIT_SUBJECT=$(git log -1 --pretty=%s 2>/dev/null || echo "")
GIT_AHEAD_MAIN=$(git rev-list --count main..HEAD 2>/dev/null || echo 0)
GIT_BEHIND_MAIN=$(git rev-list --count HEAD..main 2>/dev/null || echo 0)
GIT_DIRTY=$(test -z "$(git status --porcelain 2>/dev/null)" && echo false || echo true)

# ── tool versions ────────────────────────────────────────────────────────
RUSTC_VER=$(rustc --version 2>/dev/null | awk '{print $2}' || echo "?")
CARGO_VER=$(cargo --version 2>/dev/null | awk '{print $2}' || echo "?")

# ── machine ──────────────────────────────────────────────────────────────
OS_NAME=$(uname -s)
OS_REL=$(uname -r)
CPU_BRAND=$(sysctl -n machdep.cpu.brand_string 2>/dev/null || lscpu 2>/dev/null | awk -F: '/Model name/{print $2}' | sed 's/^ *//' || echo "?")
CORES=$(sysctl -n hw.ncpu 2>/dev/null || nproc 2>/dev/null || echo 0)
MEM_BYTES=$(sysctl -n hw.memsize 2>/dev/null || awk '/MemTotal/{print $2*1024}' /proc/meminfo 2>/dev/null || echo 0)
MEM_GB=$(awk -v b="$MEM_BYTES" 'BEGIN { printf "%.1f", b/1024/1024/1024 }')

# ── timings ──────────────────────────────────────────────────────────────
echo "→ cargo check (incremental)…" >&2
T=$(now_ms); cargo check --all-targets --offline 2>/dev/null >/dev/null || cargo check --all-targets >/dev/null 2>&1
T_CHECK=$(elapsed_s "$T")

echo "→ cargo nextest run --lib --test lib_integration…" >&2
T=$(now_ms); cargo nextest run --lib --test lib_integration >/dev/null 2>&1 || true
T_TEST=$(elapsed_s "$T")
TEST_COUNT=$(cargo nextest run --lib --test lib_integration 2>&1 | grep -oE '[0-9]+ tests run' | grep -oE '[0-9]+' | tail -1)
TEST_COUNT=${TEST_COUNT:-0}

echo "→ cargo build (debug incremental)…" >&2
T=$(now_ms); cargo build --bins >/dev/null 2>&1
T_BUILD_DEBUG=$(elapsed_s "$T")

T_BUILD_RELEASE_COLD="null"
T_BUILD_RELEASE_INC="null"
SIZE_DISKY="null"
SIZE_DISKY_MCP="null"
if [ "$COLD" = "1" ]; then
    echo "→ cargo clean + release (cold)…" >&2
    cargo clean >/dev/null 2>&1
    T=$(now_ms); cargo build --release --bins >/dev/null 2>&1
    T_BUILD_RELEASE_COLD=$(elapsed_s "$T")
fi

if [ -f "target/release/disky" ]; then
    echo "→ cargo build --release (incremental)…" >&2
    T=$(now_ms); cargo build --release --bins >/dev/null 2>&1
    T_BUILD_RELEASE_INC=$(elapsed_s "$T")
    SIZE_DISKY=$(stat -f%z target/release/disky 2>/dev/null || stat -c%s target/release/disky 2>/dev/null || echo null)
fi

# ── LOC ──────────────────────────────────────────────────────────────────
LOC_SRC=$(find src -name '*.rs' 2>/dev/null | xargs wc -l 2>/dev/null | awk '/total$/{print $1}' | tail -1)
LOC_TESTS=$(find tests -name '*.rs' 2>/dev/null | xargs wc -l 2>/dev/null | awk '/total$/{print $1}' | tail -1)
LOC_BENCHES=$(find benches -name '*.rs' 2>/dev/null | xargs wc -l 2>/dev/null | awk '/total$/{print $1}' | tail -1)
LOC_SRC=${LOC_SRC:-0}; LOC_TESTS=${LOC_TESTS:-0}; LOC_BENCHES=${LOC_BENCHES:-0}

# ── emit JSONL ──────────────────────────────────────────────────────────
COLD_BOOL=$([ "$COLD" = "1" ] && echo true || echo false)
GIT_SUBJECT_JSON=$(printf '%s' "$GIT_SUBJECT" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))')
CPU_JSON=$(printf '%s' "$CPU_BRAND" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read().strip()))')

DIRTY_PY=$([ "$GIT_DIRTY" = "true" ] && echo True || echo False)
COLD_PY=$([ "$COLD" = "1" ] && echo True || echo False)

python3 - <<PY >> metrics/build-timings.jsonl
import json
def n(v):
    if v in (None, "null", ""):
        return None
    try:
        return int(v)
    except ValueError:
        try:
            return float(v)
        except ValueError:
            return v
entry = {
    "ts": "$(ts)",
    "commit": "$GIT_COMMIT",
    "commit_full": "$GIT_COMMIT_FULL",
    "commit_subject": $GIT_SUBJECT_JSON,
    "branch": "$GIT_BRANCH",
    "dirty": $DIRTY_PY,
    "ahead_of": {"main": $GIT_AHEAD_MAIN},
    "behind": {"main": $GIT_BEHIND_MAIN},
    "rustc": "$RUSTC_VER",
    "cargo": "$CARGO_VER",
    "machine": {
        "os": "$OS_NAME $OS_REL",
        "cpu": $CPU_JSON,
        "cores": $CORES,
        "mem_gb": $MEM_GB
    },
    "timings_s": {
        "cargo_check_inc": n("$T_CHECK"),
        "nextest_fast_tier": n("$T_TEST"),
        "build_debug_inc": n("$T_BUILD_DEBUG"),
        "build_release_inc": n("$T_BUILD_RELEASE_INC"),
        "build_release_cold": n("$T_BUILD_RELEASE_COLD")
    },
    "binary_size_bytes": {"disky": n("$SIZE_DISKY")},
    "tests": {"fast_tier_total": n("$TEST_COUNT")},
    "loc": {"src": $LOC_SRC, "tests": $LOC_TESTS, "benches": $LOC_BENCHES},
    "cold": $COLD_PY
}
print(json.dumps(entry, sort_keys=True))
PY

# Also write current snapshot (overwrites). Easier for report rendering than parsing JSONL tail.
tail -n 1 metrics/build-timings.jsonl > metrics/latest.json

# Git snapshot (overwrites)
cat > metrics/git-snapshot.json <<EOF
{
  "ts": "$(ts)",
  "commit": "$GIT_COMMIT",
  "commit_full": "$GIT_COMMIT_FULL",
  "subject": $(printf '%s' "$GIT_SUBJECT" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))'),
  "branch": "$GIT_BRANCH",
  "dirty": $GIT_DIRTY,
  "ahead_of_main": $GIT_AHEAD_MAIN,
  "behind_main": $GIT_BEHIND_MAIN,
  "author": $(git log -1 --pretty=%an 2>/dev/null | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read().strip()))'),
  "author_date": $(git log -1 --pretty=%aI 2>/dev/null | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read().strip()))')
}
EOF

echo "✓ metrics appended to metrics/build-timings.jsonl"
echo "✓ metrics/latest.json + metrics/git-snapshot.json updated"
