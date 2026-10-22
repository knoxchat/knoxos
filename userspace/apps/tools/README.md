# userspace/apps/tools

Host scripts that assemble Ring 3 ELF blobs committed next to each app.

- `elfpack.py` — Gate B2 static ELF64 packer (`build_static_user_elf`)
- `gen_apps.py` — write `hello.elf`, `/sbin/init`, `/bin/sh`, calculator, Files, term, paint, and shm-clients
- `gen_gate_elfs.py` — print Rust byte arrays for remaining kernel-hosted gate fixtures (BH–BK)

The kernel still `include_bytes!` the blobs. Re-run `python3 userspace/apps/tools/gen_apps.py` after changing an app `src.py`.
