ENTRY(_start)

USER_PROGRAM_BASE = 0x0000400000000000;
PAGE_SIZE = 0x1000;
/* The shell now carries built-in smoke commands plus a real EOF/exit input loop. */
USER_IMAGE_MAX_PAGES = 5;

PHDRS
{
    text PT_LOAD FLAGS(5);
    rodata PT_LOAD FLAGS(4);
    data PT_LOAD FLAGS(6);
}

SECTIONS
{
    . = USER_PROGRAM_BASE;

    .text : {
        KEEP(*(.text._start))
        *(.text*)
    } :text

    . = ALIGN(PAGE_SIZE);
    .rodata : {
        *(.rodata*)
    } :rodata

    . = ALIGN(PAGE_SIZE);
    .data : {
        *(.data*)
    } :data

    .bss : {
        *(.bss*)
        *(COMMON)
    } :data

    . = ALIGN(PAGE_SIZE);
    ASSERT(. <= USER_PROGRAM_BASE + (PAGE_SIZE * USER_IMAGE_MAX_PAGES), "userland image must fit in five pages")

    /DISCARD/ : {
        *(.comment*)
        *(.eh_frame*)
        *(.note*)
    }
}
