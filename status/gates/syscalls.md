# Gates J–BL — syscall / namespace live path

← [Status index](../README.md) · [Do this first](../next.md) · [Gates](README.md)

These gates prove Linux syscalls and namespaces on the QEMU boot path. They are **done through BL**. Adding another `GATE_xx` ENOSYS is not the critical path.

### Gate J — Threads, buddy RAM, reclaim, honest syscalls (after I)

| ID | Task | Done when |
|----|------|-----------|
| J1 | `clone(CLONE_VM)` child runs on shared CR3 | **Done** — child writes `thread child ran`; parent `wait4`s (`GATE_J1 thread clone`) |
| J2 | Buddy over leftover RAM | **Done** — remaining usable frames enter the buddy; alloc/free restores (`GATE_J2 buddy ram`) |
| J3 | Page-cache LRU reclaim | **Done** — shrink drops oldest clean pages; dirty survives (`GATE_J3 lru reclaim`) |
| J4 | No silent `Ok(0)` for unimplemented security syscalls | **Done** — Ring 3 `bpf` returns `-ENOSYS` (`GATE_J4 enosys`) |

### Gate K — Threads join, swap I/O, init, Landlock (after J)

| ID | Task | Done when |
|----|------|-----------|
| K1 | `clone(CLONE_THREAD)` + `thread_join` | **Done** — child exit status 42; parent writes (`GATE_K1 thread join`) |
| K2 | Swap I/O + `#PF` swap-in | **Done** — ram-backed slot restores magic bytes (`GATE_K2 swap io`) |
| K3 | Ring 3 `/sbin/init` | **Done** — PID 1 `iretq`s and parks on `wait4` (`GATE_K3 init userspace`) |
| K4 | Landlock on VFS | **Done** — deny-by-default write outside allow path (`GATE_K4 landlock vfs`) |

### Gate L — TLS, namespaces, D-Bus socket, honest syscalls (after K)

| ID | Task | Done when |
|----|------|-----------|
| L1 | `arch_prctl(ARCH_SET_FS)` + `%fs:0` | **Done** — Ring 3 reads magic at the programmed FS base (`GATE_L1 tls fs`) |
| L2 | UTS namespace isolation | **Done** — child `sethostname` does not change parent (`GATE_L2 uts ns`) |
| L3 | D-Bus `AF_UNIX` socket | **Done** — connect/send/recv on `/run/dbus/system_bus_socket` (`GATE_L3 dbus unix`) |
| L4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `quotactl` returns `-ENOSYS` (`GATE_L4 enosys`) |

### Gate M — PID ns, pipes, futex, honest syscalls (after L)

| ID | Task | Done when |
|----|------|-----------|
| M1 | PID namespace isolation | **Done** — `unshare(CLONE_NEWPID)` child `getpid` is 1; parent unchanged (`GATE_M1 pid ns`) |
| M2 | `pipe()` write/read round-trip | **Done** — Ring 3 write/read through the shared ipc buffer (`GATE_M2 pipe`) |
| M3 | futex wait/wake | **Done** — `clone(CLONE_VM)` child `FUTEX_WAIT`s; parent `FUTEX_WAKE`s (`GATE_M3 futex`) |
| M4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `io_uring_setup(0, 0)` returns `-ENOSYS` (`GATE_M4 enosys`); `io_uring_setup(8)` is a ring fd (`GATE_CHR iouring`) |

### Gate N — Mount ns, socketpair, eventfd, honest syscalls (after M)

| ID | Task | Done when |
|----|------|-----------|
| N1 | Mount namespace isolation | **Done** — `unshare(CLONE_NEWNS)` bind mount is not visible to the parent (`GATE_N1 mount ns`); `remove_mount` drops it again on umount (`GATE_UMOUNT live`) |
| N2 | `socketpair` write/read round-trip | **Done** — Ring 3 AF_UNIX pair write/read (`GATE_N2 socketpair`) |
| N3 | `eventfd` write/read | **Done** — Ring 3 counter write then read (`GATE_N3 eventfd`) |
| N4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `userfaultfd` returns `-ENOSYS` (`GATE_N4 enosys`) |

### Gate O — Net ns, epoll, memfd, honest syscalls (after N)

| ID | Task | Done when |
|----|------|-----------|
| O1 | Network namespace isolation | **Done** — `unshare(CLONE_NEWNET)` child does not see `eth0`; child `veth0` is private (`GATE_O1 net ns`) |
| O2 | `epoll` wait on a pipe | **Done** — Ring 3 `epoll_create1`/`epoll_ctl`/`epoll_wait` sees `EPOLLIN` after write (`GATE_O2 epoll`) |
| O3 | `memfd_create` write/read | **Done** — Ring 3 write then `lseek`/`read` round-trip (`GATE_O3 memfd`) |
| O4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `perf_event_open` returns `-ENOSYS` (`GATE_O4 enosys`) |

### Gate P — User ns, timerfd, signalfd, honest syscalls (after O)

