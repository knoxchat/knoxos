#!/usr/bin/env python3
"""Pack Gate-B2-style static ELF64 images used by Ring 3 apps.

Matches `kernel/src/init/elf.rs` `build_static_user_elf` byte-for-byte so the
kernel can `include_bytes!` the blobs without a userspace toolchain.
"""

from __future__ import annotations

ENTRY = 0x0000_0080_0000_1000
PHOFF = 0x40
CODE_OFFSET = 0x1000
CODE_VADDR = 0x0000_0080_0000_1000
CODE_SIZE = 0x1000


def build_static_user_elf(code: bytes) -> bytes:
    elf = bytearray()
    elf.extend([0x7F, ord("E"), ord("L"), ord("F"), 2, 1, 1, 0])
    elf.extend(bytes(8))
    elf.extend((2).to_bytes(2, "little"))  # ET_EXEC
    elf.extend((62).to_bytes(2, "little"))  # EM_X86_64
    elf.extend((1).to_bytes(4, "little"))
    elf.extend(ENTRY.to_bytes(8, "little"))
    elf.extend(PHOFF.to_bytes(8, "little"))
    elf.extend((0).to_bytes(8, "little"))
    elf.extend((0).to_bytes(4, "little"))
    elf.extend((64).to_bytes(2, "little"))
    elf.extend((56).to_bytes(2, "little"))
    elf.extend((1).to_bytes(2, "little"))
    elf.extend((0).to_bytes(2, "little"))
    elf.extend((0).to_bytes(2, "little"))
    elf.extend((0).to_bytes(2, "little"))
    elf.extend((1).to_bytes(4, "little"))  # PT_LOAD
    elf.extend((5).to_bytes(4, "little"))  # PF_R | PF_X
    elf.extend(CODE_OFFSET.to_bytes(8, "little"))
    elf.extend(CODE_VADDR.to_bytes(8, "little"))
    elf.extend(CODE_VADDR.to_bytes(8, "little"))
    elf.extend(CODE_SIZE.to_bytes(8, "little"))
    elf.extend(CODE_SIZE.to_bytes(8, "little"))
    elf.extend((0x1000).to_bytes(8, "little"))
    if len(elf) < CODE_OFFSET:
        elf.extend(bytes(CODE_OFFSET - len(elf)))
    elf.extend(code)
    end = CODE_OFFSET + CODE_SIZE
    if len(elf) < end:
        elf.extend(bytes(end - len(elf)))
    return bytes(elf)


class Reloc:
    """RIP-relative assembler used by PTY / Wayland clients."""

    def __init__(self) -> None:
        self.code = bytearray()
        self.rels: list[tuple[int, str]] = []
        self.labels: dict[str, int] = {}

    def emit(self, *bs: int | bytes) -> None:
        for b in bs:
            if isinstance(b, bytes):
                self.code.extend(b)
            else:
                self.code.append(b)

    def emit_u32(self, n: int) -> None:
        self.code.extend((n & 0xFFFFFFFF).to_bytes(4, "little"))

    def label(self, name: str) -> None:
        self.labels[name] = len(self.code)

    def _rel32(self, name: str) -> None:
        self.rels.append((len(self.code), name))
        self.emit(0, 0, 0, 0)

    def js(self, name: str) -> None:
        self.emit(0x0F, 0x88)
        self._rel32(name)

    def jne(self, name: str) -> None:
        self.emit(0x0F, 0x85)
        self._rel32(name)

    def jle(self, name: str) -> None:
        self.emit(0x0F, 0x8E)
        self._rel32(name)

    def jmp(self, name: str) -> None:
        self.emit(0xE9)
        self._rel32(name)

    def lea_rdi(self, name: str) -> None:
        self.emit(0x48, 0x8D, 0x3D)
        self._rel32(name)

    def lea_rsi(self, name: str) -> None:
        self.emit(0x48, 0x8D, 0x35)
        self._rel32(name)

    def finish(self) -> bytes:
        out = bytearray(self.code)
        for off, name in self.rels:
            target = self.labels[name]
            disp = target - (off + 4)
            out[off : off + 4] = disp.to_bytes(4, "little", signed=True)
        return bytes(out)


