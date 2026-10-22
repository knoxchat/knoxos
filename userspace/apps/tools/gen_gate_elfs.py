#!/usr/bin/env python3
"""Assemble Gate BH–BK Ring 3 ELF payloads and print Rust byte arrays."""

from __future__ import annotations


class Asm:
    def __init__(self) -> None:
        self.buf = bytearray()
        self.labels: dict[str, int] = {}
        self.fixups: list[tuple[int, str]] = []  # (offset_of_disp32, label)

    def emit(self, *bs: int) -> None:
        self.buf.extend(bs)

    def label(self, name: str) -> None:
        self.labels[name] = len(self.buf)

    def lea_rsi_label(self, name: str) -> None:
        # 48 8D 35 xx xx xx xx   lea rsi, [rip+disp]
        self.emit(0x48, 0x8D, 0x35)
        self.fixups.append((len(self.buf), name))
        self.emit(0, 0, 0, 0)

    def finish(self) -> bytes:
        out = bytearray(self.buf)
        for off, name in self.fixups:
            target = self.labels[name]
            disp = target - (off + 4)
            out[off : off + 4] = disp.to_bytes(4, "little", signed=True)
        return bytes(out)

    def hex_rows(self) -> str:
        data = self.finish()
        rows = []
        i = 0
        while i < len(data):
            chunk = data[i : i + 15]
            rows.append("        " + ", ".join(f"0x{b:02X}" for b in chunk) + ",")
            i += 15
        return "\n".join(rows)


def mov_rax_imm(a: Asm, n: int) -> None:
    a.emit(0x48, 0xC7, 0xC0, n & 0xFF, (n >> 8) & 0xFF, (n >> 16) & 0xFF, (n >> 24) & 0xFF)


def mov_rdi_imm(a: Asm, n: int) -> None:
    a.emit(0x48, 0xC7, 0xC7, n & 0xFF, (n >> 8) & 0xFF, (n >> 16) & 0xFF, (n >> 24) & 0xFF)


def mov_rsi_imm(a: Asm, n: int) -> None:
    a.emit(0x48, 0xC7, 0xC6, n & 0xFF, (n >> 8) & 0xFF, (n >> 16) & 0xFF, (n >> 24) & 0xFF)


def mov_rdx_imm(a: Asm, n: int) -> None:
    a.emit(0x48, 0xC7, 0xC2, n & 0xFF, (n >> 8) & 0xFF, (n >> 16) & 0xFF, (n >> 24) & 0xFF)


def xor_rax(a: Asm) -> None:
    a.emit(0x48, 0x31, 0xC0)


def xor_rdi(a: Asm) -> None:
    a.emit(0x48, 0x31, 0xFF)


def xor_rsi(a: Asm) -> None:
    a.emit(0x48, 0x31, 0xF6)


def xor_rdx(a: Asm) -> None:
    a.emit(0x48, 0x31, 0xD2)


def xor_r8(a: Asm) -> None:
    a.emit(0x4D, 0x31, 0xC0)


def xor_r9(a: Asm) -> None:
    a.emit(0x4D, 0x31, 0xC9)


def xor_r10(a: Asm) -> None:
    a.emit(0x4D, 0x31, 0xD2)


def syscall(a: Asm) -> None:
    a.emit(0x0F, 0x05)


def test_rax(a: Asm) -> None:
    a.emit(0x48, 0x85, 0xC0)


def js_fail(a: Asm) -> None:
    a.emit(0x0F, 0x88)
    a.fixups.append((len(a.buf), "fail"))
    a.emit(0, 0, 0, 0)


def jnz_fail(a: Asm) -> None:
    a.emit(0x0F, 0x85)
    a.fixups.append((len(a.buf), "fail"))
    a.emit(0, 0, 0, 0)


def jne_fail(a: Asm) -> None:
    jnz_fail(a)


def je_fail(a: Asm) -> None:
    a.emit(0x0F, 0x84)
    a.fixups.append((len(a.buf), "fail"))
    a.emit(0, 0, 0, 0)


def cmp_rax_imm(a: Asm, n: int) -> None:
    if -128 <= n <= 127:
        a.emit(0x48, 0x83, 0xF8, n & 0xFF)
    else:
        a.emit(0x48, 0x3D, n & 0xFF, (n >> 8) & 0xFF, (n >> 16) & 0xFF, (n >> 24) & 0xFF)


