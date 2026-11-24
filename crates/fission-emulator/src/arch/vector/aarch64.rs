//! AArch64 NEON / AdvSIMD `CALLOTHER` vector and scalar userops.

use fission_pcode_ir::ir::{PcodeOp, Varnode};

use super::{
    read_bytes, read_const_u64, read_elem_u64, sign_extend_esize, write_bytes, write_elem_u64,
};
use crate::core::Emulator;

pub(super) fn is_known(name: &str) -> bool {
    matches!(
        name,
        "NEON_umaxp"
            | "NEON_uminp"
            | "NEON_smaxp"
            | "NEON_sminp"
            | "NEON_ext"
            | "NEON_fmin"
            | "NEON_fmax"
            | "NEON_fminnm"
            | "NEON_fmaxnm"
            | "NEON_scvtf"
            | "NEON_ucvtf"
            | "NEON_sqadd"
            | "NEON_uqadd"
            | "NEON_sqsub"
            | "NEON_uqsub"
            | "NEON_sqxtn"
            | "NEON_uqxtn"
            | "NEON_sqxtun"
            | "NEON_sqxtn2"
            | "NEON_uqxtn2"
            | "NEON_sqxtun2"
            | "NEON_sshl"
            | "NEON_ushl"
            | "NEON_urshr"
            | "NEON_srshr"
            | "NEON_rshrn"
            | "NEON_rshrn2"
            | "NEON_sqrdmulh"
            | "NEON_sqdmulh"
            | "NEON_saba"
            | "NEON_uaba"
            | "NEON_uabd"
            | "NEON_sabd"
            | "NEON_uaddlv"
            | "NEON_saddlv"
            | "NEON_rev64"
            | "NEON_rev32"
            | "NEON_rev16"
    )
}

