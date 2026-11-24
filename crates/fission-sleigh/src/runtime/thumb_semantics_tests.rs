//! Thumb instructions whose lift is checked by running its raw p-code.
//!
//! A small evaluator executes one instruction's p-code against named
//! registers and a byte-addressed memory, so each test states what the
//! ARMv7-M Architecture Reference Manual says the instruction leaves behind
//! rather than which p-code operations the spec happens to use.

use std::collections::HashMap;

use fission_pcode_ir::space::{is_register_space_id, is_unique_space_id};

use super::*;
use crate::compiler::{load_construct_templates_from_sla, sleigh_compiled_root};

/// Registers and memory after one Thumb instruction.
struct ThumbState {
    names: HashMap<String, (u64, u32)>,
    registers: HashMap<(u64, u32), u64>,
    memory: HashMap<u64, u8>,
}

impl ThumbState {
    fn reg(&self, name: &str) -> u64 {
        let key = self.names[name];
        *self
            .registers
            .get(&key)
            .unwrap_or_else(|| panic!("{name} was never set"))
    }

    fn word(&self, address: u64) -> u32 {
        let byte = |i: u64| u32::from(self.memory.get(&(address + i)).copied().unwrap_or(0));
        byte(0) | byte(1) << 8 | byte(2) << 16 | byte(3) << 24
    }

    fn flag(&self, name: &str) -> bool {
        self.reg(name) != 0
    }
}

/// Lift one Thumb instruction of `language` and run its p-code with the
/// registers in `init` (name, value); memory starts empty.
fn run_thumb(language: &str, code: &[u8], init: &[(&str, u64)]) -> ThumbState {
    let frontend = RuntimeSleighFrontend::new_for_language(language).expect("ARM runtime");
    let address = frontend.normalize_low_bit_code_address(0x100001);
    let (ops, length, _) = frontend
        .decode_and_lift_with_context_override(code, address.address, address.context_override)
        .unwrap_or_else(|error| panic!("{language} Thumb {code:02x?}: {error:#}"));
    assert_eq!(length as usize, code.len(), "{code:02x?} length");

    let sla = sleigh_compiled_root()
        .join("ARM")
        .join(format!("{language}.sla"));
    let names: HashMap<String, (u64, u32)> = load_construct_templates_from_sla(&sla)
        .expect("ARM .sla")
        .register_map
        .into_iter()
        .map(|(name, varnode)| (name, (varnode.offset, varnode.size)))
        .collect();
    let mut state = ThumbState {
        registers: init
            .iter()
            .map(|(name, value)| (names[*name], *value))
            .collect(),
        names,
        memory: HashMap::new(),
    };
    let mut unique: HashMap<(u64, u32), u64> = HashMap::new();
    let mask = |size: u32| u64::MAX >> (64 - 8 * size.min(8));

    for op in &ops {
        let read = |vn: &Varnode, unique: &HashMap<(u64, u32), u64>, state: &ThumbState| {
            if vn.is_constant {
                vn.offset & mask(vn.size)
            } else if is_unique_space_id(vn.space_id) {
                unique[&(vn.offset, vn.size)]
            } else {
                assert!(is_register_space_id(vn.space_id), "{vn:?} in {ops:#?}");
                *state
                    .registers
                    .get(&(vn.offset, vn.size))
                    .unwrap_or_else(|| panic!("read of unset register {vn:?} in {ops:#?}"))
            }
        };
        let input = |i: usize| read(&op.inputs[i], &unique, &state);
        let in_size = |i: usize| op.inputs[i].size;
        let signed = |v: u64, size: u32| {
            let bits = 64 - 8 * size;
            ((v << bits) as i64) >> bits
        };
        let value = match op.opcode {
            PcodeOpcode::Store => {
                let (address, value, size) = (input(1), input(2), in_size(2));
                for i in 0..u64::from(size) {
                    state.memory.insert(address + i, (value >> (8 * i)) as u8);
                }
                continue;
            }
            PcodeOpcode::Load => {
                let address = input(1);
                let size = op.output.as_ref().expect("load output").size;
                (0..u64::from(size)).fold(0, |v, i| {
                    v | u64::from(state.memory.get(&(address + i)).copied().unwrap_or(0)) << (8 * i)
                })
            }
            PcodeOpcode::Copy | PcodeOpcode::IntZExt => input(0),
            PcodeOpcode::IntSExt => signed(input(0), in_size(0)) as u64,
            PcodeOpcode::IntAdd => input(0).wrapping_add(input(1)),
            PcodeOpcode::IntSub => input(0).wrapping_sub(input(1)),
            PcodeOpcode::IntMult => input(0).wrapping_mul(input(1)),
            PcodeOpcode::IntAnd | PcodeOpcode::BoolAnd => input(0) & input(1),
            PcodeOpcode::IntOr | PcodeOpcode::BoolOr => input(0) | input(1),
            PcodeOpcode::IntXor | PcodeOpcode::BoolXor => input(0) ^ input(1),
            PcodeOpcode::IntNegate => !input(0),
            PcodeOpcode::BoolNegate => u64::from(input(0) == 0),
            PcodeOpcode::IntLeft => input(0).checked_shl(input(1) as u32).unwrap_or(0),
            PcodeOpcode::IntRight => input(0).checked_shr(input(1) as u32).unwrap_or(0),
            PcodeOpcode::IntSRight => (signed(input(0), in_size(0)) >> input(1).min(63)) as u64,
            PcodeOpcode::SubPiece => input(0) >> (8 * input(1)),
            PcodeOpcode::IntEqual => u64::from(input(0) == input(1)),
            PcodeOpcode::IntNotEqual => u64::from(input(0) != input(1)),
            PcodeOpcode::IntLess => u64::from(input(0) < input(1)),
            PcodeOpcode::IntSLess => {
                u64::from(signed(input(0), in_size(0)) < signed(input(1), in_size(1)))
            }
            other => panic!("evaluator has no {other:?}: {ops:#?}"),
        };
        let out = op.output.as_ref().expect("value op has an output");
        let value = value & mask(out.size);
        if is_unique_space_id(out.space_id) {
            unique.insert((out.offset, out.size), value);
        } else {
            assert!(is_register_space_id(out.space_id), "{out:?}");
            state.registers.insert((out.offset, out.size), value);
        }
    }
    state
}

