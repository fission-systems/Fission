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

/// A memory region of its own behind a pointer field: the bytes the pointer
/// stored at `field` of its parent (the context, or an earlier region) points
/// to. The region's address is never known to the model -- only offsets
/// within it are -- and it aliases nothing else: not the context, not the
/// frame, not any other region (an assumption every quotient over one
/// reports). Its bytes are hidden state like the context's, `initial.len()`
/// of them; an access the model cannot keep inside them stops the quotient.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PointerRegion {
    /// Index of the region holding the pointer; `None` for the context.
    pub parent: Option<usize>,
    /// Byte offset of the pointer within its parent.
    pub field: u32,
    /// The region's bytes at the start of the first step. Bytes where a
    /// nested region's pointer lives are filled in by whoever lays the
    /// regions out.
    #[serde(serialize_with = "crate::report::hex_bytes")]
    pub initial: Vec<u8>,
    /// Found by the summary (a pointer field the slice dereferences), not
    /// declared with the boundary; its bytes start zeroed (a global's start
    /// as the image holds them).
    pub discovered: bool,
    /// A region anchored at a fixed address instead of behind a pointer
    /// field: global memory the slice reads or writes at an absolute
    /// address. `parent` and `field` are then unused.
    #[serde(serialize_with = "crate::report::hex_address_opt")]
    pub address: Option<u64>,
    /// A region anchored at a register the caller sets up and the slice
    /// only reads memory through -- the thread pointer, whose block holds
    /// thread-local storage. `parent` and `field` are then unused.
    pub register: Option<String>,
}

impl PointerRegion {
    /// Whether the region hangs off a pointer field (rather than a fixed
    /// address or a register).
    #[must_use]
    pub fn is_pointee(&self) -> bool {
        self.address.is_none() && self.register.is_none()
    }
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
    /// Pointer fields with regions of their own, declared or discovered.
    /// Nested regions come after their parent.
    pub regions: Vec<PointerRegion>,
}

/// Regions are laid out one after another behind the context, each starting
/// on a multiple of this. The model addresses all of them as one byte space
/// ("memory"); a slot is a piece of it.
const REGION_ALIGN: usize = 16;

/// Where the context and each region sit in the one byte space the model
/// and the oracle share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryLayout {
    /// The start of each region, in `QuotientBoundary::regions` order.
    pub bases: Vec<u32>,
    /// Where each region's pointer lives in that space; `None` for a
    /// region anchored at an address or a register.
    pub fields: Vec<Option<u32>>,
    /// Bytes in all.
    pub len: usize,
}

impl QuotientBoundary {
    /// The context and every region, laid out in one byte space.
    #[must_use]
    pub fn layout(&self) -> MemoryLayout {
        let mut len = self.context.initial.len();
        let mut bases = Vec::with_capacity(self.regions.len());
        let mut fields = Vec::with_capacity(self.regions.len());
        for region in &self.regions {
            len = len.div_ceil(REGION_ALIGN) * REGION_ALIGN;
            bases.push(len as u32);
            len += region.initial.len();
        }
        for region in &self.regions {
            let parent = region.parent.map_or(0, |index| bases[index]);
            fields.push(region.is_pointee().then_some(parent + region.field));
        }
        MemoryLayout { bases, fields, len }
    }

    /// The byte space at the start of the first step, pointer fields left
    /// zero: the model never reads a pointer's bytes as data.
    #[must_use]
    pub fn initial_memory(&self) -> Vec<u8> {
        let layout = self.layout();
        let mut memory = vec![0; layout.len];
        memory[..self.context.initial.len()].copy_from_slice(&self.context.initial);
        for (region, base) in self.regions.iter().zip(&layout.bases) {
            let base = *base as usize;
            memory[base..base + region.initial.len()].copy_from_slice(&region.initial);
        }
        memory
    }

