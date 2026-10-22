#!/usr/bin/env python3
"""Generate committed ELF blobs for userspace/apps.

Instruction bytes are authored in each app's src.py. The kernel embeds the
blobs with include_bytes!; it does not assemble them.
"""

from __future__ import annotations

import importlib.util
import sys
from pathlib import Path

TOOLS = Path(__file__).resolve().parent
APPS = TOOLS.parent
sys.path.insert(0, str(TOOLS))
sys.path.insert(0, str(APPS / "hello"))
sys.path.insert(0, str(APPS / "init"))
sys.path.insert(0, str(APPS / "sh"))
sys.path.insert(0, str(APPS / "calculator"))
sys.path.insert(0, str(APPS / "files"))
sys.path.insert(0, str(APPS / "paint"))
sys.path.insert(0, str(APPS / "term"))
sys.path.insert(0, str(APPS / "shm-clients"))


def _load(app: str, mod_name: str):
    path = APPS / app / "src.py"
    spec = importlib.util.spec_from_file_location(mod_name, path)
    assert spec and spec.loader
    mod = importlib.util.module_from_spec(spec)
    sys.modules[mod_name] = mod
    spec.loader.exec_module(mod)
    return mod


def write_elf(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    print(f"wrote {path.relative_to(APPS.parent.parent)} ({len(data)} bytes)")


def main() -> None:
    hello = _load("hello", "hello_src")
    init = _load("init", "init_src")
    sh = _load("sh", "sh_src")
    calc = _load("calculator", "calculator_src")
    files = _load("files", "files_src")
    paint = _load("paint", "paint_src")
    term = _load("term", "term_src")
    shm = _load("shm-clients", "shm_clients_src")

    blobs = {
        APPS / "hello" / "hello.elf": hello.elf(),
        APPS / "init" / "init.elf": init.elf(),
        APPS / "sh" / "sh.elf": sh.elf(),
        APPS / "calculator" / "calculator.elf": calc.elf(),
        APPS / "files" / "files.elf": files.elf(),
        APPS / "paint" / "paint.elf": paint.elf(),
        APPS / "term" / "term.elf": term.elf(),
    }
    for name, data in shm.all_elves().items():
        blobs[APPS / "shm-clients" / name] = data

    hello_elf = blobs[APPS / "hello" / "hello.elf"]
    if hello_elf[:4] != b"\x7fELF":
        raise SystemExit("hello.elf is not ELF")
    if b"hello from userspace\n" not in hello_elf:
        raise SystemExit("hello.elf missing marker string")
    if len(hello_elf) != 0x2000:
        raise SystemExit(f"hello.elf size {len(hello_elf):#x} want 0x2000")

    for path, data in blobs.items():
        if data[:4] != b"\x7fELF":
            raise SystemExit(f"{path.name} is not ELF")
        if len(data) != 0x2000:
            raise SystemExit(f"{path.name} size {len(data):#x} want 0x2000")
        write_elf(path, data)


if __name__ == "__main__":
    main()
