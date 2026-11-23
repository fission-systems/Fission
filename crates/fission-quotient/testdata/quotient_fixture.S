/*
 * Functions for fission-quotient's tests. Built into `quotient_fixture.elf`,
 * which is checked in next to this file:
 *
 *   clang --target=x86_64-linux-gnu -nostdlib -static -fuse-ld=lld \
 *     -Wl,--build-id=none -Wl,-e,_start \
 *     -o quotient_fixture.elf quotient_fixture.S
 *
 * Written in assembly so the control-flow shape is exactly what the tests
 * name, independent of what a compiler would make of it. Every step function
 * has the signature `int step(struct ctx *ctx, unsigned cmd)` (SysV x86-64:
 * ctx in rdi, cmd in esi, result in eax). The context persists between steps.
 */
        .intel_syntax noprefix
        .text

        .globl  _start
        .type   _start,@function
_start:
        ud2
        .size   _start, .-_start

/*
 * A three-state authentication parser (IDLE -> CHALLENGE -> AUTH_OK) behind
 * a flattened dispatcher. ctx->mode (offset 0) is the real control state;
 * ctx->acc (offset 4) accumulates every command and is never read by a
 * branch or the result. eax carries artificial dispatcher state numbers,
 * and the dispatcher is guarded by the opaque predicate
 * `(cmd * (cmd + 1)) & 1`, which is never true: its two decoy blocks are
 * statically reachable and dynamically dead. The second one would clobber
 * ctx->mode if it ever ran.
 *
 *   IDLE      cmd == 0x5a -> CHALLENGE, else IDLE;        returns 0
 *   CHALLENGE cmd == 0xa5 -> AUTH_OK (returns 1), else IDLE (returns 0)
 *   AUTH_OK   absorbing;                                    returns 1
 */
        .globl  auth_step
        .type   auth_step,@function
auth_step:
        mov     eax, dword ptr [rdi + 4]
        add     eax, esi
        mov     dword ptr [rdi + 4], eax
        xor     r8d, r8d
        mov     eax, 0x7a31
.Lauth_loop:
        cmp     eax, 0x7a31
        je      .Lauth_check_mode
        cmp     eax, 0x1f09
        je      .Lauth_challenge
        cmp     eax, 0xc442
        je      .Lauth_decoy_hash
        cmp     eax, 0x3b77
        je      .Lauth_idle
        cmp     eax, 0x5d12
        je      .Lauth_decoy_mix
        jmp     .Lauth_ret
.Lauth_check_mode:
        mov     ecx, dword ptr [rdi]
        lea     edx, [rsi + 1]
        imul    edx, esi
        test    dl, 1
        jne     .Lauth_go_decoy
        test    ecx, ecx
        je      .Lauth_from_idle
        cmp     ecx, 1
        je      .Lauth_from_challenge
        mov     r8d, 1
        mov     eax, 0x99e0
        jmp     .Lauth_loop
.Lauth_from_idle:
        mov     eax, 0x3b77
        jmp     .Lauth_loop
.Lauth_idle:
        cmp     sil, 0x5a
        jne     .Lauth_stay_idle
        mov     dword ptr [rdi], 1
.Lauth_stay_idle:
        mov     eax, 0x99e0
        jmp     .Lauth_loop
.Lauth_from_challenge:
        mov     eax, 0x1f09
        jmp     .Lauth_loop
.Lauth_challenge:
        cmp     sil, 0xa5
        jne     .Lauth_reset
        mov     dword ptr [rdi], 2
        mov     r8d, 1
        mov     eax, 0x99e0
        jmp     .Lauth_loop
.Lauth_reset:
        mov     dword ptr [rdi], 0
        mov     eax, 0x99e0
        jmp     .Lauth_loop
.Lauth_go_decoy:
        mov     eax, 0xc442
        jmp     .Lauth_loop
.Lauth_decoy_hash:
        mov     r9d, esi
        rol     r9d, 5
        xor     r9d, 0x1337
        mov     eax, 0x5d12
        jmp     .Lauth_loop
.Lauth_decoy_mix:
        imul    r9d, r9d, 31
        add     r9d, ecx
        mov     dword ptr [rdi], 0
        mov     eax, 0x99e0
        jmp     .Lauth_loop
.Lauth_ret:
        mov     eax, r8d
        ret
        .size   auth_step, .-auth_step

/*
 * Two encodings of one behaviour: ctx->mode alternates between 0 and 3 on
 * every step and the result is `mode != 1`, which both values satisfy. The
 * mode reaches the result, so it is control state, yet the two valuations
 * are observably equal: the minimal quotient has one state.
 */
        .globl  toggle_step
        .type   toggle_step,@function
toggle_step:
        mov     ecx, dword ptr [rdi]
        xor     ecx, 3
        mov     dword ptr [rdi], ecx
        xor     eax, eax
        cmp     ecx, 1
        setne   al
        ret
        .size   toggle_step, .-toggle_step

/*
 * The same toggle, with ctx->acc (offset 4) accumulating the command only on
 * the steps that leave mode 0. The result cannot see the difference, so the
 * one-state control quotient holds -- but acc's update differs between the
 * two merged valuations, which the quotient's data registers cannot express.
 */
        .globl  split_step
        .type   split_step,@function
