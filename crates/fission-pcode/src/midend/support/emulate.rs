use super::*;
use fission_loader::loader::LoadedBinary;

/// Traces the operations required to compute a target varnode.
/// Returns a topologically sorted vector of PcodeOps and a set of leaf varnode keys (un-defined variables).
pub(crate) fn collect_switch_dependencies(
    target: &Varnode,
    defs: &HashMap<VarnodeKey, DefSite<'_>>,
    pcode: &PcodeFunction,
) -> Option<(Vec<PcodeOp>, HashSet<VarnodeKey>)> {
    let mut visited = HashSet::default();
    let mut ordered_ops = Vec::new();
    let mut leaves = HashSet::default();

    fn visit(
        vn: &Varnode,
        defs: &HashMap<VarnodeKey, DefSite<'_>>,
        pcode: &PcodeFunction,
        visited: &mut HashSet<VarnodeKey>,
        ordered_ops: &mut Vec<PcodeOp>,
        leaves: &mut HashSet<VarnodeKey>,
    ) -> bool {
        // Safe bound: abort tracing if we collect too many operations (e.g. 100)
        if ordered_ops.len() > 100 {
            return false;
        }

        let key = VarnodeKey::from(vn);
        if key.is_constant {
            return true;
        }
        if visited.contains(&key) {
            return true;
        }

        visited.insert(key.clone());

        if let Some(def_site) = defs.get(&key) {
            if def_site.block_idx >= pcode.blocks.len() {
                return false;
            }
            let block = &pcode.blocks[def_site.block_idx];
            if def_site.op_idx >= block.ops.len() {
                return false;
            }
            let op = &block.ops[def_site.op_idx];

            // Recursively visit inputs first to establish topological order
            for input in &op.inputs {
                if !visit(input, defs, pcode, visited, ordered_ops, leaves) {
                    return false;
                }
            }
            ordered_ops.push(op.clone());
        } else {
            leaves.insert(key);
        }
        true
    }

    if visit(
        target,
        defs,
        pcode,
        &mut visited,
        &mut ordered_ops,
        &mut leaves,
    ) {
        Some((ordered_ops, leaves))
    } else {
        None
    }
}

/// Helper to perform sign extension to i64 for signed arithmetic operations.
fn sign_extend(val: u64, size_in_bytes: u32) -> i64 {
    let bits = size_in_bytes.saturating_mul(8);
    if bits == 0 || bits >= 64 {
        val as i64
    } else {
        let shift = 64 - bits;
        ((val << shift) as i64) >> shift
    }
}

/// Helper to mask values to their varnode bit size to avoid overflow garbage.
fn mask_to_size(val: u64, size_in_bytes: u32) -> u64 {
    let bits = size_in_bytes.saturating_mul(8);
    if bits >= 64 {
        val
    } else {
        val & ((1u64 << bits) - 1)
    }
}

