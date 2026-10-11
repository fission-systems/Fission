//! FSL-owned byte-addressed register views and ABI name resolution.
//! Overlapping views share bytes. Partial writes preserve all other bytes;
//! architectural zero extension must be an explicit FIR effect.
use std::collections::{BTreeMap, BTreeSet};

use crate::abi::{AbiProfile, AbiRegisterEntry};
use crate::{
    ByteOrder, DecodedInstruction, Evidence, ExecutionStatus, FirOp, FslError, FslcPackage,
    MachineState,
};

fn invalid(message: impl Into<String>) -> FslError {
    FslError::at(1, 1, message)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpaceKind {
    Register,
    Memory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Space {
    pub name: String,
    pub kind: SpaceKind,
    pub address_bytes: u64,
    pub byte_order: ByteOrder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegisterId(pub usize);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterView {
    pub name: String,
    pub space: String,
    /// Byte offset, not an instruction selector or a host address.
    pub offset: u64,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterLayout {
    pub name: String,
    pub default_space: String,
    pub evidence: Vec<Evidence>,
    pub spaces: Vec<Space>,
    pub registers: Vec<RegisterView>,
}

impl RegisterLayout {
    pub fn validate(&self) -> Result<(), FslError> {
        if self.name.is_empty() || self.evidence.is_empty() || self.registers.is_empty() {
            return Err(invalid(
                "layout requires identity, evidence and register views",
            ));
        }
        for e in &self.evidence {
            if [&e.source_id, &e.url, &e.revision, &e.claim]
                .iter()
                .any(|s| s.is_empty())
            {
                return Err(invalid("layout evidence fields must be nonempty"));
            }
        }
        let mut names = BTreeSet::new();
        for space in &self.spaces {
            if space.name.is_empty()
                || !names.insert(&space.name)
                || !(1..=8).contains(&space.address_bytes)
            {
                return Err(invalid(
                    "space names must be unique and address sizes must be 1..8 bytes",
                ));
            }
        }
        if self.space(&self.default_space)?.kind != SpaceKind::Memory {
            return Err(invalid("default space must be a memory space"));
        }
        names.clear();
        for view in &self.registers {
            if view.name.is_empty()
                || !names.insert(&view.name)
                || !(1..=512).contains(&view.size_bytes)
            {
                return Err(invalid(
                    "register names must be unique and widths must be 1..512 bytes",
                ));
            }
            let space = self.space(&view.space)?;
            if space.kind != SpaceKind::Register {
                return Err(invalid("register view requires a register space"));
            }
            check_range(space, view.offset, view.size_bytes)?;
        }
        Ok(())
    }

    pub fn space(&self, name: &str) -> Result<&Space, FslError> {
        self.spaces
            .iter()
            .find(|s| s.name == name)
            .ok_or_else(|| invalid(format!("unknown space {name}")))
    }

    pub fn resolve(&self, name: &str) -> Result<RegisterId, FslError> {
        self.registers
            .iter()
            .position(|v| v.name == name)
            .map(RegisterId)
            .ok_or_else(|| invalid(format!("unknown register {name}")))
    }

    pub fn view(&self, id: RegisterId) -> Result<&RegisterView, FslError> {
        self.registers
            .get(id.0)
            .ok_or_else(|| invalid("register identity outside layout"))
    }

    pub fn overlaps(&self, a: RegisterId, b: RegisterId) -> Result<bool, FslError> {
        let a = self.view(a)?;
        let b = self.view(b)?;
        Ok(a.space == b.space
            && u128::from(a.offset) < u128::from(b.offset) + u128::from(b.size_bytes)
            && u128::from(b.offset) < u128::from(a.offset) + u128::from(a.size_bytes))
    }
}

fn check_range(space: &Space, offset: u64, size: u64) -> Result<(), FslError> {
    if size == 0 || u128::from(offset) + u128::from(size) > (1u128 << (space.address_bytes * 8)) {
        return Err(invalid(format!(
            "byte range exceeds address space {}",
            space.name
        )));
    }
    Ok(())
}

pub fn compile_layout_source(source: &str) -> Result<RegisterLayout, FslError> {
    let layout = crate::parser::parse_layout(source)?;
    layout.validate()?;
    Ok(layout)
}

/// Byte storage is private so every access uses the validated layout. Memory
/// spaces are metadata only; this file does not emulate architectural memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterFile {
    layout: RegisterLayout,
    spaces: BTreeMap<String, Vec<u8>>,
}

impl RegisterFile {
    pub fn new(layout: RegisterLayout) -> Result<Self, FslError> {
        layout.validate()?;
        let mut extents = BTreeMap::<String, usize>::new();
        for view in &layout.registers {
            let end = usize::try_from(u128::from(view.offset) + u128::from(view.size_bytes))
                .map_err(|_| invalid("register storage exceeds host address range"))?;
            let extent = extents.entry(view.space.clone()).or_default();
            *extent = (*extent).max(end);
        }
        let total = extents
            .values()
            .try_fold(0usize, |a, b| a.checked_add(*b))
            .ok_or_else(|| invalid("register storage extent overflow"))?;
        if total > 16 * 1024 * 1024 {
            return Err(invalid(
                "reference register storage exceeds 16 MiB allocation limit",
            ));
        }
        Ok(Self {
            layout,
            spaces: extents
                .into_iter()
                .map(|(name, size)| (name, vec![0; size]))
                .collect(),
        })
    }

    pub fn layout(&self) -> &RegisterLayout {
        &self.layout
    }

    pub fn read_bytes(&self, name: &str) -> Result<&[u8], FslError> {
        let view = self.layout.view(self.layout.resolve(name)?)?;
        let start = view.offset as usize;
        Ok(&self.spaces[&view.space][start..start + view.size_bytes as usize])
    }

    pub fn write_bytes(&mut self, name: &str, bytes: &[u8]) -> Result<(), FslError> {
        let view = self.layout.view(self.layout.resolve(name)?)?;
        if bytes.len() != view.size_bytes as usize {
            return Err(invalid("register write width mismatch"));
        }
        let start = view.offset as usize;
        self.spaces.get_mut(&view.space).expect("validated storage")[start..start + bytes.len()]
            .copy_from_slice(bytes);
        Ok(())
    }

    pub fn read_u64(&self, name: &str) -> Result<u64, FslError> {
        let view = self.layout.view(self.layout.resolve(name)?)?;
        if view.size_bytes > 8 {
            return Err(invalid("u64 access requires at most 8 register bytes"));
        }
        let bytes = self.read_bytes(name)?;
        let mut value = 0u64;
        match self.layout.space(&view.space)?.byte_order {
            ByteOrder::Little => {
                for (i, &b) in bytes.iter().enumerate() {
                    value |= u64::from(b) << (8 * i);
                }
            }
            ByteOrder::Big => {
                for &b in bytes {
                    value = (value << 8) | u64::from(b);
                }
            }
        }
        Ok(value)
    }

    pub fn write_u64(&mut self, name: &str, value: u64) -> Result<(), FslError> {
        let view = self.layout.view(self.layout.resolve(name)?)?;
        let size = view.size_bytes as usize;
        if size > 8 || (size < 8 && value >> (8 * size) != 0) {
            return Err(invalid("integer does not fit register view"));
        }
        let bytes = match self.layout.space(&view.space)?.byte_order {
            ByteOrder::Little => value.to_le_bytes()[..size].to_vec(),
            ByteOrder::Big => value.to_be_bytes()[8 - size..].to_vec(),
        };
        self.write_bytes(name, &bytes)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedRegisterEntry {
    pub register: RegisterId,
    pub min_bytes: u64,
    pub max_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedConvention {
    pub name: String,
    pub inputs: Vec<LinkedRegisterEntry>,
    pub outputs: Vec<LinkedRegisterEntry>,
    pub preserved_registers: Vec<RegisterId>,
    pub clobbered_registers: Vec<RegisterId>,
}

/// Keeps original metadata (cleanup, ordering, memory effects) together with
/// resolved register identities. This is not a parameter allocation engine.
#[derive(Debug)]
pub struct LinkedAbi<'a> {
    pub abi: &'a AbiProfile,
    pub layout: &'a RegisterLayout,
    pub stack_register: RegisterId,
    pub conventions: Vec<LinkedConvention>,
}

pub fn link_abi<'a>(
    abi: &'a AbiProfile,
    layout: &'a RegisterLayout,
) -> Result<LinkedAbi<'a>, FslError> {
    abi.validate()?;
    layout.validate()?;
    let stack_register = layout.resolve(&abi.stack_register)?;
    let stack = layout.view(stack_register)?;
    if stack.size_bytes != abi.data["pointer_size"] {
        return Err(invalid(format!(
            "stack pointer {} width {} differs from ABI pointer_size {}",
            stack.name, stack.size_bytes, abi.data["pointer_size"]
        )));
    }
    for name in abi
        .global_spaces
        .iter()
        .chain(std::iter::once(&abi.stack_space))
    {
        if layout.space(name)?.kind != SpaceKind::Memory {
            return Err(invalid("ABI memory references require memory spaces"));
        }
    }
    let entries = |items: &[AbiRegisterEntry]| -> Result<Vec<LinkedRegisterEntry>, FslError> {
        let mut seen = BTreeSet::new();
        items
            .iter()
            .map(|entry| {
                let register = layout.resolve(&entry.register)?;
                if !seen.insert(register.0) || entry.max_bytes > layout.view(register)?.size_bytes {
                    return Err(invalid(format!(
                        "duplicate or oversized ABI entry {}",
                        entry.register
                    )));
                }
                Ok(LinkedRegisterEntry {
                    register,
                    min_bytes: entry.min_bytes,
                    max_bytes: entry.max_bytes,
                })
            })
            .collect()
    };
    let ids = |names: &[String]| -> Result<Vec<RegisterId>, FslError> {
        let mut seen = BTreeSet::new();
        names
            .iter()
            .map(|name| {
                let id = layout.resolve(name)?;
                if !seen.insert(id.0) {
                    return Err(invalid("duplicate ABI effect register"));
                }
                Ok(id)
            })
            .collect()
    };
    let mut conventions = Vec::new();
    for convention in &abi.conventions {
        let preserved_registers = ids(&convention.preserved_registers)?;
        let clobbered_registers = ids(&convention.clobbered_registers)?;
        for &preserved in &preserved_registers {
            for &clobbered in &clobbered_registers {
                if layout.overlaps(preserved, clobbered)? {
                    return Err(invalid("ABI preserved and clobbered registers overlap"));
                }
            }
        }
        for effect in &convention.preserved_memory {
            let space = layout.space(&effect.space)?;
            if space.kind != SpaceKind::Memory {
                return Err(invalid("ABI preserved memory requires a memory space"));
            }
            check_range(space, effect.offset, effect.size_bytes)?;
        }
        conventions.push(LinkedConvention {
            name: convention.name.clone(),
            inputs: entries(&convention.inputs)?,
            outputs: entries(&convention.outputs)?,
            preserved_registers,
            clobbered_registers,
        });
    }
    Ok(LinkedAbi {
        abi,
        layout,
        stack_register,
        conventions,
    })
}

/// Explicit logical FIR slot mapping, supplied by the architecture profile.
/// The current adapter requires disjoint slots. Aliased *views* remain supported
/// by RegisterFile, but inter-slot aliases need direct storage FIR effects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterBinding {
    pub registers: Vec<String>,
    pub flags: Vec<String>,
}

pub fn execute_bound(
    package: &FslcPackage,
    decoded: &DecodedInstruction,
    binding: &RegisterBinding,
    file: &mut RegisterFile,
) -> Result<ExecutionStatus, FslError> {
    package.reencode(decoded, &[])?;
    let layout = file.layout();
    let mut bound = Vec::new();
    for name in binding.registers.iter().chain(&binding.flags) {
        let id = layout.resolve(name)?;
        if layout.view(id)?.size_bytes > 8 {
            return Err(invalid("state binding supports at most 64 bits"));
        }
        for &other in &bound {
            if layout.overlaps(id, other)? {
                return Err(invalid("logical FIR slot bindings must not overlap"));
            }
        }
        bound.push(id);
    }
    for name in &binding.flags {
        if layout.view(layout.resolve(name)?)?.size_bytes != 1 {
            return Err(invalid("flag binding requires one byte storage"));
        }
    }
    let instruction = &package.instructions[decoded.instruction_index];
    for op in &instruction.ops {
        let (field, value) = match *op {
            FirOp::RegisterRead { field, output } => (field, output),
            FirOp::RegisterWrite { field, value } => (field, value),
            _ => continue,
        };
        let slot = usize::try_from(decoded.fields[usize::from(field)].1)
            .map_err(|_| invalid("register selector exceeds host index"))?;
        let Some(name) = binding.registers.get(slot) else {
            return Ok(ExecutionStatus::InvalidState);
        };
        if layout.view(layout.resolve(name)?)?.size_bytes * 8
            != u64::from(instruction.values[usize::from(value.0)].ty.bits)
        {
            return Err(invalid("FIR value width differs from bound register width"));
        }
    }
    let mut state = MachineState {
        registers: binding
            .registers
            .iter()
            .map(|name| file.read_u64(name))
            .collect::<Result<_, _>>()?,
        flags: binding
            .flags
            .iter()
            .map(|name| file.read_u64(name))
            .collect::<Result<_, _>>()?,
    };
    let status = crate::execute_decoded(package, decoded, &mut state)?;
    if status == ExecutionStatus::Success {
        let mut candidate = file.clone();
        for (name, &value) in binding
            .registers
            .iter()
            .zip(&state.registers)
            .chain(binding.flags.iter().zip(&state.flags))
        {
            candidate.write_u64(name, value)?;
        }
        *file = candidate;
    }
    Ok(status)
}
