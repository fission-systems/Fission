//! The architectural state both sides start from and are compared on.

use crate::rng::Rng;

/// Bytes of scratch memory that memory operands point into. Both sides map
/// it at the same address, so a pointer means the same thing to each.
pub const BUF_LEN: usize = 16384;

/// One machine's observable state, in a layout shared by both ISAs.
///
/// `gpr`, `flags` and `vec` are indexed by the ISA's tables in [`Isa`]; the
/// native side and the Fission side both read and write them by those
/// indices, so the comparison cannot pair the wrong registers.
#[derive(Clone)]
pub struct CpuState {
    pub gpr: [u64; 32],
    /// One byte per flag: 0 or 1 natively; whatever the p-code left in
    /// Fission's flag register (anything but 0/1 is itself a divergence).
    pub flags: [u8; 8],
    pub vec: [[u8; 16]; 32],
    pub buf: Vec<u8>,
}

impl CpuState {
    pub fn zeroed() -> Self {
        Self {
            gpr: [0; 32],
            flags: [0; 8],
            vec: [[0; 16]; 32],
            buf: vec![0; BUF_LEN],
        }
    }
}

/// Register names (as Fission's register map spells them) in the order the
/// native stub stores them.
pub struct Isa {
    pub name: &'static str,
    pub gpr: &'static [&'static str],
    pub flags: &'static [&'static str],
    pub vec: &'static [&'static str],
    /// A control register both sides hold at a fixed value (MXCSR / FPCR),
    /// with the value. It is set, not compared: see `docs/CI_RELEASE_GATES.md`.
    pub control: (&'static str, u64),
    /// GPRs never compared, by index, with why (e.g. macOS's x18).
    pub ignored_gprs: u32,
}

/// Fields the architecture manual leaves undefined for one instruction.
#[derive(Clone, Copy, Default)]
pub struct Mask {
    /// Bit `i` masks `Isa::flags[i]`.
    pub flags: u8,
    /// Bit `i` masks `Isa::gpr[i]`.
    pub gprs: u32,
    /// The scratch buffer (a memory destination the manual leaves undefined).
    pub mem: bool,
}

impl Mask {
    pub fn union(self, other: Mask) -> Mask {
        Mask {
            flags: self.flags | other.flags,
            gprs: self.gprs | other.gprs,
            mem: self.mem || other.mem,
        }
    }
}

#[derive(Clone, Debug)]
pub struct FieldDiff {
    pub field: String,
    pub input: String,
    pub native: String,
    pub fission: String,
}

fn hex16(v: &[u8; 16]) -> String {
    format!("{:032x}", u128::from_le_bytes(*v))
}

/// Every unmasked difference between the two final states.
pub fn compare(
    isa: &Isa,
    mask: Mask,
    input: &CpuState,
    native: &CpuState,
    fission: &CpuState,
) -> Vec<FieldDiff> {
    let mut out = Vec::new();
    for (i, name) in isa.gpr.iter().enumerate() {
        if (mask.gprs | isa.ignored_gprs) & (1 << i) != 0 {
            continue;
        }
        if native.gpr[i] != fission.gpr[i] {
            out.push(FieldDiff {
                field: (*name).to_string(),
                input: format!("{:#x}", input.gpr[i]),
                native: format!("{:#x}", native.gpr[i]),
                fission: format!("{:#x}", fission.gpr[i]),
            });
        }
    }
    for (i, name) in isa.flags.iter().enumerate() {
        if mask.flags & (1 << i) != 0 {
            continue;
        }
        if native.flags[i] != fission.flags[i] {
            out.push(FieldDiff {
                field: (*name).to_string(),
                input: input.flags[i].to_string(),
                native: native.flags[i].to_string(),
                fission: fission.flags[i].to_string(),
            });
        }
    }
    for (i, name) in isa.vec.iter().enumerate() {
        if native.vec[i] != fission.vec[i] {
            // Both sides produced a NaN and only the payload differs: kept
            // apart, because which NaN propagates is one evaluator question
            // (#1055), not a defect of each instruction.
            let suffix = if nan_payload_only(&native.vec[i], &fission.vec[i]) {
                "~nan"
            } else {
                ""
            };
            out.push(FieldDiff {
                field: format!("{name}{suffix}"),
                input: hex16(&input.vec[i]),
                native: hex16(&native.vec[i]),
                fission: hex16(&fission.vec[i]),
            });
        }
    }
    if native.buf != fission.buf && !mask.mem {
        let first = (0..BUF_LEN)
            .find(|&i| native.buf[i] != fission.buf[i])
            .unwrap_or(0);
        let last = (0..BUF_LEN)
            .rev()
            .find(|&i| native.buf[i] != fission.buf[i])
            .unwrap_or(first);
        let end = (last + 1).min(first + 16);
        let show = |b: &[u8]| {
            b[first..end]
                .iter()
                .map(|x| format!("{x:02x}"))
                .collect::<String>()
        };
        out.push(FieldDiff {
            field: "mem".to_string(),
            input: format!("buf+{first:#x}..{:#x}: {}", last + 1, show(&input.buf)),
            native: show(&native.buf),
            fission: show(&fission.buf),
        });
    }
    out
}

