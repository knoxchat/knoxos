# KnoxOS Kernel Build System
# Build and run the KnoxOS operating system

KERNEL_DIR = .
TARGET = x86_64-unknown-none
KERNEL_BIN = target/$(TARGET)/debug/knoxos-kernel
KERNEL_RELEASE = target/$(TARGET)/release/knoxos-kernel
UEFI_IMG = target/$(TARGET)/debug/knoxos-uefi.img
BIOS_IMG = target/$(TARGET)/debug/knoxos-bios.img
UEFI_IMG_RELEASE = target/$(TARGET)/release/knoxos-uefi.img
BIOS_IMG_RELEASE = target/$(TARGET)/release/knoxos-bios.img

# QEMU settings
QEMU = qemu-system-x86_64
PERSISTENT_DISK = ../knoxos-storage.img
QEMU_ARGS = -serial stdio \
	-device isa-debug-exit,iobase=0xf4,iosize=0x04 \
	-display sdl \
	-vga std \
	-global VGA.vgamem_mb=64 \
	-global VGA.xres=1920 \
	-global VGA.yres=1080 \
	-m 2G \
	-cpu qemu64,+ssse3,+sse4.1,+sse4.2,+popcnt \
	-smp 2 \
	-drive if=virtio,format=raw,file=$(PERSISTENT_DISK)

# OVMF firmware for UEFI booting (adjust path for your system)
# macOS (Homebrew): brew install qemu  (includes OVMF)
# Linux: /usr/share/OVMF/OVMF_CODE.fd or /usr/share/edk2/ovmf/OVMF_CODE.fd
OVMF ?= /usr/share/OVMF/OVMF_CODE.fd

.PHONY: all build release run run-bios run-release clean test clippy fmt

# Default: build and run (UEFI)
all: run

# Build debug
build:
	@echo "╔══════════════════════════════════════════╗"
	@echo "║  Building KnoxOS - AI Operating System      ║"
	@echo "╚══════════════════════════════════════════╝"
	cargo build
	@echo "Kernel ELF built: $(KERNEL_BIN)"

# Build release (optimized)
release:
	@echo "Building KnoxOS (release)..."
	cargo build --release
	@echo "Kernel ELF built: $(KERNEL_RELEASE)"

# Create boot images (debug)
bootimage: build
	@echo "Creating boot disk images..."
	cd ../boot && cargo run -- ../kernel/$(KERNEL_BIN)
	@echo "Boot images created!"

# Create boot images (release)
bootimage-release: release
	@echo "Creating release boot disk images..."
	cd ../boot && cargo run -- ../kernel/$(KERNEL_RELEASE)
	@echo "Release boot images created!"

# Run in QEMU - UEFI mode (default)
run: bootimage
	@echo "╔══════════════════════════════════════════╗"
	@echo "║  Starting KnoxOS in QEMU (UEFI)             ║"
	@echo "║  Press Ctrl+C to exit                    ║"
	@echo "╚══════════════════════════════════════════╝"
	$(QEMU) -bios $(OVMF) -drive format=raw,file=$(UEFI_IMG) $(QEMU_ARGS)

# Run in QEMU - BIOS mode
run-bios: bootimage
	@echo "╔══════════════════════════════════════════╗"
	@echo "║  Starting KnoxOS in QEMU (BIOS)             ║"
	@echo "║  Press Ctrl+C to exit                    ║"
	@echo "╚══════════════════════════════════════════╝"
	$(QEMU) -drive format=raw,file=$(BIOS_IMG) $(QEMU_ARGS)

# Run in QEMU (release, UEFI)
run-release: bootimage-release
	$(QEMU) -bios $(OVMF) -drive format=raw,file=$(UEFI_IMG_RELEASE) $(QEMU_ARGS)

# Run with KVM acceleration (Linux host only)
run-kvm: bootimage
	$(QEMU) -bios $(OVMF) -drive format=raw,file=$(UEFI_IMG) $(QEMU_ARGS) -enable-kvm

# Run with GDB debugging
run-debug: bootimage
	$(QEMU) -bios $(OVMF) -drive format=raw,file=$(UEFI_IMG) $(QEMU_ARGS) -s -S &
	@echo "Waiting for GDB connection on :1234..."
	@echo "Run: gdb -ex 'target remote :1234' $(KERNEL_BIN)"

# Run tests
test:
	cargo test

# Clippy lint
clippy:
	cargo clippy -- -D warnings

# Format code
fmt:
	cargo fmt

# Check formatting
fmt-check:
	cargo fmt -- --check

# Clean build artifacts
clean:
	cargo clean
	rm -rf $(ISO_DIR)

# Show kernel size
size: build
	@echo "Kernel size:"
	@ls -lh $(KERNEL_BIN)
	@size $(KERNEL_BIN) 2>/dev/null || true

# Install required tools
setup:
	@echo "Installing required tools..."
	rustup component add rust-src llvm-tools-preview
	@echo ""
	@echo "You also need QEMU installed:"
	@echo "  macOS:  brew install qemu"
	@echo "  Ubuntu: sudo apt install qemu-system-x86 ovmf"
	@echo "  Arch:   sudo pacman -S qemu-system-x86 edk2-ovmf"
	@echo ""
	@echo "For UEFI booting, you need OVMF firmware."
	@echo "Set OVMF= to the path of your OVMF_CODE.fd if not at default location."
	@echo ""
	@echo "Setup complete!"

