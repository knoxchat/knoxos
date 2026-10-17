← [Status index](../README.md) · [Do this first](../next.md)

# 3. Process & Scheduling

**Grade: Wired (99%)** · `scheduler.rs`, `process.rs`, `context.rs`, `usermode.rs`, `signals.rs`, `user_task.rs`

Kernel threads can switch RIP. Gate B2 enters Ring 3 for a one-shot hello. Gate B3–B6 then run **scheduled** Ring 3 tasks with their own CR3: `execve`+`waitpid`, `fork`+child, SIGKILL/SIGSEGV/PTY SIGINT, and `/bin/sh` on a PTY.

### Wired (data plane)
- [x] Process table, PIDs, parent/child, reparent to init, zombie bookkeeping
- [x] CFS vruntime / weight table in software
- [x] Timer ISR sets `NEED_RESCHED`
- [x] `sys_fork` CoW-forks VMM and clones a context with `rax = 0`
- [x] `sys_execve` can map ELF into an address space + stack/auxv
- [x] `usermode.rs`: `wrmsr` STAR/LSTAR/SFMASK/EFER.SCE + `syscall`/`sysretq` stubs
- [x] Signal frame builder (`signals.rs`)
- [x] **Full kernel context switch** — GPRs, RIP, CS/SS, RFLAGS, FXSAVE/FXRSTOR; boot self-test switches two kernel threads
- [x] **Idle thread** PID 0 `STI; HLT`; executor `yield_to_idle()` when the desktop has no work
- [x] **`KERNEL_GS_BASE` / `GS_BASE`** programmed for `syscall` `swapgs`; Ring 3 `syscall` round-trip proven on hello
- [x] **`deliver_signals`** called from `deferred_schedule`; SIGKILL/default-terminate actually retire the task
- [x] **Gate B2 one-shot Ring 3** — static hello ELF mapped into current CR3, `iretq`, `sys_write` to serial, `sys_exit` returns to kernel
- [x] **Gate B3–B6 scheduled Ring 3** — `execve`+`waitpid`, `fork` child, SIGKILL / SIGSEGV / PTY SIGINT, `/bin/sh` on a PTY
- [x] **Gate B7 live `sigreturn`** — `rt_sigaction` handler runs in Ring 3, `ret`s into trampoline, `rt_sigreturn` resumes `pause`

