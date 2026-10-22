"""Gate F7 Files: Ring 3 SHM client, not in-kernel WindowContentType."""

from elfpack import shm_present_client_elf


def elf() -> bytes:
    return shm_present_client_elf(0xFF4E_8A26, b"GATE_F7 files userspace\n")