pub(super) fn answer(emu: &mut Emulator, name: &str, op: &PcodeOp, out: &Varnode) -> bool {
    let nargs = op.inputs.len().saturating_sub(1);
    match name {
        "NEON_umaxp" => pairwise_op(emu, op, out, Pairwise::UnsignedMax),
        "NEON_uminp" => pairwise_op(emu, op, out, Pairwise::UnsignedMin),
        "NEON_smaxp" => pairwise_op(emu, op, out, Pairwise::SignedMax),
        "NEON_sminp" => pairwise_op(emu, op, out, Pairwise::SignedMin),
        "NEON_ext" => {
            let Some(index) = op
                .inputs
                .get(3)
                .filter(|vn| vn.is_constant && vn.constant_val >= 0)
                .and_then(|vn| usize::try_from(vn.constant_val).ok())
            else {
                return false;
            };
            let (Ok(a), Ok(b)) = (read_bytes(emu, op, 1), read_bytes(emu, op, 2)) else {
                return false;
            };
            let Some(result) = extract_bytes(&a, &b, index) else {
                return false;
            };
            if result.len() != out.size as usize {
                return false;
            }
            write_neon_result(emu, out, &result)
        }
        "NEON_fmin" | "NEON_fmax" | "NEON_fminnm" | "NEON_fmaxnm" => {
            let (Ok(a), Ok(b)) = (read_bytes(emu, op, 1), read_bytes(emu, op, 2)) else {
                return false;
            };
            let esize = if nargs == 2 {
                out.size as usize
            } else {
                let Some(e) = read_const_u64(op, 3) else {
                    return false;
                };
                e as usize
            };
            let is_max = name.contains("max");
            let is_nm = name.ends_with("nm");
            neon_fp_minmax(&a, &b, esize, is_max, is_nm)
                .filter(|r| r.len() == out.size as usize)
                .is_some_and(|r| write_neon_result(emu, out, &r))
        }
        "NEON_scvtf" => answer_scvtf(emu, op, out, nargs),
        "NEON_ucvtf" => answer_ucvtf(emu, op, out, nargs),
        "NEON_sqadd" | "NEON_uqadd" | "NEON_sqsub" | "NEON_uqsub" => {
            let (Ok(a), Ok(b)) = (read_bytes(emu, op, 1), read_bytes(emu, op, 2)) else {
                return false;
            };
            let esize = if nargs == 2 {
                out.size as usize
            } else {
                let Some(e) = read_const_u64(op, 3) else {
                    return false;
                };
                e as usize
            };
            let signed = name.contains("sq");
            let is_sub = name.ends_with("sub");
            neon_sat_add_sub(&a, &b, esize, signed, is_sub)
                .filter(|r| r.len() == out.size as usize)
                .is_some_and(|r| write_neon_result(emu, out, &r))
        }
        "NEON_sqxtn" | "NEON_uqxtn" | "NEON_sqxtun" | "NEON_sqxtn2" | "NEON_uqxtn2"
        | "NEON_sqxtun2" => {
            let is_part2 = name.ends_with('2');
            let (Ok(rd), Ok(rn)) = (read_bytes(emu, op, 1), read_bytes(emu, op, 2)) else {
                return false;
            };
            let src_esize = if nargs == 2 {
                rn.len()
            } else {
                let Some(e) = read_const_u64(op, 3) else {
                    return false;
                };
                e as usize
            };
            let kind = match name {
                "NEON_sqxtn" | "NEON_sqxtn2" => NarrowSatKind::SignedToSigned,
                "NEON_uqxtn" | "NEON_uqxtn2" => NarrowSatKind::UnsignedToUnsigned,
                _ => NarrowSatKind::SignedToUnsigned,
            };
            neon_xtn(&rd, &rn, src_esize, kind, is_part2)
                .filter(|r| r.len() == out.size as usize)
                .is_some_and(|r| write_neon_result(emu, out, &r))
        }
        "NEON_sshl" | "NEON_ushl" => {
            let (Ok(a), Ok(b)) = (read_bytes(emu, op, 1), read_bytes(emu, op, 2)) else {
                return false;
            };
            let esize = if nargs == 2 {
                out.size as usize
            } else {
                let Some(e) = read_const_u64(op, 3) else {
                    return false;
                };
                e as usize
            };
            neon_shl(&a, &b, esize, name == "NEON_sshl")
                .filter(|r| r.len() == out.size as usize)
                .is_some_and(|r| write_neon_result(emu, out, &r))
        }
        "NEON_urshr" | "NEON_srshr" => {
            let (Ok(rn), Some(shift_raw)) = (read_bytes(emu, op, 1), read_const_u64(op, 2)) else {
                return false;
            };
            let (esize, shift) = if nargs == 2 {
                // Scalar `urshr Rd_FPR64, Rn_FPR64, Imm_shr_imm32` in AArch64neon.sinc
                // passes `(true_shift - 64) as u8` because it used Imm_shr_imm32 instead of imm64.
                let s = if name == "NEON_urshr" {
                    u32::from((shift_raw as u8).wrapping_add(64))
                } else {
                    shift_raw as u32
                };
                (out.size as usize, s)
            } else {
                let Some(e) = read_const_u64(op, 3) else {
                    return false;
                };
                (e as usize, shift_raw as u32)
            };
            neon_rshr(&rn, shift, esize, name == "NEON_srshr")
                .filter(|r| r.len() == out.size as usize)
                .is_some_and(|r| write_neon_result(emu, out, &r))
        }
        "NEON_rshrn" | "NEON_rshrn2" => {
            let is_part2 = name.ends_with('2');
            let (Ok(rd), Ok(rn), Some(shift), Some(src_esize)) = (
                read_bytes(emu, op, 1),
                read_bytes(emu, op, 2),
                read_const_u64(op, 3),
                read_const_u64(op, 4),
            ) else {
                return false;
            };
            neon_rshrn(&rd, &rn, shift as u32, src_esize as usize, is_part2)
                .filter(|r| r.len() == out.size as usize)
                .is_some_and(|r| write_neon_result(emu, out, &r))
        }
        "NEON_sqrdmulh" | "NEON_sqdmulh" => {
            let (Ok(a), Ok(b)) = (read_bytes(emu, op, 1), read_bytes(emu, op, 2)) else {
                return false;
            };
            let esize = if nargs == 2 {
                out.size as usize
            } else {
                let Some(e) = read_const_u64(op, 3) else {
                    return false;
                };
                e as usize
            };
            neon_sqdmulh(&a, &b, esize, name == "NEON_sqrdmulh")
                .filter(|r| r.len() == out.size as usize)
                .is_some_and(|r| write_neon_result(emu, out, &r))
        }
        "NEON_saba" | "NEON_uaba" => {
            let (Ok(rd), Ok(rn), Ok(rm), Some(esize)) = (
                read_bytes(emu, op, 1),
                read_bytes(emu, op, 2),
                read_bytes(emu, op, 3),
                read_const_u64(op, 4),
            ) else {
                return false;
            };
            neon_aba(&rd, &rn, &rm, esize as usize, name == "NEON_saba")
                .filter(|r| r.len() == out.size as usize)
                .is_some_and(|r| write_neon_result(emu, out, &r))
        }
        "NEON_uabd" | "NEON_sabd" => {
            let (Ok(rn), Ok(rm), Some(esize)) = (
                read_bytes(emu, op, 1),
                read_bytes(emu, op, 2),
                read_const_u64(op, 3),
            ) else {
                return false;
            };
            neon_abd(&rn, &rm, esize as usize, name == "NEON_sabd")
                .filter(|r| r.len() == out.size as usize)
                .is_some_and(|r| write_neon_result(emu, out, &r))
        }
        "NEON_uaddlv" | "NEON_saddlv" => {
            let (Ok(rn), Some(esize)) = (read_bytes(emu, op, 1), read_const_u64(op, 2)) else {
                return false;
            };
            neon_addlv(
                &rn,
                esize as usize,
                name == "NEON_saddlv",
                out.size as usize,
            )
            .filter(|r| r.len() == out.size as usize)
            .is_some_and(|r| write_neon_result(emu, out, &r))
        }
        "NEON_rev64" | "NEON_rev32" | "NEON_rev16" => {
            let (Ok(rn), Some(esize)) = (read_bytes(emu, op, 1), read_const_u64(op, 2)) else {
                return false;
            };
            let container = match name {
                "NEON_rev64" => 8,
                "NEON_rev32" => 4,
                _ => 2,
            };
            neon_rev(&rn, container, esize as usize)
                .filter(|r| r.len() == out.size as usize)
                .is_some_and(|r| write_neon_result(emu, out, &r))
        }
        _ => false,
    }
}