| ID | Task | Done when |
|----|------|-----------|
| P1 | User namespace isolation | **Done** — `unshare(CLONE_NEWUSER)` child is uid 0; parent unchanged (`GATE_P1 user ns`) |
| P2 | `timerfd` expire/read | **Done** — Ring 3 `timerfd_create`/`timerfd_settime`; `read` returns an expiration (`GATE_P2 timerfd`) |
| P3 | `signalfd` SIGUSR1 | **Done** — Ring 3 `signalfd` + `kill(SIGUSR1)`; `read` returns signo 10 (`GATE_P3 signalfd`) |
| P4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `fanotify_init` returns `-ENOSYS` (`GATE_P4 enosys`) |

### Gate Q — IPC ns, poll, inotify, honest syscalls (after P)

| ID | Task | Done when |
|----|------|-----------|
| Q1 | IPC namespace isolation | **Done** — `unshare(CLONE_NEWIPC)` child does not see the parent's SysV shm key; child-only key stays private (`GATE_Q1 ipc ns`) |
| Q2 | `poll` wait on a pipe | **Done** — Ring 3 `poll` stays idle on an empty pipe and returns `POLLIN` after write (`GATE_Q2 poll`) |
| Q3 | Ring 3 `inotify` | **Done** — `inotify_init1` + `add_watch(/tmp)` + `open(O_CREAT)`; `read` returns a create event (`GATE_Q3 inotify`) |
| Q4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `io_setup` returns `-ENOSYS` (`GATE_Q4 enosys`) |

### Gate R — Cgroup ns, splice, flock, honest syscalls (after Q)

| ID | Task | Done when |
|----|------|-----------|
| R1 | Cgroup namespace isolation | **Done** — `unshare(CLONE_NEWCGROUP)` child does not see `/system.slice`; child-only path stays private (`GATE_R1 cgroup ns`) |
| R2 | `splice` pipe to pipe | **Done** — Ring 3 `splice` moves a byte from one pipe to another (`GATE_R2 splice`) |
| R3 | `flock` exclusive lock | **Done** — Ring 3 `flock(LOCK_EX)` then `LOCK_UN` on a VFS file (`GATE_R3 flock`) |
| R4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `kexec_load` returns `-ENOSYS` (`GATE_R4 enosys`) |

### Gate S — Time ns, sendfile, tee, honest syscalls (after R)

| ID | Task | Done when |
|----|------|-----------|
| S1 | Time namespace isolation | **Done** — `unshare(CLONE_NEWTIME)` monotonic offset does not change the parent (`GATE_S1 time ns`) |
| S2 | `sendfile` file to pipe | **Done** — Ring 3 `sendfile` copies a byte from a file into a pipe (`GATE_S2 sendfile`) |
| S3 | `tee` without consume | **Done** — Ring 3 `tee` duplicates a pipe byte and the source can still be read (`GATE_S3 tee`) |
| S4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `init_module` returns `-ENOSYS` (`GATE_S4 enosys`) |

### Gate T — setns, copy_file_range, vmsplice, honest syscalls (after S)

| ID | Task | Done when |
|----|------|-----------|
| T1 | `setns` joins an existing namespace | **Done** — joiner sees the owner's UTS hostname; parent unchanged (`GATE_T1 setns`) |
| T2 | `copy_file_range` file to file | **Done** — Ring 3 copies a byte from one VFS file into another (`GATE_T2 copy_file_range`) |
| T3 | `vmsplice` user pages into a pipe | **Done** — Ring 3 `vmsplice` copies a user byte into a pipe (`GATE_T3 vmsplice`) |
| T4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `mount_setattr` returns `-ENOSYS` (`GATE_T4 enosys`) |

### Gate U — chroot, xattr, statx, honest syscalls (after T)

| ID | Task | Done when |
|----|------|-----------|
| U1 | `chroot` jails path lookup | **Done** — child cannot see `/etc`; `/../etc` stays in the jail; parent unchanged (`GATE_U1 chroot`) |
| U2 | `setxattr`/`getxattr` round-trip | **Done** — Ring 3 writes `user.knox` and reads it back (`GATE_U2 xattr`) |
| U3 | `statx` reports file size | **Done** — Ring 3 `statx` returns `stx_size == 1` (`GATE_U3 statx`) |
| U4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `fsopen` returns `-ENOSYS` (`GATE_U4 enosys`) |

### Gate V — pivot_root, fallocate, utimensat, honest syscalls (after U)

| ID | Task | Done when |
|----|------|-----------|
| V1 | `pivot_root` swaps the process root | **Done** — `/ok` is in `new_root`; `/etc` is not; `/old/etc` is the previous `/etc`; parent unchanged (`GATE_V1 pivot_root`) |
| V2 | `fallocate` extends a file | **Done** — Ring 3 `fallocate` makes `statx` report `stx_size == 8` (`GATE_V2 fallocate`) |
| V3 | `utimensat` sets mtime | **Done** — Ring 3 `utimensat` then `statx` `stx_mtime.tv_sec == 123456789` (`GATE_V3 utimensat`) |
| V4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `keyctl` returns `-ENOSYS` (`GATE_V4 enosys`) |

### Gate W — OverlayFS, umask, symlink, honest syscalls (after V)