### Stub (control plane)
- [x] **Scheduled Ring 3** — `user_task::spawn_elf` builds a CFS task with its own CR3; boot path `execve`s `/bin/hello`, then `/bin/sh` on a PTY
- [x] **Preemption of userspace** — naked APIC-timer stub saves GPRs + FXSAVE; spinner `rbx` magic survives (`GATE_I2 irq gprs`)
- [x] **`wait4` blocking** — parks a Ring 3 parent until the child exits
- [x] **SMP load balance / affinity** — `set_cpu_affinity` is honoured by CFS; per-CPU CFS `current`; APs run pinned (I3) and unpinned (I4) Ring 3; `sched_setaffinity`/`sched_getaffinity` syscalls write a real mask. BSP desktop stays in CPU 0's current slot.
- [x] **Threads / `clone(CLONE_VM)`** — child shares the parent's CR3 and runs; parent `wait4`s (`GATE_J1 thread clone`). `CLONE_THREAD` + `thread_join` live (K1). `CLONE_SETTLS` / `arch_prctl(ARCH_SET_FS)` restore `%fs` (L1).
- [x] **UTS namespaces** — `unshare(CLONE_NEWUTS)` isolates `sethostname` from the parent (`GATE_L2 uts ns`).
- [x] **PID namespaces** — `unshare(CLONE_NEWPID)` then fork: child `getpid` is 1; parent unchanged (`GATE_M1 pid ns`).
- [x] **Mount namespaces** — `unshare(CLONE_NEWNS)` bind mount does not leak to the parent (`GATE_N1 mount ns`).
- [x] **Network namespaces** — `unshare(CLONE_NEWNET)` child does not inherit `eth0`; child `veth0` stays private (`GATE_O1 net ns`).
- [x] **User namespaces** — `unshare(CLONE_NEWUSER)` maps the creator to uid 0; parent uid unchanged (`GATE_P1 user ns`).
- [x] **IPC namespaces** — `unshare(CLONE_NEWIPC)` isolates SysV shm keys from the parent (`GATE_Q1 ipc ns`).
- [x] **Cgroup namespaces** — `unshare(CLONE_NEWCGROUP)` isolates cgroup paths from the parent (`GATE_R1 cgroup ns`).
- [x] **Time namespaces** — `unshare(CLONE_NEWTIME)` isolates CLOCK_MONOTONIC offsets from the parent (`GATE_S1 time ns`).
- [x] **`setns`** — joiner attaches to an existing UTS ns and sees the owner's hostname (`GATE_T1 setns`).
- [x] **`chroot`** — per-process root jails path lookup; `/../etc` stays inside the jail (`GATE_U1 chroot`).
- [x] **`pivot_root`** — `new_root` becomes `/`; old root is visible at `put_old`; parent unchanged (`GATE_V1 pivot_root`).
- [x] **`chdir`/`getcwd`** — Ring 3 `chdir` then `getcwd` returns the new path (`GATE_Z3 chdir`). Ring 3 `fchdir` then `getcwd` (`GATE_AA2 fchdir`).
- [x] **`getppid`** — Ring 3 spawned task parent is non-zero (`GATE_AG3 getppid`).
- [x] **`getpgid`** — Ring 3 spawned task `getpgid(0)` is non-zero (`GATE_AH3 getpgid`).
- [x] **`getsid`** — Ring 3 spawned task `getsid(0)` is non-zero (`GATE_AI3 getsid`).
- [x] **`setuid`** — Ring 3 `setuid(1000)` then `getuid` is 1000 (`GATE_AJ3 setuid`).
- [x] **`getrlimit`** — Ring 3 `getrlimit(RLIMIT_NOFILE)` reports `rlim_cur == 1024` (`GATE_AK2 getrlimit`).
- [x] **`setgid`** — Ring 3 `setgid(2000)` then `getgid` is 2000 (`GATE_AK3 setgid`).
- [x] **`setrlimit`** — Ring 3 `setrlimit(RLIMIT_NOFILE, {512, 1024})` then `getrlimit` reports `rlim_cur == 512` (`GATE_AL2 setrlimit`).
- [x] **`setresuid`** — Ring 3 `setresuid(-1, 1000, -1)` then `geteuid` is 1000 (`GATE_AL3 setresuid`).
- [x] **`prlimit64`** — Ring 3 `prlimit64(0, RLIMIT_NOFILE, {256, 1024})` then `getrlimit` reports `rlim_cur == 256` (`GATE_AM2 prlimit64`).
- [x] **`setresgid`** — Ring 3 `setresgid(-1, 2000, -1)` then `getegid` is 2000 (`GATE_AM3 setresgid`).
- [x] **`setreuid`** — Ring 3 `setreuid(-1, 1000)` then `geteuid` is 1000 (`GATE_AN2 setreuid`).
- [x] **`setgroups`/`getgroups`** — Ring 3 `setgroups(1, [2000])` then `getgroups` returns 2000 (`GATE_AN3 getgroups`).
- [x] **`setregid`** — Ring 3 `setregid(-1, 2000)` then `getegid` is 2000 (`GATE_AO2 setregid`).
- [x] **`getresuid`** — Ring 3 `setuid(1000)` then `getresuid` reports ruid/euid 1000 (`GATE_AO3 getresuid`).
- [x] **`getresgid`** — Ring 3 `setgid(2000)` then `getresgid` reports rgid/egid 2000 (`GATE_AP2 getresgid`).
- [x] **`setpgid`/`getpgrp`** — Ring 3 `setpgid(0, 0)` then `getpgrp` equals `getpid` (`GATE_AP3 setpgid`).
- [x] **`setsid`** — Ring 3 `setsid` then `getsid(0)` equals `getpid` (`GATE_AQ2 setsid`).
- [x] **`setpriority`/`getpriority`** — Ring 3 `setpriority(PRIO_PROCESS, 0, 5)` then `getpriority` is 15 (`GATE_AQ3 setpriority`).
- [x] **`getrusage`** — Ring 3 `getrusage(RUSAGE_SELF)` reports `ru_maxrss == 4096` (`GATE_AR2 getrusage`).
- [x] **`clock_gettime`** — Ring 3 `CLOCK_MONOTONIC` `tv_nsec < 1e9` (`GATE_AR3 clock_gettime`).
- [x] **`gettimeofday`** — Ring 3 `tv_usec < 1e6` (`GATE_AT2 gettimeofday`).
- [x] **`sysinfo`** — Ring 3 `totalram` is non-zero (`GATE_AT3 sysinfo`).
- [x] **`sched_yield`** — Ring 3 returns 0 (`GATE_AU2 sched_yield`).
- [x] **`alarm`** — Ring 3 `alarm(0)` is non-negative (`GATE_AU3 alarm`).
- [x] **`getpid`** — Ring 3 spawned task `getpid` is non-zero (`GATE_AV2 getpid`).
- [x] **`gettid`** — Ring 3 spawned task `gettid` is non-zero (`GATE_AV3 gettid`).
- [x] **`setitimer`** — Ring 3 sets ITIMER_REAL 1s; `getitimer` reports `tv_sec == 1` (`GATE_BE2 setitimer`).
- [x] **`timer_gettime`** — Ring 3 `timer_create`/`timer_settime` then remaining nsec (`GATE_BE3 timer_gettime`).
- [x] **`clone3`** — Ring 3 zeroed `clone_args` forks; parent `wait4`s (`GATE_BL3 clone3`).
- [x] **futex wait/wake** — Ring 3 `FUTEX_WAIT` parks; `FUTEX_WAKE` resumes (`GATE_M3 futex`).
- [x] **sched_ext / eBPF** — verifier + interpreter `r0=42`; sched_ext pick (`GATE_EBPF exec`)

### Perfect-OS next steps (this is the critical path)
1. ~~Save/restore full `iretq` frame + `fxsave`/`fxrstor` in `context.rs`.~~ (kernel threads done)
2. ~~Per-CPU TSS RSP0 + `GS_BASE` / `KERNEL_GS_BASE`.~~ **Done** (I1) — each CPU has a TSS; GS points at `CpuLocal`.
3. ~~First userspace: map a static `hello` ELF, `iretq`, `sys_write` to serial, `sys_exit`.~~ (Gate B2)
4. Then `fork` + `execve` + `waitpid` + SIGCHLD + Ctrl+C via PTY. **B3–B7 wired on the boot path.** Isolated GUI clients (Gate F) have a first SHM client; the in-kernel terminal remains.

---
