/* KnoxOS RISC-V 64 linker script */
OUTPUT_ARCH(riscv)
OUTPUT_FORMAT("elf64-littleriscv")
ENTRY(_start)

/* OpenSBI passes control at 0x80200000 for S-mode kernels */
KERNEL_START = 0x80200000;

SECTIONS {
    . = KERNEL_START;

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
        . = ALIGN(8);
        __bss_end = .;
    }

    _kernel_end = .;

    /DISCARD/ : {
        *(.eh_frame)
        *(.comment)
        *(.note*)
    }
}