fn pairwise_op(emu: &mut Emulator, op: &PcodeOp, out: &Varnode, kind: Pairwise) -> bool {
    let Some(esize) = op.inputs.get(3).map(|vn| vn.constant_val as usize) else {
        return false;
    };
    if esize == 0 || esize > 8 {
        return false;
    }
    let (Ok(a), Ok(b)) = (read_bytes(emu, op, 1), read_bytes(emu, op, 2)) else {
        return false;
    };
    if a.len() != b.len() || a.is_empty() || a.len() % (esize * 2) != 0 {
        return false;
    }
    let result = pairwise_reduce(&a, &b, esize, kind);
    if result.len() != out.size as usize {
        return false;
    }
    write_neon_result(emu, out, &result)
}

/// Write a NEON register or scalar result and zero bits `[out.size*8 .. 128]`
/// when the destination is an AArch64 VPR/FPR register smaller than 16 bytes.
///
/// The SLEIGH `NEON_*` userops omit the `zext_z*` tail macros that pure p-code
/// SIMD constructors emit.
fn write_neon_result(emu: &mut Emulator, out: &Varnode, data: &[u8]) -> bool {
    if !write_bytes(emu, out, data) {
        return false;
    }
    let reg_space = emu.state.register_space();
    if out.space_id == reg_space
        && out.offset >= 0x5000
        && out.offset < 0x5400
        && (out.offset - 0x5000) % 32 == 0
        && out.size < 16
    {
        let zero_len = 16usize - out.size as usize;
        let zeros = [0u8; 16];
        return emu
            .state
            .write_space(
                reg_space,
                out.offset + u64::from(out.size),
                &zeros[..zero_len],
            )
            .is_ok();
    }
    true
}

#[derive(Clone, Copy)]
enum Pairwise {
    UnsignedMax,
    UnsignedMin,
    SignedMax,
    SignedMin,
}

fn pairwise_reduce(a: &[u8], b: &[u8], esize: usize, kind: Pairwise) -> Vec<u8> {
    let n = a.len() / esize;
    let half = n / 2;
    let mut out = vec![0u8; a.len()];
    for i in 0..n {
        let (src, j) = if i < half { (a, i) } else { (b, i - half) };
        let (x, y) = (
            read_elem_u64(src, 2 * j, esize),
            read_elem_u64(src, 2 * j + 1, esize),
        );
        let picked = match kind {
            Pairwise::UnsignedMax => x.max(y),
            Pairwise::UnsignedMin => x.min(y),
            Pairwise::SignedMax => {
                sign_extend_esize(x, esize).max(sign_extend_esize(y, esize)) as u64
            }
            Pairwise::SignedMin => {
                sign_extend_esize(x, esize).min(sign_extend_esize(y, esize)) as u64
            }
        };
        write_elem_u64(&mut out, i, esize, picked);
    }
    out
}

fn extract_bytes(a: &[u8], b: &[u8], index: usize) -> Option<Vec<u8>> {
    if a.is_empty() || a.len() != b.len() || index >= a.len() {
        return None;
    }
    let end = index.checked_add(a.len())?;
    let mut joined = Vec::with_capacity(a.len().checked_mul(2)?);
    joined.extend_from_slice(a);
    joined.extend_from_slice(b);
    (end <= joined.len()).then(|| joined[index..end].to_vec())
}

fn f16_to_f64(bits: u16) -> f64 {
    let sign = (u64::from(bits) & 0x8000) << 48;
    let exp = (bits >> 10) & 0x1F;
    let frac = bits & 0x03FF;
    match exp {
        0 => {
            if frac == 0 {
                f64::from_bits(sign)
            } else {
                let mag = (f64::from(frac)) * (1.0 / ((1u64 << 24) as f64));
                f64::from_bits(mag.to_bits() | sign)
            }
        }
        0x1F => {
            if frac == 0 {
                f64::from_bits(sign | 0x7FF0_0000_0000_0000)
            } else {
                f64::from_bits(sign | 0x7FF0_0000_0000_0000 | (u64::from(frac) << 42))
            }
        }
        _ => {
            let f64_exp = u64::from(exp) + 1008;
            f64::from_bits(sign | (f64_exp << 52) | (u64::from(frac) << 42))
        }
    }
}

