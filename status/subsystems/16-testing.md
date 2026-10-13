← [Status index](../README.md) · [Do this first](../next.md)

# 16. Testing & Quality

**Grade: Wired (82%)**

### Exists
- [x] `#[test_case]` framework + QEMU exit ports
- [x] Real tests: VFS read/write, allocator Box/Vec, some path tests (~subset of 103 `#[test_case]`)
- [x] `tests/run_integration.sh` waits for serial `Desktop Environment ready` plus C1–C6 / D1–D5 / E1–E4 / F1–F4 / H1–H4 / I1–I3 / J1–J4 / K1–K4 / L1–L4 / M1–M4 / N1–N4 / O1–O4 / P1–P4 / Q1–Q4 / R1–R4 / S1–S4 / T1–T4 / U1–U4 / V1–V4 / W1–W4 / X1–X4 / Y1–Y4 / Z1–Z4 / AA1–AA4 / AB1–AB4 / AC1–AC4 / AD1–AD4 / AE1–AE4 / AF1–AF4 / AG1–AG4 / AH1–AH4 / AI1–AI4 / AJ1–AJ4 / AK1–AK4 / AL1–AL4 / AM1–AM4 / AN1–AN4 / AO1–AO4 / AP1–AP4 / AQ1–AQ4 / AR1–AR4 / AS1–AS4 / AT1–AT4 / AU1–AU4 / AV1–AV4 markers

### Harmful
- [x] **`assert!(true)` tests removed** — widgets, VFS stress, DNS, TCP flags, creds, buddy, path normalize are real assertions
- [ ] No `#[test]` (expected for `no_std`, but host-side tests could exist)
- [ ] 300+ `unsafe` blocks without a systematic SAFETY audit
- [ ] No Miri, no fuzz of ELF/ext4/packet parsers, no coverage

### Perfect-OS next steps
1. Delete or rewrite every `assert!(true)`.
2. Host unit tests for parsers (ELF, ext4 superblock, TCP checksum) without QEMU.
3. QEMU tests: boot marker, virtio-blk persist across reboot, later `hello` in Ring 3.
4. SAFETY comments on every `unsafe`; clippy `undocumented_unsafe_blocks` as a gate.

---
