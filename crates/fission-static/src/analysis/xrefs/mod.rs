//! Cross-References (Xrefs) analysis module.
//!
//! Analyzes binary code to find call/jump/data references between addresses.
//! Shape follows Ghidra-style refs (from/to, flow vs data, operand slot); see vendor
//! `Reference` / `RefType` for conceptual parity — implementation is Sleigh-backed.
//!
//! **Deferred:** explicit fall-through edges (Ghidra `FALL_THROUGH`) and indirect/computed
//! flow placeholders are out of scope for this module; track as a follow-up if CFG consumers need them.

use fission_sleigh::runtime::{DecodedFlowKind, DecodedReferenceKind, RuntimeSleighFrontend};
use rustc_hash::{FxHashMap, FxHashSet};

/// Ghidra-compatible sentinel: reference arises from mnemonic / primary decode path (no operand slot).
pub const OPERAND_INDEX_MNEMONIC: i32 = -1;

/// Type of cross-reference (coarse bucket).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XrefType {
    /// Function call (CALL instruction)
    Call,
    /// Jump (JMP, Jcc instructions)
    Jump,
    /// Data reference (MOV, LEA with address)
    Data,
    /// Data read reference (memory load)
    DataRead,
    /// Data write reference (memory store)
    DataWrite,
}

/// A single cross-reference from decoded instructions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Xref {
    pub from_addr: u64,
    pub to_addr: u64,
    pub xref_type: XrefType,
    /// Operand index from Sleigh; [`OPERAND_INDEX_MNEMONIC`] when inferred from mnemonic/direct flow only.
    pub operand_index: i32,
    /// Present when this xref came from [`DecodedInstruction::references`].
    pub sleigh_kind: Option<DecodedReferenceKind>,
    /// Flow refinement for CALL/JMP rows (conditional vs unconditional).
    pub flow_kind: Option<DecodedFlowKind>,
}

pub mod pointer_sweep;

/// Database of all cross-references in a binary.
#[derive(Debug, Clone, Default)]
pub struct XrefDatabase {
    refs_to: FxHashMap<u64, Vec<Xref>>,
    refs_from: FxHashMap<u64, Vec<Xref>>,
    total_count: usize,
}

impl XrefDatabase {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_xref(&mut self, xref: Xref) {
        self.refs_to.entry(xref.to_addr).or_default().push(xref);
        self.refs_from.entry(xref.from_addr).or_default().push(xref);
        self.total_count += 1;
    }

