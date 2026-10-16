#!/usr/bin/env bash
# Pack the BIOS boot image into dist/knoxos.iso.
#
# Prefers xorriso (El Torito). Falls back to a raw copy of knoxos-bios.img
# so checksum/signing still have an ISO-named artifact. Secure Boot keys
# and optional OVMF PK enrollment: ./scripts/secure-boot-enroll.sh
#
# Usage (after a release kernel + boot image build):
#   ./scripts/make-iso.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
KERNEL_DIR="$ROOT/kernel/target/x86_64-unknown-none/release"
OUT="${RELEASE_OUT:-$ROOT/dist}"
BIOS="$KERNEL_DIR/knoxos-bios.img"
ISO="$OUT/knoxos.iso"

if [ ! -f "$BIOS" ]; then
  echo "missing $BIOS — build release images first" >&2
  exit 1
fi
mkdir -p "$OUT"

if command -v xorriso >/dev/null 2>&1; then
  WORK="$(mktemp -d)"
  trap 'rm -rf "$WORK"' EXIT
  cp "$BIOS" "$WORK/knoxos-bios.img"
  xorriso -as mkisofs \
    -R -J \
    -V "KNOXOS" \
    -o "$ISO" \
    "$WORK"
  echo "wrote $ISO (xorriso)"
else
  cp "$BIOS" "$ISO"
  echo "wrote $ISO (raw bios image; install xorriso for El Torito)"
fi
