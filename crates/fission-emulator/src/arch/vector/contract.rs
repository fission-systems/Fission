//! Shared SIMD register-view, lane-layout, and upper-bit write contracts.
//!
//! SLEIGH vector `CALLOTHER`s and sub-register writes must agree on:
//! 1. Checked operand widths, element byte sizes, lane counts, and endianness.
//! 2. Architectural upper-bit policies when writing a narrow view (`S`, `D`,
//!    64-bit `V` arrangements, MMX, scalar SSE, legacy XMM vs VEX-128).
//! 3. Dynamic resolution of enclosing register storage from the active
//!    language's `register_map` rather than hardcoded register offsets.

use std::fmt;

use fission_pcode_ir::ir::Varnode;

use crate::arch::Endianness;
use crate::core::Emulator;

/// Target SIMD ISA family owning a vector operation contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VectorIsa {
    X86,
    AArch64,
}

/// Encoding / register-bank update regime for a SIMD instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VectorEncoding {
    X86Mmx,
    X86LegacySse,
    X86Vex128,
    AArch64NeonScalar,
    AArch64NeonVector64,
    AArch64NeonVector128,
}

/// Architectural view width within an enclosing SIMD register bank.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VectorViewKind {
    Mmx64,
    Xmm128,
    Ymm256,
    Zmm512,
    NeonB8,
    NeonH16,
    NeonS32,
    NeonD64,
    NeonQ128,
}

impl VectorViewKind {
    /// Width in bytes of this architectural view.
    #[must_use]
    pub const fn byte_len(self) -> usize {
        match self {
            Self::NeonB8 => 1,
            Self::NeonH16 => 2,
            Self::NeonS32 => 4,
            Self::Mmx64 | Self::NeonD64 => 8,
            Self::Xmm128 | Self::NeonQ128 => 16,
            Self::Ymm256 => 32,
            Self::Zmm512 => 64,
        }
    }

    /// Full architectural register size enclosing this view (`ZMM` omitted
    /// until modeled; `YMM` for x86-64 AVX-capable views, `Q` for AArch64 NEON).
    #[must_use]
    pub const fn full_bank_bytes(self) -> usize {
        match self {
            Self::Mmx64 => 8,
            Self::Xmm128 => 16,
            Self::Ymm256 => 32,
            Self::Zmm512 => 64,
            Self::NeonB8 | Self::NeonH16 | Self::NeonS32 | Self::NeonD64 | Self::NeonQ128 => 16,
        }
    }
}

/// How bytes outside the actively written lanes/view of a vector destination
/// register behave upon writeback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UpperBitsPolicy {
    /// Write `[0..out.size]` and preserve any upper bits of an enclosing
    /// architectural register (e.g. x86 legacy SSE preserving `YMM[255:128]`).
    PreserveUpper,
    /// Write `[0..out.size]` and zero `[out.size..full_reg_bytes]` within the
    /// enclosing register (Arm ARM A1.4 `V[t, width] = ZeroExtend(val, 128)`,
    /// or x86 VEX-128 zeroing `YMM[255:128]`).
    ZeroUpperTo { full_reg_bytes: usize },
    /// Scalar SSE update: modify `[0..active_low_bytes]`, preserve remaining
    /// lanes `[active_low_bytes..out.size]` and any enclosing upper bits.
    MergeLowPreserveUpper { active_low_bytes: usize },
    /// Two-part NEON write (`SQXTN2`/`UQXTN2`/`SQXTUN2`/`RSHRN2`): preserve
    /// `[0..low_preserve_bytes]` and write `[low_preserve_bytes..out.size]`.
    MergeHighHalf { low_preserve_bytes: usize },
    /// Lane-masked / per-byte zeroing SSE write (`pshufb`, `insertps`).
    ZeroMaskedLanes,
    /// Explicitly unmodeled upper-bit behavior; execution refuses rather than
    /// silently corrupting upper lanes.
    UnknownUpper,
}

/// Checked lane geometry and endianness for one vector operation instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VectorShape {
    pub total_bytes: usize,
    pub elem_bytes: usize,
    pub lanes: usize,
    pub endian: Endianness,
    pub is_scalar: bool,
}

