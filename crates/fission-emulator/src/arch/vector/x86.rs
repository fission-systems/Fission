//! x86 / x86-64 SSE, SSSE3, and SSE4.1 `CALLOTHER` vector userops.

use fission_pcode_ir::ir::{PcodeOp, Varnode};

use super::{
    read_bytes, read_const_u64, read_elem_u64, read_u64_le, sign_extend_esize, write_bytes_exact,
    write_elem_u64,
};
use crate::core::Emulator;

const X86_QNAN32: u32 = 0xFFC0_0000;
const X86_QNAN64: u64 = 0xFFF8_0000_0000_0000;

pub(super) fn is_known(name: &str) -> bool {
    matches!(
        name,
        "roundss"
            | "roundsd"
            | "roundps"
            | "roundpd"
            | "pshufb"
            | "pshuflw"
            | "pshufhw"
            | "packsswb"
            | "packssdw"
            | "packusdw"
            | "packuswb"
            | "paddsb"
            | "paddsw"
            | "paddusb"
            | "paddusw"
            | "psubsb"
            | "psubsw"
            | "psubusb"
            | "psubusw"
            | "pmulhw"
            | "pmulhuw"
            | "pmulhrsw"
            | "pmulld"
            | "pmuldq"
            | "pmaddwd"
            | "pmaddubsw"
            | "psadbw"
            | "mpsadbw"
            | "phaddw"
            | "phaddd"
            | "phaddsw"
            | "phsubw"
            | "phsubd"
            | "phsubsw"
            | "psignb"
            | "psignw"
            | "psignd"
            | "pabsb"
            | "pabsw"
            | "pabsd"
            | "psllw"
            | "psraw"
            | "pmovsxbw"
            | "pmovsxbd"
            | "pmovsxbq"
            | "pmovsxwd"
            | "pmovsxwq"
            | "pmovsxdq"
            | "pmovzxbw"
            | "pmovzxbd"
            | "pmovzxbq"
            | "pmovzxwd"
            | "pmovzxwq"
            | "pmovzxdq"
            | "pblendw"
            | "blendps"
            | "blendpd"
            | "dpps"
            | "dppd"
            | "extractps"
            | "insertps"
            | "phminposuw"
            | "movmskps"
            | "movmskpd"
            | "divps"
            | "divpd"
            | "sqrtps"
            | "sqrtpd"
            | "minps"
            | "maxps"
            | "minpd"
            | "maxpd"
            | "rcpps"
            | "rcpss"
            | "rsqrtps"
            | "rsqrtss"
            | "pavgb"
            | "pavgw"
    )
}

