//! x86-64: register tables, curated instructions, undefined-result masks and
//! the filters that keep the host safe.
//!
//! Every mask below cites the "Flags Affected" (or "Operation") section of the
//! instruction's page in the Intel 64 and IA-32 Architectures Software
//! Developer's Manual, Volume 2 (order no. 325383, cited as "SDM 2"). A mask is
//! a statement about that manual: where it says a flag or result is undefined,
//! the two sides may disagree; nowhere else.

use crate::fission::{FissionMachine, Lifted, touches_registers};
use crate::model::{BUF_LEN, CpuState, Isa, Mask, int_value, vec_value};
use crate::native::buf_addr;
use crate::rng::Rng;
use crate::{Curated, Target};

pub static ISA: Isa = Isa {
    name: "x86_64",
    gpr: &[
        "RAX", "RCX", "RDX", "RBX", "RSP", "RBP", "RSI", "RDI", "R8", "R9", "R10", "R11", "R12",
        "R13", "R14", "R15",
    ],
    flags: &["CF", "PF", "AF", "ZF", "SF", "OF", "DF"],
    vec: &[
        "XMM0", "XMM1", "XMM2", "XMM3", "XMM4", "XMM5", "XMM6", "XMM7", "XMM8", "XMM9", "XMM10",
        "XMM11", "XMM12", "XMM13", "XMM14", "XMM15",
    ],
    control: ("MXCSR", 0x1F80),
    ignored_gprs: 0,
};

pub static TARGET: Target = Target {
    isa: &ISA,
    fixture: "x64_static_printf_malloc.elf",
    curated,
    state,
    random_insn,
    skip,
    repeats,
    mask,
    extra,
};

const CF: u8 = 1 << 0;
const PF: u8 = 1 << 1;
const AF: u8 = 1 << 2;
const ZF: u8 = 1 << 3;
const SF: u8 = 1 << 4;
const OF: u8 = 1 << 5;
const ALL_STATUS: u8 = CF | PF | AF | ZF | SF | OF;

const RAX: usize = 0;
const RCX: usize = 1;
const RDX: usize = 2;
const RSP: usize = 4;
const RSI: usize = 6;
const RDI: usize = 7;

