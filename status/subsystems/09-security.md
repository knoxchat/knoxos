← [Status index](../README.md) · [Do this first](../next.md)

# 9. Security & Cryptography

**Grade: Wired (96%)** · `crypto.rs`, `tls.rs`, `seccomp.rs`, `random.rs`, `ssp.rs`

### Wired
- [x] AES-128/256 block + CBC (software)
- [x] SHA-256 / SHA-512
- [x] ChaCha20 block function + RFC 8439 known-answer (E2)
- [x] dm-crypt AES-XTS / LUKS **structures** + sector crypto helpers
- [x] `seccomp::check_syscall` denies listed syscalls with EPERM (E3)
- [x] Entropy pool + RDRAND mix; **ChaCha20 CSPRNG** for `getrandom` (E2)
- [x] `__stack_chk_fail` + canary (`ssp.rs`); compiler SSP not shown enabled
- [x] Unix permission bits on VFS inodes
- [x] W^X + NX on user maps; `mprotect` RWX fails; ASLR randomizes (E1)
- [x] CapNetBindService on `bind`; unprivileged `:80` fails; dropped on exec (E4)
- [x] Unimplemented security-sensitive syscalls (`bpf`, `pkey_alloc`/`pkey_free`/`pkey_mprotect`, `process_mrelease`, `quotactl`, `remap_file_pages`, `io_uring_*`, `userfaultfd`, `perf_event_open`, `fanotify_init`/`fanotify_mark`, Linux AIO `io_setup`/`io_submit`/`io_getevents`, `kexec_load`/`kexec_file_load`, `init_module`/`finit_module`/`delete_module`, `mount_setattr`, `fsopen`/`fsconfig`/`fsmount`/`move_mount`/`open_tree`/`fspick`, `add_key`/`request_key`/`keyctl`, `ioperm`, `iopl`, `acct`, `swapon`/`swapoff`, `modify_ldt`, `sysfs`, `vhangup`, `lookup_dcookie`, `memfd_secret`, `uselib`, `set_mempolicy`/`get_mempolicy`/`mbind`/`migrate_pages`/`move_pages`, `sched_setattr`/`sched_getattr`/`futex_waitv`, KVM vCPU regs) return ENOSYS (J4, L4, M4, N4, O4, P4, Q4, R4, S4, T4, U4, V4, W4, X4, Y4, Z4, AA4, AB4, AC4, AD4, AE4, AF4, AG4, AH4, AI4, AJ4, AK4, AL4, AM4, AN4, AO4)
- [x] Landlock deny-by-default on VFS open/write (K4)

### Stub / unused
- [x] AES-GCM, Poly1305 — NIST AES-128-GCM + RFC 8439 Poly1305 (`GATE_AESGCM kat`, `GATE_POLY1305 kat`). TLS 1.3 HKDF (`GATE_TLS13 hkdf`) plus ClientHello/ServerHello (`GATE_TLS13 hs`). X.509 chain later.
- [x] X25519 / Ed25519 — RFC 7748 Alice public + RFC 8032 TEST 1 (`GATE_X25519 kat`, `GATE_ED25519 kat`)
- [x] RSA / ECDSA-P256 — PKCS#1 v1.5 SHA-256 + RFC 6979 P-256 (`GATE_RSA kat`, `GATE_ECDSA kat`)
- [x] **Landlock** hooked on VFS (`GATE_K4 landlock vfs`). SELinux / AppArmor still unused.
- [x] Kernel lockdown, secure wipe on free, IMA — unsigned `insert_module` denied (`GATE_LOCKDOWN module`); freed buddy frames are zeros (`GATE_WIPE free`); exec SHA-256 measure (`GATE_IMA exec`)

### Perfect-OS next steps
1. ~~Replace xorshift userspace RNG with ChaCha20 from a real entropy pool (RDRAND + jitter + timings).~~ **E2 done.**
2. ~~W^X + NX on user maps the day Ring 3 boots.~~ **E1 done.**
3. ~~Deny-by-default seccomp for desktop apps; capabilities on `execve`.~~ Seccomp EPERM + CapNetBindService live; desktop default-deny still later.
4. Landlock on VFS is live (K4). MAC labels on disk still later.

---
