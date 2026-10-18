← [Status index](../README.md) · [Do this first](../next.md)

# 16. Testing & Quality

**Grade: Wired (98%)**

### Exists
- [x] `#[test_case]` framework + QEMU exit ports
- [x] Real tests: VFS read/write, allocator Box/Vec, some path tests (~subset of 103 `#[test_case]`)
- [x] `tests/run_integration.sh` waits for serial `Desktop Environment ready` plus C1–C9 / D1–D6 / E1–E4 / F1–F8 / H1–H4 / I1–I4 / J–BL / BM1–BM4 / G3–G5 / USB HID URB / XHCI MMIO / no-PS/2 cursor / udev / quota / cron / vsync / TERM / WL / GPU / cgroup / job / IMA / lockdown / UEFI / wipe / NM / NTFS / heredoc / profile / VT100 / HDA / PA / IOMMU / THP / KASLR / AML / KO / NUMA / KSM / nested-IRQ / livepatch / exFAT / CPU-IO / suspend / logind / AES-GCM / Poly1305 / font-fallback / XFS / X25519 / Ed25519 / locale / IME / MSC / NAT / DRM / GSUB / RSA / ECDSA / emoji / hub / gamepad / TPM / TTF / I2C / TLS13 / HDR / VRR / e1000 / eBPF / Wi-Fi / i915 / Btrfs / VLAN / USB audio / trackpad / wacom / UVC / ethernet / touch / GPIO / rtl8139 / CIFS / GPOS / AMDGPU / AX211 / FUSE / KVM / OCI / ZFS / WPA3 / bridge / TLS-HS / GPU matmul / AI / NFS / BRCM / TB markers (**379/379**)

### Harmful
- [x] **`assert!(true)` tests removed** — widgets, VFS stress, DNS, TCP flags, creds, buddy, path normalize are real assertions
- [x] Host-side `#[test]` in `tools/host-tests` (`cargo test -p knoxos-host-tests`)
- [x] SAFETY comments on every `unsafe {`; `undocumented_unsafe_blocks = deny`
- [x] Host fuzz of ELF / ext4 / packet parsers (`cargo test -p knoxos-host-tests`). No Miri, no coverage.

### Perfect-OS next steps
1. Delete or rewrite every `assert!(true)`.
2. ~~Host unit tests for parsers (ELF, ext4 superblock, TCP checksum) without QEMU.~~ **Done** (`cargo test -p knoxos-host-tests`), including malformed-input fuzz.
3. QEMU tests: boot marker, virtio-blk persist across reboot, later `hello` in Ring 3.
4. ~~SAFETY comments on every `unsafe`; clippy `undocumented_unsafe_blocks` as a gate.~~ **Done** (G5).

---
