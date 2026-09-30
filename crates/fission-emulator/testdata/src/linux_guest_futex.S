/*
 * Deterministic two-task Linux futex fixture. Build from this directory with:
 *
 *   zig cc -target x86_64-linux-musl -static -nostdlib -fno-pie -no-pie \
 *     -Wl,-e,_start -Wl,--build-id=none -o ../linux_guest_futex.elf \
 *     linux_guest_futex.S
 *
 * The parent waits on a shared word. The child changes it, wakes one waiter,
 * and exits. The parent returns 0 only if it resumed after the wake and sees
 * the shared value.
 */

    .section .text
    .global _start
    .type _start,@function
_start:
    mov $0x1310f00, %rdi        /* thread flags plus parent/child TID tracking */
    lea child_stack_top(%rip), %rsi
    lea parent_tid_word(%rip), %rdx
    lea child_tid_word(%rip), %r10
    xor %r8d, %r8d              /* tls */
    mov $56, %eax               /* clone */
    syscall
    test %rax, %rax
    jz child

parent:
    lea shared_word(%rip), %rdi
    mov $128, %esi              /* FUTEX_WAIT_PRIVATE */
    xor %edx, %edx              /* expected value: 0 */
    xor %r10d, %r10d            /* no timeout */
    xor %r8d, %r8d
    xor %r9d, %r9d
    mov $202, %eax              /* futex */
    syscall
    test %rax, %rax
    jne failed_wait
    cmpl $1, shared_word(%rip)
    jne failed_value
    cmpl $0, parent_tid_word(%rip)
    je failed_parent_tid
    cmpl $0, child_tid_word(%rip)
    jne failed_child_tid
    xor %edi, %edi
    jmp exit

child:
    movl $1, shared_word(%rip)
    lea shared_word(%rip), %rdi
    mov $129, %esi              /* FUTEX_WAKE_PRIVATE */
    mov $1, %edx                /* wake at most one task */
    xor %r10d, %r10d
    xor %r8d, %r8d
    xor %r9d, %r9d
    mov $202, %eax
    syscall
    xor %edi, %edi
    jmp exit

failed_wait:
    mov $1, %edi
    jmp exit
failed_value:
    mov $2, %edi
    jmp exit
failed_parent_tid:
    mov $3, %edi
    jmp exit
failed_child_tid:
    mov $4, %edi
exit:
    mov $60, %eax
    syscall
    ud2
    .size _start, .-_start

    .section .bss
    .align 4
shared_word:
    .long 0
parent_tid_word:
    .long 0
child_tid_word:
    .long 0
    .align 16
child_stack:
    .skip 16384
child_stack_top:
    .section .note.GNU-stack,"",@progbits
