use super::*;
use fission_loader::loader::LoadedBinary;
use fission_midend_core::ir::CallEffectSummarySource;
pub use fission_pcode_ir::block_cfg::{
    allocate_virtual_block_target_key, block_label, block_terminator_op, branch_target_address,
    build_address_to_index_map, build_block_target_keys, build_layout_fallthrough_map,
    build_predecessor_index_map, build_successor_index_map, canonical_block_index_for_address,
    canonical_block_start_for_address, const_offset, duplicate_block_start_count,
    encode_duplicate_block_key, instruction_local_branch_target_seq,
    is_instruction_local_forward_branch, resolve_branch_target_index,
    same_block_forward_branch_target_op_idx, structuring_cfg_edges,
};

/// Extra `(from_block_idx, to_block_idx)` edges for C++ exception-handling
/// landing pads (see `fission_loader::loader::gcc_lsda`) that
/// `build_successor_index_map`'s terminator-op-based derivation can never
/// discover on its own: a landing pad is reachable only via the personality
/// routine unwinding into it at runtime, not any `Branch`/`CBranch`/
/// `BranchInd`/`Return`/fallthrough in the function's own p-code -- so
/// without this, it ends up with an empty predecessor list and gets treated
/// as dead code by structuring's orphan-block elimination.
///
/// Scoped strictly to `binary.eh_lsda`, keyed by this p-code's own function
/// entry address (the lowest-addressed block, which is always the decode
/// entry point -- see `build_cfg_blocks_with_hints`'s address-sorted block
/// ordering in `fission-sleigh`). A function with no LSDA entry -- which is
/// every function in every binary without C++ exception handling -- gets
/// zero edges here, so this can't change behavior for anything else.
pub(super) fn lsda_extra_edges(
    pcode: &PcodeFunction,
    address_to_index: &HashMap<u64, usize>,
    binary: Option<&LoadedBinary>,
) -> Vec<(usize, usize)> {
    let Some(binary) = binary else {
        return Vec::new();
    };
    let Some(entry_address) = pcode.blocks.first().map(|block| block.start_address) else {
        return Vec::new();
    };
    let Some(info) = binary.eh_lsda.get(&entry_address) else {
        return Vec::new();
    };
    info.call_sites
        .iter()
        .filter_map(|call_site| {
            let landing_pad = call_site.landing_pad?;
            let from = canonical_block_index_for_address(pcode, address_to_index, call_site.start)?;
            let to = canonical_block_index_for_address(pcode, address_to_index, landing_pad)?;
            Some((from, to))
        })
        .collect()
}

pub(super) fn block_ends_in_proven_noreturn_call(
    block: &crate::pcode::PcodeBasicBlock,
    options: &MlilPreviewOptions,
    type_context: Option<&PreviewTypeContext>,
) -> bool {
    // This repairs only the builder's synthetic lexical fallthrough. If the
    // lifted CFG itself carries an outgoing edge, preserve that stronger fact.
    if !block.successors.is_empty() {
        return false;
    }
    let Some(last_addr) = block.ops.last().map(|op| op.address) else {
        return false;
    };
    if block_terminator_op(block).is_some_and(|terminator| terminator.address == last_addr) {
        return false;
    }
    let Some(call) = block
        .ops
        .iter()
        .rev()
        .take_while(|op| op.address == last_addr)
        .find(|op| op.opcode == PcodeOpcode::Call)
    else {
        return false;
    };
    let Some(target) =
        super::builder::resolve_lifted_direct_call_target(call, options, type_context)
    else {
        return false;
    };
    type_context
        .and_then(|context| context.call_effect_summaries.get(&target))
        .is_some_and(|summary| {
            summary.may_exit == Some(true)
                && summary.source == Some(CallEffectSummarySource::GhidraNoReturnData)
        })
}

