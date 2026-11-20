//! Value Set Analysis (VSA) and Constant Propagation module.
//!
//! Provides static analysis facts about the values that registers or memory locations
//! can hold at a given address, enabling jump-table resolution, read/write xref refinement,
//! and constant propagation without full decompilation.

use fission_pcode_ir::{PcodeFunction, PcodeOpcode, Varnode};
use rustc_hash::{FxHashMap, FxHashSet};

/// Represents the known possible states of a value during analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbstractValue {
    /// Value is completely unknown.
    Top,
    /// Value is known to be exactly this constant.
    Constant(u64),
    /// Value is a set of known constants (useful for jump tables).
    Set(Vec<u64>),
    /// Value is within a bounded range.
    Range { min: u64, max: u64 },
}

impl Default for AbstractValue {
    fn default() -> Self {
        Self::Top
    }
}

impl AbstractValue {
    pub fn merge(&self, other: &Self) -> Self {
        if self == other {
            return self.clone();
        }
        match (self, other) {
            (Self::Top, _) | (_, Self::Top) => Self::Top,
            (Self::Constant(a), Self::Constant(b)) => {
                let mut values = vec![*a, *b];
                values.sort_unstable();
                values.dedup();
                Self::Set(values)
            }
            (Self::Set(a), Self::Constant(b)) | (Self::Constant(b), Self::Set(a)) => {
                let mut s = a.clone();
                if !s.contains(b) {
                    s.push(*b);
                }
                s.sort_unstable();
                if s.len() > 8 { Self::Top } else { Self::Set(s) }
            }
            (Self::Set(a), Self::Set(b)) => {
                let mut s = a.clone();
                for val in b {
                    if !s.contains(val) {
                        s.push(*val);
                    }
                }
                s.sort_unstable();
                if s.len() > 8 { Self::Top } else { Self::Set(s) }
            }
            _ => Self::Top,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VarnodeKey {
    pub space_id: u64,
    pub offset: u64,
}

impl From<&Varnode> for VarnodeKey {
    fn from(v: &Varnode) -> Self {
        Self {
            space_id: v.space_id,
            offset: v.offset,
        }
    }
}

/// Abstract state at a single program point.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValueState {
    /// Maps varnode offset/ID to their abstract values.
    pub varnodes: FxHashMap<VarnodeKey, AbstractValue>,
}

impl ValueState {
    pub fn merge(&mut self, other: &Self) -> bool {
        let mut changed = false;
        let mut keys: Vec<VarnodeKey> = self.varnodes.keys().cloned().collect();
        keys.extend(
            other
                .varnodes
                .keys()
                .filter(|key| !self.varnodes.contains_key(key))
                .cloned(),
        );
        keys.sort_by_key(|key| (key.space_id, key.offset));

        for key in keys {
            // A missing definition means Top at a join.  The join must be
            // commutative: inserting a value merely because it arrived in
            // `other` would make the result depend on predecessor order.
            let merged = match (self.varnodes.get(&key), other.varnodes.get(&key)) {
                (Some(left), Some(right)) => left.merge(right),
                _ => AbstractValue::Top,
            };
            let old = self.varnodes.get(&key).cloned().unwrap_or_default();
            if old == merged {
                continue;
            }
            changed = true;
            if merged == AbstractValue::Top {
                self.varnodes.remove(&key);
            } else {
                self.varnodes.insert(key, merged);
            }
        }
        changed
    }

    pub fn get_value(&self, v: &Varnode) -> AbstractValue {
        if v.is_constant {
            return AbstractValue::Constant(mask_for_size(v.constant_val as u64, v.size));
        }
        self.varnodes
            .get(&VarnodeKey::from(v))
            .cloned()
            .unwrap_or(AbstractValue::Top)
    }

    pub fn set_value(&mut self, v: &Varnode, val: AbstractValue) {
        let masked = mask_abstract_value(val, v.size);
        if masked == AbstractValue::Top {
            self.varnodes.remove(&VarnodeKey::from(v));
        } else {
            self.varnodes.insert(VarnodeKey::from(v), masked);
        }
    }
}

fn mask_for_size(value: u64, size: u32) -> u64 {
    match size {
        0 => 0,
        1..=7 => value & ((1u64 << (size * 8)) - 1),
        _ => value,
    }
}

fn sign_extend_for_size(value: u64, size: u32) -> i64 {
    let bits = size.saturating_mul(8);
    if bits == 0 {
        return 0;
    }
    if bits >= 64 {
        return value as i64;
    }
    let shift = 64 - bits;
    ((value << shift) as i64) >> shift
}

fn mask_abstract_value(val: AbstractValue, size: u32) -> AbstractValue {
    match val {
        AbstractValue::Constant(c) => AbstractValue::Constant(mask_for_size(c, size)),
        AbstractValue::Set(mut vals) => {
            for v in &mut vals {
                *v = mask_for_size(*v, size);
            }
            vals.sort_unstable();
            vals.dedup();
            AbstractValue::Set(vals)
        }
        other => other,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VsaFact {
    DataRead {
        instruction_addr: u64,
        target_addr: u64,
        pcode_op: PcodeOpcode,
    },
    DataWrite {
        instruction_addr: u64,
        target_addr: u64,
        pcode_op: PcodeOpcode,
    },
    JumpTableTarget {
        instruction_addr: u64,
        targets: Vec<u64>,
        pcode_op: PcodeOpcode,
    },
}

/// Computes Value Set Analysis over a Pcode function.
#[derive(Default)]
pub struct ValueSetAnalyzer {
    /// The abstract state at the beginning of each block.
    pub block_states: FxHashMap<usize, ValueState>,
    /// Facts extracted during analysis
    pub facts: Vec<VsaFact>,
}

impl ValueSetAnalyzer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a fact unless this exact fact is already recorded. A block is
    /// evaluated again whenever its entry state grows, so the same load or
    /// indirect branch reaches here once per visit; the facts a visit adds
    /// that an earlier one did not (a wider target set, a new address) are
    /// still new facts and are kept.
    fn emit_fact(&mut self, fact: VsaFact) {
        if !self.facts.contains(&fact) {
            self.facts.push(fact);
        }
    }

    /// Converts all collected VSA facts into `Xref` instances.
    ///
    /// Each distinct `(from, to, kind)` appears once: a jump-table fact that
    /// grows from `[A]` to `[A, B]` over two visits otherwise reports `A` twice.
    pub fn into_xrefs(&self) -> Vec<crate::analysis::xrefs::Xref> {
        let mut xrefs = Vec::new();
        for fact in &self.facts {
            match fact {
                VsaFact::DataRead {
                    instruction_addr,
                    target_addr,
                    pcode_op: _,
                } => {
                    xrefs.push(crate::analysis::xrefs::Xref {
                        from_addr: *instruction_addr,
                        to_addr: *target_addr,
                        xref_type: crate::analysis::xrefs::XrefType::DataRead,
                        operand_index: -1,
                        sleigh_kind: None,
                        flow_kind: None,
                    });
                }
                VsaFact::DataWrite {
                    instruction_addr,
                    target_addr,
                    pcode_op: _,
                } => {
                    xrefs.push(crate::analysis::xrefs::Xref {
                        from_addr: *instruction_addr,
                        to_addr: *target_addr,
                        xref_type: crate::analysis::xrefs::XrefType::DataWrite,
                        operand_index: -1,
                        sleigh_kind: None,
                        flow_kind: None,
                    });
                }
                VsaFact::JumpTableTarget {
                    instruction_addr,
                    targets,
                    pcode_op,
                } => {
                    for &target in targets {
                        xrefs.push(crate::analysis::xrefs::Xref {
                            from_addr: *instruction_addr,
                            to_addr: target,
                            xref_type: if *pcode_op == PcodeOpcode::CallInd {
                                crate::analysis::xrefs::XrefType::Call
                            } else {
                                crate::analysis::xrefs::XrefType::Jump
                            },
                            operand_index: -1,
                            sleigh_kind: None,
                            flow_kind: None,
                        });
                    }
                }
            }
        }
        let mut seen = std::collections::HashSet::new();
        xrefs.retain(|x| seen.insert((x.from_addr, x.to_addr, x.xref_type as u8)));
        xrefs
    }

    /// Analyzes a single PcodeFunction and populates abstract states.
    pub fn analyze(&mut self, function: &PcodeFunction) -> bool {
        if function.blocks.is_empty() {
            return false;
        }

        let mut worklist: FxHashSet<usize> = FxHashSet::default();
        worklist.insert(0);
        self.block_states.insert(0, ValueState::default());

        let mut iteration_count = 0;

        while let Some(&block_idx) = worklist.iter().next() {
            worklist.remove(&block_idx);
            iteration_count += 1;
            if iteration_count > 10000 {
                // Failsafe
                return false;
            }

            let Some(block) = function.blocks.get(block_idx) else {
                continue;
            };

            let mut state = self
                .block_states
                .get(&block_idx)
                .cloned()
                .unwrap_or_default();

            for op in &block.ops {
                self.evaluate_op(&mut state, op);
            }

            // Propagate to successors
            for &succ_idx in &block.successors {
                let succ_idx = succ_idx as usize;
                if succ_idx < function.blocks.len() {
                    if let Some(next_state) = self.block_states.get_mut(&succ_idx) {
                        if next_state.merge(&state) {
                            worklist.insert(succ_idx);
                        }
                    } else {
                        self.block_states.insert(succ_idx, state.clone());
                        worklist.insert(succ_idx);
                    }
                }
            }
        }

        true
    }

    fn evaluate_op(&mut self, state: &mut ValueState, op: &fission_pcode_ir::PcodeOp) {
        match op.opcode {
            PcodeOpcode::Copy => {
                if let (Some(out), Some(in0)) = (&op.output, op.inputs.first()) {
                    let val = mask_abstract_value(state.get_value(in0), out.size);
                    state.set_value(out, val);
                }
            }
            PcodeOpcode::IntZExt => {
                if let (Some(out), Some(in0)) = (&op.output, op.inputs.first()) {
                    match state.get_value(in0) {
                        AbstractValue::Constant(a) => {
                            let extended = mask_for_size(mask_for_size(a, in0.size), out.size);
                            state.set_value(out, AbstractValue::Constant(extended));
                        }
                        _ => state.set_value(out, AbstractValue::Top),
                    }
                }
            }
            PcodeOpcode::IntSExt => {
                if let (Some(out), Some(in0)) = (&op.output, op.inputs.first()) {
                    match state.get_value(in0) {
                        AbstractValue::Constant(a) => {
                            let extended =
                                mask_for_size(sign_extend_for_size(a, in0.size) as u64, out.size);
                            state.set_value(out, AbstractValue::Constant(extended));
                        }
                        _ => state.set_value(out, AbstractValue::Top),
                    }
                }
            }
            PcodeOpcode::IntAdd => {
                if let (Some(out), Some(in0), Some(in1)) =
                    (&op.output, op.inputs.first(), op.inputs.get(1))
                {
                    match (state.get_value(in0), state.get_value(in1)) {
                        (AbstractValue::Constant(a), AbstractValue::Constant(b)) => {
                            state.set_value(
                                out,
                                AbstractValue::Constant(mask_for_size(a.wrapping_add(b), out.size)),
                            );
                        }
                        _ => state.set_value(out, AbstractValue::Top),
                    }
                }
            }
            PcodeOpcode::IntSub => {
                if let (Some(out), Some(in0), Some(in1)) =
                    (&op.output, op.inputs.first(), op.inputs.get(1))
                {
                    match (state.get_value(in0), state.get_value(in1)) {
                        (AbstractValue::Constant(a), AbstractValue::Constant(b)) => {
                            state.set_value(
                                out,
                                AbstractValue::Constant(mask_for_size(a.wrapping_sub(b), out.size)),
                            );
                        }
                        _ => state.set_value(out, AbstractValue::Top),
                    }
                }
            }
            PcodeOpcode::IntMult => {
                if let (Some(out), Some(in0), Some(in1)) =
                    (&op.output, op.inputs.first(), op.inputs.get(1))
                {
                    match (state.get_value(in0), state.get_value(in1)) {
                        (AbstractValue::Constant(a), AbstractValue::Constant(b)) => {
                            state.set_value(
                                out,
                                AbstractValue::Constant(mask_for_size(a.wrapping_mul(b), out.size)),
                            );
                        }
                        _ => state.set_value(out, AbstractValue::Top),
                    }
                }
            }
            PcodeOpcode::IntLeft => {
                if let (Some(out), Some(in0), Some(in1)) =
                    (&op.output, op.inputs.first(), op.inputs.get(1))
                {
                    match (state.get_value(in0), state.get_value(in1)) {
                        (AbstractValue::Constant(a), AbstractValue::Constant(b)) => {
                            let width = u64::from(out.size.saturating_mul(8));
                            let shifted = if width == 0 || b >= width {
                                0
                            } else {
                                mask_for_size(a.wrapping_shl(b as u32), out.size)
                            };
                            state.set_value(out, AbstractValue::Constant(shifted));
                        }
                        _ => state.set_value(out, AbstractValue::Top),
                    }
                }
            }
            PcodeOpcode::IntRight => {
                if let (Some(out), Some(in0), Some(in1)) =
                    (&op.output, op.inputs.first(), op.inputs.get(1))
                {
                    match (state.get_value(in0), state.get_value(in1)) {
                        (AbstractValue::Constant(a), AbstractValue::Constant(b)) => {
                            let width = u64::from(in0.size.saturating_mul(8));
                            let val = mask_for_size(a, in0.size);
                            let shifted = if width == 0 || b >= width {
                                0
                            } else {
                                mask_for_size(val >> (b as u32), out.size)
                            };
                            state.set_value(out, AbstractValue::Constant(shifted));
                        }
                        _ => state.set_value(out, AbstractValue::Top),
                    }
                }
            }
            PcodeOpcode::IntSRight => {
                if let (Some(out), Some(in0), Some(in1)) =
                    (&op.output, op.inputs.first(), op.inputs.get(1))
                {
                    match (state.get_value(in0), state.get_value(in1)) {
                        (AbstractValue::Constant(a), AbstractValue::Constant(b)) => {
                            let width = u64::from(in0.size.saturating_mul(8));
                            let signed = sign_extend_for_size(a, in0.size);
                            let raw = if width == 0 {
                                0
                            } else if b >= width {
                                if signed < 0 { u64::MAX } else { 0 }
                            } else {
                                (signed >> (b as u32)) as u64
                            };
                            state.set_value(
                                out,
                                AbstractValue::Constant(mask_for_size(raw, out.size)),
                            );
                        }
                        _ => state.set_value(out, AbstractValue::Top),
                    }
                }
            }
            PcodeOpcode::IntAnd => {
                if let (Some(out), Some(in0), Some(in1)) =
                    (&op.output, op.inputs.first(), op.inputs.get(1))
                {
                    match (state.get_value(in0), state.get_value(in1)) {
                        (AbstractValue::Constant(a), AbstractValue::Constant(b)) => {
                            state.set_value(
                                out,
                                AbstractValue::Constant(mask_for_size(a & b, out.size)),
                            );
                        }
                        _ => state.set_value(out, AbstractValue::Top),
                    }
                }
            }
            PcodeOpcode::IntOr => {
                if let (Some(out), Some(in0), Some(in1)) =
                    (&op.output, op.inputs.first(), op.inputs.get(1))
                {
                    match (state.get_value(in0), state.get_value(in1)) {
                        (AbstractValue::Constant(a), AbstractValue::Constant(b)) => {
                            state.set_value(
                                out,
                                AbstractValue::Constant(mask_for_size(a | b, out.size)),
                            );
                        }
                        _ => state.set_value(out, AbstractValue::Top),
                    }
                }
            }
            PcodeOpcode::IntXor => {
                if let (Some(out), Some(in0), Some(in1)) =
                    (&op.output, op.inputs.first(), op.inputs.get(1))
                {
                    match (state.get_value(in0), state.get_value(in1)) {
                        (AbstractValue::Constant(a), AbstractValue::Constant(b)) => {
                            state.set_value(
                                out,
                                AbstractValue::Constant(mask_for_size(a ^ b, out.size)),
                            );
                        }
                        _ => state.set_value(out, AbstractValue::Top),
                    }
                }
            }
            PcodeOpcode::Load => {
                if let Some(addr_in) = op.inputs.get(1) {
                    // inputs[0] is space ID, inputs[1] is pointer
                    match state.get_value(addr_in) {
                        AbstractValue::Constant(addr) => {
                            self.emit_fact(VsaFact::DataRead {
                                instruction_addr: op.address,
                                target_addr: addr,
                                pcode_op: op.opcode,
                            });
                        }
                        AbstractValue::Set(addrs) => {
                            for addr in addrs {
                                self.emit_fact(VsaFact::DataRead {
                                    instruction_addr: op.address,
                                    target_addr: addr,
                                    pcode_op: op.opcode,
                                });
                            }
                        }
                        _ => {}
                    }
                }
                if let Some(out) = &op.output {
                    state.set_value(out, AbstractValue::Top); // Load result is unknown without memory model
                }
            }
            PcodeOpcode::Store => {
                if let Some(addr_in) = op.inputs.get(1) {
                    // inputs[0] is space ID, inputs[1] is pointer
                    match state.get_value(addr_in) {
                        AbstractValue::Constant(addr) => {
                            self.emit_fact(VsaFact::DataWrite {
                                instruction_addr: op.address,
                                target_addr: addr,
                                pcode_op: op.opcode,
                            });
                        }
                        AbstractValue::Set(addrs) => {
                            for addr in addrs {
                                self.emit_fact(VsaFact::DataWrite {
                                    instruction_addr: op.address,
                                    target_addr: addr,
                                    pcode_op: op.opcode,
                                });
                            }
                        }
                        _ => {}
                    }
                }
            }
            PcodeOpcode::BranchInd | PcodeOpcode::CallInd => {
                if let Some(target_in) = op.inputs.get(0) {
                    match state.get_value(target_in) {
                        AbstractValue::Constant(target) => {
                            self.emit_fact(VsaFact::JumpTableTarget {
                                instruction_addr: op.address,
                                targets: vec![target],
                                pcode_op: op.opcode,
                            });
                        }
                        AbstractValue::Set(addrs) => {
                            self.emit_fact(VsaFact::JumpTableTarget {
                                instruction_addr: op.address,
                                targets: addrs,
                                pcode_op: op.opcode,
                            });
                        }
                        _ => {}
                    }
                }
            }
            _ => {
                // If it has an output, we must mark it as Top to prevent stale values
                if let Some(out) = &op.output {
                    state.set_value(out, AbstractValue::Top);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_pcode_ir::{PcodeBasicBlock, PcodeOp};

    #[test]
    fn test_abstract_value_default() {
        assert_eq!(AbstractValue::default(), AbstractValue::Top);
    }

    #[test]
    fn test_value_state_default() {
        let state = ValueState::default();
        assert!(state.varnodes.is_empty());
    }

    #[test]
    fn test_value_set_analyzer_empty() {
        let mut analyzer = ValueSetAnalyzer::new();
        let function = PcodeFunction { blocks: vec![] };
        assert!(!analyzer.analyze(&function));
    }

    #[test]
    fn value_state_merge_is_commutative_and_missing_is_top() {
        let var = Varnode {
            space_id: 1,
            offset: 0x10,
            size: 8,
            is_constant: false,
            constant_val: 0,
        };

        let mut left = ValueState::default();
        left.set_value(&var, AbstractValue::Constant(2));
        let mut right = ValueState::default();
        right.set_value(&var, AbstractValue::Constant(1));

        let mut left_then_right = left.clone();
        left_then_right.merge(&right);
        let mut right_then_left = right.clone();
        right_then_left.merge(&left);
        assert_eq!(left_then_right, right_then_left);
        assert_eq!(
            left_then_right.get_value(&var),
            AbstractValue::Set(vec![1, 2])
        );

        let empty = ValueState::default();
        let mut defined_then_empty = left;
        defined_then_empty.merge(&empty);
        let mut empty_then_defined = empty;
        empty_then_defined.merge(&right);
        assert_eq!(defined_then_empty.get_value(&var), AbstractValue::Top);
        assert_eq!(empty_then_defined.get_value(&var), AbstractValue::Top);
    }

    #[test]
    fn constant_indirect_target_emits_jump_fact() {
        let mut analyzer = ValueSetAnalyzer::new();
        let mut state = ValueState::default();
        analyzer.evaluate_op(
            &mut state,
            &PcodeOp {
                seq_num: 0,
                opcode: PcodeOpcode::BranchInd,
                address: 0x1000,
                output: None,
                inputs: vec![Varnode::constant(0x401000, 8)],
                asm_mnemonic: None,
            },
        );

        assert_eq!(
            analyzer.facts,
            vec![VsaFact::JumpTableTarget {
                instruction_addr: 0x1000,
                targets: vec![0x401000],
                pcode_op: PcodeOpcode::BranchInd,
            }]
        );
    }

    #[test]
    fn test_vsa_constant_propagation() {
        let mut analyzer = ValueSetAnalyzer::new();

        let mut block = PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![],
            ops: vec![],
        };
        let var_out = Varnode {
            space_id: 1,
            offset: 0x10,
            size: 8,
            is_constant: false,
            constant_val: 0,
        };
        let cst_10 = Varnode::constant(10, 8);
        let cst_20 = Varnode::constant(20, 8);

        block.ops.push(PcodeOp {
            seq_num: 0,
            opcode: PcodeOpcode::IntAdd,
            address: 0x1000,
            output: Some(var_out.clone()),
            inputs: vec![cst_10.clone(), cst_20.clone()],
            asm_mnemonic: None,
        });

        let mut state = ValueState::default();
        analyzer.evaluate_op(&mut state, &block.ops[0]);

        assert_eq!(state.get_value(&var_out), AbstractValue::Constant(30));
    }

    /// Block 0 sets `r = 0x2000` and falls into block 1; block 1 reads memory
    /// at `r`, then adds 8 to `r` and loops to itself. The worklist visits
    /// block 1 more than once before it settles.
    fn looping_read() -> PcodeFunction {
        let r = Varnode {
            space_id: 1,
            offset: 0x10,
            size: 8,
            is_constant: false,
            constant_val: 0,
        };
        let loaded = Varnode {
            space_id: 1,
            offset: 0x40,
            size: 8,
            is_constant: false,
            constant_val: 0,
        };
        let op = |seq, opcode, address, output: Option<Varnode>, inputs: Vec<Varnode>| PcodeOp {
            seq_num: seq,
            opcode,
            address,
            output,
            inputs,
            asm_mnemonic: None,
        };
        PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x1000,
                    successors: vec![1],
                    ops: vec![op(
                        0,
                        PcodeOpcode::Copy,
                        0x1000,
                        Some(r.clone()),
                        vec![Varnode::constant(0x2000, 8)],
                    )],
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x1010,
                    successors: vec![1],
                    ops: vec![
                        op(
                            1,
                            PcodeOpcode::Load,
                            0x1010,
                            Some(loaded),
                            vec![Varnode::constant(0, 4), r.clone()],
                        ),
                        op(
                            2,
                            PcodeOpcode::IntAdd,
                            0x1014,
                            Some(r.clone()),
                            vec![r, Varnode::constant(8, 8)],
                        ),
                    ],
                },
            ],
        }
    }

    #[test]
    fn a_block_revisited_during_convergence_emits_each_fact_once() {
        let mut analyzer = ValueSetAnalyzer::new();
        assert!(analyzer.analyze(&looping_read()));
        let xrefs = analyzer.into_xrefs();
        let mut seen = std::collections::HashSet::new();
        for x in &xrefs {
            assert!(
                seen.insert((x.from_addr, x.to_addr, format!("{:?}", x.xref_type))),
                "duplicate xref {x:?} in {xrefs:?}"
            );
        }
        assert!(!xrefs.is_empty(), "the loop's read should be reported");
    }

    #[test]
    fn copy_without_inputs_does_not_panic_and_sub64_ops_mask_width() {
        let mut analyzer = ValueSetAnalyzer::new();
        let mut state = ValueState::default();

        let out32 = Varnode {
            space_id: 1,
            offset: 0x00,
            size: 4,
            is_constant: false,
            constant_val: 0,
        };

        // Malformed Copy with empty inputs must not panic.
        analyzer.evaluate_op(
            &mut state,
            &PcodeOp {
                seq_num: 0,
                opcode: PcodeOpcode::Copy,
                address: 0x1000,
                output: Some(out32.clone()),
                inputs: vec![],
                asm_mnemonic: None,
            },
        );
        assert_eq!(state.get_value(&out32), AbstractValue::Top);

        // 32-bit IntAdd wraps modulo 2^32, not 2^64.
        analyzer.evaluate_op(
            &mut state,
            &PcodeOp {
                seq_num: 1,
                opcode: PcodeOpcode::IntAdd,
                address: 0x1004,
                output: Some(out32.clone()),
                inputs: vec![Varnode::constant(0xFFFF_FFFF, 4), Varnode::constant(1, 4)],
                asm_mnemonic: None,
            },
        );
        assert_eq!(state.get_value(&out32), AbstractValue::Constant(0));

        // 32-bit IntLeft by >= 32 yields 0, not wrapping_shl(32) on 64 bits.
        analyzer.evaluate_op(
            &mut state,
            &PcodeOp {
                seq_num: 2,
                opcode: PcodeOpcode::IntLeft,
                address: 0x1008,
                output: Some(out32.clone()),
                inputs: vec![Varnode::constant(1, 4), Varnode::constant(32, 4)],
                asm_mnemonic: None,
            },
        );
        assert_eq!(state.get_value(&out32), AbstractValue::Constant(0));

        // 32-bit IntLeft within width masks high bits.
        analyzer.evaluate_op(
            &mut state,
            &PcodeOp {
                seq_num: 3,
                opcode: PcodeOpcode::IntLeft,
                address: 0x100c,
                output: Some(out32.clone()),
                inputs: vec![Varnode::constant(0x1_0001, 4), Varnode::constant(16, 4)],
                asm_mnemonic: None,
            },
        );
        assert_eq!(
            state.get_value(&out32),
            AbstractValue::Constant(0x0001_0000)
        );
    }
}
