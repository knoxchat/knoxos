#!/usr/bin/env bash
#
# KnoxOS pre-commit — rustc/cargo quality gate for the kernel, boot, and tools.
#
# Install the git hook:
#   ./scripts/pre-commit.sh --install-hook
#
# Emergency bypass (hook only):
#   KNOX_SKIP_HOOK=1 git commit ...
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
KERNEL_DIR="$PROJECT_ROOT/kernel"
BOOT_DIR="$PROJECT_ROOT/boot"
TOOLS_DIR="$PROJECT_ROOT/tools/svg2rgba"
KERNEL_CRATE="knoxos-kernel"
KERNEL_TARGET="x86_64-unknown-none"

# Edition 2024 requires rustc 1.85+. kernel/Cargo.toml rust-version is 1.98.1.
MIN_RUSTC_MAJOR=1
MIN_RUSTC_MINOR=85

MODE="full"
FROM_HOOK=0
KEEP_GOING=0
APPLY_FMT=0
OFFLINE=0

export CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-always}"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-1}"
export CARGO_TERM_PROGRESS_WHEN="${CARGO_TERM_PROGRESS_WHEN:-auto}"

if [[ -t 1 ]]; then
  BOLD=$'\033[1m'
  DIM=$'\033[2m'
  RED=$'\033[31m'
  GREEN=$'\033[32m'
  YELLOW=$'\033[33m'
  CYAN=$'\033[36m'
  RESET=$'\033[0m'
else
  BOLD="" DIM="" RED="" GREEN="" YELLOW="" CYAN="" RESET=""
fi

PASSED_STEPS=()
FAILED_STEPS=()
SKIPPED_STEPS=()
SUITE_START=0
KEEP_GOING_FAILED=0
CARGO_COMMON=()
HOST_TARGET=""
CLIPPY_DENY=()

usage() {
  cat <<'EOF'
KnoxOS pre-commit — rustc/cargo quality gate for knoxos-kernel.

Usage:
  scripts/pre-commit.sh [options]

Modes:
  (default)       Full gate: hygiene, fmt, kernel+boot check, clippy,
                  compile kernel tests (no QEMU boot)
  --quick         Fast loop: hygiene, fmt, check, clippy (no tests)
  --strict        Full gate plus clippy extra warnings and cargo-audit /
                  cargo-deny when those tools are installed
  --fix           Apply `cargo fmt` in each crate then run the full gate

Git:
  --install-hook  Install .git/hooks/pre-commit (runs this script when
                  staged files touch kernel, boot, tools, or scripts)
  --from-hook     Internal: invoked by the git hook (skip if no Rust files)

Flags:
  --keep-going    Run every step even after a failure; exit non-zero at end
  --offline       Pass --offline to cargo (registry must already be cached)
  -h, --help      Show this help

Emergency bypass (hook only):
  KNOX_SKIP_HOOK=1 git commit ...
EOF
}

log() { printf '%s\n' "$*"; }
info() { log "${CYAN}==>${RESET} $*"; }
ok() { log "${GREEN}OK${RESET}  $*"; }
warn() { log "${YELLOW}WARN${RESET} $*"; }
err() { log "${RED}FAIL${RESET} $*" >&2; }

die() {
  err "$*"
  exit 1
}

have_cmd() {
  command -v "$1" >/dev/null 2>&1
}

elapsed_since() {
  local start="$1"
  local now
  now="$(date +%s)"
  printf '%s' "$((now - start))"
}

format_duration() {
  local secs="$1"
  if ((secs < 60)); then
    printf '%ss' "$secs"
  else
    printf '%dm%02ds' "$((secs / 60))" "$((secs % 60))"
  fi
}

record_pass() {
  local name="$1"
  local secs="$2"
  PASSED_STEPS+=("$name")
  ok "$name $(format_duration "$secs")"
}

