#!/usr/bin/env bash
# Hash release disk images. Signing is optional: set GPG_KEY to a secret-key
# fingerprint and this script will detach-sign SHA256SUMS.
#
# Usage (after a release kernel + boot image build):
#   ./scripts/release-checksums.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
KERNEL_DIR="$ROOT/kernel/target/x86_64-unknown-none/release"
OUT="${RELEASE_OUT:-$ROOT/dist}"
mkdir -p "$OUT"

missing=0
for f in knoxos-kernel knoxos-bios.img knoxos-uefi.img; do
  if [ -f "$KERNEL_DIR/$f" ]; then
    cp "$KERNEL_DIR/$f" "$OUT/"
  else
    echo "missing $KERNEL_DIR/$f" >&2
    missing=1
  fi
done
if [ "$missing" -ne 0 ]; then
  echo "Build release images first (./run.sh --release or make kernel + boot)." >&2
  exit 1
fi

"$ROOT/scripts/make-iso.sh"

(
  cd "$OUT"
  HASH_FILES=(knoxos-kernel knoxos-bios.img knoxos-uefi.img)
  if [ -f knoxos.iso ]; then
    HASH_FILES+=(knoxos.iso)
  fi
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "${HASH_FILES[@]}" > SHA256SUMS
  else
    sha256sum "${HASH_FILES[@]}" > SHA256SUMS
  fi
  if [ -n "${GPG_KEY:-}" ]; then
    gpg --batch --yes --local-user "$GPG_KEY" --detach-sign --armor SHA256SUMS
    echo "wrote $OUT/SHA256SUMS.asc"
  else
    echo "SHA256SUMS written; set GPG_KEY to detach-sign. Secure Boot: ./scripts/secure-boot-enroll.sh"
  fi
  echo "artifacts in $OUT"
)