def write_marker_and_exit(a: Asm, marker: str) -> None:
    mov_rax_imm(a, 1)
    mov_rdi_imm(a, 1)
    a.lea_rsi_label("marker")
    mov_rdx_imm(a, len(marker))
    syscall(a)
    mov_rax_imm(a, 60)
    xor_rdi(a)
    syscall(a)
    a.label("fail")
    mov_rax_imm(a, 60)
    mov_rdi_imm(a, 1)
    syscall(a)
    a.label("marker")
    a.buf.extend(marker.encode())


def enosys_elf(nr: int, marker: str) -> Asm:
    a = Asm()
    mov_rax_imm(a, nr)
    xor_rdi(a)
    xor_rsi(a)
    xor_rdx(a)
    syscall(a)
    cmp_rax_imm(a, -38)  # ENOSYS
    jne_fail(a)
    write_marker_and_exit(a, marker)
    return a


def select_elf() -> Asm:
    """pipe + select idle, write, select returns 1. Marker GATE_BH1 select\\n."""
    a = Asm()
    # sub rsp, 0xA0
    a.emit(0x48, 0x81, 0xEC, 0xA0, 0x00, 0x00, 0x00)
    # pipe(rsp)
    a.emit(0x48, 0x89, 0xE7)  # mov rdi, rsp
    mov_rax_imm(a, 22)
    syscall(a)
    test_rax(a)
    js_fail(a)
    # zero fdset at [rsp+0x10] (16 qwords)
    a.emit(0x48, 0x8D, 0x7C, 0x24, 0x10)  # lea rdi, [rsp+0x10]
    a.emit(0x48, 0xC7, 0xC1, 0x10, 0x00, 0x00, 0x00)  # mov rcx, 16
    xor_rax(a)
    a.emit(0xF3, 0x48, 0xAB)  # rep stosq
    # bts [rsp+0x10], read_fd
    a.emit(0x8B, 0x44, 0x24, 0x00)  # mov eax, [rsp]
    a.emit(0x48, 0x0F, 0xAB, 0x44, 0x24, 0x10)  # bts [rsp+0x10], rax
    # nfds = read_fd + 1
    a.emit(0x8B, 0x7C, 0x24, 0x00)  # mov edi, [rsp]
    a.emit(0xFF, 0xC7)  # inc edi
    a.emit(0x48, 0x8D, 0x74, 0x24, 0x10)  # lea rsi, [rsp+0x10]
    xor_rdx(a)
    xor_r10(a)
    xor_r8(a)
    mov_rax_imm(a, 23)  # select
    syscall(a)
    test_rax(a)
    jnz_fail(a)
    # write 1 byte to write end
    a.emit(0x8B, 0x7C, 0x24, 0x04)  # mov edi, [rsp+4]
    a.lea_rsi_label("xbyte")
    mov_rdx_imm(a, 1)
    mov_rax_imm(a, 1)
    syscall(a)
    cmp_rax_imm(a, 1)
    jne_fail(a)
    # reset fdset and select again
    a.emit(0x48, 0x8D, 0x7C, 0x24, 0x10)
    a.emit(0x48, 0xC7, 0xC1, 0x10, 0x00, 0x00, 0x00)
    xor_rax(a)
    a.emit(0xF3, 0x48, 0xAB)
    a.emit(0x8B, 0x44, 0x24, 0x00)
    a.emit(0x48, 0x0F, 0xAB, 0x44, 0x24, 0x10)
    a.emit(0x8B, 0x7C, 0x24, 0x00)
    a.emit(0xFF, 0xC7)
    a.emit(0x48, 0x8D, 0x74, 0x24, 0x10)
    xor_rdx(a)
    xor_r10(a)
    xor_r8(a)
    mov_rax_imm(a, 23)
    syscall(a)
    cmp_rax_imm(a, 1)
    jne_fail(a)
    write_marker_and_exit(a, "GATE_BH1 select\n")
    a.label("xbyte")
    a.buf.extend(b"x")
    return a