/// Emulates a list of topologically sorted Pcode operations with a set of concrete input values.
/// Returns the value of the final destination varnode if emulation completes successfully.
pub(crate) fn emulate_path(
    ops: &[PcodeOp],
    leaf_values: &HashMap<VarnodeKey, u64>,
    binary: Option<&LoadedBinary>,
    is_big_endian: bool,
) -> Option<u64> {
    let mut values: HashMap<VarnodeKey, u64> = HashMap::default();

    // Populate leaf values
    for (k, &v) in leaf_values {
        values.insert(k.clone(), v);
    }

    for op in ops {
        // Resolve input values
        let mut inputs = Vec::with_capacity(op.inputs.len());
        for input in &op.inputs {
            let val = if input.is_constant {
                // `constant_val` holds the signed form: `-1:4` is all 64 bits set.
                // A constant is a value of its own width; left unmasked it feeds
                // upper one-bits into every unsigned operation below.
                if input.size == 0 {
                    input.constant_val as u64
                } else {
                    mask_to_size(input.constant_val as u64, input.size)
                }
            } else {
                let key = VarnodeKey::from(input);
                *values.get(&key).unwrap_or(&0)
            };
            inputs.push(val);
        }

        let mut computed = 0u64;

        match op.opcode {
            PcodeOpcode::Copy => {
                if !inputs.is_empty() {
                    computed = inputs[0];
                }
            }
            PcodeOpcode::Load => {
                if inputs.len() >= 2 {
                    let addr = inputs[1];
                    let size = op.output.as_ref()?.size as usize;
                    if let Some(bin) = binary {
                        if let Some(bytes) = bin.get_bytes(addr, size) {
                            let mut val = 0u64;
                            if !is_big_endian {
                                // Little Endian
                                // Only the low eight bytes fit a `u64`; a wider load keeps
                                // its low half rather than shifting past the word.
                                for (i, &b) in bytes.iter().take(8).enumerate() {
                                    val |= (b as u64) << (i * 8);
                                }
                            } else {
                                // Big Endian
                                for &b in &bytes {
                                    val = (val << 8) | (b as u64);
                                }
                            }
                            computed = val;
                        } else {
                            // Memory load failed (out of bounds or not in read-only section).
                            // Safely abort or return 0. Returning None aborts emulation of this path.
                            return None;
                        }
                    } else {
                        return None;
                    }
                }
            }
            PcodeOpcode::IntAdd => {
                if inputs.len() >= 2 {
                    computed = inputs[0].wrapping_add(inputs[1]);
                }
            }
            PcodeOpcode::IntSub => {
                if inputs.len() >= 2 {
                    computed = inputs[0].wrapping_sub(inputs[1]);
                }
            }
            PcodeOpcode::IntMult => {
                if inputs.len() >= 2 {
                    computed = inputs[0].wrapping_mul(inputs[1]);
                }
            }
            PcodeOpcode::IntDiv => {
                if inputs.len() >= 2 {
                    if inputs[1] == 0 {
                        return None; // Safe abort on div by zero
                    }
                    computed = inputs[0] / inputs[1];
                }
            }
            PcodeOpcode::IntSDiv => {
                if inputs.len() >= 2 {
                    let divisor = sign_extend(inputs[1], op.inputs[1].size);
                    if divisor == 0 {
                        return None;
                    }
                    let dividend = sign_extend(inputs[0], op.inputs[0].size);
                    // `i64::MIN / -1` overflows and panics with plain `/`.
                    computed = dividend.wrapping_div(divisor) as u64;
                }
            }
            PcodeOpcode::IntAnd => {
                if inputs.len() >= 2 {
                    computed = inputs[0] & inputs[1];
                }
            }
            PcodeOpcode::IntOr => {
                if inputs.len() >= 2 {
                    computed = inputs[0] | inputs[1];
                }
            }
            PcodeOpcode::IntXor => {
                if inputs.len() >= 2 {
                    computed = inputs[0] ^ inputs[1];
                }
            }
            PcodeOpcode::IntLeft => {
                if inputs.len() >= 2 {
                    let shift = inputs[1] as u32;
                    computed = if shift >= 64 { 0 } else { inputs[0] << shift };
                }
            }
            PcodeOpcode::IntRight => {
                if inputs.len() >= 2 {
                    let shift = inputs[1] as u32;
                    computed = if shift >= 64 { 0 } else { inputs[0] >> shift };
                }
            }
            PcodeOpcode::IntSRight => {
                if inputs.len() >= 2 {
                    let lhs = sign_extend(inputs[0], op.inputs[0].size);
                    let shift = inputs[1] as u32;
                    let res = if shift >= 64 {
                        if lhs < 0 { -1 } else { 0 }
                    } else {
                        lhs >> shift
                    };
                    computed = res as u64;
                }
            }
            PcodeOpcode::IntZExt => {
                if !inputs.is_empty() {
                    computed = inputs[0];
                }
            }
            PcodeOpcode::IntSExt => {
                if !inputs.is_empty() {
                    computed = sign_extend(inputs[0], op.inputs[0].size) as u64;
                }
            }
            PcodeOpcode::Int2Comp => {
                if !inputs.is_empty() {
                    computed = inputs[0].wrapping_neg();
                }
            }
            PcodeOpcode::IntNegate => {
                if !inputs.is_empty() {
                    computed = !inputs[0];
                }
            }
            PcodeOpcode::SubPiece => {
                if inputs.len() >= 2 {
                    let bits = (inputs[1] as u32).saturating_mul(8);
                    computed = inputs[0].checked_shr(bits).unwrap_or(0);
                }
            }
            PcodeOpcode::Piece => {
                if inputs.len() >= 2 {
                    // A low half of eight bytes or more leaves no room in a `u64`
                    // for the high half; the result keeps only the low half.
                    let shift = op.inputs[1].size.saturating_mul(8);
                    computed = inputs[0].checked_shl(shift).unwrap_or(0) | inputs[1];
                }
            }
            PcodeOpcode::IntCarry => {
                if inputs.len() >= 2 {
                    // Carry out of the operand width, not out of 64 bits.
                    let bits = op.inputs[0].size.saturating_mul(8).min(64);
                    let sum = u128::from(mask_to_size(inputs[0], op.inputs[0].size))
                        + u128::from(mask_to_size(inputs[1], op.inputs[0].size));
                    computed = if bits > 0 && (sum >> bits) != 0 { 1 } else { 0 };
                }
            }
            PcodeOpcode::IntSCarry => {
                if inputs.len() >= 2 {
                    let sum = inputs[0].wrapping_add(inputs[1]);
                    let sign_lhs = sign_extend(inputs[0], op.inputs[0].size) < 0;
                    let sign_rhs = sign_extend(inputs[1], op.inputs[1].size) < 0;
                    // The sign of the sum is judged at the operand width; the
                    // output is a one-byte flag.
                    let sign_sum = sign_extend(sum, op.inputs[0].size) < 0;
                    let overflow = (sign_lhs == sign_rhs) && (sign_lhs != sign_sum);
                    computed = if overflow { 1 } else { 0 };
                }
            }
            PcodeOpcode::IntSBorrow => {
                if inputs.len() >= 2 {
                    let diff = inputs[0].wrapping_sub(inputs[1]);
                    let sign_lhs = sign_extend(inputs[0], op.inputs[0].size) < 0;
                    let sign_rhs = sign_extend(inputs[1], op.inputs[1].size) < 0;
                    let sign_diff = sign_extend(diff, op.inputs[0].size) < 0;
                    let overflow = (sign_lhs != sign_rhs) && (sign_lhs != sign_diff);
                    computed = if overflow { 1 } else { 0 };
                }
            }
            _ => {
                // If any unsupported operation is encountered in target computation, abort.
                return None;
            }
        }

        if let Some(ref out) = op.output {
            let key = VarnodeKey::from(out);
            let masked = mask_to_size(computed, out.size);
            values.insert(key, masked);
        }
    }

    // Return final value of the last operation's output (or we can lookup the target varnode)
    let last_op = ops.last()?;
    let out_vn = last_op.output.as_ref()?;
    values.get(&VarnodeKey::from(out_vn)).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emulate_basic_arithmetic() {
        let v_in1 = Varnode {
            space_id: 1,
            offset: 0x10,
            size: 4,
            is_constant: false,
            constant_val: 0,
        };
        let v_in2 = Varnode {
            space_id: 1,
            offset: 0x20,
            size: 4,
            is_constant: false,
            constant_val: 0,
        };
        let v_out = Varnode {
            space_id: 1,
            offset: 0x30,
            size: 4,
            is_constant: false,
            constant_val: 0,
        };

        let op = PcodeOp {
            seq_num: 0,
            opcode: PcodeOpcode::IntAdd,
            address: 0x1000,
            output: Some(v_out.clone()),
            inputs: vec![v_in1.clone(), v_in2.clone()],
            asm_mnemonic: None,
        };

        let mut leaf_values = HashMap::default();
        leaf_values.insert(VarnodeKey::from(&v_in1), 15);
        leaf_values.insert(VarnodeKey::from(&v_in2), 27);

        let res = emulate_path(&[op], &leaf_values, None, false);
        assert_eq!(res, Some(42));
    }

    #[test]
    fn test_emulate_sign_extension_and_shift() {
        let v_in = Varnode {
            space_id: 1,
            offset: 0x10,
            size: 1,
            is_constant: false,
            constant_val: 0,
        };
        let v_out_sext = Varnode {
            space_id: 1,
            offset: 0x20,
            size: 4,
            is_constant: false,
            constant_val: 0,
        };

        let op_sext = PcodeOp {
            seq_num: 0,
            opcode: PcodeOpcode::IntSExt,
            address: 0x1000,
            output: Some(v_out_sext.clone()),
            inputs: vec![v_in.clone()],
            asm_mnemonic: None,
        };

        let mut leaf_values = HashMap::default();
        leaf_values.insert(VarnodeKey::from(&v_in), 0x80); // -128 in 8-bit signed

        let res = emulate_path(&[op_sext], &leaf_values, None, false);
        assert_eq!(res, Some(0xFFFFFF80));
    }
    fn vn(offset: u64, size: u32) -> Varnode {
        Varnode {
            space_id: 1,
            offset,
            size,
            is_constant: false,
            constant_val: 0,
        }
    }

    fn op(opcode: PcodeOpcode, output: Varnode, inputs: Vec<Varnode>) -> PcodeOp {
        PcodeOp {
            seq_num: 0,
            opcode,
            address: 0x1000,
            output: Some(output),
            inputs,
            asm_mnemonic: None,
        }
    }

    /// A shift by the whole word (8-byte low half, or a byte offset of 8) is
    /// not a shift in a `u64`; it must not trap in a build with overflow checks.
    #[test]
    fn full_width_piece_and_subpiece_do_not_overflow() {
        let hi = vn(0x10, 8);
        let lo = vn(0x20, 8);
        let wide = vn(0x30, 16);
        let mut leaves = HashMap::default();
        leaves.insert(VarnodeKey::from(&hi), 0xAAAA);
        leaves.insert(VarnodeKey::from(&lo), 0x55);

        let piece = op(
            PcodeOpcode::Piece,
            wide.clone(),
            vec![hi.clone(), lo.clone()],
        );
        assert_eq!(emulate_path(&[piece], &leaves, None, false), Some(0x55));

        let mut offset = vn(0, 4);
        offset.is_constant = true;
        offset.constant_val = 8;
        let narrow = vn(0x40, 8);
        let sub = op(PcodeOpcode::SubPiece, narrow, vec![hi, offset]);
        assert_eq!(emulate_path(&[sub], &leaves, None, false), Some(0));
    }

    /// A zero-sized varnode has no sign bit; extending it must not shift by 64.
    #[test]
    fn zero_sized_varnode_helpers_do_not_overflow() {
        assert_eq!(sign_extend(0x80, 0), 0x80);
        assert_eq!(mask_to_size(0x80, 0), 0);
        assert_eq!(sign_extend(0x80, u32::MAX), 0x80);
    }

    fn constant(value: i64, size: u32) -> Varnode {
        let mut c = vn(0, size);
        c.is_constant = true;
        c.constant_val = value;
        c
    }

    /// `constant_val` holds the signed form: `-8:4` is all 64 bits set. As a
    /// 4-byte value it is 0xFFFFFFF8, and shifting that right by one is
    /// 0x7FFFFFFC -- not the 0xFFFFFFFC a sign-extended input would give.
    #[test]
    fn a_narrow_negative_constant_is_masked_to_its_width() {
        let out = vn(0x40, 4);
        let shift = op(
            PcodeOpcode::IntRight,
            out,
            vec![constant(-8, 4), constant(1, 4)],
        );
        let leaves = HashMap::default();
        assert_eq!(
            emulate_path(&[shift], &leaves, None, false),
            Some(0x7FFF_FFFC)
        );
    }

    #[test]
    fn carry_is_judged_at_the_operand_width() {
        // 0xFFFFFFFF + 1 carries out of 32 bits (and not out of 64).
        let out = vn(0x40, 1);
        let carry = |a: i64, b: i64| {
            let o = op(
                PcodeOpcode::IntCarry,
                out.clone(),
                vec![constant(a, 4), constant(b, 4)],
            );
            emulate_path(&[o], &HashMap::default(), None, false)
        };
        assert_eq!(carry(-1, 1), Some(1));
        // 2^31 + 2^31 carries out of 32 bits; as 64-bit values the sum does not wrap.
        assert_eq!(carry(0x8000_0000, 0x8000_0000), Some(1));
        assert_eq!(carry(0x7FFF_FFFF, 1), Some(0));
        // Eight bytes still carry out of 64 bits.
        let o = op(
            PcodeOpcode::IntCarry,
            out,
            vec![constant(-1, 8), constant(1, 8)],
        );
        assert_eq!(
            emulate_path(&[o], &HashMap::default(), None, false),
            Some(1)
        );
    }

    /// The flag output is one byte; the sign it tests is the operands'.
    #[test]
    fn signed_overflow_flags_test_the_operand_sign_not_the_flag_width() {
        let out = vn(0x40, 1);
        let run = |opcode, a: i64, b: i64| {
            let o = op(opcode, out.clone(), vec![constant(a, 4), constant(b, 4)]);
            emulate_path(&[o], &HashMap::default(), None, false)
        };
        // i32::MAX + 1 overflows; -1 + 1 does not.
        assert_eq!(run(PcodeOpcode::IntSCarry, 0x7FFF_FFFF, 1), Some(1));
        assert_eq!(run(PcodeOpcode::IntSCarry, -1, 1), Some(0));
        // i32::MIN - 1 overflows; 0 - 1 does not.
        assert_eq!(
            run(PcodeOpcode::IntSBorrow, i64::from(i32::MIN), 1),
            Some(1)
        );
        assert_eq!(run(PcodeOpcode::IntSBorrow, 0, 1), Some(0));
    }

    #[test]
    fn signed_division_of_the_minimum_by_minus_one_does_not_panic() {
        let out = vn(0x40, 8);
        let div = op(
            PcodeOpcode::IntSDiv,
            out,
            vec![constant(i64::MIN, 8), constant(-1, 8)],
        );
        assert_eq!(
            emulate_path(&[div], &HashMap::default(), None, false),
            Some(i64::MIN as u64)
        );
    }
}
