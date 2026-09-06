/* KnoxOS AArch64 linker script */
OUTPUT_FORMAT("elf64-littleaarch64")
ENTRY(_start)

KERNEL_VADDR = 0xFFFF000000080000;

SECTIONS {
    . = KERNEL_VADDR;

    .text ALIGN(4K) : AT(ADDR(.text) - KERNEL_VADDR + 0x80000) {
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