/// Whether every differing lane, read as f32 lanes or as f64 lanes, holds a
/// NaN on both sides.
fn nan_payload_only(a: &[u8; 16], b: &[u8; 16]) -> bool {
    let f32_lanes = (0..4).all(|l| {
        let x = u32::from_le_bytes(a[4 * l..4 * l + 4].try_into().unwrap());
        let y = u32::from_le_bytes(b[4 * l..4 * l + 4].try_into().unwrap());
        x == y || (f32::from_bits(x).is_nan() && f32::from_bits(y).is_nan())
    });
    let f64_lanes = (0..2).all(|l| {
        let x = u64::from_le_bytes(a[8 * l..8 * l + 8].try_into().unwrap());
        let y = u64::from_le_bytes(b[8 * l..8 * l + 8].try_into().unwrap());
        x == y || (f64::from_bits(x).is_nan() && f64::from_bits(y).is_nan())
    });
    f32_lanes || f64_lanes
}

// ── Value generation ────────────────────────────────────────────────────────

const INT_SPECIALS: &[u64] = &[
    0,
    1,
    2,
    u64::MAX,
    0x7F,
    0x80,
    0xFF,
    0x7FFF,
    0x8000,
    0xFFFF,
    0x7FFF_FFFF,
    0x8000_0000,
    0xFFFF_FFFF,
    0x1_0000_0000,
    0x7FFF_FFFF_FFFF_FFFF,
    0x8000_0000_0000_0000,
    0xFFFF_FFFF_8000_0000,
    31,
    32,
    63,
    64,
];

const F32_SPECIALS: &[u32] = &[
    0x0000_0000, // +0
    0x8000_0000, // -0
    0x3F80_0000, // 1
    0xBF80_0000, // -1
    0x3F00_0000, // 0.5
    0xBF00_0000, // -0.5
    0x3FC0_0000, // 1.5
    0x4020_0000, // 2.5
    0xC020_0000, // -2.5
    0x4040_0000, // 3.0
    0x3EAA_AAAB, // 1/3
    0x4B00_0000, // 2^23
    0x4F00_0000, // 2^31
    0xCF00_0000, // -2^31
    0x4F80_0000, // 2^32
    0x5F00_0000, // 2^63
    0xDF00_0000, // -2^63
    0x5F80_0000, // 2^64
    0x7F7F_FFFF, // max
    0x0080_0000, // min normal
    0x0000_0001, // min subnormal
    0x807F_FFFF, // -max subnormal
    0x7F80_0000, // +inf
    0xFF80_0000, // -inf
    0x7FC0_0000, // qNaN
    0xFFC0_0001, // -qNaN with payload
    0x7FA0_0000, // sNaN
    0x4EFF_FFFF, // just below 2^31
];

