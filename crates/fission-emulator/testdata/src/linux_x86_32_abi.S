/* Safe freestanding i386 Linux guest for the 32-bit syscall ABI: `writev`
 * with a two-entry `struct iovec` (8 bytes each), then `fstat64` into a
 * 96-byte `struct stat64` with a guard word after it. Results are left in
 * exported data for the test to read. */
.intel_syntax noprefix

.section .rodata
first:  .ascii "ab"
second: .ascii "cd\n"

.section .data
.balign 4
iov:
    .long first,  2
    .long second, 3

.section .bss
.balign 16
.global writev_result
.type writev_result, @object
writev_result:  .long 0
.size writev_result, .-writev_result
.global fstat_result
.type fstat_result, @object
fstat_result:   .long 0
.size fstat_result, .-fstat_result
.global stat_buffer
.type stat_buffer, @object
stat_buffer:    .skip 96
.size stat_buffer, .-stat_buffer
.global stat_guard
.type stat_guard, @object
stat_guard:     .long 0
.size stat_guard, .-stat_guard

.section .text
.global _start
_start:
    mov dword ptr [stat_guard], 0x5AFEC0DE

    /* writev(1, iov, 2) */
    mov eax, 146
    mov ebx, 1
    lea ecx, [iov]
    mov edx, 2
    int 0x80
    mov [writev_result], eax

    /* fstat64(1, stat_buffer) */
    mov eax, 197
    mov ebx, 1
    lea ecx, [stat_buffer]
    int 0x80
    mov [fstat_result], eax

    /* exit(0) */
    mov eax, 1
    xor ebx, ebx
    int 0x80