/// Hand-picked instructions, assembled with `llvm-mc -triple=x86_64
/// -x86-asm-syntax=intel`. Memory operands use RSI/RDI, which the curated
/// states always point into the buffer.
const CURATED: &[(&str, &[u8])] = &[
    ("add eax, ecx", &[0x01, 0xc8]),
    ("add rax, rcx", &[0x48, 0x01, 0xc8]),
    ("add al, cl", &[0x00, 0xc8]),
    ("add ax, cx", &[0x66, 0x01, 0xc8]),
    ("add eax, 0x7fffffff", &[0x05, 0xff, 0xff, 0xff, 0x7f]),
    ("add dword ptr [rsi+8], ecx", &[0x01, 0x4e, 0x08]),
    ("adc eax, ecx", &[0x11, 0xc8]),
    ("adc rax, rcx", &[0x48, 0x11, 0xc8]),
    ("adc al, cl", &[0x10, 0xc8]),
    ("adc qword ptr [rdi], rax", &[0x48, 0x11, 0x07]),
    ("sbb eax, ecx", &[0x19, 0xc8]),
    ("sbb rax, rcx", &[0x48, 0x19, 0xc8]),
    ("sbb al, cl", &[0x18, 0xc8]),
    ("sub eax, ecx", &[0x29, 0xc8]),
    ("sub rax, rcx", &[0x48, 0x29, 0xc8]),
    ("sub ax, cx", &[0x66, 0x29, 0xc8]),
    ("cmp eax, ecx", &[0x39, 0xc8]),
    ("cmp rax, rcx", &[0x48, 0x39, 0xc8]),
    ("cmp al, cl", &[0x38, 0xc8]),
    ("cmp dword ptr [rsi], 0x10", &[0x83, 0x3e, 0x10]),
    ("and eax, ecx", &[0x21, 0xc8]),
    ("and rax, rcx", &[0x48, 0x21, 0xc8]),
    ("or rax, rcx", &[0x48, 0x09, 0xc8]),
    ("or al, cl", &[0x08, 0xc8]),
    ("xor eax, ecx", &[0x31, 0xc8]),
    ("xor ax, cx", &[0x66, 0x31, 0xc8]),
    ("test eax, ecx", &[0x85, 0xc8]),
    ("test al, cl", &[0x84, 0xc8]),
    ("test rax, rcx", &[0x48, 0x85, 0xc8]),
    ("neg eax", &[0xf7, 0xd8]),
    ("neg rax", &[0x48, 0xf7, 0xd8]),
    ("neg al", &[0xf6, 0xd8]),
    ("not rax", &[0x48, 0xf7, 0xd0]),
    ("inc eax", &[0xff, 0xc0]),
    ("inc al", &[0xfe, 0xc0]),
    ("dec rax", &[0x48, 0xff, 0xc8]),
    ("dec cx", &[0x66, 0xff, 0xc9]),
    ("xadd ecx, eax", &[0x0f, 0xc1, 0xc1]),
    ("xadd byte ptr [rsi], sil", &[0x40, 0x0f, 0xc0, 0x36]),
    ("xchg dword ptr [rsi], edi", &[0x87, 0x3e]),
    ("xacquire xchg dword ptr [rsi], ecx", &[0xf2, 0x87, 0x0e]),
    (
        "pinsrb xmm0, byte ptr [rsi], 5",
        &[0x66, 0x0f, 0x3a, 0x20, 0x06, 0x05],
    ),
    (
        "punpcklqdq xmm0, xmmword ptr [rsi]",
        &[0x66, 0x0f, 0x6c, 0x06],
    ),
    (
        "shld word ptr [rsi], dx, 24",
        &[0x66, 0x0f, 0xa4, 0x16, 0x18],
    ),
    // Encodings an assembler does not emit but the CPU accepts.
    ("rex.b pause (= xchg r8d, eax)", &[0xf3, 0x41, 0x90]),
    ("rex.x nop (= xchg eax, eax)", &[0x42, 0x90]),
    ("66 66 add bp, ax", &[0x66, 0x66, 0x03, 0xe8]),
    ("pushfw", &[0x66, 0x9c]),
    ("lock xadd dword ptr [rsi], eax", &[0xf0, 0x0f, 0xc1, 0x06]),
    ("cmpxchg ecx, edx", &[0x0f, 0xb1, 0xd1]),
    ("cmpxchg rcx, rdx", &[0x48, 0x0f, 0xb1, 0xd1]),
    (
        "lock cmpxchg dword ptr [rsi], edx",
        &[0xf0, 0x0f, 0xb1, 0x16],
    ),
    ("xchg eax, ecx", &[0x91]),
    ("xchg qword ptr [rdi], rax", &[0x48, 0x87, 0x07]),
    ("bswap eax", &[0x0f, 0xc8]),
    ("bswap rax", &[0x48, 0x0f, 0xc8]),
    ("shl eax, cl", &[0xd3, 0xe0]),
    ("shr eax, cl", &[0xd3, 0xe8]),
    ("sar eax, cl", &[0xd3, 0xf8]),
    ("shl rax, cl", &[0x48, 0xd3, 0xe0]),
    ("shr rax, cl", &[0x48, 0xd3, 0xe8]),
    ("sar rax, cl", &[0x48, 0xd3, 0xf8]),
    ("shl al, cl", &[0xd2, 0xe0]),
    ("shr ax, cl", &[0x66, 0xd3, 0xe8]),
    ("sar al, cl", &[0xd2, 0xf8]),
    ("shl eax", &[0xd1, 0xe0]),
    ("shr eax", &[0xd1, 0xe8]),
    ("sar eax", &[0xd1, 0xf8]),
    ("shl eax, 5", &[0xc1, 0xe0, 0x05]),
    ("shr rax, 33", &[0x48, 0xc1, 0xe8, 0x21]),
    ("sar ax, 3", &[0x66, 0xc1, 0xf8, 0x03]),
    ("rol eax, cl", &[0xd3, 0xc0]),
    ("ror eax, cl", &[0xd3, 0xc8]),
    ("rol rax, cl", &[0x48, 0xd3, 0xc0]),
    ("ror rax, cl", &[0x48, 0xd3, 0xc8]),
    ("rol al, cl", &[0xd2, 0xc0]),
    ("ror ax, cl", &[0x66, 0xd3, 0xc8]),
    ("rol eax", &[0xd1, 0xc0]),
    ("ror eax", &[0xd1, 0xc8]),
    ("rol eax, 7", &[0xc1, 0xc0, 0x07]),
    ("rcl eax, cl", &[0xd3, 0xd0]),
    ("rcr eax, cl", &[0xd3, 0xd8]),
    ("rcl rax, cl", &[0x48, 0xd3, 0xd0]),
    ("rcr rax, cl", &[0x48, 0xd3, 0xd8]),
    ("rcl al, cl", &[0xd2, 0xd0]),
    ("rcr ax, cl", &[0x66, 0xd3, 0xd8]),
    ("rcl eax", &[0xd1, 0xd0]),
    ("rcr eax", &[0xd1, 0xd8]),
    ("shld eax, edx, cl", &[0x0f, 0xa5, 0xd0]),
    ("shrd eax, edx, cl", &[0x0f, 0xad, 0xd0]),
    ("shld rax, rdx, cl", &[0x48, 0x0f, 0xa5, 0xd0]),
    ("shrd rax, rdx, cl", &[0x48, 0x0f, 0xad, 0xd0]),
    ("shld eax, edx, 7", &[0x0f, 0xa4, 0xd0, 0x07]),
    ("shrd rax, rdx, 40", &[0x48, 0x0f, 0xac, 0xd0, 0x28]),
    ("mul ecx", &[0xf7, 0xe1]),
    ("mul rcx", &[0x48, 0xf7, 0xe1]),
    ("mul cl", &[0xf6, 0xe1]),
    ("mul cx", &[0x66, 0xf7, 0xe1]),
    ("imul ecx", &[0xf7, 0xe9]),
    ("imul rcx", &[0x48, 0xf7, 0xe9]),
    ("imul cl", &[0xf6, 0xe9]),
    ("imul eax, ecx", &[0x0f, 0xaf, 0xc1]),
    ("imul rax, rcx", &[0x48, 0x0f, 0xaf, 0xc1]),
    ("imul ax, cx", &[0x66, 0x0f, 0xaf, 0xc1]),
    (
        "imul eax, ecx, 0x1234",
        &[0x69, 0xc1, 0x34, 0x12, 0x00, 0x00],
    ),
    ("imul rax, rcx, -7", &[0x48, 0x6b, 0xc1, 0xf9]),
    ("div ecx", &[0xf7, 0xf1]),
    ("div rcx", &[0x48, 0xf7, 0xf1]),
    ("div cl", &[0xf6, 0xf1]),
    ("div cx", &[0x66, 0xf7, 0xf1]),
    ("idiv ecx", &[0xf7, 0xf9]),
    ("idiv rcx", &[0x48, 0xf7, 0xf9]),
    ("idiv cl", &[0xf6, 0xf9]),
    ("bsf eax, ecx", &[0x0f, 0xbc, 0xc1]),
    ("bsf rax, rcx", &[0x48, 0x0f, 0xbc, 0xc1]),
    ("bsf ax, cx", &[0x66, 0x0f, 0xbc, 0xc1]),
    ("bsr eax, ecx", &[0x0f, 0xbd, 0xc1]),
    ("bsr rax, rcx", &[0x48, 0x0f, 0xbd, 0xc1]),
    ("bsr ax, cx", &[0x66, 0x0f, 0xbd, 0xc1]),
    ("tzcnt eax, ecx", &[0xf3, 0x0f, 0xbc, 0xc1]),
    ("tzcnt rax, rcx", &[0xf3, 0x48, 0x0f, 0xbc, 0xc1]),
    ("tzcnt ax, cx", &[0x66, 0xf3, 0x0f, 0xbc, 0xc1]),
    ("lzcnt eax, ecx", &[0xf3, 0x0f, 0xbd, 0xc1]),
    ("lzcnt rax, rcx", &[0xf3, 0x48, 0x0f, 0xbd, 0xc1]),
    ("lzcnt ax, cx", &[0x66, 0xf3, 0x0f, 0xbd, 0xc1]),
    ("popcnt eax, ecx", &[0xf3, 0x0f, 0xb8, 0xc1]),
    ("popcnt rax, rcx", &[0xf3, 0x48, 0x0f, 0xb8, 0xc1]),
    ("popcnt ax, cx", &[0x66, 0xf3, 0x0f, 0xb8, 0xc1]),
    ("bt eax, ecx", &[0x0f, 0xa3, 0xc8]),
    ("bt rax, rcx", &[0x48, 0x0f, 0xa3, 0xc8]),
    ("bts eax, ecx", &[0x0f, 0xab, 0xc8]),
    ("btr rax, rcx", &[0x48, 0x0f, 0xb3, 0xc8]),
    ("btc eax, ecx", &[0x0f, 0xbb, 0xc8]),
    ("bt eax, 5", &[0x0f, 0xba, 0xe0, 0x05]),
    ("bts rax, 63", &[0x48, 0x0f, 0xba, 0xe8, 0x3f]),
    ("btr eax, 31", &[0x0f, 0xba, 0xf0, 0x1f]),
    ("btc ax, 3", &[0x66, 0x0f, 0xba, 0xf8, 0x03]),
    ("cmove eax, ecx", &[0x0f, 0x44, 0xc1]),
    ("cmovne rax, rcx", &[0x48, 0x0f, 0x45, 0xc1]),
    ("cmovb eax, ecx", &[0x0f, 0x42, 0xc1]),
    ("cmovae eax, ecx", &[0x0f, 0x43, 0xc1]),
    ("cmovbe eax, ecx", &[0x0f, 0x46, 0xc1]),
    ("cmova rax, rcx", &[0x48, 0x0f, 0x47, 0xc1]),
    ("cmovl eax, ecx", &[0x0f, 0x4c, 0xc1]),
    ("cmovge eax, ecx", &[0x0f, 0x4d, 0xc1]),
    ("cmovle eax, ecx", &[0x0f, 0x4e, 0xc1]),
    ("cmovg rax, rcx", &[0x48, 0x0f, 0x4f, 0xc1]),
    ("cmovs eax, ecx", &[0x0f, 0x48, 0xc1]),
    ("cmovns eax, ecx", &[0x0f, 0x49, 0xc1]),
    ("cmovo eax, ecx", &[0x0f, 0x40, 0xc1]),
    ("cmovno eax, ecx", &[0x0f, 0x41, 0xc1]),
    ("cmovp eax, ecx", &[0x0f, 0x4a, 0xc1]),
    ("cmovnp eax, ecx", &[0x0f, 0x4b, 0xc1]),
    ("cmove ax, cx", &[0x66, 0x0f, 0x44, 0xc1]),
    ("cmove eax, dword ptr [rsi]", &[0x0f, 0x44, 0x06]),
    ("sete al", &[0x0f, 0x94, 0xc0]),
    ("setne cl", &[0x0f, 0x95, 0xc1]),
    ("setb al", &[0x0f, 0x92, 0xc0]),
    ("setae al", &[0x0f, 0x93, 0xc0]),
    ("setbe al", &[0x0f, 0x96, 0xc0]),
    ("seta al", &[0x0f, 0x97, 0xc0]),
    ("setl al", &[0x0f, 0x9c, 0xc0]),
    ("setge al", &[0x0f, 0x9d, 0xc0]),
    ("setle al", &[0x0f, 0x9e, 0xc0]),
    ("setg al", &[0x0f, 0x9f, 0xc0]),
    ("sets al", &[0x0f, 0x98, 0xc0]),
    ("setns al", &[0x0f, 0x99, 0xc0]),
    ("seto al", &[0x0f, 0x90, 0xc0]),
    ("setno al", &[0x0f, 0x91, 0xc0]),
    ("setp al", &[0x0f, 0x9a, 0xc0]),
    ("setnp al", &[0x0f, 0x9b, 0xc0]),
    ("sete byte ptr [rdi]", &[0x0f, 0x94, 0x07]),
    ("movsx eax, cl", &[0x0f, 0xbe, 0xc1]),
    ("movsx rax, cl", &[0x48, 0x0f, 0xbe, 0xc1]),
    ("movsx eax, cx", &[0x0f, 0xbf, 0xc1]),
    ("movsx rax, cx", &[0x48, 0x0f, 0xbf, 0xc1]),
    ("movsxd rax, ecx", &[0x48, 0x63, 0xc1]),
    ("movsx ax, cl", &[0x66, 0x0f, 0xbe, 0xc1]),
    ("movzx eax, cl", &[0x0f, 0xb6, 0xc1]),
    ("movzx eax, cx", &[0x0f, 0xb7, 0xc1]),
    ("movzx rax, cl", &[0x48, 0x0f, 0xb6, 0xc1]),
    ("movzx ax, cl", &[0x66, 0x0f, 0xb6, 0xc1]),
    ("movzx eax, byte ptr [rsi]", &[0x0f, 0xb6, 0x06]),
    ("movsx rax, word ptr [rsi]", &[0x48, 0x0f, 0xbf, 0x06]),
    ("cbw", &[0x66, 0x98]),
    ("cwde", &[0x98]),
    ("cdqe", &[0x48, 0x98]),
    ("cwd", &[0x66, 0x99]),
    ("cdq", &[0x99]),
    ("cqo", &[0x48, 0x99]),
    ("lea rax, [rcx+rdx*4+0x10]", &[0x48, 0x8d, 0x44, 0x91, 0x10]),
    ("lea eax, [rcx+rdx]", &[0x8d, 0x04, 0x11]),
    ("lea ax, [rcx+rdx*2-3]", &[0x66, 0x8d, 0x44, 0x51, 0xfd]),
    ("mov eax, ecx", &[0x89, 0xc8]),
    ("mov al, ch", &[0x88, 0xe8]),
    ("mov ah, cl", &[0x88, 0xcc]),
    ("mov rax, qword ptr [rsi+16]", &[0x48, 0x8b, 0x46, 0x10]),
    ("mov dword ptr [rdi-8], ecx", &[0x89, 0x4f, 0xf8]),
    ("mov word ptr [rdi], cx", &[0x66, 0x89, 0x0f]),
    (
        "movabs rax, 0x123456789abcdef0",
        &[0x48, 0xb8, 0xf0, 0xde, 0xbc, 0x9a, 0x78, 0x56, 0x34, 0x12],
    ),
    ("push rax", &[0x50]),
    ("pop rcx", &[0x59]),
    ("push qword ptr [rsi]", &[0xff, 0x36]),
    ("lahf", &[0x9f]),
    ("sahf", &[0x9e]),
    ("pushfq", &[0x9c]),
    ("stc", &[0xf9]),
    ("clc", &[0xf8]),
    ("cmc", &[0xf5]),
    ("std", &[0xfd]),
    ("cld", &[0xfc]),
    ("leave", &[0xc9]),
    ("movsb", &[0xa4]),
    ("movsq", &[0x48, 0xa5]),
    ("stosd", &[0xab]),
    ("lodsb", &[0xac]),
    ("scasb", &[0xae]),
    ("cmpsd", &[0xa7]),
    ("rep movsb", &[0xf3, 0xa4]),
    ("rep stosq", &[0xf3, 0x48, 0xab]),
    ("rep stosb", &[0xf3, 0xaa]),
    ("repe cmpsb", &[0xf3, 0xa6]),
    ("repne scasb", &[0xf2, 0xae]),
    ("cvtsi2ss xmm0, eax", &[0xf3, 0x0f, 0x2a, 0xc0]),
    ("cvtsi2ss xmm0, rax", &[0xf3, 0x48, 0x0f, 0x2a, 0xc0]),
    ("cvtsi2sd xmm0, eax", &[0xf2, 0x0f, 0x2a, 0xc0]),
    ("cvtsi2sd xmm0, rax", &[0xf2, 0x48, 0x0f, 0x2a, 0xc0]),
    ("cvtss2si eax, xmm1", &[0xf3, 0x0f, 0x2d, 0xc1]),
    ("cvtss2si rax, xmm1", &[0xf3, 0x48, 0x0f, 0x2d, 0xc1]),
    ("cvttss2si eax, xmm1", &[0xf3, 0x0f, 0x2c, 0xc1]),
    ("cvttss2si rax, xmm1", &[0xf3, 0x48, 0x0f, 0x2c, 0xc1]),
    ("cvtsd2si eax, xmm1", &[0xf2, 0x0f, 0x2d, 0xc1]),
    ("cvtsd2si rax, xmm1", &[0xf2, 0x48, 0x0f, 0x2d, 0xc1]),
    ("cvttsd2si eax, xmm1", &[0xf2, 0x0f, 0x2c, 0xc1]),
    ("cvttsd2si rax, xmm1", &[0xf2, 0x48, 0x0f, 0x2c, 0xc1]),
    ("cvtss2sd xmm0, xmm1", &[0xf3, 0x0f, 0x5a, 0xc1]),
    ("cvtsd2ss xmm0, xmm1", &[0xf2, 0x0f, 0x5a, 0xc1]),
    ("cvtdq2ps xmm0, xmm1", &[0x0f, 0x5b, 0xc1]),
    ("cvtps2dq xmm0, xmm1", &[0x66, 0x0f, 0x5b, 0xc1]),
    ("cvttps2dq xmm0, xmm1", &[0xf3, 0x0f, 0x5b, 0xc1]),
    ("cvtdq2pd xmm0, xmm1", &[0xf3, 0x0f, 0xe6, 0xc1]),
    ("cvtpd2dq xmm0, xmm1", &[0xf2, 0x0f, 0xe6, 0xc1]),
    ("cvttpd2dq xmm0, xmm1", &[0x66, 0x0f, 0xe6, 0xc1]),
    ("cvtps2pd xmm0, xmm1", &[0x0f, 0x5a, 0xc1]),
    ("cvtpd2ps xmm0, xmm1", &[0x66, 0x0f, 0x5a, 0xc1]),
    (
        "cvtsi2sd xmm0, qword ptr [rsi]",
        &[0xf2, 0x48, 0x0f, 0x2a, 0x06],
    ),
    ("cvtss2sd xmm0, dword ptr [rsi]", &[0xf3, 0x0f, 0x5a, 0x06]),
    ("minss xmm0, xmm1", &[0xf3, 0x0f, 0x5d, 0xc1]),
    ("maxss xmm0, xmm1", &[0xf3, 0x0f, 0x5f, 0xc1]),
    ("minsd xmm0, xmm1", &[0xf2, 0x0f, 0x5d, 0xc1]),
    ("maxsd xmm0, xmm1", &[0xf2, 0x0f, 0x5f, 0xc1]),
    ("minps xmm0, xmm1", &[0x0f, 0x5d, 0xc1]),
    ("maxps xmm0, xmm1", &[0x0f, 0x5f, 0xc1]),
    ("minpd xmm0, xmm1", &[0x66, 0x0f, 0x5d, 0xc1]),
    ("maxpd xmm0, xmm1", &[0x66, 0x0f, 0x5f, 0xc1]),
    ("sqrtss xmm0, xmm1", &[0xf3, 0x0f, 0x51, 0xc1]),
    ("sqrtsd xmm0, xmm1", &[0xf2, 0x0f, 0x51, 0xc1]),
    ("sqrtps xmm0, xmm1", &[0x0f, 0x51, 0xc1]),
    ("sqrtpd xmm0, xmm1", &[0x66, 0x0f, 0x51, 0xc1]),
    ("addss xmm0, xmm1", &[0xf3, 0x0f, 0x58, 0xc1]),
    ("subss xmm0, xmm1", &[0xf3, 0x0f, 0x5c, 0xc1]),
    ("mulss xmm0, xmm1", &[0xf3, 0x0f, 0x59, 0xc1]),
    ("divss xmm0, xmm1", &[0xf3, 0x0f, 0x5e, 0xc1]),
    ("addsd xmm0, xmm1", &[0xf2, 0x0f, 0x58, 0xc1]),
    ("subsd xmm0, xmm1", &[0xf2, 0x0f, 0x5c, 0xc1]),
    ("mulsd xmm0, xmm1", &[0xf2, 0x0f, 0x59, 0xc1]),
    ("divsd xmm0, xmm1", &[0xf2, 0x0f, 0x5e, 0xc1]),
    ("addps xmm0, xmm1", &[0x0f, 0x58, 0xc1]),
    ("subpd xmm0, xmm1", &[0x66, 0x0f, 0x5c, 0xc1]),
    ("mulps xmm0, xmm1", &[0x0f, 0x59, 0xc1]),
    ("divpd xmm0, xmm1", &[0x66, 0x0f, 0x5e, 0xc1]),
    ("addsubps xmm0, xmm1", &[0xf2, 0x0f, 0xd0, 0xc1]),
    ("haddpd xmm0, xmm1", &[0x66, 0x0f, 0x7c, 0xc1]),
    ("comiss xmm0, xmm1", &[0x0f, 0x2f, 0xc1]),
    ("ucomiss xmm0, xmm1", &[0x0f, 0x2e, 0xc1]),
    ("comisd xmm0, xmm1", &[0x66, 0x0f, 0x2f, 0xc1]),
    ("ucomisd xmm0, xmm1", &[0x66, 0x0f, 0x2e, 0xc1]),
    ("pshufd xmm0, xmm1, 0x1b", &[0x66, 0x0f, 0x70, 0xc1, 0x1b]),
    ("pshufd xmm0, xmm1, 0x4e", &[0x66, 0x0f, 0x70, 0xc1, 0x4e]),
    ("pshuflw xmm0, xmm1, 0x1b", &[0xf2, 0x0f, 0x70, 0xc1, 0x1b]),
    ("pshufhw xmm0, xmm1, 0xb1", &[0xf3, 0x0f, 0x70, 0xc1, 0xb1]),
    ("pshufb xmm0, xmm1", &[0x66, 0x0f, 0x38, 0x00, 0xc1]),
    ("shufps xmm0, xmm1, 0x4e", &[0x0f, 0xc6, 0xc1, 0x4e]),
    ("shufpd xmm0, xmm1, 1", &[0x66, 0x0f, 0xc6, 0xc1, 0x01]),
    ("punpcklbw xmm0, xmm1", &[0x66, 0x0f, 0x60, 0xc1]),
    ("punpckhbw xmm0, xmm1", &[0x66, 0x0f, 0x68, 0xc1]),
    ("punpcklwd xmm0, xmm1", &[0x66, 0x0f, 0x61, 0xc1]),
    ("punpckhwd xmm0, xmm1", &[0x66, 0x0f, 0x69, 0xc1]),
    ("punpckldq xmm0, xmm1", &[0x66, 0x0f, 0x62, 0xc1]),
    ("punpckhdq xmm0, xmm1", &[0x66, 0x0f, 0x6a, 0xc1]),
    ("punpcklqdq xmm0, xmm1", &[0x66, 0x0f, 0x6c, 0xc1]),
    ("punpckhqdq xmm0, xmm1", &[0x66, 0x0f, 0x6d, 0xc1]),
    ("unpcklps xmm0, xmm1", &[0x0f, 0x14, 0xc1]),
    ("unpckhpd xmm0, xmm1", &[0x66, 0x0f, 0x15, 0xc1]),
    ("packsswb xmm0, xmm1", &[0x66, 0x0f, 0x63, 0xc1]),
    ("packuswb xmm0, xmm1", &[0x66, 0x0f, 0x67, 0xc1]),
    ("packssdw xmm0, xmm1", &[0x66, 0x0f, 0x6b, 0xc1]),
    ("packusdw xmm0, xmm1", &[0x66, 0x0f, 0x38, 0x2b, 0xc1]),
    ("pmovmskb eax, xmm1", &[0x66, 0x0f, 0xd7, 0xc1]),
    ("movmskps eax, xmm1", &[0x0f, 0x50, 0xc1]),
    ("movmskpd eax, xmm1", &[0x66, 0x0f, 0x50, 0xc1]),
    ("paddb xmm0, xmm1", &[0x66, 0x0f, 0xfc, 0xc1]),
    ("paddw xmm0, xmm1", &[0x66, 0x0f, 0xfd, 0xc1]),
    ("paddd xmm0, xmm1", &[0x66, 0x0f, 0xfe, 0xc1]),
    ("paddq xmm0, xmm1", &[0x66, 0x0f, 0xd4, 0xc1]),
    ("psubb xmm0, xmm1", &[0x66, 0x0f, 0xf8, 0xc1]),
    ("psubq xmm0, xmm1", &[0x66, 0x0f, 0xfb, 0xc1]),
    ("paddsb xmm0, xmm1", &[0x66, 0x0f, 0xec, 0xc1]),
    ("paddusw xmm0, xmm1", &[0x66, 0x0f, 0xdd, 0xc1]),
    ("psubsw xmm0, xmm1", &[0x66, 0x0f, 0xe9, 0xc1]),
    ("psubusb xmm0, xmm1", &[0x66, 0x0f, 0xd8, 0xc1]),
    ("pmullw xmm0, xmm1", &[0x66, 0x0f, 0xd5, 0xc1]),
    ("pmulhw xmm0, xmm1", &[0x66, 0x0f, 0xe5, 0xc1]),
    ("pmulhuw xmm0, xmm1", &[0x66, 0x0f, 0xe4, 0xc1]),
    ("pmuludq xmm0, xmm1", &[0x66, 0x0f, 0xf4, 0xc1]),
    ("pmuldq xmm0, xmm1", &[0x66, 0x0f, 0x38, 0x28, 0xc1]),
    ("pmulld xmm0, xmm1", &[0x66, 0x0f, 0x38, 0x40, 0xc1]),
    ("pmaddwd xmm0, xmm1", &[0x66, 0x0f, 0xf5, 0xc1]),
    ("psadbw xmm0, xmm1", &[0x66, 0x0f, 0xf6, 0xc1]),
    ("pavgb xmm0, xmm1", &[0x66, 0x0f, 0xe0, 0xc1]),
    ("pavgw xmm0, xmm1", &[0x66, 0x0f, 0xe3, 0xc1]),
    ("pcmpeqb xmm0, xmm1", &[0x66, 0x0f, 0x74, 0xc1]),
    ("pcmpeqd xmm0, xmm1", &[0x66, 0x0f, 0x76, 0xc1]),
    ("pcmpeqq xmm0, xmm1", &[0x66, 0x0f, 0x38, 0x29, 0xc1]),
    ("pcmpgtb xmm0, xmm1", &[0x66, 0x0f, 0x64, 0xc1]),
    ("pcmpgtd xmm0, xmm1", &[0x66, 0x0f, 0x66, 0xc1]),
    ("pcmpgtq xmm0, xmm1", &[0x66, 0x0f, 0x38, 0x37, 0xc1]),
    ("pand xmm0, xmm1", &[0x66, 0x0f, 0xdb, 0xc1]),
    ("pandn xmm0, xmm1", &[0x66, 0x0f, 0xdf, 0xc1]),
    ("por xmm0, xmm1", &[0x66, 0x0f, 0xeb, 0xc1]),
    ("pxor xmm0, xmm1", &[0x66, 0x0f, 0xef, 0xc1]),
    ("andps xmm0, xmm1", &[0x0f, 0x54, 0xc1]),
    ("andnpd xmm0, xmm1", &[0x66, 0x0f, 0x55, 0xc1]),
    ("orps xmm0, xmm1", &[0x0f, 0x56, 0xc1]),
    ("xorpd xmm0, xmm1", &[0x66, 0x0f, 0x57, 0xc1]),
    ("psllw xmm0, 3", &[0x66, 0x0f, 0x71, 0xf0, 0x03]),
    ("pslld xmm0, 17", &[0x66, 0x0f, 0x72, 0xf0, 0x11]),
    ("psllq xmm0, 40", &[0x66, 0x0f, 0x73, 0xf0, 0x28]),
    ("psrlw xmm0, 9", &[0x66, 0x0f, 0x71, 0xd0, 0x09]),
    ("psrld xmm0, 5", &[0x66, 0x0f, 0x72, 0xd0, 0x05]),
    ("psrlq xmm0, 63", &[0x66, 0x0f, 0x73, 0xd0, 0x3f]),
    ("psraw xmm0, 2", &[0x66, 0x0f, 0x71, 0xe0, 0x02]),
    ("psrad xmm0, 31", &[0x66, 0x0f, 0x72, 0xe0, 0x1f]),
    ("psllw xmm0, xmm1", &[0x66, 0x0f, 0xf1, 0xc1]),
    ("psrld xmm0, xmm1", &[0x66, 0x0f, 0xd2, 0xc1]),
    ("psraw xmm0, xmm1", &[0x66, 0x0f, 0xe1, 0xc1]),
    ("psllq xmm0, xmm1", &[0x66, 0x0f, 0xf3, 0xc1]),
    ("pslldq xmm0, 5", &[0x66, 0x0f, 0x73, 0xf8, 0x05]),
    ("psrldq xmm0, 3", &[0x66, 0x0f, 0x73, 0xd8, 0x03]),
    ("pminub xmm0, xmm1", &[0x66, 0x0f, 0xda, 0xc1]),
    ("pmaxub xmm0, xmm1", &[0x66, 0x0f, 0xde, 0xc1]),
    ("pminsw xmm0, xmm1", &[0x66, 0x0f, 0xea, 0xc1]),
    ("pmaxsw xmm0, xmm1", &[0x66, 0x0f, 0xee, 0xc1]),
    ("pminsb xmm0, xmm1", &[0x66, 0x0f, 0x38, 0x38, 0xc1]),
    ("pmaxsb xmm0, xmm1", &[0x66, 0x0f, 0x38, 0x3c, 0xc1]),
    ("pminsd xmm0, xmm1", &[0x66, 0x0f, 0x38, 0x39, 0xc1]),
    ("pmaxsd xmm0, xmm1", &[0x66, 0x0f, 0x38, 0x3d, 0xc1]),
    ("pminud xmm0, xmm1", &[0x66, 0x0f, 0x38, 0x3b, 0xc1]),
    ("pmaxud xmm0, xmm1", &[0x66, 0x0f, 0x38, 0x3f, 0xc1]),
    ("pminuw xmm0, xmm1", &[0x66, 0x0f, 0x38, 0x3a, 0xc1]),
    ("pmaxuw xmm0, xmm1", &[0x66, 0x0f, 0x38, 0x3e, 0xc1]),
    ("pabsb xmm0, xmm1", &[0x66, 0x0f, 0x38, 0x1c, 0xc1]),
    ("pabsd xmm0, xmm1", &[0x66, 0x0f, 0x38, 0x1e, 0xc1]),
    ("psignw xmm0, xmm1", &[0x66, 0x0f, 0x38, 0x09, 0xc1]),
    (
        "palignr xmm0, xmm1, 5",
        &[0x66, 0x0f, 0x3a, 0x0f, 0xc1, 0x05],
    ),
    ("phaddd xmm0, xmm1", &[0x66, 0x0f, 0x38, 0x02, 0xc1]),
    ("pmaddubsw xmm0, xmm1", &[0x66, 0x0f, 0x38, 0x04, 0xc1]),
    ("pmulhrsw xmm0, xmm1", &[0x66, 0x0f, 0x38, 0x0b, 0xc1]),
    ("ptest xmm0, xmm1", &[0x66, 0x0f, 0x38, 0x17, 0xc1]),
    ("pextrb eax, xmm1, 3", &[0x66, 0x0f, 0x3a, 0x14, 0xc8, 0x03]),
    ("pextrw eax, xmm1, 5", &[0x66, 0x0f, 0xc5, 0xc1, 0x05]),
    ("pextrd eax, xmm1, 2", &[0x66, 0x0f, 0x3a, 0x16, 0xc8, 0x02]),
    (
        "pextrq rax, xmm1, 1",
        &[0x66, 0x48, 0x0f, 0x3a, 0x16, 0xc8, 0x01],
    ),
    ("pinsrb xmm0, eax, 7", &[0x66, 0x0f, 0x3a, 0x20, 0xc0, 0x07]),
    ("pinsrw xmm0, eax, 3", &[0x66, 0x0f, 0xc4, 0xc0, 0x03]),
    ("pinsrd xmm0, eax, 1", &[0x66, 0x0f, 0x3a, 0x22, 0xc0, 0x01]),
    (
        "pinsrq xmm0, rax, 1",
        &[0x66, 0x48, 0x0f, 0x3a, 0x22, 0xc0, 0x01],
    ),
    ("pmovsxbw xmm0, xmm1", &[0x66, 0x0f, 0x38, 0x20, 0xc1]),
    ("pmovsxwd xmm0, xmm1", &[0x66, 0x0f, 0x38, 0x23, 0xc1]),
    ("pmovsxdq xmm0, xmm1", &[0x66, 0x0f, 0x38, 0x25, 0xc1]),
    ("pmovzxbw xmm0, xmm1", &[0x66, 0x0f, 0x38, 0x30, 0xc1]),
    ("pmovzxbd xmm0, xmm1", &[0x66, 0x0f, 0x38, 0x31, 0xc1]),
    ("pmovzxwq xmm0, xmm1", &[0x66, 0x0f, 0x38, 0x34, 0xc1]),
    (
        "blendps xmm0, xmm1, 5",
        &[0x66, 0x0f, 0x3a, 0x0c, 0xc1, 0x05],
    ),
    (
        "blendpd xmm0, xmm1, 2",
        &[0x66, 0x0f, 0x3a, 0x0d, 0xc1, 0x02],
    ),
    (
        "pblendw xmm0, xmm1, 0xa5",
        &[0x66, 0x0f, 0x3a, 0x0e, 0xc1, 0xa5],
    ),
    (
        "insertps xmm0, xmm1, 0x4c",
        &[0x66, 0x0f, 0x3a, 0x21, 0xc1, 0x4c],
    ),
    (
        "extractps eax, xmm1, 2",
        &[0x66, 0x0f, 0x3a, 0x17, 0xc8, 0x02],
    ),
    (
        "dpps xmm0, xmm1, 0xf1",
        &[0x66, 0x0f, 0x3a, 0x40, 0xc1, 0xf1],
    ),
    (
        "dppd xmm0, xmm1, 0x31",
        &[0x66, 0x0f, 0x3a, 0x41, 0xc1, 0x31],
    ),
    (
        "mpsadbw xmm0, xmm1, 5",
        &[0x66, 0x0f, 0x3a, 0x42, 0xc1, 0x05],
    ),
    ("phminposuw xmm0, xmm1", &[0x66, 0x0f, 0x38, 0x41, 0xc1]),
    ("movd eax, xmm1", &[0x66, 0x0f, 0x7e, 0xc8]),
    ("movd xmm0, eax", &[0x66, 0x0f, 0x6e, 0xc0]),
    ("movq rax, xmm1", &[0x66, 0x48, 0x0f, 0x7e, 0xc8]),
    ("movq xmm0, rax", &[0x66, 0x48, 0x0f, 0x6e, 0xc0]),
    ("movq xmm0, xmm1", &[0xf3, 0x0f, 0x7e, 0xc1]),
    ("movss xmm0, xmm1", &[0xf3, 0x0f, 0x10, 0xc1]),
    ("movsd xmm0, xmm1", &[0xf2, 0x0f, 0x10, 0xc1]),
    ("movss xmm0, dword ptr [rsi]", &[0xf3, 0x0f, 0x10, 0x06]),
    ("movsd qword ptr [rdi], xmm1", &[0xf2, 0x0f, 0x11, 0x0f]),
    ("movups xmm0, xmmword ptr [rsi]", &[0x0f, 0x10, 0x06]),
    ("movdqu xmmword ptr [rdi], xmm1", &[0xf3, 0x0f, 0x7f, 0x0f]),
    ("movaps xmm0, xmm1", &[0x0f, 0x28, 0xc1]),
    ("movhlps xmm0, xmm1", &[0x0f, 0x12, 0xc1]),
    ("movlhps xmm0, xmm1", &[0x0f, 0x16, 0xc1]),
    ("movshdup xmm0, xmm1", &[0xf3, 0x0f, 0x16, 0xc1]),
    ("movddup xmm0, xmm1", &[0xf2, 0x0f, 0x12, 0xc1]),
];