const F64_SPECIALS: &[u64] = &[
    0x0000_0000_0000_0000,
    0x8000_0000_0000_0000,
    0x3FF0_0000_0000_0000, // 1
    0xBFF0_0000_0000_0000, // -1
    0x3FE0_0000_0000_0000, // 0.5
    0xBFE0_0000_0000_0000, // -0.5
    0x3FF8_0000_0000_0000, // 1.5
    0x4004_0000_0000_0000, // 2.5
    0xC004_0000_0000_0000, // -2.5
    0x4330_0000_0000_0000, // 2^52
    0x41E0_0000_0000_0000, // 2^31
    0xC1E0_0000_0000_0000, // -2^31
    0x41DF_FFFF_FFC0_0000, // 2^31 - 1
    0xC1E0_0000_0020_0000, // -2^31 - 1
    0x41F0_0000_0000_0000, // 2^32
    0x43E0_0000_0000_0000, // 2^63
    0xC3E0_0000_0000_0000, // -2^63
    0x43F0_0000_0000_0000, // 2^64
    0x7FEF_FFFF_FFFF_FFFF, // max
    0x0010_0000_0000_0000, // min normal
    0x0000_0000_0000_0001, // min subnormal
    0x7FF0_0000_0000_0000, // +inf
    0xFFF0_0000_0000_0000, // -inf
    0x7FF8_0000_0000_0000, // qNaN
    0xFFF8_0000_0000_0001, // -qNaN with payload
    0x7FF4_0000_0000_0000, // sNaN
    0x3FD5_5555_5555_5555, // 1/3
];

/// An interesting 64-bit integer.
pub fn int_value(rng: &mut Rng) -> u64 {
    match rng.below(10) {
        0..=2 => rng.next_u64(),
        3..=5 => rng.pick(INT_SPECIALS),
        6 => rng.below(256),
        7 => rng.next_u64() & 0xFFFF_FFFF,
        8 => rng
            .pick(INT_SPECIALS)
            .wrapping_add(rng.below(3))
            .wrapping_sub(1),
        _ => (rng.next_u64() as i8) as i64 as u64,
    }
}

fn f32_value(rng: &mut Rng) -> u32 {
    match rng.below(8) {
        0..=3 => rng.pick(F32_SPECIALS),
        4 => rng.next_u64() as u32,
        5 => {
            // A float with a small exponent, so rounding modes disagree.
            let mant = (rng.next_u64() as u32) & 0x007F_FFFF;
            let exp = 120 + rng.below(40) as u32;
            let sign = (rng.below(2) as u32) << 31;
            sign | (exp << 23) | mant
        }
        6 => ((rng.below(64) as i32 - 32) as f32).to_bits(),
        _ => {
            let whole = rng.below(200) as f32 - 100.0;
            let frac = rng.pick(&[0.0f32, 0.25, 0.5, 0.75, 0.499_999_97, 0.500_000_06]);
            (whole + frac).to_bits()
        }
    }
}

fn f64_value(rng: &mut Rng) -> u64 {
    match rng.below(8) {
        0..=3 => rng.pick(F64_SPECIALS),
        4 => rng.next_u64(),
        5 => {
            let mant = rng.next_u64() & 0x000F_FFFF_FFFF_FFFF;
            let exp = 1000 + rng.below(100);
            let sign = rng.below(2) << 63;
            sign | (exp << 52) | mant
        }
        _ => {
            let whole = rng.below(200) as f64 - 100.0;
            let frac = rng.pick(&[0.0f64, 0.25, 0.5, 0.75, 0.499_999_999_999_999_9]);
            (whole + frac).to_bits()
        }
    }
}

/// An interesting 128-bit vector: float lanes, double lanes, integer lanes or noise.
pub fn vec_value(rng: &mut Rng) -> [u8; 16] {
    let mut out = [0u8; 16];
    match rng.below(5) {
        0 => {
            for lane in out.chunks_mut(4) {
                lane.copy_from_slice(&f32_value(rng).to_le_bytes());
            }
        }
        1 => {
            for lane in out.chunks_mut(8) {
                lane.copy_from_slice(&f64_value(rng).to_le_bytes());
            }
        }
        2 => {
            for lane in out.chunks_mut(8) {
                lane.copy_from_slice(&int_value(rng).to_le_bytes());
            }
        }
        3 => {
            // Small signed lanes, which saturating and packing ops care about.
            let width = rng.pick(&[1usize, 2, 4]);
            for lane in out.chunks_mut(width) {
                let v = (rng.below(512) as i64 - 256) as u64;
                lane.copy_from_slice(&v.to_le_bytes()[..width]);
            }
        }
        _ => rng.fill(&mut out),
    }
    out
}
