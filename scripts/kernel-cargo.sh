#!/usr/bin/env bash
# Build/check/clippy/fmt the kernel package from the repo workspace.
# Keeps artifacts in kernel/target so existing BIOS/UEFI image paths stay valid.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
CMD="${1:-build}"
shift || true
TARGET="${KERNEL_TARGET:-x86_64-unknown-none}"
TARGET_DIR="${KERNEL_TARGET_DIR:-$ROOT/kernel/target}"

if [[ "$CMD" == "fmt" ]]; then
  exec cargo fmt -p knoxos-kernel "$@"
fi
if [[ "$CMD" == "clean" ]]; then
  exec cargo clean -p knoxos-kernel --target-dir "$TARGET_DIR"
fi

exec cargo "$CMD" -p knoxos-kernel \
  --target "$TARGET" \
  -Zbuild-std=core,alloc,compiler_builtins \
  -Zbuild-std-features=compiler-builtins-mem \
  --target-dir "$TARGET_DIR" \
  "$@"