split_step:
        mov     ecx, dword ptr [rdi]
        xor     ecx, 3
        mov     dword ptr [rdi], ecx
        test    ecx, ecx
        jne     .Lsplit_skip
        add     dword ptr [rdi + 4], esi
.Lsplit_skip:
        xor     eax, eax
        cmp     ecx, 1
        setne   al
        ret
        .size   split_step, .-split_step

/*
 * A counter whose value is the result: every count is observably distinct,
 * so the quotient is as large as the counter's range. Must stop at the state
 * budget and say so, not return a wrong small machine.
 */
        .globl  counter_step
        .type   counter_step,@function
counter_step:
        mov     eax, dword ptr [rdi]
        add     eax, esi
        mov     dword ptr [rdi], eax
        ret
        .size   counter_step, .-counter_step

/*
 * Reads the time-stamp counter: an effect outside the modelled boundary.
 */
        .globl  clock_step
        .type   clock_step,@function
clock_step:
        rdtsc
        and     eax, 1
        ret
        .size   clock_step, .-clock_step

/*
 * Calls another function: the callee's effects are outside the slice.
 */
        .globl  calling_step
        .type   calling_step,@function
calling_step:
        push    rbx
        mov     ebx, esi
        call    toggle_step
        add     eax, ebx
        pop     rbx
        ret
        .size   calling_step, .-calling_step

/*
 * A one-bit latch that a stack round trip carries: the step spills the
 * context pointer and the command to its frame, as unoptimised code does,
 * and reloads them. ctx->mode latches to 1 once cmd is odd; the result is
 * the latch.
 */
        .globl  latch_step
        .type   latch_step,@function
latch_step:
        push    rbp
        mov     rbp, rsp
        mov     qword ptr [rbp - 8], rdi
        mov     dword ptr [rbp - 12], esi
        mov     rax, qword ptr [rbp - 8]
        mov     ecx, dword ptr [rax]
        test    ecx, ecx
        jne     .Llatch_done
        mov     edx, dword ptr [rbp - 12]
        and     edx, 1
        mov     dword ptr [rax], edx
        mov     ecx, edx
.Llatch_done:
        mov     eax, ecx
        pop     rbp
        ret
        .size   latch_step, .-latch_step

/*
 * A latch set by a conditional move. x86 lifts `cmov` as a branch to the
 * next instruction, which lies inside the same block: a branch target need
 * not start a block.
 */
        .globl  select_step
        .type   select_step,@function
select_step:
        mov     eax, dword ptr [rdi]
        mov     ecx, 1
        cmp     esi, 7
        cmove   eax, ecx
        mov     dword ptr [rdi], eax
        ret
        .size   select_step, .-select_step

/*
 * Calls as effects (RFC #1093, P4). `isdigit` and `abort` stand in for the
 * C library's; the engine never runs them -- both sides stub every call --
 * and their names are what the signature library looks up.
 */
        .globl  isdigit
        .type   isdigit,@function
isdigit:
        lea     eax, [rdi - 48]
        cmp     eax, 10
        setb    al
        movzx   eax, al
        ret
        .size   isdigit, .-isdigit

        .globl  abort
        .type   abort,@function
abort:
        ud2
        .size   abort, .-abort

/*
 * A digit-run detector over a classifier call: ctx->in_run (offset 0)
 * follows what isdigit(cmd) said; the result is 1 when a run starts. The
 * classifier's answer is an input the environment chooses.
 */
        .globl  digits_step
        .type   digits_step,@function
digits_step:
        push    rbx
        mov     rbx, rdi
        mov     edi, esi
        call    isdigit
        xor     ecx, ecx
        test    eax, eax
        setne   cl
        mov     edx, dword ptr [rbx]
        mov     dword ptr [rbx], ecx
        xor     eax, eax
        test    edx, edx
        cmove   eax, ecx
        pop     rbx
        ret
        .size   digits_step, .-digits_step

/*
 * A guard that aborts: a zero command while ctx->armed (offset 0) is set
 * calls abort(), which does not return; any other command toggles armed.
 * The result is the new armed bit.
 */
        .globl  guarded_step
        .type   guarded_step,@function
guarded_step:
        push    rax
        mov     eax, dword ptr [rdi]
        test    eax, eax
        je      .Lguard_toggle
        test    esi, esi
        je      .Lguard_abort
.Lguard_toggle:
        xor     eax, 1
        mov     dword ptr [rdi], eax
        pop     rcx
        ret
.Lguard_abort:
        call    abort
        .size   guarded_step, .-guarded_step

/*
 * A tail call: ctx->parity (offset 0) flips every step, and the step ends
 * by jumping to isdigit(parity) -- its return is the step's.
 */
        .globl  tail_step
        .type   tail_step,@function
tail_step:
        mov     eax, dword ptr [rdi]
        xor     eax, 1
        mov     dword ptr [rdi], eax
        mov     edi, eax
        jmp     isdigit
        .size   tail_step, .-tail_step

/*
 * `select_step` with a `nop` after the conditional move: the branch the
 * `cmov` lifts to targets an instruction that has no P-code of its own.
 */
        .globl  padded_select_step
        .type   padded_select_step,@function
padded_select_step:
        mov     eax, dword ptr [rdi]
        mov     ecx, 1
        cmp     esi, 7
        cmove   eax, ecx
        nop
        mov     dword ptr [rdi], eax
        ret
        .size   padded_select_step, .-padded_select_step