| ID | Task | Done when |
|----|------|-----------|
| W1 | OverlayFS merges VFS layers | **Done** — lower+upper visible at merge; whiteout hides; parent mount ns unchanged (`GATE_W1 overlayfs`) |
| W2 | `umask` applied on creat | **Done** — Ring 3 `umask(077)` + `open(O_CREAT, 0666)`; `statx` `stx_mode == 0100600` (`GATE_W2 umask`) |
| W3 | `symlink`/`readlink` round-trip | **Done** — Ring 3 writes a symlink and `readlink`s `x` (`GATE_W3 symlink`) |
| W4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `ioperm` returns `-ENOSYS` (`GATE_W4 enosys`) |

### Gate X — Hard links, rename, truncate, honest syscalls (after W)

| ID | Task | Done when |
|----|------|-----------|
| X1 | VFS hard links share an inode | **Done** — two names, same ino; write via link visible on original; unlink drops nlink to 1 (`GATE_X1 hardlink`) |
| X2 | `rename` file | **Done** — Ring 3 writes a byte, `rename`s, reads it from the new path (`GATE_X2 rename`) |
| X3 | `truncate` shrinks a file | **Done** — Ring 3 writes 8 bytes, `truncate`s to 1, `statx` `stx_size == 1` (`GATE_X3 truncate`) |
| X4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `iopl` returns `-ENOSYS` (`GATE_X4 enosys`) |

### Gate Y — chmod, chown, mkdir, honest syscalls (after X)

| ID | Task | Done when |
|----|------|-----------|
| Y1 | VFS `chmod` is enforced | **Done** — mode 0400: owner can read but not write; other cannot read (`GATE_Y1 chmod`) |
| Y2 | `chown` then `statx` uid | **Done** — Ring 3 `chown` reports `stx_uid == 1000` (`GATE_Y2 chown`) |
| Y3 | `mkdir` then `statx` dir | **Done** — Ring 3 `mkdir`; `statx` `S_IFDIR` (`GATE_Y3 mkdir`) |
| Y4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `acct` returns `-ENOSYS` (`GATE_Y4 enosys`) |

### Gate Z — rmdir, unlink, chdir, honest syscalls (after Y)

| ID | Task | Done when |
|----|------|-----------|
| Z1 | VFS `rmdir` removes empty dirs | **Done** — empty dir gone; non-empty ENOTEMPTY (`GATE_Z1 rmdir`) |
| Z2 | `unlink` then `open` fails | **Done** — Ring 3 creates, unlinks, `open` fails (`GATE_Z2 unlink`) |
| Z3 | `chdir` then `getcwd` | **Done** — Ring 3 `getcwd` is `/tmp/gate_z3` (`GATE_Z3 chdir`) |
| Z4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `swapon` returns `-ENOSYS` (`GATE_Z4 enosys`) |

### Gate AA — mkfifo, fchdir, access, honest syscalls (after Z)

| ID | Task | Done when |
|----|------|-----------|
| AA1 | Named FIFO write/read | **Done** — `mkfifo` then write a byte that a reader reads back (`GATE_AA1 mkfifo`) |
| AA2 | `fchdir` then `getcwd` | **Done** — Ring 3 `getcwd` is `/tmp/gate_aa2` (`GATE_AA2 fchdir`) |
| AA3 | `access(F_OK)` | **Done** — created file is OK; missing path fails (`GATE_AA3 access`) |
| AA4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `modify_ldt` returns `-ENOSYS` (`GATE_AA4 enosys`) |

### Gate AB — getdents, dup2, uname, honest syscalls (after AA)

| ID | Task | Done when |
|----|------|-----------|
| AB1 | VFS directory listing | **Done** — `list_dir` includes a file created in the directory (`GATE_AB1 getdents`) |
| AB2 | `dup2` then read via new fd | **Done** — Ring 3 writes a byte, `dup2`s, `lseek`/`read`s it back (`GATE_AB2 dup2`) |
| AB3 | `uname` reports KnoxOS | **Done** — Ring 3 `uname` `sysname == "KnoxOS"` (`GATE_AB3 uname`) |
| AB4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `sysfs` returns `-ENOSYS` (`GATE_AB4 enosys`) |

### Gate AC — fcntl, pread64, getuid, honest syscalls (after AB)

| ID | Task | Done when |
|----|------|-----------|
| AC1 | `fcntl` CLOEXEC / DUPFD | **Done** — `F_SETFD`/`F_GETFD` toggle cloexec; `dup` clears it (`GATE_AC1 fcntl`) |
| AC2 | `pread64` without moving offset | **Done** — Ring 3 writes `xy`, `pread64` at 1 returns `y`; `lseek` CUR stays 2 (`GATE_AC2 pread64`) |
| AC3 | `getuid` from Ring 3 | **Done** — boot task `getuid` is 0 (`GATE_AC3 getuid`) |
| AC4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `vhangup` returns `-ENOSYS` (`GATE_AC4 enosys`) |

### Gate AD — fstat, pwrite64, getgid, honest syscalls (after AC)

| ID | Task | Done when |
|----|------|-----------|
| AD1 | `fstat` reports file size | **Done** — written VFS file `st_size == 8` (`GATE_AD1 fstat`) |
| AD2 | `pwrite64` without moving offset | **Done** — Ring 3 writes `xy`, `pwrite64` `z` at 1; `lseek` CUR stays 2; read is `xz` (`GATE_AD2 pwrite64`) |
| AD3 | `getgid` from Ring 3 | **Done** — spawned task `getgid` is 1000 (`GATE_AD3 getgid`) |
| AD4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `lookup_dcookie` returns `-ENOSYS` (`GATE_AD4 enosys`) |

