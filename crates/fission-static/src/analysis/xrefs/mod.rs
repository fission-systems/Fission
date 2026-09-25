//! Cross-References (Xrefs) analysis module.
//!
//! Analyzes binary code to find call/jump/data references between addresses.
//! Shape follows Ghidra-style refs (from/to, flow vs data, operand slot); see vendor
//! `Reference` / `RefType` for conceptual parity — implementation is Sleigh-backed.
//!
//! **Deferred:** explicit fall-through edges (Ghidra `FALL_THROUGH`) and indirect/computed
//! flow placeholders are out of scope for this module; track as a follow-up if CFG consumers need them.

use fission_sleigh::runtime::{
    DecodeStopReason, DecodedFlowKind, DecodedReferenceKind, RuntimeSleighFrontend,
};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::analysis::xref_coverage::{
    XrefAnalysisLayer, XrefAnalysisState, XrefCoverageUnit, XrefLayerCoverage, XrefOmissionReason,
    XrefUnsupportedReason,
};

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
        Self::build_from_binary_with_coverage(binary).0
    }

    /// Build xrefs and report the executable/data sections covered by this scan.
    pub fn build_from_binary_with_coverage(
        binary: &fission_loader::loader::LoadedBinary,
    ) -> (Self, XrefLayerCoverage) {
        let executable_sections = binary
            .executable_sections()
            .into_iter()
            .filter(|section| section.file_size > 0)
            .count();
        let pointer_sections = pointer_sweep::PointerSweeper::candidate_section_count(binary);
        let mut coverage = XrefLayerCoverage::requested(
            XrefAnalysisLayer::Disassembly,
            "file-backed executable sections decoded linearly, plus aligned pointer-sized slots in readable non-executable file-backed sections",
            XrefCoverageUnit::ExecutableOrPointerDataSection,
        );
        coverage.candidate_units = executable_sections + pointer_sections;

        let Some(load_spec) = binary.load_spec() else {
            coverage.mark_unsupported(XrefUnsupportedReason::LoadSpecUnavailable);
            coverage.finalize();
            return (Self::new(), coverage);
        };
        let frontend = match RuntimeSleighFrontend::new_for_load_spec(load_spec) {
            Ok(frontend) => frontend,
            Err(_) => {
                coverage.mark_unsupported(XrefUnsupportedReason::SleighFrontendUnavailable);
                coverage.finalize();
                return (Self::new(), coverage);
            }
        };

        Self::build_with_frontend_and_coverage(binary, &frontend, coverage)
    }

    /// Build xref database using a caller-provided Sleigh frontend.
    pub fn build_with_frontend(
        binary: &fission_loader::loader::LoadedBinary,
        frontend: &RuntimeSleighFrontend,
    ) -> Self {
        let executable_sections = binary
            .executable_sections()
            .into_iter()
            .filter(|section| section.file_size > 0)
            .count();
        let pointer_sections = pointer_sweep::PointerSweeper::candidate_section_count(binary);
        let mut coverage = XrefLayerCoverage::requested(
            XrefAnalysisLayer::Disassembly,
            "file-backed executable sections decoded linearly, plus aligned pointer-sized slots in readable non-executable file-backed sections",
            XrefCoverageUnit::ExecutableOrPointerDataSection,
        );
        coverage.candidate_units = executable_sections + pointer_sections;
        Self::build_with_frontend_and_coverage(binary, frontend, coverage).0
    }

    fn build_with_frontend_and_coverage(
        binary: &fission_loader::loader::LoadedBinary,
        frontend: &RuntimeSleighFrontend,
        mut coverage: XrefLayerCoverage,
    ) -> (Self, XrefLayerCoverage) {
        let mut db = Self::new();

        // Every address the image maps, so a data reference can be judged by
        // whether it lands *in the binary* rather than by whether it lands in
        // the code section being decoded.
        let mapped: Vec<(u64, u64)> = binary
            .sections
            .iter()
            .filter_map(|section| {
                let size = section.virtual_size.max(section.file_size);
                let end = section.virtual_address.checked_add(size)?;
                (size > 0).then_some((section.virtual_address, end))
            })
            .collect();

        for section in binary.executable_sections() {
            if section.file_size == 0 {
                continue;
            }
            let start = section.file_offset as usize;
            let end = start.saturating_add(section.file_size as usize);
            let Some(code) = binary.data.as_slice().get(start..end) else {
                coverage.omit(XrefOmissionReason::SectionBytesUnavailable, 1);
                continue;
            };
            let base_addr = section.virtual_address;
            match db.analyze_code(frontend, code, base_addr, &mapped) {
                Some(reason) => coverage.omit(reason, 1),
                None => coverage.completed_units += 1,
            }
        }

        // Sweep data sections for hardcoded pointers to enrich xref coverage
        let sweeper = pointer_sweep::PointerSweeper::new(binary);
        let sweep = sweeper.sweep_with_coverage(binary);
        coverage.completed_units += sweep.coverage.completed_sections;
        coverage.omit(
            XrefOmissionReason::SectionBytesUnavailable,
            sweep.coverage.omitted_sections,
        );
        for xref in sweep.xrefs {
            db.add_xref(xref);
        }

        coverage.records_emitted = db.total_refs();
        coverage.finalize();
        (db, coverage)
    }

    /// Refines the xref database using Value Set Analysis (VSA) over known functions.
    pub fn refine_with_vsa(
        &mut self,
        binary: &fission_loader::loader::LoadedBinary,
        frontend: &RuntimeSleighFrontend,
        function_addrs: &[u64],
    ) {
        for &addr in function_addrs {
            let Some(function) = binary.function_at_exact(addr) else {
                continue;
            };
            if function.is_import || function.size == 0 || function.size > 1 << 20 {
                continue;
            }
            let Ok(size) = usize::try_from(function.size) else {
                continue;
            };
            if binary
                .available_execution_bytes(addr)
                .map_or(true, |available| available < size)
            {
                continue;
            }
            let Some(code) = binary.view_executable_bytes(addr, size) else {
                continue;
            };
            let Ok(decoded) = frontend.lift_raw_pcode_function_with_contract(code, addr, 4096)
            else {
                continue;
            };
            if decoded.stop_reason != DecodeStopReason::TerminalControlFlow {
                continue;
            }
            let mut analyzer = crate::analysis::value_set::ValueSetAnalyzer::new();
            if analyzer.analyze(&decoded.function) {
                for xref in analyzer.into_xrefs() {
                    self.add_xref(xref);
                }
            }
        }
    }

    fn analyze_code(
        &mut self,
        frontend: &RuntimeSleighFrontend,
        code: &[u8],
        base_addr: u64,
        mapped: &[(u64, u64)],
    ) -> Option<XrefOmissionReason> {
        let Ok(instructions) = frontend.decode_window(code, base_addr, usize::MAX) else {
            return Some(XrefOmissionReason::InstructionDecodeFailed);
        };
        let decoded_bytes = instructions
            .iter()
            .map(|instruction| usize::from(instruction.length))
            .sum::<usize>();
        let omission = if decoded_bytes < code.len() {
            Some(if instructions.is_empty() {
                XrefOmissionReason::InstructionDecodeFailed
            } else {
                XrefOmissionReason::UndecodedExecutableTail
            })
        } else {
            None
        };

        for instr in instructions {
            let mut emitted_flow_targets: FxHashSet<u64> = FxHashSet::default();

            for reference in &instr.references {
                let xref_type = xref_type_from_sleigh_kind(reference.kind);
                // A data reference is kept when it points somewhere the image
                // maps. This used to require the target to land inside the
                // *code section currently being decoded* (`base_addr` up to
                // `base_addr + code.len() * 2`), so every reference into
                // `.rodata` or `.data` was dropped: `mov ESI, 0x40201f`
                // loading a string never reached the index, `strings --xrefs`
                // printed an empty "Referenced by" column for strings that are
                // plainly used, and "who references this address" could only
                // ever answer for targets inside the same section.
                if matches!(xref_type, XrefType::Data)
                    && !mapped
                        .iter()
                        .any(|(start, end)| (*start..*end).contains(&reference.target))
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
        omission
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

    /// A reference into a data section is kept.
    ///
    /// The filter used to require a data target to land inside the code
    /// section being decoded (`base_addr .. base_addr + code.len() * 2`), so
    /// every pointer into `.rodata` was dropped before it reached the index:
    /// `strings --xrefs` showed an empty "Referenced by" column for strings
    /// that are plainly loaded, and "who references this address" could not
    /// answer for a string or a global at all. Found in an agent RE pilot,
    /// where the empty column reads as "nothing uses this string".
    #[test]
    fn a_reference_into_a_data_section_is_kept() {
        use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder, SectionInfo};

        // .text at 0x1000: `mov ESI, 0x2000` then `ret`. 0x2000 is in
        // .rodata -- outside the code section, which is the whole point.
        let mut image = vec![0u8; 0x3000];
        image[0x1000..0x1006].copy_from_slice(&[0xbe, 0x00, 0x20, 0x00, 0x00, 0xc3]);
        image[0x2000..0x2006].copy_from_slice(b"hello\0");

        let binary = LoadedBinaryBuilder::new("data_xref.bin".to_string(), DataBuffer::Heap(image))
            .format("RAW")
            .entry_point(0x1000)
            .image_base(0)
            .is_64bit(true)
            // .text is deliberately small: the filter this pins used to accept
            // anything below `base + code.len() * 2`, so a large code section
            // would cover .rodata by accident and the test would pass either
            // way. 0x100 bytes at 0x1000 reach only 0x1200.
            .add_section(SectionInfo {
                name: ".text".to_string(),
                virtual_address: 0x1000,
                virtual_size: 0x100,
                file_offset: 0x1000,
                file_size: 0x100,
                is_executable: true,
                is_readable: true,
                is_writable: false,
            })
            .add_section(SectionInfo {
                name: ".rodata".to_string(),
                virtual_address: 0x2000,
                virtual_size: 0x100,
                file_offset: 0x2000,
                file_size: 0x100,
                is_executable: false,
                is_readable: true,
                is_writable: false,
            })
            .build()
            .expect("build");

        // A synthetic image has no load spec to resolve a frontend from, so
        // the frontend is handed in directly.
        let frontend =
            RuntimeSleighFrontend::new_for_language("x86-64").expect("x86-64 runtime frontend");
        let db = XrefDatabase::build_with_frontend(&binary, &frontend);

        assert!(
            !db.get_refs_to(0x2000).is_empty(),
            "the instruction at 0x1000 loads 0x2000; that reference must reach the index"
        );
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