impl VectorShape {
    /// Construct and validate a vector operand geometry against allowed total widths.
    pub fn checked(
        total_bytes: usize,
        elem_bytes: usize,
        allowed_totals: &[usize],
        endian: Endianness,
        is_scalar: bool,
    ) -> Result<Self, VectorContractError> {
        if endian != Endianness::Little {
            return Err(VectorContractError::UnsupportedEndianness(endian));
        }
        if total_bytes == 0
            || elem_bytes == 0
            || !total_bytes.is_multiple_of(elem_bytes)
            || !allowed_totals.contains(&total_bytes)
        {
            return Err(VectorContractError::UnsupportedShape {
                total_bytes,
                elem_bytes,
            });
        }
        Ok(Self {
            total_bytes,
            elem_bytes,
            lanes: total_bytes / elem_bytes,
            endian,
            is_scalar,
        })
    }
}

/// Enclosing architectural vector register discovered from SLEIGH `register_map`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorRegisterSlot {
    pub canonical_name: String,
    pub space_id: u64,
    pub offset: u64,
    pub full_bytes: usize,
    pub isa: VectorIsa,
}

/// Stateless lookup over an [`Emulator`]'s SLEIGH register map to resolve the
/// full enclosing vector register for any narrow/wide alias varnode.
pub struct VectorRegisterBank;

impl VectorRegisterBank {
    /// Find the widest architectural vector register in `emu.register_map`
    /// rooted at `out.offset` within `emu.state.register_space()`.
    #[must_use]
    pub fn resolve_enclosing_slot(
        emu: &Emulator,
        out: &Varnode,
        isa: VectorIsa,
    ) -> Option<VectorRegisterSlot> {
        let reg_space = emu.state.register_space();
        if out.space_id != reg_space {
            return None;
        }
        let mut best: Option<VectorRegisterSlot> = None;
        for (name, &(space_id, offset, size)) in &emu.register_map {
            if space_id != reg_space || offset != out.offset {
                continue;
            }
            let size_usize = size as usize;
            let matches_bank = match isa {
                VectorIsa::AArch64 => Self::is_aarch64_q_reg(name) && size_usize == 16,
                VectorIsa::X86 => {
                    Self::is_x86_vec_reg(name) && matches!(size_usize, 8 | 16 | 32 | 64)
                }
            };
            if !matches_bank {
                continue;
            }
            let replace = best
                .as_ref()
                .is_none_or(|prev| size_usize > prev.full_bytes);
            if replace {
                best = Some(VectorRegisterSlot {
                    canonical_name: name.clone(),
                    space_id,
                    offset,
                    full_bytes: size_usize,
                    isa,
                });
            }
        }
        best
    }

    fn is_aarch64_q_reg(name: &str) -> bool {
        let lower = name.to_ascii_lowercase();
        let Some(rest) = lower.strip_prefix('q') else {
            return false;
        };
        matches!(rest.parse::<u8>(), Ok(idx) if idx < 32)
    }

    fn is_x86_vec_reg(name: &str) -> bool {
        let upper = name.to_ascii_uppercase();
        if let Some(rest) = upper
            .strip_prefix("ZMM")
            .or_else(|| upper.strip_prefix("YMM"))
            .or_else(|| upper.strip_prefix("XMM"))
        {
            return matches!(rest.parse::<u8>(), Ok(idx) if idx < 32);
        }
        if let Some(rest) = upper.strip_prefix("MM") {
            return matches!(rest.parse::<u8>(), Ok(idx) if idx < 8);
        }
        false
    }
}

/// Static declaration of a supported SIMD `CALLOTHER` userop's shape and
/// upper-bit policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VectorOpContract {
    pub name: &'static str,
    pub isa: VectorIsa,
    pub allowed_total_bytes: &'static [usize],
    pub policy: UpperBitsPolicy,
    pub supports_src_dst_alias: bool,
}