### Gate AE — writev, ftruncate, geteuid, honest syscalls (after AD)

| ID | Task | Done when |
|----|------|-----------|
| AE1 | `writev` scatter-gather | **Done** — two iovecs write `x`+`y`; a read returns `xy` (`GATE_AE1 writev`) |
| AE2 | `ftruncate` shrinks an fd | **Done** — Ring 3 writes 8 bytes, `ftruncate`s to 1, `statx` `stx_size == 1` (`GATE_AE2 ftruncate`) |
| AE3 | `geteuid` from Ring 3 | **Done** — boot task `geteuid` is 0 (`GATE_AE3 geteuid`) |
| AE4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `memfd_secret` returns `-ENOSYS` (`GATE_AE4 enosys`) |

### Gate AF — readv, lseek, getegid, honest syscalls (after AE)

| ID | Task | Done when |
|----|------|-----------|
| AF1 | `readv` scatter-gather | **Done** — two iovecs read `x`+`y` from `xy` (`GATE_AF1 readv`) |
| AF2 | `lseek` then read | **Done** — Ring 3 writes `xy`, `lseek`s to 1, `read`s `y` (`GATE_AF2 lseek`) |
| AF3 | `getegid` from Ring 3 | **Done** — spawned task `getegid` is 1000 (`GATE_AF3 getegid`) |
| AF4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `uselib` returns `-ENOSYS` (`GATE_AF4 enosys`) |

### Gate AG — fsync, fdatasync, getppid, honest syscalls (after AF)

| ID | Task | Done when |
|----|------|-----------|
| AG1 | `fsync` on a VFS fd | **Done** — written fd succeeds; bad fd is EBADF (`GATE_AG1 fsync`) |
| AG2 | `fdatasync` after write | **Done** — Ring 3 writes a byte then `fdatasync` (`GATE_AG2 fdatasync`) |
| AG3 | `getppid` from Ring 3 | **Done** — spawned task `getppid` is non-zero (`GATE_AG3 getppid`) |
| AG4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `pkey_alloc` returns `-ENOSYS` (`GATE_AG4 enosys`) |

### Gate AH — syncfs, sync, getpgid, honest syscalls (after AG)

| ID | Task | Done when |
|----|------|-----------|
| AH1 | `syncfs` on a VFS fd | **Done** — written fd succeeds; bad fd is EBADF (`GATE_AH1 syncfs`) |
| AH2 | `sync` after write | **Done** — Ring 3 writes a byte then `sync` (`GATE_AH2 sync`) |
| AH3 | `getpgid` from Ring 3 | **Done** — spawned task `getpgid(0)` is non-zero (`GATE_AH3 getpgid`) |
| AH4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `pkey_mprotect` returns `-ENOSYS` (`GATE_AH4 enosys`) |

### Gate AI — fchmod, fchown, getsid, honest syscalls (after AH)

| ID | Task | Done when |
|----|------|-----------|
| AI1 | `fchmod` on a VFS fd | **Done** — mode 0400 is set; bad fd is EBADF (`GATE_AI1 fchmod`) |
| AI2 | `fchown` then `statx` uid | **Done** — Ring 3 `fchown` reports `stx_uid == 1000` (`GATE_AI2 fchown`) |
| AI3 | `getsid` from Ring 3 | **Done** — spawned task `getsid(0)` is non-zero (`GATE_AI3 getsid`) |
| AI4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `pkey_free` returns `-ENOSYS` (`GATE_AI4 enosys`) |

### Gate AJ — fstatfs, statfs, setuid, honest syscalls (after AI)

| ID | Task | Done when |
|----|------|-----------|
| AJ1 | `fstatfs` on a VFS fd | **Done** — `f_bsize == 4096`; bad fd is EBADF (`GATE_AJ1 fstatfs`) |
| AJ2 | `statfs` reports block size | **Done** — Ring 3 `statfs("/tmp")` `f_bsize == 4096` (`GATE_AJ2 statfs`) |
| AJ3 | `setuid` then `getuid` | **Done** — Ring 3 `setuid(1000)` then `getuid` is 1000 (`GATE_AJ3 setuid`) |
| AJ4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `process_mrelease` returns `-ENOSYS` (`GATE_AJ4 enosys`) |

### Gate AK — fsetxattr, getrlimit, setgid, honest syscalls (after AJ)

| ID | Task | Done when |
|----|------|-----------|
| AK1 | `fsetxattr`/`fgetxattr` on a VFS fd | **Done** — `user.knox` round-trips; bad fd is EBADF (`GATE_AK1 fsetxattr`) |
| AK2 | `getrlimit` reports RLIMIT_NOFILE | **Done** — Ring 3 `rlim_cur == 1024` (`GATE_AK2 getrlimit`) |
| AK3 | `setgid` then `getgid` | **Done** — Ring 3 `setgid(2000)` then `getgid` is 2000 (`GATE_AK3 setgid`) |
| AK4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `set_mempolicy` returns `-ENOSYS` (`GATE_AK4 enosys`) |