print_summary() {
  local total
  total="$(elapsed_since "$SUITE_START")"
  log ""
  log "${BOLD}summary${RESET}  $(format_duration "$total")"
  if ((${#PASSED_STEPS[@]})); then
    log "  ${GREEN}passed${RESET}  ${PASSED_STEPS[*]}"
  fi
  if ((${#SKIPPED_STEPS[@]})); then
    log "  ${YELLOW}skipped${RESET} ${SKIPPED_STEPS[*]}"
  fi
  if ((${#FAILED_STEPS[@]})); then
    log "  ${RED}failed${RESET}  ${FAILED_STEPS[*]}"
    log ""
    log "Fix the failed step(s) and re-run: scripts/pre-commit.sh"
    return 1
  fi
  log "${GREEN}${BOLD}KnoxOS is commit-ready${RESET}"
  return 0
}

record_fail() {
  local name="$1"
  local secs="$2"
  FAILED_STEPS+=("$name")
  err "$name $(format_duration "$secs")"
  if ((KEEP_GOING)); then
    KEEP_GOING_FAILED=1
    return 0
  fi
  print_summary
  exit 1
}

record_skip() {
  local name="$1"
  local reason="$2"
  SKIPPED_STEPS+=("$name ($reason)")
  warn "$name skipped: $reason"
}

run_step() {
  local name="$1"
  shift
  local start now
  start="$(date +%s)"
  info "${BOLD}${name}${RESET}"
  if "$@"; then
    now="$(date +%s)"
    record_pass "$name" "$((now - start))"
  else
    now="$(date +%s)"
    record_fail "$name" "$((now - start))"
  fi
}

# Run cargo inside a crate so rust-toolchain.toml and .cargo/config.toml apply.
cargo_in() {
  local dir="$1"
  shift
  (cd "$dir" && cargo "$@")
}

cargo_common() {
  CARGO_COMMON=(--locked)
  if ((OFFLINE)); then
    CARGO_COMMON+=(--offline)
  fi
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --quick) MODE="quick" ;;
      --strict) MODE="strict" ;;
      --fix) APPLY_FMT=1 ;;
      --from-hook) FROM_HOOK=1 ;;
      --install-hook)
        install_git_hook
        exit 0
        ;;
      --keep-going) KEEP_GOING=1 ;;
      --offline) OFFLINE=1 ;;
      -h | --help)
        usage
        exit 0
        ;;
      *)
        die "Unknown option: $1 (see --help)"
        ;;
    esac
    shift
  done
}

repo_root() {
  git -C "$PROJECT_ROOT" rev-parse --show-toplevel 2>/dev/null || printf '%s' "$PROJECT_ROOT"
}

install_git_hook() {
  local root hook_dir hook
  root="$(repo_root)"
  hook_dir="$(git -C "$root" rev-parse --git-path hooks)"
  mkdir -p "$hook_dir"
  hook="$hook_dir/pre-commit"

  if [[ -e "$hook" ]] && ! grep -q 'scripts/pre-commit.sh' "$hook" 2>/dev/null; then
    die "Existing git hook at $hook does not invoke this script. Move it aside and re-run --install-hook."
  fi

  cat >"$hook" <<'HOOK'
#!/usr/bin/env bash
# Generated by scripts/pre-commit.sh --install-hook
set -euo pipefail
ROOT="$(git rev-parse --show-toplevel)"
exec "$ROOT/scripts/pre-commit.sh" --from-hook
HOOK
  chmod +x "$hook"
  ok "Installed git pre-commit hook: $hook"
  log "The hook runs the KnoxOS Rust gate when staged files touch kernel, boot, tools, or scripts."
}

knox_paths_staged() {
  git -C "$(repo_root)" diff --cached --name-only --diff-filter=ACMR |
    grep -E '^(kernel/|boot/|tools/|scripts/pre-commit\.sh|Makefile|run\.sh)' >/dev/null
}

maybe_skip_from_hook() {
  ((FROM_HOOK)) || return 0

  if [[ "${KNOX_SKIP_HOOK:-}" == "1" ]]; then
    warn "KNOX_SKIP_HOOK=1 — skipping KnoxOS pre-commit gate"
    exit 0
  fi

  if ! git -C "$PROJECT_ROOT" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    return 0
  fi

  if ! knox_paths_staged; then
    info "No staged KnoxOS Rust files; skipping pre-commit gate"
    exit 0
  fi
}

rustc_in_kernel() {
  (cd "$KERNEL_DIR" && rustc "$@")
}

cargo_in_kernel_version() {
  (cd "$KERNEL_DIR" && cargo --version)
}

print_banner() {
  log "${BOLD}KnoxOS pre-commit${RESET}  ${DIM}(${MODE})${RESET}"
  log "root:   $PROJECT_ROOT"
  log "cargo:  $(cargo_in_kernel_version 2>/dev/null || echo missing)"
  log "rustc:  $(rustc_in_kernel --version 2>/dev/null || echo missing)"
  log "host:   ${HOST_TARGET:-unknown}"
  log "kernel: $KERNEL_TARGET ($KERNEL_CRATE)"
  log ""
}

# --- checks ----------------------------------------------------------------