fn f64_to_f16(val: f64) -> u16 {
    let bits = val.to_bits();
    let sign = ((bits >> 48) & 0x8000) as u16;
    let exp = ((bits >> 52) & 0x7FF) as i32;
    let frac = bits & ((1u64 << 52) - 1);
    if exp == 0x7FF {
        return if frac == 0 {
            sign | 0x7C00
        } else {
            sign | 0x7E00 | (((frac >> 42) as u16) & 0x03FF)
        };
    }
    if exp == 0 {
        return sign;
    }
    let mut e16 = exp - 1023 + 15;
    if e16 >= 31 {
        return sign | 0x7C00;
    }
    let sig = (1u64 << 52) | frac;
    if e16 >= 1 {
        let mut q = sig >> 42;
        let rem = sig & ((1u64 << 42) - 1);
        let half = 1u64 << 41;
        if rem > half || (rem == half && (q & 1) != 0) {
            q += 1;
            if q == 0x0400 {
                e16 += 1;
                q = 0;
                if e16 >= 31 {
                    return sign | 0x7C00;
                }
            }
        }
        return sign | ((e16 as u16) << 10) | ((q as u16) & 0x03FF);
    }
    let extra = 1 - e16;
    if extra > 11 {
        return sign;
    }
    let shift = 42 + extra as u32;
    let half = 1u64 << (shift - 1);
    let mask = (1u64 << shift) - 1;
    let mut q = sig >> shift;
    let rem = sig & mask;
    if rem > half || (rem == half && (q & 1) != 0) {
        q += 1;
    }
    sign | (q as u16)
}

fn classify_nan(bits: u64, esize: usize) -> Option<(bool, u64)> {
    match esize {
        2 => {
            let b = bits as u16;
            ((b & 0x7FFF) > 0x7C00).then(|| ((b & 0x0200) == 0, u64::from(b | 0x0200)))
        }
        4 => {
            let b = bits as u32;
            ((b & 0x7FFF_FFFF) > 0x7F80_0000)
                .then(|| ((b & 0x0040_0000) == 0, u64::from(b | 0x0040_0000)))
        }
        8 => ((bits & 0x7FFF_FFFF_FFFF_FFFF) > 0x7FF0_0000_0000_0000).then(|| {
            (
                (bits & 0x0008_0000_0000_0000) == 0,
                bits | 0x0008_0000_0000_0000,
            )
        }),
        _ => None,
    }
}

fn fp_to_f64(bits: u64, esize: usize) -> Option<f64> {
    match esize {
        2 => Some(f16_to_f64(bits as u16)),
        4 => Some(f64::from(f32::from_bits(bits as u32))),
        8 => Some(f64::from_bits(bits)),
        _ => None,
    }
}

fn neon_fp_minmax(a: &[u8], b: &[u8], esize: usize, is_max: bool, is_nm: bool) -> Option<Vec<u8>> {
    if a.len() != b.len() || a.is_empty() || a.len() % esize != 0 {
        return None;
    }
    if !matches!(esize, 2 | 4 | 8) {
        return None;
    }
    let sign_bit = 1u64 << (esize * 8 - 1);
    let lanes = a.len() / esize;
    let mut out = vec![0u8; a.len()];
    for i in 0..lanes {
        let ab = read_elem_u64(a, i, esize);
        let bb = read_elem_u64(b, i, esize);
        let picked = match (classify_nan(ab, esize), classify_nan(bb, esize)) {
            (Some((true, aq)), _) => aq,
            (_, Some((true, bq))) => bq,
            (Some((false, aq)), Some((false, _))) => aq,
            (Some((false, aq)), None) => {
                if is_nm {
                    bb
                } else {
                    aq
                }
            }
            (None, Some((false, bq))) => {
                if is_nm {
                    ab
                } else {
                    bq
                }
            }
            (None, None) => {
                let af = fp_to_f64(ab, esize)?;
                let bf = fp_to_f64(bb, esize)?;
                if af == 0.0 && bf == 0.0 {
                    if is_max {
                        if (ab & sign_bit) == 0 { ab } else { bb }
                    } else if (ab & sign_bit) != 0 {
                        ab
                    } else {
                        bb
                    }
                } else if is_max {
                    if af > bf { ab } else { bb }
                } else if af < bf {
                    ab
                } else {
                    bb
                }
            }
        };
        write_elem_u64(&mut out, i, esize, picked);
    }
    Some(out)
}

fn answer_scvtf(emu: &mut Emulator, op: &PcodeOp, out: &Varnode, nargs: usize) -> bool {
    let Ok(rn) = read_bytes(emu, op, 1) else {
        return false;
    };
    let (fbits, esize) = match nargs {
        2 => {
            let Some(vn2) = op.inputs.get(2) else {
                return false;
            };
            let Some(val) = read_const_u64(op, 2) else {
                return false;
            };
            if vn2.size == 1 {
                (0u32, val as usize)
            } else {
                (val as u32, out.size as usize)
            }
        }
        3 => {
            let (Some(fb), Some(es)) = (read_const_u64(op, 2), read_const_u64(op, 3)) else {
                return false;
            };
            let esize = es as usize;
            // Fixed-point 4H/8H uses Imm_shr_imm32 (64 - immh:immb), adding 32 to fracbits.
            let fbits = if esize == 2 && fb > 32 {
                (fb - 32) as u32
            } else {
                fb as u32
            };
            (fbits, esize)
        }
        _ => return false,
    };
    neon_cvtf(&rn, esize, fbits, true)
        .filter(|r| r.len() == out.size as usize)
        .is_some_and(|r| write_neon_result(emu, out, &r))
}

