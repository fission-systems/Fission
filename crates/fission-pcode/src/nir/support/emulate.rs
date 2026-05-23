use super::*;
use std::collections::{HashMap, HashSet};
use fission_loader::loader::LoadedBinary;

/// Traces the operations required to compute a target varnode.
/// Returns a topologically sorted vector of PcodeOps and a set of leaf varnode keys (un-defined variables).
pub(crate) fn collect_switch_dependencies(
    target: &Varnode,
    defs: &HashMap<VarnodeKey, DefSite<'_>>,
    pcode: &PcodeFunction,
) -> Option<(Vec<PcodeOp>, HashSet<VarnodeKey>)> {
    let mut visited = HashSet::new();
    let mut ordered_ops = Vec::new();
    let mut leaves = HashSet::new();

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

    if visit(target, defs, pcode, &mut visited, &mut ordered_ops, &mut leaves) {
        Some((ordered_ops, leaves))
    } else {
        None
    }
}

/// Helper to perform sign extension to i64 for signed arithmetic operations.
fn sign_extend(val: u64, size_in_bytes: u32) -> i64 {
    let bits = size_in_bytes * 8;
    if bits >= 64 {
        val as i64
    } else {
        let shift = 64 - bits;
        ((val << shift) as i64) >> shift
    }
}

/// Helper to mask values to their varnode bit size to avoid overflow garbage.
fn mask_to_size(val: u64, size_in_bytes: u32) -> u64 {
    let bits = size_in_bytes * 8;
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
    let mut values = HashMap::new();

    // Populate leaf values
    for (k, &v) in leaf_values {
        values.insert(k.clone(), v);
    }

    for op in ops {
        // Resolve input values
        let mut inputs = Vec::with_capacity(op.inputs.len());
        for input in &op.inputs {
            let val = if input.is_constant {
                input.constant_val as u64
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
                                for (i, &b) in bytes.iter().enumerate() {
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
                    computed = (dividend / divisor) as u64;
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
                    let offset = inputs[1] as u32;
                    computed = inputs[0] >> (offset * 8);
                }
            }
            PcodeOpcode::Piece => {
                if inputs.len() >= 2 {
                    let shift = op.inputs[1].size * 8;
                    computed = (inputs[0] << shift) | inputs[1];
                }
            }
            PcodeOpcode::IntCarry => {
                if inputs.len() >= 2 {
                    let sum = inputs[0].wrapping_add(inputs[1]);
                    computed = if sum < inputs[0] { 1 } else { 0 };
                }
            }
            PcodeOpcode::IntSCarry => {
                if inputs.len() >= 2 {
                    let sum = inputs[0].wrapping_add(inputs[1]);
                    let sign_lhs = sign_extend(inputs[0], op.inputs[0].size) < 0;
                    let sign_rhs = sign_extend(inputs[1], op.inputs[1].size) < 0;
                    let sign_sum = sign_extend(sum, op.output.as_ref()?.size) < 0;
                    let overflow = (sign_lhs == sign_rhs) && (sign_lhs != sign_sum);
                    computed = if overflow { 1 } else { 0 };
                }
            }
            PcodeOpcode::IntSBorrow => {
                if inputs.len() >= 2 {
                    let diff = inputs[0].wrapping_sub(inputs[1]);
                    let sign_lhs = sign_extend(inputs[0], op.inputs[0].size) < 0;
                    let sign_rhs = sign_extend(inputs[1], op.inputs[1].size) < 0;
                    let sign_diff = sign_extend(diff, op.output.as_ref()?.size) < 0;
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

        let mut leaf_values = HashMap::new();
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

        let mut leaf_values = HashMap::new();
        leaf_values.insert(VarnodeKey::from(&v_in), 0x80); // -128 in 8-bit signed

        let res = emulate_path(&[op_sext], &leaf_values, None, false);
        assert_eq!(res, Some(0xFFFFFF80));
    }
}
