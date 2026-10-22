#!/usr/bin/env bash
#
# KnoxOS - AI Operating System
# Build and run script for QEMU testing
#
# Usage:
#   ./run.sh              # Build and run (BIOS mode, default)
#   ./run.sh --uefi       # Build and run in UEFI mode
#   ./run.sh --release    # Build optimized release
#   ./run.sh --debug      # Build with GDB debugging
#   ./run.sh --setup      # Install required tools
#   ./run.sh --clean      # Clean build artifacts
#   ./run.sh --help       # Show help
#

set -euo pipefail

# ─── Configuration ─────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
KERNEL_DIR="$SCRIPT_DIR/kernel"
BOOT_DIR="$SCRIPT_DIR/boot"

TARGET="x86_64-unknown-none"
KERNEL_BIN_DEBUG="$KERNEL_DIR/target/$TARGET/debug/knoxos-kernel"
KERNEL_BIN_RELEASE="$KERNEL_DIR/target/$TARGET/release/knoxos-kernel"

QEMU="qemu-system-x86_64"
QEMU_MEMORY="2G"
QEMU_CPUS="2"
# Default guest framebuffer (virtio-vga + bootloader request).
QEMU_DISPLAY_WIDTH="1920"
QEMU_DISPLAY_HEIGHT="1080"

# Persistent storage disk image (survives reboots) — KNOXPERSIST blob on vda
PERSISTENT_DISK="$SCRIPT_DIR/knoxos-storage.img"
PERSISTENT_DISK_SIZE="20G"
# Separate virtio-blk formatted ext4 — default root on vdb
EXT4_ROOT_DISK="$SCRIPT_DIR/knoxos-root.img"
EXT4_ROOT_DISK_SIZE="1G"
AHCI_DISK="$SCRIPT_DIR/knoxos-ahci.img"
AHCI_DISK_SIZE="64M"
NVME_DISK="$SCRIPT_DIR/knoxos-nvme.img"
NVME_DISK_SIZE="64M"
D4_HTTP="$SCRIPT_DIR/tests/d4_http.sh"