fn answer_ucvtf(emu: &mut Emulator, op: &PcodeOp, out: &Varnode, nargs: usize) -> bool {
    let Ok(rn) = read_bytes(emu, op, 1) else {
        return false;
    };
    let (fbits, esize) = match nargs {
        1 => (0u32, out.size as usize),
        2 => {
            let Some(val) = read_const_u64(op, 2) else {
                return false;
            };
            if out.size == 16 || (out.size == 8 && (val == 2 || val == 4)) {
                (0u32, val as usize)
            } else {
                (val as u32, out.size as usize)
            }
        }
        3 => {
            let (Some(fb), Some(es)) = (read_const_u64(op, 2), read_const_u64(op, 3)) else {
                return false;
            };
            let esize = es as usize;
            let fbits = if esize == 2 && fb > 32 {
                (fb - 32) as u32
            } else {
                fb as u32
            };
            (fbits, esize)
        }
        _ => return false,
    };
    neon_cvtf(&rn, esize, fbits, false)
        .filter(|r| r.len() == out.size as usize)
        .is_some_and(|r| write_neon_result(emu, out, &r))
}

fn neon_cvtf(rn: &[u8], esize: usize, fbits: u32, signed: bool) -> Option<Vec<u8>> {
    if rn.is_empty() || rn.len() % esize != 0 || !matches!(esize, 2 | 4 | 8) {
        return None;
    }
    let scale = if fbits == 0 {
        1.0
    } else {
        2.0f64.powi(-(fbits as i32))
    };
    let lanes = rn.len() / esize;
    let mut out = vec![0u8; rn.len()];
    for i in 0..lanes {
        let raw = read_elem_u64(rn, i, esize);
        let float_val = if signed {
            (sign_extend_esize(raw, esize) as f64) * scale
        } else {
            let unsigned = match esize {
                2 => raw as u16 as u64,
                4 => raw as u32 as u64,
                _ => raw,
            };
            (unsigned as f64) * scale
        };
        let encoded = match esize {
            2 => u64::from(f64_to_f16(float_val)),
            4 => u64::from((float_val as f32).to_bits()),
            _ => float_val.to_bits(),
        };
        write_elem_u64(&mut out, i, esize, encoded);
    }
    Some(out)
}

fn neon_sat_add_sub(
    a: &[u8],
    b: &[u8],
    esize: usize,
    signed: bool,
    is_sub: bool,
) -> Option<Vec<u8>> {
    if a.len() != b.len() || a.is_empty() || a.len() % esize != 0 || !matches!(esize, 1 | 2 | 4 | 8)
    {
        return None;
    }
    let lanes = a.len() / esize;
    let mut out = vec![0u8; a.len()];
    for i in 0..lanes {
        let x = read_elem_u64(a, i, esize);
        let y = read_elem_u64(b, i, esize);
        let res = match (esize, signed, is_sub) {
            (1, true, false) => (x as i8).saturating_add(y as i8) as u64,
            (1, true, true) => (x as i8).saturating_sub(y as i8) as u64,
            (1, false, false) => (x as u8).saturating_add(y as u8) as u64,
            (1, false, true) => (x as u8).saturating_sub(y as u8) as u64,
            (2, true, false) => (x as i16).saturating_add(y as i16) as u64,
            (2, true, true) => (x as i16).saturating_sub(y as i16) as u64,
            (2, false, false) => (x as u16).saturating_add(y as u16) as u64,
            (2, false, true) => (x as u16).saturating_sub(y as u16) as u64,
            (4, true, false) => (x as i32).saturating_add(y as i32) as u32 as u64,
            (4, true, true) => (x as i32).saturating_sub(y as i32) as u32 as u64,
            (4, false, false) => (x as u32).saturating_add(y as u32) as u64,
            (4, false, true) => (x as u32).saturating_sub(y as u32) as u64,
            (8, true, false) => (x as i64).saturating_add(y as i64) as u64,
            (8, true, true) => (x as i64).saturating_sub(y as i64) as u64,
            (8, false, false) => x.saturating_add(y),
            (8, false, true) => x.saturating_sub(y),
            _ => return None,
        };
        write_elem_u64(&mut out, i, esize, res);
    }
    Some(out)
}

#[derive(Clone, Copy)]
enum NarrowSatKind {
    SignedToSigned,
    UnsignedToUnsigned,
    SignedToUnsigned,
}

