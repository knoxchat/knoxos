# Contributing to KnoxOS

Read [status.md](status.md) first. A module that compiles is not a feature. Only **live path** work (what runs after `./run.sh`) counts.

## What to work on

Follow the gate sequence in `status.md` (A → B → C → D → E → F → H → I → G). Do not add KVM, containers, Wi-Fi, TLS 1.3, GPU compute, or AI inference until Gates A–E are done — shipping those first increases the distance to a real OS.

1. Wire an existing stub into the live path, **or**
2. Delete / feature-gate Unused modules, **or**
3. Add a test that can fail (serial marker + `tests/run_integration.sh`)

## Honesty rules

- Tick a `status.md` checkbox only when the **Done when** criterion is met on QEMU (or real hardware).
- Do not add `assert!(true)` tests.
- Do not count syscall **numbers** as compatibility. `Ok(0)` is not an implementation.
- Do not add another `pub mod` that `serial_println`s and returns `Ok(())`.

## Tests

```bash
./tests/run_integration.sh   # QEMU serial markers
make ci-all                  # fmt, clippy, size
```

New live-path behavior needs a serial marker (`GATE_…`) and a `check_marker` line in `tests/run_integration.sh`.

## License

By contributing you agree the work is licensed under the [MIT License](LICENSE).