check_toolchain() {
  (
    cd "$KERNEL_DIR"

    have_cmd rustc || die "rustc not found. Install https://rustup.rs and retry."
    have_cmd cargo || die "cargo not found. Install the Rust toolchain and retry."

    if ! cargo fmt --version >/dev/null 2>&1; then
      die "rustfmt is missing. Run: rustup component add rustfmt"
    fi
    if ! cargo clippy --version >/dev/null 2>&1; then
      die "clippy is missing. Run: rustup component add clippy"
    fi
    if ! rustc --print sysroot >/dev/null 2>&1; then
      die "rustc sysroot is unavailable"
    fi
    if [[ ! -d "$(rustc --print sysroot)/lib/rustlib/src/rust/library/core" ]]; then
      die "rust-src is missing (needed for build-std). Run: rustup component add rust-src"
    fi

    local ver major minor
    ver="$(rustc --version | awk '{print $2}' | sed 's/-nightly//' | sed 's/-dev//')"
    major="${ver%%.*}"
    minor="${ver#*.}"
    minor="${minor%%.*}"
    if ((major < MIN_RUSTC_MAJOR)) ||
      { ((major == MIN_RUSTC_MAJOR)) && ((minor < MIN_RUSTC_MINOR)); }; then
      die "rustc $ver is too old; KnoxOS uses edition 2024 (need >= ${MIN_RUSTC_MAJOR}.${MIN_RUSTC_MINOR})"
    fi
  )
}

check_crate_layout() {
  [[ -f "$KERNEL_DIR/Cargo.toml" ]] || die "kernel Cargo.toml missing in $KERNEL_DIR"
  [[ -f "$KERNEL_DIR/Cargo.lock" ]] || die "kernel/Cargo.lock missing; run: (cd kernel && cargo generate-lockfile)"
  grep -q 'name = "knoxos-kernel"' "$KERNEL_DIR/Cargo.toml" ||
    die "kernel Cargo.toml package name is not knoxos-kernel"
  grep -q 'edition = "2024"' "$KERNEL_DIR/Cargo.toml" ||
    die "kernel edition is not 2024"
  [[ -f "$KERNEL_DIR/rust-toolchain.toml" ]] || die "kernel/rust-toolchain.toml missing"
  [[ -f "$BOOT_DIR/Cargo.toml" ]] || die "boot Cargo.toml missing in $BOOT_DIR"
}

check_lockfile() {
  cargo_in "$KERNEL_DIR" metadata --format-version 1 "${CARGO_COMMON[@]}" >/dev/null
  cargo_in "$BOOT_DIR" metadata --format-version 1 "${CARGO_COMMON[@]}" >/dev/null
}

check_hygiene() {
  local tmp leftovers
  tmp="$(mktemp)"

  if git -C "$PROJECT_ROOT" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    git -C "$PROJECT_ROOT" grep -nI -E '^(<<<<<<< |=======|>>>>>>> )' -- \
      kernel boot tools scripts Makefile run.sh \
      >"$tmp" 2>/dev/null || true
  else
    grep -RInE --include='*.rs' --include='*.toml' --include='*.sh' \
      '^(<<<<<<< |=======|>>>>>>> )' \
      "$PROJECT_ROOT/kernel" "$PROJECT_ROOT/boot" "$PROJECT_ROOT/tools" \
      "$PROJECT_ROOT/scripts" >"$tmp" 2>/dev/null || true
  fi
  if [[ -s "$tmp" ]]; then
    err "Merge conflict markers:"
    cat "$tmp" >&2
    rm -f "$tmp"
    return 1
  fi
  rm -f "$tmp"

  leftovers="$(find "$PROJECT_ROOT/kernel" "$PROJECT_ROOT/boot" "$PROJECT_ROOT/tools" \
    \( -name '*.rs.bk' -o -name '*~' \) \
    -not -path '*/target/*' 2>/dev/null | head -n 20 || true)"
  if [[ -n "$leftovers" ]]; then
    err "Leftover editor/rustfmt backup files:"
    printf '%s\n' "$leftovers" >&2
    return 1
  fi

  if ((FROM_HOOK)) && git -C "$(repo_root)" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    check_staged_lockfiles || return 1
  fi
}

check_staged_lockfiles() {
  local root crate
  root="$(repo_root)"
  for crate in kernel boot tools/svg2rgba; do
    if git -C "$root" diff --cached --name-only | grep -qx "$crate/Cargo.toml"; then
      if ! git -C "$root" diff --cached --name-only | grep -qx "$crate/Cargo.lock"; then
        err "$crate/Cargo.toml is staged but $crate/Cargo.lock is not"
        return 1
      fi
    fi
  done
}

check_dbg_macros() {
  local hits
  hits="$(grep -RIn --include='*.rs' --exclude-dir=target \
    -E '\bdbg!\s*\(' \
    "$PROJECT_ROOT/kernel" "$PROJECT_ROOT/boot" "$PROJECT_ROOT/tools" 2>/dev/null || true)"
  if [[ -n "$hits" ]]; then
    err "Remove dbg!(...) before commit:"
    printf '%s\n' "$hits" >&2
    return 1
  fi
}