pub(super) fn answer(emu: &mut Emulator, name: &str, op: &PcodeOp, out: &Varnode) -> bool {
    match name {
        "roundss" => {
            let (Ok(dst), Ok(src), Some(imm)) = (
                read_bytes(emu, op, 1),
                read_bytes(emu, op, 2),
                read_const_u64(op, 3),
            ) else {
                return false;
            };
            let mode = resolve_round_mode(emu, imm);
            round_ss(&dst, &src, mode).is_some_and(|res| write_bytes_exact(emu, out, &res))
        }
        "roundsd" => {
            let (Ok(dst), Ok(src), Some(imm)) = (
                read_bytes(emu, op, 1),
                read_bytes(emu, op, 2),
                read_const_u64(op, 3),
            ) else {
                return false;
            };
            let mode = resolve_round_mode(emu, imm);
            round_sd(&dst, &src, mode).is_some_and(|res| write_bytes_exact(emu, out, &res))
        }
        "roundps" => {
            let (Ok(src), Some(imm)) = (read_bytes(emu, op, 2), read_const_u64(op, 3)) else {
                return false;
            };
            let mode = resolve_round_mode(emu, imm);
            round_ps(&src, mode).is_some_and(|res| write_bytes_exact(emu, out, &res))
        }
        "roundpd" => {
            let (Ok(src), Some(imm)) = (read_bytes(emu, op, 2), read_const_u64(op, 3)) else {
                return false;
            };
            let mode = resolve_round_mode(emu, imm);
            round_pd(&src, mode).is_some_and(|res| write_bytes_exact(emu, out, &res))
        }
        "pshufb" => {
            let (Ok(dst), Ok(mask)) = (read_bytes(emu, op, 1), read_bytes(emu, op, 2)) else {
                return false;
            };
            pshufb(&dst, &mask).is_some_and(|res| write_bytes_exact(emu, out, &res))
        }
        "pshuflw" => {
            let (Ok(src), Some(imm)) = (read_bytes(emu, op, 2), read_const_u64(op, 3)) else {
                return false;
            };
            pshuf_words(&src, imm as u8, false).is_some_and(|res| write_bytes_exact(emu, out, &res))
        }
        "pshufhw" => {
            let (Ok(src), Some(imm)) = (read_bytes(emu, op, 2), read_const_u64(op, 3)) else {
                return false;
            };
            pshuf_words(&src, imm as u8, true).is_some_and(|res| write_bytes_exact(emu, out, &res))
        }
        "packsswb" => {
            let (Ok(dst), Ok(src)) = (read_bytes(emu, op, 1), read_bytes(emu, op, 2)) else {
                return false;
            };
            pack(&dst, &src, PackKind::SignedWordToByte)
                .is_some_and(|res| write_bytes_exact(emu, out, &res))
        }
        "packssdw" => {
            let (Ok(dst), Ok(src)) = (read_bytes(emu, op, 1), read_bytes(emu, op, 2)) else {
                return false;
            };
            pack(&dst, &src, PackKind::SignedDwordToWord)
                .is_some_and(|res| write_bytes_exact(emu, out, &res))
        }
        "packusdw" => {
            let (Ok(dst), Ok(src)) = (read_bytes(emu, op, 1), read_bytes(emu, op, 2)) else {
                return false;
            };
            pack(&dst, &src, PackKind::UnsignedDwordToWord)
                .is_some_and(|res| write_bytes_exact(emu, out, &res))
        }
        "packuswb" => {
            let (Ok(dst), Ok(src)) = (read_bytes(emu, op, 1), read_bytes(emu, op, 2)) else {
                return false;
            };
            pack(&dst, &src, PackKind::UnsignedWordToByte)
                .is_some_and(|res| write_bytes_exact(emu, out, &res))
        }
        "paddsb" | "paddsw" | "paddusb" | "paddusw" | "psubsb" | "psubsw" | "psubusb"
        | "psubusw" => {
            let (Ok(dst), Ok(src)) = (read_bytes(emu, op, 1), read_bytes(emu, op, 2)) else {
                return false;
            };
            sat_add_sub(&dst, &src, name).is_some_and(|res| write_bytes_exact(emu, out, &res))
        }
        "pmulhw" => bin_vec(emu, op, out, |a, b| {
            pmul_words(a, b, MulWordKind::SignedHigh)
        }),
        "pmulhuw" => bin_vec(emu, op, out, |a, b| {
            pmul_words(a, b, MulWordKind::UnsignedHigh)
        }),
        "pmulhrsw" => bin_vec(emu, op, out, |a, b| {
            pmul_words(a, b, MulWordKind::SignedRoundScale)
        }),
        "pmulld" => bin_vec(emu, op, out, pmulld),
        "pmuldq" => bin_vec(emu, op, out, pmuldq),
        "pmaddwd" => bin_vec(emu, op, out, pmaddwd),
        "pmaddubsw" => bin_vec(emu, op, out, pmaddubsw),
        "psadbw" => bin_vec(emu, op, out, psadbw),
        "mpsadbw" => {
            let (Ok(dst), Ok(src), Some(imm)) = (
                read_bytes(emu, op, 1),
                read_bytes(emu, op, 2),
                read_const_u64(op, 3),
            ) else {
                return false;
            };
            mpsadbw(&dst, &src, imm as u8).is_some_and(|res| write_bytes_exact(emu, out, &res))
        }
        "phaddw" | "phaddd" | "phaddsw" | "phsubw" | "phsubd" | "phsubsw" => {
            let (Ok(dst), Ok(src)) = (read_bytes(emu, op, 1), read_bytes(emu, op, 2)) else {
                return false;
            };
            horiz_add_sub(&dst, &src, name).is_some_and(|res| write_bytes_exact(emu, out, &res))
        }
        "psignb" | "psignw" | "psignd" => {
            let (Ok(dst), Ok(src)) = (read_bytes(emu, op, 1), read_bytes(emu, op, 2)) else {
                return false;
            };
            let esize = match name {
                "psignb" => 1,
                "psignw" => 2,
                _ => 4,
            };
            psign(&dst, &src, esize).is_some_and(|res| write_bytes_exact(emu, out, &res))
        }
        "pabsb" | "pabsw" | "pabsd" => {
            let Ok(src) = read_bytes(emu, op, 2) else {
                return false;
            };
            let esize = match name {
                "pabsb" => 1,
                "pabsw" => 2,
                _ => 4,
            };
            pabs(&src, esize).is_some_and(|res| write_bytes_exact(emu, out, &res))
        }
        "psllw" | "psraw" => {
            let (Ok(dst), Ok(count)) = (read_bytes(emu, op, 1), read_bytes(emu, op, 2)) else {
                return false;
            };
            pshift_w(&dst, read_u64_le(&count), name == "psraw")
                .is_some_and(|res| write_bytes_exact(emu, out, &res))
        }
        "pmovsxbw" | "pmovsxbd" | "pmovsxbq" | "pmovsxwd" | "pmovsxwq" | "pmovsxdq"
        | "pmovzxbw" | "pmovzxbd" | "pmovzxbq" | "pmovzxwd" | "pmovzxwq" | "pmovzxdq" => {
            let Ok(src) = read_bytes(emu, op, 2) else {
                return false;
            };
            pmov_ext(&src, name, out.size as usize)
                .is_some_and(|res| write_bytes_exact(emu, out, &res))
        }
        "pblendw" => blend(emu, op, out, 2),
        "blendps" => blend(emu, op, out, 4),
        "blendpd" => blend(emu, op, out, 8),
        "dpps" => {
            let (Ok(dst), Ok(src), Some(imm)) = (
                read_bytes(emu, op, 1),
                read_bytes(emu, op, 2),
                read_const_u64(op, 3),
            ) else {
                return false;
            };
            dpps(&dst, &src, imm as u8).is_some_and(|res| write_bytes_exact(emu, out, &res))
        }
        "dppd" => {
            let (Ok(dst), Ok(src), Some(imm)) = (
                read_bytes(emu, op, 1),
                read_bytes(emu, op, 2),
                read_const_u64(op, 3),
            ) else {
                return false;
            };
            dppd(&dst, &src, imm as u8).is_some_and(|res| write_bytes_exact(emu, out, &res))
        }
        "extractps" => {
            let (Ok(src), Some(imm)) = (read_bytes(emu, op, 1), read_const_u64(op, 2)) else {
                return false;
            };
            extractps(&src, imm as u8, out.size as usize)
                .is_some_and(|res| write_bytes_exact(emu, out, &res))
        }
        "insertps" => {
            let (Ok(dst), Ok(src), Some(imm)) = (
                read_bytes(emu, op, 1),
                read_bytes(emu, op, 2),
                read_const_u64(op, 3),
            ) else {
                return false;
            };
            insertps(&dst, &src, imm as u8).is_some_and(|res| write_bytes_exact(emu, out, &res))
        }
        "phminposuw" => {
            let Ok(src) = read_bytes(emu, op, 1) else {
                return false;
            };
            phminposuw(&src).is_some_and(|res| write_bytes_exact(emu, out, &res))
        }
        "movmskps" => {
            let Ok(src) = read_bytes(emu, op, 2) else {
                return false;
            };
            movmsk(&src, 4, out.size as usize).is_some_and(|res| write_bytes_exact(emu, out, &res))
        }
        "movmskpd" => {
            let Ok(src) = read_bytes(emu, op, 2) else {
                return false;
            };
            movmsk(&src, 8, out.size as usize).is_some_and(|res| write_bytes_exact(emu, out, &res))
        }
        "divps" => bin_vec(emu, op, out, |a, b| fp_binop_f32(a, b, f32_div)),
        "divpd" => bin_vec(emu, op, out, |a, b| fp_binop_f64(a, b, f64_div)),
        "sqrtps" => {
            let Ok(src) = read_bytes(emu, op, 2) else {
                return false;
            };
            fp_unop_f32(&src, f32_sqrt).is_some_and(|res| write_bytes_exact(emu, out, &res))
        }
        "sqrtpd" => {
            let Ok(src) = read_bytes(emu, op, 2) else {
                return false;
            };
            fp_unop_f64(&src, f64_sqrt).is_some_and(|res| write_bytes_exact(emu, out, &res))
        }
        "minps" => bin_vec(emu, op, out, |a, b| fp_binop_f32(a, b, f32_min)),
        "maxps" => bin_vec(emu, op, out, |a, b| fp_binop_f32(a, b, f32_max)),
        "minpd" => bin_vec(emu, op, out, |a, b| fp_binop_f64(a, b, f64_min)),
        "maxpd" => bin_vec(emu, op, out, |a, b| fp_binop_f64(a, b, f64_max)),
        "rcpps" => {
            let Ok(src) = read_bytes(emu, op, 2) else {
                return false;
            };
            fp_unop_f32(&src, f32_rcp).is_some_and(|res| write_bytes_exact(emu, out, &res))
        }
        "rsqrtps" => {
            let Ok(src) = read_bytes(emu, op, 2) else {
                return false;
            };
            fp_unop_f32(&src, f32_rsqrt).is_some_and(|res| write_bytes_exact(emu, out, &res))
        }
        "rcpss" => {
            let (Ok(dst), Ok(src)) = (read_bytes(emu, op, 1), read_bytes(emu, op, 2)) else {
                return false;
            };
            scalar_f32_unop(&dst, &src, f32_rcp)
                .is_some_and(|res| write_bytes_exact(emu, out, &res))
        }
        "rsqrtss" => {
            let (Ok(dst), Ok(src)) = (read_bytes(emu, op, 1), read_bytes(emu, op, 2)) else {
                return false;
            };
            scalar_f32_unop(&dst, &src, f32_rsqrt)
                .is_some_and(|res| write_bytes_exact(emu, out, &res))
        }
        "pavgb" | "pavgw" => {
            let (Ok(a), Ok(b)) = (read_bytes(emu, op, 1), read_bytes(emu, op, 2)) else {
                return false;
            };
            pavg(&a, &b).is_some_and(|res| write_bytes_exact(emu, out, &res))
        }
        _ => false,
    }
}

