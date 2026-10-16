#!/usr/bin/env bash
# Prove KnoxOS boots under OVMF Secure Boot with enrolled PK/KEK/db.
#
# Host-side gate: firmware is the policy engine. Success prints
# `GATE_SB ovmf boot` after the signed UEFI image reaches KnoxOS serial.
#
# Required on PATH (Ubuntu: apt install ovmf python3-virt-firmware
# sbsigntool mtools qemu-system-x86):
#   virt-fw-vars, sbsign, mcopy, qemu-system-x86_64
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${SBKEYS_OUT:-$ROOT/dist/sbkeys}"
SERIAL="${SB_SERIAL:-$ROOT/tests/sb-serial.log}"
TIMEOUT_SECS="${SB_TIMEOUT:-120}"

die() { echo "$*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

if ! have qemu-system-x86_64; then
  die "qemu-system-x86_64 is required"
fi

"$ROOT/scripts/secure-boot-enroll.sh"

sb_skip() {
  echo "$*"
  echo "GATE_SB ovmf boot skipped"
  if [ "${GITHUB_ACTIONS:-}" = "true" ]; then
    echo "refusing to skip Secure Boot proof on GitHub Actions" >&2
    exit 1
  fi
  exit 0
}

if ! have virt-fw-vars; then
  sb_skip "virt-fw-vars not installed; cannot enroll OVMF vars"
fi

VARS_ENROLLED="$OUT/OVMF_VARS.knoxos.fd"
if [ ! -f "$VARS_ENROLLED" ]; then
  sb_skip "OVMF vars were not enrolled; cannot prove Secure Boot"
fi

find_secboot_code() {
  local candidates=(
    "${OVMF_CODE:-}"
    "/usr/share/OVMF/OVMF_CODE_4M.secboot.fd"
    "/usr/share/edk2/ovmf/OVMF_CODE_4M.secboot.fd"
    "/usr/share/edk2-ovmf/x64/OVMF_CODE_4M.secboot.fd"
    "/usr/share/OVMF/OVMF_CODE.secboot.fd"
    "/usr/share/edk2/ovmf/OVMF_CODE.secboot.fd"
    "/usr/share/edk2-ovmf/x64/OVMF_CODE.secboot.fd"
    "/opt/homebrew/share/qemu/edk2-x86_64-secure-code.fd"
    "/usr/local/share/qemu/edk2-x86_64-secure-code.fd"
  )
  local c
  for c in "${candidates[@]}"; do
    [ -n "$c" ] && [ -f "$c" ] && printf '%s' "$c" && return 0
  done
  return 1
}

CODE="$(find_secboot_code)" || {
  sb_skip "no OVMF Secure Boot CODE firmware found"
}

UEFI_IMG="${UEFI_IMG:-$ROOT/kernel/target/x86_64-unknown-none/release/knoxos-uefi.img}"
if [ ! -f "$UEFI_IMG" ]; then
  die "UEFI image missing: $UEFI_IMG (build with boot/ first)"
fi

have sbsign || die "sbsign is required to Authenticode-sign BOOTX64.EFI"
have mcopy || die "mcopy (mtools) is required to extract BOOTX64.EFI from the GPT"
have python3 || die "python3 is required to locate the ESP"

# mtools rejects some GPT-hosted FAT images unless this is set.
export MTOOLS_SKIP_CHECK=1

esp_offset() {
  python3 -c '
import struct, sys
path = sys.argv[1]
esp_type = bytes.fromhex("28732ac11ff8d211ba4b00a0c93ec93b")
with open(path, "rb") as f:
    f.seek(512)
    hdr = f.read(96)
    if hdr[:8] != b"EFI PART":
        sys.exit(1)
    part_lba, nparts, psz = struct.unpack_from("<QII", hdr, 0x48)
    if psz < 48 or nparts == 0:
        sys.exit(1)
    f.seek(part_lba * 512)
    for _ in range(nparts):
        entry = f.read(psz)
        if len(entry) < psz:
            break
        if entry[:16] == esp_type:
            first = struct.unpack_from("<Q", entry, 32)[0]
            print(first * 512)
            sys.exit(0)
sys.exit(1)
' "$1"
}

SIGNED_IMG="$OUT/knoxos-uefi.signed.img"
cp "$UEFI_IMG" "$SIGNED_IMG"

ESP_OFF="$(esp_offset "$UEFI_IMG" || true)"
if [ -z "$ESP_OFF" ]; then
  ESP_OFF=1048576
fi

ESP="$OUT/BOOTX64.EFI"
SIGNED_PE="$OUT/BOOTX64.signed.EFI"
rm -f "$ESP" "$SIGNED_PE"

mcopy -i "${UEFI_IMG}@@${ESP_OFF}" ::/EFI/BOOT/BOOTX64.EFI "$ESP" 2>/dev/null \
  || mcopy -i "${UEFI_IMG}@@1M" ::/EFI/BOOT/BOOTX64.EFI "$ESP" 2>/dev/null \
  || mcopy -i "$UEFI_IMG" ::/EFI/BOOT/BOOTX64.EFI "$ESP" 2>/dev/null \
  || die "could not extract BOOTX64.EFI from $UEFI_IMG"

if [ "$(head -c 2 "$ESP")" != "MZ" ]; then
  die "extracted BOOTX64.EFI is not PE/COFF (missing MZ DOS header)"
fi

sbsign --key "$OUT/db.key" --cert "$OUT/db.crt" --output "$SIGNED_PE" "$ESP"
mcopy -o -i "${SIGNED_IMG}@@${ESP_OFF}" "$SIGNED_PE" ::/EFI/BOOT/BOOTX64.EFI 2>/dev/null \
  || mcopy -o -i "${SIGNED_IMG}@@1M" "$SIGNED_PE" ::/EFI/BOOT/BOOTX64.EFI 2>/dev/null \
  || mcopy -o -i "$SIGNED_IMG" "$SIGNED_PE" ::/EFI/BOOT/BOOTX64.EFI \
  || die "could not write signed BOOTX64.EFI back to $SIGNED_IMG"
echo "signed BOOTX64.EFI and wrote it back to $SIGNED_IMG"

rm -f "$SERIAL"
VARS_RUN="$OUT/OVMF_VARS.knoxos.run.fd"
cp "$VARS_ENROLLED" "$VARS_RUN"

qemu-system-x86_64 \
  -machine q35,smm=on \
  -global driver=cfi.pflash01,property=secure,value=on \
  -drive if=pflash,format=raw,readonly=on,file="$CODE" \
  -drive if=pflash,format=raw,file="$VARS_RUN" \
  -drive format=raw,file="$SIGNED_IMG" \
  -serial file:"$SERIAL" \
  -display none \
  -m 2G \
  -smp 2 \
  -cpu qemu64,+ssse3,+sse4.1,+sse4.2,+popcnt \
  -no-reboot \
  -no-shutdown &
QEMU_PID=$!

ok=0
for _ in $(seq 1 "$TIMEOUT_SECS"); do
  sleep 1
  if [ -f "$SERIAL" ] && grep -qE "Booting KnoxOS|Desktop Environment ready|GATE_G5" "$SERIAL" 2>/dev/null; then
    ok=1
    break
  fi
  if ! kill -0 "$QEMU_PID" 2>/dev/null; then
    break
  fi
done
kill "$QEMU_PID" 2>/dev/null || true
wait "$QEMU_PID" 2>/dev/null || true

if [ "$ok" -eq 1 ]; then
  echo "GATE_SB ovmf boot"
  exit 0
fi
echo "Secure Boot QEMU did not reach KnoxOS" >&2
tail -40 "$SERIAL" 2>/dev/null || true
exit 1
