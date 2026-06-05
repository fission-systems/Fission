use super::*;
use crate::PcodeBasicBlock;
use crate::nir::builder::materialize::test_support::{
    block, block_at, constant, int, op, pcode_function,
};
use crate::nir::render_mlil_preview;

fn register(space_id: u64, offset: u64, size: u32) -> Varnode {
    Varnode {
        space_id,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    }
}

#[test]
fn call_result_observation_accepts_partial_return_register_reads() {
    let ret_eax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 4);
    let ebx = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x0c, 4);
    let out = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x100, 4);
    let block = block(vec![
        op(1, PcodeOpcode::Call, None, vec![constant(0x2000)]),
        op(2, PcodeOpcode::IntAdd, Some(out), vec![ebx, ret_eax]),
    ]);
    let pcode = pcode_function(vec![block.clone()]);
    let options = crate::nir::builder::materialize::test_support::test_options();
    let builder = PreviewBuilder::new(&pcode, &options, None);

    assert!(builder.call_result_is_observed(&block, 0));
}

#[test]
fn predecessor_assignment_accepts_predicate_merge_consumers() {
    let pcode = pcode_function(vec![block(Vec::new())]);
    let options = crate::nir::builder::materialize::test_support::test_options();
    let builder = PreviewBuilder::new(&pcode, &options, None);
    let proof = MergeBindingCandidateProof {
        merge_block: 0x2000,
        predecessor_count: 3,
        missing_incoming_count: 0,
        conflicting_incoming_count: 1,
        incoming_value_kinds: vec![
            MergeBindingCandidateIncomingKind::VarOrConst,
            MergeBindingCandidateIncomingKind::Arithmetic,
        ],
        consumer_kind: DisallowedSingleConsumerConsumerKind::Predicate,
        rhs_kind: DisallowedSingleConsumerRhsKind::VarOrConst,
        can_synthesize_phi_like_binding: true,
        result: MergeBindingCandidateResult::PhiLikeBindingCandidate,
    };

    assert!(builder.merge_binding_proof_allows_predecessor_assignment(&proof, false,));
}

#[test]
fn direct_successor_return_register_merge_uses_shared_edge_binding() {
    let rax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 8);
    let r12 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0xa0, 4);
    let pcode = pcode_function(vec![
        PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![2],
            ops: vec![
                op(1, PcodeOpcode::Copy, Some(rax.clone()), vec![constant(5)]),
                op(2, PcodeOpcode::Branch, None, vec![constant(0x1020)]),
            ],
        },
        PcodeBasicBlock {
            index: 1,
            start_address: 0x1010,
            successors: vec![2],
            ops: vec![
                op(3, PcodeOpcode::Copy, Some(rax.clone()), vec![constant(7)]),
                op(4, PcodeOpcode::Branch, None, vec![constant(0x1020)]),
            ],
        },
        PcodeBasicBlock {
            index: 2,
            start_address: 0x1020,
            successors: Vec::new(),
            ops: vec![op(
                5,
                PcodeOpcode::IntAdd,
                Some(r12.clone()),
                vec![r12, rax.clone()],
            )],
        },
    ]);
    let options = crate::nir::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    let rhs = HirExpr::Const(5, type_from_size(8, false));

    let name = builder
        .merge_binding_name_for_direct_successor_accumulator(&pcode.blocks[0], &rax, &rhs)
        .expect("shared return register merge binding");

    assert!(
        builder
            .explicit_merge_bindings
            .contains_key(&(2, VarnodeKey::from(&rax)))
    );
    assert_eq!(
        builder
            .explicit_merge_bindings
            .get(&(2, VarnodeKey::from(&rax))),
        Some(&name)
    );
}

#[test]
fn direct_successor_return_register_merge_rejects_side_effect_after_def() {
    let rax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 8);
    let r12 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0xa0, 4);
    let ptr = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x28, 8);
    let pcode = pcode_function(vec![
        PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![2],
            ops: vec![
                op(1, PcodeOpcode::Copy, Some(rax.clone()), vec![constant(5)]),
                op(
                    2,
                    PcodeOpcode::Store,
                    None,
                    vec![constant(3), ptr, constant(0)],
                ),
                op(3, PcodeOpcode::Branch, None, vec![constant(0x1020)]),
            ],
        },
        PcodeBasicBlock {
            index: 1,
            start_address: 0x1010,
            successors: vec![2],
            ops: vec![
                op(4, PcodeOpcode::Copy, Some(rax.clone()), vec![constant(7)]),
                op(5, PcodeOpcode::Branch, None, vec![constant(0x1020)]),
            ],
        },
        PcodeBasicBlock {
            index: 2,
            start_address: 0x1020,
            successors: Vec::new(),
            ops: vec![op(
                6,
                PcodeOpcode::IntAdd,
                Some(r12.clone()),
                vec![r12, rax.clone()],
            )],
        },
    ]);
    let options = crate::nir::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    let rhs = HirExpr::Const(5, type_from_size(8, false));

    assert!(
        builder
            .merge_binding_name_for_direct_successor_accumulator(&pcode.blocks[0], &rax, &rhs)
            .is_none()
    );
}

#[test]
fn direct_successor_accumulator_merge_uses_shared_gpr_edge_binding() {
    let r12 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0xa0, 8);
    let rax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 8);
    let pcode = pcode_function(vec![
        PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![2],
            ops: vec![
                op(1, PcodeOpcode::Copy, Some(r12.clone()), vec![constant(5)]),
                op(2, PcodeOpcode::Branch, None, vec![constant(0x1020)]),
            ],
        },
        PcodeBasicBlock {
            index: 1,
            start_address: 0x1010,
            successors: vec![2],
            ops: vec![
                op(3, PcodeOpcode::Copy, Some(r12.clone()), vec![constant(7)]),
                op(4, PcodeOpcode::Branch, None, vec![constant(0x1020)]),
            ],
        },
        PcodeBasicBlock {
            index: 2,
            start_address: 0x1020,
            successors: Vec::new(),
            ops: vec![op(
                5,
                PcodeOpcode::IntAdd,
                Some(rax),
                vec![r12.clone(), constant(1)],
            )],
        },
    ]);
    let options = crate::nir::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    let rhs = HirExpr::Const(5, type_from_size(8, false));

    let name = builder
        .merge_binding_name_for_direct_successor_accumulator(&pcode.blocks[0], &r12, &rhs)
        .expect("shared accumulator merge binding");

    assert_eq!(
        builder
            .explicit_merge_bindings
            .get(&(2, VarnodeKey::from(&r12))),
        Some(&name)
    );
}

#[test]
fn direct_successor_accumulator_merge_rejects_partial_register_output() {
    let r12d = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0xa0, 4);
    let rax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 8);
    let pcode = pcode_function(vec![
        PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![2],
            ops: vec![
                op(1, PcodeOpcode::Copy, Some(r12d.clone()), vec![constant(5)]),
                op(2, PcodeOpcode::Branch, None, vec![constant(0x1020)]),
            ],
        },
        PcodeBasicBlock {
            index: 1,
            start_address: 0x1010,
            successors: vec![2],
            ops: vec![
                op(3, PcodeOpcode::Copy, Some(r12d.clone()), vec![constant(7)]),
                op(4, PcodeOpcode::Branch, None, vec![constant(0x1020)]),
            ],
        },
        PcodeBasicBlock {
            index: 2,
            start_address: 0x1020,
            successors: Vec::new(),
            ops: vec![op(
                5,
                PcodeOpcode::IntAdd,
                Some(rax),
                vec![r12d.clone(), constant(1)],
            )],
        },
    ]);
    let options = crate::nir::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    let rhs = HirExpr::Const(5, type_from_size(4, false));

    assert!(
        builder
            .merge_binding_name_for_direct_successor_accumulator(&pcode.blocks[0], &r12d, &rhs,)
            .is_none()
    );
}

