//! What a quotient is computed over: the observable boundary of one P-code
//! slice, the ABI facts that place it in registers, and the budgets that
//! bound the search.

use anyhow::{Context, Result, bail};
use fission_loader::loader::LoadedBinary;
use serde::Serialize;

/// One input port: an integer argument whose value is drawn afresh at every
/// step. The port's value is the low `bits` of its argument slot; the rest of
/// the slot is caller garbage, and a slice that reads it is rejected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InputPort {
    pub name: String,
    pub arg_index: usize,
    pub bits: u32,
}

/// The persistent state the slice carries between steps: a memory region
/// whose address arrives in one argument slot. Its bytes are hidden state --
/// observable only through what the slice returns -- and start as `initial`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContextRegion {
    pub arg_index: usize,
    #[serde(serialize_with = "crate::report::hex_bytes")]
    pub initial: Vec<u8>,
}

/// The observational contract of one quotient: inputs `X`, the hidden
/// context, and the observed result `Y` (the low `output_bits` of the return
/// register, plus how each step ended).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QuotientBoundary {
    #[serde(serialize_with = "crate::report::hex_address")]
    pub entry: u64,
    pub inputs: Vec<InputPort>,
    pub context: ContextRegion,
    pub output_bits: u32,
}

/// Resource bounds. Every bound that is hit is reported in the result's
/// status; none is silently absorbed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct QuotientBudget {
    /// `K_max`: the largest quotient the learner may build.
    pub max_states: usize,
    /// `H`: the length of input words the bounded certificate covers.
    pub horizon: u32,
    /// SAT conflicts per solver query; past it the query is `Unknown`.
    pub conflict_budget: Option<u64>,
    /// Feasible paths one step of the slice may have.
    pub max_paths: usize,
    /// P-code operations one path may execute.
    pub max_path_ops: usize,
    /// Concrete probes run when the solver cannot decide.
    pub probes: u32,
    /// Learn/certify rounds before giving up.
    pub max_rounds: usize,
    /// Input symbols the alphabet may grow to.
    pub max_alphabet: usize,
}

impl Default for QuotientBudget {
    fn default() -> Self {
        Self {
            max_states: 32,
            horizon: 4,
            conflict_budget: Some(200_000),
            max_paths: 256,
            max_path_ops: 200_000,
            probes: 64,
            max_rounds: 64,
            max_alphabet: 64,
        }
    }
}

/// A register named by the ABI, located in the SLEIGH register space.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterSlot {
    pub name: String,
    pub offset: u64,
    pub size: u32,
}

/// Where a call leaves its return address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReturnAddress {
    /// On top of the stack at entry (x86).
    Stack,
    /// In a link register (ARM, MIPS, PowerPC).
    Register(RegisterSlot),
}

/// The ISA facts the slice executor needs. They come from the SLEIGH
/// register table and the calling-convention table, never from rules about a
/// particular instruction set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbiModel {
    pub ram_space: u64,
    pub register_space: u64,
    pub big_endian: bool,
    pub pointer_bits: u32,
    pub stack_pointer: RegisterSlot,
    pub arguments: Vec<RegisterSlot>,
    pub return_value: RegisterSlot,
    pub return_address: ReturnAddress,
}

impl AbiModel {
    /// Read the model for `binary` from its SLEIGH register table and the
    /// emulator's calling-convention table -- the same convention the
    /// emulator calls the slice with.
    pub fn for_binary(binary: &LoadedBinary) -> Result<Self> {
        let load_spec = binary
            .load_spec()
            .context("binary lacks a load spec (can't select a register table)")?;
        let registers = fission_sleigh::runtime::register_map_for_load_spec(load_spec)
            .context("no SLEIGH register table for this language")?;
        let arch = fission_emulator::ArchInfo::from_language_id(
            load_spec.pair.language_id.as_str(),
            Some(binary),
        )?;
        // The SLA's own space numbering, read the way the emulator reads it.
        let frontend = fission_sleigh::runtime::RuntimeSleighFrontend::new_for_load_spec(load_spec)
            .context("no SLEIGH frontend for this language")?;
        let layout = frontend
            .compiled_frontend()
            .map(fission_emulator::pcode::spaces::SpaceLayout::from_compiled)
            .context("the SLEIGH frontend has no compiled space table")?;
        let ram_space = layout.ram;
        let register_space = layout.register;
        let lookup = |name: &str| -> Result<RegisterSlot> {
            let (space, offset, size) = registers
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(name))
                .map(|(_, value)| *value)
                .with_context(|| format!("register {name} is not in the SLEIGH table"))?;
            if space != register_space {
                bail!("register {name} lives outside the register space");
            }
            Ok(RegisterSlot {
                name: name.to_ascii_uppercase(),
                offset,
                size,
            })
        };
        Ok(Self {
            ram_space,
            register_space,
            big_endian: matches!(arch.endian, fission_emulator::arch::Endianness::Big),
            pointer_bits: u32::from(arch.pointer_size) * 8,
            stack_pointer: lookup(arch.sp_reg)?,
            arguments: arch
                .cc
                .arg_regs()
                .iter()
                .map(|name| lookup(name))
                .collect::<Result<_>>()?,
            return_value: lookup(arch.cc.return_reg())?,
            return_address: match arch.cc.return_addr_reg() {
                Some(name) => ReturnAddress::Register(lookup(name)?),
                None => ReturnAddress::Stack,
            },
        })
    }
}
