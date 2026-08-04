//! Diagnostic-only cross-check between the current name-merge decisions
//! (`materialized_vns` / `explicit_merge_bindings`) and the already-computed
//! SSA `HighVariable` cover data (`crate::midend::ir::ssa`).
//!
//! This module changes no materialize output. It exists to measure, on real
//! corpus functions, how often the existing reachability-proof-based name
//! merge system (`materialize/mod.rs`, `cross_block.rs`) assigns the same
//! rendered name to two SSA values that the (separately built, unconsumed)
//! Cover analysis proves have interfering live ranges -- Ghidra's
//! `HighIntersectTest` failure mode. See
//! `docs/proposals/2026-07-26-heritage-memory-promotion-and-cover-coalescing.md`
//! for the Cover data's own provenance.
use super::*;
use fission_midend_core::ir::{SsaHighVariableId, SsaOpSite, SsaStorageKey};

/// One case where two different (interfering) `SsaHighVariable`s ended up
/// sharing the same rendered PreHIR binding name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CoverViolation {
    pub(crate) name: String,
    pub(crate) high_a: SsaHighVariableId,
    pub(crate) high_b: SsaHighVariableId,
}

/// `(op.address, op.seq_num) -> LoweringSite` over every op in every block of
/// `pcode`. `MaterializedVarnodeKey` identifies a definition this way (not by
/// `(block_idx, op_idx)`), so this is required to correlate it back to the
/// SSA side, which is dense-block/op indexed.
fn build_addr_seq_index(pcode: &PcodeFunction) -> HashMap<(u64, u32), LoweringSite> {
    let mut index = HashMap::default();
    for (block_idx, block) in pcode.blocks.iter().enumerate() {
        for (op_idx, op) in block.ops.iter().enumerate() {
            index.insert((op.address, op.seq_num), LoweringSite { block_idx, op_idx });
        }
    }
    index
}

/// Resolve the `SsaHighVariableId` for the value an op defines at `site`,
/// matching on `key`'s storage identity. Conservative: only resolves whole-
/// varnode (non-partial, `byte_offset == 0`) pieces -- a sub-register/piece
/// definition is skipped rather than guessed at, since a wrong guess here
/// would misreport a "measured" violation count.
fn high_variable_at_output(
    scalar_ssa: &NirScalarSsa,
    site: LoweringSite,
    key: &VarnodeKey,
) -> Option<SsaHighVariableId> {
    let op_site = SsaOpSite {
        block: u32::try_from(site.block_idx).ok()?,
        op: u32::try_from(site.op_idx).ok()?,
    };
    let storage = SsaStorageKey {
        space_id: key.space_id,
        offset: key.offset,
        size: key.size,
    };
    let pieces = scalar_ssa.operation_outputs.get(&op_site)?;
    let piece = pieces.iter().find(|p| p.byte_offset == 0)?;
    let value = scalar_ssa.values.get(piece.value.0 as usize)?;
    if value.storage != storage {
        return None;
    }
    scalar_ssa
        .value_high_variables
        .get(piece.value.0 as usize)
        .copied()
}

/// Resolve the `SsaHighVariableId` for the phi output at `block_idx` whose
/// storage matches `key` -- the SSA value `explicit_merge_bindings`' block-
/// scoped name is meant to represent at that block's entry.
fn high_variable_at_block_entry(
    scalar_ssa: &NirScalarSsa,
    block_idx: u32,
    key: &VarnodeKey,
) -> Option<SsaHighVariableId> {
    let storage = SsaStorageKey {
        space_id: key.space_id,
        offset: key.offset,
        size: key.size,
    };
    let phis = scalar_ssa.phis.get(&block_idx)?;
    let phi = phis.iter().find(|p| p.storage == storage)?;
    scalar_ssa
        .value_high_variables
        .get(phi.output.0 as usize)
        .copied()
}

fn high_variables_interfere(
    scalar_ssa: &NirScalarSsa,
    a: SsaHighVariableId,
    b: SsaHighVariableId,
) -> bool {
    let Some(high_a) = scalar_ssa.high_variables.get(a.0 as usize) else {
        return false;
    };
    let Some(high_b) = scalar_ssa.high_variables.get(b.0 as usize) else {
        return false;
    };
    high_a.cover.iter().any(|left| {
        high_b.cover.iter().any(|right| {
            left.block == right.block
                && left.start < right.end_exclusive
                && right.start < left.end_exclusive
        })
    })
}