/// Mirrors `conditional_loop_exit_accumulator_merge_uses_seeded_edge_binding` but
/// uses EAX (size=4, offset=0) instead of r10 (size=8). This is the canonical
/// pattern for a C `int`-returning loop accumulator in x86-64, e.g.:
///   `int sum_array(int *arr, int n) { int s = 0; for (...) s += ...; return s; }`
/// The fix to allow `size == 4` for the primary ABI return register must accept this.
#[test]
fn conditional_loop_exit_accumulator_merge_accepts_32bit_return_register_eax() {
    let eax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x00, 4);
    let rax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x00, 8);
    let cond = register(UNIQUE_SPACE_ID, 0x300, 1);
    let pcode = pcode_function(vec![
        PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![1],
            ops: vec![
                op(1, PcodeOpcode::Copy, Some(eax.clone()), vec![constant(0)]),
                op(2, PcodeOpcode::IntZExt, Some(rax.clone()), vec![eax.clone()]),
                op(3, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
            ],
        },
        PcodeBasicBlock {
            index: 1,
            start_address: 0x1010,
            successors: vec![2, 3],
            ops: vec![op(4, PcodeOpcode::CBranch, None, vec![constant(0x1020), cond.clone()])],
        },
        PcodeBasicBlock {
            index: 2,
            start_address: 0x1020,
            successors: Vec::new(),
            ops: vec![op(5, PcodeOpcode::Return, None, vec![rax.clone()])],
        },
        PcodeBasicBlock {
            index: 3,
            start_address: 0x1030,
            successors: vec![1, 2],
            ops: vec![
                op(6, PcodeOpcode::IntAdd, Some(eax.clone()), vec![eax.clone(), constant(1)]),
                op(7, PcodeOpcode::CBranch, None, vec![constant(0x1010), cond]),
            ],
        },
    ]);
    let options = crate::nir::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.successors[3] = vec![1, 2];
    builder.predecessors[1] = vec![0, 3];
    builder.predecessors[2] = vec![1, 3];
    builder.loop_bodies = vec![crate::nir::structuring::loop_analysis::LoopBody {
        head: 1,
        tails: vec![3],
        body: vec![1, 3],
        exit_idx: Some(2),
        all_exits: vec![2],
    }];
    let rhs = HirExpr::Binary {
        op: HirBinaryOp::Add,
        lhs: Box::new(HirExpr::Var("rax".to_string())),
        rhs: Box::new(HirExpr::Const(1, type_from_size(4, false))),
        ty: type_from_size(4, false),
    };

    assert_eq!(builder.canonical_x86_gpr64_name_for_value(&eax), Some(("rax", 0)));
    assert!(builder.loop_header_external_predecessors_seed_zero(
        1, &builder.loop_bodies[0], 0, false
    ));
    assert!(builder.block_reads_merge_input_before_redefinition(&pcode.blocks[2], &eax));
    assert!(!builder.loop_body_has_side_entry_or_irreducible_edge(&builder.loop_bodies[0]));
    assert!(builder
        .last_redefinition_index_before_terminator(&pcode.blocks[3], &eax)
        .is_some());

    let name = builder.with_lowering_site(
        LoweringSite { block_idx: 3, op_idx: 0 },
        |builder| {
            builder
                .merge_binding_name_for_direct_successor_accumulator(&pcode.blocks[3], &eax, &rhs)
                .expect("EAX (32-bit return register) must be accepted as a loop accumulator")
        },
    );

    assert_eq!(
        builder.explicit_merge_bindings.get(&(2, VarnodeKey::from(&eax))),
        Some(&name)
    );
    // Initializer must be 32-bit zero (output.size=4), not 64-bit (pointer_size=8)
    assert_eq!(
        builder.temps.get(&name).and_then(|b| b.initializer.as_ref()),
        Some(&HirExpr::Const(0, type_from_size(4, false)))
    );
}

#[test]
fn conditional_loop_exit_accumulator_merge_uses_seeded_edge_binding() {

    let r10d = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x90, 4);
    let r10 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x90, 8);
    let cond = register(UNIQUE_SPACE_ID, 0x300, 1);
    let pcode = pcode_function(vec![
        PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![1],
            ops: vec![
                op(1, PcodeOpcode::Copy, Some(r10d.clone()), vec![constant(0)]),
                op(
                    2,
                    PcodeOpcode::IntZExt,
                    Some(r10.clone()),
                    vec![r10d.clone()],
                ),
                op(3, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
            ],
        },
        PcodeBasicBlock {
            index: 1,
            start_address: 0x1010,
            successors: vec![2, 3],
            ops: vec![op(
                4,
                PcodeOpcode::CBranch,
                None,
                vec![constant(0x1020), cond.clone()],
            )],
        },
        PcodeBasicBlock {
            index: 2,
            start_address: 0x1020,
            successors: Vec::new(),
            ops: vec![op(5, PcodeOpcode::Return, None, vec![r10.clone()])],
        },
        PcodeBasicBlock {
            index: 3,
            start_address: 0x1030,
            successors: vec![1, 2],
            ops: vec![
                op(6, PcodeOpcode::IntZExt, Some(r10.clone()), vec![r10d]),
                op(7, PcodeOpcode::CBranch, None, vec![constant(0x1010), cond]),
            ],
        },
    ]);
    let options = crate::nir::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.successors[3] = vec![1, 2];
    builder.predecessors[1] = vec![0, 3];
    builder.predecessors[2] = vec![1, 3];
    builder.loop_bodies = vec![crate::nir::structuring::loop_analysis::LoopBody {
        head: 1,
        tails: vec![3],
        body: vec![1, 3],
        exit_idx: Some(2),
        all_exits: vec![2],
    }];
    let rhs = HirExpr::Const(7, type_from_size(8, false));
    assert_eq!(
        builder.canonical_x86_gpr64_name_for_value(&r10),
        Some(("r10", 10))
    );
    assert!(builder.loop_header_external_predecessors_seed_zero(
        1,
        &builder.loop_bodies[0],
        10,
        false
    ));
    assert!(builder.block_reads_merge_input_before_redefinition(&pcode.blocks[2], &r10));
    assert!(!builder.block_reads_merge_input_before_redefinition(&pcode.blocks[1], &r10));
    assert!(!builder.loop_body_has_side_entry_or_irreducible_edge(&builder.loop_bodies[0]));
    assert_eq!(builder.predecessors[2], vec![1, 3]);
    assert!(
        builder
            .last_redefinition_index_before_terminator(&pcode.blocks[3], &r10)
            .is_some()
    );

    let name = builder.with_lowering_site(
        LoweringSite {
            block_idx: 3,
            op_idx: 0,
        },
        |builder| {
            builder
                .merge_binding_name_for_direct_successor_accumulator(&pcode.blocks[3], &r10, &rhs)
                .expect("conditional loop-exit accumulator merge binding")
        },
    );

    assert_eq!(
        builder
            .explicit_merge_bindings
            .get(&(2, VarnodeKey::from(&r10))),
        Some(&name)
    );
    assert_eq!(
        builder
            .temps
            .get(&name)
            .and_then(|binding| binding.initializer.as_ref()),
        Some(&HirExpr::Const(0, type_from_size(8, false)))
    );
}

#[test]
fn conditional_loop_exit_accumulator_merge_uses_external_seed_binding() {
    let rax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 8);
    let cond = register(UNIQUE_SPACE_ID, 0x300, 1);
    let pcode = pcode_function(vec![
        PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![2, 1],
            ops: vec![
                op(1, PcodeOpcode::Copy, Some(rax.clone()), vec![constant(10)]),
                op(
                    2,
                    PcodeOpcode::CBranch,
                    None,
                    vec![constant(0x1020), cond.clone()],
                ),
            ],
        },
        PcodeBasicBlock {
            index: 1,
            start_address: 0x1010,
            successors: vec![3],
            ops: vec![op(3, PcodeOpcode::Branch, None, vec![constant(0x1030)])],
        },
        PcodeBasicBlock {
            index: 2,
            start_address: 0x1020,
            successors: Vec::new(),
            ops: vec![op(4, PcodeOpcode::Return, None, vec![rax.clone()])],
        },
        PcodeBasicBlock {
            index: 3,
            start_address: 0x1030,
            successors: vec![1, 2],
            ops: vec![
                op(5, PcodeOpcode::Copy, Some(rax.clone()), vec![constant(7)]),
                op(6, PcodeOpcode::CBranch, None, vec![constant(0x1010), cond]),
            ],
        },
    ]);
    let options = crate::nir::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.successors[3] = vec![1, 2];
    builder.predecessors[2] = vec![0, 3];
    builder.loop_bodies = vec![crate::nir::structuring::loop_analysis::LoopBody {
        head: 1,
        tails: vec![3],
        body: vec![1, 3],
        exit_idx: Some(2),
        all_exits: vec![2],
    }];
    let external_rhs = HirExpr::Const(10, type_from_size(8, false));
    let latch_rhs = HirExpr::Const(7, type_from_size(8, false));

    let external_name = builder.with_lowering_site(
        LoweringSite {
            block_idx: 0,
            op_idx: 0,
        },
        |builder| {
            builder
                .merge_binding_name_for_direct_successor_accumulator(
                    &pcode.blocks[0],
                    &rax,
                    &external_rhs,
                )
                .expect("external seed merge binding")
        },
    );
    let latch_name = builder.with_lowering_site(
        LoweringSite {
            block_idx: 3,
            op_idx: 0,
        },
        |builder| {
            builder
                .merge_binding_name_for_direct_successor_accumulator(
                    &pcode.blocks[3],
                    &rax,
                    &latch_rhs,
                )
                .expect("loop latch merge binding")
        },
    );

    assert_eq!(external_name, latch_name);
    assert_eq!(
        builder
            .explicit_merge_bindings
            .get(&(2, VarnodeKey::from(&rax))),
        Some(&external_name)
    );
    assert!(
        builder
            .temps
            .get(&external_name)
            .and_then(|binding| binding.initializer.as_ref())
            .is_none(),
        "external seed path is assigned by its predecessor, not by a broad initializer"
    );
}