def inet_sockaddr_on_stack(a: Asm, port_be_imm: int) -> None:
    """Write sockaddr_in at [rsp]: AF_INET, port (as mov word 0xA05B style), 127.0.0.1."""
    a.emit(0x66, 0xC7, 0x04, 0x24, 0x02, 0x00)  # mov word [rsp], 2
    a.emit(0x66, 0xC7, 0x44, 0x24, 0x02, port_be_imm & 0xFF, (port_be_imm >> 8) & 0xFF)
    a.emit(0xC7, 0x44, 0x24, 0x04, 0x7F, 0x00, 0x00, 0x01)  # 127.0.0.1
    a.emit(0x48, 0xC7, 0x44, 0x24, 0x08, 0x00, 0x00, 0x00, 0x00)


def socket_tcp(a: Asm) -> None:
    mov_rax_imm(a, 41)
    mov_rdi_imm(a, 2)
    mov_rsi_imm(a, 1)
    xor_rdx(a)
    syscall(a)
    test_rax(a)
    js_fail(a)


def getsockname_elf() -> Asm:
    a = Asm()
    a.emit(0x48, 0x83, 0xEC, 0x40)  # sub rsp, 0x40
    socket_tcp(a)
    a.emit(0x49, 0x89, 0xC4)  # mov r12, rax
    inet_sockaddr_on_stack(a, 0xA071)
    mov_rax_imm(a, 49)  # bind
    a.emit(0x4C, 0x89, 0xE7)  # mov rdi, r12
    a.emit(0x48, 0x89, 0xE6)  # mov rsi, rsp
    mov_rdx_imm(a, 16)
    syscall(a)
    test_rax(a)
    js_fail(a)
    # zero out-addr at [rsp+0x20], addrlen=16 at [rsp+0x18]
    xor_rax(a)
    a.emit(0x48, 0x89, 0x44, 0x24, 0x20)
    a.emit(0x48, 0x89, 0x44, 0x24, 0x28)
    a.emit(0xC7, 0x44, 0x24, 0x18, 0x10, 0x00, 0x00, 0x00)
    mov_rax_imm(a, 51)  # getsockname
    a.emit(0x4C, 0x89, 0xE7)
    a.emit(0x48, 0x8D, 0x74, 0x24, 0x20)  # lea rsi, [rsp+0x20]
    a.emit(0x48, 0x8D, 0x54, 0x24, 0x18)  # lea rdx, [rsp+0x18]
    syscall(a)
    test_rax(a)
    js_fail(a)
    a.emit(0x66, 0x83, 0x7C, 0x24, 0x20, 0x02)  # cmp word [rsp+0x20], 2
    jne_fail(a)
    a.emit(0x66, 0x81, 0x7C, 0x24, 0x22, 0x71, 0xA0)  # cmp word [rsp+0x22], 0xA071
    jne_fail(a)
    write_marker_and_exit(a, "GATE_BH2 getsockname\n")
    return a


def getpeername_elf() -> Asm:
    a = Asm()
    a.emit(0x48, 0x83, 0xEC, 0x40)
    # listen socket
    socket_tcp(a)
    a.emit(0x49, 0x89, 0xC4)  # r12 = listen
    inet_sockaddr_on_stack(a, 0xA072)
    mov_rax_imm(a, 49)  # bind
    a.emit(0x4C, 0x89, 0xE7)
    a.emit(0x48, 0x89, 0xE6)
    mov_rdx_imm(a, 16)
    syscall(a)
    test_rax(a)
    js_fail(a)
    mov_rax_imm(a, 50)  # listen
    a.emit(0x4C, 0x89, 0xE7)
    mov_rsi_imm(a, 1)
    syscall(a)
    test_rax(a)
    js_fail(a)
    # client
    socket_tcp(a)
    a.emit(0x49, 0x89, 0xC5)  # r13 = client
    mov_rax_imm(a, 42)  # connect
    a.emit(0x4C, 0x89, 0xEF)
    a.emit(0x48, 0x89, 0xE6)
    mov_rdx_imm(a, 16)
    syscall(a)
    test_rax(a)
    js_fail(a)
    xor_rax(a)
    a.emit(0x48, 0x89, 0x44, 0x24, 0x20)
    a.emit(0x48, 0x89, 0x44, 0x24, 0x28)
    a.emit(0xC7, 0x44, 0x24, 0x18, 0x10, 0x00, 0x00, 0x00)
    mov_rax_imm(a, 52)  # getpeername
    a.emit(0x4C, 0x89, 0xEF)
    a.emit(0x48, 0x8D, 0x74, 0x24, 0x20)
    a.emit(0x48, 0x8D, 0x54, 0x24, 0x18)
    syscall(a)
    test_rax(a)
    js_fail(a)
    a.emit(0x66, 0x83, 0x7C, 0x24, 0x20, 0x02)
    jne_fail(a)
    a.emit(0x66, 0x81, 0x7C, 0x24, 0x22, 0x72, 0xA0)  # port 0xA072
    jne_fail(a)
    write_marker_and_exit(a, "GATE_BH3 getpeername\n")
    return a