impl<'a> PreviewBuilder<'a> {
    /// Diagnostic-only: see module docs. Pure/read-only, `FISSION_PREVIEW_DIAG`-gated caller.
    pub(crate) fn scan_cover_violations(&self) -> Vec<CoverViolation> {
        scan_cover_violations(
            &self.scalar_ssa,
            self.pcode,
            &self.materialized_vns,
            &self.explicit_merge_bindings,
        )
    }

    /// Live counterpart of `scan_cover_violations`'s core test, for gating a
    /// same-block name-reuse decision before it happens (rather than
    /// reporting it after the fact). `true` only when the SSA Cover
    /// analysis positively proves the values defined at
    /// `(block_idx, op_idx_a)`/`output_a` and `(block_idx, op_idx_b)`/
    /// `output_b` are different, interfering `SsaHighVariable`s (Ghidra's
    /// `HighIntersectTest` failure mode). `false` whenever either side
    /// can't be resolved -- this only ever blocks a reuse the SSA model can
    /// positively disprove, never blocks on missing/ambiguous data.
    pub(crate) fn cover_proves_distinct_and_interfering(
        &self,
        block_idx: usize,
        op_idx_a: usize,
        output_a: &Varnode,
        op_idx_b: usize,
        output_b: &Varnode,
    ) -> bool {
        let site_a = LoweringSite {
            block_idx,
            op_idx: op_idx_a,
        };
        let site_b = LoweringSite {
            block_idx,
            op_idx: op_idx_b,
        };
        let Some(high_a) = high_variable_at_output(&self.scalar_ssa, site_a, &VarnodeKey::from(output_a))
        else {
            return false;
        };
        let Some(high_b) = high_variable_at_output(&self.scalar_ssa, site_b, &VarnodeKey::from(output_b))
        else {
            return false;
        };
        high_a != high_b && high_variables_interfere(&self.scalar_ssa, high_a, high_b)
    }
}

