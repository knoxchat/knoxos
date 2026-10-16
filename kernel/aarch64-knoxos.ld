/* KnoxOS AArch64 linker script — QEMU virt identity map */
OUTPUT_FORMAT("elf64-littleaarch64")
ENTRY(_start)

/* QEMU virt RAM starts at 0x40000000; -kernel jumps here with MMU off. */
KERNEL_VADDR = 0x40080000;

SECTIONS {
    . = KERNEL_VADDR;

    .text ALIGN(4K) : {
        KEEP(*(.text.boot))   /* Boot entry must be first */
        *(.text .text.*)
    }

    .rodata ALIGN(4K) : {
        *(.rodata .rodata.*)
    }

    .data ALIGN(4K) : {
        *(.data .data.*)
    }

    .bss ALIGN(4K) : {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        __bss_end = .;
    }

    _kernel_end = .;

    /DISCARD/ : {
        *(.eh_frame)
        *(.comment)
        *(.note*)
    }
}