def sendmsg_recvmsg_elf(which: str, port_imm: int) -> Asm:
    """UDP bind + sendmsg + recvmsg of one byte 'x'."""
    a = Asm()
    a.emit(0x48, 0x81, 0xEC, 0x80, 0x00, 0x00, 0x00)
    # recv UDP socket
    mov_rax_imm(a, 41)
    mov_rdi_imm(a, 2)
    mov_rsi_imm(a, 2)  # SOCK_DGRAM
    xor_rdx(a)
    syscall(a)
    test_rax(a)
    js_fail(a)
    a.emit(0x49, 0x89, 0xC4)  # r12 = recv
    inet_sockaddr_on_stack(a, port_imm)
    mov_rax_imm(a, 49)  # bind
    a.emit(0x4C, 0x89, 0xE7)
    a.emit(0x48, 0x89, 0xE6)
    mov_rdx_imm(a, 16)
    syscall(a)
    test_rax(a)
    js_fail(a)
    # send UDP socket
    mov_rax_imm(a, 41)
    mov_rdi_imm(a, 2)
    mov_rsi_imm(a, 2)
    xor_rdx(a)
    syscall(a)
    test_rax(a)
    js_fail(a)
    a.emit(0x49, 0x89, 0xC5)  # r13 = send
    # iovec at [rsp+0x20]: base=&x, len=1
    a.lea_rsi_label("xbyte")
    a.emit(0x48, 0x89, 0x74, 0x24, 0x20)  # mov [rsp+0x20], rsi
    a.emit(0x48, 0xC7, 0x44, 0x24, 0x28, 0x01, 0x00, 0x00, 0x00)
    # msghdr at [rsp+0x30]
    a.emit(0x48, 0x89, 0x64, 0x24, 0x30)  # msg_name = rsp (dest)
    a.emit(0xC7, 0x44, 0x24, 0x38, 0x10, 0x00, 0x00, 0x00)  # namelen=16
    a.emit(0x48, 0x8D, 0x44, 0x24, 0x20)
    a.emit(0x48, 0x89, 0x44, 0x24, 0x40)  # msg_iov
    a.emit(0x48, 0xC7, 0x44, 0x24, 0x48, 0x01, 0x00, 0x00, 0x00)  # iovlen=1
    xor_rax(a)
    a.emit(0x48, 0x89, 0x44, 0x24, 0x50)  # control
    a.emit(0x48, 0x89, 0x44, 0x24, 0x58)
    a.emit(0x48, 0x89, 0x44, 0x24, 0x60)  # flags + pad
    mov_rax_imm(a, 46)  # sendmsg
    a.emit(0x4C, 0x89, 0xEF)
    a.emit(0x48, 0x8D, 0x74, 0x24, 0x30)
    xor_rdx(a)
    syscall(a)
    cmp_rax_imm(a, 1)
    jne_fail(a)
    # recv buffer at [rsp+0x70]
    a.emit(0xC6, 0x44, 0x24, 0x70, 0x00)
    a.emit(0x48, 0x8D, 0x44, 0x24, 0x70)
    a.emit(0x48, 0x89, 0x44, 0x24, 0x20)  # iov_base = buf
    a.emit(0x48, 0xC7, 0x44, 0x24, 0x28, 0x01, 0x00, 0x00, 0x00)
    xor_rax(a)
    a.emit(0x48, 0x89, 0x44, 0x24, 0x30)  # msg_name = 0
    a.emit(0x48, 0x89, 0x44, 0x24, 0x38)
    a.emit(0x48, 0x8D, 0x44, 0x24, 0x20)
    a.emit(0x48, 0x89, 0x44, 0x24, 0x40)
    a.emit(0x48, 0xC7, 0x44, 0x24, 0x48, 0x01, 0x00, 0x00, 0x00)
    mov_rax_imm(a, 47)  # recvmsg
    a.emit(0x4C, 0x89, 0xE7)
    a.emit(0x48, 0x8D, 0x74, 0x24, 0x30)
    xor_rdx(a)
    syscall(a)
    cmp_rax_imm(a, 1)
    jne_fail(a)
    a.emit(0x80, 0x7C, 0x24, 0x70, 0x78)  # cmp byte [rsp+0x70], 'x'
    jne_fail(a)
    write_marker_and_exit(a, which)
    a.label("xbyte")
    a.buf.extend(b"x")
    return a