fn curated() -> Vec<Curated> {
    let mut out: Vec<Curated> = CURATED
        .iter()
        .map(|(name, bytes)| Curated {
            name: (*name).to_string(),
            bytes: bytes.to_vec(),
        })
        .collect();
    // ROUNDSS/SD/PS/PD xmm0, xmm1, imm8 with every rounding immediate:
    // bits 1:0 the mode, bit 2 "use MXCSR.RC", bit 3 "suppress precision
    // exception" (SDM 2, ROUNDSS, Table 4-18).
    for (name, op) in [
        ("roundps", 0x08u8),
        ("roundpd", 0x09),
        ("roundss", 0x0A),
        ("roundsd", 0x0B),
    ] {
        for imm in 0..16u8 {
            out.push(Curated {
                name: format!("{name} xmm0, xmm1, {imm}"),
                bytes: vec![0x66, 0x0F, 0x3A, op, 0xC1, imm],
            });
        }
    }
    // CMPccSS/SD/PS/PD with all eight SSE predicates.
    for (name, prefix) in [
        ("cmpps", None),
        ("cmppd", Some(0x66u8)),
        ("cmpss", Some(0xF3)),
        ("cmpsd", Some(0xF2)),
    ] {
        for imm in 0..8u8 {
            let mut bytes = prefix.map(|p| vec![p]).unwrap_or_default();
            bytes.extend_from_slice(&[0x0F, 0xC2, 0xC1, imm]);
            out.push(Curated {
                name: format!("{name} xmm0, xmm1, {imm}"),
                bytes,
            });
        }
    }
    out
}

