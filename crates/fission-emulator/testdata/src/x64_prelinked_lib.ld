PHDRS
{
  text PT_LOAD FILEHDR PHDRS FLAGS(5);
  data PT_LOAD FLAGS(6);
  dynamic PT_DYNAMIC FLAGS(6);
}
SECTIONS
{
  . = 0x7000000 + SIZEOF_HEADERS;
  .dynsym : { *(.dynsym) } :text
  .dynstr : { *(.dynstr) } :text
  .hash : { *(.hash) } :text
  .gnu.hash : { *(.gnu.hash) } :text
  .rela.dyn : { *(.rela.dyn) } :text
  .text : { *(.text .text.*) } :text
  . = ALIGN(0x1000);
  .dynamic : { *(.dynamic) } :data :dynamic
  .data : { *(.data .data.*) *(.got .got.plt) } :data
  .bss : { *(.bss) } :data
  /DISCARD/ : { *(.comment) *(.eh_frame) *(.eh_frame_hdr) *(.note.*) }
}