    pub fn get_refs_to(&self, addr: u64) -> &[Xref] {
        self.refs_to.get(&addr).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn get_refs_from(&self, addr: u64) -> &[Xref] {
        self.refs_from
            .get(&addr)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    pub fn total_refs(&self) -> usize {
        self.total_count
    }

    pub fn iter(&self) -> impl Iterator<Item = &Xref> {
        self.refs_from.values().flatten()
    }

    /// Build xref database from disassembled executable sections (same criterion as loader).
    pub fn build_from_binary(binary: &fission_loader::loader::LoadedBinary) -> Self {
        let frontend = binary
            .load_spec()
            .and_then(|load_spec| RuntimeSleighFrontend::new_for_load_spec(load_spec).ok());

        let Some(frontend) = frontend.as_ref() else {
            return Self::new();
        };

        Self::build_with_frontend(binary, frontend)
    }

    /// Build xref database using a caller-provided Sleigh frontend.
    pub fn build_with_frontend(
        binary: &fission_loader::loader::LoadedBinary,
        frontend: &RuntimeSleighFrontend,
    ) -> Self {
        let mut db = Self::new();

        for section in binary.executable_sections() {
            let start = section.file_offset as usize;
            let end = start.saturating_add(section.file_size as usize);
            let Some(code) = binary.data.as_slice().get(start..end) else {
                continue;
            };
            let base_addr = section.virtual_address;
            db.analyze_code(frontend, code, base_addr);
        }

        // Sweep data sections for hardcoded pointers to enrich xref coverage
        let sweeper = pointer_sweep::PointerSweeper::new(binary);
        let data_xrefs = sweeper.sweep(binary);
        for xref in data_xrefs {
            db.add_xref(xref);
        }

        db
    }

    /// Refines the xref database using Value Set Analysis (VSA) over known functions.
    pub fn refine_with_vsa(
        &mut self,
        binary: &fission_loader::loader::LoadedBinary,
        frontend: &RuntimeSleighFrontend,
        function_addrs: &[u64],
    ) {
        let mut analyzer = crate::analysis::value_set::ValueSetAnalyzer::new();
        let code = binary.data.as_slice();
        for &addr in function_addrs {
            let start = addr as usize;
            if start >= code.len() {
                continue;
            }
            if let Ok(pcode_fn) = frontend.lift_raw_pcode_function(&code[start..], addr) {
                analyzer.analyze(&pcode_fn);
            }
        }
        let vsa_xrefs = analyzer.into_xrefs();
        for xref in vsa_xrefs {
            self.add_xref(xref);
        }
    }

    fn analyze_code(&mut self, frontend: &RuntimeSleighFrontend, code: &[u8], base_addr: u64) {
        let addr_upper_bound = base_addr + code.len() as u64 * 2;

        let Ok(instructions) = frontend.decode_window(code, base_addr, usize::MAX) else {
            return;
        };

        for instr in instructions {
            let mut emitted_flow_targets: FxHashSet<u64> = FxHashSet::default();

            for reference in &instr.references {
                let xref_type = xref_type_from_sleigh_kind(reference.kind);
                if matches!(xref_type, XrefType::Data)
                    && (reference.target <= base_addr || reference.target >= addr_upper_bound)
                {
                    continue;
                }

                if matches!(xref_type, XrefType::Call | XrefType::Jump) {
                    emitted_flow_targets.insert(reference.target);
                }

                let flow_kind_opt = if matches!(xref_type, XrefType::Call | XrefType::Jump) {
                    Some(instr.flow_kind)
                } else {
                    None
                };

                self.add_xref(Xref {
                    from_addr: instr.address,
                    to_addr: reference.target,
                    xref_type,
                    operand_index: usize_to_operand_index(reference.operand_index),
                    sleigh_kind: Some(reference.kind),
                    flow_kind: flow_kind_opt,
                });
            }

            if let Some(dt) = instr.direct_target {
                let is_flow = matches!(
                    instr.flow_kind,
                    DecodedFlowKind::Call
                        | DecodedFlowKind::Jump
                        | DecodedFlowKind::ConditionalJump
                );
                if is_flow && !emitted_flow_targets.contains(&dt) {
                    let xref_type = match instr.flow_kind {
                        DecodedFlowKind::Call => XrefType::Call,
                        _ => XrefType::Jump,
                    };
                    self.add_xref(Xref {
                        from_addr: instr.address,
                        to_addr: dt,
                        xref_type,
                        operand_index: OPERAND_INDEX_MNEMONIC,
                        sleigh_kind: None,
                        flow_kind: Some(instr.flow_kind),
                    });
                }
            }
        }
    }
}

#[inline]
fn xref_type_from_sleigh_kind(kind: DecodedReferenceKind) -> XrefType {
    match kind {
        DecodedReferenceKind::CallTarget => XrefType::Call,
        DecodedReferenceKind::BranchTarget => XrefType::Jump,
        DecodedReferenceKind::MemoryAddress
        | DecodedReferenceKind::ImmediateAddress
        | DecodedReferenceKind::RipRelativeAddress => XrefType::Data,
    }
}

#[inline]
fn usize_to_operand_index(op: usize) -> i32 {
    i32::try_from(op).unwrap_or(i32::MAX)
}

impl Xref {
    /// Short tag for UI / CLI (`call`, `jmp`, `jcc`, `data`).
    #[must_use]
    pub fn flow_tag(&self) -> &'static str {
        match self.xref_type {
            XrefType::Call => "call",
            XrefType::Data => "data",
            XrefType::DataRead => "read",
            XrefType::DataWrite => "write",
            XrefType::Jump => match self.flow_kind {
                Some(DecodedFlowKind::ConditionalJump) => "jcc",
                _ => "jmp",
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xref_database() {
        let mut db = XrefDatabase::new();

        db.add_xref(Xref {
            from_addr: 0x1000,
            to_addr: 0x2000,
            xref_type: XrefType::Call,
            operand_index: OPERAND_INDEX_MNEMONIC,
            sleigh_kind: None,
            flow_kind: Some(DecodedFlowKind::Call),
        });

        db.add_xref(Xref {
            from_addr: 0x1100,
            to_addr: 0x2000,
            xref_type: XrefType::Call,
            operand_index: 0,
            sleigh_kind: Some(DecodedReferenceKind::CallTarget),
            flow_kind: Some(DecodedFlowKind::Call),
        });

        assert_eq!(db.get_refs_to(0x2000).len(), 2);
        assert_eq!(db.get_refs_from(0x1000).len(), 1);
        assert_eq!(db.total_refs(), 2);
    }

    #[test]
    fn executable_sections_skip_non_executable_for_build() {
        use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder, SectionInfo};

        let binary =
            LoadedBinaryBuilder::new("x.bin".to_string(), DataBuffer::Heap(vec![0x90; 64]))
                .format("RAW")
                .entry_point(0)
                .image_base(0)
                .is_64bit(false)
                .add_section(SectionInfo {
                    name: ".text".to_string(),
                    virtual_address: 0,
                    virtual_size: 64,
                    file_offset: 0,
                    file_size: 64,
                    is_executable: false,
                    is_readable: true,
                    is_writable: false,
                })
                .build()
                .expect("build");

        let db = XrefDatabase::build_from_binary(&binary);
        assert_eq!(db.total_refs(), 0);
    }
}