/// String instructions, which a `rep` prefix turns into a loop on the
/// instruction itself (SDM 2, REP/REPE/REPZ/REPNE/REPNZ): one-byte opcodes
/// 6C-6F and A4-A7, AA-AF after any prefixes.
fn repeats(bytes: &[u8], _lifted: &Lifted) -> bool {
    let op = bytes.iter().copied().find(|b| {
        !matches!(
            b,
            0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65 | 0x66 | 0x67 | 0xF0 | 0xF2 | 0xF3 | 0x40
                ..=0x4F
        )
    });
    matches!(op, Some(0x6C..=0x6F | 0xA4..=0xA7 | 0xAA..=0xAF))
}

fn buffer_pointer(rng: &mut Rng, align: u64) -> u64 {
    let off = BUF_LEN as u64 / 4 + rng.below(BUF_LEN as u64 / 2);
    (buf_addr() + off) & !(align - 1)
}

fn state(rng: &mut Rng, curated: Option<&str>) -> CpuState {
    let mut s = CpuState::zeroed();
    for i in 0..16 {
        s.gpr[i] = if rng.chance(30) {
            buffer_pointer(rng, 1)
        } else {
            int_value(rng)
        };
    }
    s.gpr[RSP] = buffer_pointer(rng, 16);
    for f in 0..6 {
        s.flags[f] = rng.below(2) as u8;
    }
    // DF: string instructions in either direction.
    s.flags[6] = u8::from(rng.chance(25));
    for v in 0..16 {
        s.vec[v] = vec_value(rng);
    }
    rng.fill(&mut s.buf);
    if let Some(name) = curated {
        s.gpr[RSI] = buffer_pointer(rng, 1);
        s.gpr[RDI] = buffer_pointer(rng, 1);
        if name.contains("rep") || name.starts_with("leave") {
            s.gpr[RCX] = rng.below(40);
            s.gpr[5] = buffer_pointer(rng, 8); // RBP for LEAVE
        }
        if name.starts_with("div ") || name.starts_with("idiv ") {
            fix_divide(name, &mut s, rng);
        }
    }
    s
}

