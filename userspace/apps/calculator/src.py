"""Gate F6 calculator: Ring 3 SHM client, not in-kernel WindowContentType."""

from elfpack import shm_present_client_elf


def elf() -> bytes:
    return shm_present_client_elf(0xFF20_C997, b"GATE_F6 calculator userspace\n")
