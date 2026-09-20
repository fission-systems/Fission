//! Value Set Analysis (VSA) and Constant Propagation module.
//!
//! Provides static analysis facts about the values that registers or memory locations
//! can hold at a given address, enabling jump-table resolution, read/write xref refinement,
//! and constant propagation without full decompilation.

use fission_pcode::{PcodeFunction, PcodeOpcode, Varnode};
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
            return AbstractValue::Constant(v.constant_val as u64);
        }
        self.varnodes
            .get(&VarnodeKey::from(v))
            .cloned()
            .unwrap_or(AbstractValue::Top)
    }

    pub fn set_value(&mut self, v: &Varnode, val: AbstractValue) {
        if val == AbstractValue::Top {
            self.varnodes.remove(&VarnodeKey::from(v));
        } else {
            self.varnodes.insert(VarnodeKey::from(v), val);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VsaFact {
    DataRead {
        instruction_addr: u64,
        target_addr: u64,
    },
    DataWrite {
        instruction_addr: u64,
        target_addr: u64,
    },
    JumpTableTarget {
        instruction_addr: u64,
        targets: Vec<u64>,
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

    /// Converts all collected VSA facts into `Xref` instances.
    pub fn into_xrefs(&self) -> Vec<crate::analysis::xrefs::Xref> {
        let mut xrefs = Vec::new();
        for fact in &self.facts {
            match fact {
                VsaFact::DataRead {
                    instruction_addr,
                    target_addr,
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
                } => {
                    for &target in targets {
                        xrefs.push(crate::analysis::xrefs::Xref {
                            from_addr: *instruction_addr,
                            to_addr: target,
                            xref_type: crate::analysis::xrefs::XrefType::Jump,
                            operand_index: -1,
                            sleigh_kind: None,
                            flow_kind: None,
                        });
                    }
                }
            }
        }
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

    fn evaluate_op(&mut self, state: &mut ValueState, op: &fission_pcode::PcodeOp) {
        match op.opcode {
            PcodeOpcode::Copy => {
                if let Some(out) = &op.output {
                    let val = state.get_value(&op.inputs[0]);
                    state.set_value(out, val);
                }
            }
            PcodeOpcode::IntAdd => {
                if let (Some(out), Some(in0), Some(in1)) =
                    (&op.output, op.inputs.get(0), op.inputs.get(1))
                {
                    match (state.get_value(in0), state.get_value(in1)) {
                        (AbstractValue::Constant(a), AbstractValue::Constant(b)) => {
                            state.set_value(out, AbstractValue::Constant(a.wrapping_add(b)));
                        }
                        _ => state.set_value(out, AbstractValue::Top),
                    }
                }
            }
            PcodeOpcode::IntSub => {
                if let (Some(out), Some(in0), Some(in1)) =
                    (&op.output, op.inputs.get(0), op.inputs.get(1))
                {
                    match (state.get_value(in0), state.get_value(in1)) {
                        (AbstractValue::Constant(a), AbstractValue::Constant(b)) => {
                            state.set_value(out, AbstractValue::Constant(a.wrapping_sub(b)));
                        }
                        _ => state.set_value(out, AbstractValue::Top),
                    }
                }
            }
            PcodeOpcode::IntLeft => {
                if let (Some(out), Some(in0), Some(in1)) =
                    (&op.output, op.inputs.get(0), op.inputs.get(1))
                {
                    match (state.get_value(in0), state.get_value(in1)) {
                        (AbstractValue::Constant(a), AbstractValue::Constant(b)) => {
                            state.set_value(out, AbstractValue::Constant(a.wrapping_shl(b as u32)));
                        }
                        _ => state.set_value(out, AbstractValue::Top),
                    }
                }
            }
            PcodeOpcode::IntAnd => {
                if let (Some(out), Some(in0), Some(in1)) =
                    (&op.output, op.inputs.get(0), op.inputs.get(1))
                {
                    match (state.get_value(in0), state.get_value(in1)) {
                        (AbstractValue::Constant(a), AbstractValue::Constant(b)) => {
                            state.set_value(out, AbstractValue::Constant(a & b));
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
                            self.facts.push(VsaFact::DataRead {
                                instruction_addr: op.address,
                                target_addr: addr,
                            });
                        }
                        AbstractValue::Set(addrs) => {
                            for addr in addrs {
                                self.facts.push(VsaFact::DataRead {
                                    instruction_addr: op.address,
                                    target_addr: addr,
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
                            self.facts.push(VsaFact::DataWrite {
                                instruction_addr: op.address,
                                target_addr: addr,
                            });
                        }
                        AbstractValue::Set(addrs) => {
                            for addr in addrs {
                                self.facts.push(VsaFact::DataWrite {
                                    instruction_addr: op.address,
                                    target_addr: addr,
                                });
                            }
                        }
                        _ => {}
                    }
                }
            }
            PcodeOpcode::BranchInd | PcodeOpcode::CallInd | PcodeOpcode::CallOther => {
                if let Some(target_in) = op.inputs.get(0) {
                    match state.get_value(target_in) {
                        AbstractValue::Constant(target) => {
                            self.facts.push(VsaFact::JumpTableTarget {
                                instruction_addr: op.address,
                                targets: vec![target],
                            });
                        }
                        AbstractValue::Set(addrs) => {
                            self.facts.push(VsaFact::JumpTableTarget {
                                instruction_addr: op.address,
                                targets: addrs,
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
    use fission_pcode::{PcodeBasicBlock, PcodeOp};

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
}