/// Keep a curated DIV/IDIV inside its defined range, so the case compares
/// values instead of both sides raising #DE (SDM 2, DIV/IDIV "Operation").
fn fix_divide(name: &str, s: &mut CpuState, rng: &mut Rng) {
    let width = if name.ends_with("cl") {
        8
    } else if name.ends_with("cx") && !name.ends_with("rcx") && !name.ends_with("ecx") {
        16
    } else if name.ends_with("ecx") {
        32
    } else {
        64
    };
    let mask = if width == 64 {
        u64::MAX
    } else {
        (1u64 << width) - 1
    };
    let mut divisor = s.gpr[RCX] & mask;
    if divisor == 0 {
        divisor = 1 + rng.below(mask.min(1000));
    }
    let signed = name.starts_with("idiv");
    if signed {
        let sign_bit = 1u64 << (width - 1);
        if divisor == mask {
            divisor = 3; // -1 would overflow on the minimum dividend
        }
        let low = s.gpr[RAX] & mask;
        let ext = if low & sign_bit != 0 { mask } else { 0 };
        match width {
            8 => {
                let ax = (ext << 8 | low) & 0xFFFF;
                s.gpr[RAX] = (s.gpr[RAX] & !0xFFFF) | ax;
            }
            _ => s.gpr[RDX] = (s.gpr[RDX] & !mask) | ext,
        }
    } else {
        match width {
            8 => {
                let ah = ((s.gpr[RAX] >> 8) & 0xFF) % divisor;
                s.gpr[RAX] = (s.gpr[RAX] & !0xFF00) | (ah << 8);
            }
            _ => {
                let high = (s.gpr[RDX] & mask) % divisor;
                s.gpr[RDX] = (s.gpr[RDX] & !mask) | high;
            }
        }
    }
    s.gpr[RCX] = (s.gpr[RCX] & !mask) | divisor;
}