### Gate AL — flistxattr, setrlimit, setresuid, honest syscalls (after AK)

| ID | Task | Done when |
|----|------|-----------|
| AL1 | `flistxattr`/`fremovexattr` on a VFS fd | **Done** — list contains `user.knox`; remove clears it; bad fd is EBADF (`GATE_AL1 flistxattr`) |
| AL2 | `setrlimit` then `getrlimit` | **Done** — Ring 3 `rlim_cur == 512` (`GATE_AL2 setrlimit`) |
| AL3 | `setresuid` then `geteuid` | **Done** — Ring 3 `setresuid(-1, 1000, -1)` then `geteuid` is 1000 (`GATE_AL3 setresuid`) |
| AL4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `get_mempolicy` returns `-ENOSYS` (`GATE_AL4 enosys`) |

### Gate AM — listxattr, prlimit64, setresgid, honest syscalls (after AL)

| ID | Task | Done when |
|----|------|-----------|
| AM1 | `listxattr`/`removexattr` on a VFS path | **Done** — list contains `user.knox`; remove clears it; missing path is ENOENT (`GATE_AM1 listxattr`) |
| AM2 | `prlimit64` then `getrlimit` | **Done** — Ring 3 `rlim_cur == 256` (`GATE_AM2 prlimit64`) |
| AM3 | `setresgid` then `getegid` | **Done** — Ring 3 `setresgid(-1, 2000, -1)` then `getegid` is 2000 (`GATE_AM3 setresgid`) |
| AM4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `mbind` returns `-ENOSYS` (`GATE_AM4 enosys`) |

### Gate AN — faccessat, setreuid, getgroups, honest syscalls (after AM)

| ID | Task | Done when |
|----|------|-----------|
| AN1 | `faccessat` via dirfd | **Done** — relative `F_OK` succeeds; missing child is ENOENT; bad dirfd is EBADF (`GATE_AN1 faccessat`) |
| AN2 | `setreuid` then `geteuid` | **Done** — Ring 3 `setreuid(-1, 1000)` then `geteuid` is 1000 (`GATE_AN2 setreuid`) |
| AN3 | `setgroups` then `getgroups` | **Done** — Ring 3 `getgroups` returns gid 2000 (`GATE_AN3 getgroups`) |
| AN4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `sched_setattr` returns `-ENOSYS` (`GATE_AN4 enosys`) |

### Gate AO — mkdirat, setregid, getresuid, honest syscalls (after AN)

| ID | Task | Done when |
|----|------|-----------|
| AO1 | `mkdirat` via dirfd | **Done** — relative create succeeds; missing parent is ENOENT; bad dirfd is EBADF (`GATE_AO1 mkdirat`) |
| AO2 | `setregid` then `getegid` | **Done** — Ring 3 `setregid(-1, 2000)` then `getegid` is 2000 (`GATE_AO2 setregid`) |
| AO3 | `getresuid` after `setuid` | **Done** — Ring 3 `getresuid` reports ruid/euid 1000 (`GATE_AO3 getresuid`) |
| AO4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `futex_waitv` returns `-ENOSYS` (`GATE_AO4 enosys`) |

### Gate AP — unlinkat, getresgid, setpgid, honest syscalls (after AO)

| ID | Task | Done when |
|----|------|-----------|
| AP1 | `unlinkat` via dirfd | **Done** — relative unlink succeeds; missing child is ENOENT; bad dirfd is EBADF (`GATE_AP1 unlinkat`) |
| AP2 | `getresgid` after `setgid` | **Done** — Ring 3 `getresgid` reports rgid/egid 2000 (`GATE_AP2 getresgid`) |
| AP3 | `setpgid` then `getpgrp` | **Done** — Ring 3 `setpgid(0, 0)` then `getpgrp` equals `getpid` (`GATE_AP3 setpgid`) |
| AP4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `open_by_handle_at` returns `-ENOSYS` (`GATE_AP4 enosys`) |

### Gate AQ — renameat, setsid, setpriority, honest syscalls (after AP)

| ID | Task | Done when |
|----|------|-----------|
| AQ1 | `renameat` via dirfd | **Done** — relative rename succeeds; missing source is ENOENT; bad dirfd is EBADF (`GATE_AQ1 renameat`) |
| AQ2 | `setsid` then `getsid` | **Done** — Ring 3 `setsid` then `getsid(0)` equals `getpid` (`GATE_AQ2 setsid`) |
| AQ3 | `setpriority` then `getpriority` | **Done** — Ring 3 `setpriority(0, 0, 5)` then `getpriority` is 15 (`GATE_AQ3 setpriority`) |
| AQ4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `name_to_handle_at` returns `-ENOSYS` (`GATE_AQ4 enosys`) |

### Gate AR — linkat, getrusage, clock_gettime, honest syscalls (after AQ)

| ID | Task | Done when |
|----|------|-----------|
| AR1 | `linkat` via dirfd | **Done** — relative hard link succeeds; missing source is ENOENT; bad dirfd is EBADF (`GATE_AR1 linkat`) |
| AR2 | `getrusage` reports rss | **Done** — Ring 3 `ru_maxrss == 4096` (`GATE_AR2 getrusage`) |
| AR3 | `clock_gettime` monotonic | **Done** — Ring 3 `CLOCK_MONOTONIC` `tv_nsec < 1e9` (`GATE_AR3 clock_gettime`) |
| AR4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `map_shadow_stack` returns `-ENOSYS` (`GATE_AR4 enosys`) |