def unix_listen_server_elf(path: bytes, marker: bytes) -> bytes:
    """Ring 3 bind+listen+pause on an AF_UNIX path."""
    if not path.endswith(b"\0"):
        path = path + b"\0"
    a = Reloc()
    # socket(AF_UNIX, SOCK_STREAM, 0)
    a.emit(0x48, 0xC7, 0xC0, 0x29, 0x00, 0x00, 0x00)
    a.emit(0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00)
    a.emit(0x48, 0xC7, 0xC6, 0x01, 0x00, 0x00, 0x00)
    a.emit(0x48, 0x31, 0xD2)
    a.emit(0x0F, 0x05)
    a.emit(0x48, 0x85, 0xC0)
    a.js("fail")
    a.emit(0x49, 0x89, 0xC4)

    a.emit(0x48, 0x81, 0xEC, 0x80, 0x00, 0x00, 0x00)
    a.emit(0x48, 0x31, 0xC0)
    a.emit(0x48, 0x89, 0xE7)
    a.emit(0x48, 0xC7, 0xC1, 0x10, 0x00, 0x00, 0x00)
    a.emit(0xF3, 0x48, 0xAB)
    a.emit(0x66, 0xC7, 0x04, 0x24, 0x01, 0x00)
    a.emit(0x48, 0x8D, 0x3C, 0x24)
    a.emit(0x48, 0x83, 0xC7, 0x02)
    a.lea_rsi("path")
    a.emit(0x48, 0xC7, 0xC1)
    a.emit_u32(len(path))
    a.emit(0xF3, 0xA4)

    # bind(fd, sockaddr, 110)
    a.emit(0x48, 0xC7, 0xC0, 0x31, 0x00, 0x00, 0x00)
    a.emit(0x4C, 0x89, 0xE7)
    a.emit(0x48, 0x89, 0xE6)
    a.emit(0x48, 0xC7, 0xC2, 0x6E, 0x00, 0x00, 0x00)
    a.emit(0x0F, 0x05)
    a.emit(0x48, 0x85, 0xC0)
    a.js("fail")

    # listen(fd, 16)
    a.emit(0x48, 0xC7, 0xC0, 0x32, 0x00, 0x00, 0x00)
    a.emit(0x4C, 0x89, 0xE7)
    a.emit(0x48, 0xC7, 0xC6, 0x10, 0x00, 0x00, 0x00)
    a.emit(0x0F, 0x05)
    a.emit(0x48, 0x85, 0xC0)
    a.js("fail")

    a.emit(0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00)
    a.emit(0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00)
    a.lea_rsi("marker")
    a.emit(0x48, 0xC7, 0xC2)
    a.emit_u32(len(marker))
    a.emit(0x0F, 0x05)

    # pause — keep the listen fd alive
    a.emit(0x48, 0xC7, 0xC0, 0x22, 0x00, 0x00, 0x00)
    a.emit(0x0F, 0x05)
    a.emit(0x48, 0x31, 0xFF)
    a.emit(0xEB, 0x07)
    a.label("fail")
    a.emit(0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00)
    a.emit(0x48, 0xC7, 0xC0, 0x3C, 0x00, 0x00, 0x00)
    a.emit(0x0F, 0x05)
    a.label("path")
    a.emit(*path)
    a.label("marker")
    a.emit(*marker)
    return build_static_user_elf(a.finish())


def shm_present_client_elf(fill: int, marker: bytes) -> bytes:
    """mmap + fill + ioctl(/dev/wl0) + write marker. Gate F SHM clients."""
    f = fill.to_bytes(4, "little")
    n = len(marker).to_bytes(4, "little")
    code = bytearray(
        [
            0x48,
            0xC7,
            0xC0,
            0x09,
            0x00,
            0x00,
            0x00,
            0x48,
            0x31,
            0xFF,
            0x48,
            0xC7,
            0xC6,
            0x00,
            0x40,
            0x00,
            0x00,
            0x48,
            0xC7,
            0xC2,
            0x03,
            0x00,
            0x00,
            0x00,
            0x49,
            0xC7,
            0xC2,
            0x22,
            0x80,
            0x00,
            0x00,
            0x49,
            0xC7,
            0xC0,
            0xFF,
            0xFF,
            0xFF,
            0xFF,
            0x4D,
            0x31,
            0xC9,
            0x0F,
            0x05,
            0x48,
            0x85,
            0xC0,
            0x0F,
            0x88,
            0x7C,
            0x00,
            0x00,
            0x00,
            0x49,
            0x89,
            0xC4,
            0x4C,
            0x89,
            0xE7,
            0x48,
            0xC7,
            0xC1,
            0x00,
            0x10,
            0x00,
            0x00,
            0xB8,
            f[0],
            f[1],
            f[2],
            f[3],
            0xF3,
            0xAB,
            0x48,
            0xC7,
            0xC0,
            0x02,
            0x00,
            0x00,
            0x00,
            0x48,
            0x8D,
            0x3D,
            0x6A,
            0x00,
            0x00,
            0x00,
            0x48,
            0xC7,
            0xC6,
            0x02,
            0x00,
            0x00,
            0x00,
            0x48,
            0x31,
            0xD2,
            0x0F,
            0x05,
            0x48,
            0x85,
            0xC0,
            0x0F,
            0x88,
            0x45,
            0x00,
            0x00,
            0x00,
            0x49,
            0x89,
            0xC5,
            0x48,
            0xC7,
            0xC0,
            0x10,
            0x00,
            0x00,
            0x00,
            0x4C,
            0x89,
            0xEF,
            0x48,
            0xC7,
            0xC6,
            0x01,
            0x00,
            0x4C,
            0x57,
            0x4C,
            0x89,
            0xE2,
            0x0F,
            0x05,
            0x48,
            0x85,
            0xC0,
            0x0F,
            0x88,
            0x23,
            0x00,
            0x00,
            0x00,
            0x48,
            0xC7,
            0xC0,
            0x01,
            0x00,
            0x00,
            0x00,
            0x48,
            0xC7,
            0xC7,
            0x01,
            0x00,
            0x00,
            0x00,
            0x48,
            0x8D,
            0x35,
            0x27,
            0x00,
            0x00,
            0x00,
            0x48,
            0xC7,
            0xC2,
            n[0],
            n[1],
            n[2],
            n[3],
            0x0F,
            0x05,
            0x48,
            0x31,
            0xFF,
            0xEB,
            0x07,
            0x48,
            0xC7,
            0xC7,
            0x01,
            0x00,
            0x00,
            0x00,
            0x48,
            0xC7,
            0xC0,
            0x3C,
            0x00,
            0x00,
            0x00,
            0x0F,
            0x05,
            *b"/dev/wl0\0",
        ]
    )
    code.extend(marker)
    return build_static_user_elf(bytes(code))
