/* The loader occupies the bottom of the 64 KB RAM, below the 12-byte header
   it places just under the main firmware at 0x4000. */
MEMORY { RAM : ORIGIN = 0x00000000, LENGTH = 0x3ff0 }
ENTRY(_start)
SECTIONS {
    .text : { KEEP(*(.text.start)) *(.text .text.*) } > RAM
    .rodata : { *(.rodata .rodata.*) *(.srodata .srodata.*) } > RAM
    .data : { *(.data .data.*) *(.sdata .sdata.*) } > RAM
    .bss (NOLOAD) : { _bss_start = .; *(.bss .bss.*) *(.sbss .sbss.*) *(COMMON) _bss_end = .; } > RAM
    _stack_top = 0x10000;
    /DISCARD/ : { *(.eh_frame) *(.comment) }
}