#[test]
fn stack_home_accumulator_store_uses_seeded_live_gpr_binding() {
    let ebp = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x14, 4);
    let rbp = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x28, 8);
    let rsp_addr = register(UNIQUE_SPACE_ID, 0x200, 8);
    let cond = register(UNIQUE_SPACE_ID, 0x300, 1);
    let mut store = op(
        2,
        PcodeOpcode::Store,
        None,
        vec![constant(0), rsp_addr, ebp.clone()],
    );
    store.asm_mnemonic = Some("MOV dword ptr [RSP+0x4c], EBP".to_string());
    let pcode = pcode_function(vec![
        block_at(
            0x1000,
            0,
            vec![
                op(1, PcodeOpcode::Copy, Some(ebp.clone()), vec![constant(0)]),
                op(10, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
            ],
        ),
        block_at(
            0x1010,
            1,
            vec![
                store.clone(),
                op(3, PcodeOpcode::CBranch, None, vec![constant(0x1030), cond]),
            ],
        ),
        block_at(
            0x1020,
            2,
            vec![
                op(
                    4,
                    PcodeOpcode::IntAdd,
                    Some(rbp.clone()),
                    vec![rbp.clone(), constant(1)],
                ),
                op(5, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
            ],
        ),
        block_at(
            0x1030,
            3,
            vec![op(6, PcodeOpcode::Return, None, vec![constant(0)])],
        ),
    ]);
    let options = crate::nir::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);

    let rhs = builder
        .stack_home_accumulator_store_rhs(&pcode.blocks[1], 0, &store, "home_4c", &ebp)
        .expect("stack-home accumulator merge");

    assert_eq!(rhs, HirExpr::Var("rbp".to_string()));
    assert!(builder.params.is_empty(), "must not promote rbp to a param");
    assert_eq!(
        builder
            .temps
            .get("rbp")
            .and_then(|binding| binding.initializer.as_ref()),
        Some(&HirExpr::Const(0, type_from_size(8, false)))
    );
}

#[test]
fn stack_home_accumulator_store_accepts_joined_backedge_defs() {
    let ebp = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x28, 4);
    let rbp = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x28, 8);
    let rsp_addr = register(UNIQUE_SPACE_ID, 0x200, 8);
    let store_value = register(UNIQUE_SPACE_ID, 0xd400, 4);
    let cond = register(UNIQUE_SPACE_ID, 0x300, 1);
    let mut store = op(
        3,
        PcodeOpcode::Store,
        None,
        vec![constant(0), rsp_addr, store_value.clone()],
    );
    store.asm_mnemonic = Some("MOV dword ptr [RSP+0x4c], EBP".to_string());
    let pcode = pcode_function(vec![
        block_at(
            0x1000,
            0,
            vec![
                op(1, PcodeOpcode::Copy, Some(ebp.clone()), vec![constant(0)]),
                op(10, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
            ],
        ),
        block_at(
            0x1010,
            1,
            vec![
                op(
                    2,
                    PcodeOpcode::Copy,
                    Some(store_value.clone()),
                    vec![ebp.clone()],
                ),
                store.clone(),
                op(
                    4,
                    PcodeOpcode::CBranch,
                    None,
                    vec![constant(0x1060), cond.clone()],
                ),
            ],
        ),
        block_at(
            0x1020,
            2,
            vec![op(
                5,
                PcodeOpcode::CBranch,
                None,
                vec![constant(0x1040), cond.clone()],
            )],
        ),
        block_at(
            0x1030,
            3,
            vec![
                op(
                    6,
                    PcodeOpcode::IntAdd,
                    Some(rbp.clone()),
                    vec![rbp.clone(), constant(1)],
                ),
                op(7, PcodeOpcode::Branch, None, vec![constant(0x1050)]),
            ],
        ),
        block_at(
            0x1040,
            4,
            vec![
                op(
                    8,
                    PcodeOpcode::IntAdd,
                    Some(rbp.clone()),
                    vec![rbp.clone(), constant(2)],
                ),
                op(9, PcodeOpcode::Branch, None, vec![constant(0x1050)]),
            ],
        ),
        block_at(
            0x1050,
            5,
            vec![op(11, PcodeOpcode::Branch, None, vec![constant(0x1010)])],
        ),
        block_at(
            0x1060,
            6,
            vec![op(12, PcodeOpcode::Return, None, vec![constant(0)])],
        ),
    ]);
    let options = crate::nir::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);

    let rhs = builder.with_lowering_site(
        LoweringSite {
            block_idx: 1,
            op_idx: 1,
        },
        |builder| {
            builder
                .stack_home_accumulator_store_rhs(
                    &pcode.blocks[1],
                    1,
                    &store,
                    "home_4c",
                    &store_value,
                )
                .expect("stack-home accumulator merge across joined backedge")
        },
    );

    assert_eq!(rhs, HirExpr::Var("rbp".to_string()));
    assert!(builder.params.is_empty(), "must not promote rbp to a param");
}

#[test]
fn block_entry_accumulator_read_uses_joined_live_gpr_binding() {
    let rbp = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x28, 8);
    let tmp = register(UNIQUE_SPACE_ID, 0x8f00, 8);
    let cond = register(UNIQUE_SPACE_ID, 0x300, 1);
    let read_op = op(
        10,
        PcodeOpcode::IntAdd,
        Some(tmp),
        vec![rbp.clone(), constant(1)],
    );
    let pcode = pcode_function(vec![
        block_at(
            0x1000,
            0,
            vec![
                op(1, PcodeOpcode::Copy, Some(rbp.clone()), vec![constant(0)]),
                op(2, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
            ],
        ),
        block_at(
            0x1010,
            1,
            vec![op(
                3,
                PcodeOpcode::CBranch,
                None,
                vec![constant(0x1060), cond.clone()],
            )],
        ),
        block_at(
            0x1020,
            2,
            vec![op(
                4,
                PcodeOpcode::CBranch,
                None,
                vec![constant(0x1030), cond.clone()],
            )],
        ),
        block_at(
            0x1030,
            3,
            vec![
                op(
                    5,
                    PcodeOpcode::IntAdd,
                    Some(rbp.clone()),
                    vec![rbp.clone(), constant(1)],
                ),
                op(6, PcodeOpcode::Branch, None, vec![constant(0x1050)]),
            ],
        ),
        block_at(
            0x1040,
            4,
            vec![
                op(
                    7,
                    PcodeOpcode::IntAdd,
                    Some(rbp.clone()),
                    vec![rbp.clone(), constant(2)],
                ),
                op(8, PcodeOpcode::Branch, None, vec![constant(0x1050)]),
            ],
        ),
        block_at(
            0x1050,
            5,
            vec![
                read_op.clone(),
                op(11, PcodeOpcode::Branch, None, vec![constant(0x1060)]),
            ],
        ),
        block_at(
            0x1060,
            6,
            vec![op(12, PcodeOpcode::Return, None, vec![constant(0)])],
        ),
    ]);
    let options = crate::nir::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.predecessors[5] = vec![3, 4];
    builder.loop_bodies = vec![crate::nir::structuring::loop_analysis::LoopBody {
        head: 1,
        tails: vec![5],
        body: vec![1, 2, 3, 4, 5],
        exit_idx: Some(6),
        all_exits: vec![6],
    }];
    let stale_rhs = HirExpr::Binary {
        op: HirBinaryOp::Add,
        lhs: Box::new(HirExpr::Var("xVar53".to_string())),
        rhs: Box::new(HirExpr::Const(1, int(64))),
        ty: int(64),
    };

    let rewritten = builder.with_lowering_site(
        LoweringSite {
            block_idx: 5,
            op_idx: 0,
        },
        |builder| {
            builder.rewrite_block_entry_accumulator_rhs_with_live_gpr(
                pcode.blocks[5].start_address,
                &read_op,
                stale_rhs,
            )
        },
    );

    assert_eq!(
        rewritten,
        HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(HirExpr::Var("rbp".to_string())),
            rhs: Box::new(HirExpr::Const(1, int(64))),
            ty: int(64),
        }
    );
    assert!(builder.params.is_empty(), "must not promote rbp to a param");
}