run_fmt() {
  if ((APPLY_FMT)); then
    cargo_in "$KERNEL_DIR" fmt --all
    cargo_in "$BOOT_DIR" fmt --all
    if [[ -f "$TOOLS_DIR/Cargo.toml" ]]; then
      cargo_in "$TOOLS_DIR" fmt --all
    fi
  fi
  cargo_in "$KERNEL_DIR" fmt --all -- --check
  cargo_in "$BOOT_DIR" fmt --all -- --check
  if [[ -f "$TOOLS_DIR/Cargo.toml" ]]; then
    cargo_in "$TOOLS_DIR" fmt --all -- --check
  fi
}

run_check() {
  cargo_in "$KERNEL_DIR" check "${CARGO_COMMON[@]}" --target "$KERNEL_TARGET"
}

run_check_boot() {
  cargo_in "$BOOT_DIR" check "${CARGO_COMMON[@]}"
}

clippy_deny_args() {
  CLIPPY_DENY=(
    -D warnings
    -D clippy::dbg_macro
  )
  if [[ "$MODE" == "strict" ]]; then
    CLIPPY_DENY+=(
      -W clippy::cargo
      -W clippy::cast_possible_truncation
      -W clippy::doc_markdown
      -W clippy::manual_let_else
    )
  fi
}

run_clippy() {
  clippy_deny_args
  cargo_in "$KERNEL_DIR" clippy "${CARGO_COMMON[@]}" --target "$KERNEL_TARGET" -- "${CLIPPY_DENY[@]}"
}

run_clippy_boot() {
  clippy_deny_args
  cargo_in "$BOOT_DIR" clippy "${CARGO_COMMON[@]}" -- "${CLIPPY_DENY[@]}"
}

# Compile the kernel test harness. Do not boot QEMU (too slow for a git hook).
# Use a separate target dir: build-std after clippy otherwise rebuilds `core`
# with different cfg and hits duplicate lang item `sized`.
run_tests() {
  cargo_in "$KERNEL_DIR" test "${CARGO_COMMON[@]}" --target "$KERNEL_TARGET" \
    --target-dir target/precommit-test --no-run --quiet
}

run_optional_audit() {
  if ! cargo audit --help >/dev/null 2>&1; then
    record_skip "cargo-audit" "install with: cargo install cargo-audit"
    return 0
  fi
  local start now
  start="$(date +%s)"
  info "${BOLD}cargo-audit${RESET}"
  if (cd "$KERNEL_DIR" && cargo audit); then
    now="$(date +%s)"
    record_pass "cargo-audit" "$((now - start))"
  else
    now="$(date +%s)"
    record_fail "cargo-audit" "$((now - start))"
  fi
}

run_optional_deny() {
  if ! cargo deny --help >/dev/null 2>&1; then
    record_skip "cargo-deny" "install with: cargo install cargo-deny"
    return 0
  fi
  if [[ ! -f "$PROJECT_ROOT/deny.toml" ]] && [[ ! -f "$KERNEL_DIR/deny.toml" ]]; then
    record_skip "cargo-deny" "no deny.toml"
    return 0
  fi
  local start now deny_dir
  deny_dir="$KERNEL_DIR"
  [[ -f "$PROJECT_ROOT/deny.toml" ]] && deny_dir="$PROJECT_ROOT"
  start="$(date +%s)"
  info "${BOLD}cargo-deny${RESET}"
  if (cd "$deny_dir" && cargo deny check); then
    now="$(date +%s)"
    record_pass "cargo-deny" "$((now - start))"
  else
    now="$(date +%s)"
    record_fail "cargo-deny" "$((now - start))"
  fi
}

# --- main ------------------------------------------------------------------

parse_args "$@"
maybe_skip_from_hook

cd "$PROJECT_ROOT"
HOST_TARGET="$(rustc_in_kernel -vV 2>/dev/null | sed -n 's/^host: //p' || true)"
cargo_common
SUITE_START="$(date +%s)"
print_banner

check_crate_layout
run_step "toolchain" check_toolchain
run_step "lockfile" check_lockfile
run_step "hygiene" check_hygiene
run_step "no-dbg" check_dbg_macros
run_step "fmt" run_fmt
run_step "check" run_check
run_step "check-boot" run_check_boot
run_step "clippy" run_clippy
run_step "clippy-boot" run_clippy_boot

if [[ "$MODE" != "quick" ]]; then
  run_step "test" run_tests
fi

if [[ "$MODE" == "strict" ]]; then
  run_optional_audit
  run_optional_deny
fi

print_summary
if ((KEEP_GOING_FAILED)); then
  exit 1
fi
exit 0
