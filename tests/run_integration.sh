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
cd "$PROJECT_ROOT"
./scripts/kernel-cargo.sh build --release 2>&1 | tail -5
KERNEL_BIN="$KERNEL_DIR/target/x86_64-unknown-none/release/knoxos-kernel"
if [ ! -f "$KERNEL_BIN" ]; then
    echo "ERROR: Kernel binary not found at $KERNEL_BIN"
    exit 1
fi
echo "       Kernel OK"

echo "       Creating BIOS disk image..."
cd "$PROJECT_ROOT"
cargo run -p knoxos-boot --release -- "$KERNEL_BIN" >/dev/null
BIOS_IMG="$KERNEL_DIR/target/x86_64-unknown-none/release/knoxos-bios.img"
if [ ! -f "$BIOS_IMG" ]; then
    echo "ERROR: BIOS image not found at $BIOS_IMG"
    exit 1
fi
# Copy so a desktop `./run.sh` QEMU can keep the original image locked.
ITEST_BIOS="$PROJECT_ROOT/tests/knoxos-bios-itest.img"
cp "$BIOS_IMG" "$ITEST_BIOS"
BIOS_IMG="$ITEST_BIOS"

PERSIST_DISK="$PROJECT_ROOT/tests/c1-persist.img"
EXT4_DISK="$PROJECT_ROOT/tests/c9-ext4.img"
AHCI_DISK="$PROJECT_ROOT/tests/c4-ahci.img"
NVME_DISK="$PROJECT_ROOT/tests/c5-nvme.img"
rm -f "$PERSIST_DISK" "$EXT4_DISK" "$AHCI_DISK" "$NVME_DISK"
if command -v qemu-img >/dev/null 2>&1; then
    qemu-img create -f raw "$PERSIST_DISK" 64M >/dev/null
    qemu-img create -f raw "$EXT4_DISK" 64M >/dev/null
    qemu-img create -f raw "$AHCI_DISK" 64M >/dev/null
    qemu-img create -f raw "$NVME_DISK" 64M >/dev/null
else
    dd if=/dev/zero of="$PERSIST_DISK" bs=1m count=64 status=none 2>/dev/null \
        || dd if=/dev/zero of="$PERSIST_DISK" bs=1m count=64
    dd if=/dev/zero of="$EXT4_DISK" bs=1m count=64 status=none 2>/dev/null \
        || dd if=/dev/zero of="$EXT4_DISK" bs=1m count=64
    dd if=/dev/zero of="$AHCI_DISK" bs=1m count=64 status=none 2>/dev/null \
        || dd if=/dev/zero of="$AHCI_DISK" bs=1m count=64
    dd if=/dev/zero of="$NVME_DISK" bs=1m count=64 status=none 2>/dev/null \
        || dd if=/dev/zero of="$NVME_DISK" bs=1m count=64
fi
MKFS_EXT4=""
for c in mkfs.ext4 /opt/homebrew/opt/e2fsprogs/sbin/mkfs.ext4 \
    /usr/local/opt/e2fsprogs/sbin/mkfs.ext4 mke2fs; do
    if command -v "$c" >/dev/null 2>&1 || [ -x "$c" ]; then
        MKFS_EXT4="$c"
        break
    fi
done
if [ -n "$MKFS_EXT4" ]; then
    if [ "$(basename "$MKFS_EXT4")" = "mke2fs" ]; then
        "$MKFS_EXT4" -t ext4 -F -q -L knoxos-root "$EXT4_DISK" >/dev/null
    else
        "$MKFS_EXT4" -q -F -L knoxos-root "$EXT4_DISK" >/dev/null
    fi
    echo "       ext4 root disk formatted"
