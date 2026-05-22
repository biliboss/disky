#!/usr/bin/env bash
# Idempotent installer for the pre-commit hook.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

HOOK_TARGET="$(pwd)/scripts/pre-commit.sh"
HOOK_LINK=".git/hooks/pre-commit"

chmod +x "$HOOK_TARGET"

if [ -L "$HOOK_LINK" ] && [ "$(readlink "$HOOK_LINK")" = "$HOOK_TARGET" ]; then
    echo "✓ pre-commit hook already installed"
    exit 0
fi

if [ -e "$HOOK_LINK" ]; then
    mv "$HOOK_LINK" "$HOOK_LINK.backup-$(date +%s)"
    echo "→ backed up existing hook"
fi

ln -s "$HOOK_TARGET" "$HOOK_LINK"
echo "✓ pre-commit hook installed → $HOOK_TARGET"
