#!/usr/bin/env bash
# Competitor benchmark via hyperfine. Compares disky scan against
# dust / dua / gdu / pdu / du on a synthetic tree.
#
# Output: metrics/competitors-latest.json (machine-readable)
#       + console table
#
# Prereqs: brew install dust dua-cli gdu pdu hyperfine
#
# Usage:
#   bash scripts/bench-competitors.sh             # 10k-file tree
#   bash scripts/bench-competitors.sh 100000      # 100k-file tree (slower)

set -eo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
mkdir -p metrics

N=${1:-10000}
TREE=/tmp/disky-bench-tree-$N

# Ensure tree exists
bash scripts/mk-bigtree.sh "$N" "$TREE"

# Ensure disky release binary exists
if [ ! -x target/release/disky ]; then
    echo "→ building disky --release (first run)"
    cargo build --release --bin disky >/dev/null
fi

# Discover competitor binaries — parallel arrays (works on macOS bash 3.2)
NAMES=(disky dust dua gdu du)
CMDS=(
    "$ROOT/target/release/disky scan $TREE --db /tmp/disky-bench.db"
    "dust -d 0 $TREE"
    "dua aggregate $TREE"
    "gdu-go -np $TREE"
    "du -sh $TREE"
)

if ! command -v hyperfine >/dev/null 2>&1; then
    echo "error: hyperfine not installed (brew install hyperfine)" >&2
    exit 1
fi

# Filter to available binaries
ACTIVE_NAMES=()
ACTIVE_CMDS=()
for i in "${!NAMES[@]}"; do
    name="${NAMES[$i]}"
    cmd="${CMDS[$i]}"
    bin=$(echo "$cmd" | awk '{print $1}')
    if [ "$name" = "disky" ] || command -v "$bin" >/dev/null 2>&1; then
        ACTIVE_NAMES+=("$name")
        ACTIVE_CMDS+=("$cmd")
    fi
done

OUT=metrics/competitors-latest.json

# Build hyperfine args
HF_ARGS=(--warmup 2 --runs 5 --export-json "$OUT")
for i in "${!ACTIVE_NAMES[@]}"; do
    HF_ARGS+=(--command-name "${ACTIVE_NAMES[$i]}" "${ACTIVE_CMDS[$i]}")
done

hyperfine "${HF_ARGS[@]}"

# Annotate the file with tree size and timestamp.
python3 - <<PY
import json, time, os
p = "$OUT"
with open(p) as f: data = json.load(f)
data["tree_path"] = "$TREE"
data["tree_files"] = int(os.popen(f"find $TREE -type f | wc -l").read().strip())
data["tree_bytes"] = int(os.popen(f"du -sk $TREE").read().split()[0]) * 1024
data["ts"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
with open(p, "w") as f: json.dump(data, f, indent=2)
PY

echo "✓ competitor benchmark → $OUT"