pub(super) fn prune_proven_noreturn_successors(
    pcode: &PcodeFunction,
    successors: &mut [Vec<usize>],
    options: &MlilPreviewOptions,
    type_context: Option<&PreviewTypeContext>,
) {
    for (idx, block) in pcode.blocks.iter().enumerate() {
        if block_ends_in_proven_noreturn_call(block, options, type_context)
            && let Some(block_successors) = successors.get_mut(idx)
        {
            block_successors.clear();
        }
    }
}

pub(super) fn fold_logical_chain(mut exprs: Vec<PreHirExpr>, op: PreHirBinaryOp) -> PreHirExpr {
    debug_assert!(matches!(
        op,
        PreHirBinaryOp::LogicalAnd | PreHirBinaryOp::LogicalOr
    ));
    if exprs.is_empty() {
        return PreHirExpr::Const(
            if op == PreHirBinaryOp::LogicalAnd {
                1
            } else {
                0
            },
            NirType::Bool,
        );
    }
    let first = exprs.remove(0);
    exprs
        .into_iter()
        .fold(first, |lhs, rhs| PreHirExpr::Binary {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            ty: NirType::Bool,
        })
}

pub(super) fn negate_expr(expr: PreHirExpr) -> PreHirExpr {
    match expr {
        PreHirExpr::Unary {
            op: PreHirUnaryOp::Not,
            expr,
            ..
        } => *expr,
        other => PreHirExpr::Unary {
            op: PreHirUnaryOp::Not,
            expr: Box::new(other),
            ty: NirType::Bool,
        },
    }
}

pub(super) fn strip_casts(expr: &PreHirExpr) -> PreHirExpr {
    match expr {
        PreHirExpr::Cast { expr, .. } => strip_casts(expr),
        other => other.clone(),
    }
}

pub(super) fn simplify_logical_expr(expr: PreHirExpr) -> PreHirExpr {
    match expr {
        PreHirExpr::Binary {
            op: PreHirBinaryOp::LogicalAnd,
            lhs,
            rhs,
            ty,
        } => {
            let lhs = Box::new(simplify_logical_expr(*lhs));
            let rhs = Box::new(simplify_logical_expr(*rhs));

            if let (
                PreHirExpr::Unary {
                    op: PreHirUnaryOp::Not,
                    expr: inner_lhs,
                    ..
                },
                PreHirExpr::Unary {
                    op: PreHirUnaryOp::Not,
                    expr: inner_rhs,
                    ..
                },
            ) = (&*lhs, &*rhs)
            {
                return PreHirExpr::Unary {
                    op: PreHirUnaryOp::Not,
                    expr: Box::new(PreHirExpr::Binary {
                        op: PreHirBinaryOp::LogicalOr,
                        lhs: inner_lhs.clone(),
                        rhs: inner_rhs.clone(),
                        ty,
                    }),
                    ty: NirType::Bool,
                };
            }

            PreHirExpr::Binary {
                op: PreHirBinaryOp::LogicalAnd,
                lhs,
                rhs,
                ty,
            }
        }
        PreHirExpr::Binary {
            op: PreHirBinaryOp::LogicalOr,
            lhs,
            rhs,
            ty,
        } => {
            let lhs = Box::new(simplify_logical_expr(*lhs));
            let rhs = Box::new(simplify_logical_expr(*rhs));

            if let (
                PreHirExpr::Unary {
                    op: PreHirUnaryOp::Not,
                    expr: inner_lhs,
                    ..
                },
                PreHirExpr::Unary {
                    op: PreHirUnaryOp::Not,
                    expr: inner_rhs,
                    ..
                },
            ) = (&*lhs, &*rhs)
            {
                return PreHirExpr::Unary {
                    op: PreHirUnaryOp::Not,
                    expr: Box::new(PreHirExpr::Binary {
                        op: PreHirBinaryOp::LogicalAnd,
                        lhs: inner_lhs.clone(),
                        rhs: inner_rhs.clone(),
                        ty,
                    }),
                    ty: NirType::Bool,
                };
            }

            PreHirExpr::Binary {
                op: PreHirBinaryOp::LogicalOr,
                lhs,
                rhs,
                ty,
            }
        }
        PreHirExpr::Unary { op, expr, ty } => PreHirExpr::Unary {
            op,
            expr: Box::new(simplify_logical_expr(*expr)),
            ty,
        },
        other => other,
    }
}