fn neon_xtn(
    rd: &[u8],
    rn: &[u8],
    src_esize: usize,
    kind: NarrowSatKind,
    is_part2: bool,
) -> Option<Vec<u8>> {
    if !matches!(src_esize, 2 | 4 | 8) || rn.is_empty() || rn.len() % src_esize != 0 {
        return None;
    }
    let dst_esize = src_esize / 2;
    let lanes = rn.len() / src_esize;
    let half_bytes = lanes * dst_esize;
    let (mut out, write_offset) = if is_part2 {
        if rd.len() != half_bytes * 2 {
            return None;
        }
        (rd.to_vec(), half_bytes)
    } else {
        (vec![0u8; half_bytes], 0)
    };
    let dst_bits = (dst_esize * 8) as u32;
    let s_min = -(1i128 << (dst_bits - 1));
    let s_max = (1i128 << (dst_bits - 1)) - 1;
    let u_max = (1i128 << dst_bits) - 1;

    for i in 0..lanes {
        let raw = read_elem_u64(rn, i, src_esize);
        let val = match kind {
            NarrowSatKind::SignedToSigned => {
                (sign_extend_esize(raw, src_esize) as i128).clamp(s_min, s_max) as u64
            }
            NarrowSatKind::UnsignedToUnsigned => {
                let u = match src_esize {
                    2 => raw as u16 as u64,
                    4 => raw as u32 as u64,
                    _ => raw,
                };
                (u as i128).min(u_max) as u64
            }
            NarrowSatKind::SignedToUnsigned => {
                (sign_extend_esize(raw, src_esize) as i128).clamp(0, u_max) as u64
            }
        };
        let slot = &mut out[write_offset + i * dst_esize..write_offset + (i + 1) * dst_esize];
        slot.copy_from_slice(&val.to_le_bytes()[..dst_esize]);
    }
    Some(out)
}

fn neon_shl(a: &[u8], b: &[u8], esize: usize, signed: bool) -> Option<Vec<u8>> {
    if a.len() != b.len() || a.is_empty() || a.len() % esize != 0 || !matches!(esize, 1 | 2 | 4 | 8)
    {
        return None;
    }
    let bits = (esize * 8) as i32;
    let lanes = a.len() / esize;
    let mut out = vec![0u8; a.len()];
    for i in 0..lanes {
        let x = read_elem_u64(a, i, esize);
        let shift = (read_elem_u64(b, i, esize) as u8) as i8 as i32;
        let res = if shift >= 0 {
            if shift >= bits {
                0
            } else {
                x << (shift as u32)
            }
        } else {
            let r = -shift;
            if signed {
                let s = sign_extend_esize(x, esize);
                if r >= bits {
                    if s < 0 { !0u64 } else { 0 }
                } else {
                    (s >> (r as u32)) as u64
                }
            } else if r >= bits {
                0
            } else {
                let mask = if bits == 64 {
                    u64::MAX
                } else {
                    (1u64 << bits) - 1
                };
                (x & mask) >> (r as u32)
            }
        };
        write_elem_u64(&mut out, i, esize, res);
    }
    Some(out)
}

fn neon_rshr(rn: &[u8], shift: u32, esize: usize, signed: bool) -> Option<Vec<u8>> {
    if rn.is_empty() || rn.len() % esize != 0 || !matches!(esize, 1 | 2 | 4 | 8) {
        return None;
    }
    let bits = (esize * 8) as u32;
    if shift == 0 || shift > bits {
        return None;
    }
    let lanes = rn.len() / esize;
    let mut out = vec![0u8; rn.len()];
    for i in 0..lanes {
        let raw = read_elem_u64(rn, i, esize);
        let res = if signed {
            let round = 1i128 << (shift - 1);
            ((sign_extend_esize(raw, esize) as i128 + round) >> shift) as i64 as u64
        } else {
            let round = 1u128 << (shift - 1);
            let mask = if bits == 64 {
                u64::MAX
            } else {
                (1u64 << bits) - 1
            };
            ((u128::from(raw & mask) + round) >> shift) as u64
        };
        write_elem_u64(&mut out, i, esize, res);
    }
    Some(out)
}

fn neon_rshrn(
    rd: &[u8],
    rn: &[u8],
    shift: u32,
    src_esize: usize,
    is_part2: bool,
) -> Option<Vec<u8>> {
    if !matches!(src_esize, 2 | 4 | 8) || rn.is_empty() || rn.len() % src_esize != 0 {
        return None;
    }
    let dst_esize = src_esize / 2;
    let dst_bits = (dst_esize * 8) as u32;
    if shift == 0 || shift > dst_bits {
        return None;
    }
    let lanes = rn.len() / src_esize;
    let half_bytes = lanes * dst_esize;
    let (mut out, write_offset) = if is_part2 {
        if rd.len() != half_bytes * 2 {
            return None;
        }
        (rd.to_vec(), half_bytes)
    } else {
        (vec![0u8; half_bytes], 0)
    };
    let round = 1u128 << (shift - 1);
    for i in 0..lanes {
        let raw = read_elem_u64(rn, i, src_esize);
        let res = (u128::from(raw).wrapping_add(round) >> shift) as u64;
        let slot = &mut out[write_offset + i * dst_esize..write_offset + (i + 1) * dst_esize];
        slot.copy_from_slice(&res.to_le_bytes()[..dst_esize]);
    }
    Some(out)
}

