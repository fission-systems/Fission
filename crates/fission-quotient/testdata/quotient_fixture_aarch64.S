/*
 * The flattened authentication dispatcher of `quotient_fixture.S`, for
 * AArch64, built into `quotient_fixture_aarch64.elf`:
 *
 *   clang --target=aarch64-linux-gnu -nostdlib -static -fuse-ld=lld \
 *     -Wl,--build-id=none -Wl,-e,_start \
 *     -o quotient_fixture_aarch64.elf quotient_fixture_aarch64.S
 *
 * Same protocol, same context layout (mode at 0, acc at 4), same opaque
 * predicate; ctx in x0, cmd in w1, result in w0, return address in x30.
 */
        .text

        .globl  _start
        .type   _start,%function
_start:
        brk     #0
        .size   _start, .-_start

        .globl  auth_step
        .type   auth_step,%function
auth_step:
        ldr     w8, [x0, #4]
        add     w8, w8, w1
        str     w8, [x0, #4]
        mov     w9, #0
        mov     w10, #0x7a31
.Lauth_loop:
        mov     w11, #0x7a31
        cmp     w10, w11
        b.eq    .Lauth_check_mode
        mov     w11, #0x1f09
        cmp     w10, w11
        b.eq    .Lauth_challenge
        mov     w11, #0xc442
        cmp     w10, w11
        b.eq    .Lauth_decoy
        mov     w11, #0x3b77
        cmp     w10, w11
        b.eq    .Lauth_idle
        b       .Lauth_ret
.Lauth_check_mode:
        ldr     w12, [x0]
        add     w13, w1, #1
        mul     w13, w13, w1
        tbnz    w13, #0, .Lauth_go_decoy
        cbz     w12, .Lauth_from_idle
        cmp     w12, #1
        b.eq    .Lauth_from_challenge
        mov     w9, #1
        mov     w10, #0x99e0
        b       .Lauth_loop
.Lauth_from_idle:
        mov     w10, #0x3b77
        b       .Lauth_loop
.Lauth_idle:
        and     w14, w1, #0xff
        cmp     w14, #0x5a
        b.ne    .Lauth_stay_idle
        mov     w15, #1
        str     w15, [x0]
.Lauth_stay_idle:
        mov     w10, #0x99e0
        b       .Lauth_loop
.Lauth_from_challenge:
        mov     w10, #0x1f09
        b       .Lauth_loop
.Lauth_challenge:
        and     w14, w1, #0xff
        cmp     w14, #0xa5
        b.ne    .Lauth_reset
        mov     w15, #2
        str     w15, [x0]
        mov     w9, #1
        mov     w10, #0x99e0
        b       .Lauth_loop
.Lauth_reset:
        str     wzr, [x0]
        mov     w10, #0x99e0
        b       .Lauth_loop
.Lauth_go_decoy:
        mov     w10, #0xc442
        b       .Lauth_loop
.Lauth_decoy:
        str     wzr, [x0]
        mov     w10, #0x99e0
        b       .Lauth_loop
.Lauth_ret:
        mov     w0, w9
        ret
        .size   auth_step, .-auth_step
