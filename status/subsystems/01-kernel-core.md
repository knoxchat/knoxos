← [Status index](../README.md) · [Do this first](../next.md)

# 1. Kernel Core

**Grade: Wired (82%)** · `gdt.rs`, `interrupts.rs`, `smp.rs`, `apic_timer.rs`, `acpi_tables.rs`, `cmdline.rs`

### Live
- [x] GDT + TSS load on BSP (`gdt::init`)
- [x] IDT: breakpoint, #PF, #GP, #UD, #SS, #DF, NMI, MCE, timer, kbd, mouse, virtio, ATA, APIC timer
- [x] PIC 8259 init and IRQ unmask (timer, kbd, cascade, virtio, mouse, ATA)
- [x] LAPIC detection, APIC timer + PIT calibration
- [x] Serial UART 16550 TX (debug console)
- [x] Kernel heap 512 MiB + slab
- [x] Boot splash + long `kernel_main` init sequence
- [x] Panic handler
- [x] RTC CMOS read
- [x] Page-fault → `vmm::handle_page_fault` (when a process address space exists)
- [x] ACPI RSDP scan; MADT / FADT / HPET / MCFG parse from physical memory

### Wired but incomplete
- [x] **IOAPIC** — MADT address + interrupt-source overrides applied after ACPI parse (IRQ0→GSI 2 on QEMU); all IRQs still to BSP
- [x] **SMP** — real INIT/SIPI trampoline 16→32→64; AP loads per-CPU TSS + GS, STI + LAPIC timer, runs pinned Ring 3 (`GATE_I1 smp online`, `GATE_I3 ap ring3`); per-CPU CFS `current`; unpinned work on an idle AP (`GATE_I4 smp balance`); `balance_load` does not steal single-CPU-pinned tasks
- [x] **Per-CPU data** — GS-relative `CpuLocal` (`gs:[0]`/`gs:[8]` syscall scratch + `gs:[24]` cpu index + `gs:[32]` current PID); `CPU_DATA` mutex remains for stats
- [x] **Command line** — real `key=value` parser; QEMU fw_cfg cmdline when present, else a documented default
- [ ] **x86_64 / aarch64 / riscv64** — x86_64 boots; other arches compile with shims

### Stub / unused
- [x] **NMI** — IDT handler logs and returns
- [x] **MCE** — IDT handler logs and panics
- [ ] **ACPI AML / DSDT** — no interpreter; `acpi::shutdown` is QEMU port writes
- [ ] **UEFI runtime** — `get_time` hardcoded; `get_variable` → `None`
- [ ] **Kernel modules** — `modules.rs` / `kpm.rs` are name registries, not `.ko` loaders
- [ ] **IOMMU** — DMAR parse; `enable_translation` never called
- [ ] **Live patch** — writes a jmp over a symbol; no W^X, no activeness wait
- [ ] **Nested IRQ / TPR** — not used from ISRs

### Perfect-OS next steps
1. ~~Per-CPU TSS + GS base + exception stacks before SMP is useful.~~ **Done** (I1) — each CPU has TSS + IST stacks; GS points at `CpuLocal`.
2. ~~Wire MADT IOAPIC address and interrupt source overrides.~~ **Done** — `smp::apply_madt_ioapic` after ACPI parse.
3. ~~Add NMI and MCE handlers that log and recover or panic cleanly.~~ **Done** — NMI logs; MCE panics.
4. ~~Pass the real bootloader command line into `cmdline::parse`.~~ **Done** when fw_cfg has one; otherwise the documented default.
5. ~~AP runs a Ring 3 task.~~ **Done** (I3) — affinity-pinned `getcpu` ELF on CPU 1.

---