def shutdown_elf() -> Asm:
    a = Asm()
    a.emit(0x48, 0x83, 0xEC, 0x20)
    socket_tcp(a)
    a.emit(0x49, 0x89, 0xC4)
    mov_rax_imm(a, 48)  # shutdown
    a.emit(0x4C, 0x89, 0xE7)
    mov_rsi_imm(a, 2)  # SHUT_RDWR
    syscall(a)
    test_rax(a)
    jnz_fail(a)
    write_marker_and_exit(a, "GATE_BI3 shutdown\n")
    return a


def sendmmsg_recvmmsg_elf(which: str, port_imm: int) -> Asm:
    """UDP bind + sendmmsg + recvmmsg of two bytes 'xy'. Returns message count 1."""
    a = Asm()
    a.emit(0x48, 0x81, 0xEC, 0x80, 0x00, 0x00, 0x00)
    # recv UDP socket
    mov_rax_imm(a, 41)
    mov_rdi_imm(a, 2)
    mov_rsi_imm(a, 2)  # SOCK_DGRAM
    xor_rdx(a)
    syscall(a)
    test_rax(a)
    js_fail(a)
    a.emit(0x49, 0x89, 0xC4)  # r12 = recv
    inet_sockaddr_on_stack(a, port_imm)
    mov_rax_imm(a, 49)  # bind
    a.emit(0x4C, 0x89, 0xE7)
    a.emit(0x48, 0x89, 0xE6)
    mov_rdx_imm(a, 16)
    syscall(a)
    test_rax(a)
    js_fail(a)
    # send UDP socket
    mov_rax_imm(a, 41)
    mov_rdi_imm(a, 2)
    mov_rsi_imm(a, 2)
    xor_rdx(a)
    syscall(a)
    test_rax(a)
    js_fail(a)
    a.emit(0x49, 0x89, 0xC5)  # r13 = send
    # iovec at [rsp+0x20]: base=&xy, len=2
    a.lea_rsi_label("xy")
    a.emit(0x48, 0x89, 0x74, 0x24, 0x20)
    a.emit(0x48, 0xC7, 0x44, 0x24, 0x28, 0x02, 0x00, 0x00, 0x00)
    # mmsghdr at [rsp+0x30]
    a.emit(0x48, 0x89, 0x64, 0x24, 0x30)  # msg_name = rsp
    a.emit(0xC7, 0x44, 0x24, 0x38, 0x10, 0x00, 0x00, 0x00)  # namelen=16
    a.emit(0x48, 0x8D, 0x44, 0x24, 0x20)
    a.emit(0x48, 0x89, 0x44, 0x24, 0x40)  # msg_iov
    a.emit(0x48, 0xC7, 0x44, 0x24, 0x48, 0x01, 0x00, 0x00, 0x00)  # iovlen=1
    xor_rax(a)
    a.emit(0x48, 0x89, 0x44, 0x24, 0x50)  # control
    a.emit(0x48, 0x89, 0x44, 0x24, 0x58)
    a.emit(0x48, 0x89, 0x44, 0x24, 0x60)  # flags
    a.emit(0xC7, 0x44, 0x24, 0x68, 0x00, 0x00, 0x00, 0x00)  # msg_len=0
    mov_rax_imm(a, 307)  # sendmmsg
    a.emit(0x4C, 0x89, 0xEF)
    a.emit(0x48, 0x8D, 0x74, 0x24, 0x30)
    mov_rdx_imm(a, 1)  # vlen=1
    xor_r10(a)
    syscall(a)
    cmp_rax_imm(a, 1)
    jne_fail(a)
    # recv buffer at [rsp+0x70]
    a.emit(0x66, 0xC7, 0x44, 0x24, 0x70, 0x00, 0x00)
    a.emit(0x48, 0x8D, 0x44, 0x24, 0x70)
    a.emit(0x48, 0x89, 0x44, 0x24, 0x20)
    a.emit(0x48, 0xC7, 0x44, 0x24, 0x28, 0x02, 0x00, 0x00, 0x00)
    xor_rax(a)
    a.emit(0x48, 0x89, 0x44, 0x24, 0x30)
    a.emit(0x48, 0x89, 0x44, 0x24, 0x38)
    a.emit(0x48, 0x8D, 0x44, 0x24, 0x20)
    a.emit(0x48, 0x89, 0x44, 0x24, 0x40)
    a.emit(0x48, 0xC7, 0x44, 0x24, 0x48, 0x01, 0x00, 0x00, 0x00)
    mov_rax_imm(a, 299)  # recvmmsg
    a.emit(0x4C, 0x89, 0xE7)
    a.emit(0x48, 0x8D, 0x74, 0x24, 0x30)
    mov_rdx_imm(a, 1)
    xor_r10(a)
    xor_r8(a)
    syscall(a)
    cmp_rax_imm(a, 1)
    jne_fail(a)
    a.emit(0x66, 0x81, 0x7C, 0x24, 0x70, 0x78, 0x79)  # cmp word [rsp+0x70], 'xy'
    jne_fail(a)
    write_marker_and_exit(a, which)
    a.label("xy")
    a.buf.extend(b"xy")
    return a


