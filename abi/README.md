# knox-abi

Syscall **contract** (`SyscallNumber`, `SyscallError`, `stat`/`statx`, `sockaddr_*`). `no_std`, no kernel dependencies.

Handlers stay in `kernel/src/syscall/`. VFS inodes stay in `kernel/src/fs/`. Userspace should compile against this crate rather than duplicating Linux x86_64 numbers and operand layouts.

See [docs/syscalls.md](../docs/syscalls.md).