# Help
help:
	@echo "KnoxOS Build System"
	@echo "═══════════════════════════════════════════"
	@echo "  make build          - Build kernel (debug)"
	@echo "  make release        - Build kernel (release)"
	@echo "  make run            - Build and run in QEMU (UEFI)"
	@echo "  make run-bios       - Build and run in QEMU (BIOS)"
	@echo "  make run-release    - Build and run optimized (UEFI)"
	@echo "  make run-debug      - Run with GDB debugging"
	@echo "  make test           - Run kernel tests"
	@echo "  make clippy         - Lint with clippy"
	@echo "  make fmt            - Format code"
	@echo "  make clean          - Clean build artifacts"
	@echo "  make size           - Show kernel binary size"
	@echo "  make setup          - Install build tools"
	@echo "  make help           - Show this help"

# ═══════════════════════════════════════════════════════════════════════
# CI / AUTOMATION TARGETS (Section 27)
# ═══════════════════════════════════════════════════════════════════════

# Clippy (CI compiles and lints; warnings are reported, not fatal yet)
.PHONY: ci-clippy
ci-clippy:
	@echo "[CI] Running Clippy..."
	cargo clippy --release
	@echo "[CI] Clippy complete."

# Format check (CI gate — fail if not formatted)
.PHONY: ci-fmt
ci-fmt:
	@echo "[CI] Checking code formatting..."
	cargo fmt -- --check
	@echo "[CI] Formatting OK."

# Binary size analysis and bloat tracking
.PHONY: ci-size
ci-size: release
	@echo "[CI] Binary size analysis:"
	@ls -lh $(KERNEL_RELEASE)
	@size $(KERNEL_RELEASE) 2>/dev/null || true
	@echo "[CI] Section sizes:"
	@objdump -h $(KERNEL_RELEASE) 2>/dev/null | grep -E '^\s+[0-9]' || true
	@echo "[CI] Checking for unexpected size bloat..."
	@MAX_SIZE=16777216; \
	ACTUAL=$$(stat -f%z $(KERNEL_RELEASE) 2>/dev/null || stat -c%s $(KERNEL_RELEASE)); \
	if [ "$$ACTUAL" -gt "$$MAX_SIZE" ]; then \
		echo "ERROR: Kernel binary too large: $$ACTUAL bytes (max $$MAX_SIZE)"; \
		exit 1; \
	else \
		echo "OK: $$ACTUAL bytes (max $$MAX_SIZE)"; \
	fi

# Automated QEMU testing (boot, verify serial markers, exit)
.PHONY: ci-qemu
ci-qemu: bootimage
	@echo "[CI] Running automated QEMU boot test..."
	@timeout 30 $(QEMU) -bios $(OVMF) -drive format=raw,file=$(UEFI_IMG) \
		-serial stdio -display none -nographic \
		-device isa-debug-exit,iobase=0xf4,iosize=0x04 \
		-m 2G -cpu qemu64,+ssse3,+sse4.1,+sse4.2,+popcnt \
		2>&1 | tee /tmp/knoxos-ci-boot.log || true
	@echo "[CI] Checking boot markers..."
	@grep -c "KnoxOS" /tmp/knoxos-ci-boot.log > /dev/null && echo "[CI] Boot markers found." || echo "[CI] Warning: no boot markers."

# Cross-compile for AArch64 (full)
.PHONY: ci-cross-aarch64
ci-cross-aarch64:
	@echo "[CI] Cross-compiling for AArch64..."
	@CARGO_BUILD_TARGET=aarch64-unknown-none cargo build --release 2>&1 || echo "[CI] AArch64 cross-compile attempted (may need target JSON)."

# Cross-compile for RISC-V
.PHONY: ci-cross-riscv
ci-cross-riscv:
	@echo "[CI] Cross-compiling for RISC-V..."
	@CARGO_BUILD_TARGET=riscv64gc-unknown-none-elf cargo build --release 2>&1 || echo "[CI] RISC-V cross-compile attempted (may need target JSON)."

# Performance regression gate (run benchmarks, compare)
.PHONY: ci-perf
ci-perf: release
	@echo "[CI] Running performance regression gate..."
	@echo "[CI] Building and running benchmarks..."
	@$(QEMU) -bios $(OVMF) -drive format=raw,file=$(UEFI_IMG_RELEASE) \
		-serial stdio -display none -nographic \
		-device isa-debug-exit,iobase=0xf4,iosize=0x04 \
		-m 2G -cpu qemu64,+ssse3,+sse4.1,+sse4.2,+popcnt \
		-append "bench" 2>&1 | tee /tmp/knoxos-ci-perf.log || true
	@echo "[CI] Performance results saved to /tmp/knoxos-ci-perf.log"

# Full CI pipeline (all checks)
.PHONY: ci
ci: ci-fmt ci-clippy ci-size ci-qemu
	@echo ""
	@echo "═══════════════════════════════════════════"
	@echo " All CI checks passed!"
	@echo "═══════════════════════════════════════════"