/// A structured random instruction: optional prefixes, an opcode from one of
/// the four legacy maps, a ModRM biased toward register operands, and enough
/// trailing bytes for any SIB, displacement and immediate.
fn random_insn(rng: &mut Rng) -> Vec<u8> {
    let mut b = Vec::new();
    if rng.chance(30) {
        b.push(rng.pick(&[0x66u8, 0xF2, 0xF3]));
    }
    if rng.chance(50) {
        b.push(0x40 | rng.below(16) as u8);
    }
    match rng.below(10) {
        0..=3 => b.push(rng.below(256) as u8),
        4..=7 => b.extend_from_slice(&[0x0F, rng.below(256) as u8]),
        8 => b.extend_from_slice(&[0x0F, 0x38, rng.below(256) as u8]),
        _ => b.extend_from_slice(&[0x0F, 0x3A, rng.below(256) as u8]),
    }
    let modrm = if rng.chance(55) {
        0xC0 | rng.below(64) as u8
    } else {
        rng.below(192) as u8
    };
    b.push(modrm);
    while b.len() < 15 {
        b.push(rng.below(256) as u8);
    }
    b
}

/// Instructions that cannot be compared, by mnemonic: they read host state
/// Fission cannot share (time, identity, randomness, TLS bases, descriptor
/// tables), change process state the harness depends on (FS/GS bases, TF/AC
/// via POPF, the shadow stack, x87 or extended state), or wait.
const DENIED: &[&str] = &[
    "CPUID",
    "RDTSC",
    "RDTSCP",
    "RDRAND",
    "RDSEED",
    "RDPID",
    "RDPKRU",
    "WRPKRU",
    "XGETBV",
    "XSETBV",
    "RDFSBASE",
    "RDGSBASE",
    "WRFSBASE",
    "WRGSBASE",
    "POPF",
    "POPFQ",
    "POPFW",
    "IRET",
    "IRETD",
    "IRETQ",
    "LSL",
    "LAR",
    "VERR",
    "VERW",
    "SLDT",
    "SGDT",
    "SIDT",
    "STR",
    "SMSW",
    "XBEGIN",
    "XEND",
    "XABORT",
    "XTEST",
    "UMONITOR",
    "UMWAIT",
    "TPAUSE",
    "MONITOR",
    "MWAIT",
    "ENQCMD",
    "ENQCMDS",
    "MOVDIR64B",
    "INCSSPD",
    "INCSSPQ",
    "RDSSPD",
    "RDSSPQ",
    "SAVEPREVSSP",
    "RSTORSSP",
    "WRSSD",
    "WRSSQ",
    "SETSSBSY",
    "CLRSSBSY",
    "LDMXCSR",
    "FXRSTOR",
    "FXRSTOR64",
    "FXSAVE",
    "FXSAVE64",
    "XSAVE",
    "XSAVEC",
    "XSAVEOPT",
    "XSAVES",
    "XRSTOR",
    "XRSTORS",
    "XSAVE64",
    "XRSTOR64",
    "SYSCALL",
    "SYSENTER",
    "SYSEXIT",
    "SYSRET",
    "INT",
    "INT1",
    "INT3",
    "INTO",
    "HLT",
    "SWAPGS",
    "ENCLU",
    "SERIALIZE",
    "HRESET",
    "UIRET",
    "TESTUI",
    "CLUI",
    "STUI",
    "SENDUIPI",
    // AMD SSE4a: absent on Intel hosts, and AMD64 APM vol. 4 leaves bits
    // 127:64 of the destination undefined.
    "EXTRQ",
    "INSERTQ",
];

