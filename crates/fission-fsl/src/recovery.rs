//! Bounded abstract interpretation of canonical FIR, not an ISA executor.
//! Entry dependencies are evidence candidates; they do not choose an ABI/type.
use crate::{CompiledInstruction, DecodedInstruction, FirOp, FirTerminator, FslError};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EntryDependency {
    Register(usize),
    MemoryRead(usize),
    Flag(usize),
}

/// Modular affine address relative to an entry register (or a constant).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AffineAddress {
    pub entry_register: Option<usize>,
    pub offset: u64,
    pub bits: u16,
}

#[derive(Clone, Debug)]
pub struct MemoryReadEvidence {
    pub instruction_address: u64,
    pub operation_index: usize,
    pub value_bits: u16,
    pub address: Option<AffineAddress>,
    pub forwarded_store: Option<usize>,
    pub possible_store_aliases: Vec<usize>,
}

#[derive(Clone, Debug)]
pub struct MemoryWriteEvidence {
    pub instruction_address: u64,
    pub operation_index: usize,
    pub value_bits: u16,
    pub address: Option<AffineAddress>,
    pub dependencies: BTreeSet<EntryDependency>,
}

fn disjoint(a: &AffineAddress, a_size: u64, b: &AffineAddress, b_size: u64) -> bool {
    if a.entry_register != b.entry_register || a.bits != b.bits {
        return false;
    }
    let delta = a.offset.wrapping_sub(b.offset) & mask(a.bits);
    let reverse = b.offset.wrapping_sub(a.offset) & mask(a.bits);
    delta >= b_size && reverse >= a_size
}

