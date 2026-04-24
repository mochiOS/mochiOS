OUTPUT_FORMAT("elf64-x86-64")
OUTPUT_ARCH(i386:x86-64)
ENTRY(_start)

PHDRS {
  text PT_LOAD FLAGS(5);
  data PT_LOAD FLAGS(6);
}

SECTIONS {
  /* Link user programs at 0x200000 (matches kernel expectations) */
  . = 0x200000;

  .text : {
    *(.text._start)
    *(.text .text.*)
  } :text

  .rodata : ALIGN(0x1000) {
    *(.rodata .rodata.*)
  } :data

  .data : ALIGN(0x1000) {
    *(.data .data.*)
  } :data

  .bss : ALIGN(0x1000) {
    *(COMMON)
    *(.bss .bss.*)
  } :data

  /DISCARD/ : {
    *(.eh_frame)
    *(.comment)
  }
}
