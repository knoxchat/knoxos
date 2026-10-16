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
TIMEOUT_SECS="${SB_TIMEOUT:-90}"

die() { echo "$*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

if ! have qemu-system-x86_64; then
  die "qemu-system-x86_64 is required"
fi

"$ROOT/scripts/secure-boot-enroll.sh"

if ! have virt-fw-vars; then
  echo "virt-fw-vars not installed; cannot enroll OVMF vars"
  echo "GATE_SB ovmf boot skipped"
  exit 0
fi

VARS_ENROLLED="$OUT/OVMF_VARS.knoxos.fd"
if [ ! -f "$VARS_ENROLLED" ]; then
  echo "OVMF vars were not enrolled; cannot prove Secure Boot"
  echo "GATE_SB ovmf boot skipped"
  exit 0
fi

find_secboot_code() {
  local candidates=(
    "${OVMF_CODE:-}"
    "/usr/share/OVMF/OVMF_CODE_4M.secboot.fd"
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
  echo "no OVMF Secure Boot CODE firmware found"
  echo "GATE_SB ovmf boot skipped"
  exit 0
}

UEFI_IMG="${UEFI_IMG:-$ROOT/kernel/target/x86_64-unknown-none/release/knoxos-uefi.img}"
if [ ! -f "$UEFI_IMG" ]; then
  die "UEFI image missing: $UEFI_IMG (build with boot/ first)"
fi

SIGNED_IMG="$OUT/knoxos-uefi.signed.img"
cp "$UEFI_IMG" "$SIGNED_IMG"

# Authenticode needs the PE/COFF EFI app, not the GPT. Extract, sign, put back.
if have sbsign && have mcopy; then
  ESP="$OUT/BOOTX64.EFI"
  SIGNED_PE="$OUT/BOOTX64.signed.EFI"
  # bootloader 0.11 ESP typically starts at 1 MiB.
  if mcopy -i "${UEFI_IMG}@@1M" ::/EFI/BOOT/BOOTX64.EFI "$ESP" 2>/dev/null \
    || mcopy -i "$UEFI_IMG" ::/EFI/BOOT/BOOTX64.EFI "$ESP" 2>/dev/null; then
    sbsign --key "$OUT/db.key" --cert "$OUT/db.crt" --output "$SIGNED_PE" "$ESP"
    mcopy -o -i "${SIGNED_IMG}@@1M" "$SIGNED_PE" ::/EFI/BOOT/BOOTX64.EFI 2>/dev/null \
      || mcopy -o -i "$SIGNED_IMG" "$SIGNED_PE" ::/EFI/BOOT/BOOTX64.EFI
    echo "signed BOOTX64.EFI and wrote it back to $SIGNED_IMG"
  else
    echo "could not extract BOOTX64.EFI; booting unsigned image (may fail SB)"
  fi
fi

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