#[test]
fn block_entry_accumulator_read_projects_full_width_explicit_merge_for_partial_read() {
    let rsi = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x30, 8);
    let esi = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x30, 4);
    let rbx = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x18, 8);
    let tmp = register(UNIQUE_SPACE_ID, 0x9400, 4);
    let read_op = op(
        30,
        PcodeOpcode::IntSub,
        Some(tmp),
        vec![rbx.clone(), esi.clone()],
    );
    let pcode = pcode_function(vec![block_at(0x1000, 0, vec![read_op.clone()])]);
    let options = crate::nir::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    let binding = builder.ensure_explicit_merge_binding_for_block(0, &rsi);
    let stale_rhs = HirExpr::Binary {
        op: HirBinaryOp::Sub,
        lhs: Box::new(HirExpr::Var("rbx".to_string())),
        rhs: Box::new(HirExpr::Var("xVar49".to_string())),
        ty: int(32),
    };

    let rewritten = builder.with_lowering_site(
        LoweringSite {
            block_idx: 0,
            op_idx: 0,
        },
        |builder| {
            builder.rewrite_block_entry_accumulator_rhs_with_live_gpr(
                pcode.blocks[0].start_address,
                &read_op,
                stale_rhs,
            )
        },
    );

    assert_eq!(
        rewritten,
        HirExpr::Binary {
            op: HirBinaryOp::Sub,
            lhs: Box::new(HirExpr::Var("rbx".to_string())),
            rhs: Box::new(HirExpr::Cast {
                ty: int(32),
                expr: Box::new(HirExpr::Var(binding.name)),
            }),
            ty: int(32),
        }
    );
}

#[test]
fn block_entry_partial_gpr_read_uses_pred_restore_binding() {
    let rsi = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x30, 8);
    let esi = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x30, 4);
    let r14 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0xd0, 8);
    let rbx = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x18, 8);
    let tmp = register(UNIQUE_SPACE_ID, 0x9500, 4);
    let read_op = op(
        30,
        PcodeOpcode::IntSub,
        Some(tmp),
        vec![rbx.clone(), esi.clone()],
    );
    let pcode = pcode_function(vec![
        block_at(
            0x1000,
            0,
            vec![
                op(1, PcodeOpcode::Copy, Some(rsi.clone()), vec![r14.clone()]),
                op(2, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
            ],
        ),
        block_at(
            0x1010,
            1,
            vec![op(3, PcodeOpcode::Branch, None, vec![constant(0x1030)])],
        ),
        block_at(
            0x1020,
            2,
            vec![
                op(4, PcodeOpcode::Copy, Some(rsi.clone()), vec![r14.clone()]),
                op(5, PcodeOpcode::Branch, None, vec![constant(0x1030)]),
            ],
        ),
        block_at(0x1030, 3, vec![read_op.clone()]),
    ]);
    let options = crate::nir::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.predecessors[1] = vec![0];
    builder.predecessors[3] = vec![1, 2];
    builder.materialized_vns.insert(
        MaterializedVarnodeKey::new(&rsi, &pcode.blocks[0].ops[0]),
        "limit".to_string(),
    );
    builder.materialized_vns.insert(
        MaterializedVarnodeKey::new(&rsi, &pcode.blocks[2].ops[0]),
        "limit".to_string(),
    );
    builder.temps.insert(
        "limit".to_string(),
        NirBinding {
            name: "limit".to_string(),
            ty: int(64),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::TempPreserved),
            initializer: None,
        },
    );
    let stale_rhs = HirExpr::Binary {
        op: HirBinaryOp::Sub,
        lhs: Box::new(HirExpr::Var("rbx".to_string())),
        rhs: Box::new(HirExpr::Var("xVar49".to_string())),
        ty: int(32),
    };

    let rewritten = builder.with_lowering_site(
        LoweringSite {
            block_idx: 3,
            op_idx: 0,
        },
        |builder| {
            builder.rewrite_block_entry_accumulator_rhs_with_live_gpr(
                pcode.blocks[3].start_address,
                &read_op,
                stale_rhs,
            )
        },
    );

    assert_eq!(
        rewritten,
        HirExpr::Binary {
            op: HirBinaryOp::Sub,
            lhs: Box::new(HirExpr::Var("rbx".to_string())),
            rhs: Box::new(HirExpr::Cast {
                ty: int(32),
                expr: Box::new(HirExpr::Var("limit".to_string())),
            }),
            ty: int(32),
        }
    );
    assert!(builder.params.is_empty(), "must not promote rsi to a param");
}

#[test]
fn block_entry_partial_gpr_read_rejects_side_effect_after_pred_def() {
    let rsi = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x30, 8);
    let esi = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x30, 4);
    let r14 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0xd0, 8);
    let rbx = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x18, 8);
    let tmp = register(UNIQUE_SPACE_ID, 0x9600, 4);
    let read_op = op(
        30,
        PcodeOpcode::IntSub,
        Some(tmp),
        vec![rbx.clone(), esi.clone()],
    );
    let pcode = pcode_function(vec![
        block_at(
            0x1000,
            0,
            vec![
                op(1, PcodeOpcode::Copy, Some(rsi.clone()), vec![r14.clone()]),
                op(2, PcodeOpcode::Call, None, vec![constant(0x2000)]),
                op(3, PcodeOpcode::Branch, None, vec![constant(0x1020)]),
            ],
        ),
        block_at(
            0x1010,
            1,
            vec![
                op(4, PcodeOpcode::Copy, Some(rsi.clone()), vec![r14.clone()]),
                op(5, PcodeOpcode::Branch, None, vec![constant(0x1020)]),
            ],
        ),
        block_at(0x1020, 2, vec![read_op.clone()]),
    ]);
    let options = crate::nir::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.predecessors[2] = vec![0, 1];
    builder.materialized_vns.insert(
        MaterializedVarnodeKey::new(&rsi, &pcode.blocks[0].ops[0]),
        "limit".to_string(),
    );
    builder.materialized_vns.insert(
        MaterializedVarnodeKey::new(&rsi, &pcode.blocks[1].ops[0]),
        "limit".to_string(),
    );
    let stale_rhs = HirExpr::Binary {
        op: HirBinaryOp::Sub,
        lhs: Box::new(HirExpr::Var("rbx".to_string())),
        rhs: Box::new(HirExpr::Var("xVar49".to_string())),
        ty: int(32),
    };

    let rewritten = builder.with_lowering_site(
        LoweringSite {
            block_idx: 2,
            op_idx: 0,
        },
        |builder| {
            builder.rewrite_block_entry_accumulator_rhs_with_live_gpr(
                pcode.blocks[2].start_address,
                &read_op,
                stale_rhs.clone(),
            )
        },
    );

    assert_eq!(rewritten, stale_rhs);
}

