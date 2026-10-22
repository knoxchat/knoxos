# userspace/apps

User-space **programs**: sources for `/sbin/init`, `/bin/sh`, hello, calculator, Files, terminal, paint, and other `execve` ELFs.

Instruction bytes are authored in each app's `src.py`. `tools/gen_apps.py` writes the committed `.elf` blobs. The kernel still embeds those blobs (`include_bytes!`) until a userspace toolchain installs `/bin` into the image.

**Does not belong here**

- In-kernel desktop apps, shells, and daemons (`kernel/src/compat/shell/`, D-Bus, PulseAudio, …) until they actually run as processes.
- Kernel init and ELF *embedding* (`kernel/src/init/`).
- Gate fixtures that are not programs (statx/fork/ENOSYS tests stay in `kernel/src/init/programs/`).

A directory named `calculator/` is not Ring 3 until QEMU prints the matching gate marker for a user process.
