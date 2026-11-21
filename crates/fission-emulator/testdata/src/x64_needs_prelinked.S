/* Safe freestanding x86-64 Linux guest that depends on x64_prelinked_lib.so:
 * it calls the library's `answer` through the PLT and then through the
 * library's own `answer_ptr` (which the library's relocation fills in), and
 * exits with the sum -- 84 when both were bound to where the library is. */
.intel_syntax noprefix
.section .text
.global _start
_start:
    call answer@PLT
    mov ebx, eax
    mov rax, [rip + answer_ptr@GOTPCREL]
    call [rax]
    add ebx, eax
    mov edi, ebx
    mov eax, 60
    syscall