def setsockopt_elf() -> Asm:
    """SOCK_DGRAM socket; setsockopt(SO_REUSEADDR, 1) then getsockopt == 1."""
    a = Asm()
    a.emit(0x48, 0x83, 0xEC, 0x20)
    mov_rax_imm(a, 41)
    mov_rdi_imm(a, 2)
    mov_rsi_imm(a, 2)  # SOCK_DGRAM
    xor_rdx(a)
    syscall(a)
    test_rax(a)
    js_fail(a)
    a.emit(0x49, 0x89, 0xC4)
    a.emit(0xC7, 0x04, 0x24, 0x01, 0x00, 0x00, 0x00)  # [rsp] = 1
    mov_rax_imm(a, 54)  # setsockopt
    a.emit(0x4C, 0x89, 0xE7)
    mov_rsi_imm(a, 1)  # SOL_SOCKET
    mov_rdx_imm(a, 2)  # SO_REUSEADDR
    a.emit(0x4C, 0x8D, 0x14, 0x24)  # lea r10, [rsp]
    a.emit(0x49, 0xC7, 0xC0, 0x04, 0x00, 0x00, 0x00)  # mov r8, 4
    syscall(a)
    test_rax(a)
    js_fail(a)
    xor_rax(a)
    a.emit(0x48, 0x89, 0x04, 0x24)  # [rsp] = 0
    a.emit(0xC7, 0x44, 0x24, 0x08, 0x04, 0x00, 0x00, 0x00)  # optlen = 4
    mov_rax_imm(a, 55)  # getsockopt
    a.emit(0x4C, 0x89, 0xE7)
    mov_rsi_imm(a, 1)
    mov_rdx_imm(a, 2)
    a.emit(0x4C, 0x8D, 0x14, 0x24)
    a.emit(0x4C, 0x8D, 0x44, 0x24, 0x08)  # lea r8, [rsp+8]
    syscall(a)
    test_rax(a)
    js_fail(a)
    a.emit(0x83, 0x3C, 0x24, 0x01)  # cmp dword [rsp], 1
    jne_fail(a)
    write_marker_and_exit(a, "GATE_BK1 setsockopt\n")
    return a