### Gate AS — symlinkat, clock_getres, times, honest syscalls (after AR)

| ID | Task | Done when |
|----|------|-----------|
| AS1 | `symlinkat` via dirfd | **Done** — relative symlink succeeds; missing parent is ENOENT; bad dirfd is EBADF (`GATE_AS1 symlinkat`) |
| AS2 | `clock_getres` is 1ns | **Done** — Ring 3 `CLOCK_MONOTONIC` `{0, 1}` (`GATE_AS2 clock_getres`) |
| AS3 | `times` returns ticks | **Done** — Ring 3 `times` is non-zero (`GATE_AS3 times`) |
| AS4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `ustat` returns `-ENOSYS` (`GATE_AS4 enosys`) |

### Gate AT — readlinkat, gettimeofday, sysinfo, honest syscalls (after AS)

| ID | Task | Done when |
|----|------|-----------|
| AT1 | `readlinkat` via dirfd | **Done** — relative read returns the target; missing child is ENOENT; bad dirfd is EBADF (`GATE_AT1 readlinkat`) |
| AT2 | `gettimeofday` usec in range | **Done** — Ring 3 `tv_usec < 1e6` (`GATE_AT2 gettimeofday`) |
| AT3 | `sysinfo` reports RAM | **Done** — Ring 3 `totalram` is non-zero (`GATE_AT3 sysinfo`) |
| AT4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `migrate_pages` returns `-ENOSYS` (`GATE_AT4 enosys`) |

### Gate AU — mknodat, sched_yield, alarm, honest syscalls (after AT)

| ID | Task | Done when |
|----|------|-----------|
| AU1 | `mknodat` via dirfd | **Done** — relative create succeeds; missing parent is ENOENT; bad dirfd is EBADF (`GATE_AU1 mknodat`) |
| AU2 | `sched_yield` from Ring 3 | **Done** — Ring 3 returns 0 (`GATE_AU2 sched_yield`) |
| AU3 | `alarm(0)` is non-negative | **Done** — Ring 3 `alarm(0)` does not fail (`GATE_AU3 alarm`) |
| AU4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `swapoff` returns `-ENOSYS` (`GATE_AU4 enosys`) |

### Gate AV — fchmodat, getpid, gettid, honest syscalls (after AU)

| ID | Task | Done when |
|----|------|-----------|
| AV1 | `fchmodat` via dirfd | **Done** — relative chmod 0400 succeeds; missing child is ENOENT; bad dirfd is EBADF (`GATE_AV1 fchmodat`) |
| AV2 | `getpid` from Ring 3 | **Done** — spawned task `getpid` is non-zero (`GATE_AV2 getpid`) |
| AV3 | `gettid` from Ring 3 | **Done** — spawned task `gettid` is non-zero (`GATE_AV3 gettid`) |
| AV4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `move_pages` returns `-ENOSYS` (`GATE_AV4 enosys`) |

### Gate AW — fchownat, sched_getscheduler, sched_getparam, honest syscalls (after AV)

| ID | Task | Done when |
|----|------|-----------|
| AW1 | `fchownat` via dirfd | **Done** — relative chown uid 1000 succeeds; missing child is ENOENT; bad dirfd is EBADF (`GATE_AW1 fchownat`) |
| AW2 | `sched_getscheduler` from Ring 3 | **Done** — Ring 3 `sched_getscheduler(0)` is 0 (`GATE_AW2 getsched`) |
| AW3 | `sched_getparam` from Ring 3 | **Done** — Ring 3 `sched_getparam(0)` returns 0 (`GATE_AW3 getparam`) |
| AW4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `remap_file_pages` returns `-ENOSYS` (`GATE_AW4 enosys`) |

### Gate AX — newfstatat, priority range, honest syscalls (after AW)

| ID | Task | Done when |
|----|------|-----------|
| AX1 | `newfstatat` via dirfd | **Done** — relative `st_size == 1`; missing child is ENOENT; bad dirfd is EBADF (`GATE_AX1 newfstatat`) |
| AX2 | `sched_get_priority_max` from Ring 3 | **Done** — Ring 3 returns 99 (`GATE_AX2 prio_max`) |
| AX3 | `sched_get_priority_min` from Ring 3 | **Done** — Ring 3 returns 0 (`GATE_AX3 prio_min`) |
| AX4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `sched_getattr` returns `-ENOSYS` (`GATE_AX4 enosys`) |

### Gate AY — openat, rr interval, getcpu, honest syscalls (after AX)

| ID | Task | Done when |
|----|------|-----------|
| AY1 | `openat` via dirfd | **Done** — relative open succeeds; missing child is ENOENT; bad dirfd is EBADF (`GATE_AY1 openat`) |
| AY2 | `sched_rr_get_interval` from Ring 3 | **Done** — Ring 3 writes 100ms nsec (`GATE_AY2 rr_interval`) |
| AY3 | `getcpu` from Ring 3 | **Done** — Ring 3 returns 0 (`GATE_AY3 getcpu`) |
| AY4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `io_destroy` returns `-ENOSYS` (`GATE_AY4 enosys`) |