fn bin_vec(
    emu: &mut Emulator,
    op: &PcodeOp,
    out: &Varnode,
    f: impl FnOnce(&[u8], &[u8]) -> Option<Vec<u8>>,
) -> bool {
    let (Ok(dst), Ok(src)) = (read_bytes(emu, op, 1), read_bytes(emu, op, 2)) else {
        return false;
    };
    f(&dst, &src).is_some_and(|res| write_bytes_exact(emu, out, &res))
}

fn blend(emu: &mut Emulator, op: &PcodeOp, out: &Varnode, esize: usize) -> bool {
    let (Ok(dst), Ok(src), Some(imm)) = (
        read_bytes(emu, op, 1),
        read_bytes(emu, op, 2),
        read_const_u64(op, 3),
    ) else {
        return false;
    };
    if dst.len() != src.len() || dst.is_empty() || dst.len() % esize != 0 {
        return false;
    }
    let lanes = dst.len() / esize;
    let mut out_bytes = dst.clone();
    for i in 0..lanes {
        if ((imm >> i) & 1) != 0 {
            out_bytes[i * esize..(i + 1) * esize].copy_from_slice(&src[i * esize..(i + 1) * esize]);
        }
    }
    write_bytes_exact(emu, out, &out_bytes)
}

fn resolve_round_mode(emu: &mut Emulator, imm: u64) -> u8 {
    if (imm & 4) == 0 {
        (imm & 3) as u8
    } else {
        let mxcsr = emu.read_register_u64("MXCSR").unwrap_or(0x1F80);
        ((mxcsr >> 13) & 3) as u8
    }
}

fn round_f32_bits(bits: u32, mode: u8) -> u32 {
    if (bits & 0x7FFF_FFFF) > 0x7F80_0000 {
        return bits | 0x0040_0000;
    }
    let v = f32::from_bits(bits);
    if !v.is_finite() || v == 0.0 {
        return bits;
    }
    let rounded = match mode & 3 {
        0 => v.round_ties_even(),
        1 => v.floor(),
        2 => v.ceil(),
        _ => v.trunc(),
    };
    rounded.to_bits()
}

fn round_f64_bits(bits: u64, mode: u8) -> u64 {
    if (bits & 0x7FFF_FFFF_FFFF_FFFF) > 0x7FF0_0000_0000_0000 {
        return bits | 0x0008_0000_0000_0000;
    }
    let v = f64::from_bits(bits);
    if !v.is_finite() || v == 0.0 {
        return bits;
    }
    let rounded = match mode & 3 {
        0 => v.round_ties_even(),
        1 => v.floor(),
        2 => v.ceil(),
        _ => v.trunc(),
    };
    rounded.to_bits()
}

fn round_ss(dst: &[u8], src: &[u8], mode: u8) -> Option<Vec<u8>> {
    if dst.len() < 4 || src.len() < 4 {
        return None;
    }
    let mut out = dst.to_vec();
    let val = u32::from_le_bytes(src[..4].try_into().ok()?);
    out[..4].copy_from_slice(&round_f32_bits(val, mode).to_le_bytes());
    Some(out)
}

fn round_sd(dst: &[u8], src: &[u8], mode: u8) -> Option<Vec<u8>> {
    if dst.len() < 8 || src.len() < 8 {
        return None;
    }
    let mut out = dst.to_vec();
    let val = u64::from_le_bytes(src[..8].try_into().ok()?);
    out[..8].copy_from_slice(&round_f64_bits(val, mode).to_le_bytes());
    Some(out)
}