def tcp_send_recv_elf(which: str, port_imm: int) -> Asm:
    """TCP listen/connect/accept then sendto/recvfrom of one byte 'x'."""
    a = Asm()
    a.emit(0x48, 0x83, 0xEC, 0x40)
    socket_tcp(a)
    a.emit(0x49, 0x89, 0xC4)  # r12 = listen
    inet_sockaddr_on_stack(a, port_imm)
    mov_rax_imm(a, 49)  # bind
    a.emit(0x4C, 0x89, 0xE7)
    a.emit(0x48, 0x89, 0xE6)
    mov_rdx_imm(a, 16)
    syscall(a)
    test_rax(a)
    js_fail(a)
    mov_rax_imm(a, 50)  # listen
    a.emit(0x4C, 0x89, 0xE7)
    mov_rsi_imm(a, 1)
    syscall(a)
    test_rax(a)
    js_fail(a)
    socket_tcp(a)
    a.emit(0x49, 0x89, 0xC5)  # r13 = client
    mov_rax_imm(a, 42)  # connect
    a.emit(0x4C, 0x89, 0xEF)
    a.emit(0x48, 0x89, 0xE6)
    mov_rdx_imm(a, 16)
    syscall(a)
    test_rax(a)
    js_fail(a)
    mov_rax_imm(a, 43)  # accept
    a.emit(0x4C, 0x89, 0xE7)
    xor_rsi(a)
    xor_rdx(a)
    syscall(a)
    test_rax(a)
    js_fail(a)
    a.emit(0x49, 0x89, 0xC6)  # r14 = accepted
    mov_rax_imm(a, 44)  # sendto
    a.emit(0x4C, 0x89, 0xEF)
    a.lea_rsi_label("xbyte")
    mov_rdx_imm(a, 1)
    xor_r10(a)
    xor_r8(a)
    xor_r9(a)
    syscall(a)
    cmp_rax_imm(a, 1)
    jne_fail(a)
    a.emit(0xC6, 0x44, 0x24, 0x20, 0x00)  # mov byte [rsp+0x20], 0
    mov_rax_imm(a, 45)  # recvfrom
    a.emit(0x4C, 0x89, 0xF7)  # mov rdi, r14
    a.emit(0x48, 0x8D, 0x74, 0x24, 0x20)
    mov_rdx_imm(a, 1)
    xor_r10(a)
    xor_r8(a)
    xor_r9(a)
    syscall(a)
    cmp_rax_imm(a, 1)
    jne_fail(a)
    a.emit(0x80, 0x7C, 0x24, 0x20, 0x78)  # cmp byte [rsp+0x20], 'x'
    jne_fail(a)
    write_marker_and_exit(a, which)
    a.label("xbyte")
    a.buf.extend(b"x")
    return a


def getsockopt_elf() -> Asm:
    """SOCK_DGRAM socket; getsockopt(SOL_SOCKET, SO_TYPE) == 2."""
    a = Asm()
    a.emit(0x48, 0x83, 0xEC, 0x20)
    mov_rax_imm(a, 41)
    mov_rdi_imm(a, 2)
    mov_rsi_imm(a, 2)  # SOCK_DGRAM
    xor_rdx(a)
    syscall(a)
    test_rax(a)
    js_fail(a)
    a.emit(0x49, 0x89, 0xC4)
    xor_rax(a)
    a.emit(0x48, 0x89, 0x04, 0x24)  # [rsp] = 0
    a.emit(0xC7, 0x44, 0x24, 0x08, 0x04, 0x00, 0x00, 0x00)  # optlen = 4
    mov_rax_imm(a, 55)  # getsockopt
    a.emit(0x4C, 0x89, 0xE7)
    mov_rsi_imm(a, 1)  # SOL_SOCKET
    mov_rdx_imm(a, 3)  # SO_TYPE
    a.emit(0x4C, 0x8D, 0x14, 0x24)  # lea r10, [rsp]
    a.emit(0x4C, 0x8D, 0x44, 0x24, 0x08)  # lea r8, [rsp+8]
    syscall(a)
    test_rax(a)
    js_fail(a)
    a.emit(0x83, 0x3C, 0x24, 0x02)  # cmp dword [rsp], 2
    jne_fail(a)
    write_marker_and_exit(a, "GATE_BJ3 getsockopt\n")
    return a


def rust_fn(name: str, doc: str, asm: Asm) -> str:
    return (
        f"/// {doc}\n"
        f"pub fn {name}() -> Vec<u8> {{\n"
        f"    build_static_user_elf(&[\n"
        f"{asm.hex_rows()}\n"
        f"    ])\n"
        f"}}\n"
    )