### Gate AZ — utimensat dirfd, robust list, honest syscalls (after AY)

| ID | Task | Done when |
|----|------|-----------|
| AZ1 | `utimensat` via dirfd | **Done** — relative mtime 42 succeeds; missing child is ENOENT; bad dirfd is EBADF (`GATE_AZ1 utimensat`) |
| AZ2 | `set_robust_list` from Ring 3 | **Done** — Ring 3 returns 0 (`GATE_AZ2 set_robust`) |
| AZ3 | `get_robust_list` from Ring 3 | **Done** — Ring 3 returns 0 (`GATE_AZ3 get_robust`) |
| AZ4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `set_thread_area` returns `-ENOSYS` (`GATE_AZ4 enosys`) |

### Gate BA — statx dirfd, personality, nanosleep, honest syscalls (after AZ)

| ID | Task | Done when |
|----|------|-----------|
| BA1 | `statx` via dirfd | **Done** — relative `stx_size == 1`; missing child is ENOENT; bad dirfd is EBADF (`GATE_BA1 statx`) |
| BA2 | `personality(-1)` from Ring 3 | **Done** — Ring 3 returns 0 (`GATE_BA2 personality`) |
| BA3 | `nanosleep` from Ring 3 | **Done** — Ring 3 `nanosleep({0,1})` returns 0 (`GATE_BA3 nanosleep`) |
| BA4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `io_cancel` returns `-ENOSYS` (`GATE_BA4 enosys`) |

### Gate BB — futimesat, capget, ioprio_get, honest syscalls (after BA)

| ID | Task | Done when |
|----|------|-----------|
| BB1 | `futimesat` via dirfd | **Done** — relative mtime 42 succeeds; missing child is ENOENT; bad dirfd is EBADF (`GATE_BB1 futimesat`) |
| BB2 | `capget` from Ring 3 | **Done** — Ring 3 returns 0 (`GATE_BB2 capget`) |
| BB3 | `ioprio_get` from Ring 3 | **Done** — Ring 3 default priority is 4 (`GATE_BB3 ioprio_get`) |
| BB4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `add_key` returns `-ENOSYS` (`GATE_BB4 enosys`) |

### Gate BC — renameat2, capset, ioprio_set, honest syscalls (after BB)

| ID | Task | Done when |
|----|------|-----------|
| BC1 | `renameat2` via dirfd | **Done** — relative rename succeeds; missing source is ENOENT; bad dirfd is EBADF (`GATE_BC1 renameat2`) |
| BC2 | `capset` from Ring 3 | **Done** — Ring 3 returns 0 (`GATE_BC2 capset`) |
| BC3 | `ioprio_set` from Ring 3 | **Done** — Ring 3 `ioprio_set(1, 0, 4)` returns 0 (`GATE_BC3 ioprio_set`) |
| BC4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `request_key` returns `-ENOSYS` (`GATE_BC4 enosys`) |

### Gate BD — faccessat2, clock_nanosleep, getitimer, honest syscalls (after BC)

| ID | Task | Done when |
|----|------|-----------|
| BD1 | `faccessat2` via dirfd | **Done** — relative access succeeds; missing child is ENOENT; bad dirfd is EBADF (`GATE_BD1 faccessat2`) |
| BD2 | `clock_nanosleep` from Ring 3 | **Done** — Ring 3 `clock_nanosleep(CLOCK_MONOTONIC, 0, {0,1})` returns 0 (`GATE_BD2 clock_nanosleep`) |
| BD3 | `getitimer` from Ring 3 | **Done** — Ring 3 `getitimer(ITIMER_REAL)` returns 0 (`GATE_BD3 getitimer`) |
| BD4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `io_submit` returns `-ENOSYS` (`GATE_BD4 enosys`) |

### Gate BE — execveat, setitimer, timer_gettime, honest syscalls (after BD)

| ID | Task | Done when |
|----|------|-----------|
| BE1 | `execveat` via dirfd | **Done** — relative exec succeeds; missing child is ENOENT; bad dirfd is EBADF (`GATE_BE1 execveat`) |
| BE2 | `setitimer` then `getitimer` | **Done** — Ring 3 `setitimer(ITIMER_REAL, 1s)` then `getitimer` reports `tv_sec == 1` (`GATE_BE2 setitimer`) |
| BE3 | `timer_gettime` remaining | **Done** — Ring 3 `timer_create`/`timer_settime` then `timer_gettime` reports remaining nsec (`GATE_BE3 timer_gettime`) |
| BE4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `io_getevents` returns `-ENOSYS` (`GATE_BE4 enosys`) |

### Gate BF — dup3, pipe2, pselect6, honest syscalls (after BE)

