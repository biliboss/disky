#!/usr/bin/env bash
# Deterministic synthetic tree for competitor benchmarks.
# Generates `/tmp/disky-bench-tree-<N>/` with N files at depth ≤ 4.
#
# Usage:
#   bash scripts/mk-bigtree.sh 10000           # 10k files in /tmp/disky-bench-tree-10000
#   bash scripts/mk-bigtree.sh 10000 /tmp/foo  # custom dir
#
# Idempotent: if the target already has exactly N files, no-op.

set -euo pipefail

N=${1:-10000}
DIR=${2:-/tmp/disky-bench-tree-$N}

existing=0
if [ -d "$DIR" ]; then
    existing=$(find "$DIR" -type f 2>/dev/null | wc -l | tr -d ' ')
    if [ "$existing" = "$N" ]; then
        echo "✓ $DIR already has $N files"
        exit 0
    fi
    rm -rf "$DIR"
fi

mkdir -p "$DIR"
echo "→ generating $N files in ${DIR}"

# Distribute across depth 1-4 sub-directories. Use seq + xargs for speed.
# File sizes vary: most tiny, a few medium, one large — reflects a real repo.
seq 1 "$N" | while read -r i; do
    depth=$(( (i % 4) + 1 ))
    dir="$DIR"
    for d in $(seq 1 $depth); do
        dir="$dir/d$(( (i / (d * 17)) % 7 ))"
    done
    mkdir -p "$dir"
    # Vary content size to approximate a real tree.
    case $(( i % 50 )) in
        0)   head -c 1048576 /dev/urandom > "$dir/big_$i.bin" ;;   # 1 MiB every 50th
        1|2) head -c 65536 /dev/urandom > "$dir/mid_$i.bin" ;;     # 64 KiB
        *)   head -c 1024 /dev/urandom > "$dir/small_$i.txt" ;;    # 1 KiB
    esac
done

actual=$(find "$DIR" -type f | wc -l | tr -d ' ')
total_bytes=$(du -sk "$DIR" 2>/dev/null | awk '{print $1*1024}')
echo "✓ $actual files, $total_bytes bytes in $DIR"
