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
echo "       Kernel OK"

echo "       Creating BIOS disk image..."
cd "$PROJECT_ROOT/boot"
export CARGO_TARGET_X86_64_UNKNOWN_UEFI_RUSTFLAGS="${CARGO_TARGET_X86_64_UNKNOWN_UEFI_RUSTFLAGS:--C llvm-args=-disable-loop-idiom-wcslen}"
cargo run --release -- "$KERNEL_BIN" >/dev/null
BIOS_IMG="$KERNEL_DIR/target/x86_64-unknown-none/release/knoxos-bios.img"
if [ ! -f "$BIOS_IMG" ]; then
    echo "ERROR: BIOS image not found at $BIOS_IMG"
    exit 1
fi

PERSIST_DISK="$PROJECT_ROOT/tests/c1-persist.img"
AHCI_DISK="$PROJECT_ROOT/tests/c4-ahci.img"
NVME_DISK="$PROJECT_ROOT/tests/c5-nvme.img"
rm -f "$PERSIST_DISK" "$AHCI_DISK" "$NVME_DISK"
if command -v qemu-img >/dev/null 2>&1; then
    qemu-img create -f raw "$PERSIST_DISK" 64M >/dev/null
    qemu-img create -f raw "$AHCI_DISK" 64M >/dev/null
    qemu-img create -f raw "$NVME_DISK" 64M >/dev/null
else
    dd if=/dev/zero of="$PERSIST_DISK" bs=1m count=64 status=none 2>/dev/null \
        || dd if=/dev/zero of="$PERSIST_DISK" bs=1m count=64
    dd if=/dev/zero of="$AHCI_DISK" bs=1m count=64 status=none 2>/dev/null \
        || dd if=/dev/zero of="$AHCI_DISK" bs=1m count=64
    dd if=/dev/zero of="$NVME_DISK" bs=1m count=64 status=none 2>/dev/null \
        || dd if=/dev/zero of="$NVME_DISK" bs=1m count=64
fi
chmod +x "$PROJECT_ROOT/tests/d4_http.sh"
echo "       Build OK"

# 2. Boot QEMU with serial to file (headless, auto-exit on triple fault)
echo "[2/4] Booting QEMU (timeout ${TIMEOUT_SECS}s)..."
rm -f "$SERIAL_LOG"

QEMU_ERR="$PROJECT_ROOT/tests/qemu_stderr.log"
rm -f "$QEMU_ERR"

# GNU timeout is not on macOS; the wait loop below already kills QEMU.
QEMU_CMD=(qemu-system-x86_64
    -drive format=raw,file="$BIOS_IMG"
    -drive if=virtio,format=raw,file="$PERSIST_DISK"
    -drive if=none,id=ahcidisk,format=raw,file="$AHCI_DISK"
    -device ahci,id=ahci0
    -device ide-hd,drive=ahcidisk,bus=ahci0.0
    -drive if=none,id=nvme0,format=raw,file="$NVME_DISK"
    -device nvme,serial=knoxos,drive=nvme0
    -netdev user,id=net1,guestfwd=tcp:10.0.2.100:80-cmd:"$PROJECT_ROOT/tests/d4_http.sh"
    -device virtio-net-pci,netdev=net1,disable-modern=on
    -serial file:"$SERIAL_LOG"
    -display none
    -device isa-debug-exit,iobase=0xf4,iosize=0x04
    -m 2G
    -smp 2
    -cpu qemu64,+ssse3,+sse4.1,+sse4.2,+popcnt
    -no-reboot
    -no-shutdown
)
if command -v timeout >/dev/null 2>&1; then
    timeout "$TIMEOUT_SECS" "${QEMU_CMD[@]}" 2>"$QEMU_ERR" &
else
    "${QEMU_CMD[@]}" 2>"$QEMU_ERR" &
fi
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
    if [ -s "$QEMU_ERR" ]; then
        echo "QEMU stderr:"
        cat "$QEMU_ERR"
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
check_marker "Network stack initialized"
check_marker "Firewall initialized"
check_marker "Settings persistence initialized"
check_marker "Desktop Environment ready"
check_marker "hello from userspace"
check_marker "Returned from Ring 3 userspace"
check_marker "GATE_B3 wait complete"
check_marker "GATE_B4 fork complete"
check_marker "GATE_B5 signals complete"
check_marker "GATE_B6 sh complete"
check_marker "GATE_D1 loopback complete"
check_marker "GATE_D2 virtio-net complete"
check_marker "GATE_C1 persist complete"
check_marker "GATE_C2 journal recovered"
check_marker "GATE_C3 writeback complete"
check_marker "GATE_C4 ahci dma complete"
check_marker "GATE_D3 dhcp applied"
check_marker "GATE_D4 dns tcp complete"
check_marker "GATE_E1 wx aslr complete"
check_marker "GATE_E2 csprng complete"
check_marker "GATE_E3 seccomp deny"
check_marker "GATE_E4 caps exec"
check_marker "GATE_B7 sigreturn complete"
check_marker "GATE_B8 timer preempt complete"
check_marker "GATE_F1 client isolated"
check_marker "GATE_F2 shm commit"
check_marker "GATE_F3 terminal isolated"
check_marker "GATE_F4 launcher userspace"
check_marker "GATE_C5 nvme dma complete"
check_marker "GATE_D5 cubic window"
check_marker "GATE_C6 vfs persist"
check_marker "GATE_H1 cow fault"
check_marker "GATE_H2 mmap fault"
check_marker "GATE_H3 inotify"
check_marker "GATE_H4 guard oom"
check_marker "GATE_I1 smp online"
check_marker "GATE_I2 irq gprs"

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