| ID | Task | Done when |
|----|------|-----------|
| BF1 | `dup3` with `O_CLOEXEC` | **Done** — Ring 3 writes a byte, `dup3`s onto fd 8 with `O_CLOEXEC`, `lseek`/`read`s it back; `F_GETFD` is `FD_CLOEXEC` (`GATE_BF1 dup3`) |
| BF2 | `pipe2` with `O_CLOEXEC` | **Done** — Ring 3 `pipe2(O_CLOEXEC)` write/read round-trips a byte; `F_GETFD` is `FD_CLOEXEC` (`GATE_BF2 pipe2`) |
| BF3 | `pselect6` on a pipe | **Done** — Ring 3 `pselect6` stays idle, then a write makes it return 1 (`GATE_BF3 pselect6`) |
| BF4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `io_pgetevents` returns `-ENOSYS` (`GATE_BF4 enosys`) |

### Gate BG — ppoll, accept4, epoll_pwait, honest syscalls (after BF)

| ID | Task | Done when |
|----|------|-----------|
| BG1 | `ppoll` on a pipe | **Done** — Ring 3 `ppoll` stays idle, then a write makes it return 1 (`GATE_BG1 ppoll`) |
| BG2 | `accept4` listen/connect | **Done** — Ring 3 AF_INET listen/connect then `accept4(SOCK_CLOEXEC)` returns a new fd (`GATE_BG2 accept4`) |
| BG3 | `epoll_pwait` on a pipe | **Done** — Ring 3 `epoll_pwait` returns 1 after a write (`GATE_BG3 epoll_pwait`) |
| BG4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `fsconfig` returns `-ENOSYS` (`GATE_BG4 enosys`) |

### Gate BH — select, getsockname, getpeername, honest syscalls (after BG)

| ID | Task | Done when |
|----|------|-----------|
| BH1 | `select` on a pipe | **Done** — Ring 3 `select` stays idle, then a write makes it return 1 (`GATE_BH1 select`) |
| BH2 | `getsockname` after bind | **Done** — Ring 3 AF_INET bind then `getsockname` reports family + port (`GATE_BH2 getsockname`) |
| BH3 | `getpeername` after connect | **Done** — Ring 3 AF_INET listen/connect then `getpeername` reports the peer port (`GATE_BH3 getpeername`) |
| BH4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `fsmount` returns `-ENOSYS` (`GATE_BH4 enosys`) |

### Gate BI — sendmsg, recvmsg, shutdown, honest syscalls (after BH)

| ID | Task | Done when |
|----|------|-----------|
| BI1 | `sendmsg` UDP byte | **Done** — Ring 3 `sendmsg` of `x` then `recvmsg` (`GATE_BI1 sendmsg`) |
| BI2 | `recvmsg` UDP byte | **Done** — Ring 3 `sendmsg`/`recvmsg` of `x` (`GATE_BI2 recvmsg`) |
| BI3 | `shutdown` on a socket | **Done** — Ring 3 `shutdown(SHUT_RDWR)` returns 0 (`GATE_BI3 shutdown`) |
| BI4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `open_tree` returns `-ENOSYS` (`GATE_BI4 enosys`) |

### Gate BJ — sendmmsg, recvmmsg, getsockopt, honest syscalls (after BI)

| ID | Task | Done when |
|----|------|-----------|
| BJ1 | `sendmmsg` UDP messages | **Done** — Ring 3 `sendmmsg` of `xy` then `recvmmsg` returns 1 (`GATE_BJ1 sendmmsg`) |
| BJ2 | `recvmmsg` UDP messages | **Done** — Ring 3 `sendmmsg`/`recvmmsg` of `xy` (`GATE_BJ2 recvmmsg`) |
| BJ3 | `getsockopt` SO_TYPE | **Done** — Ring 3 `SOCK_DGRAM` `SO_TYPE` is 2 (`GATE_BJ3 getsockopt`) |
| BJ4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `move_mount` returns `-ENOSYS` (`GATE_BJ4 enosys`) |

### Gate BK — setsockopt, TCP send/recv, honest syscalls (after BJ)

| ID | Task | Done when |
|----|------|-----------|
| BK1 | `setsockopt` SO_REUSEADDR | **Done** — Ring 3 `setsockopt(SOL_SOCKET, SO_REUSEADDR, 1)` then `getsockopt` is 1 (`GATE_BK1 setsockopt`) |
| BK2 | TCP `sendto` after accept | **Done** — Ring 3 listen/connect/accept then `sendto` of `x` (`GATE_BK2 tcp_send`) |
| BK3 | TCP `recvfrom` after accept | **Done** — Ring 3 listen/connect/accept then `recvfrom` of `x` (`GATE_BK3 tcp_recv`) |
| BK4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `fspick` returns `-ENOSYS` (`GATE_BK4 enosys`) |

### Gate BL — close_range, pidfd_open, clone3, honest syscalls (after BK)

| ID | Task | Done when |
|----|------|-----------|
| BL1 | `close_range` closes an fd | **Done** — Ring 3 writes a file, `close_range`s that fd, then `read` is EBADF (`GATE_BL1 close_range`) |
| BL2 | `pidfd_open` of self | **Done** — Ring 3 `pidfd_open(getpid())` returns fd > 2 (`GATE_BL2 pidfd_open`) |
| BL3 | Ring 3 `clone3` | **Done** — Ring 3 `clone3` with a zeroed `clone_args` forks; parent `wait4`s (`GATE_BL3 clone3`) |
| BL4 | More silent `Ok(0)` → ENOSYS | **Done** — Ring 3 `delete_module` returns `-ENOSYS` (`GATE_BL4 enosys`) |