#[cfg(test)]
mod lsda_extra_edges_tests {
    use super::*;
    use crate::pcode::PcodeBasicBlock;
    use fission_loader::loader::gcc_lsda::{LsdaCallSite, LsdaInfo};
    use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder};
    use fission_midend_core::ir::NirCallEffectSummary;

    fn empty_block(start_address: u64) -> PcodeBasicBlock {
        PcodeBasicBlock {
            index: 0,
            start_address,
            successors: vec![],
            ops: vec![],
        }
    }

    fn direct_call_block(start_address: u64, target: u64) -> PcodeBasicBlock {
        PcodeBasicBlock {
            index: 0,
            start_address,
            successors: vec![],
            ops: vec![PcodeOp {
                seq_num: 0,
                opcode: PcodeOpcode::Call,
                address: start_address,
                output: None,
                inputs: vec![Varnode {
                    space_id: 3,
                    offset: target,
                    size: 8,
                    is_constant: false,
                    constant_val: 0,
                }],
                asm_mnemonic: None,
            }],
        }
    }

    fn noreturn_context(
        target_addr: u64,
        target: &str,
        source: CallEffectSummarySource,
    ) -> PreviewTypeContext {
        let mut context = PreviewTypeContext::default();
        context.call_targets.insert(target_addr, target.to_string());
        context.call_effect_summaries.insert(
            target.to_string(),
            NirCallEffectSummary {
                may_exit: Some(true),
                source: Some(source),
                ..NirCallEffectSummary::default()
            },
        );
        context
    }

    fn test_binary() -> fission_loader::loader::LoadedBinary {
        LoadedBinaryBuilder::new("lsda_test".to_string(), DataBuffer::Heap(vec![]))
            .format("ELF64")
            .arch_spec("x86:LE:64:default")
            .is_64bit(true)
            .build()
            .expect("build test LoadedBinary")
    }

    /// The scenario that motivated this function: a call-site block (index
    /// 0) whose only edge to the landing-pad block (index 2) comes from
    /// `binary.eh_lsda`, not from any p-code branch/call/fallthrough
    /// `build_successor_index_map` could derive on its own -- see
    /// `crates/fission-loader/src/loader/gcc_lsda.rs` and the real
    /// `guarded()`/`x64_dyn_lsda_test.elf` case this mirrors.
    #[test]
    fn resolves_landing_pad_edge_from_binary_eh_lsda() {
        let pcode = PcodeFunction {
            blocks: vec![
                empty_block(0x1000),
                empty_block(0x1010),
                empty_block(0x1020),
            ],
        };
        let address_to_index = build_address_to_index_map(&pcode);

        let mut binary = test_binary();
        binary.eh_lsda.insert(
            0x1000,
            LsdaInfo {
                lp_start: 0x1000,
                call_sites: vec![
                    LsdaCallSite {
                        start: 0x1000,
                        end: 0x1010,
                        landing_pad: Some(0x1020),
                        action_chain: vec![1],
                    },
                    LsdaCallSite {
                        start: 0x1010,
                        end: 0x1020,
                        landing_pad: None, // no edge: must be skipped, not (idx, None)
                        action_chain: vec![],
                    },
                ],
                type_table: vec![],
            },
        );

        let edges = lsda_extra_edges(&pcode, &address_to_index, Some(&binary));
        assert_eq!(edges, vec![(0, 2)]);
    }

    #[test]
    fn returns_empty_without_a_binary() {
        let pcode = PcodeFunction {
            blocks: vec![empty_block(0x1000)],
        };
        let address_to_index = build_address_to_index_map(&pcode);
        assert!(lsda_extra_edges(&pcode, &address_to_index, None).is_empty());
    }

    #[test]
    fn returns_empty_when_function_has_no_lsda_entry() {
        let pcode = PcodeFunction {
            blocks: vec![empty_block(0x1000)],
        };
        let address_to_index = build_address_to_index_map(&pcode);
        let binary = test_binary(); // eh_lsda left empty
        assert!(lsda_extra_edges(&pcode, &address_to_index, Some(&binary)).is_empty());
    }

    /// `build_successor_index_map`'s own instruction-derived edges must
    /// stay untouched when there's no LSDA data for the function -- the
    /// merge in `builder/init.rs` only ever *adds* edges, on top of
    /// whatever this already computes.
    #[test]
    fn build_successor_index_map_unaffected_by_absent_lsda_data() {
        let pcode = PcodeFunction {
            blocks: vec![empty_block(0x1000), empty_block(0x1010)],
        };
        let address_to_index = build_address_to_index_map(&pcode);
        let layout_fallthrough = build_layout_fallthrough_map(&pcode);
        let successors = build_successor_index_map(&pcode, &address_to_index, &layout_fallthrough);
        assert_eq!(successors, vec![vec![1], vec![]]);
    }

    #[test]
    fn proven_noreturn_call_prunes_lexical_fallthrough() {
        let call_addr = 0x1000;
        let target_addr = 0x5000;
        let pcode = PcodeFunction {
            blocks: vec![
                direct_call_block(call_addr, target_addr),
                empty_block(0x1010),
            ],
        };
        let options = MlilPreviewOptions::default();
        let context = noreturn_context(
            target_addr,
            "fatal",
            CallEffectSummarySource::GhidraNoReturnData,
        );
        let mut successors = vec![vec![1], vec![]];

        assert!(block_ends_in_proven_noreturn_call(
            &pcode.blocks[0],
            &options,
            Some(&context),
        ));
        prune_proven_noreturn_successors(&pcode, &mut successors, &options, Some(&context));

        assert_eq!(successors, vec![Vec::<usize>::new(), Vec::<usize>::new()]);
    }

    #[test]
    fn possible_exit_without_noreturn_provenance_keeps_fallthrough() {
        let call_addr = 0x1000;
        let pcode = PcodeFunction {
            blocks: vec![direct_call_block(call_addr, 0x5000), empty_block(0x1010)],
        };
        let options = MlilPreviewOptions::default();
        let context = noreturn_context(
            0x5000,
            "maybe_fatal",
            CallEffectSummarySource::CallTargetRef,
        );
        let mut successors = vec![vec![1], vec![]];

        assert!(!block_ends_in_proven_noreturn_call(
            &pcode.blocks[0],
            &options,
            Some(&context),
        ));
        prune_proven_noreturn_successors(&pcode, &mut successors, &options, Some(&context));

        assert_eq!(successors, vec![vec![1], vec![]]);
    }

    #[test]
    fn lifted_successor_outweighs_noreturn_summary() {
        let call_addr = 0x1000;
        let target_addr = 0x5000;
        let mut call_block = direct_call_block(call_addr, target_addr);
        call_block.successors.push(0x1010);
        let pcode = PcodeFunction {
            blocks: vec![call_block, empty_block(0x1010)],
        };
        let options = MlilPreviewOptions::default();
        let context = noreturn_context(
            target_addr,
            "fatal",
            CallEffectSummarySource::GhidraNoReturnData,
        );
        let mut successors = vec![vec![1], vec![]];

        assert!(!block_ends_in_proven_noreturn_call(
            &pcode.blocks[0],
            &options,
            Some(&context),
        ));
        prune_proven_noreturn_successors(&pcode, &mut successors, &options, Some(&context));

        assert_eq!(successors, vec![vec![1], vec![]]);
    }
}
