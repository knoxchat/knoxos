"""Gate F4 Paint / launcher: Ring 3 SHM client."""

from elfpack import shm_present_client_elf


def elf() -> bytes:
    return shm_present_client_elf(0xFF33_66CC, b"GATE_F4 launcher userspace\n")