fn round_ps(src: &[u8], mode: u8) -> Option<Vec<u8>> {
    fp_unop_f32(src, |bits| round_f32_bits(bits, mode))
}

fn round_pd(src: &[u8], mode: u8) -> Option<Vec<u8>> {
    fp_unop_f64(src, |bits| round_f64_bits(bits, mode))
}

fn pshufb(dst: &[u8], mask: &[u8]) -> Option<Vec<u8>> {
    if dst.len() != mask.len() || (dst.len() != 8 && dst.len() != 16) {
        return None;
    }
    let lane_mask = dst.len() - 1;
    let out = mask
        .iter()
        .map(|&m| {
            if (m & 0x80) != 0 {
                0
            } else {
                dst[(m as usize) & lane_mask]
            }
        })
        .collect();
    Some(out)
}

fn pshuf_words(src: &[u8], imm: u8, high: bool) -> Option<Vec<u8>> {
    if src.len() != 16 {
        return None;
    }
    let mut out = src.to_vec();
    let base = if high { 8 } else { 0 };
    for i in 0..4 {
        let sel = ((imm >> (i * 2)) & 3) as usize;
        let dst_off = base + i * 2;
        let src_off = base + sel * 2;
        out[dst_off..dst_off + 2].copy_from_slice(&src[src_off..src_off + 2]);
    }
    Some(out)
}

#[derive(Clone, Copy)]
enum PackKind {
    SignedWordToByte,
    SignedDwordToWord,
    UnsignedDwordToWord,
    UnsignedWordToByte,
}

fn pack(dst: &[u8], src: &[u8], kind: PackKind) -> Option<Vec<u8>> {
    if dst.len() != src.len() || (dst.len() != 8 && dst.len() != 16) {
        return None;
    }
    let src_esize = match kind {
        PackKind::SignedWordToByte | PackKind::UnsignedWordToByte => 2,
        PackKind::SignedDwordToWord | PackKind::UnsignedDwordToWord => 4,
    };
    let dst_esize = src_esize / 2;
    let mut out = vec![0u8; dst.len()];
    for (half_idx, half) in [dst, src].into_iter().enumerate() {
        let count = half.len() / src_esize;
        for i in 0..count {
            let out_idx = half_idx * count + i;
            let raw = read_elem_u64(half, i, src_esize);
            let packed: u64 = match kind {
                PackKind::SignedWordToByte => ((raw as i16).clamp(-128, 127) as i8) as u64,
                PackKind::UnsignedWordToByte => ((raw as i16).clamp(0, 255) as u8) as u64,
                PackKind::SignedDwordToWord => ((raw as i32).clamp(-32768, 32767) as i16) as u64,
                PackKind::UnsignedDwordToWord => ((raw as i32).clamp(0, 65535) as u16) as u64,
            };
            write_elem_u64(&mut out, out_idx, dst_esize, packed);
        }
    }
    Some(out)
}

fn sat_add_sub(dst: &[u8], src: &[u8], op: &str) -> Option<Vec<u8>> {
    if dst.len() != src.len() || dst.is_empty() || dst.len() % 2 != 0 {
        return None;
    }
    let esize = if op.ends_with('w') { 2 } else { 1 };
    let is_sub = op.starts_with("psub");
    let is_unsigned = op.contains('u');
    let lanes = dst.len() / esize;
    let mut out = vec![0u8; dst.len()];
    for i in 0..lanes {
        let a = read_elem_u64(dst, i, esize);
        let b = read_elem_u64(src, i, esize);
        let res = match (esize, is_unsigned, is_sub) {
            (1, false, false) => (a as i8).saturating_add(b as i8) as u64,
            (1, false, true) => (a as i8).saturating_sub(b as i8) as u64,
            (1, true, false) => (a as u8).saturating_add(b as u8) as u64,
            (1, true, true) => (a as u8).saturating_sub(b as u8) as u64,
            (2, false, false) => (a as i16).saturating_add(b as i16) as u64,
            (2, false, true) => (a as i16).saturating_sub(b as i16) as u64,
            (2, true, false) => (a as u16).saturating_add(b as u16) as u64,
            (2, true, true) => (a as u16).saturating_sub(b as u16) as u64,
            _ => return None,
        };
        write_elem_u64(&mut out, i, esize, res);
    }
    Some(out)
}

#[derive(Clone, Copy)]
enum MulWordKind {
    SignedHigh,
    UnsignedHigh,
    SignedRoundScale,
}

fn pmul_words(dst: &[u8], src: &[u8], kind: MulWordKind) -> Option<Vec<u8>> {
    if dst.len() != src.len() || dst.is_empty() || dst.len() % 2 != 0 {
        return None;
    }
    let lanes = dst.len() / 2;
    let mut out = vec![0u8; dst.len()];
    for i in 0..lanes {
        let a = read_elem_u64(dst, i, 2);
        let b = read_elem_u64(src, i, 2);
        let res = match kind {
            MulWordKind::SignedHigh => {
                (((a as i16 as i32) * (b as i16 as i32)) >> 16) as i16 as u64
            }
            MulWordKind::UnsignedHigh => {
                (((a as u16 as u32) * (b as u16 as u32)) >> 16) as u16 as u64
            }
            MulWordKind::SignedRoundScale => {
                let t = (((a as i16 as i32) * (b as i16 as i32)) >> 14) + 1;
                (t >> 1) as i16 as u64
            }
        };
        write_elem_u64(&mut out, i, 2, res);
    }
    Some(out)
}

fn pmulld(dst: &[u8], src: &[u8]) -> Option<Vec<u8>> {
    if dst.len() != src.len() || dst.is_empty() || dst.len() % 4 != 0 {
        return None;
    }
    let lanes = dst.len() / 4;
    let mut out = vec![0u8; dst.len()];
    for i in 0..lanes {
        let a = read_elem_u64(dst, i, 4) as i32;
        let b = read_elem_u64(src, i, 4) as i32;
        write_elem_u64(&mut out, i, 4, a.wrapping_mul(b) as u32 as u64);
    }
    Some(out)
}

