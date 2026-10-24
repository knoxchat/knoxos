← [Status index](../README.md) · [Do this first](../next.md)

# 17. Documentation

**Grade: Wired (70%)**

| Artifact | Status |
|----------|--------|
| `status/` | Split status tree (updated 2026-10-16). Start at [next.md](../next.md). |
| `README.md` | Present — honesty paragraph, `./run.sh`, architecture sketch |
| `LICENSE` | MIT |
| `BUILDING.md` / `CONTRIBUTING.md` | Present |
| Architecture / syscall / driver / GUI guides | Present — `docs/` |
| `cargo doc` published | CI rustdoc artifact (`.github/workflows/ci.yml` `rustdoc` job) |
| Changelog | Present — `CHANGELOG.md` |

### Missing
- [x] Architecture / syscall / driver / GUI guides (`docs/architecture.md`, `docs/syscalls.md`, `docs/drivers.md`, `docs/gui.md`)
- [x] `cargo doc` published (CI rustdoc artifact)
- [x] Changelog

### Perfect-OS next steps (cheap, high leverage)
1. MIT `LICENSE`.
2. `README.md`: what KnoxOS is, honesty paragraph, `./run.sh`, screenshots, link here.
3. Boot flow diagram: bootloader → `kernel_main` → compositor loop.
4. Syscall table: **behavior** (`Ok(0)` vs implemented vs ENOSYS), not just numbers.
5. Browser backlog lives in [19-browser.md](19-browser.md); `docs/gui.md` records Chromium/Vivaldi SHM (`GATE_CHR paint` / `GATE_VIV ui`).

---
