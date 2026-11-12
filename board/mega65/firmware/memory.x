/* 64 KB of block RAM: code, data, then the stack growing down from the top. */
MEMORY { RAM : ORIGIN = 0x00000000, LENGTH = 64K }
ENTRY(_start)
SECTIONS {
    .text : { KEEP(*(.text.start)) *(.text .text.*) } > RAM
    .rodata : { *(.rodata .rodata.*) *(.srodata .srodata.*) } > RAM
    .data : { *(.data .data.*) *(.sdata .sdata.*) } > RAM
    .bss (NOLOAD) : { _bss_start = .; *(.bss .bss.*) *(.sbss .sbss.*) *(COMMON) _bss_end = .; } > RAM
    _stack_top = ORIGIN(RAM) + LENGTH(RAM);
    /DISCARD/ : { *(.eh_frame) *(.comment) }
}