fn pmuldq(dst: &[u8], src: &[u8]) -> Option<Vec<u8>> {
    if dst.len() != src.len() || dst.len() != 16 {
        return None;
    }
    let mut out = vec![0u8; 16];
    for i in 0..2 {
        let a = read_elem_u64(dst, i * 2, 4) as i32 as i64;
        let b = read_elem_u64(src, i * 2, 4) as i32 as i64;
        write_elem_u64(&mut out, i, 8, a.wrapping_mul(b) as u64);
    }
    Some(out)
}

fn pmaddwd(dst: &[u8], src: &[u8]) -> Option<Vec<u8>> {
    if dst.len() != src.len() || dst.is_empty() || dst.len() % 4 != 0 {
        return None;
    }
    let pairs = dst.len() / 4;
    let mut out = vec![0u8; dst.len()];
    for i in 0..pairs {
        let d0 = read_elem_u64(dst, 2 * i, 2) as i16 as i32;
        let s0 = read_elem_u64(src, 2 * i, 2) as i16 as i32;
        let d1 = read_elem_u64(dst, 2 * i + 1, 2) as i16 as i32;
        let s1 = read_elem_u64(src, 2 * i + 1, 2) as i16 as i32;
        let sum = (d0 * s0).wrapping_add(d1 * s1);
        write_elem_u64(&mut out, i, 4, sum as u32 as u64);
    }
    Some(out)
}

fn pmaddubsw(dst: &[u8], src: &[u8]) -> Option<Vec<u8>> {
    if dst.len() != src.len() || dst.is_empty() || dst.len() % 2 != 0 {
        return None;
    }
    let pairs = dst.len() / 2;
    let mut out = vec![0u8; dst.len()];
    for i in 0..pairs {
        let d0 = dst[2 * i] as i32;
        let s0 = (src[2 * i] as i8) as i32;
        let d1 = dst[2 * i + 1] as i32;
        let s1 = (src[2 * i + 1] as i8) as i32;
        let sum = (d0 * s0 + d1 * s1).clamp(-32768, 32767) as i16;
        write_elem_u64(&mut out, i, 2, sum as u64);
    }
    Some(out)
}

fn psadbw(dst: &[u8], src: &[u8]) -> Option<Vec<u8>> {
    if dst.len() != src.len() || (dst.len() != 8 && dst.len() != 16) {
        return None;
    }
    let mut out = vec![0u8; dst.len()];
    for blk in 0..(dst.len() / 8) {
        let off = blk * 8;
        let sum: u16 = dst[off..off + 8]
            .iter()
            .zip(&src[off..off + 8])
            .map(|(&a, &b)| u16::from(a.abs_diff(b)))
            .sum();
        out[off..off + 2].copy_from_slice(&sum.to_le_bytes());
    }
    Some(out)
}

fn mpsadbw(dst: &[u8], src: &[u8], imm: u8) -> Option<Vec<u8>> {
    if dst.len() != 16 || src.len() != 16 {
        return None;
    }
    let src_off = ((imm & 3) as usize) * 4;
    let dst_off = (((imm >> 2) & 1) as usize) * 4;
    let mut out = vec![0u8; 16];
    for i in 0..8 {
        let mut sum = 0u16;
        for k in 0..4 {
            sum += u16::from(dst[dst_off + i + k].abs_diff(src[src_off + k]));
        }
        write_elem_u64(&mut out, i, 2, u64::from(sum));
    }
    Some(out)
}

fn horiz_add_sub(dst: &[u8], src: &[u8], op: &str) -> Option<Vec<u8>> {
    if dst.len() != src.len() || (dst.len() != 8 && dst.len() != 16) {
        return None;
    }
    let esize = if op.ends_with('d') { 4 } else { 2 };
    let is_sub = op.contains("sub");
    let is_sat = op.ends_with("sw");
    let n = dst.len() / esize;
    let half = n / 2;
    let mut out = vec![0u8; dst.len()];
    for i in 0..n {
        let (v, j) = if i < half { (dst, i) } else { (src, i - half) };
        let a = read_elem_u64(v, 2 * j, esize);
        let b = read_elem_u64(v, 2 * j + 1, esize);
        let res = match (esize, is_sub, is_sat) {
            (2, false, false) => (a as i16).wrapping_add(b as i16) as u64,
            (2, true, false) => (a as i16).wrapping_sub(b as i16) as u64,
            (2, false, true) => (a as i16).saturating_add(b as i16) as u64,
            (2, true, true) => (a as i16).saturating_sub(b as i16) as u64,
            (4, false, false) => (a as i32).wrapping_add(b as i32) as u32 as u64,
            (4, true, false) => (a as i32).wrapping_sub(b as i32) as u32 as u64,
            _ => return None,
        };
        write_elem_u64(&mut out, i, esize, res);
    }
    Some(out)
}

fn psign(dst: &[u8], src: &[u8], esize: usize) -> Option<Vec<u8>> {
    if dst.len() != src.len() || dst.is_empty() || dst.len() % esize != 0 {
        return None;
    }
    let lanes = dst.len() / esize;
    let mut out = vec![0u8; dst.len()];
    for i in 0..lanes {
        let d = sign_extend_esize(read_elem_u64(dst, i, esize), esize);
        let s = sign_extend_esize(read_elem_u64(src, i, esize), esize);
        let res = if s < 0 {
            d.wrapping_neg()
        } else if s == 0 {
            0
        } else {
            d
        };
        write_elem_u64(&mut out, i, esize, res as u64);
    }
    Some(out)
}

fn pabs(src: &[u8], esize: usize) -> Option<Vec<u8>> {
    if src.is_empty() || src.len() % esize != 0 {
        return None;
    }
    let lanes = src.len() / esize;
    let mut out = vec![0u8; src.len()];
    for i in 0..lanes {
        let s = sign_extend_esize(read_elem_u64(src, i, esize), esize);
        write_elem_u64(&mut out, i, esize, s.unsigned_abs());
    }
    Some(out)
}