def main() -> None:
    parts = [
        rust_fn(
            "select_userspace_elf_data",
            "`pipe` + `select` idle, write, `select` returns 1; then `GATE_BH1 select\\n`.",
            select_elf(),
        ),
        rust_fn(
            "getsockname_userspace_elf_data",
            "AF_INET bind then `getsockname` reports family + port; write `GATE_BH2 getsockname\\n`.",
            getsockname_elf(),
        ),
        rust_fn(
            "getpeername_userspace_elf_data",
            "AF_INET listen/connect then `getpeername` reports the peer port; write `GATE_BH3 getpeername\\n`.",
            getpeername_elf(),
        ),
        rust_fn(
            "fsmount_enosys_elf_data",
            "`fsmount(0, 0, 0)` must return `-ENOSYS`; then write `GATE_BH4 enosys\\n`.",
            enosys_elf(432, "GATE_BH4 enosys\n"),
        ),
        rust_fn(
            "sendmsg_userspace_elf_data",
            "UDP `sendmsg` of `x` then `recvmsg`; write `GATE_BI1 sendmsg\\n`.",
            sendmsg_recvmsg_elf("GATE_BI1 sendmsg\n", 0xA073),
        ),
        rust_fn(
            "recvmsg_userspace_elf_data",
            "UDP `sendmsg`/`recvmsg` of `x`; write `GATE_BI2 recvmsg\\n`.",
            sendmsg_recvmsg_elf("GATE_BI2 recvmsg\n", 0xA074),
        ),
        rust_fn(
            "shutdown_userspace_elf_data",
            "AF_INET `socket` then `shutdown(SHUT_RDWR)` returns 0; write `GATE_BI3 shutdown\\n`.",
            shutdown_elf(),
        ),
        rust_fn(
            "open_tree_enosys_elf_data",
            "`open_tree(0, 0, 0)` must return `-ENOSYS`; then write `GATE_BI4 enosys\\n`.",
            enosys_elf(428, "GATE_BI4 enosys\n"),
        ),
        rust_fn(
            "sendmmsg_userspace_elf_data",
            "UDP `sendmmsg` of `xy` then `recvmmsg`; write `GATE_BJ1 sendmmsg\\n`.",
            sendmmsg_recvmmsg_elf("GATE_BJ1 sendmmsg\n", 0xA075),
        ),
        rust_fn(
            "recvmmsg_userspace_elf_data",
            "UDP `sendmmsg`/`recvmmsg` of `xy`; write `GATE_BJ2 recvmmsg\\n`.",
            sendmmsg_recvmmsg_elf("GATE_BJ2 recvmmsg\n", 0xA076),
        ),
        rust_fn(
            "getsockopt_userspace_elf_data",
            "AF_INET `SOCK_DGRAM` then `getsockopt(SOL_SOCKET, SO_TYPE)` is 2; write `GATE_BJ3 getsockopt\\n`.",
            getsockopt_elf(),
        ),
        rust_fn(
            "move_mount_enosys_elf_data",
            "`move_mount(0, 0, 0, 0, 0)` must return `-ENOSYS`; then write `GATE_BJ4 enosys\\n`.",
            enosys_elf(429, "GATE_BJ4 enosys\n"),
        ),
        rust_fn(
            "setsockopt_userspace_elf_data",
            "AF_INET `SOCK_DGRAM` `setsockopt(SO_REUSEADDR, 1)` then `getsockopt` is 1; write `GATE_BK1 setsockopt\\n`.",
            setsockopt_elf(),
        ),
        rust_fn(
            "tcp_send_userspace_elf_data",
            "TCP listen/connect/accept then `sendto`/`recvfrom` of `x`; write `GATE_BK2 tcp_send\\n`.",
            tcp_send_recv_elf("GATE_BK2 tcp_send\n", 0xA077),
        ),
        rust_fn(
            "tcp_recv_userspace_elf_data",
            "TCP listen/connect/accept then `sendto`/`recvfrom` of `x`; write `GATE_BK3 tcp_recv\\n`.",
            tcp_send_recv_elf("GATE_BK3 tcp_recv\n", 0xA078),
        ),
        rust_fn(
            "fspick_enosys_elf_data",
            "`fspick(0, 0, 0)` must return `-ENOSYS`; then write `GATE_BK4 enosys\\n`.",
            enosys_elf(433, "GATE_BK4 enosys\n"),
        ),
    ]
    print("\n".join(parts))


if __name__ == "__main__":
    main()
