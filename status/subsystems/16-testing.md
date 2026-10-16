← [Status index](../README.md) · [Do this first](../next.md)

# 16. Testing & Quality

**Grade: Wired (86%)**

### Exists
- [x] `#[test_case]` framework + QEMU exit ports
- [x] Real tests: VFS read/write, allocator Box/Vec, some path tests (~subset of 103 `#[test_case]`)
- [x] `tests/run_integration.sh` waits for serial `Desktop Environment ready` plus C1–C9 / D1–D5 / E1–E4 / F1–F8 / H1–H4 / I1–I4 / J–BL / BM1–BM4 / G3–G5 / USB HID URB / XHCI MMIO / no-PS/2 cursor markers (**286/286**)

### Harmful
- [x] **`assert!(true)` tests removed** — widgets, VFS stress, DNS, TCP flags, creds, buddy, path normalize are real assertions
- [ ] No `#[test]` (expected for `no_std`, but host-side tests could exist)
- [x] SAFETY comments on every `unsafe {`; `undocumented_unsafe_blocks = deny`
- [ ] No Miri, no fuzz of ELF/ext4/packet parsers, no coverage

### Perfect-OS next steps
1. Delete or rewrite every `assert!(true)`.
2. Host unit tests for parsers (ELF, ext4 superblock, TCP checksum) without QEMU.
3. QEMU tests: boot marker, virtio-blk persist across reboot, later `hello` in Ring 3.
4. ~~SAFETY comments on every `unsafe`; clippy `undocumented_unsafe_blocks` as a gate.~~ **Done** (G5).

---