#[derive(Clone, Debug, Default)]
struct Value {
    affine: Option<AffineAddress>,
    exact_read: Option<usize>,
    dependencies: BTreeSet<EntryDependency>,
}
fn mask(bits: u16) -> u64 {
    if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    }
}
fn error(message: &str) -> FslError {
    FslError::at(1, 1, message)
}
impl Value {
    fn constant(value: u64, bits: u16) -> Self {
        Self {
            exact_read: None,
            affine: Some(AffineAddress {
                entry_register: None,
                offset: value & mask(bits),
                bits,
            }),
            dependencies: BTreeSet::new(),
        }
    }
    fn number(&self) -> Option<u64> {
        self.affine
            .as_ref()
            .filter(|a| a.entry_register.is_none())
            .map(|a| a.offset)
    }
    fn narrow(mut self, bits: u16) -> Self {
        if bits < 64 {
            self.exact_read = None;
        }
        if let Some(a) = &mut self.affine {
            a.bits = a.bits.min(bits);
            a.offset &= mask(a.bits);
        }
        self
    }
    fn combined(a: &Self, b: &Self) -> Self {
        Self {
            affine: None,
            exact_read: None,
            dependencies: a.dependencies.union(&b.dependencies).cloned().collect(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveredSuccessor {
    Fallthrough,
    Constant(u64),
    Dynamic,
}

/// Derived analysis view. It is never used as executable instruction semantics.
#[derive(Clone)]
pub struct RecoveryState {
    registers: Vec<Value>,
    flags: Vec<Value>,
    pub reads: Vec<MemoryReadEvidence>,
    pub writes: Vec<MemoryWriteEvidence>,
    stored_values: Vec<Value>,
    pub control_dependencies: BTreeSet<EntryDependency>,
    next_value: Option<Value>,
}
impl RecoveryState {
    pub fn new(registers: usize, flags: usize, zero_register: usize) -> Result<Self, FslError> {
        if registers > 4096 || flags > 4096 || zero_register >= registers {
            return Err(error("invalid recovery bank contract"));
        }
        let mut values: Vec<_> = (0..registers)
            .map(|slot| Value {
                exact_read: None,
                affine: Some(AffineAddress {
                    entry_register: Some(slot),
                    offset: 0,
                    bits: 64,
                }),
                dependencies: BTreeSet::from([EntryDependency::Register(slot)]),
            })
            .collect();
        values[zero_register] = Value::constant(0, 64);
        Ok(Self {
            registers: values,
            flags: (0..flags)
                .map(|slot| Value {
                    dependencies: BTreeSet::from([EntryDependency::Flag(slot)]),
                    ..Value::default()
                })
                .collect(),
            reads: Vec::new(),
            writes: Vec::new(),
            stored_values: Vec::new(),
            control_dependencies: BTreeSet::new(),
            next_value: None,
        })
    }
    pub fn dependencies(&self, register: usize) -> Result<BTreeSet<EntryDependency>, FslError> {
        Ok(self
            .registers
            .get(register)
            .ok_or_else(|| error("recovery return register out of range"))?
            .dependencies
            .clone())
    }
    /// Returns an optional constant guest successor. Unknown paths refuse;
    /// callers must discard this analysis state on refusal.
    pub fn instruction(
        &mut self,
        instruction: &CompiledInstruction,
        decoded: &DecodedInstruction,
        address: u64,
    ) -> Result<RecoveredSuccessor, FslError> {
        self.instruction_path(instruction, decoded, address, &[])?
            .ok_or_else(|| error("unknown branch in recovery; alternatives not requested"))
    }
    /// Enumerate both successors of unknown FIR conditions, with bounded replay.
    /// Values and effects still come exclusively from canonical FIR transfers.
    pub fn alternatives(
        &self,
        instruction: &CompiledInstruction,
        decoded: &DecodedInstruction,
        address: u64,
    ) -> Result<Vec<(Self, RecoveredSuccessor)>, FslError> {
        let mut pending = vec![Vec::new()];
        let mut result = Vec::new();
        while let Some(choices) = pending.pop() {
            if choices.len() > 8 || result.len() + pending.len() >= 256 {
                return Err(error("recovery alternative budget exhausted"));
            }
            let mut state = self.clone();
            match state.instruction_path(instruction, decoded, address, &choices)? {
                Some(successor) => result.push((state, successor)),
                None => {
                    for choice in [false, true] {
                        let mut fork = choices.clone();
                        fork.push(choice);
                        pending.push(fork);
                    }
                }
            }
        }
        Ok(result)
    }
    pub fn register_address(&self, slot: usize) -> Option<&AffineAddress> {
        self.registers.get(slot).and_then(|v| v.affine.as_ref())
    }
    pub fn next_exact_read(&self) -> Option<usize> {
        self.next_value.as_ref().and_then(|v| v.exact_read)
    }
    fn instruction_path(
        &mut self,
        instruction: &CompiledInstruction,
        decoded: &DecodedInstruction,
        address: u64,
        choices: &[bool],
    ) -> Result<Option<RecoveredSuccessor>, FslError> {
        let mut choice_index = 0;
        self.next_value = None;
        crate::state::admit_context(instruction, true, true)?;
        if !crate::state::bank_indices_fit(
            instruction,
            decoded,
            self.registers.len(),
            self.flags.len(),
        ) {
            return Err(error("recovery selector out of range"));
        }
        let blocks = crate::control::blocks(instruction);
        let mut values = vec![Value::default(); instruction.values.len()];
        let mut block_index = 0;
        let mut next_pc = RecoveredSuccessor::Fallthrough;
        for _ in 0..=blocks.len() {
            let block = &blocks[block_index];
            for position in usize::from(block.start)..usize::from(block.end) {
                let op = &instruction.ops[position];
                let get = |id: crate::ValueId| values[usize::from(id.0)].clone();
                let bits = |id: crate::ValueId| instruction.values[usize::from(id.0)].ty.bits;
                let definition = match *op {
                    FirOp::IntConstant { output, value } => {
                        Some((output, Value::constant(value, bits(output))))
                    }
                    FirOp::FieldRead { output, field } => Some((
                        output,
                        Value::constant(decoded.fields[usize::from(field)].1, bits(output)),
                    )),
                    FirOp::RegisterRead { output, field } => Some((
                        output,
                        self.registers[decoded.fields[usize::from(field)].1 as usize]
                            .clone()
                            .narrow(bits(output)),
                    )),
                    FirOp::FlagRead { output, slot } => {
                        Some((output, self.flags[usize::from(slot)].clone()))
                    }
                    FirOp::IntConvert {
                        output,
                        input,
                        kind,
                    } => {
                        let mut v = get(input);
                        if let Some(n) = v.number() {
                            v = Value::constant(
                                crate::control::convert(n, bits(input), bits(output), kind),
                                bits(output),
                            );
                        } else if kind == crate::IntConversion::SignExtend {
                            v.affine = None;
                            v.exact_read = None;
                        } else {
                            v = v.narrow(bits(output));
                        }
                        Some((output, v))
                    }
                    FirOp::IntAddWrap {
                        output,
                        left,
                        right,
                    }
                    | FirOp::IntBinary {
                        output,
                        left,
                        right,
                        ..
                    } => {
                        let (a, b) = (get(left), get(right));
                        let op = if let FirOp::IntBinary { op, .. } = *op {
                            Some(op)
                        } else {
                            None
                        };
                        let mut v = Value::combined(&a, &b);
                        if let (Some(l), Some(r)) = (a.number(), b.number()) {
                            v = Value::constant(
                                op.map_or_else(|| l.wrapping_add(r), |o| o.apply(l, r)),
                                bits(output),
                            );
                        } else {
                            let shifted = match op {
                                None => a
                                    .affine
                                    .as_ref()
                                    .zip(b.number())
                                    .or_else(|| b.affine.as_ref().zip(a.number())),
                                Some(crate::IntBinaryOp::Sub) => a
                                    .affine
                                    .as_ref()
                                    .zip(b.number())
                                    .map(|(p, n)| (p, n.wrapping_neg())),
                                _ => None,
                            };
                            if let Some((p, n)) = shifted {
                                // Avoid flattening a prior narrower modular wrap into a wider add.
                                if p.bits == bits(output) {
                                    v.affine = Some(AffineAddress {
                                        entry_register: p.entry_register,
                                        offset: p.offset.wrapping_add(n) & mask(p.bits),
                                        bits: p.bits,
                                    });
                                }
                            }
                        }
                        Some((output, v))
                    }
                    FirOp::IntCompare {
                        output,
                        left,
                        right,
                        predicate,
                    } => {
                        let (a, b) = (get(left), get(right));
                        Some((
                            output,
                            match (a.number(), b.number()) {
                                (Some(l), Some(r)) => Value::constant(
                                    crate::control::compare(l, r, bits(left), predicate),
                                    1,
                                ),
                                _ => Value::combined(&a, &b),
                            },
                        ))
                    }
                    FirOp::IntAddCarry {
                        output,
                        left,
                        right,
                    } => Some((output, Value::combined(&get(left), &get(right)))),
                    FirOp::MemoryStoreLittle {
                        address: pointer,
                        value,
                    } => {
                        let pointer_value = get(pointer);
                        let stored = get(value);
                        self.writes.push(MemoryWriteEvidence {
                            instruction_address: address,
                            operation_index: position,
                            value_bits: bits(value),
                            address: pointer_value.affine,
                            dependencies: stored
                                .dependencies
                                .union(&pointer_value.dependencies)
                                .cloned()
                                .collect(),
                        });
                        self.stored_values.push(stored);
                        None
                    }
                    FirOp::MemoryLoadLittle {
                        output,
                        address: pointer,
                    } => {
                        let pointer = get(pointer);
                        let id = self.reads.len();
                        let mut aliases = Vec::new();
                        let mut forwarded = None;
                        let mut v = Value {
                            affine: None,
                            exact_read: Some(id),
                            dependencies: pointer.dependencies.clone(),
                        };
                        for (store_id, store) in self.writes.iter().enumerate().rev() {
                            if let (Some(a), Some(b)) = (&pointer.affine, &store.address) {
                                if disjoint(
                                    a,
                                    u64::from(bits(output) / 8),
                                    b,
                                    u64::from(store.value_bits / 8),
                                ) {
                                    continue;
                                }
                                if a == b && bits(output) == store.value_bits && aliases.is_empty()
                                {
                                    forwarded = Some(store_id);
                                    v = self.stored_values[store_id].clone();
                                    v.dependencies.extend(pointer.dependencies.clone());
                                    break;
                                }
                            }
                            aliases.push(store_id);
                            v.dependencies.extend(store.dependencies.clone());
                        }
                        if forwarded.is_none() {
                            v.dependencies.insert(EntryDependency::MemoryRead(id));
                        }
                        self.reads.push(MemoryReadEvidence {
                            instruction_address: address,
                            operation_index: position,
                            value_bits: bits(output),
                            address: pointer.affine,
                            forwarded_store: forwarded,
                            possible_store_aliases: aliases,
                        });
                        Some((output, v))
                    }
                    FirOp::RegisterWrite { field, value } => {
                        self.registers[decoded.fields[usize::from(field)].1 as usize] = get(value);
                        None
                    }
                    FirOp::FlagWrite { slot, value } => {
                        self.flags[usize::from(slot)] = get(value);
                        None
                    }
                    FirOp::GuestPcRead { output } => Some((output, Value::constant(address, 64))),
                    FirOp::GuestNextPcWrite { value } => {
                        if next_pc != RecoveredSuccessor::Fallthrough {
                            return Err(error("multiple recovery guest successors"));
                        }
                        self.next_value = Some(get(value));
                        next_pc = get(value)
                            .number()
                            .map_or(RecoveredSuccessor::Dynamic, RecoveredSuccessor::Constant);
                        None
                    }
                    _ => {
                        return Err(error(
                            "unsupported recovery transfer; no inferred ABI emitted",
                        ))
                    }
                };
                if let Some((output, value)) = definition {
                    values[usize::from(output.0)] = value;
                }
            }
            let edge = match &block.terminator {
                FirTerminator::Return => return Ok(Some(next_pc)),
                FirTerminator::Branch(edge) => edge,
                FirTerminator::CondBranch {
                    condition,
                    on_true,
                    on_false,
                } => {
                    let condition = &values[usize::from(condition.0)];
                    self.control_dependencies
                        .extend(condition.dependencies.clone());
                    match condition.number() {
                        Some(0) => on_false,
                        Some(1) => on_true,
                        _ => {
                            let Some(&choice) = choices.get(choice_index) else {
                                return Ok(None);
                            };
                            choice_index += 1;
                            if choice {
                                on_true
                            } else {
                                on_false
                            }
                        }
                    }
                }
            };
            let arguments: Vec<_> = edge
                .arguments
                .iter()
                .map(|id| values[usize::from(id.0)].clone())
                .collect();
            block_index = usize::from(edge.target);
            for (parameter, value) in blocks[block_index].parameters.iter().zip(arguments) {
                values[usize::from(parameter.0)] = value;
            }
        }
        Err(error("recovery control budget exhausted"))
    }
}