fn neon_sqdmulh(a: &[u8], b: &[u8], esize: usize, rounding: bool) -> Option<Vec<u8>> {
    if a.len() != b.len() || a.is_empty() || a.len() % esize != 0 || !matches!(esize, 2 | 4) {
        return None;
    }
    let bits = (esize * 8) as u32;
    let min_val = -(1i128 << (bits - 1));
    let max_val = (1i128 << (bits - 1)) - 1;
    let round = if rounding { 1i128 << (bits - 1) } else { 0 };
    let lanes = a.len() / esize;
    let mut out = vec![0u8; a.len()];
    for i in 0..lanes {
        let x = sign_extend_esize(read_elem_u64(a, i, esize), esize) as i128;
        let y = sign_extend_esize(read_elem_u64(b, i, esize), esize) as i128;
        let val = ((2 * x * y + round) >> bits).clamp(min_val, max_val) as i64 as u64;
        write_elem_u64(&mut out, i, esize, val);
    }
    Some(out)
}

fn elem_abs_diff(a: u64, b: u64, esize: usize, signed: bool) -> u64 {
    if signed {
        let sa = i128::from(sign_extend_esize(a, esize));
        let sb = i128::from(sign_extend_esize(b, esize));
        (sa - sb).unsigned_abs() as u64
    } else {
        let mask = if esize >= 8 {
            u64::MAX
        } else {
            (1u64 << (esize * 8)) - 1
        };
        (a & mask).abs_diff(b & mask)
    }
}

fn neon_abd(rn: &[u8], rm: &[u8], esize: usize, signed: bool) -> Option<Vec<u8>> {
    if rn.len() != rm.len() || rn.is_empty() || rn.len() % esize != 0 || !matches!(esize, 1 | 2 | 4)
    {
        return None;
    }
    let lanes = rn.len() / esize;
    let mut out = vec![0u8; rn.len()];
    for i in 0..lanes {
        let d = elem_abs_diff(
            read_elem_u64(rn, i, esize),
            read_elem_u64(rm, i, esize),
            esize,
            signed,
        );
        write_elem_u64(&mut out, i, esize, d);
    }
    Some(out)
}

fn neon_aba(rd: &[u8], rn: &[u8], rm: &[u8], esize: usize, signed: bool) -> Option<Vec<u8>> {
    if rd.len() != rn.len()
        || rd.len() != rm.len()
        || rd.is_empty()
        || rd.len() % esize != 0
        || !matches!(esize, 1 | 2 | 4)
    {
        return None;
    }
    let lanes = rd.len() / esize;
    let mut out = vec![0u8; rd.len()];
    for i in 0..lanes {
        let acc = read_elem_u64(rd, i, esize);
        let diff = elem_abs_diff(
            read_elem_u64(rn, i, esize),
            read_elem_u64(rm, i, esize),
            esize,
            signed,
        );
        write_elem_u64(&mut out, i, esize, acc.wrapping_add(diff));
    }
    Some(out)
}

fn neon_addlv(rn: &[u8], esize: usize, signed: bool, out_size: usize) -> Option<Vec<u8>> {
    if rn.is_empty()
        || rn.len() % esize != 0
        || !matches!(esize, 1 | 2 | 4)
        || out_size != esize * 2
    {
        return None;
    }
    let lanes = rn.len() / esize;
    let total = if signed {
        let mut sum = 0i128;
        for i in 0..lanes {
            sum += i128::from(sign_extend_esize(read_elem_u64(rn, i, esize), esize));
        }
        sum as i64 as u64
    } else {
        let mut sum = 0u64;
        for i in 0..lanes {
            sum = sum.wrapping_add(read_elem_u64(rn, i, esize));
        }
        sum
    };
    let mut out = vec![0u8; out_size];
    out.copy_from_slice(&total.to_le_bytes()[..out_size]);
    Some(out)
}