#[test]
fn block_entry_accumulator_read_accepts_loop_exit_zero_seed() {
    let rbp = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x28, 8);
    let tmp = register(UNIQUE_SPACE_ID, 0x9300, 8);
    let read_op = op(
        20,
        PcodeOpcode::IntMult,
        Some(tmp),
        vec![rbp.clone(), constant(1)],
    );
    let pcode = pcode_function(vec![
        block_at(
            0x1000,
            0,
            vec![
                op(1, PcodeOpcode::Copy, Some(rbp.clone()), vec![constant(0)]),
                op(2, PcodeOpcode::Branch, None, vec![constant(0x1030)]),
            ],
        ),
        block_at(
            0x1010,
            1,
            vec![op(3, PcodeOpcode::Branch, None, vec![constant(0x1020)])],
        ),
        block_at(
            0x1020,
            2,
            vec![
                op(
                    4,
                    PcodeOpcode::IntAdd,
                    Some(rbp.clone()),
                    vec![rbp.clone(), constant(1)],
                ),
                op(5, PcodeOpcode::Branch, None, vec![constant(0x1030)]),
            ],
        ),
        block_at(
            0x1030,
            3,
            vec![
                read_op.clone(),
                op(21, PcodeOpcode::Branch, None, vec![constant(0x1040)]),
            ],
        ),
        block_at(
            0x1040,
            4,
            vec![op(22, PcodeOpcode::Return, None, vec![constant(0)])],
        ),
    ]);
    let options = crate::nir::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.ensure_live_register_binding("rbp", 8);
    builder.predecessors[3] = vec![0, 2];
    builder.loop_bodies = vec![crate::nir::structuring::loop_analysis::LoopBody {
        head: 1,
        tails: vec![2],
        body: vec![1, 2],
        exit_idx: Some(3),
        all_exits: vec![3],
    }];
    let stale_rhs = HirExpr::Binary {
        op: HirBinaryOp::Mul,
        lhs: Box::new(HirExpr::Var("xVar53".to_string())),
        rhs: Box::new(HirExpr::Const(1, int(64))),
        ty: int(64),
    };

    let rewritten = builder.with_lowering_site(
        LoweringSite {
            block_idx: 3,
            op_idx: 0,
        },
        |builder| {
            builder.rewrite_block_entry_accumulator_rhs_with_live_gpr(
                pcode.blocks[3].start_address,
                &read_op,
                stale_rhs,
            )
        },
    );

    assert_eq!(
        rewritten,
        HirExpr::Binary {
            op: HirBinaryOp::Mul,
            lhs: Box::new(HirExpr::Var("rbp".to_string())),
            rhs: Box::new(HirExpr::Const(1, int(64))),
            ty: int(64),
        }
    );
    assert!(builder.params.is_empty(), "must not promote rbp to a param");
}

#[test]
fn stack_home_accumulator_store_rejects_side_effect_after_live_def() {
    let ebp = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x14, 4);
    let rbp = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x28, 8);
    let rsp_addr = register(UNIQUE_SPACE_ID, 0x200, 8);
    let load_tmp = register(UNIQUE_SPACE_ID, 0x208, 8);
    let cond = register(UNIQUE_SPACE_ID, 0x300, 1);
    let mut store = op(
        2,
        PcodeOpcode::Store,
        None,
        vec![constant(0), rsp_addr.clone(), ebp.clone()],
    );
    store.asm_mnemonic = Some("MOV dword ptr [RSP+0x4c], EBP".to_string());
    let pcode = pcode_function(vec![
        block_at(
            0x1000,
            0,
            vec![
                op(1, PcodeOpcode::Copy, Some(ebp.clone()), vec![constant(0)]),
                op(10, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
            ],
        ),
        block_at(
            0x1010,
            1,
            vec![
                store.clone(),
                op(3, PcodeOpcode::CBranch, None, vec![constant(0x1030), cond]),
            ],
        ),
        block_at(
            0x1020,
            2,
            vec![
                op(
                    4,
                    PcodeOpcode::IntAdd,
                    Some(rbp.clone()),
                    vec![rbp.clone(), constant(1)],
                ),
                op(
                    5,
                    PcodeOpcode::Load,
                    Some(load_tmp),
                    vec![constant(0), rsp_addr],
                ),
                op(6, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
            ],
        ),
        block_at(
            0x1030,
            3,
            vec![op(7, PcodeOpcode::Return, None, vec![constant(0)])],
        ),
    ]);
    let options = crate::nir::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);

    assert!(
        builder
            .stack_home_accumulator_store_rhs(&pcode.blocks[1], 0, &store, "home_4c", &ebp)
            .is_none()
    );
}

#[test]
fn stack_home_accumulator_store_rejects_partial_register_value() {
    let bp = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x14, 2);
    let rsp_addr = register(UNIQUE_SPACE_ID, 0x200, 8);
    let cond = register(UNIQUE_SPACE_ID, 0x300, 1);
    let mut store = op(
        2,
        PcodeOpcode::Store,
        None,
        vec![constant(0), rsp_addr, bp.clone()],
    );
    store.asm_mnemonic = Some("MOV word ptr [RSP+0x4c], BP".to_string());
    let pcode = pcode_function(vec![
        block_at(
            0x1000,
            0,
            vec![
                op(1, PcodeOpcode::Copy, Some(bp.clone()), vec![constant(0)]),
                op(10, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
            ],
        ),
        block_at(
            0x1010,
            1,
            vec![
                store.clone(),
                op(3, PcodeOpcode::CBranch, None, vec![constant(0x1030), cond]),
            ],
        ),
        block_at(
            0x1020,
            2,
            vec![op(4, PcodeOpcode::Branch, None, vec![constant(0x1010)])],
        ),
        block_at(
            0x1030,
            3,
            vec![op(5, PcodeOpcode::Return, None, vec![constant(0)])],
        ),
    ]);
    let options = crate::nir::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);

    assert!(
        builder
            .stack_home_accumulator_store_rhs(&pcode.blocks[1], 0, &store, "home_4c", &bp)
            .is_none()
    );
}

#[test]
fn explicit_merge_select_materializes_store_value_diamond() {
    fn op_at(
        seq_num: u32,
        address: u64,
        opcode: PcodeOpcode,
        output: Option<Varnode>,
        inputs: Vec<Varnode>,
    ) -> PcodeOp {
        PcodeOp {
            seq_num,
            opcode,
            address,
            output,
            inputs,
            asm_mnemonic: None,
        }
    }

    let param = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4000, 4);
    let lhs = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4008, 4);
    let rhs = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4010, 4);
    let merge_value = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4028, 4);
    let ptr = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4030, 8);
    let first = PcodeBasicBlock {
        index: 0,
        start_address: 0x1000,
        successors: vec![2, 1],
        ops: vec![
            op_at(
                0,
                0x1000,
                PcodeOpcode::IntSub,
                Some(merge_value.clone()),
                vec![lhs.clone(), rhs.clone()],
            ),
            op_at(
                1,
                0x1004,
                PcodeOpcode::CBranch,
                None,
                vec![Varnode::constant(0x1020, 8), param],
            ),
        ],
    };
    let alternate = PcodeBasicBlock {
        index: 1,
        start_address: 0x1010,
        successors: vec![2],
        ops: vec![op_at(
            2,
            0x1010,
            PcodeOpcode::IntSub,
            Some(merge_value.clone()),
            vec![rhs, lhs],
        )],
    };
    let merge = PcodeBasicBlock {
        index: 2,
        start_address: 0x1020,
        successors: Vec::new(),
        ops: vec![op_at(
            3,
            0x1020,
            PcodeOpcode::Store,
            None,
            vec![Varnode::constant(3, 8), ptr, merge_value.clone()],
        )],
    };
    let pcode = pcode_function(vec![first.clone(), alternate.clone(), merge.clone()]);
    let options = crate::nir::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);

    let stmts = builder
        .synthesize_explicit_merge_bindings_for_block(&merge)
        .expect("synthesize merge binding");

    assert!(
        matches!(
            stmts.as_slice(),
            [HirStmt::Assign {
                rhs: HirExpr::Select { .. },
                ..
            }]
        ),
        "{stmts:?}"
    );
}

#[test]
fn missing_merge_aarch64_zero_extend_uses_low_live_register_binding_for_safe_rhs() {
    let x12 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4060, 8);
    let w12 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4060, 4);
    let w8 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4040, 4);
    let def_op = op(1, PcodeOpcode::IntZExt, Some(x12.clone()), vec![w8]);
    let mut def_block = block_at(0x1000, 0, vec![def_op.clone()]);
    def_block.successors = vec![1];
    let merge_block = block_at(
        0x2000,
        1,
        vec![op(
            2,
            PcodeOpcode::IntEqual,
            Some(register(UNIQUE_SPACE_ID, 0x100, 1)),
            vec![w12, constant(0)],
        )],
    );
    let pcode = pcode_function(vec![def_block.clone(), merge_block]);
    let mut options = crate::nir::builder::materialize::test_support::test_options();
    options.calling_convention = CallingConvention::AArch64;
    options.format = "ELF64".to_string();
    options.pe_x64_only = false;
    let builder = PreviewBuilder::new(&pcode, &options, None);
    let rhs = HirExpr::Cast {
        ty: int(64),
        expr: Box::new(HirExpr::Cast {
            ty: int(32),
            expr: Box::new(HirExpr::Var("xVar7".to_string())),
        }),
    };

    assert_eq!(
        builder.live_register_lhs_name_for_safe_missing_merge(
            &def_block,
            0,
            &def_op,
            &x12,
            &rhs,
            ReplacementValuePlan::incomplete(
                ReplacementReadClass::Merge,
                MaterializationRejectionReason::MissingMergeBinding,
            ),
        ),
        Some(("w12".to_string(), 4))
    );
}