/// Look up the execution & writeback contract for a known SIMD userop name.
#[must_use]
pub fn describe_vector_op(name: &str, out_size: usize) -> Option<VectorOpContract> {
    match name {
        // ── AArch64 NEON / AdvSIMD ───────────────────────────────────────────
        "NEON_umaxp" | "NEON_uminp" | "NEON_smaxp" | "NEON_sminp" | "NEON_ext" | "NEON_saba"
        | "NEON_uaba" | "NEON_uabd" | "NEON_sabd" | "NEON_rev64" | "NEON_rev32" | "NEON_rev16" => {
            Some(VectorOpContract {
                name: canonical_static_name(name),
                isa: VectorIsa::AArch64,
                allowed_total_bytes: &[8, 16],
                policy: UpperBitsPolicy::ZeroUpperTo { full_reg_bytes: 16 },
                supports_src_dst_alias: true,
            })
        }
        "NEON_fmin" | "NEON_fmax" | "NEON_fminnm" | "NEON_fmaxnm" | "NEON_scvtf" | "NEON_ucvtf"
        | "NEON_sqadd" | "NEON_uqadd" | "NEON_sqsub" | "NEON_uqsub" | "NEON_sshl" | "NEON_ushl"
        | "NEON_urshr" | "NEON_srshr" | "NEON_sqrdmulh" | "NEON_sqdmulh" | "NEON_uaddlv"
        | "NEON_saddlv" => Some(VectorOpContract {
            name: canonical_static_name(name),
            isa: VectorIsa::AArch64,
            allowed_total_bytes: &[1, 2, 4, 8, 16],
            policy: UpperBitsPolicy::ZeroUpperTo { full_reg_bytes: 16 },
            supports_src_dst_alias: true,
        }),
        "NEON_sqxtn" | "NEON_uqxtn" | "NEON_sqxtun" | "NEON_rshrn" => Some(VectorOpContract {
            name: canonical_static_name(name),
            isa: VectorIsa::AArch64,
            allowed_total_bytes: &[4, 8],
            policy: UpperBitsPolicy::ZeroUpperTo { full_reg_bytes: 16 },
            supports_src_dst_alias: true,
        }),
        "NEON_sqxtn2" | "NEON_uqxtn2" | "NEON_sqxtun2" | "NEON_rshrn2" => Some(VectorOpContract {
            name: canonical_static_name(name),
            isa: VectorIsa::AArch64,
            allowed_total_bytes: &[16],
            policy: UpperBitsPolicy::MergeHighHalf {
                low_preserve_bytes: 8,
            },
            supports_src_dst_alias: true,
        }),

        // ── x86 / x86-64 SSE, SSSE3, SSE4.1, MMX ────────────────────────────
        "roundss" | "rcpss" | "rsqrtss" => Some(VectorOpContract {
            name: canonical_static_name(name),
            isa: VectorIsa::X86,
            allowed_total_bytes: &[16],
            policy: UpperBitsPolicy::MergeLowPreserveUpper {
                active_low_bytes: 4,
            },
            supports_src_dst_alias: true,
        }),
        "roundsd" => Some(VectorOpContract {
            name: "roundsd",
            isa: VectorIsa::X86,
            allowed_total_bytes: &[16],
            policy: UpperBitsPolicy::MergeLowPreserveUpper {
                active_low_bytes: 8,
            },
            supports_src_dst_alias: true,
        }),
        "pshufb" | "insertps" => Some(VectorOpContract {
            name: canonical_static_name(name),
            isa: VectorIsa::X86,
            allowed_total_bytes: if name == "pshufb" { &[8, 16] } else { &[16] },
            policy: UpperBitsPolicy::ZeroMaskedLanes,
            supports_src_dst_alias: true,
        }),
        "paddsb" | "paddsw" | "paddusb" | "paddusw" | "psubsb" | "psubsw" | "psubusb"
        | "psubusw" | "pmulhw" | "pmulhuw" | "pmulhrsw" | "pmaddwd" | "pmaddubsw" | "psadbw"
        | "phaddw" | "phaddd" | "phaddsw" | "phsubw" | "phsubd" | "phsubsw" | "psignb"
        | "psignw" | "psignd" | "pabsb" | "pabsw" | "pabsd" | "psllw" | "psraw" | "packsswb"
        | "packssdw" | "packusdw" | "packuswb" => Some(VectorOpContract {
            name: canonical_static_name(name),
            isa: VectorIsa::X86,
            allowed_total_bytes: &[8, 16],
            policy: UpperBitsPolicy::PreserveUpper,
            supports_src_dst_alias: true,
        }),
        "pavgb" | "pavgw" => Some(VectorOpContract {
            name: canonical_static_name(name),
            isa: VectorIsa::X86,
            // SLEIGH lifts PAVGB/PAVGW per scalar byte/word lane.
            allowed_total_bytes: &[1, 2],
            policy: UpperBitsPolicy::PreserveUpper,
            supports_src_dst_alias: true,
        }),
        "movmskps" | "movmskpd" | "extractps" => Some(VectorOpContract {
            name: canonical_static_name(name),
            isa: VectorIsa::X86,
            allowed_total_bytes: &[4, 8],
            policy: if out_size == 8 {
                UpperBitsPolicy::PreserveUpper
            } else {
                UpperBitsPolicy::ZeroUpperTo { full_reg_bytes: 8 }
            },
            supports_src_dst_alias: true,
        }),
        "roundps" | "roundpd" | "pshuflw" | "pshufhw" | "pmulld" | "pmuldq" | "mpsadbw"
        | "pmovsxbw" | "pmovsxbd" | "pmovsxbq" | "pmovsxwd" | "pmovsxwq" | "pmovsxdq"
        | "pmovzxbw" | "pmovzxbd" | "pmovzxbq" | "pmovzxwd" | "pmovzxwq" | "pmovzxdq"
        | "pblendw" | "blendps" | "blendpd" | "dpps" | "dppd" | "phminposuw" | "divps"
        | "divpd" | "sqrtps" | "sqrtpd" | "minps" | "maxps" | "minpd" | "maxpd" | "rcpps"
        | "rsqrtps" => Some(VectorOpContract {
            name: canonical_static_name(name),
            isa: VectorIsa::X86,
            allowed_total_bytes: &[16],
            policy: UpperBitsPolicy::PreserveUpper,
            supports_src_dst_alias: true,
        }),
        _ => None,
    }
}