fn neon_rev(rn: &[u8], container: usize, esize: usize) -> Option<Vec<u8>> {
    if rn.is_empty()
        || container == 0
        || esize == 0
        || rn.len() % container != 0
        || container % esize != 0
    {
        return None;
    }
    let elems_per_c = container / esize;
    let mut out = vec![0u8; rn.len()];
    for (c_out, c_in) in out
        .chunks_exact_mut(container)
        .zip(rn.chunks_exact(container))
    {
        for i in 0..elems_per_c {
            let src = &c_in[(elems_per_c - 1 - i) * esize..(elems_per_c - i) * esize];
            c_out[i * esize..(i + 1) * esize].copy_from_slice(src);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn umaxp_takes_the_first_half_from_the_first_vector() {
        let a: Vec<u8> = vec![1, 9, 3, 7, 5, 5, 8, 2];
        let b: Vec<u8> = vec![0, 4, 6, 6, 2, 1, 9, 9];
        let out = pairwise_reduce(&a, &b, 1, Pairwise::UnsignedMax);
        assert_eq!(out, vec![9, 7, 5, 8, 4, 6, 2, 9]);
    }

    #[test]
    fn uminp_is_the_same_shape() {
        let a: Vec<u8> = vec![1, 9, 3, 7, 5, 5, 8, 2];
        let b: Vec<u8> = vec![0, 4, 6, 6, 2, 1, 9, 9];
        let out = pairwise_reduce(&a, &b, 1, Pairwise::UnsignedMin);
        assert_eq!(out, vec![1, 3, 5, 2, 0, 6, 1, 9]);
    }

    #[test]
    fn the_comparison_is_unsigned_unless_it_is_not() {
        let a: Vec<u8> = vec![0xFF, 0x01, 0, 0, 0, 0, 0, 0];
        let b: Vec<u8> = vec![0; 8];
        assert_eq!(
            pairwise_reduce(&a, &b, 1, Pairwise::UnsignedMax)[0],
            0xFF,
            "255 > 1"
        );
        assert_eq!(
            pairwise_reduce(&a, &b, 1, Pairwise::SignedMax)[0],
            0x01,
            "-1 < 1"
        );
    }

    #[test]
    fn wider_elements_keep_their_width() {
        let a: Vec<u8> = vec![0x00, 0x01, 0xFF, 0x00, 0x34, 0x12, 0x00, 0x00];
        let b: Vec<u8> = vec![0; 8];
        let out = pairwise_reduce(&a, &b, 2, Pairwise::UnsignedMax);
        assert_eq!(&out[0..2], &[0x00, 0x01]);
        assert_eq!(&out[2..4], &[0x34, 0x12]);
    }

    #[test]
    fn ext_extracts_a_window_across_the_vector_boundary() {
        let a: Vec<u8> = (0..16).collect();
        let b: Vec<u8> = (16..32).collect();
        assert_eq!(
            extract_bytes(&a, &b, 11).unwrap(),
            (11..27).collect::<Vec<_>>()
        );
    }

    #[test]
    fn ext_supports_the_64_bit_vector_form() {
        let a: Vec<u8> = (0..8).collect();
        let b: Vec<u8> = (8..16).collect();
        assert_eq!(
            extract_bytes(&a, &b, 7).unwrap(),
            (7..15).collect::<Vec<_>>()
        );
    }

    #[test]
    fn ext_rejects_out_of_range_or_mismatched_vectors() {
        let a: Vec<u8> = (0..16).collect();
        let b: Vec<u8> = (16..32).collect();
        assert!(extract_bytes(&a, &b, 16).is_none());
        assert!(extract_bytes(&a, &b[..8], 0).is_none());
        assert!(extract_bytes(&[], &[], 0).is_none());
    }

    #[test]
    fn fminnm_prefers_numbers_over_qnan_but_propagates_snan() {
        let qnan = 0x7FC0_0001u32;
        let snan = 0x7F80_0001u32;
        let two = 2.0f32.to_bits();
        let out = neon_fp_minmax(&qnan.to_le_bytes(), &two.to_le_bytes(), 4, false, true).unwrap();
        assert_eq!(out, two.to_le_bytes());

        let out_snan =
            neon_fp_minmax(&snan.to_le_bytes(), &two.to_le_bytes(), 4, false, true).unwrap();
        assert_eq!(out_snan, (snan | 0x0040_0000).to_le_bytes());

        // -0.0 vs +0.0: fmin returns -0.0, fmax returns +0.0
        let nz = (-0.0f32).to_bits().to_le_bytes();
        let pz = 0.0f32.to_bits().to_le_bytes();
        assert_eq!(neon_fp_minmax(&pz, &nz, 4, false, false).unwrap(), nz);
        assert_eq!(neon_fp_minmax(&nz, &pz, 4, true, false).unwrap(), pz);
    }

    #[test]
    fn rshr_rshrn_sqrdmulh_and_addlv_match_arm_semantics() {
        // urshr u64 shift 64: 0x8000... -> 1, 0x7FFF... -> 0
        let hi = (1u64 << 63).to_le_bytes();
        let lo = ((1u64 << 63) - 1).to_le_bytes();
        assert_eq!(
            u64::from_le_bytes(neon_rshr(&hi, 64, 8, false).unwrap().try_into().unwrap()),
            1
        );
        assert_eq!(
            u64::from_le_bytes(neon_rshr(&lo, 64, 8, false).unwrap().try_into().unwrap()),
            0
        );

        // sqrdmulh saturates INT32_MIN * INT32_MIN -> INT32_MAX
        let min_i32 = i32::MIN.to_le_bytes();
        let prod = neon_sqdmulh(&min_i32, &min_i32, 4, true).unwrap();
        assert_eq!(i32::from_le_bytes(prod.try_into().unwrap()), i32::MAX);

        // uaddlv across 16x 0xFF bytes -> 16 * 255 = 4080
        let bytes = [0xFFu8; 16];
        let sum = neon_addlv(&bytes, 1, false, 2).unwrap();
        assert_eq!(u16::from_le_bytes(sum.try_into().unwrap()), 4080);

        // sqxtun clamps negative values to 0
        let rn = (-5i32).to_le_bytes();
        let narrow = neon_xtn(&[0u8; 2], &rn, 4, NarrowSatKind::SignedToUnsigned, false).unwrap();
        assert_eq!(narrow, vec![0, 0]);
    }
}
