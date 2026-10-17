← [Status index](../README.md) · [Do this first](../next.md)

# 2. Memory Management

**Grade: Wired (78%)** · `memory.rs`, `allocator.rs`, `vmm.rs`, `mmap.rs`, `slab.rs`, `page_cache.rs`

The real MMU work is in **`vmm.rs`**, not a buddy allocator.

### Live / Wired
- [x] Bootloader mmap bump-pointer frame allocator (no free)
- [x] Linked-list kernel heap (eager-mapped)
- [x] Slab for ≤4 KiB objects (backed by heap, not physical buddy)
- [x] 4-level page-table walk (`map_page_in_table`)
- [x] Demand paging: `#PF` → allocate + map + zero (`handle_demand_fault`)
- [x] CoW: `AddressSpace::fork` marks PTEs read-only; write fault copies (**Gate H1** / A4)
- [x] Anonymous `mmap` (lazy unless `MAP_POPULATE`)
- [x] File-backed `mmap`: one-page fault-in from page cache / VFS; siblings stay unmapped (**Gate H2**)
- [x] User ASLR offsets (xorshift; kernel image not randomized)
- [x] **Buddy allocator** on the VMM frame pool (orders 0–10, split/merge; 32 MiB prefill plus leftover usable RAM)
- [x] **OOM on alloc** — empty buddy pool shrinks the page cache then `trigger_oom` (**Gate H4**)
- [x] **Kernel stack guards** — `alloc_guarded_stack` used after VMM is ready; guard page is unmapped (**Gate H4**)

### Unused / stub
- [x] **Buddy over leftover RAM** — remaining usable bootloader frames are ingested into the buddy (`GATE_J2 buddy ram`)
- [x] **File-backed mmap** — lazy VMA; `#PF` fills one 4 KiB page from the page cache / VFS (H2)
- [x] **Page reclamation / LRU eviction** — `page_cache::shrink` drops oldest clean pages and keeps dirty (`GATE_J3 lru reclaim`); anonymous pages can swap (K2)
- [x] **Swap** — ram-backed slots store page bytes; reclaim calls `page_out`; `#PF` swap-in restores (`GATE_K2 swap io`)
- [x] **OOM killer** — scoring + `trigger_oom` from failed `allocate_physical_frame` (H4)
- [ ] **NUMA** — SRAT parse; `allocate_node` returns an id, not memory
- [ ] **KSM** — hashes phys addr as a virt pointer; merge is a counter
- [x] **THP** — buddy order-9 2 MiB allocation (`GATE_THP hugepage`)
- [x] **Kernel stack guards** — `stack_guard::alloc_guarded_stack` on the live path (H4)
- [x] **KASLR** for loadable kernel mappings (`GATE_KASLR slide`)
- [x] **Memory cgroups** enforcement — mmap/brk `try_charge`; hard limit is ENOMEM (`GATE_CGROUP memory`, `GATE_CGROUP2 memory`)

### Perfect-OS next steps
1. ~~Physical buddy (or equivalent) with free, orders, and a real page inventory.~~ **Done** (J2) for leftover usable RAM after the heap; reserved/firmware holes stay out of the pool.
2. ~~Call OOM from failed alloc; reclaim before OOM.~~ **Done** (H4) — shrink then `trigger_oom`.
3. ~~File-backed VMAs that fault from the block layer through the page cache.~~ **Done** (H2).
4. ~~Guard pages on every kernel stack; W^X on all user maps.~~ Guarded stacks after VMM init (H4); W^X was E1. LRU reclaim of clean file pages (J3). Swap-in on `#PF` (K2).

---