fn canonical_static_name(name: &str) -> &'static str {
    // Only called on known literals; keep a small lookup helper cleanly typed.
    match name {
        "NEON_umaxp" => "NEON_umaxp",
        "NEON_uminp" => "NEON_uminp",
        "NEON_smaxp" => "NEON_smaxp",
        "NEON_sminp" => "NEON_sminp",
        "NEON_ext" => "NEON_ext",
        "NEON_saba" => "NEON_saba",
        "NEON_uaba" => "NEON_uaba",
        "NEON_uabd" => "NEON_uabd",
        "NEON_sabd" => "NEON_sabd",
        "NEON_rev64" => "NEON_rev64",
        "NEON_rev32" => "NEON_rev32",
        "NEON_rev16" => "NEON_rev16",
        "NEON_fmin" => "NEON_fmin",
        "NEON_fmax" => "NEON_fmax",
        "NEON_fminnm" => "NEON_fminnm",
        "NEON_fmaxnm" => "NEON_fmaxnm",
        "NEON_scvtf" => "NEON_scvtf",
        "NEON_ucvtf" => "NEON_ucvtf",
        "NEON_sqadd" => "NEON_sqadd",
        "NEON_uqadd" => "NEON_uqadd",
        "NEON_sqsub" => "NEON_sqsub",
        "NEON_uqsub" => "NEON_uqsub",
        "NEON_sshl" => "NEON_sshl",
        "NEON_ushl" => "NEON_ushl",
        "NEON_urshr" => "NEON_urshr",
        "NEON_srshr" => "NEON_srshr",
        "NEON_sqrdmulh" => "NEON_sqrdmulh",
        "NEON_sqdmulh" => "NEON_sqdmulh",
        "NEON_uaddlv" => "NEON_uaddlv",
        "NEON_saddlv" => "NEON_saddlv",
        "NEON_sqxtn" => "NEON_sqxtn",
        "NEON_uqxtn" => "NEON_uqxtn",
        "NEON_sqxtun" => "NEON_sqxtun",
        "NEON_rshrn" => "NEON_rshrn",
        "NEON_sqxtn2" => "NEON_sqxtn2",
        "NEON_uqxtn2" => "NEON_uqxtn2",
        "NEON_sqxtun2" => "NEON_sqxtun2",
        "NEON_rshrn2" => "NEON_rshrn2",
        "roundss" => "roundss",
        "rcpss" => "rcpss",
        "rsqrtss" => "rsqrtss",
        "pshufb" => "pshufb",
        "insertps" => "insertps",
        "paddsb" => "paddsb",
        "paddsw" => "paddsw",
        "paddusb" => "paddusb",
        "paddusw" => "paddusw",
        "psubsb" => "psubsb",
        "psubsw" => "psubsw",
        "psubusb" => "psubusb",
        "psubusw" => "psubusw",
        "pmulhw" => "pmulhw",
        "pmulhuw" => "pmulhuw",
        "pmulhrsw" => "pmulhrsw",
        "pmaddwd" => "pmaddwd",
        "pmaddubsw" => "pmaddubsw",
        "psadbw" => "psadbw",
        "phaddw" => "phaddw",
        "phaddd" => "phaddd",
        "phaddsw" => "phaddsw",
        "phsubw" => "phsubw",
        "phsubd" => "phsubd",
        "phsubsw" => "phsubsw",
        "psignb" => "psignb",
        "psignw" => "psignw",
        "psignd" => "psignd",
        "pabsb" => "pabsb",
        "pabsw" => "pabsw",
        "pabsd" => "pabsd",
        "psllw" => "psllw",
        "psraw" => "psraw",
        "packsswb" => "packsswb",
        "packssdw" => "packssdw",
        "packusdw" => "packusdw",
        "packuswb" => "packuswb",
        "pavgb" => "pavgb",
        "pavgw" => "pavgw",
        "movmskps" => "movmskps",
        "movmskpd" => "movmskpd",
        "extractps" => "extractps",
        "roundps" => "roundps",
        "roundpd" => "roundpd",
        "pshuflw" => "pshuflw",
        "pshufhw" => "pshufhw",
        "pmulld" => "pmulld",
        "pmuldq" => "pmuldq",
        "mpsadbw" => "mpsadbw",
        "pmovsxbw" => "pmovsxbw",
        "pmovsxbd" => "pmovsxbd",
        "pmovsxbq" => "pmovsxbq",
        "pmovsxwd" => "pmovsxwd",
        "pmovsxwq" => "pmovsxwq",
        "pmovsxdq" => "pmovsxdq",
        "pmovzxbw" => "pmovzxbw",
        "pmovzxbd" => "pmovzxbd",
        "pmovzxbq" => "pmovzxbq",
        "pmovzxwd" => "pmovzxwd",
        "pmovzxwq" => "pmovzxwq",
        "pmovzxdq" => "pmovzxdq",
        "pblendw" => "pblendw",
        "blendps" => "blendps",
        "blendpd" => "blendpd",
        "dpps" => "dpps",
        "dppd" => "dppd",
        "phminposuw" => "phminposuw",
        "divps" => "divps",
        "divpd" => "divpd",
        "sqrtps" => "sqrtps",
        "sqrtpd" => "sqrtpd",
        "minps" => "minps",
        "maxps" => "maxps",
        "minpd" => "minpd",
        "maxpd" => "maxpd",
        "rcpps" => "rcpps",
        "rsqrtps" => "rsqrtps",
        _ => "unknown",
    }
}

