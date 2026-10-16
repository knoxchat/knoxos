#!/usr/bin/env bash
# Minimal aarch64 + riscv64 QEMU serial boot.
# Each arch `_start` writes GATE_ARCH … boot on UART and idles.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
KERNEL="$ROOT/kernel"
TIMEOUT_SECS="${CROSS_TIMEOUT:-15}"

die() { echo "$*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

boot_one() {
  local name="$1"
  local qemu="$2"
  local target="$3"
  local bin="$4"
  local marker="$5"
  shift 5

  if ! have "$qemu"; then
    echo "$qemu not installed; $name boot skipped"
    return 0
  fi
  if [ ! -f "$bin" ]; then
    die "kernel binary missing: $bin"
  fi

  local log="$ROOT/tests/${name}-serial.log"
  rm -f "$log"
  "$qemu" "$@" -kernel "$bin" -serial file:"$log" -display none -no-reboot &
  local pid=$!
  local ok=0
  local i
  for i in $(seq 1 "$TIMEOUT_SECS"); do
    sleep 1
    if [ -f "$log" ] && grep -q "$marker" "$log" 2>/dev/null; then
      ok=1
      break
    fi
    if ! kill -0 "$pid" 2>/dev/null; then
      break
    fi
  done
  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  if [ "$ok" -eq 1 ]; then
    echo "$marker"
    return 0
  fi
  echo "$name QEMU boot did not print $marker" >&2
  tail -20 "$log" 2>/dev/null || true
  return 1
}

A64="$KERNEL/target/aarch64-knoxos/release/knoxos-kernel"
# cargo names JSON targets after the file stem
if [ ! -f "$A64" ]; then
  A64="$KERNEL/target/aarch64-knoxos.json/release/knoxos-kernel"
fi
RV64="$KERNEL/target/riscv64-knoxos/release/knoxos-kernel"
if [ ! -f "$RV64" ]; then
  RV64="$KERNEL/target/riscv64-knoxos.json/release/knoxos-kernel"
fi

fail=0
boot_one aarch64 qemu-system-aarch64 aarch64-knoxos "$A64" "GATE_ARCH aarch64 boot" \
  -M virt -cpu cortex-a53 -m 256M || fail=1
boot_one riscv64 qemu-system-riscv64 riscv64-knoxos "$RV64" "GATE_ARCH riscv64 boot" \
  -M virt -m 256M || fail=1
exit "$fail"
