#!/bin/bash
# KnoxOS Integration Test — Boots QEMU and verifies serial output
#
# Usage: ./tests/run_integration.sh
# Exit code 0 = success, 1 = failure
#
# This script builds the kernel in release mode, boots it under QEMU
# with serial output redirected to a file, waits for the "Desktop
# Environment ready!" marker, and then checks for known-good log lines.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
KERNEL_DIR="$PROJECT_ROOT/kernel"
SERIAL_LOG="$PROJECT_ROOT/tests/serial_output.log"
TIMEOUT_SECS=120

echo "═══════════════════════════════════════════════════════"
echo " KnoxOS Integration Test"
echo "═══════════════════════════════════════════════════════"

# 1. Build
echo "[1/4] Building kernel..."
cd "$KERNEL_DIR"
cargo build --release 2>&1 | tail -5
KERNEL_BIN="$KERNEL_DIR/target/x86_64-unknown-none/release/knoxos-kernel"
if [ ! -f "$KERNEL_BIN" ]; then
    echo "ERROR: Kernel binary not found at $KERNEL_BIN"
    exit 1
fi
echo "       Build OK"

# 2. Boot QEMU with serial to file (headless, auto-exit on triple fault)
echo "[2/4] Booting QEMU (timeout ${TIMEOUT_SECS}s)..."
rm -f "$SERIAL_LOG"

timeout "$TIMEOUT_SECS" qemu-system-x86_64 \
    -drive format=raw,file="$KERNEL_BIN" \
    -serial file:"$SERIAL_LOG" \
    -display none \
    -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
    -m 2G \
    -smp 2 \
    -no-reboot \
    -no-shutdown \
    2>/dev/null &
QEMU_PID=$!

# 3. Wait for boot marker or timeout
echo "[3/4] Waiting for boot..."
WAITED=0
BOOT_OK=false
while [ "$WAITED" -lt "$TIMEOUT_SECS" ]; do
    sleep 2
    WAITED=$((WAITED + 2))
    if [ -f "$SERIAL_LOG" ] && grep -q "Desktop Environment ready" "$SERIAL_LOG" 2>/dev/null; then
        BOOT_OK=true
        break
    fi
    # Check if QEMU died
    if ! kill -0 "$QEMU_PID" 2>/dev/null; then
        break
    fi
done

# Kill QEMU
kill "$QEMU_PID" 2>/dev/null || true
wait "$QEMU_PID" 2>/dev/null || true

if [ "$BOOT_OK" != "true" ]; then
    echo "FAIL: Boot did not complete within ${TIMEOUT_SECS}s"
    if [ -f "$SERIAL_LOG" ]; then
        echo "Last 20 lines of serial output:"
        tail -20 "$SERIAL_LOG"
    fi
    exit 1
fi

# 4. Verify expected log markers
echo "[4/4] Verifying boot log..."
PASS=0
FAIL=0

check_marker() {
    local marker="$1"
    if grep -q "$marker" "$SERIAL_LOG" 2>/dev/null; then
        echo "  ✅ $marker"
        PASS=$((PASS + 1))
    else
        echo "  ❌ $marker (not found)"
        FAIL=$((FAIL + 1))
    fi
}

check_marker "GDT initialized"
check_marker "IDT initialized"
check_marker "PIC initialized"
check_marker "Heap allocator initialized"
check_marker "Framebuffer initialized"
check_marker "Boot splash displayed"
check_marker "VFS initialized"
check_marker "Scheduler initialized"
check_marker "Networking stack initialized"
check_marker "Firewall initialized"
check_marker "Settings persistence initialized"
check_marker "Desktop Environment ready"

echo ""
echo "═══════════════════════════════════════════════════════"
if [ "$FAIL" -eq 0 ]; then
    echo " PASSED ($PASS/$PASS checks)"
    echo "═══════════════════════════════════════════════════════"
    rm -f "$SERIAL_LOG"
    exit 0
else
    echo " FAILED ($FAIL failures, $PASS passed)"
    echo "═══════════════════════════════════════════════════════"
    exit 1
fi