fn pshift_w(dst: &[u8], count: u64, arith: bool) -> Option<Vec<u8>> {
    if dst.is_empty() || dst.len() % 2 != 0 {
        return None;
    }
    let lanes = dst.len() / 2;
    let mut out = vec![0u8; dst.len()];
    for i in 0..lanes {
        let w = read_elem_u64(dst, i, 2) as u16;
        let res = if arith {
            let s = w as i16;
            if count >= 16 {
                (s >> 15) as u16
            } else {
                (s >> (count as u32)) as u16
            }
        } else if count >= 16 {
            0
        } else {
            w << (count as u32)
        };
        write_elem_u64(&mut out, i, 2, u64::from(res));
    }
    Some(out)
}

fn pmov_ext(src: &[u8], name: &str, out_size: usize) -> Option<Vec<u8>> {
    let signed = name.starts_with("pmovsx");
    let suffix = name.get(name.len().checked_sub(2)?..)?;
    let (src_esize, dst_esize) = match suffix {
        "bw" => (1, 2),
        "bd" => (1, 4),
        "bq" => (1, 8),
        "wd" => (2, 4),
        "wq" => (2, 8),
        "dq" => (4, 8),
        _ => return None,
    };
    if out_size == 0 || out_size % dst_esize != 0 {
        return None;
    }
    let lanes = out_size / dst_esize;
    if src.len() < lanes * src_esize {
        return None;
    }
    let mut out = vec![0u8; out_size];
    for i in 0..lanes {
        let raw = read_elem_u64(src, i, src_esize);
        let extended = if signed {
            sign_extend_esize(raw, src_esize) as u64
        } else {
            raw
        };
        write_elem_u64(&mut out, i, dst_esize, extended);
    }
    Some(out)
}

fn dpps(dst: &[u8], src: &[u8], imm: u8) -> Option<Vec<u8>> {
    if dst.len() != 16 || src.len() != 16 {
        return None;
    }
    let mut t = [0u32; 4];
    for (i, slot) in t.iter_mut().enumerate() {
        *slot = if ((imm >> (4 + i)) & 1) != 0 {
            let a = read_elem_u64(dst, i, 4) as u32;
            let b = read_elem_u64(src, i, 4) as u32;
            f32_mul(a, b)
        } else {
            0
        };
    }
    let sum = f32_add(f32_add(t[0], t[1]), f32_add(t[2], t[3]));
    let mut out = vec![0u8; 16];
    for i in 0..4 {
        let val = if ((imm >> i) & 1) != 0 { sum } else { 0 };
        write_elem_u64(&mut out, i, 4, u64::from(val));
    }
    Some(out)
}

fn dppd(dst: &[u8], src: &[u8], imm: u8) -> Option<Vec<u8>> {
    if dst.len() != 16 || src.len() != 16 {
        return None;
    }
    let mut t = [0u64; 2];
    for (i, slot) in t.iter_mut().enumerate() {
        *slot = if ((imm >> (4 + i)) & 1) != 0 {
            let a = read_elem_u64(dst, i, 8);
            let b = read_elem_u64(src, i, 8);
            f64_mul(a, b)
        } else {
            0
        };
    }
    let sum = f64_add(t[0], t[1]);
    let mut out = vec![0u8; 16];
    for i in 0..2 {
        let val = if ((imm >> i) & 1) != 0 { sum } else { 0 };
        write_elem_u64(&mut out, i, 8, val);
    }
    Some(out)
}

fn extractps(src: &[u8], imm: u8, out_size: usize) -> Option<Vec<u8>> {
    if src.len() != 16 || (out_size != 4 && out_size != 8) {
        return None;
    }
    let lane = (imm & 3) as usize;
    let val = read_elem_u64(src, lane, 4);
    let mut out = vec![0u8; out_size];
    out[..4].copy_from_slice(&(val as u32).to_le_bytes());
    Some(out)
}

fn insertps(dst: &[u8], src: &[u8], imm: u8) -> Option<Vec<u8>> {
    if dst.len() != 16 || (src.len() != 4 && src.len() != 16) {
        return None;
    }
    let src_lane = if src.len() == 4 {
        0
    } else {
        ((imm >> 6) & 3) as usize
    };
    let dst_lane = ((imm >> 4) & 3) as usize;
    let zmask = imm & 0x0F;
    let inserted = read_elem_u64(src, src_lane, 4);
    let mut out = dst.to_vec();
    write_elem_u64(&mut out, dst_lane, 4, inserted);
    for i in 0..4 {
        if ((zmask >> i) & 1) != 0 {
            write_elem_u64(&mut out, i, 4, 0);
        }
    }
    Some(out)
}

fn phminposuw(src: &[u8]) -> Option<Vec<u8>> {
    if src.len() != 16 {
        return None;
    }
    let mut best_val = u16::MAX;
    let mut best_idx = 0usize;
    for i in 0..8 {
        let v = read_elem_u64(src, i, 2) as u16;
        if i == 0 || v < best_val {
            best_val = v;
            best_idx = i;
        }
    }
    let mut out = vec![0u8; 16];
    write_elem_u64(&mut out, 0, 2, u64::from(best_val));
    write_elem_u64(&mut out, 1, 2, best_idx as u64);
    Some(out)
}

fn movmsk(src: &[u8], esize: usize, out_size: usize) -> Option<Vec<u8>> {
    if src.len() != 16 || out_size == 0 || out_size > 8 {
        return None;
    }
    let lanes = 16 / esize;
    let sign_shift = esize * 8 - 1;
    let mut mask = 0u64;
    for i in 0..lanes {
        let bits = read_elem_u64(src, i, esize);
        mask |= ((bits >> sign_shift) & 1) << i;
    }
    let mut out = vec![0u8; out_size];
    write_elem_u64(&mut out, 0, out_size, mask);
    Some(out)
}

