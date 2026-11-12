/* The main firmware is loaded at 0x4000. The stack grows down from 64 KB;
   the top 8 KB is left to it. */
MEMORY { RAM : ORIGIN = 0x00004000, LENGTH = 0xa000 }
ENTRY(_start)
SECTIONS {
    .text : { KEEP(*(.text.start)) *(.text .text.*) } > RAM
    .rodata : { *(.rodata .rodata.*) *(.srodata .srodata.*) } > RAM
    .data : { *(.data .data.*) *(.sdata .sdata.*) } > RAM
    .bss (NOLOAD) : { _bss_start = .; *(.bss .bss.*) *(.sbss .sbss.*) *(COMMON) _bss_end = .; } > RAM
    _stack_top = 0x10000;
    /DISCARD/ : { *(.eh_frame) *(.comment) }
}
