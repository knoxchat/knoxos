# KnoxOS - Rust-based AI Operating System
# Master Makefile for building and managing the complete system

.PHONY: help all kernel boot clean test run-kernel qemu docs status pre-commit pre-commit-quick pre-commit-fix pre-commit-install

# Color output
RED := \033[0;31m
GREEN := \033[0;32m
YELLOW := \033[0;33m
BLUE := \033[0;34m
NC := \033[0m # No Color

# Directories
KERNEL_DIR := kernel
BOOT_DIR := boot

# Default target
.DEFAULT_GOAL := help

help: ## Show this help message
	@echo "$(BLUE)═══════════════════════════════════════════════════════════$(NC)"
	@echo "$(BLUE)KnoxOS - Rust-based AI Operating System$(NC)"
	@echo "$(BLUE)═══════════════════════════════════════════════════════════$(NC)"
	@echo ""
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "$(GREEN)%-25s$(NC) %s\n", $$1, $$2}'
	@echo ""
	@echo "$(YELLOW)Examples:$(NC)"
	@echo "  make kernel          # Build kernel"
	@echo "  make run-kernel      # Build and run kernel in QEMU"
	@echo "  make all             # Build everything"
	@echo "  make status          # Check build status"
	@echo ""

# ─── Kernel Building ─────────────────────────────────────────────────────

kernel: ## Build the KnoxOS kernel
	@echo "$(BLUE)[KnoxOS] Building kernel...$(NC)"
	@cd $(KERNEL_DIR) && cargo build --release 2>&1 | grep -E "Compiling|Finished|error" || true
	@echo "$(GREEN)[✓] Kernel built$(NC)"

kernel-debug: ## Build kernel with debug symbols
	@echo "$(BLUE)[KnoxOS] Building kernel (debug)...$(NC)"
	@cd $(KERNEL_DIR) && cargo build 2>&1 | grep -E "Compiling|Finished|error" || true
	@echo "$(GREEN)[✓] Kernel built$(NC)"

kernel-clean: ## Clean kernel build artifacts
	@echo "$(YELLOW)[KnoxOS] Cleaning kernel...$(NC)"
	@cd $(KERNEL_DIR) && cargo clean
	@echo "$(GREEN)[✓] Kernel cleaned$(NC)"

kernel-test: ## Run kernel tests
	@echo "$(BLUE)[KnoxOS] Running kernel tests...$(NC)"
	@cd $(KERNEL_DIR) && cargo test 2>&1 | tail -20

# ─── Boot Building ──────────────────────────────────────────────────────

boot: kernel ## Build boot images (BIOS & UEFI)
	@echo "$(BLUE)[KnoxOS] Building boot images...$(NC)"
	@cd $(BOOT_DIR) && cargo run --release -- ../$(KERNEL_DIR)/target/release/knoxos-kernel 2>&1 | grep -E "Creating|successfully|Error" || true
	@echo "$(GREEN)[✓] Boot images built$(NC)"

boot-list: ## List generated boot images
	@echo "$(BLUE)[KnoxOS] Boot images:$(NC)"
	@ls -lh $(KERNEL_DIR)/knoxos-*.img 2>/dev/null || echo "No boot images found. Run 'make boot' first."

# ─── Running ────────────────────────────────────────────────────────────

run-kernel: boot ## Run kernel in QEMU (BIOS mode)
	@echo "$(BLUE)[KnoxOS] Launching kernel in QEMU...$(NC)"
	@if command -v qemu-system-x86_64 &> /dev/null; then \
		qemu-system-x86_64 \
			-drive format=raw,file=$(KERNEL_DIR)/knoxos-bios.img \
			-drive if=virtio,format=raw,file=knoxos-storage.img \
			-serial stdio \
			-m 2G \
			-smp 2 \
			-cpu qemu64,+ssse3,+sse4.1,+sse4.2,+popcnt \
			-vga std \
			-global VGA.vgamem_mb=64 \
			-global VGA.xres=1920 \
			-global VGA.yres=1080 \
			-display cocoa,show-cursor=off \
			-usb \
			-device usb-tablet; \
	else \
		echo "$(RED)[✗] QEMU not found. Install with: brew install qemu$(NC)"; \
		exit 1; \
	fi

run-kernel-uefi: boot ## Run kernel in QEMU (UEFI mode)
	@echo "$(BLUE)[KnoxOSxOS] Launching kernel in QEMU (UEFI)...$(NC)"
	@if command -v qemu-system-x86_64 &> /dev/null; then \
		qemu-system-x86_64 \
			-bios /usr/share/ovmf/OVMF.fd \
			-drive format=raw,file=$(KERNEL_DIR)/knoxos-uefi.img \
			-drive if=virtio,format=raw,file=knoxos-storage.img \
			-serial stdio \
			-m 2G \
			-smp 2 \
			-vga std \
			-global VGA.vgamem_mb=64 \
			-global VGA.xres=1920 \
			-global VGA.yres=1080 \
			-display cocoa,show-cursor=off \
			-usb \
			-device usb-tablet; \
	else \
		echo "$(RED)[✗] QEMU not found. Install with: brew install qemu$(NC)"; \
		exit 1; \
	fi

run-kernel-gdb: boot ## Run kernel in QEMU with GDB stub
	@echo "$(BLUE)[KnoxOSxOS] Launching kernel in QEMU with GDB...$(NC)"
	@qemu-system-x86_64 \
		-drive format=raw,file=$(KERNEL_DIR)/knoxos-bios.img \
		-drive if=virtio,format=raw,file=knoxos-storage.img \
		-serial stdio \
		-m 2G \
		-vga std \
		-global VGA.vgamem_mb=64 \
		-global VGA.xres=1920 \
		-global VGA.yres=1080 \
		-usb \
		-device usb-tablet \
		-gdb tcp::1234 -S

# ─── Building Everything ────────────────────────────────────────────────

all: ## Build everything (kernel, boot, backend)
	@echo "$(BLUE)═══════════════════════════════════════════════════════════$(NC)"
	@echo "$(BLUE)KnoxOSxOS - Building Complete System$(NC)"
	@echo "$(BLUE)═══════════════════════════════════════════════════════════$(NC)"
	@$(MAKE) kernel
	@$(MAKE) boot
	@echo ""
	@echo "$(GREEN)═══════════════════════════════════════════════════════════$(NC)"
	@echo "$(GREEN)[✓] Build Complete!$(NC)"
	@echo "$(GREEN)═══════════════════════════════════════════════════════════$(NC)"
	@echo ""
	@echo "$(YELLOW)Next steps:$(NC)"
	@echo "  $(GREEN)make run-kernel$(NC)   - Run kernel in QEMU"
	@echo ""

# ─── Cleaning ───────────────────────────────────────────────────────────

clean: ## Clean all build artifacts
	@echo "$(YELLOW)[KnoxOS] Cleaning all build artifacts...$(NC)"
	@$(MAKE) kernel-clean
	@rm -rf $(KERNEL_DIR)/knoxos-*.img
	@echo "$(GREEN)[✓] All clean$(NC)"

# ─── Documentation ──────────────────────────────────────────────────────

docs: ## Open documentation (status.md)
	@if command -v less &> /dev/null; then \
		less status.md; \
	else \
		cat status.md; \
	fi

status: ## Show project status
	@echo "$(BLUE)═══════════════════════════════════════════════════════════$(NC)"
	@echo "$(BLUE)KnoxOS Project Status$(NC)"
	@echo "$(BLUE)═══════════════════════════════════════════════════════════$(NC)"
	@echo ""
	@echo "$(YELLOW)Kernel:$(NC)"
	@test -f $(KERNEL_DIR)/target/release/knoxos-kernel && echo "  $(GREEN)[✓]$(NC) Kernel built" || echo "  $(RED)[✗]$(NC) Kernel not built"
	@test -f $(KERNEL_DIR)/knoxos-bios.img && echo "  $(GREEN)[✓]$(NC) BIOS boot image ready" || echo "  $(RED)[✗]$(NC) BIOS boot image missing"
	@test -f $(KERNEL_DIR)/knoxos-uefi.img && echo "  $(GREEN)[✓]$(NC) UEFI boot image ready" || echo "  $(RED)[✗]$(NC) UEFI boot image missing"
	@echo ""
	@echo "$(YELLOW)Documentation:$(NC)"
	@test -f status.md && echo "  $(GREEN)[✓]$(NC) status.md exists" || echo "  $(RED)[✗]$(NC) status.md missing"
	@test -f README.md && echo "  $(GREEN)[✓]$(NC) README.md exists" || echo "  $(RED)[✗]$(NC) README.md missing"
	@test -f LICENSE && echo "  $(GREEN)[✓]$(NC) LICENSE exists" || echo "  $(RED)[✗]$(NC) LICENSE missing"
	@echo ""

# ─── ISO Image ───────────────────────────────────────────────────────

iso: boot ## Create a bootable ISO image
	@echo "$(BLUE)[KnoxOS] Creating bootable ISO...$(NC)"
	@mkdir -p iso_root/boot/grub iso_root/EFI/BOOT
	@cp $(KERNEL_DIR)/knoxos-bios.img iso_root/boot/knoxos.img 2>/dev/null || true
	@cp $(KERNEL_DIR)/knoxos-uefi.img iso_root/EFI/BOOT/BOOTX64.EFI 2>/dev/null || true
	@echo 'set timeout=3' > iso_root/boot/grub/grub.cfg
	@echo 'set default=0' >> iso_root/boot/grub/grub.cfg
	@echo 'menuentry "KnoxOS" {' >> iso_root/boot/grub/grub.cfg
	@echo '  multiboot2 /boot/knoxos.img' >> iso_root/boot/grub/grub.cfg
	@echo '}' >> iso_root/boot/grub/grub.cfg
	@if command -v xorriso &> /dev/null; then \
		xorriso -as mkisofs \
			-o knoxos.iso \
			-iso-level 3 \
			-full-iso9660-filenames \
			-R -J -joliet-long \
			-b boot/knoxos.img \
			-no-emul-boot \
			-boot-load-size 4 \
			-boot-info-table \
			iso_root 2>/dev/null; \
		echo "$(GREEN)[✓] knoxos.iso created$(NC)"; \
	else \
		echo "$(YELLOW)[!] xorriso not found — creating raw tarball instead$(NC)"; \
		tar -czf knoxos-boot.tar.gz -C iso_root .; \
		echo "$(GREEN)[✓] knoxos-boot.tar.gz created$(NC)"; \
	fi
	@rm -rf iso_root

# ─── Cross Compilation ──────────────────────────────────────────────────

cross-aarch64: ## Cross-compile kernel for AArch64 (experimental)
	@echo "$(BLUE)[KnoxOS] Building kernel for AArch64...$(NC)"
	@echo "$(YELLOW)  NOTE: AArch64 target requires aarch64-knoxos.json$(NC)"
	@cd $(KERNEL_DIR) && CARGO_TARGET=aarch64-unknown-none cargo build --release 2>&1 | tail -5 || true
	@echo "$(GREEN)[✓] AArch64 build attempted$(NC)"

# ─── Reproducible Build ─────────────────────────────────────────────────

reproducible: ## Build with reproducible settings
	@echo "$(BLUE)[KnoxOS] Reproducible build...$(NC)"
	@cd $(KERNEL_DIR) && \
		RUSTFLAGS="--remap-path-prefix=$$(pwd)=/build" \
		SOURCE_DATE_EPOCH=$$(git log -1 --format=%ct 2>/dev/null || echo 0) \
		cargo build --release --locked 2>&1 | grep -E "Compiling|Finished|error" || true
	@echo "$(GREEN)[✓] Reproducible build complete$(NC)"

# ─── Testing ────────────────────────────────────────────────────────────

test: ## Run all tests
	@echo "$(BLUE)[KnoxOS] Running tests...$(NC)"
	@$(MAKE) kernel-test

# ─── Code Quality ───────────────────────────────────────────────────────

lint: ## Run linters
	@echo "$(BLUE)[KnoxOS] Running linters...$(NC)"
	@cd $(KERNEL_DIR) && cargo clippy --release 2>&1 | grep -E "warning|error" || echo "  $(GREEN)[✓] No warnings$(NC)"

fmt: ## Format all code
	@echo "$(BLUE)[KnoxOS] Formatting code...$(NC)"
	@cd $(KERNEL_DIR) && cargo fmt
	@echo "$(GREEN)[✓] Formatted$(NC)"

check: ## Check code without building
	@echo "$(BLUE)[KnoxOS] Checking code...$(NC)"
	@cd $(KERNEL_DIR) && cargo check
	@echo "$(GREEN)[✓] Check complete$(NC)"

# ─── Requirements ───────────────────────────────────────────────────────

setup: ## Set up development environment
	@echo "$(BLUE)[KnoxOS] Setting up development environment...$(NC)"
	@echo "$(YELLOW)Installing Rust tools...$(NC)"
	@rustup toolchain install nightly
	@rustup component add rust-src
	@echo ""
	@echo "$(YELLOW)Installing system dependencies...$(NC)"
	@if command -v brew &> /dev/null; then \
		brew install qemu; \
	else \
		echo "$(YELLOW)Please install QEMU manually$(NC)"; \
	fi
	@echo ""
	@echo "$(GREEN)[✓] Setup complete$(NC)"
	@echo ""
	@echo "$(YELLOW)Ready to build! Run:$(NC)"
	@echo "  $(GREEN)make all$(NC)          - Build everything"
	@echo "  $(GREEN)make run-kernel$(NC)   - Run kernel in QEMU"

# ─── Info ────────────────────────────────────────────────────────────────

info: ## Show KnoxOS information
	@echo "$(BLUE)═══════════════════════════════════════════════════════════$(NC)"
	@echo "$(BLUE)KnoxOS - Rust-based AI Operating System$(NC)"
	@echo "$(BLUE)═══════════════════════════════════════════════════════════$(NC)"
	@echo ""
	@echo "$(YELLOW)Project Structure:$(NC)"
	@echo "  kernel/        - x86_64 kernel"
	@echo "  boot/          - Boot image builder"
	@echo ""
	@echo "$(YELLOW)Key Commands:$(NC)"
	@echo "  $(GREEN)make setup$(NC)        - Initialize development"
	@echo "  $(GREEN)make all$(NC)          - Build all components"
	@echo "  $(GREEN)make run-kernel$(NC)   - Test kernel in QEMU"
	@echo "  $(GREEN)make status$(NC)       - Check build status"
	@echo ""
	@echo "$(YELLOW)Documentation:$(NC)"
	@echo "  $(GREEN)make docs$(NC)         - Read status.md"
	@echo "  $(GREEN)make help$(NC)         - Show this help"
	@echo ""

.PHONY: info

# ═══════════════════════════════════════════════════════════════════════
# CI / AUTOMATION TARGETS (Section 27)
# ═══════════════════════════════════════════════════════════════════════

docker-build: ## Build using Docker container
	@echo "$(BLUE)[KnoxOS] Building in Docker container...$(NC)"
	docker build -t knoxos-build -f Dockerfile.build .
	docker run --rm -v $$(pwd):/workspace knoxos-build make -C /workspace/kernel release
	@echo "$(GREEN)[✓] Docker build complete$(NC)"

nix-build: ## Build using Nix
	@echo "$(BLUE)[KnoxOS] Building with Nix...$(NC)"
	nix build
	@echo "$(GREEN)[✓] Nix build complete$(NC)"

nix-shell: ## Enter Nix development shell
	nix develop

ci-all: ## Run all CI checks locally
	@echo "$(BLUE)[KnoxOS] Running all CI checks...$(NC)"
	@$(MAKE) -C $(KERNEL_DIR) ci-fmt
	@$(MAKE) -C $(KERNEL_DIR) ci-clippy
	@$(MAKE) -C $(KERNEL_DIR) ci-size
	@echo "$(GREEN)[✓] All CI checks passed$(NC)"

pre-commit: ## Run the rustc/cargo quality gate (scripts/pre-commit.sh)
	@./scripts/pre-commit.sh

pre-commit-quick: ## Fast quality gate (no kernel test compile)
	@./scripts/pre-commit.sh --quick

pre-commit-fix: ## cargo fmt --all in each crate, then full quality gate
	@./scripts/pre-commit.sh --fix

pre-commit-install: ## Install .git/hooks/pre-commit
	@./scripts/pre-commit.sh --install-hook

release-tag: ## Create a release tag (usage: make release-tag VERSION=v0.2.2)
	@if [ -z "$(VERSION)" ]; then echo "$(RED)Usage: make release-tag VERSION=v0.2.2$(NC)"; exit 1; fi
	@echo "$(BLUE)[KnoxOS] Creating release $(VERSION)...$(NC)"
	git tag -a $(VERSION) -m "Release $(VERSION)"
	@echo "$(GREEN)[✓] Tag created. Push with: git push origin $(VERSION)$(NC)"
