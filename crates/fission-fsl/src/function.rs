//! Bounded ABI admission over canonical FIR. This is a derived proof/evidence
//! view, never an executable IR or an instruction semantics implementation.
use crate::{
    recovery::{AffineAddress, EntryDependency, RecoveredSuccessor, RecoveryState},
    CompiledInstruction, DecodedInstruction, FslError,
};
use std::collections::BTreeSet;

pub(crate) struct Instruction<'a> {
    pub address: u64,
    pub length: u16,
    pub body: &'a CompiledInstruction,
    pub decoded: &'a DecodedInstruction,
    pub returns: bool,
}
pub(crate) struct ScalarAbi {
    pub registers: usize,
    pub flags: usize,
    pub zero: usize,
    pub inputs: Vec<usize>,
    pub output: usize,
    pub preserved: Vec<usize>,
    pub stack: usize,
    pub return_bytes: u16,
}
#[derive(Debug)]
pub struct FunctionEvidence {
    /// Smallest ABI register prefix containing all observed dependencies.
    /// This is not the original source parameter count or source types.
    pub parameter_prefix: usize,
    pub input_slots: BTreeSet<usize>,
    pub edges: BTreeSet<(u64, u64)>,
    pub return_sites: BTreeSet<u64>,
    pub return_paths: usize,
}
fn error(message: &str) -> FslError {
    FslError::at(1, 1, message)
}
fn identity(slot: usize, offset: u64) -> AffineAddress {
    AffineAddress {
        entry_register: Some(slot),
        offset,
        bits: 64,
    }
}
/// Acyclic, integer-register, memory-free leaf contract. The sole allowed
/// memory effect is reading the return address at the unchanged entry stack.
pub(crate) fn recover(
    instructions: &[Instruction<'_>],
    abi: &ScalarAbi,
) -> Result<FunctionEvidence, FslError> {
    let mut result = FunctionEvidence {
        parameter_prefix: 0,
        input_slots: BTreeSet::new(),
        edges: BTreeSet::new(),
        return_sites: BTreeSet::new(),
        return_paths: 0,
    };
    if instructions.is_empty() || abi.return_bytes != 8 {
        return Err(error("unsupported scalar function contract"));
    }
    let mut work = vec![(
        0usize,
        RecoveryState::new(abi.registers, abi.flags, abi.zero)?,
        BTreeSet::new(),
    )];
    let mut steps = 0;
    while let Some((index, state, mut visited)) = work.pop() {
        steps += 1;
        if steps > 16384 || work.len() > 1024 {
            return Err(error("function CFG path budget exhausted"));
        }
        if !visited.insert(index) {
            return Err(error("function CFG cycle unsupported"));
        }
        let i = &instructions[index];
        for (state, successor) in state.alternatives(i.body, i.decoded, i.address)? {
            if !state.writes.is_empty() {
                return Err(error("scalar function has memory writes"));
            }
            if i.returns {
                if successor != RecoveredSuccessor::Dynamic
                    || state.reads.len() != 1
                    || state.next_exact_read() != Some(0)
                {
                    return Err(error(
                        "return target is not the original stack return token",
                    ));
                }
                let read = &state.reads[0];
                if read.value_bits != 64
                    || read.address.as_ref() != Some(&identity(abi.stack, 0))
                    || read.instruction_address != i.address
                {
                    return Err(error("return token is not an exact entry-stack load"));
                }
                if state.register_address(abi.stack)
                    != Some(&identity(abi.stack, u64::from(abi.return_bytes)))
                {
                    return Err(error("function stack cleanup violates selected ABI"));
                }
                for slot in &abi.preserved {
                    if state.register_address(*slot) != Some(&identity(*slot, 0)) {
                        return Err(error("function clobbers an ABI preserved register"));
                    }
                }
                for dependency in state
                    .dependencies(abi.output)?
                    .union(&state.control_dependencies)
                {
                    let EntryDependency::Register(slot) = dependency else {
                        return Err(error("function depends on entry flags or memory"));
                    };
                    let position = abi.inputs.iter().position(|s| s == slot).ok_or_else(|| {
                        error("function depends on a non-argument entry register")
                    })?;
                    result.parameter_prefix = result.parameter_prefix.max(position + 1);
                    result.input_slots.insert(*slot);
                }
                result.return_paths += 1;
                result.return_sites.insert(i.address);
            } else {
                // Even a dead memory read can fault; do not erase it.
                if !state.reads.is_empty() {
                    return Err(error("scalar function has non-return memory reads"));
                }
                let next = match successor {
                    RecoveredSuccessor::Fallthrough => {
                        i.address
                            .checked_add(u64::from(i.length))
                            .ok_or_else(|| error("CFG address overflow"))?
                    }
                    RecoveredSuccessor::Constant(pc) => pc,
                    RecoveredSuccessor::Dynamic => {
                        return Err(error("function has an indirect non-return successor"))
                    }
                };
                let target = instructions
                    .iter()
                    .position(|i| i.address == next)
                    .ok_or_else(|| error("function CFG leaves selected span"))?;
                result.edges.insert((i.address, next));
                work.push((target, state, visited.clone()));
            }
        }
    }
    if result.return_paths == 0 {
        return Err(error("function has no admitted return path"));
    }
    Ok(result)
}