else
    python3 "$PROJECT_ROOT/scripts/mkfs-ext4-min.py" "$EXT4_DISK"
    echo "       ext4 root disk formatted (scripts/mkfs-ext4-min.py)"
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
    -drive if=virtio,format=raw,file="$EXT4_DISK"
    -drive if=none,id=ahcidisk,format=raw,file="$AHCI_DISK"
    -device ahci,id=ahci0
    -device ide-hd,drive=ahcidisk,bus=ahci0.0
    -drive if=none,id=nvme0,format=raw,file="$NVME_DISK"
    -device nvme,serial=knoxos,drive=nvme0
    -netdev user,id=net1,guestfwd=tcp:10.0.2.100:80-cmd:"$PROJECT_ROOT/tests/d4_http.sh"
    -device virtio-net-pci,netdev=net1,disable-modern=on
    -device virtio-gpu-pci
    -device virtio-tablet-pci
    -device virtio-keyboard-pci
    -device qemu-xhci,id=xhci
    -device usb-kbd,bus=xhci.0
    -device usb-tablet,bus=xhci.0
    -device intel-hda,id=hda0
    -device hda-duplex,bus=hda0.0
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
check_marker "GATE_F5 wayland unix"
check_marker "GATE_C5 nvme dma complete"
check_marker "GATE_D5 cubic window"
check_marker "GATE_C6 vfs persist"
check_marker "GATE_C7 bin persist"
check_marker "GATE_H1 cow fault"
check_marker "GATE_H2 mmap fault"
check_marker "GATE_H3 inotify"
check_marker "GATE_H4 guard oom"
check_marker "GATE_I1 smp online"
check_marker "GATE_I2 irq gprs"
check_marker "GATE_I3 ap ring3"
check_marker "GATE_J1 thread clone"
check_marker "GATE_J2 buddy ram"
check_marker "GATE_J3 lru reclaim"
check_marker "GATE_J4 enosys"
check_marker "GATE_K1 thread join"
check_marker "GATE_K2 swap io"
check_marker "GATE_K3 init userspace"
check_marker "GATE_K4 landlock vfs"
check_marker "GATE_L1 tls fs"
check_marker "GATE_L2 uts ns"
check_marker "GATE_L3 dbus unix"
check_marker "GATE_L4 enosys"
check_marker "GATE_M1 pid ns"
check_marker "GATE_M2 pipe"
check_marker "GATE_M3 futex"
check_marker "GATE_M4 enosys"
check_marker "GATE_N1 mount ns"
check_marker "GATE_N2 socketpair"
check_marker "GATE_N3 eventfd"
check_marker "GATE_N4 enosys"
check_marker "GATE_O1 net ns"
check_marker "GATE_O2 epoll"
check_marker "GATE_O3 memfd"
check_marker "GATE_O4 enosys"
check_marker "GATE_P1 user ns"
check_marker "GATE_P2 timerfd"
check_marker "GATE_P3 signalfd"
check_marker "GATE_P4 enosys"
check_marker "GATE_Q1 ipc ns"
check_marker "GATE_Q2 poll"
check_marker "GATE_Q3 inotify"
check_marker "GATE_Q4 enosys"
check_marker "GATE_R1 cgroup ns"
check_marker "GATE_R2 splice"
check_marker "GATE_R3 flock"
check_marker "GATE_R4 enosys"
check_marker "GATE_S1 time ns"
check_marker "GATE_S2 sendfile"
check_marker "GATE_S3 tee"
check_marker "GATE_S4 enosys"
check_marker "GATE_T1 setns"
check_marker "GATE_T2 copy_file_range"
check_marker "GATE_T3 vmsplice"
check_marker "GATE_T4 enosys"
check_marker "GATE_U1 chroot"
check_marker "GATE_U2 xattr"
check_marker "GATE_U3 statx"
check_marker "GATE_U4 enosys"
check_marker "GATE_V1 pivot_root"
check_marker "GATE_V2 fallocate"
check_marker "GATE_V3 utimensat"
check_marker "GATE_V4 enosys"
check_marker "GATE_W1 overlayfs"
check_marker "GATE_W2 umask"
check_marker "GATE_W3 symlink"
check_marker "GATE_W4 enosys"
check_marker "GATE_X1 hardlink"
check_marker "GATE_X2 rename"
check_marker "GATE_X3 truncate"
check_marker "GATE_X4 enosys"
check_marker "GATE_Y1 chmod"
check_marker "GATE_Y2 chown"
check_marker "GATE_Y3 mkdir"
check_marker "GATE_Y4 enosys"
check_marker "GATE_Z1 rmdir"
check_marker "GATE_Z2 unlink"
check_marker "GATE_Z3 chdir"
check_marker "GATE_Z4 enosys"
check_marker "GATE_AA1 mkfifo"
check_marker "GATE_AA2 fchdir"
check_marker "GATE_AA3 access"
check_marker "GATE_AA4 enosys"
check_marker "GATE_AB1 getdents"
check_marker "GATE_AB2 dup2"
check_marker "GATE_AB3 uname"
check_marker "GATE_AB4 enosys"
check_marker "GATE_AC1 fcntl"
check_marker "GATE_AC2 pread64"
check_marker "GATE_AC3 getuid"
check_marker "GATE_AC4 enosys"
check_marker "GATE_AD1 fstat"
check_marker "GATE_AD2 pwrite64"
check_marker "GATE_AD3 getgid"
check_marker "GATE_AD4 enosys"
check_marker "GATE_AE1 writev"
check_marker "GATE_AE2 ftruncate"
check_marker "GATE_AE3 geteuid"
check_marker "GATE_AE4 enosys"
check_marker "GATE_AF1 readv"
check_marker "GATE_AF2 lseek"
check_marker "GATE_AF3 getegid"
check_marker "GATE_AF4 enosys"
check_marker "GATE_AG1 fsync"
check_marker "GATE_AG2 fdatasync"
check_marker "GATE_AG3 getppid"
check_marker "GATE_AG4 enosys"
check_marker "GATE_AH1 syncfs"
check_marker "GATE_AH2 sync"
check_marker "GATE_AH3 getpgid"
check_marker "GATE_AH4 enosys"
check_marker "GATE_AI1 fchmod"
check_marker "GATE_AI2 fchown"
check_marker "GATE_AI3 getsid"
check_marker "GATE_AI4 enosys"
check_marker "GATE_AJ1 fstatfs"
check_marker "GATE_AJ2 statfs"
check_marker "GATE_AJ3 setuid"
check_marker "GATE_AJ4 enosys"
check_marker "GATE_AK1 fsetxattr"
check_marker "GATE_AK2 getrlimit"
check_marker "GATE_AK3 setgid"
check_marker "GATE_AK4 enosys"
check_marker "GATE_AL1 flistxattr"
check_marker "GATE_AL2 setrlimit"
check_marker "GATE_AL3 setresuid"
check_marker "GATE_AL4 enosys"
check_marker "GATE_AM1 listxattr"
check_marker "GATE_AM2 prlimit64"
check_marker "GATE_AM3 setresgid"
check_marker "GATE_AM4 enosys"
check_marker "GATE_AN1 faccessat"
check_marker "GATE_AN2 setreuid"
check_marker "GATE_AN3 getgroups"
check_marker "GATE_AN4 enosys"
check_marker "GATE_AO1 mkdirat"
check_marker "GATE_AO2 setregid"
check_marker "GATE_AO3 getresuid"
check_marker "GATE_AO4 enosys"
check_marker "GATE_AP1 unlinkat"
check_marker "GATE_AP2 getresgid"
check_marker "GATE_AP3 setpgid"
check_marker "GATE_AP4 enosys"
check_marker "GATE_AQ1 renameat"
check_marker "GATE_AQ2 setsid"
check_marker "GATE_AQ3 setpriority"
check_marker "GATE_AQ4 enosys"
check_marker "GATE_AR1 linkat"
check_marker "GATE_AR2 getrusage"
check_marker "GATE_AR3 clock_gettime"
check_marker "GATE_AR4 enosys"
check_marker "GATE_AS1 symlinkat"
check_marker "GATE_AS2 clock_getres"
check_marker "GATE_AS3 times"
check_marker "GATE_AS4 enosys"
check_marker "GATE_AT1 readlinkat"
check_marker "GATE_AT2 gettimeofday"
check_marker "GATE_AT3 sysinfo"
check_marker "GATE_AT4 enosys"
check_marker "GATE_AU1 mknodat"
check_marker "GATE_AU2 sched_yield"
check_marker "GATE_AU3 alarm"
check_marker "GATE_AU4 enosys"
check_marker "GATE_AV1 fchmodat"
check_marker "GATE_AV2 getpid"
check_marker "GATE_AV3 gettid"
check_marker "GATE_AV4 enosys"
check_marker "GATE_AW1 fchownat"
check_marker "GATE_AW2 getsched"
check_marker "GATE_AW3 getparam"
check_marker "GATE_AW4 enosys"
check_marker "GATE_AX1 newfstatat"
check_marker "GATE_AX2 prio_max"
check_marker "GATE_AX3 prio_min"
check_marker "GATE_AX4 enosys"
check_marker "GATE_AY1 openat"
check_marker "GATE_AY2 rr_interval"
check_marker "GATE_AY3 getcpu"
check_marker "GATE_AY4 enosys"
check_marker "GATE_AZ1 utimensat"
check_marker "GATE_AZ2 set_robust"
check_marker "GATE_AZ3 get_robust"
check_marker "GATE_AZ4 enosys"
check_marker "GATE_BA1 statx"
check_marker "GATE_BA2 personality"
check_marker "GATE_BA3 nanosleep"
check_marker "GATE_BA4 enosys"
check_marker "GATE_BB1 futimesat"
check_marker "GATE_BB2 capget"
check_marker "GATE_BB3 ioprio_get"
check_marker "GATE_BB4 enosys"
check_marker "GATE_BC1 renameat2"
check_marker "GATE_BC2 capset"
check_marker "GATE_BC3 ioprio_set"
check_marker "GATE_BC4 enosys"
check_marker "GATE_BD1 faccessat2"
check_marker "GATE_BD2 clock_nanosleep"
check_marker "GATE_BD3 getitimer"
check_marker "GATE_BD4 enosys"
check_marker "GATE_BE1 execveat"
check_marker "GATE_BE2 setitimer"
check_marker "GATE_BE3 timer_gettime"
check_marker "GATE_BE4 enosys"
check_marker "GATE_BF1 dup3"
check_marker "GATE_BF2 pipe2"
check_marker "GATE_BF3 pselect6"
check_marker "GATE_BF4 enosys"
check_marker "GATE_BG1 ppoll"
check_marker "GATE_BG2 accept4"
check_marker "GATE_BG3 epoll_pwait"
check_marker "GATE_BG4 enosys"
check_marker "GATE_BH1 select"
check_marker "GATE_BH2 getsockname"
check_marker "GATE_BH3 getpeername"
check_marker "GATE_BH4 enosys"
check_marker "GATE_BI1 sendmsg"
check_marker "GATE_BI2 recvmsg"
check_marker "GATE_BI3 shutdown"
check_marker "GATE_BI4 enosys"
check_marker "GATE_BJ1 sendmmsg"
check_marker "GATE_BJ2 recvmmsg"
check_marker "GATE_BJ3 getsockopt"
check_marker "GATE_BJ4 enosys"
check_marker "GATE_BK1 setsockopt"
check_marker "GATE_BK2 tcp_send"
check_marker "GATE_BK3 tcp_recv"
check_marker "GATE_BK4 enosys"
check_marker "GATE_BL1 close_range"
check_marker "GATE_BL2 pidfd_open"
check_marker "GATE_BL3 clone3"
check_marker "GATE_BL4 enosys"
check_marker "GATE_BM1 dynlink complete"
check_marker "GATE_BM2 dlopen complete"
check_marker "GATE_BM3 musl abi"
check_marker "GATE_BM4 musl dynlink"
check_marker "GATE_G3 features live"
check_marker "GATE_G4 enosys"
check_marker "GATE_G4 audit live"
check_marker "GATE_F6 calculator userspace"
check_marker "GATE_F7 files userspace"
check_marker "GATE_I4 smp balance"
check_marker "GATE_F8 virtio-gpu scanout"
check_marker "GATE_F8 hw cursor"
check_marker "GATE_C8 jbd2 replay"
check_marker "GATE_USB hid urb"
check_marker "GATE_USB xhci mmio"
check_marker "GATE_G5 safety comments"
check_marker "GATE_C9 ext4 root"
check_marker "GATE_USB no ps2 cursor"
check_marker "GATE_VIRTIO input tablet"
check_marker "GATE_D6 ipv6 echo"
check_marker "GATE_SVC restart"
check_marker "GATE_UDEV hotplug"
check_marker "GATE_QUOTA enforce"
check_marker "GATE_CRON reboot"
check_marker "GATE_VSYNC paced"
check_marker "GATE_TERM pty userspace"
check_marker "GATE_TERM pty interactive"
check_marker "GATE_WL display protocol"
check_marker "GATE_GPU desktop scanout"
check_marker "GATE_CGROUP memory"
check_marker "GATE_CGROUP2 memory"
check_marker "GATE_JOB stop"
check_marker "GATE_IMA exec"
check_marker "GATE_LOCKDOWN module"
check_marker "GATE_UEFI time"
check_marker "GATE_WIPE free"
check_marker "GATE_NM dhcp"
check_marker "GATE_NTFS block"
check_marker "GATE_HEREDOC expand"
check_marker "GATE_PROFILE sourced"
check_marker "GATE_VT100 xterm"
check_marker "GATE_HDA dma"
check_marker "GATE_PA mix"
check_marker "GATE_IOMMU translation"
check_marker "GATE_THP hugepage"
check_marker "GATE_KASLR slide"
check_marker "GATE_AML dsdt"
check_marker "GATE_KO load"
check_marker "GATE_NUMA frame"
check_marker "GATE_KSM merge"
check_marker "GATE_NIRQ tpr"
check_marker "GATE_LPATCH wx"
check_marker "GATE_EXFAT block"
check_marker "GATE_CGROUP cpu"
check_marker "GATE_CGROUP io"
check_marker "GATE_SUSPEND resume"
check_marker "GATE_LOGIND session"
check_marker "GATE_AESGCM kat"
check_marker "GATE_POLY1305 kat"
check_marker "GATE_FONT fallback"
check_marker "GATE_XFS block"
check_marker "GATE_X25519 kat"
check_marker "GATE_ED25519 kat"
check_marker "GATE_LOCALE switch"
check_marker "GATE_IME compose"
check_marker "GATE_USB msc"
check_marker "GATE_NAT masquerade"
check_marker "GATE_DRM gem"
check_marker "GATE_GSUB liga"
check_marker "GATE_RSA kat"
check_marker "GATE_ECDSA kat"
check_marker "GATE_EMOJI color"
check_marker "GATE_USB hub"
check_marker "GATE_USB gamepad"
check_marker "GATE_TPM tis"
check_marker "GATE_TTF hint"
check_marker "GATE_I2C xfer"
check_marker "GATE_TLS13 hkdf"
check_marker "GATE_HDR pq"
check_marker "GATE_VRR range"
check_marker "GATE_E1000 send"
check_marker "GATE_EBPF exec"
check_marker "GATE_WIFI pmk"
check_marker "GATE_I915 modeset"
check_marker "GATE_BTRFS block"
check_marker "GATE_VLAN tag"
check_marker "GATE_USB audio"
check_marker "GATE_USB trackpad"
check_marker "GATE_USB wacom"
check_marker "GATE_USB uvc"
check_marker "GATE_USB ethernet"
check_marker "GATE_USB touch"
check_marker "GATE_GPIO xfer"
check_marker "GATE_RTL8139 send"
check_marker "GATE_CIFS smb2"
check_marker "GATE_GPOS kern"
check_marker "GATE_AMDGPU fw"
check_marker "GATE_AX211 fw"
check_marker "GATE_FUSE lookup"
check_marker "GATE_KVM ept"
check_marker "GATE_KVM vmlaunch"
check_marker "GATE_OCI start"
check_marker "GATE_ZFS block"
check_marker "GATE_WPA3 sae"
check_marker "GATE_BRIDGE fdb"
check_marker "GATE_TLS13 hs"
check_marker "GATE_GPU matmul"
check_marker "GATE_AI attn"
check_marker "GATE_AI chat"
check_marker "GATE_NFS compound"
check_marker "GATE_BRCM fw"
check_marker "GATE_TB hop"
check_marker "GATE_TLS13 x509"
check_marker "GATE_DPKG deb"
check_marker "GATE_TPM pcr"
check_marker "GATE_AI vocab"
check_marker "GATE_CPIO newc"
check_marker "GATE_FONT title"
check_marker "GATE_TERM shm paint"
check_marker "GATE_SELINUX deny"
check_marker "GATE_USB xhci slot"
check_marker "GATE_NTP apply"
check_marker "GATE_BT hci"
check_marker "GATE_VERITY root"
check_marker "GATE_APPARMOR deny"
check_marker "GATE_USB xhci desc"
check_marker "GATE_SVC cgroup"
check_marker "GATE_GPOS mark"
check_marker "GATE_IOURING sqe"
check_marker "GATE_FANOTIFY modify"
check_marker "GATE_DBUS method"
check_marker "GATE_USB xhci config"
check_marker "GATE_USB xhci addr"
check_marker "GATE_USB xhci enum"
check_marker "GATE_C10 virtfs persist"
check_marker "GATE_DBUS name"
check_marker "GATE_FUSE write"
check_marker "GATE_PAM unix"
check_marker "GATE_CGROUP freeze"
check_marker "GATE_ACPI s5"
check_marker "GATE_FAT32 block"
check_marker "GATE_EXT2 block"
check_marker "GATE_ISO9660 block"
check_marker "GATE_USB xhci ep"
check_marker "GATE_DBUS release"
check_marker "GATE_FUSE unlink"
check_marker "GATE_FUSE mkdir"
check_marker "GATE_DBUS list"
check_marker "GATE_USB xhci status"
check_marker "GATE_FAT32 mount"
check_marker "GATE_NFS write"
check_marker "GATE_FUSE rmdir"
check_marker "GATE_FUSE rename"
check_marker "GATE_DBUS pid"
check_marker "GATE_USB xhci iface"
check_marker "GATE_ISO9660 mount"
check_marker "GATE_NTFS mount"
check_marker "GATE_NFS remove"
check_marker "GATE_FUSE symlink"
check_marker "GATE_FUSE link"
check_marker "GATE_DBUS owner"
check_marker "GATE_USB xhci getcfg"
check_marker "GATE_EXFAT mount"
check_marker "GATE_BTRFS mount"
check_marker "GATE_NFS rename"
check_marker "GATE_FUSE create"
check_marker "GATE_FUSE getattr"
check_marker "GATE_DBUS hasowner"
check_marker "GATE_USB xhci getiface"
check_marker "GATE_XFS mount"
check_marker "GATE_ZFS mount"
check_marker "GATE_NFS create"
check_marker "GATE_FUSE setattr"
check_marker "GATE_FUSE readdir"
check_marker "GATE_DBUS uid"
check_marker "GATE_DBUS match"
check_marker "GATE_USB xhci clear"
check_marker "GATE_USB xhci bulk"
check_marker "GATE_NFS link"
check_marker "GATE_NFS readlink"
check_marker "GATE_FUSE mknod"
check_marker "GATE_FUSE statfs"
check_marker "GATE_DBUS remove"
check_marker "GATE_DBUS id"
check_marker "GATE_USB xhci set"
check_marker "GATE_USB xhci bulkin"
check_marker "GATE_NFS setattr"
check_marker "GATE_NFS readdir"
check_marker "GATE_FUSE setxattr"
check_marker "GATE_FUSE getxattr"
check_marker "GATE_DBUS start"
check_marker "GATE_DBUS creds"
check_marker "GATE_USB xhci stdesc"
check_marker "GATE_USB xhci isoch"
check_marker "GATE_NFS access"
check_marker "GATE_NFS commit"
check_marker "GATE_FUSE listxattr"
check_marker "GATE_FUSE removexattr"
check_marker "GATE_DBUS activate"
check_marker "GATE_DBUS queue"
check_marker "GATE_USB xhci string"
check_marker "GATE_USB xhci intr"
check_marker "GATE_NFS getfh"
check_marker "GATE_NFS putfh"

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