    /// How a region is named in reports: the path of pointer fields that
    /// reaches it, e.g. `ctx[0x8]` or `ctx[0x8]->[0x10]`.
    #[must_use]
    pub fn region_name(&self, index: usize) -> String {
        let region = &self.regions[index];
        if let Some(address) = region.address {
            return format!("global[{address:#x}]");
        }
        if let Some(register) = &region.register {
            return format!("[{}]", register.to_ascii_lowercase());
        }
        match region.parent {
            None => format!("ctx[{:#x}]", region.field),
            Some(parent) => format!("{}->[{:#x}]", self.region_name(parent), region.field),
        }
    }

    /// How many pointers lie between the context and a region.
    #[must_use]
    pub fn region_depth(&self, index: usize) -> usize {
        1 + self.regions[index]
            .parent
            .map_or(0, |parent| self.region_depth(parent))
    }
}

/// Resource bounds. Every bound that is hit is reported in the result's
/// status; none is silently absorbed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct QuotientBudget {
    /// `K_max`: the largest quotient the learner may build.
    pub max_states: usize,
    /// `H`: the shortest input words the certificate covers. The result
    /// check is deepened from here towards `states + max_states`.
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
    /// Pointer-field regions the summary may discover (declared ones do not
    /// count), and how many pointers deep one may lie.
    pub max_regions: usize,
    pub max_region_depth: usize,
    /// Regions of global memory the summary may cover.
    pub max_global_regions: usize,
    /// The bytes a discovered region starts with; it grows, up to
    /// `max_region_bytes`, to cover a constant offset the slice reads.
    pub region_bytes: u32,
    pub max_region_bytes: u32,
    /// Positions one indexed access (`buf[i]`) may reach; past it the access
    /// is unsupported rather than expanded.
    pub max_indexed_positions: u32,
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
            max_regions: 4,
            max_region_depth: 2,
            max_global_regions: 8,
            region_bytes: 64,
            max_region_bytes: 4096,
            max_indexed_positions: 512,
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
    /// The registers a callee leaves as it found them: the convention's
    /// general-purpose registers less its caller-saved set and the link
    /// register, plus the stack pointer. Every other register -- the
    /// caller-saved ones, flags, vector registers -- holds nothing the slice
    /// may rely on after a call. Empty when the convention names no
    /// caller-saved set, and then a call cannot be modelled.
    pub preserved: Vec<RegisterSlot>,
    /// The registers the convention says a call may change, by name.
    pub killed: Vec<String>,
    /// Every register of the register space, to name a register the slice
    /// reads memory through.
    pub registers: Vec<RegisterSlot>,
}

impl AbiModel {
    /// The register at exactly `[offset, +size)` of the register space.
    #[must_use]
    pub fn register_at(&self, offset: u64, size: u32) -> Option<&RegisterSlot> {
        self.registers
            .iter()
            .find(|slot| slot.offset == offset && slot.size == size)
    }
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
        let killed = arch.cc.killed_by_call();
        let link = arch.cc.return_addr_reg();
        let preserved = if killed.is_empty() {
            Vec::new()
        } else {
            arch.gp_regs
                .iter()
                .copied()
                .chain([arch.sp_reg])
                .filter(|name| {
                    !killed.iter().any(|k| k.eq_ignore_ascii_case(name))
                        && !link.is_some_and(|l| l.eq_ignore_ascii_case(name))
                })
                .map(lookup)
                .collect::<Result<Vec<_>>>()?
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
            preserved,
            killed: killed
                .iter()
                .map(|name| name.to_ascii_uppercase())
                .collect(),
            registers: {
                let mut all = registers
                    .iter()
                    .filter(|(_, (space, _, _))| *space == register_space)
                    .map(|(name, (_, offset, size))| RegisterSlot {
                        name: name.to_ascii_uppercase(),
                        offset: *offset,
                        size: *size,
                    })
                    .collect::<Vec<_>>();
                all.sort_by(|a, b| (a.offset, a.size, &a.name).cmp(&(b.offset, b.size, &b.name)));
                all
            },
        })
    }
}
