ENTRY(_start)

SECTIONS
{
    . = 0;

    .text : {
        KEEP(*(.text._start))
        *(.text*)
    }

    .rodata : {
        *(.rodata*)
    }

    .data : {
        *(.data*)
    }

    .bss : {
        *(.bss*)
        *(COMMON)
    }

    . = ALIGN(16);
    ASSERT(. <= 0x1000, "userland image must fit in one page")

    /DISCARD/ : {
        *(.comment*)
        *(.eh_frame*)
        *(.note*)
    }
}