fn pavg(a: &[u8], b: &[u8]) -> Option<Vec<u8>> {
    if a.len() != b.len() {
        return None;
    }
    match a.len() {
        1 => {
            let sum = u16::from(a[0]) + u16::from(b[0]) + 1;
            Some(vec![(sum >> 1) as u8])
        }
        2 => {
            let av = u16::from_le_bytes([a[0], a[1]]);
            let bv = u16::from_le_bytes([b[0], b[1]]);
            let sum = u32::from(av) + u32::from(bv) + 1;
            Some(((sum >> 1) as u16).to_le_bytes().to_vec())
        }
        _ => None,
    }
}

fn fp_binop_f32(dst: &[u8], src: &[u8], f: impl Fn(u32, u32) -> u32) -> Option<Vec<u8>> {
    if dst.len() != src.len() || dst.is_empty() || dst.len() % 4 != 0 {
        return None;
    }
    let lanes = dst.len() / 4;
    let mut out = vec![0u8; dst.len()];
    for i in 0..lanes {
        let a = read_elem_u64(dst, i, 4) as u32;
        let b = read_elem_u64(src, i, 4) as u32;
        write_elem_u64(&mut out, i, 4, u64::from(f(a, b)));
    }
    Some(out)
}

fn fp_binop_f64(dst: &[u8], src: &[u8], f: impl Fn(u64, u64) -> u64) -> Option<Vec<u8>> {
    if dst.len() != src.len() || dst.is_empty() || dst.len() % 8 != 0 {
        return None;
    }
    let lanes = dst.len() / 8;
    let mut out = vec![0u8; dst.len()];
    for i in 0..lanes {
        let a = read_elem_u64(dst, i, 8);
        let b = read_elem_u64(src, i, 8);
        write_elem_u64(&mut out, i, 8, f(a, b));
    }
    Some(out)
}

fn fp_unop_f32(src: &[u8], f: impl Fn(u32) -> u32) -> Option<Vec<u8>> {
    if src.is_empty() || src.len() % 4 != 0 {
        return None;
    }
    let lanes = src.len() / 4;
    let mut out = vec![0u8; src.len()];
    for i in 0..lanes {
        let a = read_elem_u64(src, i, 4) as u32;
        write_elem_u64(&mut out, i, 4, u64::from(f(a)));
    }
    Some(out)
}

fn fp_unop_f64(src: &[u8], f: impl Fn(u64) -> u64) -> Option<Vec<u8>> {
    if src.is_empty() || src.len() % 8 != 0 {
        return None;
    }
    let lanes = src.len() / 8;
    let mut out = vec![0u8; src.len()];
    for i in 0..lanes {
        let a = read_elem_u64(src, i, 8);
        write_elem_u64(&mut out, i, 8, f(a));
    }
    Some(out)
}

fn scalar_f32_unop(dst: &[u8], src: &[u8], f: impl Fn(u32) -> u32) -> Option<Vec<u8>> {
    if dst.len() < 4 || src.len() < 4 {
        return None;
    }
    let mut out = dst.to_vec();
    let val = u32::from_le_bytes(src[..4].try_into().ok()?);
    out[..4].copy_from_slice(&f(val).to_le_bytes());
    Some(out)
}

#[inline]
fn is_nan32(b: u32) -> bool {
    (b & 0x7FFF_FFFF) > 0x7F80_0000
}

#[inline]
fn is_nan64(b: u64) -> bool {
    (b & 0x7FFF_FFFF_FFFF_FFFF) > 0x7FF0_0000_0000_0000
}

fn f32_add(a: u32, b: u32) -> u32 {
    if is_nan32(a) {
        return a | 0x0040_0000;
    }
    if is_nan32(b) {
        return b | 0x0040_0000;
    }
    let res = f32::from_bits(a) + f32::from_bits(b);
    if res.is_nan() {
        X86_QNAN32
    } else {
        res.to_bits()
    }
}

fn f32_mul(a: u32, b: u32) -> u32 {
    if is_nan32(a) {
        return a | 0x0040_0000;
    }
    if is_nan32(b) {
        return b | 0x0040_0000;
    }
    let res = f32::from_bits(a) * f32::from_bits(b);
    if res.is_nan() {
        X86_QNAN32
    } else {
        res.to_bits()
    }
}

fn f32_div(a: u32, b: u32) -> u32 {
    if is_nan32(a) {
        return a | 0x0040_0000;
    }
    if is_nan32(b) {
        return b | 0x0040_0000;
    }
    let res = f32::from_bits(a) / f32::from_bits(b);
    if res.is_nan() {
        X86_QNAN32
    } else {
        res.to_bits()
    }
}

fn f64_add(a: u64, b: u64) -> u64 {
    if is_nan64(a) {
        return a | 0x0008_0000_0000_0000;
    }
    if is_nan64(b) {
        return b | 0x0008_0000_0000_0000;
    }
    let res = f64::from_bits(a) + f64::from_bits(b);
    if res.is_nan() {
        X86_QNAN64
    } else {
        res.to_bits()
    }
}

fn f64_mul(a: u64, b: u64) -> u64 {
    if is_nan64(a) {
        return a | 0x0008_0000_0000_0000;
    }
    if is_nan64(b) {
        return b | 0x0008_0000_0000_0000;
    }
    let res = f64::from_bits(a) * f64::from_bits(b);
    if res.is_nan() {
        X86_QNAN64
    } else {
        res.to_bits()
    }
}

fn f64_div(a: u64, b: u64) -> u64 {
    if is_nan64(a) {
        return a | 0x0008_0000_0000_0000;
    }
    if is_nan64(b) {
        return b | 0x0008_0000_0000_0000;
    }
    let res = f64::from_bits(a) / f64::from_bits(b);
    if res.is_nan() {
        X86_QNAN64
    } else {
        res.to_bits()
    }
}

fn f32_sqrt(a: u32) -> u32 {
    if is_nan32(a) {
        return a | 0x0040_0000;
    }
    let v = f32::from_bits(a);
    if v < 0.0 {
        return X86_QNAN32;
    }
    v.sqrt().to_bits()
}