fn skip(bytes: &[u8], lifted: &Lifted, fission: &FissionMachine) -> Option<String> {
    // Legacy prefixes, then REX, then the opcode.
    let mut i = 0;
    while i < bytes.len()
        && matches!(
            bytes[i],
            0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65 | 0x66 | 0x67 | 0xF0 | 0xF2 | 0xF3
        )
    {
        if matches!(bytes[i], 0x64 | 0x65) {
            return Some("fs/gs segment".into());
        }
        i += 1;
    }
    while i < bytes.len() && (0x40..=0x4F).contains(&bytes[i]) {
        i += 1;
    }
    match bytes.get(i) {
        // VEX and EVEX touch YMM/ZMM state the harness does not carry.
        Some(0xC4 | 0xC5 | 0x62) => return Some("vex/evex".into()),
        Some(0x8F) if bytes.get(i + 1).is_some_and(|m| (m >> 3) & 7 != 0) => {
            return Some("xop".into());
        }
        // 3DNow!
        Some(0x0F) if bytes.get(i + 1) == Some(&0x0F) => return Some("3dnow".into()),
        _ => {}
    }
    let base = lifted
        .mnemonic
        .split_whitespace()
        .last()
        .unwrap_or("")
        .trim_end_matches(".LOCK");
    if DENIED.contains(&base) {
        return Some(format!("denied:{base}"));
    }
    let rs = fission.register_space;
    // x87 and MMX state (ST0-7, the FPU control words, MM0-7).
    if touches_registers(&lifted.ops, rs, 0x1000, 0x1094, false)
        || touches_registers(&lifted.ops, rs, 0x1098, 0x1180, false)
    {
        return Some("x87/mmx".into());
    }
    // FS/GS selectors and bases, and any segment register write.
    if touches_registers(&lifted.ops, rs, 0x108, 0x10C, false)
        || touches_registers(&lifted.ops, rs, 0x110, 0x120, false)
        || touches_registers(&lifted.ops, rs, 0x100, 0x10C, true)
    {
        return Some("segment".into());
    }
    // Upper YMM/ZMM halves and AVX-512 mask registers.
    for n in 0..32u64 {
        let slot = 0x1200 + 64 * n;
        if touches_registers(&lifted.ops, rs, slot + 16, slot + 64, false) {
            return Some("ymm/zmm".into());
        }
    }
    // Control, debug, bound and shadow-stack registers.
    if touches_registers(&lifted.ops, rs, 0x300, 0x800, false) {
        return Some("system register".into());
    }
    None
}

// ── Operand parsing for the masks ───────────────────────────────────────────

/// (GPR index, width in bits) of a register as Ghidra prints it.
fn gpr_of(name: &str) -> Option<(usize, u32)> {
    const R64: [&str; 16] = [
        "RAX", "RCX", "RDX", "RBX", "RSP", "RBP", "RSI", "RDI", "R8", "R9", "R10", "R11", "R12",
        "R13", "R14", "R15",
    ];
    const R32: [&str; 16] = [
        "EAX", "ECX", "EDX", "EBX", "ESP", "EBP", "ESI", "EDI", "R8D", "R9D", "R10D", "R11D",
        "R12D", "R13D", "R14D", "R15D",
    ];
    const R16: [&str; 16] = [
        "AX", "CX", "DX", "BX", "SP", "BP", "SI", "DI", "R8W", "R9W", "R10W", "R11W", "R12W",
        "R13W", "R14W", "R15W",
    ];
    const R8: [&str; 16] = [
        "AL", "CL", "DL", "BL", "SPL", "BPL", "SIL", "DIL", "R8B", "R9B", "R10B", "R11B", "R12B",
        "R13B", "R14B", "R15B",
    ];
    let n = name.trim().to_ascii_uppercase();
    for (table, width) in [(&R64, 64), (&R32, 32), (&R16, 16), (&R8, 8)] {
        if let Some(i) = table.iter().position(|r| *r == n) {
            return Some((i, width));
        }
    }
    match n.as_str() {
        "AH" => Some((0, 8)),
        "CH" => Some((1, 8)),
        "DH" => Some((2, 8)),
        "BH" => Some((3, 8)),
        _ => None,
    }
}

