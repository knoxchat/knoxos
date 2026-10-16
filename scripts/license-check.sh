#!/usr/bin/env bash
# SPDX / license gate: the tree is MIT. Fail if LICENSE or crate
# metadata is missing, or if a tracked source file claims a conflicting
# SPDX identifier (GPL, proprietary).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

test -f LICENSE
grep -q "MIT License" LICENSE
grep -q 'license = "MIT"' kernel/Cargo.toml

fail=0
# Only scan our sources; skip vendored / generated / huge trees.
while IFS= read -r f; do
  if grep -E -q "SPDX-License-Identifier: *(GPL|AGPL|Proprietary|UNLICENSED)" "$f"; then
    echo "conflicting SPDX in $f" >&2
    fail=1
  fi
done < <(git ls-files '*.rs' '*.sh' '*.md' '*.toml' '*.yml' 2>/dev/null || true)

if [ "$fail" -ne 0 ]; then
  exit 1
fi
echo "license/SPDX ok (MIT)"