#[test]
fn thumb_stmia_writes_the_base_back_even_when_the_base_is_listed() {
    // ARMv7-M ARM, STM encoding T1 (`stmia Rn!, {list}`): `wback = TRUE`, and
    // the operation ends `if wback then R[n] = R[n] + 4*BitCount(registers)`.
    // A listed Rn stores its own value if it is the lowest register and an
    // UNKNOWN value otherwise. Only LDM T1 drops the writeback when Rn is
    // listed; upstream gave STM T1 that rule too. Unicorn and VEX agree.
    for language in ["ARM8_le", "ARM4t_le"] {
        // stmia r0!, {r0, r1}
        let s = run_thumb(
            language,
            &[0x03, 0xc0],
            &[("r0", 0x2000), ("r1", 0x1111_1111)],
        );
        assert_eq!(s.reg("r0"), 0x2008, "{language} stmia r0!, {{r0, r1}}: r0");
        assert_eq!(
            s.word(0x2000),
            0x2000,
            "{language}: r0 stores its entry value"
        );
        assert_eq!(s.word(0x2004), 0x1111_1111, "{language}: r1 stored");

        // stmia r1!, {r0, r1}: the value stored for r1 is UNKNOWN, the writeback is not.
        let s = run_thumb(
            language,
            &[0x03, 0xc1],
            &[("r0", 0x2222_2222), ("r1", 0x2000)],
        );
        assert_eq!(s.reg("r1"), 0x2008, "{language} stmia r1!, {{r0, r1}}: r1");
        assert_eq!(s.word(0x2000), 0x2222_2222, "{language}: r0 stored");

        // stmia r0!, {r1, r2}: the base is not listed (unchanged by the fix).
        let s = run_thumb(
            language,
            &[0x06, 0xc0],
            &[("r0", 0x2000), ("r1", 0x1111_1111), ("r2", 0x3333_3333)],
        );
        assert_eq!(s.reg("r0"), 0x2008, "{language} stmia r0!, {{r1, r2}}: r0");
        assert_eq!(s.word(0x2004), 0x3333_3333, "{language}: r2 stored");

        // ldmia r0, {r0, r1}: LDM T1 with the base listed has no writeback, so
        // r0 ends as the loaded word (memory starts as zeroes).
        let s = run_thumb(
            language,
            &[0x03, 0xc8],
            &[("r0", 0x2000), ("r1", 0x1111_1111)],
        );
        assert_eq!(
            (s.reg("r0"), s.reg("r1")),
            (0, 0),
            "{language} ldmia r0, {{r0, r1}}"
        );
    }
}