fn operands(lifted: &Lifted) -> Vec<String> {
    let text = lifted.text.trim();
    let rest = text
        .split_once(char::is_whitespace)
        .map(|(_, r)| r)
        .unwrap_or("");
    // A mnemonic with a prefix ("LOCK XADD") leaves the real mnemonic first.
    rest.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Width of the destination operand, from a register name or a size keyword.
fn dest_width(ops: &[String]) -> u32 {
    let Some(first) = ops.first() else { return 64 };
    if let Some((_, w)) = gpr_of(first) {
        return w;
    }
    let u = first.to_ascii_uppercase();
    if u.starts_with("BYTE") {
        8
    } else if u.starts_with("WORD") {
        16
    } else if u.starts_with("DWORD") {
        32
    } else {
        64
    }
}

/// A source operand's value: a register read from the input, or an immediate.
fn operand_value(op: &str, input: &CpuState) -> Option<u64> {
    let op = op.trim();
    if let Some((i, w)) = gpr_of(op) {
        let v = input.gpr[i];
        let high = matches!(op.to_ascii_uppercase().as_str(), "AH" | "CH" | "DH" | "BH");
        let v = if high { v >> 8 } else { v };
        return Some(if w == 64 { v } else { v & ((1u64 << w) - 1) });
    }
    let t = op.trim_start_matches('#');
    if let Some(h) = t.strip_prefix("0x") {
        return u64::from_str_radix(h, 16).ok();
    }
    t.parse::<u64>().ok()
}

fn mask(bytes: &[u8], lifted: &Lifted, input: &CpuState) -> Mask {
    let mn = lifted
        .mnemonic
        .split_whitespace()
        .last()
        .unwrap_or("")
        .to_string();
    let ops = operands(lifted);
    let width = dest_width(&ops);
    let count = || -> Option<u64> {
        let raw = match ops.get(ops.len().saturating_sub(1)) {
            Some(last) if ops.len() >= 2 => operand_value(last, input)?,
            _ => 1,
        };
        Some(raw & if width == 64 { 63 } else { 31 })
    };
    let flags = |f: u8| Mask {
        flags: f,
        ..Mask::default()
    };
    match mn.as_str() {
        // SDM 2, MUL / IMUL, Flags Affected: "The SF, ZF, AF, and PF flags are undefined."
        "MUL" | "IMUL" => flags(SF | ZF | AF | PF),
        // SDM 2, DIV / IDIV, Flags Affected: "The CF, OF, SF, ZF, AF, and PF flags are undefined."
        "DIV" | "IDIV" => flags(ALL_STATUS),
        // SDM 2, AND / OR / XOR / TEST, Flags Affected: "The state of the AF flag is undefined."
        "AND" | "OR" | "XOR" | "TEST" => flags(AF),
        // SDM 2, SAL/SAR/SHL/SHR, Flags Affected: with a count of 0 the flags
        // are not affected; otherwise AF is undefined, OF is undefined unless
        // the count is 1, and CF is undefined for SHL and SHR when the count
        // is at least the operand width.
        "SHL" | "SAL" | "SHR" | "SAR" => match count() {
            Some(0) => Mask::default(),
            Some(1) => flags(AF),
            Some(c) => {
                let cf = if mn != "SAR" && c >= u64::from(width) {
                    CF
                } else {
                    0
                };
                flags(AF | OF | cf)
            }
            None => flags(AF | OF | CF),
        },
        // SDM 2, RCL/RCR/ROL/ROR, Flags Affected: "The OF flag is defined only
        // for the 1-bit rotates; it is undefined in all other cases." A
        // masked count of 0 affects no flags.
        "ROL" | "ROR" | "RCL" | "RCR" => match count() {
            Some(0) | Some(1) => Mask::default(),
            _ => flags(OF),
        },
        // SDM 2, SHLD / SHRD, Flags Affected: count 0 affects nothing; AF is
        // undefined; OF is undefined unless the count is 1; a count greater
        // than the operand size leaves the flags and the destination
        // undefined (Description).
        "SHLD" | "SHRD" => match count() {
            Some(0) => Mask::default(),
            Some(1) => flags(AF),
            Some(c) if c > u64::from(width) => {
                let dest = ops.first().and_then(|o| gpr_of(o));
                Mask {
                    flags: ALL_STATUS,
                    gprs: dest.map_or(0, |(i, _)| 1 << i),
                    mem: dest.is_none(),
                }
            }
            _ => flags(AF | OF),
        },
        // SDM 2, BSF / BSR, Flags Affected: "The CF, OF, SF, AF, and PF flags
        // are undefined." Description: "If the content of the source operand
        // is 0, the content of the destination operand is undefined."
        "BSF" | "BSR" => {
            let src_zero = ops
                .get(1)
                .and_then(|o| operand_value(o, input))
                .map(|v| v == 0);
            let dest = ops.first().and_then(|o| gpr_of(o)).map(|(i, _)| 1u32 << i);
            Mask {
                flags: CF | OF | SF | AF | PF,
                gprs: match (src_zero, dest) {
                    (Some(false), _) => 0,
                    (_, Some(d)) => d,
                    _ => u32::MAX,
                },
                mem: false,
            }
        }
        // SDM 2, BSWAP, Description: "When the BSWAP instruction references
        // a 16-bit register, the result is undefined." (operand-size prefix)
        "BSWAP" if has_prefix(bytes, 0x66) => Mask {
            gprs: ops
                .first()
                .and_then(|o| gpr_of(o))
                .map_or(u32::MAX, |(i, _)| 1 << i),
            ..Mask::default()
        },
        // SDM 2, TZCNT / LZCNT, Flags Affected: "The OF, SF, PF and AF flags are undefined."
        "TZCNT" | "LZCNT" => flags(OF | SF | PF | AF),
        // SDM 2, BT / BTS / BTR / BTC, Flags Affected: "The OF, SF, AF, and PF
        // flags are undefined." (ZF is unaffected.)
        "BT" | "BTS" | "BTR" | "BTC" => flags(OF | SF | AF | PF),
        _ => Mask::default(),
    }
}

/// Whether `prefix` occurs among the legacy prefixes before the opcode.
fn has_prefix(bytes: &[u8], prefix: u8) -> bool {
    bytes
        .iter()
        .take_while(|b| {
            matches!(
                b,
                0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65 | 0x66 | 0x67 | 0xF0 | 0xF2 | 0xF3
            )
        })
        .any(|b| *b == prefix)
}

/// Host segment selectors, so `mov eax, cs` reads the same on both sides.
#[cfg(target_arch = "x86_64")]
fn extra() -> Vec<(&'static str, u64)> {
    let (cs, ss, ds, es): (u16, u16, u16, u16);
    unsafe {
        std::arch::asm!("mov {0:x}, cs", out(reg) cs);
        std::arch::asm!("mov {0:x}, ss", out(reg) ss);
        std::arch::asm!("mov {0:x}, ds", out(reg) ds);
        std::arch::asm!("mov {0:x}, es", out(reg) es);
    }
    vec![
        // IF is always set in user mode; PUSHF stores it.
        ("IF", 1),
        ("CS", u64::from(cs)),
        ("SS", u64::from(ss)),
        ("DS", u64::from(ds)),
        ("ES", u64::from(es)),
    ]
}

#[cfg(not(target_arch = "x86_64"))]
fn extra() -> Vec<(&'static str, u64)> {
    vec![("IF", 1)]
}
