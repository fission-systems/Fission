/* Freestanding Linux x86-64 HLE ABI regression. Runs only in Fission.
 * _start imports libc startup; main saves and checks a real FS guard.
 * An argv value beginning with ! deliberately corrupts the saved guard.
 */
.text
.global _start
.type _start,@function
_start:
    mov (%rsp), %rsi
    lea 8(%rsp), %rdx
    lea main(%rip), %rdi
    call __libc_start_main@PLT
    ud2
.size _start, .-_start
.type main,@function
main:
    sub $24, %rsp
    mov %fs:0x28, %rax
    mov %rax, 8(%rsp)
    cmp $2, %edi
    jne verdict
    mov 8(%rsi), %rax
    cmpb $33, (%rax)
    jne verdict
    xorq $1, 8(%rsp)
verdict:
    mov 8(%rsp), %rax
    xor %fs:0x28, %rax
    jne failed
    mov $7, %eax
    add $24, %rsp
    ret
failed:
    call __stack_chk_fail@PLT
    ud2
.size main, .-main
.section .note.GNU-stack,"",@progbits
