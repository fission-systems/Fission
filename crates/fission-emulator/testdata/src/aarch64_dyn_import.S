/*
 * Safe AArch64 shared-object image used only inside Fission's Linux HLE.
 * It contains one R_AARCH64_RELATIVE data pointer and one imported puts call.
 * Rebuild with:
 *   zig cc -target aarch64-linux-musl -nostdlib -fPIC -shared \
 *     -Wl,-e,_start -Wl,--build-id=none \
 *     -o testdata/aarch64_dyn_import.elf testdata/src/aarch64_dyn_import.S
 */
.text
.global _start
.type _start, %function
_start:
    adrp x0, local_pointer
    add x0, x0, :lo12:local_pointer
    ldr x0, [x0]
    bl puts
    mov x8, #93                 // AArch64 Linux exit syscall
    mov x0, #0
    svc #0
.size _start, .-_start

.data
.p2align 3
local_pointer:
    .xword local_data
local_data:
    .asciz "Fission AArch64 dynamic HLE fixture"