#[test]
fn missing_join_store_value_uses_low_live_register_binding_for_safe_rhs() {
    let x0 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4000, 8);
    let w0 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4000, 4);
    let ptr = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4040, 8);
    let value = register(UNIQUE_SPACE_ID, 0x100, 4);
    let def_op = op(1, PcodeOpcode::IntZExt, Some(x0.clone()), vec![value]);
    let def_block = block_at(
        0x1000,
        0,
        vec![
            def_op.clone(),
            op(
                4,
                PcodeOpcode::CBranch,
                None,
                vec![constant(0x2000), register(UNIQUE_SPACE_ID, 0x200, 1)],
            ),
        ],
    );
    let other_pred = block_at(
        0x1800,
        1,
        vec![op(3, PcodeOpcode::Branch, None, vec![constant(0x2000)])],
    );
    let merge_block = block_at(
        0x2000,
        2,
        vec![op(2, PcodeOpcode::Store, None, vec![constant(0), ptr, w0])],
    );
    let pcode = pcode_function(vec![def_block.clone(), other_pred, merge_block]);
    let mut options = crate::nir::builder::materialize::test_support::test_options();
    options.calling_convention = CallingConvention::AArch64;
    options.format = "ELF64".to_string();
    options.pe_x64_only = false;
    let builder = PreviewBuilder::new(&pcode, &options, None);
    let rhs = HirExpr::Cast {
        ty: int(64),
        expr: Box::new(HirExpr::Var("uVar1".to_string())),
    };

    assert_eq!(
        builder.live_register_lhs_name_for_safe_missing_merge(
            &def_block,
            0,
            &def_op,
            &x0,
            &rhs,
            ReplacementValuePlan::incomplete(
                ReplacementReadClass::Merge,
                MaterializationRejectionReason::MissingMergeBinding,
            ),
        ),
        Some(("w0".to_string(), 4))
    );
}

#[test]
fn passthrough_join_store_producer_uses_low_live_register_binding() {
    let x0 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4000, 8);
    let w0 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4000, 4);
    let ptr = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4040, 8);
    let add_out = register(UNIQUE_SPACE_ID, 0x100, 4);
    let add_op = op(
        1,
        PcodeOpcode::IntAdd,
        Some(add_out.clone()),
        vec![w0.clone(), constant(1)],
    );
    let zext_op = op(
        2,
        PcodeOpcode::IntZExt,
        Some(x0.clone()),
        vec![add_out.clone()],
    );
    let def_block = block_at(
        0x1000,
        0,
        vec![
            add_op,
            zext_op,
            op(
                4,
                PcodeOpcode::CBranch,
                None,
                vec![constant(0x2000), register(UNIQUE_SPACE_ID, 0x200, 1)],
            ),
        ],
    );
    let other_pred = block_at(
        0x1800,
        1,
        vec![op(3, PcodeOpcode::Branch, None, vec![constant(0x2000)])],
    );
    let merge_block = block_at(
        0x2000,
        2,
        vec![op(5, PcodeOpcode::Store, None, vec![constant(0), ptr, w0])],
    );
    let pcode = pcode_function(vec![def_block.clone(), other_pred, merge_block]);
    let mut options = crate::nir::builder::materialize::test_support::test_options();
    options.calling_convention = CallingConvention::AArch64;
    options.format = "ELF64".to_string();
    options.pe_x64_only = false;
    let builder = PreviewBuilder::new(&pcode, &options, None);
    let rhs = HirExpr::Binary {
        op: HirBinaryOp::Add,
        lhs: Box::new(HirExpr::Var("w0".to_string())),
        rhs: Box::new(HirExpr::Const(1, int(32))),
        ty: int(32),
    };

    assert_eq!(
        builder.live_register_lhs_name_for_passthrough_join_store_producer(
            &def_block, 0, &add_out, &rhs,
        ),
        Some(("w0".to_string(), 4))
    );
}

#[test]
fn loop_header_missing_merge_uses_x64_live_register_binding() {
    let r14d = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0xb0, 4);
    let r15d = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0xb8, 4);
    let store_ptr = register(UNIQUE_SPACE_ID, 0x100, 8);
    let cond = register(UNIQUE_SPACE_ID, 0x108, 1);
    let def_op = op(1, PcodeOpcode::Copy, Some(r15d.clone()), vec![r14d.clone()]);
    let mut entry = block_at(
        0x1000,
        0,
        vec![op(0, PcodeOpcode::Branch, None, vec![constant(0x1010)])],
    );
    entry.successors = vec![1];
    let mut header = block_at(
        0x1010,
        1,
        vec![
            op(
                2,
                PcodeOpcode::Store,
                None,
                vec![constant(0), store_ptr, r15d.clone()],
            ),
            op(3, PcodeOpcode::CBranch, None, vec![constant(0x1030), cond]),
        ],
    );
    header.successors = vec![3, 2];
    let mut body = block_at(
        0x1020,
        2,
        vec![
            def_op.clone(),
            op(5, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
        ],
    );
    body.successors = vec![1];
    let exit = block_at(
        0x1030,
        3,
        vec![op(
            4,
            PcodeOpcode::Return,
            None,
            vec![constant(0), r15d.clone()],
        )],
    );
    let pcode = pcode_function(vec![entry, header, body.clone(), exit]);
    let mut options = crate::nir::builder::materialize::test_support::test_options();
    options.calling_convention = CallingConvention::WindowsX64;
    let builder = PreviewBuilder::new(&pcode, &options, None);
    let rhs = HirExpr::Var("r14".to_string());
    let proof = builder
        .describe_missing_merge_binding_proof(&body, 0, &r15d, &rhs)
        .expect("missing merge proof");
    assert_eq!(
        proof.relation,
        MissingMergeBindingRelation::LoopHeaderMergeMissing
    );
    assert_eq!(
        proof.consumer_kind,
        DisallowedSingleConsumerConsumerKind::StoreValue
    );
    assert_eq!(
        crate::nir::cspec::RegisterNamer::from_options(&options).hw_name_at(r15d.offset, r15d.size),
        Some("r15".to_string())
    );

    assert_eq!(
        builder.live_register_lhs_name_for_safe_missing_merge(
            &body,
            0,
            &def_op,
            &r15d,
            &rhs,
            ReplacementValuePlan::incomplete(
                ReplacementReadClass::Merge,
                MaterializationRejectionReason::MissingMergeBinding,
            ),
        ),
        Some(("r15".to_string(), 4))
    );
}

#[test]
fn loop_header_missing_merge_rejects_side_effect_rhs() {
    let r14d = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0xb0, 4);
    let r15d = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0xb8, 4);
    let store_ptr = register(UNIQUE_SPACE_ID, 0x100, 8);
    let cond = register(UNIQUE_SPACE_ID, 0x108, 1);
    let def_op = op(1, PcodeOpcode::Copy, Some(r15d.clone()), vec![r14d]);
    let mut entry = block_at(
        0x1000,
        0,
        vec![op(0, PcodeOpcode::Branch, None, vec![constant(0x1010)])],
    );
    entry.successors = vec![1];
    let mut header = block_at(
        0x1010,
        1,
        vec![
            op(
                2,
                PcodeOpcode::Store,
                None,
                vec![constant(0), store_ptr, r15d.clone()],
            ),
            op(3, PcodeOpcode::CBranch, None, vec![constant(0x1030), cond]),
        ],
    );
    header.successors = vec![3, 2];
    let mut body = block_at(
        0x1020,
        2,
        vec![
            def_op.clone(),
            op(5, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
        ],
    );
    body.successors = vec![1];
    let exit = block_at(
        0x1030,
        3,
        vec![op(
            4,
            PcodeOpcode::Return,
            None,
            vec![constant(0), r15d.clone()],
        )],
    );
    let pcode = pcode_function(vec![entry, header, body.clone(), exit]);
    let options = crate::nir::builder::materialize::test_support::test_options();
    let builder = PreviewBuilder::new(&pcode, &options, None);
    let rhs = HirExpr::Call {
        target: "may_call".to_string(),
        args: vec![HirExpr::Var("r14".to_string())],
        ty: int(32),
    };

    assert_eq!(
        builder.live_register_lhs_name_for_safe_missing_merge(
            &body,
            0,
            &def_op,
            &r15d,
            &rhs,
            ReplacementValuePlan::incomplete(
                ReplacementReadClass::Merge,
                MaterializationRejectionReason::MissingMergeBinding,
            ),
        ),
        None
    );
}

