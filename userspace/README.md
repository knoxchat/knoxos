# userspace

Ring 3 lives here: **programs** in `apps/` and **system libs** in `libs/`.

Privilege is runtime, not path. A file under `userspace/` is user space only when it `execve`s as an ELF. Code that still runs in the kernel does **not** belong here — park it in `kernel/src/compat/` until a real Ring 3 binary replaces it.

Ring 3 is proven by QEMU serial markers (`hello from userspace`, `GATE_F6`, `GATE_K3`, …), not by folder names.