# Host timezone for the guest clock (QEMU RTC stays UTC; kernel applies this).
host_timezone() {
    if [ -n "${KNOXOS_TZ:-}" ]; then
        printf '%s' "$KNOXOS_TZ"
        return
    fi
    if [ -n "${TZ:-}" ] && [[ "$TZ" == */* ]]; then
        printf '%s' "$TZ"
        return
    fi
    local dest=""
    if [ -L /etc/localtime ]; then
        dest=$(readlink /etc/localtime 2>/dev/null || true)
    fi
    if [ -z "$dest" ]; then
        dest=$(readlink /etc/localtime 2>/dev/null || true)
    fi
    if [ -n "$dest" ] && [[ "$dest" == *zoneinfo/* ]]; then
        printf '%s' "${dest#*zoneinfo/}"
        return
    fi
    if [ -f /etc/timezone ]; then
        tr -d '[:space:]' < /etc/timezone
        return
    fi
    date +%z
}
HOST_TZ="$(host_timezone)"
QEMU_TIME_ARGS=(-rtc base=utc,clock=host -fw_cfg "name=opt/knoxos.tz,string=${HOST_TZ}")

# Auto-detect display backend for QEMU
# zoom-to-fit allows the QEMU window to be resized freely
if [[ "$OSTYPE" == "darwin"* ]]; then
    QEMU_DISPLAY="cocoa,show-cursor=on,zoom-to-fit=on"
elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
    QEMU_DISPLAY="gtk,show-cursor=on,grab-on-hover=off,zoom-to-fit=on"
else
    QEMU_DISPLAY="sdl,show-cursor=on"
fi

# OVMF firmware paths (tried in order)
OVMF_PATHS=(
    "/opt/homebrew/share/qemu/edk2-x86_64-code.fd"    # macOS ARM Homebrew
    "/usr/local/share/qemu/edk2-x86_64-code.fd"       # macOS Intel Homebrew
    "/opt/homebrew/Cellar/qemu/*/share/qemu/edk2-x86_64-code.fd" # macOS ARM alt
    "/usr/share/OVMF/OVMF_CODE.fd"                    # Ubuntu/Debian
    "/usr/share/edk2/ovmf/OVMF_CODE.fd"               # Fedora
    "/usr/share/edk2-ovmf/x64/OVMF_CODE.fd"           # Arch
    "/usr/share/qemu/OVMF.fd"                          # Generic
)

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
PURPLE='\033[0;35m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# ─── Helper Functions ──────────────────────────────────────────────

print_banner() {
    echo -e "\033[38;2;21;153;148m"
    echo "╔══════════════════════════════════════════════════════════╗"
    echo "║                                                          ║"
    echo "║   ██╗  ██╗███╗   ██╗ ██████╗ ██╗  ██╗ ██████╗ ███████╗   ║"
    echo "║   ██║ ██╔╝████╗  ██║██╔═══██╗╚██╗██╔╝██╔═══██╗██╔════╝   ║"
    echo "║   █████╔╝ ██╔██╗ ██║██║   ██║ ╚███╔╝ ██║   ██║███████╗   ║"
    echo "║   ██╔═██╗ ██║╚██╗██║██║   ██║ ██╔██╗ ██║   ██║╚════██║   ║"
    echo "║   ██║  ██╗██║ ╚████║╚██████╔╝██╔╝ ██╗╚██████╔╝███████║   ║"
    echo "║   ╚═╝  ╚═╝╚═╝  ╚═══╝ ╚═════╝ ╚═╝  ╚═╝ ╚═════╝ ╚══════╝   ║"
    echo "║                                                          ║"
    echo "║   AI Operating System - Linux Compatible                 ║"
    echo "║   Rust-powered | VirtIO-GPU Desktop | QEMU Testing       ║"
    echo "║                                                          ║"
    echo "╚══════════════════════════════════════════════════════════╝"
    echo -e "${NC}"
}

log_info() {
    echo -e "${CYAN}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[OK]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

log_step() {
    echo -e "${BLUE}[STEP]${NC} $1"
}

find_ovmf() {
    for path in "${OVMF_PATHS[@]}"; do
        # Handle glob patterns
        for expanded in $path; do
            if [ -f "$expanded" ]; then
                echo "$expanded"
                return 0
            fi
        done
    done
    return 1
}

check_dependencies() {
    local missing=()

    if ! command -v rustup &> /dev/null; then
        missing+=("rustup (https://rustup.rs)")
    fi

    if ! command -v cargo &> /dev/null; then
        missing+=("cargo (install via rustup)")
    fi

    if ! command -v $QEMU &> /dev/null; then
        missing+=("qemu-system-x86_64 (brew install qemu / apt install qemu-system-x86)")
    fi

    if [ ${#missing[@]} -gt 0 ]; then
        log_error "Missing dependencies:"
        for dep in "${missing[@]}"; do
            echo "  - $dep"
        done
        echo ""
        echo "Run './run.sh --setup' to install required tools."
        exit 1
    fi
}

ensure_persistent_disk() {
    if [ ! -f "$PERSISTENT_DISK" ]; then
        log_step "Creating persistent storage disk ($PERSISTENT_DISK_SIZE)..."
        # Create a raw disk image
        qemu-img create -f raw "$PERSISTENT_DISK" "$PERSISTENT_DISK_SIZE" > /dev/null 2>&1
        log_success "Persistent disk created (raw KNOXPERSIST; kernel formats)"
    else
        log_success "Persistent storage disk found: $PERSISTENT_DISK"
    fi
    if [ ! -f "$EXT4_ROOT_DISK" ]; then
        log_step "Creating ext4 root disk ($EXT4_ROOT_DISK_SIZE)..."
        qemu-img create -f raw "$EXT4_ROOT_DISK" "$EXT4_ROOT_DISK_SIZE" > /dev/null 2>&1
        MKFS=""
        for c in mkfs.ext4 /opt/homebrew/opt/e2fsprogs/sbin/mkfs.ext4 \
            /usr/local/opt/e2fsprogs/sbin/mkfs.ext4 mke2fs; do
            if command -v "$c" >/dev/null 2>&1 || [ -x "$c" ]; then
                MKFS="$c"
                break
            fi
        done
        if [ -n "$MKFS" ]; then
            if [ "$(basename "$MKFS")" = "mke2fs" ]; then
                "$MKFS" -t ext4 -F -q -L knoxos-root "$EXT4_ROOT_DISK" >/dev/null 2>&1 || true
            else
                "$MKFS" -q -F -L knoxos-root "$EXT4_ROOT_DISK" >/dev/null 2>&1 || true
            fi
            log_success "ext4 root disk created: $EXT4_ROOT_DISK"
        else
            python3 "$SCRIPT_DIR/scripts/mkfs-ext4-min.py" "$EXT4_ROOT_DISK"
            log_success "ext4 root disk created (minimal mkfs): $EXT4_ROOT_DISK"
        fi
    else
        log_success "ext4 root disk found: $EXT4_ROOT_DISK"
    fi
    if [ ! -f "$AHCI_DISK" ]; then
        log_step "Creating AHCI test disk ($AHCI_DISK_SIZE)..."
        qemu-img create -f raw "$AHCI_DISK" "$AHCI_DISK_SIZE" > /dev/null 2>&1
        log_success "AHCI disk created: $AHCI_DISK"
    fi
    if [ ! -f "$NVME_DISK" ]; then
        log_step "Creating NVMe test disk ($NVME_DISK_SIZE)..."
        qemu-img create -f raw "$NVME_DISK" "$NVME_DISK_SIZE" > /dev/null 2>&1
        log_success "NVMe disk created: $NVME_DISK"
    fi
}

# ─── Commands ──────────────────────────────────────────────────────

do_setup() {
    log_step "Installing required tools..."

    # Rust nightly toolchain
    log_info "Setting up Rust nightly toolchain..."
    rustup install nightly
    rustup default nightly
    rustup component add rust-src llvm-tools-preview clippy rustfmt

    # Target
    rustup target add x86_64-unknown-none

    # QEMU
    if ! command -v $QEMU &> /dev/null; then
        if [[ "$OSTYPE" == "darwin"* ]]; then
            log_info "Installing QEMU via Homebrew..."
            if command -v brew &> /dev/null; then
                brew install qemu
            else
                log_error "Homebrew not found. Install from https://brew.sh"
                log_info "Then run: brew install qemu"
            fi
        elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
            log_info "Installing QEMU..."
            if command -v apt &> /dev/null; then
                sudo apt update && sudo apt install -y qemu-system-x86 ovmf
            elif command -v dnf &> /dev/null; then
                sudo dnf install -y qemu-system-x86 edk2-ovmf
            elif command -v pacman &> /dev/null; then
                sudo pacman -S --noconfirm qemu-system-x86 edk2-ovmf
            fi
        fi
    else
        log_success "QEMU already installed: $(which $QEMU)"
    fi

    log_success "Setup complete!"
}

do_clean() {
    log_step "Cleaning build artifacts..."
    cd "$KERNEL_DIR" && cargo clean 2>/dev/null || true
    cd "$BOOT_DIR" && cargo clean 2>/dev/null || true
    log_success "Clean complete"
}

build_kernel() {
    local profile="$1"  # "debug" or "release"

    log_step "Building KnoxOS kernel ($profile)..."

    if [ "$profile" = "release" ]; then
        "$SCRIPT_DIR/scripts/kernel-cargo.sh" build --release
    else
        "$SCRIPT_DIR/scripts/kernel-cargo.sh" build
    fi

    if [ "$profile" = "release" ]; then
        KERNEL_BIN_PATH="$KERNEL_BIN_RELEASE"
    else
        KERNEL_BIN_PATH="$KERNEL_BIN_DEBUG"
    fi

    if [ ! -f "$KERNEL_BIN_PATH" ]; then
        log_error "Kernel binary not found at: $KERNEL_BIN_PATH"
        exit 1
    fi

    local size=$(ls -lh "$KERNEL_BIN_PATH" | awk '{print $5}')
    log_success "Kernel built: $KERNEL_BIN_PATH ($size)"
}

create_boot_image() {
    local kernel_bin="$1"

    log_step "Creating boot disk images..."

    cd "$SCRIPT_DIR"
    cargo run -p knoxos-boot -- "$kernel_bin" 2>&1

    if [ $? -ne 0 ]; then
        log_error "Boot image creation failed!"
        exit 1
    fi

    local kernel_dir=$(dirname "$kernel_bin")
    local uefi_img="$kernel_dir/knoxos-uefi.img"
    local bios_img="$kernel_dir/knoxos-bios.img"

    if [ -f "$uefi_img" ]; then
        log_success "UEFI image: $uefi_img ($(ls -lh "$uefi_img" | awk '{print $5}'))"
    fi
    if [ -f "$bios_img" ]; then
        log_success "BIOS image: $bios_img ($(ls -lh "$bios_img" | awk '{print $5}'))"
    fi
}

run_qemu_bios() {
    local kernel_bin="$1"
    local kernel_dir=$(dirname "$kernel_bin")
    local bios_img="$kernel_dir/knoxos-bios.img"

    if [ ! -f "$bios_img" ]; then
        log_error "BIOS disk image not found: $bios_img"
        exit 1
    fi

    echo ""
    log_step "Starting KnoxOS in QEMU (BIOS mode)..."
    echo -e "${PURPLE}════════════════════════════════════════════════════════${NC}"
    echo -e "  ${CYAN}Display:${NC}  $QEMU_DISPLAY"
    echo -e "  ${CYAN}Resolution:${NC} ${QEMU_DISPLAY_WIDTH}x${QEMU_DISPLAY_HEIGHT}"
    echo -e "  ${CYAN}Timezone:${NC} $HOST_TZ"
    echo -e "  ${CYAN}Input:${NC}   virtio-tablet + virtio-mouse + USB tablet (click the window first)"
    echo -e "  ${CYAN}Memory:${NC}   $QEMU_MEMORY"
    echo -e "  ${CYAN}CPUs:${NC}     $QEMU_CPUS"
    echo -e "  ${CYAN}Storage:${NC}  $PERSISTENT_DISK ($PERSISTENT_DISK_SIZE persistent)"
    echo -e "  ${CYAN}Serial:${NC}   stdout (below) + /tmp/knoxos-serial.log"
    echo -e "  ${CYAN}Exit:${NC}     Ctrl+C or close QEMU window"
    echo -e "${PURPLE}════════════════════════════════════════════════════════${NC}"
    echo ""

    # Use serial to stdio so kernel output is visible in terminal.
    # Also tee to a log file for later inspection.
    $QEMU \
        -drive format=raw,file="$bios_img" \
        -drive if=virtio,format=raw,file="$PERSISTENT_DISK" \
        -drive if=virtio,format=raw,file="$EXT4_ROOT_DISK" \
        -drive if=none,id=ahcidisk,format=raw,file="$AHCI_DISK" \
        -device ahci,id=ahci0 \
        -device ide-hd,drive=ahcidisk,bus=ahci0.0 \
        -drive if=none,id=nvme0,format=raw,file="$NVME_DISK" \
        -device nvme,serial=knoxos,drive=nvme0 \
        -netdev user,id=net1,guestfwd=tcp:10.0.2.100:80-cmd:"$D4_HTTP" \
        -device virtio-net-pci,netdev=net1,disable-modern=on \
        -device virtio-tablet-pci \
        -device virtio-mouse-pci \
        -device virtio-keyboard-pci \
        -serial stdio \
        -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
        -m "$QEMU_MEMORY" \
        -cpu qemu64 \
        -smp "$QEMU_CPUS" \
        "${QEMU_TIME_ARGS[@]}" \
        -vga virtio \
        -global virtio-vga.xres="$QEMU_DISPLAY_WIDTH" \
        -global virtio-vga.yres="$QEMU_DISPLAY_HEIGHT" \
        -display $QEMU_DISPLAY \
        -device qemu-xhci,id=xhci \
        -device usb-kbd,bus=xhci.0 \
        -device usb-tablet,bus=xhci.0 \
        -device intel-hda,id=hda0 \
        -device hda-duplex,bus=hda0.0 \
        -monitor tcp:127.0.0.1:44444,server,nowait \
        -no-reboot \
        -no-shutdown \
        2>&1 | tee /tmp/knoxos-serial.log
}

run_qemu_uefi() {
    local kernel_bin="$1"
    local kernel_dir=$(dirname "$kernel_bin")
    local uefi_img="$kernel_dir/knoxos-uefi.img"

    if [ ! -f "$uefi_img" ]; then
        log_error "UEFI disk image not found: $uefi_img"
        exit 1
    fi

    local ovmf
    ovmf=$(find_ovmf) || {
        log_error "OVMF firmware not found!"
        log_info "Searched paths:"
        for p in "${OVMF_PATHS[@]}"; do
            echo "  - $p"
        done
        log_info "Install OVMF: brew install qemu (macOS) / apt install ovmf (Ubuntu)"
        log_info "Or set OVMF environment variable to the correct path"
        log_info "Falling back to BIOS mode..."
        run_qemu_bios "$kernel_bin"
        return
    }

    echo ""
    log_step "Starting KnoxOS in QEMU (UEFI mode)..."
    echo -e "${PURPLE}════════════════════════════════════════════════════════${NC}"
    echo -e "  ${CYAN}Display:${NC}  $QEMU_DISPLAY"
    echo -e "  ${CYAN}Resolution:${NC} ${QEMU_DISPLAY_WIDTH}x${QEMU_DISPLAY_HEIGHT}"
    echo -e "  ${CYAN}Timezone:${NC} $HOST_TZ"
    echo -e "  ${CYAN}Input:${NC}   virtio-tablet + virtio-mouse + USB tablet (click the window first)"
    echo -e "  ${CYAN}OVMF:${NC}    $ovmf"
    echo -e "  ${CYAN}Memory:${NC}   $QEMU_MEMORY"
    echo -e "  ${CYAN}CPUs:${NC}     $QEMU_CPUS"
    echo -e "  ${CYAN}Serial:${NC}   stdout (below)"
    echo -e "  ${CYAN}Exit:${NC}     Ctrl+C or close QEMU window"
    echo -e "${PURPLE}════════════════════════════════════════════════════════${NC}"
    echo ""

    $QEMU \
        -bios "$ovmf" \
        -drive format=raw,file="$uefi_img" \
        -drive if=virtio,format=raw,file="$PERSISTENT_DISK" \
        -drive if=virtio,format=raw,file="$EXT4_ROOT_DISK" \
        -drive if=none,id=ahcidisk,format=raw,file="$AHCI_DISK" \
        -device ahci,id=ahci0 \
        -device ide-hd,drive=ahcidisk,bus=ahci0.0 \
        -drive if=none,id=nvme0,format=raw,file="$NVME_DISK" \
        -device nvme,serial=knoxos,drive=nvme0 \
        -netdev user,id=net1,guestfwd=tcp:10.0.2.100:80-cmd:"$D4_HTTP" \
        -device virtio-net-pci,netdev=net1,disable-modern=on \
        -device virtio-tablet-pci \
        -device virtio-mouse-pci \
        -device virtio-keyboard-pci \
        -serial stdio \
        -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
        -m "$QEMU_MEMORY" \
        -cpu qemu64 \
        -smp "$QEMU_CPUS" \
        "${QEMU_TIME_ARGS[@]}" \
        -vga virtio \
        -global virtio-vga.xres="$QEMU_DISPLAY_WIDTH" \
        -global virtio-vga.yres="$QEMU_DISPLAY_HEIGHT" \
        -display $QEMU_DISPLAY \
        -device qemu-xhci,id=xhci \
        -device usb-kbd,bus=xhci.0 \
        -device usb-tablet,bus=xhci.0 \
        -device intel-hda,id=hda0 \
        -device hda-duplex,bus=hda0.0 \
        -no-reboot \
        -no-shutdown
}

run_qemu_debug() {
    local kernel_bin="$1"
    local kernel_dir=$(dirname "$kernel_bin")
    local bios_img="$kernel_dir/knoxos-bios.img"

    if [ ! -f "$bios_img" ]; then
        log_error "BIOS disk image not found: $bios_img"
        exit 1
    fi

    echo ""
    log_step "Starting KnoxOS in QEMU (DEBUG mode)..."
    log_info "QEMU will wait for GDB connection on localhost:1234"
    log_info "Connect with: gdb -ex 'target remote :1234' $kernel_bin"
    echo ""

    $QEMU \
        -drive format=raw,file="$bios_img" \
        -drive if=virtio,format=raw,file="$PERSISTENT_DISK" \
        -drive if=virtio,format=raw,file="$EXT4_ROOT_DISK" \
        -drive if=none,id=ahcidisk,format=raw,file="$AHCI_DISK" \
        -device ahci,id=ahci0 \
        -device ide-hd,drive=ahcidisk,bus=ahci0.0 \
        -drive if=none,id=nvme0,format=raw,file="$NVME_DISK" \
        -device nvme,serial=knoxos,drive=nvme0 \
        -netdev user,id=net1,guestfwd=tcp:10.0.2.100:80-cmd:"$D4_HTTP" \
        -device virtio-net-pci,netdev=net1,disable-modern=on \
        -device virtio-tablet-pci \
        -device virtio-mouse-pci \
        -device virtio-keyboard-pci \
        -serial stdio \
        -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
        -m "$QEMU_MEMORY" \
        -cpu qemu64 \
        -smp "$QEMU_CPUS" \
        "${QEMU_TIME_ARGS[@]}" \
        -vga virtio \
        -global virtio-vga.xres="$QEMU_DISPLAY_WIDTH" \
        -global virtio-vga.yres="$QEMU_DISPLAY_HEIGHT" \
        -display $QEMU_DISPLAY \
        -device intel-hda,id=hda0 \
        -device hda-duplex,bus=hda0.0 \
        -s -S \
        -no-reboot \
        -no-shutdown
}

show_help() {
    print_banner
    echo "Usage: ./run.sh [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  (no args)       Build release and run in QEMU (BIOS, 1920x1080)"
    echo "  --uefi          Build and run in UEFI mode"
    echo "  --dev           Build debug (unoptimized) instead of release"
    echo "  --release       Build optimized release build (default)"
    echo "  --debug         Build and run with GDB debugging"
    echo "  --build-only    Build without running QEMU"
    echo "  --setup         Install required tools (Rust, QEMU)"
    echo "  --clean         Clean all build artifacts"
    echo "  --help          Show this help message"
    echo ""
    echo "Examples:"
    echo "  ./run.sh                    # Release build & run (BIOS)"
    echo "  ./run.sh --uefi             # Run with UEFI firmware"
    echo "  ./run.sh --dev              # Debug build & run (slower)"
    echo "  ./run.sh --debug            # Debug with GDB"
    echo ""
    echo "Requirements:"
    echo "  - Rust nightly toolchain (with rust-src, llvm-tools-preview)"
    echo "  - QEMU (qemu-system-x86_64)"
    echo "  - OVMF firmware (for UEFI mode only)"
    echo ""
}

# ─── Main ──────────────────────────────────────────────────────────

main() {
    local mode="bios"
    local profile="release"
    local build_only=false

    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case $1 in
            --uefi)
                mode="uefi"
                shift
                ;;
            --release)
                profile="release"
                shift
                ;;
            --dev)
                profile="debug"
                shift
                ;;
            --debug)
                mode="debug"
                shift
                ;;
            --build-only)
                build_only=true
                shift
                ;;
            --setup)
                do_setup
                exit 0
                ;;
            --clean)
                do_clean
                exit 0
                ;;
            --help|-h)
                show_help
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                echo "Run './run.sh --help' for usage information."
                exit 1
                ;;
        esac
    done

    print_banner
    check_dependencies

    # Build
    KERNEL_BIN_PATH=""
    build_kernel "$profile"

    # Create boot images
    create_boot_image "$KERNEL_BIN_PATH"

    if [ "$build_only" = true ]; then
        log_success "Build complete! Images are ready."
        exit 0
    fi

    # Ensure persistent storage disk exists
    ensure_persistent_disk

    # Run in QEMU
    case "$mode" in
        uefi)
            run_qemu_uefi "$KERNEL_BIN_PATH"
            ;;
        debug)
            run_qemu_debug "$KERNEL_BIN_PATH"
            ;;
        *)
            run_qemu_bios "$KERNEL_BIN_PATH"
            ;;
    esac
}

main "$@"