#[test]
fn missing_merge_live_register_binding_rejects_call_or_aggregate_rhs() {
    let x8 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4040, 8);
    let w8 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4040, 4);
    let input = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x5020, 16);
    let def_op = op(1, PcodeOpcode::IntZExt, Some(x8.clone()), vec![input]);
    let mut def_block = block_at(0x1000, 0, vec![def_op.clone()]);
    def_block.successors = vec![1];
    let merge_block = block_at(
        0x2000,
        1,
        vec![op(
            2,
            PcodeOpcode::IntEqual,
            Some(register(UNIQUE_SPACE_ID, 0x100, 1)),
            vec![w8, constant(0)],
        )],
    );
    let pcode = pcode_function(vec![def_block.clone(), merge_block]);
    let mut options = crate::nir::builder::materialize::test_support::test_options();
    options.calling_convention = CallingConvention::AArch64;
    options.format = "ELF64".to_string();
    options.pe_x64_only = false;
    let builder = PreviewBuilder::new(&pcode, &options, None);
    let rhs = HirExpr::Binary {
        op: HirBinaryOp::Add,
        lhs: Box::new(HirExpr::Call {
            target: "__pcodeop_294".to_string(),
            args: vec![HirExpr::Var("reg".to_string())],
            ty: NirType::Aggregate {
                size: 16,
                fields: Vec::new(),
            },
        }),
        rhs: Box::new(HirExpr::Const(4, int(32))),
        ty: int(32),
    };

    assert_eq!(
        builder.live_register_lhs_name_for_safe_missing_merge(
            &def_block,
            0,
            &def_op,
            &x8,
            &rhs,
            ReplacementValuePlan::incomplete(
                ReplacementReadClass::Merge,
                MaterializationRejectionReason::MissingMergeBinding,
            ),
        ),
        None
    );
}

#[test]
fn call_result_observation_stops_at_partial_return_register_clobber() {
    let ret_eax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 4);
    let out = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x100, 4);
    let block = block(vec![
        op(1, PcodeOpcode::Call, None, vec![constant(0x2000)]),
        op(
            2,
            PcodeOpcode::Copy,
            Some(ret_eax.clone()),
            vec![constant(1)],
        ),
        op(
            3,
            PcodeOpcode::IntAdd,
            Some(out),
            vec![ret_eax, constant(2)],
        ),
    ]);
    let pcode = pcode_function(vec![block.clone()]);
    let options = crate::nir::builder::materialize::test_support::test_options();
    let builder = PreviewBuilder::new(&pcode, &options, None);

    assert!(!builder.call_result_is_observed(&block, 0));
}

#[test]
fn partial_return_register_reads_resolve_to_live_call_result_binding() {
    let ret_eax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 4);
    let ebx = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x0c, 4);
    let out = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x100, 4);
    let block = block(vec![
        op(1, PcodeOpcode::Call, None, vec![constant(0x2000)]),
        op(
            2,
            PcodeOpcode::IntAdd,
            Some(out),
            vec![ebx, ret_eax.clone()],
        ),
    ]);
    let pcode = pcode_function(vec![block]);
    let options = crate::nir::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.call_result_bindings.insert(
        LoweringSite {
            block_idx: 0,
            op_idx: 0,
        },
        "xVarCall".to_string(),
    );
    builder.current_lowering_site = Some(LoweringSite {
        block_idx: 0,
        op_idx: 1,
    });

    assert_eq!(
        builder.live_call_result_binding_for_return_register(&ret_eax),
        Some("xVarCall".to_string())
    );
}

#[test]
fn cross_block_return_register_reads_resolve_to_live_call_result_binding() {
    let ret_eax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 4);
    let ebx = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x0c, 4);
    let out = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x100, 4);
    let mut call_block = block_at(
        0x1000,
        0,
        vec![op(1, PcodeOpcode::Call, None, vec![constant(0x2000)])],
    );
    call_block.successors = vec![1];
    let use_block = block_at(
        0x1010,
        1,
        vec![op(
            2,
            PcodeOpcode::IntAdd,
            Some(out),
            vec![ebx, ret_eax.clone()],
        )],
    );
    let pcode = pcode_function(vec![call_block, use_block]);
    let options = crate::nir::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.call_result_bindings.insert(
        LoweringSite {
            block_idx: 0,
            op_idx: 0,
        },
        "xVarCall".to_string(),
    );
    builder.current_lowering_site = Some(LoweringSite {
        block_idx: 1,
        op_idx: 0,
    });

    assert_eq!(
        builder.live_call_result_binding_for_return_register(&ret_eax),
        Some("xVarCall".to_string())
    );
}

#[test]
fn cross_block_return_register_binding_stops_at_redefinition() {
    let ret_eax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 4);
    let ebx = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x0c, 4);
    let out = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x100, 4);
    let mut call_block = block_at(
        0x1000,
        0,
        vec![
            op(1, PcodeOpcode::Call, None, vec![constant(0x2000)]),
            op(
                2,
                PcodeOpcode::IntAdd,
                Some(ret_eax.clone()),
                vec![ret_eax.clone(), constant(1)],
            ),
        ],
    );
    call_block.successors = vec![1];
    let use_block = block_at(
        0x1010,
        1,
        vec![op(
            3,
            PcodeOpcode::IntAdd,
            Some(out),
            vec![ebx, ret_eax.clone()],
        )],
    );
    let pcode = pcode_function(vec![call_block, use_block]);
    let options = crate::nir::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.call_result_bindings.insert(
        LoweringSite {
            block_idx: 0,
            op_idx: 0,
        },
        "xVarCall".to_string(),
    );
    builder.current_lowering_site = Some(LoweringSite {
        block_idx: 1,
        op_idx: 0,
    });

    assert_eq!(
        builder.live_call_result_binding_for_return_register(&ret_eax),
        None
    );
}

#[test]
fn same_instruction_callother_does_not_steal_arm_call_args_or_result() {
    fn op_at(
        seq_num: u32,
        address: u64,
        opcode: PcodeOpcode,
        output: Option<Varnode>,
        inputs: Vec<Varnode>,
    ) -> PcodeOp {
        PcodeOp {
            seq_num,
            opcode,
            address,
            output,
            inputs,
            asm_mnemonic: None,
        }
    }

    let r0 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 32, 4);
    let r1 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 36, 4);
    let out = register(RUST_SLEIGH_UNIQUE_SPACE_ID, 0x4000, 4);
    let block = block_at(
        0x1000,
        0,
        vec![
            op_at(
                0,
                0x1000,
                PcodeOpcode::Copy,
                Some(r0.clone()),
                vec![Varnode::constant(7, 4)],
            ),
            op_at(
                1,
                0x1002,
                PcodeOpcode::CallOther,
                None,
                vec![Varnode::constant(62, 4)],
            ),
            op_at(
                2,
                0x1002,
                PcodeOpcode::Call,
                None,
                vec![Varnode::constant(0x2000, 4)],
            ),
            op_at(3, 0x1004, PcodeOpcode::IntAdd, Some(out), vec![r1, r0]),
        ],
    );
    let pcode = pcode_function(vec![block.clone()]);
    let mut options = crate::nir::builder::materialize::test_support::test_options();
    options.is_64bit = false;
    options.pointer_size = 4;
    options.calling_convention = CallingConvention::Arm32;
    crate::nir::cspec::test_maps::apply_preview_cspec(&mut options);
    let mut builder = PreviewBuilder::new(&pcode, &options, None);

    let stmts = builder
        .lower_block_stmts(&block)
        .expect("lower ARM call block");

    assert!(
        matches!(
            &stmts[0],
            HirStmt::Assign {
                rhs: HirExpr::Call { args, .. },
                ..
            } if matches!(args.as_slice(), [HirExpr::Const(7, _)])
        ),
        "{stmts:?}"
    );
}

