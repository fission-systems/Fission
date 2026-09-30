/* Safe freestanding AArch64 guest for the generic readlinkat HLE path. */
.section .rodata
proc_exe:
    .asciz "/proc/self/exe"

.section .bss
.balign 16
.global readlink_buffer
.type readlink_buffer, %object
readlink_buffer:
    .skip 512
.size readlink_buffer, .-readlink_buffer
.global readlink_result
.type readlink_result, %object
readlink_result:
    .quad 0
.size readlink_result, .-readlink_result

.section .text
.global _start
_start:
    /* readlinkat(AT_FDCWD, "/proc/self/exe", buffer, sizeof(buffer)) */
    mov x0, #-100
    adr x1, proc_exe
    adr x2, readlink_buffer
    mov x3, #512
    mov x8, #78
    svc #0
    adr x9, readlink_result
    str x0, [x9]
    cmp x0, #0
    b.le fail

    mov x0, #0
    b exit
fail:
    mov x0, #1
exit:
    mov x8, #94
    svc #0
