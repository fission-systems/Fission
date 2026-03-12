//! Cross-References (Xrefs) analysis module.
//!
//! Analyzes binary code to find call/jump/data references between addresses.

use std::collections::HashMap;

/// Type of cross-reference
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XrefType {
    /// Function call (CALL instruction)
    Call,
    /// Jump (JMP, Jcc instructions)
    Jump,
    /// Data reference (MOV, LEA with address)
    Data,
}

/// A single cross-reference
///
/// Implements Copy since all fields are Copy types, avoiding heap allocations
#[derive(Debug, Clone, Copy)]
pub struct Xref {
    /// Source address (where the reference originates)
    pub from_addr: u64,
    /// Target address (where the reference points to)
    pub to_addr: u64,
    /// Type of reference
    pub xref_type: XrefType,
}

/// Database of all cross-references in a binary
#[derive(Debug, Clone, Default)]
pub struct XrefDatabase {
    /// References TO an address (key = target address)
    refs_to: HashMap<u64, Vec<Xref>>,
    /// References FROM an address (key = source address)
    refs_from: HashMap<u64, Vec<Xref>>,
    /// Cached total reference count for O(1) lookup
    /// Updated incrementally on each add_xref call
    total_count: usize,
}

impl XrefDatabase {
    /// Create a new empty database
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a cross-reference
    ///
    /// Performance: Uses Copy trait for efficient duplication. Also updates
    /// total_count incrementally for O(1) total_refs() lookup.
    pub fn add_xref(&mut self, xref: Xref) {
        // Store copy in refs_to (Xref is Copy, so this is a cheap memcpy)
        self.refs_to.entry(xref.to_addr).or_default().push(xref);
        // Store original in refs_from
        self.refs_from.entry(xref.from_addr).or_default().push(xref);
        // Update cached count
        self.total_count += 1;
    }

    /// Get all references TO an address (who calls/references this address?)
    pub fn get_refs_to(&self, addr: u64) -> &[Xref] {
        self.refs_to.get(&addr).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Get all references FROM an address (what does this address call/reference?)
    pub fn get_refs_from(&self, addr: u64) -> &[Xref] {
        self.refs_from
            .get(&addr)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Total number of cross-references
    ///
    /// Performance: O(1) using cached count instead of O(N) iteration
    pub fn total_refs(&self) -> usize {
        self.total_count
    }

    /// Iterate over all xrefs stored in the database.
    pub fn iter(&self) -> impl Iterator<Item = &Xref> {
        self.refs_from.values().flatten()
    }

    /// Build xref database from disassembled code
    pub fn build_from_binary(binary: &fission_loader::loader::LoadedBinary) -> Self {
        let mut db = Self::new();

        // Analyze each section that might contain code
        for section in &binary.sections {
            // Skip non-executable sections (heuristic: .text, CODE, etc.)
            let name_lower = section.name.to_lowercase();
            if !name_lower.contains("text")
                && !name_lower.contains("code")
                && section.name != ".text"
            {
                continue;
            }

            // Disassemble and find references
            let start = section.file_offset as usize;
            let end = start + section.file_size as usize;
            if let Some(code) = binary.data.as_slice().get(start..end) {
                // section.virtual_address is already the loaded address (includes image_base)
                let base_addr = section.virtual_address;
                db.analyze_code(code, base_addr);
            }
        }

        db
    }

    /// Analyze code bytes to find cross-references
    ///
    /// Performance optimizations:
    /// - Pre-computes address bounds once instead of per-instruction
    /// - Uses batch insertion approach to reduce HashMap overhead
    /// - Tracks refs count before iteration to avoid redundant total_refs() call
    fn analyze_code(&mut self, code: &[u8], base_addr: u64) {
        use iced_x86::{Decoder, DecoderOptions, FlowControl, OpKind};

        // Pre-compute bounds for address validation (used multiple times per instruction)
        let addr_upper_bound = base_addr + code.len() as u64 * 2;

        // Track initial refs count to check if we found anything
        let initial_refs = self.refs_to.len();

        // Try 64-bit first, then 32-bit
        for bitness in [64, 32] {
            let mut decoder = Decoder::with_ip(bitness, code, base_addr, DecoderOptions::NONE);

            for instr in &mut decoder {
                if instr.is_invalid() {
                    continue;
                }

                let from_addr = instr.ip();

                match instr.flow_control() {
                    FlowControl::Call | FlowControl::IndirectCall => {
                        // Direct call target
                        if instr.op_count() > 0 {
                            if let OpKind::NearBranch16
                            | OpKind::NearBranch32
                            | OpKind::NearBranch64 = instr.op0_kind()
                            {
                                let to_addr = instr.near_branch_target();
                                self.add_xref(Xref {
                                    from_addr,
                                    to_addr,
                                    xref_type: XrefType::Call,
                                });
                            }
                        }
                    }
                    FlowControl::UnconditionalBranch | FlowControl::ConditionalBranch => {
                        // Jump target
                        if instr.op_count() > 0 {
                            if let OpKind::NearBranch16
                            | OpKind::NearBranch32
                            | OpKind::NearBranch64 = instr.op0_kind()
                            {
                                let to_addr = instr.near_branch_target();
                                self.add_xref(Xref {
                                    from_addr,
                                    to_addr,
                                    xref_type: XrefType::Jump,
                                });
                            }
                        }
                    }
                    _ => {
                        // Check for memory references (LEA, MOV with immediate addresses)
                        let op_count = instr.op_count();
                        for i in 0..op_count {
                            match instr.op_kind(i) {
                                OpKind::Memory => {
                                    // Memory operand with displacement
                                    let disp = instr.memory_displacement64();
                                    // Use pre-computed bounds
                                    if disp > base_addr && disp < addr_upper_bound {
                                        self.add_xref(Xref {
                                            from_addr,
                                            to_addr: disp,
                                            xref_type: XrefType::Data,
                                        });
                                    }
                                }
                                OpKind::Immediate64 | OpKind::Immediate32to64 => {
                                    let imm = instr.immediate64();
                                    // Heuristic: if immediate looks like an address
                                    // Use pre-computed bounds
                                    if imm > base_addr && imm < addr_upper_bound {
                                        self.add_xref(Xref {
                                            from_addr,
                                            to_addr: imm,
                                            xref_type: XrefType::Data,
                                        });
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }

            // If we found refs in 64-bit mode, don't try 32-bit
            // Use direct HashMap len() check instead of summing all values
            if self.refs_to.len() > initial_refs {
                break;
            }
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
        });

        db.add_xref(Xref {
            from_addr: 0x1100,
            to_addr: 0x2000,
            xref_type: XrefType::Call,
        });

        assert_eq!(db.get_refs_to(0x2000).len(), 2);
        assert_eq!(db.get_refs_from(0x1000).len(), 1);
        assert_eq!(db.total_refs(), 2);
    }
}