/// Summary of bytes updated and upper-bit policy enforced during a write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VectorWriteAudit {
    pub written_bytes: usize,
    pub zeroed_upper_bytes: usize,
    pub preserved_upper_bytes: usize,
    pub policy: UpperBitsPolicy,
}

/// Error returned when a SIMD shape or writeback violates the contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VectorContractError {
    UnsupportedEndianness(Endianness),
    UnsupportedShape {
        total_bytes: usize,
        elem_bytes: usize,
    },
    ExactWidthMismatch {
        expected: usize,
        actual: usize,
    },
    UnknownUpperPolicy,
    StorageWriteFailed,
}

impl fmt::Display for VectorContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedEndianness(e) => write!(f, "unsupported SIMD endianness: {e:?}"),
            Self::UnsupportedShape {
                total_bytes,
                elem_bytes,
            } => write!(
                f,
                "unsupported SIMD shape: total_bytes={total_bytes}, elem_bytes={elem_bytes}"
            ),
            Self::ExactWidthMismatch { expected, actual } => {
                write!(
                    f,
                    "SIMD width mismatch: expected {expected}B, got {actual}B"
                )
            }
            Self::UnknownUpperPolicy => {
                write!(f, "refusing vector write with unmodeled upper-bits policy")
            }
            Self::StorageWriteFailed => write!(f, "failed writing SIMD bytes to emulator space"),
        }
    }
}

impl std::error::Error for VectorContractError {}