/// Scan every rendered binding name for cases where it was assigned to more
/// than one interfering `SsaHighVariable`. Pure/read-only: does not touch
/// `materialized_vns`/`explicit_merge_bindings`/output.
fn scan_cover_violations(
    scalar_ssa: &NirScalarSsa,
    pcode: &PcodeFunction,
    materialized_vns: &HashMap<MaterializedVarnodeKey, String>,
    explicit_merge_bindings: &HashMap<(usize, VarnodeKey), String>,
) -> Vec<CoverViolation> {
    let addr_seq_index = build_addr_seq_index(pcode);
    // `(SsaHighVariableId, source_instruction_address)`. The address is kept
    // alongside the id so a same-name pair can be told apart from a same-
    // *instruction* pair below: a single source instruction commonly lowers
    // to several p-code ops touching the same storage at different widths
    // (e.g. a CALL's 32-bit EAX result immediately zero-extended into RAX,
    // both at the call's own address) -- the SSA model correctly treats
    // those as distinct values, but sharing a display name between them is
    // the intended "same value, different width view" case, not the
    // cross-block/cross-value name reuse this diagnostic targets. Block-
    // entry (`explicit_merge_bindings`) sites carry no single instruction
    // address, so they use `None` and are never exempted by this rule.
    let mut by_name: HashMap<&str, Vec<(SsaHighVariableId, Option<u64>)>> = HashMap::default();

    for (key, name) in materialized_vns {
        let Some(&site) = addr_seq_index.get(&(key.def_addr, key.def_seq)) else {
            continue;
        };
        if let Some(high) = high_variable_at_output(scalar_ssa, site, &key.varnode) {
            by_name
                .entry(name.as_str())
                .or_default()
                .push((high, Some(key.def_addr)));
        }
    }
    for ((block_idx, key), name) in explicit_merge_bindings {
        let Ok(block_idx) = u32::try_from(*block_idx) else {
            continue;
        };
        if let Some(high) = high_variable_at_block_entry(scalar_ssa, block_idx, key) {
            by_name.entry(name.as_str()).or_default().push((high, None));
        }
    }

    let mut violations = Vec::new();
    for (name, entries) in &by_name {
        for i in 0..entries.len() {
            for j in (i + 1)..entries.len() {
                let (high_a, addr_a) = entries[i];
                let (high_b, addr_b) = entries[j];
                let same_instruction = addr_a.is_some() && addr_a == addr_b;
                if high_a != high_b
                    && !same_instruction
                    && high_variables_interfere(scalar_ssa, high_a, high_b)
                {
                    violations.push(CoverViolation {
                        name: (*name).to_string(),
                        high_a,
                        high_b,
                    });
                }
            }
        }
    }
    violations
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pcode::PcodeBasicBlock;
    use fission_midend_core::ir::{
        SsaCoverBlock, SsaHighVariable, SsaHighVariableId, SsaValue, SsaValueDefinition, SsaValueId,
    };

    fn sample_value(id: u32, space_id: u64, offset: u64, size: u32) -> SsaValue {
        SsaValue {
            id: SsaValueId(id),
            storage: SsaStorageKey {
                space_id,
                offset,
                size,
            },
            definition: SsaValueDefinition::Input,
        }
    }

    fn sample_ssa_with_two_high_variables(covers_intersect: bool) -> NirScalarSsa {
        let mut ssa = NirScalarSsa::default();
        // Two distinct values, same storage identity (space_id/offset/size),
        // as would happen when a register is reused for two logically
        // different variables at different points in the function.
        ssa.values.push(sample_value(0, 4, 0x10, 4));
        ssa.values.push(sample_value(1, 4, 0x10, 4));
        ssa.value_high_variables = vec![SsaHighVariableId(0), SsaHighVariableId(1)];
        ssa.high_variables = vec![
            SsaHighVariable {
                id: SsaHighVariableId(0),
                members: vec![SsaValueId(0)],
                storage_family: vec![],
                crossing_guards: vec![],
                cover: vec![SsaCoverBlock {
                    block: 0,
                    start: 0,
                    end_exclusive: 5,
                }],
            },
            SsaHighVariable {
                id: SsaHighVariableId(1),
                members: vec![SsaValueId(1)],
                storage_family: vec![],
                crossing_guards: vec![],
                cover: vec![SsaCoverBlock {
                    block: 0,
                    start: if covers_intersect { 3 } else { 10 },
                    end_exclusive: if covers_intersect { 8 } else { 15 },
                }],
            },
        ];
        ssa.operation_outputs.insert(
            SsaOpSite { block: 0, op: 0 },
            vec![fission_midend_core::ir::SsaAccessPiece {
                byte_offset: 0,
                value: SsaValueId(0),
            }],
        );
        ssa.operation_outputs.insert(
            SsaOpSite { block: 0, op: 1 },
            vec![fission_midend_core::ir::SsaAccessPiece {
                byte_offset: 0,
                value: SsaValueId(1),
            }],
        );
        ssa
    }

    fn sample_pcode_two_defs(space_id: u64, offset: u64, size: u32) -> PcodeFunction {
        let vn = Varnode {
            space_id,
            offset,
            size,
            is_constant: false,
            constant_val: 0,
        };
        let op0 = PcodeOp {
            seq_num: 0,
            opcode: PcodeOpcode::Copy,
            address: 0x1000,
            output: Some(vn.clone()),
            inputs: vec![vn.clone()],
            asm_mnemonic: None,
        };
        let op1 = PcodeOp {
            seq_num: 0,
            opcode: PcodeOpcode::Copy,
            address: 0x1004,
            output: Some(vn.clone()),
            inputs: vec![vn],
            asm_mnemonic: None,
        };
        PcodeFunction {
            blocks: vec![PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                successors: vec![],
                ops: vec![op0, op1],
            }],
        }
    }

    fn materialized_vns_sharing_one_name(
        space_id: u64,
        offset: u64,
        size: u32,
    ) -> HashMap<MaterializedVarnodeKey, String> {
        let varnode = VarnodeKey {
            space_id,
            offset,
            size,
            is_constant: false,
            constant_val: 0,
        };
        let mut map = HashMap::default();
        map.insert(
            MaterializedVarnodeKey {
                varnode: varnode.clone(),
                def_addr: 0x1000,
                def_seq: 0,
            },
            "shared_name".to_string(),
        );
        map.insert(
            MaterializedVarnodeKey {
                varnode,
                def_addr: 0x1004,
                def_seq: 0,
            },
            "shared_name".to_string(),
        );
        map
    }

    #[test]
    fn intersecting_covers_under_same_name_are_flagged() {
        let ssa = sample_ssa_with_two_high_variables(true);
        let pcode = sample_pcode_two_defs(4, 0x10, 4);
        let materialized = materialized_vns_sharing_one_name(4, 0x10, 4);
        let violations = scan_cover_violations(&ssa, &pcode, &materialized, &HashMap::default());
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].name, "shared_name");
    }

    #[test]
    fn nonintersecting_covers_under_same_name_are_not_flagged() {
        let ssa = sample_ssa_with_two_high_variables(false);
        let pcode = sample_pcode_two_defs(4, 0x10, 4);
        let materialized = materialized_vns_sharing_one_name(4, 0x10, 4);
        let violations = scan_cover_violations(&ssa, &pcode, &materialized, &HashMap::default());
        assert!(violations.is_empty());
    }

    #[test]
    fn same_high_variable_under_same_name_is_never_flagged() {
        let mut ssa = sample_ssa_with_two_high_variables(true);
        // Collapse both values into the *same* HighVariable id -- a
        // legitimate merge, must never be flagged regardless of cover.
        ssa.value_high_variables = vec![SsaHighVariableId(0), SsaHighVariableId(0)];
        let pcode = sample_pcode_two_defs(4, 0x10, 4);
        let materialized = materialized_vns_sharing_one_name(4, 0x10, 4);
        let violations = scan_cover_violations(&ssa, &pcode, &materialized, &HashMap::default());
        assert!(violations.is_empty());
    }

    #[test]
    fn distinct_names_are_never_flagged_even_if_covers_intersect() {
        let ssa = sample_ssa_with_two_high_variables(true);
        let pcode = sample_pcode_two_defs(4, 0x10, 4);
        let varnode = VarnodeKey {
            space_id: 4,
            offset: 0x10,
            size: 4,
            is_constant: false,
            constant_val: 0,
        };
        let mut materialized = HashMap::default();
        materialized.insert(
            MaterializedVarnodeKey {
                varnode: varnode.clone(),
                def_addr: 0x1000,
                def_seq: 0,
            },
            "name_a".to_string(),
        );
        materialized.insert(
            MaterializedVarnodeKey {
                varnode,
                def_addr: 0x1004,
                def_seq: 0,
            },
            "name_b".to_string(),
        );
        let violations = scan_cover_violations(&ssa, &pcode, &materialized, &HashMap::default());
        assert!(violations.is_empty());
    }

    /// Two p-code ops at the *same* source instruction address (e.g. a
    /// CALL's 32-bit EAX result immediately zero-extended into 64-bit RAX)
    /// sharing a name must never be flagged, even with intersecting covers
    /// -- reusing the display name across the same instruction's own
    /// sub-defs is the intended "same value, different width" case, not a
    /// cross-value name collision. Regression test for the false positive
    /// found tracing a real corpus function (`apply_binop`): two defs both
    /// named `uVar16` at the same address, different sizes, flagged before
    /// this exemption existed.
    #[test]
    fn same_instruction_widening_chain_is_never_flagged() {
        let ssa = sample_ssa_with_two_high_variables(true);
        let vn = Varnode {
            space_id: 4,
            offset: 0x10,
            size: 4,
            is_constant: false,
            constant_val: 0,
        };
        let op0 = PcodeOp {
            seq_num: 0,
            opcode: PcodeOpcode::Copy,
            address: 0x1000,
            output: Some(vn.clone()),
            inputs: vec![vn.clone()],
            asm_mnemonic: None,
        };
        let op1 = PcodeOp {
            seq_num: 1,
            opcode: PcodeOpcode::Copy,
            address: 0x1000, // same instruction as op0
            output: Some(vn.clone()),
            inputs: vec![vn],
            asm_mnemonic: None,
        };
        let pcode = PcodeFunction {
            blocks: vec![PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                successors: vec![],
                ops: vec![op0, op1],
            }],
        };
        let varnode = VarnodeKey {
            space_id: 4,
            offset: 0x10,
            size: 4,
            is_constant: false,
            constant_val: 0,
        };
        let mut materialized = HashMap::default();
        materialized.insert(
            MaterializedVarnodeKey {
                varnode: varnode.clone(),
                def_addr: 0x1000,
                def_seq: 0,
            },
            "shared_name".to_string(),
        );
        materialized.insert(
            MaterializedVarnodeKey {
                varnode,
                def_addr: 0x1000,
                def_seq: 1,
            },
            "shared_name".to_string(),
        );
        let violations = scan_cover_violations(&ssa, &pcode, &materialized, &HashMap::default());
        assert!(violations.is_empty());
    }
}
