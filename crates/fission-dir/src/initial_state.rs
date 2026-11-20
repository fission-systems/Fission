//! The initial state both sides of a ground-truth check start from.
//!
//! Every byte is defined: a run's registers, arguments and memory are a pure
//! function of `(seed, run, what, where)`. The machine side gets them through
//! the emulator's lazy memory and its register file, the decompiled side
//! through the interpreter's world, and the two agree because each computes
//! the same function -- neither copies from the other, and neither needs to
//! know which values are pointers. A random pointer argument reads the same
//! bytes on both sides wherever it points.
//!
//! The program image is the exception: an address the loaded image maps
//! reads the image, as the machine does.
//!
//! Values are words, not bytes: the first [`EDGE_RUNS`] runs, and one word in
//! eight after them, are edge values (0, 1, -1, the sign bits, small
//! counts). So a pointer chain read from random memory reaches a null, and a
//! count reaches a loop bound, often enough for a function to finish.
//!
//! The first [`UNIFORM_RUNS`] runs are not drawn, or not wholly: each fixes
//! the arguments and registers, the call results and the memory to one word
//! apiece, or leaves a part drawn (all zero, all ones, drawn arguments over
//! zeroed memory with every call returning zero, ...). A divergence that
//! needs every argument to be -1 at once, a pointer argument that leads to
//! zeroed memory, or a call that fails while its argument is a real pointer,
//! is rare in drawn runs and certain in these.

use fission_midend_core::ir::NirType;

/// Runs per function unless the caller says otherwise.
pub const DEFAULT_RUNS: usize = 256;

/// The seed every check uses unless told otherwise: a fixed one, so a result
/// can be reproduced from its run number.
pub const DEFAULT_SEED: u64 = 0x6b75_6e61_6669_7373;

/// Runs whose every word is an edge value.
pub const EDGE_RUNS: u32 = 16;

/// How many runs, from which seed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunPlan {
    pub runs: usize,
    pub seed: u64,
}

impl Default for RunPlan {
    fn default() -> Self {
        Self {
            runs: DEFAULT_RUNS,
            seed: DEFAULT_SEED,
        }
    }
}

/// What a word is for. Each kind draws from its own stream, so adding a
/// register does not move the arguments.
#[derive(Debug, Clone, Copy)]
enum Stream {
    Memory = 1,
    Argument = 2,
    Register = 3,
    CallResult = 4,
}

const EDGES: [u64; 24] = [
    0,
    1,
    2,
    u64::MAX,
    1 << 63,
    u64::MAX >> 1,
    0x7f,
    0x80,
    0xff,
    0x7fff,
    0x8000,
    0xffff,
    0x1_0000,
    0x7fff_ffff,
    0x8000_0000,
    0xffff_ffff,
    7,
    8,
    15,
    16,
    31,
    32,
    63,
    64,
];

/// One part of a uniform run: every word the same, or drawn as usual.
#[derive(Debug, Clone, Copy)]
enum Uniform {
    Word(u64),
    Drawn,
}
use Uniform::{Drawn, Word};

/// The first runs, as (arguments and registers; call results; memory).
pub const UNIFORM_RUNS: usize = 8;
const UNIFORM: [(Uniform, Uniform, Uniform); UNIFORM_RUNS] = [
    (Word(0), Word(0), Word(0)),
    (Word(u64::MAX), Word(u64::MAX), Word(u64::MAX)),
    (Word(1), Word(0), Word(0)),
    (Word(u64::MAX), Word(0), Word(0)),
    (Drawn, Word(0), Word(0)),
    (Drawn, Word(1), Word(0)),
    (Drawn, Drawn, Word(0)),
    (Drawn, Word(0), Word(u64::MAX)),
];

/// In a drawn run, one call result in this many is zero: a null pointer, a
/// zero count or success is what callers test a result against most, and an
/// edge draw alone gives it once in 192.
const ZERO_CALL_RESULT_EVERY: u64 = 4;

fn splitmix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// One run's initial state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InitialState {
    seed: u64,
    run: u32,
}

impl InitialState {
    pub fn new(seed: u64, run: u32) -> Self {
        Self { seed, run }
    }

    pub fn run(&self) -> u32 {
        self.run
    }

    fn word(&self, stream: Stream, index: u64) -> u64 {
        if let Some(&(values, calls, memory)) = UNIFORM.get(self.run as usize) {
            let pick = match stream {
                Stream::Memory => memory,
                Stream::CallResult => calls,
                Stream::Argument | Stream::Register => values,
            };
            if let Word(w) = pick {
                return w;
            }
        }
        let h = splitmix(
            self.seed
                ^ u64::from(self.run).wrapping_mul(0xd6e8_feb8_6659_fd93)
                ^ ((stream as u64) << 56)
                ^ index.wrapping_mul(0xc2b2_ae3d_27d4_eb4f),
        );
        if matches!(stream, Stream::CallResult) && (h >> 16).is_multiple_of(ZERO_CALL_RESULT_EVERY)
        {
            return 0;
        }
        let edge_run = (UNIFORM_RUNS as u32..EDGE_RUNS).contains(&self.run);
        if edge_run || h.is_multiple_of(8) {
            EDGES[(h >> 8) as usize % EDGES.len()]
        } else {
            splitmix(h)
        }
    }

    /// The byte at `addr` where nothing -- neither the image nor a write --
    /// says otherwise. Memory is drawn a word at a time, little-endian, so an
    /// aligned pointer-sized read can be an edge value whole.
    pub fn memory_byte(&self, addr: u64) -> u8 {
        (self.word(Stream::Memory, addr / 8) >> ((addr % 8) * 8)) as u8
    }

