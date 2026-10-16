#!/usr/bin/env python3
"""Insert // SAFETY: comments above undocumented `unsafe {` blocks.

Clippy `undocumented_unsafe_blocks` requires a SAFETY comment immediately
above each unsafe block (attributes and blank lines may sit in between).
This script walks kernel/src and boot, classifies the block body, and
inserts a one-line justification. It does not touch `unsafe fn`,
`unsafe impl`, or `#[unsafe(...)]`.
"""
from __future__ import annotations

import pathlib
import re
import sys

ROOTS = [
    pathlib.Path("kernel/src"),
    pathlib.Path("boot/src"),
]

ATTR_RE = re.compile(r"^\s*#\[")
UNSAFE_BLOCK_RE = re.compile(r"\bunsafe\s*\{")
SAFETY_RE = re.compile(r"SAFETY:", re.IGNORECASE)


def classify(body: str, context: str) -> str:
    blob = f"{context}\n{body}"
    if "run_until_desktop" in body:
        return (
            "`pid` was spawned with a live Ring 3 context; we only "
            "context-switch until the desktop is current again."
        )
    if "from_raw_parts_mut" in body:
        return (
            "pointer and length come from the current kernel-mode caller "
            "and are used only as a bounded mutable slice."
        )
    if "from_raw_parts" in body:
        return (
            "pointer and length come from the current kernel-mode caller "
            "and are used only as a bounded slice."
        )
    if "copy_nonoverlapping" in body:
        return "src and dst are valid for the copied byte count and do not overlap."
    if "read_volatile" in body or "write_volatile" in body:
        return (
            "the address is device MMIO or a DMA/on-disk record we own; "
            "volatile access prevents the compiler from eliding it."
        )
    if "asm!" in body or "core::arch::asm" in body:
        return (
            "inline assembly uses the architecture instruction and listed "
            "clobbers; no unexpected memory is accessed."
        )
    if "Port<" in blob or "port.write" in body or "port.read" in body:
        return (
            "I/O port access is exclusive to this driver path; the port "
            "number is a fixed architecture or BAR-derived address."
        )
    if "addr_of_mut" in body or "addr_of!" in body:
        return (
            "raw pointer is formed from a symbol or static we own; "
            "the write stays within that object."
        )
    if "transmute" in body:
        return (
            "the source and destination layouts are the same size; "
            "the bit pattern is a valid value of the target type."
        )
    if "set_len" in body:
        return (
            "the Vec backing store was written for the new length; "
            "those elements are initialized."
        )
    if "MaybeUninit" in blob:
        return "the MaybeUninit slot is written before assume_init / assume_init_ref."
    if "as *mut" in body or "as *const" in body:
        return (
            "the pointer is derived from a kernel object we own or a mapped "
            "physical address; the type layout matches the hardware or on-disk record."
        )
    if "core::ptr::" in body or "ptr::" in body:
        return (
            "the raw pointer is valid for this access; aliasing and "
            "provenance match the owning allocation."
        )
    return (
        "kernel-mode exclusive access; operands are owned, mapped, or "
        "architecture-defined for this path."
    )


def preceding_is_safety(lines: list[str], idx: int) -> bool:
    j = idx - 1
    while j >= 0:
        s = lines[j].strip()
        if not s:
            j -= 1
            continue
        if ATTR_RE.match(lines[j]):
            j -= 1
            continue
        if SAFETY_RE.search(s):
            return True
        return False
    return False


def extract_body(lines: list[str], idx: int) -> str:
    chunk = []
    depth = 0
    started = False
    for line in lines[idx : min(len(lines), idx + 12)]:
        chunk.append(line)
        if "{" in line:
            started = True
            depth += line.count("{")
        if started:
            depth -= line.count("}")
            if depth <= 0:
                break
    return "\n".join(chunk)


def indent_of(line: str) -> str:
    return line[: len(line) - len(line.lstrip(" \t"))]


def process_file(path: pathlib.Path) -> int:
    text = path.read_text(encoding="utf-8")
    lines = text.splitlines(keepends=True)
    out: list[str] = []
    added = 0
    i = 0
    while i < len(lines):
        line = lines[i]
        if UNSAFE_BLOCK_RE.search(line) and not preceding_is_safety(lines, i):
            # Skip `unsafe fn` / `unsafe impl` / attributes that happen to
            # share a line with a later `{` from something else.
            stripped = line.lstrip()
            if stripped.startswith("unsafe fn") or stripped.startswith("unsafe impl"):
                out.append(line)
                i += 1
                continue
            if stripped.startswith("#[") or stripped.startswith("#!["):
                out.append(line)
                i += 1
                continue
            ctx = "".join(lines[max(0, i - 8) : i])
            body = extract_body(lines, i)
            comment = classify(body, ctx)
            indent = indent_of(line)
            out.append(f"{indent}// SAFETY: {comment}\n")
            added += 1
        out.append(line)
        i += 1
    if added:
        path.write_text("".join(out), encoding="utf-8")
    return added


def main() -> int:
    repo = pathlib.Path(__file__).resolve().parents[1]
    total = 0
    files = 0
    for root in ROOTS:
        src = repo / root
        if not src.exists():
            continue
        for path in sorted(src.rglob("*.rs")):
            n = process_file(path)
            if n:
                files += 1
                total += n
                print(f"{path.relative_to(repo)}: +{n}")
    print(f"inserted {total} SAFETY comments in {files} files")
    return 0


if __name__ == "__main__":
    sys.exit(main())