#[test]
fn thumb2_logical_ops_set_carry_from_the_shifted_or_expanded_operand() {
    // ARMv7-M ARM, AND/BIC/MOV/MVN (immediate) T1: `(imm32, carry) =
    // ThumbExpandImm_C(i:imm3:imm8, APSR.C)`; AND/MVN (register) T2:
    // `(shifted, carry) = Shift_C(R[m], shift_t, shift_n, APSR.C)`. With S set,
    // `APSR.N = result<31>; APSR.Z = IsZeroBit(result); APSR.C = carry;` and V
    // is unchanged. ThumbExpandImm_C keeps the input carry for the unrotated
    // patterns and takes bit 31 of a rotated constant. Upstream set only N and
    // Z in these six constructors. Expected values follow the pseudocode;
    // Unicorn and VEX give the same for every case.
    const ONES: u64 = 0xffff_ffff;
    // name, bytes, r1, r2, C in, (r0, N, Z, C out)
    type Case = (
        &'static str,
        &'static [u8],
        u64,
        u64,
        u64,
        (u64, bool, bool, bool),
    );
    let cases: [Case; 12] = [
        (
            "ands r0, r1, #0x80000000",
            &[0x11, 0xf0, 0x00, 0x40],
            ONES,
            0,
            0,
            (0x8000_0000, true, false, true),
        ),
        (
            "ands r0, r1, #0xff",
            &[0x11, 0xf0, 0xff, 0x00],
            ONES,
            0,
            1,
            (0xff, false, false, true),
        ),
        (
            "ands r0, r1, #0xff",
            &[0x11, 0xf0, 0xff, 0x00],
            ONES,
            0,
            0,
            (0xff, false, false, false),
        ),
        (
            "bics r0, r1, #0x80000000",
            &[0x31, 0xf0, 0x00, 0x40],
            ONES,
            0,
            0,
            (0x7fff_ffff, false, false, true),
        ),
        (
            "movs.w r0, #0x80000000",
            &[0x5f, 0xf0, 0x00, 0x40],
            0,
            0,
            0,
            (0x8000_0000, true, false, true),
        ),
        (
            "movs.w r0, #0x7f",
            &[0x5f, 0xf0, 0x7f, 0x00],
            0,
            0,
            1,
            (0x7f, false, false, true),
        ),
        (
            "mvns r0, #0x80000000",
            &[0x7f, 0xf0, 0x00, 0x40],
            0,
            0,
            0,
            (0x7fff_ffff, false, false, true),
        ),
        (
            "ands.w r0, r1, r2, lsl #1",
            &[0x11, 0xea, 0x42, 0x00],
            ONES,
            0x8000_0000,
            0,
            (0, false, true, true),
        ),
        (
            "ands.w r0, r1, r2, asr #32",
            &[0x11, 0xea, 0x22, 0x00],
            ONES,
            0x8000_0000,
            0,
            (ONES, true, false, true),
        ),
        (
            "ands.w r0, r1, r2, rrx",
            &[0x11, 0xea, 0x32, 0x00],
            ONES,
            1,
            0,
            (0, false, true, true),
        ),
        (
            "mvns.w r0, r2, lsr #1",
            &[0x7f, 0xea, 0x52, 0x00],
            0,
            1,
            0,
            (ONES, true, false, true),
        ),
        // orrs already took C from the expansion upstream (control).
        (
            "orrs r0, r1, #0x80000000",
            &[0x51, 0xf0, 0x00, 0x40],
            0,
            0,
            0,
            (0x8000_0000, true, false, true),
        ),
    ];
    for (name, code, r1, r2, carry_in, expected) in cases {
        let init = [
            ("r0", 0x5555_5555),
            ("r1", r1),
            ("r2", r2),
            ("NG", 0),
            ("ZR", 0),
            ("CY", carry_in),
            ("OV", 1),
        ];
        let s = run_thumb("ARM8_le", code, &init);
        let got = (s.reg("r0"), s.flag("NG"), s.flag("ZR"), s.flag("CY"));
        assert_eq!(got, expected, "{name} with C={carry_in}: (r0, N, Z, C)");
        assert!(s.flag("OV"), "{name} leaves V unchanged");
    }
}