/// Write `data` to `out` under the explicit `policy`, resolving any enclosing
/// architectural vector register dynamically via [`VectorRegisterBank`].
pub fn apply_vector_write(
    emu: &mut Emulator,
    out: &Varnode,
    data: &[u8],
    isa: VectorIsa,
    policy: UpperBitsPolicy,
) -> Result<VectorWriteAudit, VectorContractError> {
    if emu.arch.endian != Endianness::Little {
        return Err(VectorContractError::UnsupportedEndianness(emu.arch.endian));
    }
    let out_size = out.size as usize;
    if data.len() != out_size {
        return Err(VectorContractError::ExactWidthMismatch {
            expected: out_size,
            actual: data.len(),
        });
    }
    if matches!(policy, UpperBitsPolicy::UnknownUpper) {
        return Err(VectorContractError::UnknownUpperPolicy);
    }

    emu.state
        .write_space(out.space_id, out.offset, data)
        .map_err(|_| VectorContractError::StorageWriteFailed)?;

    let slot = VectorRegisterBank::resolve_enclosing_slot(emu, out, isa);
    let slot_full = slot.as_ref().map_or(out_size, |s| s.full_bytes);

    match policy {
        UpperBitsPolicy::ZeroUpperTo { full_reg_bytes } => {
            let target_full = full_reg_bytes.min(slot_full);
            if slot.is_some() && out_size < target_full {
                let zero_len = target_full - out_size;
                let zeros = vec![0u8; zero_len];
                emu.state
                    .write_space(out.space_id, out.offset + out_size as u64, &zeros)
                    .map_err(|_| VectorContractError::StorageWriteFailed)?;
                Ok(VectorWriteAudit {
                    written_bytes: out_size,
                    zeroed_upper_bytes: zero_len,
                    preserved_upper_bytes: slot_full.saturating_sub(target_full),
                    policy,
                })
            } else {
                Ok(VectorWriteAudit {
                    written_bytes: out_size,
                    zeroed_upper_bytes: 0,
                    preserved_upper_bytes: slot_full.saturating_sub(out_size),
                    policy,
                })
            }
        }
        UpperBitsPolicy::PreserveUpper
        | UpperBitsPolicy::MergeLowPreserveUpper { .. }
        | UpperBitsPolicy::MergeHighHalf { .. }
        | UpperBitsPolicy::ZeroMaskedLanes => Ok(VectorWriteAudit {
            written_bytes: out_size,
            zeroed_upper_bytes: 0,
            preserved_upper_bytes: slot_full.saturating_sub(out_size),
            policy,
        }),
        UpperBitsPolicy::UnknownUpper => Err(VectorContractError::UnknownUpperPolicy),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_vector_shape_rejects_big_endian_and_invalid_geometries() {
        assert_eq!(
            VectorShape::checked(16, 4, &[8, 16], Endianness::Big, false),
            Err(VectorContractError::UnsupportedEndianness(Endianness::Big))
        );
        assert_eq!(
            VectorShape::checked(32, 4, &[8, 16], Endianness::Little, false),
            Err(VectorContractError::UnsupportedShape {
                total_bytes: 32,
                elem_bytes: 4
            })
        );
        assert_eq!(
            VectorShape::checked(16, 6, &[8, 16], Endianness::Little, false),
            Err(VectorContractError::UnsupportedShape {
                total_bytes: 16,
                elem_bytes: 6
            })
        );
        let ok = VectorShape::checked(16, 4, &[8, 16], Endianness::Little, false).unwrap();
        assert_eq!(ok.lanes, 4);
    }

    #[test]
    fn every_supported_vector_userop_has_an_explicit_contract() {
        for neon in [
            "NEON_umaxp",
            "NEON_ext",
            "NEON_fminnm",
            "NEON_sqxtn",
            "NEON_sqxtn2",
            "NEON_rshrn2",
            "NEON_uaddlv",
            "NEON_rev32",
        ] {
            let c = describe_vector_op(neon, 16).unwrap_or_else(|| panic!("missing {neon}"));
            assert_eq!(c.isa, VectorIsa::AArch64);
            assert_ne!(c.policy, UpperBitsPolicy::UnknownUpper);
        }
        for sse in [
            "roundss", "roundsd", "roundps", "pshufb", "pblendw", "insertps", "packsswb", "divps",
            "movmskps", "pavgb",
        ] {
            let c = describe_vector_op(sse, 16).unwrap_or_else(|| panic!("missing {sse}"));
            assert_eq!(c.isa, VectorIsa::X86);
            assert_ne!(c.policy, UpperBitsPolicy::UnknownUpper);
        }
        assert!(describe_vector_op("unmodeled_avx512_op", 64).is_none());
    }

    #[test]
    fn view_kinds_report_exact_widths_and_bank_sizes() {
        assert_eq!(VectorViewKind::NeonD64.byte_len(), 8);
        assert_eq!(VectorViewKind::NeonD64.full_bank_bytes(), 16);
        assert_eq!(VectorViewKind::Xmm128.byte_len(), 16);
        assert_eq!(VectorViewKind::Ymm256.full_bank_bytes(), 32);
    }
}