#[test]
fn lower_block_stmts_uses_block_index_for_duplicate_start_addresses() {
    let x0 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 8);
    let w0 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 4);
    let ptr = Varnode::constant(0x3000, 8);
    let first_duplicate = block_at(
        0x2000,
        1,
        vec![op(
            1,
            PcodeOpcode::Copy,
            Some(x0.clone()),
            vec![constant(3)],
        )],
    );
    let second_duplicate = block_at(
        0x2000,
        2,
        vec![
            op(2, PcodeOpcode::Copy, Some(x0), vec![constant(7)]),
            op(3, PcodeOpcode::Store, None, vec![constant(3), ptr, w0]),
        ],
    );
    let pcode = pcode_function(vec![
        block_at(0x1000, 0, Vec::new()),
        first_duplicate,
        second_duplicate.clone(),
    ]);
    let options = crate::nir::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);

    let stmts = builder
        .lower_block_stmts(&second_duplicate)
        .expect("lower duplicate block");

    assert!(
        matches!(
            stmts.as_slice(),
            [HirStmt::Assign {
                rhs: HirExpr::Cast {
                    expr,
                    ..
                },
            ..
        }] if matches!(expr.as_ref(), HirExpr::Const(7, _))
        ),
        "{stmts:?}"
    );
}

#[test]
fn lookup_def_site_allows_unique_low_view_of_wide_temp() {
    let wide = Varnode {
        space_id: RUST_SLEIGH_UNIQUE_SPACE_ID,
        offset: 0x40b00,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let low = Varnode {
        size: 4,
        ..wide.clone()
    };
    let x8 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x40, 8);
    let pcode = pcode_function(vec![block_at(
        0x1000,
        0,
        vec![
            op(0, PcodeOpcode::Copy, Some(wide), vec![constant(7)]),
            op(1, PcodeOpcode::IntZExt, Some(x8), vec![low.clone()]),
        ],
    )]);
    let options = crate::nir::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.current_lowering_site = Some(LoweringSite {
        block_idx: 0,
        op_idx: 1,
    });

    let (site, producer) = builder
        .lookup_def_site(&low)
        .expect("wide unique def covers low view");

    assert_eq!(site.block_idx, 0);
    assert_eq!(site.op_idx, 0);
    assert_eq!(producer.seq_num, 0);
}

#[test]
fn duplicate_start_join_uses_shared_merge_binding_for_conflicting_defs() {
    fn op_at(
        seq_num: u32,
        address: u64,
        opcode: PcodeOpcode,
        output: Option<Varnode>,
        inputs: Vec<Varnode>,
    ) -> PcodeOp {
        PcodeOp {
            seq_num,
            opcode,
            address,
            output,
            inputs,
            asm_mnemonic: None,
        }
    }

    let merge = register(RUST_SLEIGH_UNIQUE_SPACE_ID, 0x82b00, 4);
    let param = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4000, 4);
    let denom = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4040, 4);
    let cond = register(RUST_SLEIGH_UNIQUE_SPACE_ID, 0x82c00, 1);
    let w0 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4000, 4);
    let x30 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x40f0, 8);
    let ret_target = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 8);
    let pcode = pcode_function(vec![
        PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![2, 1],
            ops: vec![
                op_at(
                    0,
                    0x1000,
                    PcodeOpcode::Copy,
                    Some(merge.clone()),
                    vec![Varnode::constant(0, 4)],
                ),
                op_at(
                    1,
                    0x1000,
                    PcodeOpcode::CBranch,
                    None,
                    vec![Varnode::constant(2, 8), cond],
                ),
            ],
        },
        PcodeBasicBlock {
            index: 1,
            start_address: 0x1010,
            successors: vec![2],
            ops: vec![op_at(
                2,
                0x1010,
                PcodeOpcode::IntDiv,
                Some(merge.clone()),
                vec![param, denom],
            )],
        },
        PcodeBasicBlock {
            index: 2,
            start_address: 0x1010,
            successors: Vec::new(),
            ops: vec![
                op_at(
                    3,
                    0x1010,
                    PcodeOpcode::IntAdd,
                    Some(w0),
                    vec![merge, Varnode::constant(5, 4)],
                ),
                op_at(
                    4,
                    0x1014,
                    PcodeOpcode::Copy,
                    Some(ret_target),
                    vec![x30.clone()],
                ),
                op_at(5, 0x1014, PcodeOpcode::Return, None, vec![x30]),
            ],
        },
    ]);
    let mut options = crate::nir::builder::materialize::test_support::test_options();
    options.calling_convention = CallingConvention::AArch64;
    options.format = "ELF64".to_string();
    options.pe_x64_only = false;
    crate::nir::cspec::test_maps::sync_preview_cspec(&mut options);

    let code =
        render_mlil_preview(&pcode, "duplicate_merge", 0x1000, &options).expect("render");
    assert!(code.contains("if ("), "{code}");
    assert!(code.contains(" / "), "{code}");
    assert!(code.contains(" + 5"), "{code}");
}

#[test]
fn duplicate_start_join_preserves_register_addend_after_zero_extend() {
    fn op_at(
        seq_num: u32,
        address: u64,
        opcode: PcodeOpcode,
        output: Option<Varnode>,
        inputs: Vec<Varnode>,
    ) -> PcodeOp {
        PcodeOp {
            seq_num,
            opcode,
            address,
            output,
            inputs,
            asm_mnemonic: None,
        }
    }

    let merge = register(RUST_SLEIGH_UNIQUE_SPACE_ID, 0x82b00, 4);
    let cond = register(RUST_SLEIGH_UNIQUE_SPACE_ID, 0x82c00, 1);
    let dividend = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4048, 4);
    let denom = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4040, 4);
    let param = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4000, 4);
    let factor = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4050, 4);
    let w8 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4040, 4);
    let x8 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4040, 8);
    let product = register(RUST_SLEIGH_UNIQUE_SPACE_ID, 0x51200, 4);
    let madd_sum = register(RUST_SLEIGH_UNIQUE_SPACE_ID, 0x51400, 4);
    let ret = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4000, 4);
    let x30 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x40f0, 8);
    let ret_target = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 8);
    let pcode = pcode_function(vec![
        PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![2, 1],
            ops: vec![
                op_at(
                    0,
                    0x1000,
                    PcodeOpcode::Copy,
                    Some(merge.clone()),
                    vec![Varnode::constant(0, 4)],
                ),
                op_at(
                    2,
                    0x1010,
                    PcodeOpcode::CBranch,
                    None,
                    vec![Varnode::constant(2, 8), cond],
                ),
            ],
        },
        PcodeBasicBlock {
            index: 1,
            start_address: 0x1010,
            successors: vec![2],
            ops: vec![op_at(
                3,
                0x1010,
                PcodeOpcode::IntDiv,
                Some(merge.clone()),
                vec![dividend, denom],
            )],
        },
        PcodeBasicBlock {
            index: 2,
            start_address: 0x1010,
            successors: Vec::new(),
            ops: vec![
                op_at(
                    4,
                    0x1010,
                    PcodeOpcode::IntZExt,
                    Some(x8.clone()),
                    vec![merge],
                ),
                op_at(
                    5,
                    0x1014,
                    PcodeOpcode::IntMult,
                    Some(product.clone()),
                    vec![param.clone(), factor],
                ),
                op_at(
                    6,
                    0x1014,
                    PcodeOpcode::IntAdd,
                    Some(madd_sum.clone()),
                    vec![w8, product],
                ),
                op_at(7, 0x1014, PcodeOpcode::IntZExt, Some(x8), vec![madd_sum]),
                op_at(
                    8,
                    0x1018,
                    PcodeOpcode::IntXor,
                    Some(ret),
                    vec![param, register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4040, 4)],
                ),
                op_at(
                    9,
                    0x101c,
                    PcodeOpcode::Copy,
                    Some(ret_target),
                    vec![x30.clone()],
                ),
                op_at(10, 0x101c, PcodeOpcode::Return, None, vec![x30]),
            ],
        },
    ]);
    let mut options = crate::nir::builder::materialize::test_support::test_options();
    options.calling_convention = CallingConvention::AArch64;
    options.format = "ELF64".to_string();
    options.pe_x64_only = false;

    let code = render_mlil_preview(&pcode, "madd_addend", 0x1000, &options).expect("render");
    assert!(code.contains(" * "), "{code}");
    assert!(code.contains(" + "), "{code}");
    assert!(code.contains(" / "), "{code}");
    assert!(!code.contains("{\n    }"), "{code}");
}
