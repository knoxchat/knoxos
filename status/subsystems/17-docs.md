← [Status index](../README.md) · [Do this first](../next.md)

# 17. Documentation

**Grade: Wired (42%)**

| Artifact | Status |
|----------|--------|
| `status/` | Split status tree (updated 2026-10-13). Start at [next.md](../next.md). |
| `README.md` | Present — honesty paragraph, `./run.sh`, architecture sketch |
| `LICENSE` | MIT |
| `BUILDING.md` / `CONTRIBUTING.md` | Present |
| Architecture / syscall / driver / GUI guides | Missing |
| `cargo doc` published | Missing |
| Changelog | Missing |

### Missing
- [ ] Architecture / syscall / driver / GUI guides
- [ ] `cargo doc` published
- [ ] Changelog

### Perfect-OS next steps (cheap, high leverage)
1. MIT `LICENSE`.
2. `README.md`: what KnoxOS is, honesty paragraph, `./run.sh`, screenshots, link here.
3. Boot flow diagram: bootloader → `kernel_main` → compositor loop.
4. Syscall table: **behavior** (`Ok(0)` vs implemented vs ENOSYS), not just numbers.

---