fn f64_sqrt(a: u64) -> u64 {
    if is_nan64(a) {
        return a | 0x0008_0000_0000_0000;
    }
    let v = f64::from_bits(a);
    if v < 0.0 {
        return X86_QNAN64;
    }
    v.sqrt().to_bits()
}

fn f32_min(a: u32, b: u32) -> u32 {
    if f32::from_bits(a) < f32::from_bits(b) {
        a
    } else {
        b
    }
}

fn f32_max(a: u32, b: u32) -> u32 {
    if f32::from_bits(a) > f32::from_bits(b) {
        a
    } else {
        b
    }
}

fn f64_min(a: u64, b: u64) -> u64 {
    if f64::from_bits(a) < f64::from_bits(b) {
        a
    } else {
        b
    }
}

fn f64_max(a: u64, b: u64) -> u64 {
    if f64::from_bits(a) > f64::from_bits(b) {
        a
    } else {
        b
    }
}

fn f32_rcp(a: u32) -> u32 {
    if is_nan32(a) {
        return a | 0x0040_0000;
    }
    (1.0f32 / f32::from_bits(a)).to_bits()
}

fn f32_rsqrt(a: u32) -> u32 {
    if is_nan32(a) {
        return a | 0x0040_0000;
    }
    let v = f32::from_bits(a);
    if v < 0.0 {
        return X86_QNAN32;
    }
    if v == 0.0 {
        return (1.0f32 / v).to_bits();
    }
    (1.0f32 / v.sqrt()).to_bits()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundsd_preserves_upper_lane_and_signed_zero() {
        let mut dst = [0u8; 16];
        dst[8..16].copy_from_slice(&0xDEAD_BEEF_CAFE_BABEu64.to_le_bytes());
        let src = (-0.4f64).to_le_bytes();
        let out = round_sd(&dst, &src, 0).unwrap();
        assert_eq!(&out[..8], &(-0.0f64).to_le_bytes());
        assert_eq!(&out[8..16], &0xDEAD_BEEF_CAFE_BABEu64.to_le_bytes());

        let out_floor = round_sd(&dst, &(-1.25f64).to_le_bytes(), 1).unwrap();
        assert_eq!(&out_floor[..8], &(-2.0f64).to_le_bytes());
        let out_ceil = round_sd(&dst, &(1.25f64).to_le_bytes(), 2).unwrap();
        assert_eq!(&out_ceil[..8], &(2.0f64).to_le_bytes());
        let out_trunc = round_sd(&dst, &(-1.75f64).to_le_bytes(), 3).unwrap();
        assert_eq!(&out_trunc[..8], &(-1.0f64).to_le_bytes());
    }

    #[test]
    fn pshufb_handles_mmx_and_xmm_and_zero_bit() {
        let dst: Vec<u8> = (10..26).collect();
        let mask = [0, 15, 0x80, 3, 0xFF, 1, 2, 14, 0x90, 5, 6, 7, 8, 9, 10, 11];
        let out = pshufb(&dst, &mask).unwrap();
        assert_eq!(
            out,
            vec![10, 25, 0, 13, 0, 11, 12, 24, 0, 15, 16, 17, 18, 19, 20, 21]
        );

        let mmx_dst = [1u8, 2, 3, 4, 5, 6, 7, 8];
        let mmx_mask = [7, 0x81, 0, 9, 2, 3, 4, 5];
        assert_eq!(
            pshufb(&mmx_dst, &mmx_mask).unwrap(),
            vec![8, 0, 1, 2, 3, 4, 5, 6]
        );
    }

    #[test]
    fn pack_and_pmaddubsw_and_phminposuw_match_sdm() {
        let dst = [-200i16, 200, -128, 127]
            .into_iter()
            .flat_map(i16::to_le_bytes)
            .collect::<Vec<_>>();
        let src = [-129i16, 128, 0, 1000]
            .into_iter()
            .flat_map(i16::to_le_bytes)
            .collect::<Vec<_>>();
        let packed = pack(&dst, &src, PackKind::SignedWordToByte).unwrap();
        assert_eq!(
            packed,
            vec![
                (-128i8) as u8,
                127,
                (-128i8) as u8,
                127,
                (-128i8) as u8,
                127,
                0,
                127
            ]
        );

        let mut u_in = [0u8; 16];
        u_in[0] = 255;
        u_in[1] = 255;
        let mut s_in = [0u8; 16];
        s_in[0] = 127;
        s_in[1] = 127;
        let madd = pmaddubsw(&u_in, &s_in).unwrap();
        assert_eq!(i16::from_le_bytes([madd[0], madd[1]]), i16::MAX);

        let words = [9u16, 4, 3, 7, 3, 12, 5, 8]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        let minpos = phminposuw(&words).unwrap();
        assert_eq!(u16::from_le_bytes([minpos[0], minpos[1]]), 3);
        assert_eq!(u16::from_le_bytes([minpos[2], minpos[3]]), 2);
        assert!(minpos[4..].iter().all(|&b| b == 0));
    }

    #[test]
    fn min_max_and_insertps_follow_x86_rules() {
        // -0.0 vs +0.0 returns second operand (+0.0) in MINPD and MAXPD
        let neg_zero = (-0.0f64).to_le_bytes();
        let pos_zero = (0.0f64).to_le_bytes();
        let mut a = [0u8; 16];
        let mut b = [0u8; 16];
        a[..8].copy_from_slice(&neg_zero);
        b[..8].copy_from_slice(&pos_zero);
        assert_eq!(&fp_binop_f64(&a, &b, f64_min).unwrap()[..8], &pos_zero);

        // insertps with m32 ignores count_s and applies zmask
        let d = [1.0f32, 2.0, 3.0, 4.0]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>();
        let s32 = 9.0f32.to_le_bytes();
        // count_s=3 (ignored), count_d=1, zmask=0b0101 (zero lanes 0 and 2) => imm = 0b11_01_0101 = 0xD5
        let res = insertps(&d, &s32, 0xD5).unwrap();
        let got: Vec<f32> = res
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        assert_eq!(got, vec![0.0, 9.0, 0.0, 4.0]);
    }
}