    /// The `index`th general-purpose register's word.
    pub fn register_word(&self, index: u64) -> u64 {
        self.word(Stream::Register, index)
    }

    /// What the `n`th call (counting from zero) returns.
    pub fn call_result(&self, n: usize) -> u64 {
        self.word(Stream::CallResult, n as u64)
    }

    /// The register- or stack-slot word that carries argument `index` of
    /// type `ty`, as a caller following the ABI leaves it: a `bool` is 0 or
    /// 1, a type narrower than 32 bits arrives extended to 32 by its own
    /// signedness (what compilers rely on for x86-64 and AArch64 callers),
    /// and the bits above 32 of a 64-bit slot are whatever the caller had.
    pub fn argument_word(&self, index: usize, ty: &NirType, slot_bits: u32) -> u64 {
        let w = self.word(Stream::Argument, index as u64);
        let slot_mask = if slot_bits >= 64 {
            u64::MAX
        } else {
            (1u64 << slot_bits) - 1
        };
        let extended = match ty {
            NirType::Bool => w & 1,
            NirType::Int { bits, signed } if *bits < 32 && *bits > 0 => {
                let low = w & ((1u64 << bits) - 1);
                let ext32 = if *signed && low >> (bits - 1) & 1 == 1 {
                    low | (0xffff_ffffu64 & !((1u64 << bits) - 1))
                } else {
                    low
                };
                (w & !0xffff_ffff) | ext32
            }
            _ => w,
        };
        extended & slot_mask
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_is_a_pure_function_of_its_seed_and_number() {
        let a = InitialState::new(DEFAULT_SEED, 40);
        let b = InitialState::new(DEFAULT_SEED, 40);
        let c = InitialState::new(DEFAULT_SEED, 41);
        let bytes = |s: InitialState| {
            (0..64u64)
                .map(|i| s.memory_byte(0x1000 + i))
                .collect::<Vec<_>>()
        };
        assert_eq!(bytes(a), bytes(b));
        assert_ne!(bytes(a), bytes(c));
        assert_eq!(a.call_result(3), b.call_result(3));
    }

    #[test]
    fn the_first_runs_give_every_word_one_value() {
        let words = |run: u32| {
            let s = InitialState::new(DEFAULT_SEED, run);
            let values: Vec<u64> = (0..8)
                .map(|i| s.register_word(i))
                .chain((0..8).map(|i| s.argument_word(i as usize, &NirType::Unknown, 64)))
                .collect();
            let calls: Vec<u64> = (0..8).map(|i| s.call_result(i)).collect();
            let memory: Vec<u8> = (0..64).map(|i| s.memory_byte(0x5000 + i * 37)).collect();
            (values, calls, memory)
        };
        let all = |v: &[u64], w: u64| v.iter().all(|&x| x == w);
        let (v, c, m) = words(0);
        assert!(all(&v, 0) && all(&c, 0) && m.iter().all(|&b| b == 0));
        let (v, c, m) = words(1);
        assert!(all(&v, u64::MAX) && all(&c, u64::MAX) && m.iter().all(|&b| b == 0xff));
        let (v, c, m) = words(3);
        assert!(all(&v, u64::MAX) && all(&c, 0) && m.iter().all(|&b| b == 0));
        // Arguments that look like pointers, every call failing, over zeroed
        // memory.
        let (v, c, m) = words(4);
        assert!(v.iter().filter(|&&w| !EDGES.contains(&w)).count() > 12);
        assert!(all(&c, 0) && m.iter().all(|&b| b == 0));
    }

    #[test]
    fn a_drawn_call_result_is_often_zero() {
        let zeros = (100..200)
            .flat_map(|run| {
                let s = InitialState::new(DEFAULT_SEED, run);
                (0..8).map(move |i| s.call_result(i))
            })
            .filter(|&w| w == 0)
            .count();
        assert!((140..270).contains(&zeros), "about one in four: {zeros}");
    }

    #[test]
    fn early_runs_are_edge_values_and_later_ones_mostly_are_not() {
        let edge = InitialState::new(DEFAULT_SEED, UNIFORM_RUNS as u32);
        for i in 0..32 {
            assert!(EDGES.contains(&edge.register_word(i)));
        }
        let late = InitialState::new(DEFAULT_SEED, 100);
        let edges = (0..800)
            .filter(|&i| EDGES.contains(&late.register_word(i)))
            .count();
        assert!((50..200).contains(&edges), "about one in eight: {edges}");
    }

    #[test]
    fn arguments_keep_the_extension_the_abi_promises() {
        for run in 0..64 {
            let s = InitialState::new(DEFAULT_SEED, run);
            for i in 0..4 {
                let b = s.argument_word(i, &NirType::Bool, 64);
                assert!(b <= 1, "bool is 0 or 1: {b:#x}");
                let c = s.argument_word(
                    i,
                    &NirType::Int {
                        bits: 8,
                        signed: true,
                    },
                    64,
                );
                let low32 = c as u32;
                assert_eq!(
                    low32 as i32,
                    (c as u8) as i8 as i32,
                    "char sign-extended to 32: {c:#x}"
                );
                let u = s.argument_word(
                    i,
                    &NirType::Int {
                        bits: 16,
                        signed: false,
                    },
                    64,
                );
                assert_eq!(
                    u as u32,
                    (u as u16) as u32,
                    "ushort zero-extended to 32: {u:#x}"
                );
                let narrow = s.argument_word(
                    i,
                    &NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                    32,
                );
                assert_eq!(narrow >> 32, 0, "a 32-bit slot holds 32 bits");
            }
        }
    }
}
