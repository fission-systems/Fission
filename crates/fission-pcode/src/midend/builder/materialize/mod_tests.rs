use super::*;
use crate::PcodeBasicBlock;
use crate::midend::builder::materialize::test_support::{
    block, block_at, constant, int, op, pcode_function,
};
use crate::midend::render_mlil_preview;

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
fn escaped_fixed_frame_accesses_share_one_byte_backing_object() {
    let rsp = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x20, 8);
    let index = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x00, 8);
    let loaded = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x08, 8);
    let frame_pointer = crate::midend::builder::materialize::test_support::varnode(0x100);
    let indexed_pointer = crate::midend::builder::materialize::test_support::varnode(0x108);
    let pcode = pcode_function(vec![block(vec![
        op(
            0,
            PcodeOpcode::IntSub,
            Some(rsp.clone()),
            vec![rsp.clone(), constant(0x100)],
        ),
        op(
            1,
            PcodeOpcode::IntAdd,
            Some(frame_pointer.clone()),
            vec![rsp.clone(), constant(0x20)],
        ),
        op(
            2,
            PcodeOpcode::PtrAdd,
            Some(indexed_pointer.clone()),
            vec![frame_pointer.clone(), index, constant(8)],
        ),
        op(
            3,
            PcodeOpcode::Load,
            Some(loaded.clone()),
            vec![constant(0), indexed_pointer.clone()],
        ),
        op(
            4,
            PcodeOpcode::Load,
            Some(register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x10, 8)),
            vec![constant(0), frame_pointer.clone()],
        ),
        op(
            5,
            PcodeOpcode::Store,
            None,
            vec![constant(0), frame_pointer.clone(), loaded],
        ),
    ])]);
    let mut options = crate::midend::builder::materialize::test_support::test_options();
    options.calling_convention = CallingConvention::WindowsX64;
    let mut builder = PreviewBuilder::new(&pcode, &options, None);

    builder
        .run_incremental_heritage()
        .expect("fixed stack frame should be classified");

    let backing = builder
        .stack_frame_backing
        .as_ref()
        .expect("escaped stack pointer should create frame backing");
    assert_eq!(backing.size, 0x100);
    assert_eq!(builder.locals.len(), 1, "overlapping slots share the frame");

    let mut visiting = HashSet::default();
    let dynamic_address = builder
        .with_lowering_site(
            LoweringSite {
                block_idx: 0,
                op_idx: 3,
            },
            |builder| builder.lower_memory_pointer(&indexed_pointer, &mut visiting),
        )
        .expect("dynamic stack pointer should lower");
    let dynamic_dump = format!("{dynamic_address:?}");
    assert!(dynamic_dump.contains("AddressOfLocal(\"stack_frame\")"));

    let direct_address = builder
        .with_lowering_site(
            LoweringSite {
                block_idx: 0,
                op_idx: 4,
            },
            |builder| builder.lower_memory_pointer(&frame_pointer, &mut HashSet::default()),
        )
        .expect("direct stack pointer should lower");
    assert!(matches!(
        direct_address,
        PreHirExpr::PtrOffset { base, offset: 0x20 }
            if matches!(base.as_ref(), PreHirExpr::AddressOfLocal(name) if name == "stack_frame")
    ));
}

#[test]
fn uncovered_call_only_frame_address_uses_backing_object() {
    let rsp = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x20, 8);
    let buffer = crate::midend::builder::materialize::test_support::varnode(0x100);
    let pcode = pcode_function(vec![block(vec![
        op(
            0,
            PcodeOpcode::IntSub,
            Some(rsp.clone()),
            vec![rsp.clone(), constant(0x100)],
        ),
        op(
            1,
            PcodeOpcode::IntAdd,
            Some(buffer.clone()),
            vec![rsp, constant(0x20)],
        ),
        op(2, PcodeOpcode::Call, None, vec![constant(0x2000), buffer]),
    ])]);
    let mut options = crate::midend::builder::materialize::test_support::test_options();
    options.calling_convention = CallingConvention::SystemVAmd64;
    let mut builder = PreviewBuilder::new(&pcode, &options, None);

    builder
        .run_incremental_heritage()
        .expect("uncovered address-only stack object should be classified");

    let backing = builder
        .stack_frame_backing
        .as_ref()
        .expect("call-only frame address should use the opaque frame object");
    assert_eq!(backing.size, 0x100);
}

#[test]
fn uncovered_frame_address_copied_to_abi_argument_register_uses_backing_object() {
    let rsp = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x20, 8);
    let rcx = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x08, 8);
    let buffer = crate::midend::builder::materialize::test_support::varnode(0x100);
    let pcode = pcode_function(vec![block(vec![
        op(
            0,
            PcodeOpcode::IntSub,
            Some(rsp.clone()),
            vec![rsp.clone(), constant(0x100)],
        ),
        op(
            1,
            PcodeOpcode::IntAdd,
            Some(buffer.clone()),
            vec![rsp, constant(0x20)],
        ),
        op(2, PcodeOpcode::Copy, Some(rcx), vec![buffer]),
        op(
            3,
            PcodeOpcode::Call,
            None,
            vec![crate::midend::builder::materialize::test_support::varnode(
                0x2000,
            )],
        ),
    ])]);
    let mut options = crate::midend::builder::materialize::test_support::test_options();
    options.calling_convention = CallingConvention::WindowsX64;
    let mut builder = PreviewBuilder::new(&pcode, &options, None);

    builder
        .run_incremental_heritage()
        .expect("uncovered ABI argument stack object should be classified");

    let backing = builder
        .stack_frame_backing
        .as_ref()
        .expect("ABI register argument should preserve the address-only frame object");
    assert_eq!(backing.size, 0x100);
}

#[test]
fn frame_pointer_setup_is_not_an_escaped_local_address() {
    let rsp = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x20, 8);
    let rbp = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x28, 8);
    let rcx = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x08, 8);
    let saved_rbp = crate::midend::builder::materialize::test_support::varnode(0x118);
    let frame_base = crate::midend::builder::materialize::test_support::varnode(0x100);
    let local_address = crate::midend::builder::materialize::test_support::varnode(0x108);
    let loaded = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x00, 8);
    let mut save_rbp_stack_adjustment = op(
        1,
        PcodeOpcode::IntSub,
        Some(rsp.clone()),
        vec![rsp.clone(), constant(8)],
    );
    let mut save_rbp = op(
        2,
        PcodeOpcode::Store,
        None,
        vec![constant(3), rsp.clone(), saved_rbp.clone()],
    );
    save_rbp_stack_adjustment.address = 0x1001;
    save_rbp.address = 0x1001;
    let pcode = pcode_function(vec![block(vec![
        op(0, PcodeOpcode::Copy, Some(saved_rbp), vec![rbp.clone()]),
        save_rbp_stack_adjustment,
        save_rbp,
        op(
            3,
            PcodeOpcode::IntSub,
            Some(rsp.clone()),
            vec![rsp.clone(), constant(0x80)],
        ),
        op(
            4,
            PcodeOpcode::IntAdd,
            Some(frame_base.clone()),
            vec![rsp, constant(0x60)],
        ),
        op(5, PcodeOpcode::Copy, Some(rbp.clone()), vec![frame_base]),
        op(
            6,
            PcodeOpcode::IntSub,
            Some(local_address.clone()),
            vec![rbp, constant(0x20)],
        ),
        op(7, PcodeOpcode::Copy, Some(rcx), vec![local_address.clone()]),
        op(
            8,
            PcodeOpcode::Load,
            Some(loaded),
            vec![constant(0), local_address],
        ),
    ])]);
    let mut options = crate::midend::builder::materialize::test_support::test_options();
    options.calling_convention = CallingConvention::WindowsX64;
    let mut builder = PreviewBuilder::new(&pcode, &options, None);

    builder
        .run_incremental_heritage()
        .expect("frame-relative local should be classified");

    assert!(
        builder.stack_frame_backing.is_none(),
        "frame-base setup must not turn covered locals into an opaque whole-frame object"
    );
}

#[test]
fn fixed_frame_call_arguments_follow_reused_unique_def_sites() {
    let rsp = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x20, 8);
    let rax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x00, 8);
    let rdx = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x10, 8);
    let rsi = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x08, 8);
    let r8 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x80, 8);
    let reused_temporary = crate::midend::builder::materialize::test_support::varnode(0x9d00);
    let call_address = 0x1030;

    let mut frame_sub = op(
        0,
        PcodeOpcode::IntSub,
        Some(rsp.clone()),
        vec![rsp.clone(), constant(0x100)],
    );
    frame_sub.address = 0x1000;
    let mut timeout_address = op(
        1,
        PcodeOpcode::IntAdd,
        Some(reused_temporary.clone()),
        vec![rsp.clone(), constant(0x20)],
    );
    timeout_address.address = 0x1010;
    let mut copy_to_rdx = op(
        2,
        PcodeOpcode::Copy,
        Some(rdx.clone()),
        vec![reused_temporary.clone()],
    );
    copy_to_rdx.address = 0x1011;
    let mut copy_to_r8 = op(3, PcodeOpcode::Copy, Some(r8.clone()), vec![rdx]);
    copy_to_r8.address = 0x1012;
    let mut fd_set_address = op(
        4,
        PcodeOpcode::IntAdd,
        Some(reused_temporary.clone()),
        vec![rsp.clone(), constant(0x30)],
    );
    fd_set_address.address = 0x1020;
    let mut copy_to_rax = op(
        5,
        PcodeOpcode::Copy,
        Some(rax.clone()),
        vec![reused_temporary],
    );
    copy_to_rax.address = 0x1021;
    let mut copy_to_rsi = op(6, PcodeOpcode::Copy, Some(rsi.clone()), vec![rax]);
    copy_to_rsi.address = 0x1022;
    let mut call_sub = op(
        7,
        PcodeOpcode::IntSub,
        Some(rsp.clone()),
        vec![rsp.clone(), constant(8)],
    );
    call_sub.address = call_address;
    let mut return_address_store = op(
        8,
        PcodeOpcode::Store,
        None,
        vec![
            constant(3),
            rsp.clone(),
            constant((call_address + 5) as i64),
        ],
    );
    return_address_store.address = call_address;
    let mut call = op(
        9,
        PcodeOpcode::Call,
        None,
        vec![crate::midend::builder::materialize::test_support::varnode(
            0x2000,
        )],
    );
    call.address = call_address;

    let pcode = pcode_function(vec![block(vec![
        frame_sub,
        timeout_address,
        copy_to_rdx,
        copy_to_r8,
        fd_set_address,
        copy_to_rax,
        copy_to_rsi,
        call_sub,
        return_address_store,
        call,
    ])]);
    let mut options = crate::midend::builder::materialize::test_support::test_options();
    options.calling_convention = CallingConvention::SystemVAmd64;
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.stack_frame_backing = Some(crate::midend::builder::state::StackFrameBacking {
        name: "stack_frame".to_string(),
        size: 0x100,
    });

    let call_site = LoweringSite {
        block_idx: 0,
        op_idx: 9,
    };
    assert_eq!(
        builder.with_lowering_site(call_site, |builder| builder.resolve_stack_address(&r8)),
        Some((StackBase::Rsp, 0x20)),
        "R8 retains the earlier rsp + 0x20 definition despite reuse of the unique varnode"
    );
    assert_eq!(
        builder.with_lowering_site(call_site, |builder| builder.resolve_stack_address(&rsi)),
        Some((StackBase::Rsp, 0x30)),
        "RSI resolves to the later rsp + 0x30 definition"
    );
}

#[test]
fn defined_variadic_function_names_only_its_fixed_register_parameters() {
    let rcx = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x08, 8);
    let rdx = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x10, 8);
    let pcode = pcode_function(vec![block(Vec::new())]);
    let mut options = crate::midend::builder::materialize::test_support::test_options();
    options.calling_convention = CallingConvention::WindowsX64;

    let mut type_context = crate::midend::PreviewTypeContext::default();
    type_context.function_hints = Some(crate::midend::NirFunctionHints {
        variadic_fixed_arity: Some(1),
        ..Default::default()
    });
    let mut variadic = PreviewBuilder::new(&pcode, &options, Some(&type_context));
    variadic.entry_arity = 4;

    assert_eq!(variadic.named_entry_param_arity(), 1);
    assert_eq!(variadic.register_param(&rcx).as_deref(), Some("param_1"));
    assert_eq!(variadic.register_param(&rdx), None);
    let mut visiting = HashSet::default();
    let unnamed_input = variadic
        .lower_varnode(&rdx, &mut visiting)
        .expect("unnamed entry register should lower as a live register binding");
    assert!(matches!(unnamed_input, PreHirExpr::Var(ref name) if name == "rdx"));
    assert!(variadic.temps.contains_key("rdx"));
    assert!(!variadic.params.contains_key(&1));

    let mut fixed = PreviewBuilder::new(&pcode, &options, None);
    fixed.entry_arity = 4;
    assert_eq!(fixed.named_entry_param_arity(), 4);
    assert_eq!(fixed.register_param(&rdx).as_deref(), Some("param_2"));
}

#[test]
fn materialized_mapped_ram_output_keeps_global_lvalue_provenance() {
    let runtime_marker = register(RUST_SLEIGH_UNIQUE_SPACE_ID, 0x80, 4);
    let mapped_ram = register(UNIQUE_SPACE_ID, 0x1400_1800, 4);
    let marker = op(
        0,
        PcodeOpcode::Copy,
        Some(runtime_marker),
        vec![Varnode::constant(0, 4)],
    );
    let write = op(
        1,
        PcodeOpcode::Copy,
        Some(mapped_ram),
        vec![Varnode::constant(7, 4)],
    );
    let pcode = pcode_function(vec![block(vec![marker, write.clone()])]);
    let mut options = crate::midend::builder::materialize::test_support::test_options();
    options
        .global_names
        .insert(0x1400_1800, "counter".to_string());
    let mut builder = PreviewBuilder::new(&pcode, &options, None);

    let stmt = builder
        .maybe_materialize_output_stmt(0x1000, &pcode.blocks[0], 1, None, &write)
        .expect("mapped RAM output should lower")
        .expect("mapped RAM write is observable");

    assert!(
        matches!(
            stmt,
            PreHirStmt::Assign {
                lhs: PreHirLValue::Deref { ref ptr, .. },
                ..
            } if matches!(ptr.as_ref(), PreHirExpr::AddressOfGlobal(name) if name == "counter")
        ),
        "mapped RAM must remain a provenance-bearing memory lvalue: {stmt:?}"
    );
    assert!(
        !builder.used_param_local_names.contains("counter"),
        "a global must not be registered as a function local"
    );
}

#[test]
fn materialized_rhs_reentry_through_self_predecessor_fails_closed() {
    let eax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 4);
    let seed = op(1, PcodeOpcode::Copy, Some(eax.clone()), vec![constant(0)]);
    let update = op(
        2,
        PcodeOpcode::IntAdd,
        Some(eax.clone()),
        vec![eax, constant(1)],
    );
    let pcode = pcode_function(vec![
        block_at(0x1000, 0, vec![seed]),
        block_at(0x1010, 1, vec![update.clone()]),
    ]);
    let options = crate::midend::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    // The join's loop/back-edge predecessor is itself. Recovering that
    // predecessor definition therefore reaches the same unmaterialized
    // output def-site that started this RHS-recovery attempt.
    builder.predecessors[1] = vec![0, 1];

    let lowered = builder
        .with_lowering_site(
            LoweringSite {
                block_idx: 1,
                op_idx: 0,
            },
            |builder| {
                builder.try_lower_materialized_output_rhs(pcode.blocks[1].start_address, &update)
            },
        )
        .expect("cyclic predecessor recovery should fail closed");

    assert!(lowered.is_some(), "outer RHS recovery should remain usable");
    assert!(
        builder.active_materialized_rhs_keys.is_empty(),
        "active def-site markers must be balanced after recovery"
    );
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
    let options = crate::midend::builder::materialize::test_support::test_options();
    let builder = PreviewBuilder::new(&pcode, &options, None);

    assert!(builder.call_result_is_observed(&block, 0));
}

#[test]
fn full_width_return_extension_does_not_reuse_partial_call_carrier() {
    use crate::midend::cspec::test_maps::apply_preview_cspec;

    let rax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 8);
    let eax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 4);
    let rdx = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x10, 8);
    let mut block = block(vec![
        op(1, PcodeOpcode::Call, None, vec![constant(0x2000)]),
        // Preserve the call result in a different register before EAX is
        // cleared, matching the ABI shape that exposed the stale RAX carrier.
        op(2, PcodeOpcode::Copy, Some(rdx.clone()), vec![rax.clone()]),
        op(
            3,
            PcodeOpcode::IntXor,
            Some(eax.clone()),
            vec![eax.clone(), eax.clone()],
        ),
        op(4, PcodeOpcode::IntZExt, Some(rax.clone()), vec![eax]),
    ]);
    block.successors = vec![1];
    let use_block = block_at(
        0x1010,
        1,
        vec![op(
            5,
            PcodeOpcode::IntAdd,
            Some(register(RUST_SLEIGH_UNIQUE_SPACE_ID, 0x100, 8)),
            vec![rax.clone(), rdx.clone()],
        )],
    );
    let pcode = pcode_function(vec![block.clone(), use_block.clone()]);
    let mut options = crate::midend::builder::materialize::test_support::test_options();
    apply_preview_cspec(&mut options);
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.prime_call_result_bindings();

    // Lower the successor use first. This is the ordering that exposed the
    // real row: successor materialization can precede the predecessor's own
    // statement, so the copied return value must acquire a stable destination
    // binding before its defining Copy is visited.
    let rhs = builder
        .with_lowering_site(
            LoweringSite {
                block_idx: 1,
                op_idx: 0,
            },
            |builder| {
                builder
                    .try_lower_materialized_output_rhs(use_block.start_address, &use_block.ops[0])
            },
        )
        .expect("lower successor arithmetic use")
        .expect("successor arithmetic should have a RHS");
    let rdx_name = builder
        .materialized_vns
        .get(&MaterializedVarnodeKey::new(&rdx, &block.ops[1]))
        .cloned()
        .expect("cross-block register copy should seed a stable binding");
    assert!(
        matches!(
            &rhs,
            PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                rhs: add_rhs,
                ..
            } if add_rhs.as_ref() == &PreHirExpr::Var(rdx_name.clone())
        ),
        "successor read must use the copied destination, not the reused source register: {rhs:?}"
    );

    let statements = builder
        .lower_block_stmts(&block)
        .expect("lower partial return-register carrier");
    let zext_name = builder
        .materialized_vns
        .get(&MaterializedVarnodeKey::new(&rax, &block.ops[3]))
        .cloned();

    assert_eq!(
        zext_name.as_deref(),
        Some("rax"),
        "statements: {statements:?}"
    );
    assert_eq!(
        builder
            .materialized_vns
            .get(&MaterializedVarnodeKey::new(&rdx, &block.ops[1]))
            .map(String::as_str),
        Some(rdx_name.as_str()),
        "pre-seeded successor binding must survive predecessor materialization"
    );
}

/// CALL as CFG terminator + successor `mov reg, eax` must count as observed
/// (measured recursive dual-call pattern on PE x64 O0).
#[test]
fn call_result_observation_follows_successor_copy_of_return_register() {
    let ret_eax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 4);
    let ebx = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x0c, 4);
    let mut call_block = block_at(
        0x1000,
        0,
        vec![op(1, PcodeOpcode::Call, None, vec![constant(0x2000)])],
    );
    call_block.successors = vec![1];
    let use_block = block_at(
        0x1010,
        1,
        vec![op(2, PcodeOpcode::Copy, Some(ebx), vec![ret_eax])],
    );
    let pcode = pcode_function(vec![call_block.clone(), use_block]);
    let options = crate::midend::builder::materialize::test_support::test_options();
    let builder = PreviewBuilder::new(&pcode, &options, None);

    assert!(
        builder.call_result_is_observed(&call_block, 0),
        "terminator CALL whose successor copies EAX must observe the call result"
    );
}

/// A shared epilogue can observe a call result without naming the ABI return
/// register in its `Return` p-code.  Another predecessor may write a distinct
/// value (the failure arm), so the call's carrier must remain bound on the
/// successor path that falls through to the join.
#[test]
fn call_result_observation_follows_epilogue_return_path() {
    let rax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 8);
    let mut call_block = block_at(
        0x1000,
        0,
        vec![op(1, PcodeOpcode::Call, None, vec![constant(0x2000)])],
    );
    call_block.successors = vec![1, 2];

    let epilogue = block_at(
        0x1010,
        1,
        vec![op(2, PcodeOpcode::Return, None, vec![constant(0)])],
    );
    let mut failure = block_at(
        0x1020,
        2,
        vec![
            op(3, PcodeOpcode::Copy, Some(rax), vec![constant(-1)]),
            op(4, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
        ],
    );
    failure.successors = vec![1];

    let pcode = pcode_function(vec![call_block.clone(), epilogue, failure]);
    let options = crate::midend::builder::materialize::test_support::test_options();
    let builder = PreviewBuilder::new(&pcode, &options, None);

    assert!(
        builder.call_result_is_observed(&call_block, 0),
        "a call whose fallthrough reaches a value-bearing shared epilogue must bind its result"
    );
}

/// End-to-end: terminator CALL + successor save of return reg must bind the
/// call result into the save (`reg = f()` / `saved = ret`), not the pre-call
/// argument temp that still occupied the return register storage.
#[test]
fn terminator_call_successor_save_uses_call_result_not_precall_arg() {
    use crate::midend::cspec::test_maps::apply_preview_cspec;
    use crate::midend::render_mlil_preview;

    let eax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 4);
    let ecx = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x8, 4);
    let ebx = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x0c, 4);
    let mut options = crate::midend::builder::materialize::test_support::test_options();
    apply_preview_cspec(&mut options);

    // Block 0: arg = 3; call f;  (CALL terminator)
    // Block 1: ebx = eax; return ebx
    let mut call_block = block_at(
        0x1000,
        0,
        vec![
            op(0, PcodeOpcode::Copy, Some(eax.clone()), vec![constant(3)]),
            op(1, PcodeOpcode::Copy, Some(ecx.clone()), vec![eax.clone()]),
            op(2, PcodeOpcode::Call, None, vec![constant(0x2000)]),
        ],
    );
    call_block.successors = vec![1];
    let use_block = block_at(
        0x1010,
        1,
        vec![
            op(3, PcodeOpcode::Copy, Some(ebx.clone()), vec![eax.clone()]),
            op(4, PcodeOpcode::Return, None, vec![constant(0), ebx]),
        ],
    );
    let pcode = pcode_function(vec![call_block, use_block]);
    let code = render_mlil_preview(&pcode, "caller", 0x1000, &options).expect("preview");

    // Must not treat the pre-call arg (3 / ecx staging) as the saved return.
    let discards_result = code.contains("sub_2000")
        && !code.lines().any(|l| {
            let t = l.trim();
            // assignment form: name = sub_2000(...)
            t.contains("= sub_2000") || t.contains("=sub_2000")
        });
    assert!(
        !discards_result,
        "call result must be bound, not discarded as bare expression:\n{code}"
    );
    assert!(
        code.contains("sub_2000"),
        "expected a call to sub_2000:\n{code}"
    );
    // The saved copy should not simply re-export the literal pre-call arg.
    assert!(
        !code.contains("return 3;") && !code.contains("return 3 "),
        "must not return the pre-call argument constant as if it were the call result:\n{code}"
    );
}

#[test]
fn call_result_observation_ignores_successor_that_clobbers_return_first() {
    let ret_eax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 4);
    let ebx = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x0c, 4);
    let mut call_block = block_at(
        0x1000,
        0,
        vec![op(1, PcodeOpcode::Call, None, vec![constant(0x2000)])],
    );
    call_block.successors = vec![1];
    let use_block = block_at(
        0x1010,
        1,
        vec![
            op(
                2,
                PcodeOpcode::Copy,
                Some(ret_eax.clone()),
                vec![constant(0)],
            ),
            op(3, PcodeOpcode::Copy, Some(ebx), vec![ret_eax]),
        ],
    );
    let pcode = pcode_function(vec![call_block.clone(), use_block]);
    let options = crate::midend::builder::materialize::test_support::test_options();
    let builder = PreviewBuilder::new(&pcode, &options, None);

    assert!(
        !builder.call_result_is_observed(&call_block, 0),
        "successor that clobbers EAX before any use must not observe the prior call"
    );
}

#[test]
fn same_block_register_binding_splits_consumed_live_intervals() {
    let rax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 8);
    let saved = register(RUST_SLEIGH_UNIQUE_SPACE_ID, 0x100, 8);
    let block = block(vec![
        op(1, PcodeOpcode::Copy, Some(rax.clone()), vec![constant(1)]),
        op(2, PcodeOpcode::Copy, Some(rax.clone()), vec![constant(2)]),
        op(
            3,
            PcodeOpcode::IntAdd,
            Some(saved),
            vec![rax.clone(), constant(4)],
        ),
        op(4, PcodeOpcode::Copy, Some(rax.clone()), vec![constant(3)]),
    ]);
    let pcode = pcode_function(vec![block.clone()]);
    let options = crate::midend::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.materialized_vns.insert(
        MaterializedVarnodeKey::new(&rax, &block.ops[0]),
        "prior_value".to_string(),
    );

    assert_eq!(
        builder.same_block_prior_register_binding_name(&block, 1, &rax),
        None
    );
}

#[test]
fn same_block_register_binding_keeps_exact_self_update_chain() {
    let rax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 8);
    let first_use = register(RUST_SLEIGH_UNIQUE_SPACE_ID, 0x100, 8);
    let second_use = register(RUST_SLEIGH_UNIQUE_SPACE_ID, 0x108, 8);
    let block = block(vec![
        op(1, PcodeOpcode::Copy, Some(rax.clone()), vec![constant(1)]),
        op(
            2,
            PcodeOpcode::IntAdd,
            Some(first_use),
            vec![rax.clone(), constant(4)],
        ),
        op(
            3,
            PcodeOpcode::IntAdd,
            Some(rax.clone()),
            vec![rax.clone(), constant(2)],
        ),
        op(
            4,
            PcodeOpcode::IntAdd,
            Some(second_use),
            vec![rax.clone(), constant(8)],
        ),
        op(5, PcodeOpcode::Copy, Some(rax.clone()), vec![constant(3)]),
    ]);
    let pcode = pcode_function(vec![block.clone()]);
    let options = crate::midend::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.materialized_vns.insert(
        MaterializedVarnodeKey::new(&rax, &block.ops[0]),
        "prior_value".to_string(),
    );

    assert_eq!(
        builder.same_block_prior_register_binding_name(&block, 2, &rax),
        Some("prior_value".to_string())
    );
}

#[test]
fn same_block_register_binding_does_not_merge_independent_call_carriers() {
    let rcx = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x08, 8);
    let first = op(1, PcodeOpcode::Copy, Some(rcx.clone()), vec![constant(1)]);
    let call = op(2, PcodeOpcode::Call, None, vec![constant(0x2000)]);
    let second = op(3, PcodeOpcode::Copy, Some(rcx.clone()), vec![constant(2)]);
    let block = block(vec![first.clone(), call, second]);
    let pcode = pcode_function(vec![block.clone()]);
    let options = crate::midend::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.materialized_vns.insert(
        MaterializedVarnodeKey::new(&rcx, &block.ops[0]),
        "first_carrier".to_string(),
    );

    assert_eq!(
        builder.same_block_prior_register_binding_name(&block, 2, &rcx),
        None,
        "an ABI call consumes the prior carrier even though CALL has no register operands"
    );
}

#[test]
fn same_block_register_binding_never_skips_unmaterialized_definition() {
    let rax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 8);
    let block = block(vec![
        op(1, PcodeOpcode::Copy, Some(rax.clone()), vec![constant(1)]),
        op(2, PcodeOpcode::Copy, Some(rax.clone()), vec![constant(2)]),
        op(
            3,
            PcodeOpcode::IntAdd,
            Some(rax.clone()),
            vec![rax.clone(), constant(3)],
        ),
    ]);
    let pcode = pcode_function(vec![block.clone()]);
    let options = crate::midend::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.materialized_vns.insert(
        MaterializedVarnodeKey::new(&rax, &block.ops[0]),
        "stale_value".to_string(),
    );

    assert_eq!(
        builder.same_block_prior_register_binding_name(&block, 2, &rax),
        None
    );
}

#[test]
fn fallback_register_materialization_never_aliases_distinct_definitions() {
    let r12 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0xa0, 8);
    let first = op(1, PcodeOpcode::Copy, Some(r12.clone()), vec![constant(1)]);
    let second = op(2, PcodeOpcode::Copy, Some(r12.clone()), vec![constant(2)]);
    let pcode = pcode_function(vec![block(vec![first.clone(), second.clone()])]);
    let options = crate::midend::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);

    let first_binding = builder.ensure_temp_binding_for_output(&first, &r12, false);
    let second_binding = builder.ensure_temp_binding_for_output(&second, &r12, false);

    assert_eq!(first_binding.name, "r12");
    assert_ne!(first_binding.name, second_binding.name);
    assert_eq!(builder.materialized_vns.len(), 2);
}

#[test]
fn return_live_out_proof_rejects_definition_killed_on_every_exit_path() {
    let rax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 8);
    let mut definition = block_at(
        0x1000,
        0,
        vec![op(
            1,
            PcodeOpcode::Copy,
            Some(rax.clone()),
            vec![constant(1)],
        )],
    );
    definition.successors = vec![1];
    let mut overwrite = block_at(
        0x1010,
        1,
        vec![op(
            2,
            PcodeOpcode::IntAdd,
            Some(rax.clone()),
            vec![rax.clone(), constant(1)],
        )],
    );
    overwrite.successors = vec![2];
    let returned = block_at(
        0x1020,
        2,
        vec![op(3, PcodeOpcode::Return, None, vec![constant(0x2000)])],
    );
    let pcode = pcode_function(vec![definition, overwrite, returned]);
    let options = crate::midend::builder::materialize::test_support::test_options();
    let builder = PreviewBuilder::new(&pcode, &options, None);

    assert!(
        builder
            .prove_definition_reaches_return(0, 0, &rax)
            .is_none()
    );
}

#[test]
fn return_live_out_proof_accepts_kill_free_exit_path() {
    let rax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 8);
    let mut definition = block_at(
        0x1000,
        0,
        vec![op(
            1,
            PcodeOpcode::Copy,
            Some(rax.clone()),
            vec![constant(1)],
        )],
    );
    definition.successors = vec![1];
    let returned = block_at(
        0x1010,
        1,
        vec![op(2, PcodeOpcode::Return, None, vec![constant(0x2000)])],
    );
    let pcode = pcode_function(vec![definition, returned]);
    let options = crate::midend::builder::materialize::test_support::test_options();
    let builder = PreviewBuilder::new(&pcode, &options, None);

    let proof = builder
        .prove_definition_reaches_return(0, 0, &rax)
        .expect("definition reaches return without a kill");
    assert_eq!(proof.definition_site(), (0, 0));
    assert_eq!(proof.return_block(), 1);
}

#[test]
fn status_flag_materialization_keeps_canonical_reaching_definition_name() {
    let cf = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x200, 1);
    let first = op(1, PcodeOpcode::Copy, Some(cf.clone()), vec![constant(0)]);
    let second = op(2, PcodeOpcode::Copy, Some(cf.clone()), vec![constant(1)]);
    let pcode = pcode_function(vec![block(vec![first.clone(), second.clone()])]);
    let options = crate::midend::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);

    let first_binding = builder.ensure_temp_binding_for_output(&first, &cf, false);
    let second_binding = builder.ensure_temp_binding_for_output(&second, &cf, false);

    assert_eq!(first_binding.name, "cf");
    assert_eq!(first_binding.name, second_binding.name);
}

#[test]
fn predecessor_assignment_accepts_predicate_merge_consumers() {
    let pcode = pcode_function(vec![block(Vec::new())]);
    let options = crate::midend::builder::materialize::test_support::test_options();
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
    let options = crate::midend::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    let rhs = PreHirExpr::Const(5, type_from_size(8, false));

    let name = builder
        .merge_binding_name_for_direct_successor_accumulator(&pcode.blocks[0], 0, &rax, &rhs)
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
fn direct_successor_accumulator_does_not_claim_primary_return_join() {
    let rax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 8);
    let condition = register(UNIQUE_SPACE_ID, 0x300, 1);
    let pcode = pcode_function(vec![
        PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![2, 1],
            ops: vec![
                op(1, PcodeOpcode::Copy, Some(rax.clone()), vec![constant(1)]),
                op(
                    2,
                    PcodeOpcode::CBranch,
                    None,
                    vec![constant(0x1020), condition],
                ),
            ],
        },
        PcodeBasicBlock {
            index: 1,
            start_address: 0x1010,
            successors: vec![2],
            ops: vec![
                op(3, PcodeOpcode::Copy, Some(rax.clone()), vec![constant(2)]),
                op(4, PcodeOpcode::Branch, None, vec![constant(0x1020)]),
            ],
        },
        PcodeBasicBlock {
            index: 2,
            start_address: 0x1020,
            successors: Vec::new(),
            ops: vec![op(
                5,
                PcodeOpcode::Return,
                None,
                vec![constant(0xdead), constant(0xbeef)],
            )],
        },
    ]);
    let options = crate::midend::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);

    let name = builder.merge_binding_name_for_direct_successor_accumulator(
        &pcode.blocks[1],
        0,
        &rax,
        &PreHirExpr::Const(2, type_from_size(8, false)),
    );

    assert_eq!(
        name, None,
        "edge-sensitive return recovery must own a primary-return join"
    );
    assert!(
        !builder
            .explicit_merge_bindings
            .contains_key(&(2, VarnodeKey::from(&rax))),
        "a return join must not receive a flat accumulator carrier"
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
    let options = crate::midend::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    let rhs = PreHirExpr::Const(5, type_from_size(8, false));

    assert!(
        builder
            .merge_binding_name_for_direct_successor_accumulator(&pcode.blocks[0], 0, &rax, &rhs,)
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
    let options = crate::midend::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    let rhs = PreHirExpr::Const(5, type_from_size(8, false));

    let name = builder
        .merge_binding_name_for_direct_successor_accumulator(&pcode.blocks[0], 0, &r12, &rhs)
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
    let options = crate::midend::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    let rhs = PreHirExpr::Const(5, type_from_size(4, false));

    assert!(
        builder
            .merge_binding_name_for_direct_successor_accumulator(&pcode.blocks[0], 0, &r12d, &rhs,)
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
                op(
                    2,
                    PcodeOpcode::IntZExt,
                    Some(rax.clone()),
                    vec![eax.clone()],
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
            ops: vec![op(5, PcodeOpcode::Return, None, vec![rax.clone()])],
        },
        PcodeBasicBlock {
            index: 3,
            start_address: 0x1030,
            successors: vec![1, 2],
            ops: vec![
                op(
                    6,
                    PcodeOpcode::IntAdd,
                    Some(eax.clone()),
                    vec![eax.clone(), constant(1)],
                ),
                op(7, PcodeOpcode::CBranch, None, vec![constant(0x1010), cond]),
            ],
        },
    ]);
    let options = crate::midend::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.successors[3] = vec![1, 2];
    builder.predecessors[1] = vec![0, 3];
    builder.predecessors[2] = vec![1, 3];
    builder.loop_bodies = vec![crate::midend::structuring::loop_analysis::LoopBody {
        head: 1,
        tails: vec![3],
        body: vec![1, 3],
        exit_idx: Some(2),
        all_exits: vec![2],
    }];
    let rhs = PreHirExpr::Binary {
        op: PreHirBinaryOp::Add,
        lhs: Box::new(PreHirExpr::Var("rax".to_string())),
        rhs: Box::new(PreHirExpr::Const(1, type_from_size(4, false))),
        ty: type_from_size(4, false),
    };

    assert_eq!(
        builder.canonical_x86_gpr64_name_for_value(&eax),
        Some(("rax", 0))
    );
    assert!(builder.loop_header_external_predecessors_seed_zero(
        1,
        &builder.loop_bodies[0],
        0,
        false
    ));
    assert!(builder.block_reads_merge_input_before_redefinition(&pcode.blocks[2], &eax));
    assert!(!builder.loop_body_has_side_entry_or_irreducible_edge(&builder.loop_bodies[0]));
    assert!(
        builder
            .last_redefinition_index_before_terminator(&pcode.blocks[3], &eax)
            .is_some()
    );

    let name = builder.with_lowering_site(
        LoweringSite {
            block_idx: 3,
            op_idx: 0,
        },
        |builder| {
            builder
                .merge_binding_name_for_direct_successor_accumulator(
                    &pcode.blocks[3],
                    0,
                    &eax,
                    &rhs,
                )
                .expect("EAX (32-bit return register) must be accepted as a loop accumulator")
        },
    );

    assert_eq!(
        builder
            .explicit_merge_bindings
            .get(&(2, VarnodeKey::from(&eax))),
        Some(&name)
    );
    // Initializer must be 32-bit zero (output.size=4), not 64-bit (pointer_size=8)
    assert_eq!(
        builder
            .temps
            .get(&name)
            .and_then(|b| b.initializer.as_ref()),
        Some(&PreHirExpr::Const(0, type_from_size(4, false)))
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
    let options = crate::midend::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.successors[3] = vec![1, 2];
    builder.predecessors[1] = vec![0, 3];
    builder.predecessors[2] = vec![1, 3];
    builder.loop_bodies = vec![crate::midend::structuring::loop_analysis::LoopBody {
        head: 1,
        tails: vec![3],
        body: vec![1, 3],
        exit_idx: Some(2),
        all_exits: vec![2],
    }];
    let rhs = PreHirExpr::Const(7, type_from_size(8, false));
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
                .merge_binding_name_for_direct_successor_accumulator(
                    &pcode.blocks[3],
                    0,
                    &r10,
                    &rhs,
                )
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
        Some(&PreHirExpr::Const(0, type_from_size(8, false)))
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
    let options = crate::midend::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.successors[3] = vec![1, 2];
    builder.predecessors[2] = vec![0, 3];
    builder.loop_bodies = vec![crate::midend::structuring::loop_analysis::LoopBody {
        head: 1,
        tails: vec![3],
        body: vec![1, 3],
        exit_idx: Some(2),
        all_exits: vec![2],
    }];
    let external_rhs = PreHirExpr::Const(10, type_from_size(8, false));
    let latch_rhs = PreHirExpr::Const(7, type_from_size(8, false));

    let external_name = builder.with_lowering_site(
        LoweringSite {
            block_idx: 0,
            op_idx: 0,
        },
        |builder| {
            builder
                .merge_binding_name_for_direct_successor_accumulator(
                    &pcode.blocks[0],
                    0,
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
                    0,
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
    let options = crate::midend::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);

    let rhs = builder
        .stack_home_accumulator_store_rhs(&pcode.blocks[1], 0, &store, "home_4c", &ebp)
        .expect("stack-home accumulator merge");

    assert_eq!(rhs, PreHirExpr::Var("rbp".to_string()));
    assert!(builder.params.is_empty(), "must not promote rbp to a param");
    assert_eq!(
        builder
            .temps
            .get("rbp")
            .and_then(|binding| binding.initializer.as_ref()),
        Some(&PreHirExpr::Const(0, type_from_size(8, false)))
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
    let options = crate::midend::builder::materialize::test_support::test_options();
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

    assert_eq!(rhs, PreHirExpr::Var("rbp".to_string()));
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
    let options = crate::midend::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.predecessors[5] = vec![3, 4];
    builder.loop_bodies = vec![crate::midend::structuring::loop_analysis::LoopBody {
        head: 1,
        tails: vec![5],
        body: vec![1, 2, 3, 4, 5],
        exit_idx: Some(6),
        all_exits: vec![6],
    }];
    let stale_rhs = PreHirExpr::Binary {
        op: PreHirBinaryOp::Add,
        lhs: Box::new(PreHirExpr::Var("xVar53".to_string())),
        rhs: Box::new(PreHirExpr::Const(1, int(64))),
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
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Var("rbp".to_string())),
            rhs: Box::new(PreHirExpr::Const(1, int(64))),
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
    let options = crate::midend::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    let binding = builder.ensure_explicit_merge_binding_for_block(0, &rsi);
    let stale_rhs = PreHirExpr::Binary {
        op: PreHirBinaryOp::Sub,
        lhs: Box::new(PreHirExpr::Var("rbx".to_string())),
        rhs: Box::new(PreHirExpr::Var("xVar49".to_string())),
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
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Sub,
            lhs: Box::new(PreHirExpr::Var("rbx".to_string())),
            rhs: Box::new(PreHirExpr::Cast {
                ty: int(32),
                expr: Box::new(PreHirExpr::Var(binding.name)),
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
    let options = crate::midend::builder::materialize::test_support::test_options();
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
        PreHirBinding {
            name: "limit".to_string(),
            ty: int(64),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::TempPreserved),
            initializer: None,
        },
    );
    let stale_rhs = PreHirExpr::Binary {
        op: PreHirBinaryOp::Sub,
        lhs: Box::new(PreHirExpr::Var("rbx".to_string())),
        rhs: Box::new(PreHirExpr::Var("xVar49".to_string())),
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
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Sub,
            lhs: Box::new(PreHirExpr::Var("rbx".to_string())),
            rhs: Box::new(PreHirExpr::Cast {
                ty: int(32),
                expr: Box::new(PreHirExpr::Var("limit".to_string())),
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
    let options = crate::midend::builder::materialize::test_support::test_options();
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
    let stale_rhs = PreHirExpr::Binary {
        op: PreHirBinaryOp::Sub,
        lhs: Box::new(PreHirExpr::Var("rbx".to_string())),
        rhs: Box::new(PreHirExpr::Var("xVar49".to_string())),
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
    let options = crate::midend::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.ensure_live_register_binding("rbp", 8);
    builder.predecessors[3] = vec![0, 2];
    builder.loop_bodies = vec![crate::midend::structuring::loop_analysis::LoopBody {
        head: 1,
        tails: vec![2],
        body: vec![1, 2],
        exit_idx: Some(3),
        all_exits: vec![3],
    }];
    let stale_rhs = PreHirExpr::Binary {
        op: PreHirBinaryOp::Mul,
        lhs: Box::new(PreHirExpr::Var("xVar53".to_string())),
        rhs: Box::new(PreHirExpr::Const(1, int(64))),
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
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Mul,
            lhs: Box::new(PreHirExpr::Var("rbp".to_string())),
            rhs: Box::new(PreHirExpr::Const(1, int(64))),
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
    let options = crate::midend::builder::materialize::test_support::test_options();
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
    let options = crate::midend::builder::materialize::test_support::test_options();
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
    let options = crate::midend::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);

    let stmts = builder
        .synthesize_explicit_merge_bindings_for_block(&merge)
        .expect("synthesize merge binding");

    assert!(
        matches!(
            stmts.as_slice(),
            [PreHirStmt::Assign {
                rhs: PreHirExpr::Select { .. },
                ..
            }]
        ),
        "{stmts:?}"
    );
}

/// Regression for the `___chkstk_ms` miscompile found while wiring
/// `synthesize_explicit_merge_bindings_for_block` to fall back to
/// `scalar_ssa`'s own phi facts: a merge block downstream of a loop must
/// never synthesize an eager select/binding from an operand whose defining
/// op lives *inside* that loop's body, because "the op that defines this
/// value" is only a one-iteration delta, not the value that actually
/// reaches the merge after however many iterations really ran.
///
/// Shape: `v = 100;` then either skip straight to the merge, or enter a
/// self-loop that repeatedly does `v = v - 1;` before eventually falling
/// through to the same merge, which reads `v`. `v`'s value at the merge
/// depends on how many iterations ran -- there is no flat expression for
/// it, so no synthesis should fire here at all (for either predecessor).
#[test]
fn merge_bindings_decline_operand_defined_inside_a_loop_reaching_a_post_loop_merge() {
    let v = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4000, 4);
    let skip_cond = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4010, 4);
    let loop_cond = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4018, 4);
    let result = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x4020, 4);

    let mut entry = block_at(
        0x1000,
        0,
        vec![
            op(0, PcodeOpcode::Copy, Some(v.clone()), vec![constant(100)]),
            op(
                1,
                PcodeOpcode::CBranch,
                None,
                vec![Varnode::constant(0x1020, 8), skip_cond],
            ),
        ],
    );
    entry.successors = vec![2, 1];

    let mut loop_block = block_at(
        0x1010,
        1,
        vec![
            op(
                2,
                PcodeOpcode::IntSub,
                Some(v.clone()),
                vec![v.clone(), constant(1)],
            ),
            op(
                3,
                PcodeOpcode::CBranch,
                None,
                vec![Varnode::constant(0x1010, 8), loop_cond],
            ),
        ],
    );
    loop_block.successors = vec![1, 2];

    let mut merge = block_at(
        0x1020,
        2,
        vec![op(
            4,
            PcodeOpcode::IntAdd,
            Some(result),
            vec![v, constant(1)],
        )],
    );
    merge.successors = Vec::new();

    let pcode = pcode_function(vec![entry, loop_block, merge.clone()]);
    let options = crate::midend::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);

    let stmts = builder
        .synthesize_explicit_merge_bindings_for_block(&merge)
        .expect("synthesize merge binding");

    assert!(
        stmts.iter().all(|stmt| !matches!(
            stmt,
            PreHirStmt::Assign {
                rhs: PreHirExpr::Select { .. },
                ..
            }
        )),
        "a post-loop merge must never synthesize a select from an operand \
         defined inside the loop body -- it only captures one iteration's \
         delta, not the value after however many iterations really ran: \
         {stmts:?}"
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
    let mut options = crate::midend::builder::materialize::test_support::test_options();
    options.calling_convention = CallingConvention::AArch64;
    options.format = "ELF64".to_string();
    options.pe_x64_only = false;
    let builder = PreviewBuilder::new(&pcode, &options, None);
    let rhs = PreHirExpr::Cast {
        ty: int(64),
        expr: Box::new(PreHirExpr::Cast {
            ty: int(32),
            expr: Box::new(PreHirExpr::Var("xVar7".to_string())),
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
    let mut options = crate::midend::builder::materialize::test_support::test_options();
    options.calling_convention = CallingConvention::AArch64;
    options.format = "ELF64".to_string();
    options.pe_x64_only = false;
    let builder = PreviewBuilder::new(&pcode, &options, None);
    let rhs = PreHirExpr::Cast {
        ty: int(64),
        expr: Box::new(PreHirExpr::Var("uVar1".to_string())),
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
    let mut options = crate::midend::builder::materialize::test_support::test_options();
    options.calling_convention = CallingConvention::AArch64;
    options.format = "ELF64".to_string();
    options.pe_x64_only = false;
    let builder = PreviewBuilder::new(&pcode, &options, None);
    let rhs = PreHirExpr::Binary {
        op: PreHirBinaryOp::Add,
        lhs: Box::new(PreHirExpr::Var("w0".to_string())),
        rhs: Box::new(PreHirExpr::Const(1, int(32))),
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
    let mut options = crate::midend::builder::materialize::test_support::test_options();
    options.calling_convention = CallingConvention::WindowsX64;
    let builder = PreviewBuilder::new(&pcode, &options, None);
    let rhs = PreHirExpr::Var("r14".to_string());
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
        crate::midend::cspec::RegisterNamer::from_options(&options)
            .hw_name_at(r15d.offset, r15d.size),
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
fn loop_header_missing_merge_uses_entry_owned_parameter_binding() {
    let rdx = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x10, 4);
    let r14d = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0xb0, 4);
    let store_ptr = register(UNIQUE_SPACE_ID, 0x100, 8);
    let cond = register(UNIQUE_SPACE_ID, 0x108, 1);
    let mut entry = block_at(
        0x1000,
        0,
        vec![
            // An entry read proves that the ABI-owned RDX slot is param_2.
            op(
                0,
                PcodeOpcode::IntEqual,
                Some(cond.clone()),
                vec![rdx.clone(), Varnode::constant(0, 4)],
            ),
            op(
                1,
                PcodeOpcode::CBranch,
                None,
                vec![constant(0x1010), cond.clone()],
            ),
        ],
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
                vec![constant(0), store_ptr, rdx.clone()],
            ),
            op(3, PcodeOpcode::CBranch, None, vec![constant(0x1030), cond]),
        ],
    );
    header.successors = vec![3, 2];
    let mut body = block_at(
        0x1020,
        2,
        vec![
            // The backedge update is the same physical ABI register, but its
            // first loop-head value is the incoming param_2.
            op(4, PcodeOpcode::Copy, Some(rdx.clone()), vec![r14d]),
            op(5, PcodeOpcode::Branch, None, vec![constant(0x1010)]),
        ],
    );
    body.successors = vec![1];
    let exit = block_at(
        0x1030,
        3,
        vec![op(
            6,
            PcodeOpcode::Return,
            None,
            vec![constant(0), rdx.clone()],
        )],
    );
    let pcode = pcode_function(vec![entry, header, body.clone(), exit]);
    let mut options = crate::midend::builder::materialize::test_support::test_options();
    options.calling_convention = CallingConvention::WindowsX64;
    let builder = PreviewBuilder::new(&pcode, &options, None);
    let rhs = PreHirExpr::Var("r14".to_string());

    assert_eq!(builder.entry_arity, 2, "RDX entry read must prove param_2");
    assert_eq!(
        builder.live_register_lhs_name_for_safe_missing_merge(
            &body,
            0,
            &body.ops[0],
            &rdx,
            &rhs,
            ReplacementValuePlan::incomplete(
                ReplacementReadClass::Merge,
                MaterializationRejectionReason::MissingMergeBinding,
            ),
        ),
        Some(("param_2".to_string(), 4)),
        "an entry-owned loop carrier must retain the formal parameter seed"
    );
}

#[test]
fn loop_body_parameter_passthrough_uses_source_formal_before_carrier_write() {
    let rdx = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x10, 4);
    let rcx = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x08, 4);
    let r14d = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0xb0, 4);
    let cond = register(UNIQUE_SPACE_ID, 0x108, 1);
    let mut entry = block_at(
        0x2000,
        0,
        vec![
            // The entry read proves RDX is the second Windows x64 parameter.
            op(
                0,
                PcodeOpcode::IntEqual,
                Some(cond.clone()),
                vec![rdx.clone(), Varnode::constant(0, 4)],
            ),
            op(
                1,
                PcodeOpcode::CBranch,
                None,
                vec![constant(0x2030), cond.clone()],
            ),
        ],
    );
    entry.successors = vec![1, 3];
    let mut preheader = block_at(0x2010, 1, vec![]);
    preheader.successors = vec![2];
    let mut body = block_at(
        0x2020,
        2,
        vec![
            // The loop carrier is RCX, but its first value comes from RDX.
            op(2, PcodeOpcode::Copy, Some(rcx.clone()), vec![rdx.clone()]),
            op(3, PcodeOpcode::Copy, Some(rdx.clone()), vec![r14d]),
            op(4, PcodeOpcode::CBranch, None, vec![constant(0x2020), cond]),
        ],
    );
    body.successors = vec![2, 3];
    let exit = block_at(
        0x2030,
        3,
        vec![op(
            5,
            PcodeOpcode::Return,
            None,
            vec![constant(0), rcx.clone()],
        )],
    );
    let pcode = pcode_function(vec![entry, preheader, body, exit]);
    let mut options = crate::midend::builder::materialize::test_support::test_options();
    options.calling_convention = CallingConvention::WindowsX64;
    let mut builder = PreviewBuilder::new(&pcode, &options, None);

    assert_eq!(builder.entry_arity, 2, "RDX entry read must prove param_2");
    assert!(
        {
            builder.current_lowering_site = Some(LoweringSite {
                block_idx: 2,
                op_idx: 0,
            });
            builder.loop_body_carried_register_read_name(&rdx)
        } == Some("param_2".to_string()),
        "the first loop-body read must use the entry formal, not bare RDX"
    );
}

#[test]
fn loop_body_parameter_passthrough_keeps_dominating_seed_definition() {
    let rax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x00, 4);
    let rdx = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x10, 4);
    let rdx_wide = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x10, 8);
    let cond = register(UNIQUE_SPACE_ID, 0x120, 1);
    let mut entry = block_at(
        0x2100,
        0,
        vec![
            // RDX is an ABI-capable parameter, but the loop consumes the
            // derived upper bound, not the caller's original `n`.
            op(
                0,
                PcodeOpcode::IntSub,
                Some(rdx.clone()),
                vec![rdx.clone(), Varnode::constant(1, 4)],
            ),
            // Register lookup may see this wider alias as the dominating
            // definition for the narrow loop-carried view.
            op(1, PcodeOpcode::IntZExt, Some(rdx_wide), vec![rdx.clone()]),
            op(2, PcodeOpcode::Branch, None, vec![constant(0x2120)]),
        ],
    );
    entry.successors = vec![1];
    let mut body = block_at(
        0x2120,
        1,
        vec![
            // This passthrough is the loop-head read that used to be
            // incorrectly named `param_2`.
            op(3, PcodeOpcode::Copy, Some(rax), vec![rdx.clone()]),
            op(
                4,
                PcodeOpcode::IntAdd,
                Some(rdx.clone()),
                vec![rdx.clone(), Varnode::constant(1, 4)],
            ),
            op(5, PcodeOpcode::CBranch, None, vec![constant(0x2120), cond]),
        ],
    );
    body.successors = vec![1, 2];
    let exit = block_at(0x2130, 2, vec![op(6, PcodeOpcode::Return, None, vec![])]);
    let pcode = pcode_function(vec![entry, body, exit]);
    let mut options = crate::midend::builder::materialize::test_support::test_options();
    options.calling_convention = CallingConvention::WindowsX64;
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.current_lowering_site = Some(LoweringSite {
        block_idx: 1,
        op_idx: 0,
    });

    assert_eq!(builder.entry_arity, 2, "RDX must remain ABI-visible input");
    assert_ne!(
        builder.loop_body_carried_register_read_name(&rdx),
        Some("param_2".to_string()),
        "a dominating RDX seed must not be replaced by the original parameter"
    );
}

#[test]
fn shared_loop_exit_uses_entry_alias_carrier_binding() {
    let rax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x00, 4);
    let rcx = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x08, 4);
    let rdx = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x10, 4);
    let seed = op(0, PcodeOpcode::Copy, Some(rax.clone()), vec![rcx.clone()]);
    let mut entry = block_at(
        0x3000,
        0,
        vec![
            seed.clone(),
            op(1, PcodeOpcode::Branch, None, vec![constant(0x3010)]),
        ],
    );
    entry.successors = vec![1, 2];
    let mut body = block_at(
        0x3010,
        1,
        vec![
            // The loop tail transfers the value read at the shared exit into
            // the entry-owned RAX alias carrier.
            op(2, PcodeOpcode::Copy, Some(rax.clone()), vec![rcx.clone()]),
            op(3, PcodeOpcode::Branch, None, vec![constant(0x3010)]),
        ],
    );
    body.successors = vec![1, 2];
    let exit = block_at(0x3030, 2, vec![op(4, PcodeOpcode::Return, None, vec![rdx])]);
    let pcode = pcode_function(vec![entry, body, exit]);
    let mut options = crate::midend::builder::materialize::test_support::test_options();
    options.calling_convention = CallingConvention::WindowsX64;
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.predecessors[2] = vec![0, 1];
    // These are the entry-analysis facts supplied by the real x64 prologue:
    // RAX is an alias carrier for the first ABI register, and the loop tail's
    // RAX copy is therefore the stable binding for the shared exit.
    builder.register_param_aliases.insert(rax.offset, 0);
    builder.entry_arity = 1;
    builder.loop_bodies = vec![crate::midend::structuring::loop_analysis::LoopBody {
        head: 1,
        tails: vec![1],
        body: vec![1],
        exit_idx: Some(2),
        all_exits: vec![2],
    }];
    builder
        .materialized_vns
        .insert(MaterializedVarnodeKey::new(&rax, &seed), "rax".to_string());
    builder.temps.insert(
        "rax".to_string(),
        PreHirBinding {
            name: "rax".to_string(),
            ty: type_from_size(4, false),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::TempPreserved),
            initializer: None,
        },
    );
    builder.current_lowering_site = Some(LoweringSite {
        block_idx: 2,
        op_idx: 0,
    });

    let binding = builder.loop_exit_materialized_register_binding(&rcx);
    assert!(
        matches!(binding, Some(PreHirExpr::Var(ref name)) if name == "rax"),
        "shared exit must use the loop-tail's entry-alias carrier, got {binding:?}"
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
    let options = crate::midend::builder::materialize::test_support::test_options();
    let builder = PreviewBuilder::new(&pcode, &options, None);
    let rhs = PreHirExpr::Call {
        target: "may_call".to_string(),
        args: vec![PreHirExpr::Var("r14".to_string())],
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
    let mut options = crate::midend::builder::materialize::test_support::test_options();
    options.calling_convention = CallingConvention::AArch64;
    options.format = "ELF64".to_string();
    options.pe_x64_only = false;
    let builder = PreviewBuilder::new(&pcode, &options, None);
    let rhs = PreHirExpr::Binary {
        op: PreHirBinaryOp::Add,
        lhs: Box::new(PreHirExpr::Call {
            target: "__pcodeop_294".to_string(),
            args: vec![PreHirExpr::Var("reg".to_string())],
            ty: NirType::Aggregate {
                size: 16,
                fields: Vec::new(),
            },
        }),
        rhs: Box::new(PreHirExpr::Const(4, int(32))),
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
    let options = crate::midend::builder::materialize::test_support::test_options();
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
    let options = crate::midend::builder::materialize::test_support::test_options();
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
fn call_result_width_uses_all_reachable_reads_before_full_overwrite() {
    let wide = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 8);
    let low = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 4);
    let copied = register(UNIQUE_SPACE_ID, 0x100, 4);
    for (later_read, expected) in [(low.clone(), 4), (wide.clone(), 8)] {
        let pcode = pcode_function(vec![block(vec![
            op(0, PcodeOpcode::Call, None, vec![constant(0x2000)]),
            op(
                1,
                PcodeOpcode::Copy,
                Some(copied.clone()),
                vec![low.clone()],
            ),
            op(
                2,
                PcodeOpcode::Store,
                None,
                vec![constant(0), constant(0x3000), later_read],
            ),
            op(3, PcodeOpcode::Copy, Some(wide.clone()), vec![constant(0)]),
        ])]);
        let options = crate::midend::builder::materialize::test_support::test_options();
        let mut builder = PreviewBuilder::new(&pcode, &options, None);
        let site = LoweringSite {
            block_idx: 0,
            op_idx: 0,
        };
        assert_eq!(
            builder.observed_call_result_use_width(site, &wide),
            expected
        );
        let name = builder.ensure_call_result_binding(site, &pcode.blocks[0].ops[0]);
        assert_eq!(builder.temps[&name].ty, type_from_size(expected, false));
        if expected == 4 {
            builder.irreducible_edges.insert((0, 0));
            assert_eq!(builder.observed_call_result_use_width(site, &wide), 8);
        }
    }
}

#[test]
fn call_result_width_preserves_register_argument_projection() {
    let wide = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 8);
    let low = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 4);
    let argument = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x88, 4);
    let pcode = pcode_function(vec![block(vec![
        op(0, PcodeOpcode::Call, None, vec![constant(0x2000)]),
        op(1, PcodeOpcode::Copy, Some(argument), vec![low]),
        op(2, PcodeOpcode::Call, None, vec![constant(0x3000)]),
    ])]);
    let options = crate::midend::builder::materialize::test_support::test_options();
    let builder = PreviewBuilder::new(&pcode, &options, None);
    assert_eq!(
        builder.observed_call_result_use_width(
            LoweringSite {
                block_idx: 0,
                op_idx: 0
            },
            &wide,
        ),
        8,
    );
}

#[test]
fn call_result_width_preserves_nondominated_carrier_join() {
    let wide = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 8);
    let low = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 4);
    let mut entry = block_at(
        0x1000,
        0,
        vec![op(
            0,
            PcodeOpcode::CBranch,
            None,
            vec![constant(0x1200), register(UNIQUE_SPACE_ID, 0x200, 1)],
        )],
    );
    entry.successors = vec![1, 2];
    let mut call_block = block_at(
        0x1100,
        1,
        vec![
            op(0, PcodeOpcode::Call, None, vec![constant(0x2000)]),
            op(
                1,
                PcodeOpcode::Copy,
                Some(register(UNIQUE_SPACE_ID, 0x100, 4)),
                vec![low.clone()],
            ),
            op(2, PcodeOpcode::Branch, None, vec![constant(0x1300)]),
        ],
    );
    call_block.successors = vec![3];
    let mut other = block_at(
        0x1200,
        2,
        vec![op(
            0,
            PcodeOpcode::Copy,
            Some(wide.clone()),
            vec![constant(42)],
        )],
    );
    other.successors = vec![3];
    let join = block_at(
        0x1300,
        3,
        vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(register(UNIQUE_SPACE_ID, 0x104, 4)),
                vec![low],
            ),
            op(1, PcodeOpcode::Copy, Some(wide.clone()), vec![constant(0)]),
        ],
    );
    let pcode = pcode_function(vec![entry, call_block, other, join]);
    let options = crate::midend::builder::materialize::test_support::test_options();
    let builder = PreviewBuilder::new(&pcode, &options, None);
    assert_eq!(
        builder.observed_call_result_use_width(
            LoweringSite {
                block_idx: 1,
                op_idx: 0
            },
            &wide,
        ),
        8,
    );
}

#[test]
fn call_result_width_preserves_partial_overwrites_and_live_returns() {
    let wide = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 8);
    let low = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 4);
    for last in [
        op(2, PcodeOpcode::Copy, Some(low.clone()), vec![constant(0)]),
        op(2, PcodeOpcode::Return, None, vec![constant(0)]),
        op(2, PcodeOpcode::Call, None, vec![constant(0x3000)]),
        op(2, PcodeOpcode::CallInd, None, vec![constant(0x3000)]),
    ] {
        let pcode = pcode_function(vec![block(vec![
            op(0, PcodeOpcode::Call, None, vec![constant(0x2000)]),
            op(
                1,
                PcodeOpcode::Copy,
                Some(register(UNIQUE_SPACE_ID, 0x100, 4)),
                vec![low.clone()],
            ),
            last,
            op(3, PcodeOpcode::Return, None, vec![constant(0)]),
        ])]);
        let options = crate::midend::builder::materialize::test_support::test_options();
        let builder = PreviewBuilder::new(&pcode, &options, None);
        assert_eq!(
            builder.observed_call_result_use_width(
                LoweringSite {
                    block_idx: 0,
                    op_idx: 0
                },
                &wide
            ),
            8
        );
    }
}

#[test]
fn call_result_width_does_not_ignore_a_wider_successor() {
    let wide = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 8);
    let low = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 4);
    let mut entry = block_at(
        0x1000,
        0,
        vec![
            op(0, PcodeOpcode::Call, None, vec![constant(0x2000)]),
            op(
                1,
                PcodeOpcode::Copy,
                Some(register(UNIQUE_SPACE_ID, 0x100, 4)),
                vec![low.clone()],
            ),
            op(
                2,
                PcodeOpcode::CBranch,
                None,
                vec![constant(0x1200), register(UNIQUE_SPACE_ID, 0x200, 1)],
            ),
        ],
    );
    entry.successors = vec![1, 2];
    let successors = [low, wide.clone()]
        .into_iter()
        .enumerate()
        .map(|(index, read)| {
            block_at(
                0x1100 + index as u64 * 0x100,
                index as u32 + 1,
                vec![
                    op(
                        1,
                        PcodeOpcode::Copy,
                        Some(register(UNIQUE_SPACE_ID, 0x108, read.size)),
                        vec![read],
                    ),
                    op(2, PcodeOpcode::Copy, Some(wide.clone()), vec![constant(0)]),
                ],
            )
        });
    let pcode = pcode_function(std::iter::once(entry).chain(successors).collect());
    let options = crate::midend::builder::materialize::test_support::test_options();
    let builder = PreviewBuilder::new(&pcode, &options, None);
    assert_eq!(
        builder.observed_call_result_use_width(
            LoweringSite {
                block_idx: 0,
                op_idx: 0
            },
            &wide
        ),
        8
    );
}

#[test]
fn call_result_width_preserves_implicit_argument_reads_on_shared_abi_carriers() {
    let mut options = crate::midend::builder::materialize::test_support::test_options();
    options.calling_convention = CallingConvention::AArch64;
    options.format = "ELF64".to_string();
    options.pe_x64_only = false;
    crate::midend::cspec::test_maps::apply_preview_cspec(&mut options);
    let empty = pcode_function(vec![block(Vec::new())]);
    let model = PreviewBuilder::new(&empty, &options, None);
    let carrier = model
        .call_result_registers()
        .into_iter()
        .find(|reg| !model.register_namer().is_float_return_register(reg))
        .expect("integer return carrier");
    assert!(
        model
            .register_namer()
            .int_param_offsets
            .contains(&carrier.offset)
    );
    let mut low = carrier.clone();
    low.size = 4;
    let pcode = pcode_function(vec![block(vec![
        op(0, PcodeOpcode::Call, None, vec![constant(0x2000)]),
        op(
            1,
            PcodeOpcode::Copy,
            Some(register(UNIQUE_SPACE_ID, 0x100, 4)),
            vec![low],
        ),
        op(2, PcodeOpcode::Call, None, vec![constant(0x4000)]),
    ])]);
    let builder = PreviewBuilder::new(&pcode, &options, None);
    assert_eq!(
        builder.observed_call_result_use_width(
            LoweringSite {
                block_idx: 0,
                op_idx: 0
            },
            &carrier
        ),
        carrier.size
    );
}

#[test]
fn call_result_width_preserves_float_carrier_policy() {
    let options = crate::midend::builder::materialize::test_support::test_options();
    let empty = pcode_function(vec![block(Vec::new())]);
    let model = PreviewBuilder::new(&empty, &options, None);
    let carrier = model
        .call_result_registers()
        .into_iter()
        .find(|reg| model.register_namer().is_float_return_register(reg))
        .expect("floating return carrier");
    let mut low = carrier.clone();
    low.size = 4;
    let pcode = pcode_function(vec![block(vec![
        op(0, PcodeOpcode::Call, None, vec![constant(0x2000)]),
        op(
            1,
            PcodeOpcode::Store,
            None,
            vec![constant(0), constant(0x3000), low],
        ),
        op(
            2,
            PcodeOpcode::Copy,
            Some(carrier.clone()),
            vec![constant(0)],
        ),
    ])]);
    let builder = PreviewBuilder::new(&pcode, &options, None);
    assert_eq!(
        builder.observed_call_result_use_width(
            LoweringSite {
                block_idx: 0,
                op_idx: 0
            },
            &carrier
        ),
        carrier.size
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
    let options = crate::midend::builder::materialize::test_support::test_options();
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
    let options = crate::midend::builder::materialize::test_support::test_options();
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
    let mut options = crate::midend::builder::materialize::test_support::test_options();
    options.is_64bit = false;
    options.pointer_size = 4;
    options.calling_convention = CallingConvention::Arm32;
    crate::midend::cspec::test_maps::apply_preview_cspec(&mut options);
    let mut builder = PreviewBuilder::new(&pcode, &options, None);

    let stmts = builder
        .lower_block_stmts(&block)
        .expect("lower ARM call block");

    let call_result = stmts.iter().find_map(|stmt| match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(result),
            rhs: PreHirExpr::Call { args, .. },
        } if matches!(args.as_slice(), [PreHirExpr::Const(7, _)]) => Some(result.as_str()),
        _ => None,
    });
    let call_result =
        call_result.unwrap_or_else(|| panic!("missing call with r0 argument: {stmts:?}"));
    assert!(
        stmts.iter().any(|stmt| matches!(
            stmt,
            PreHirStmt::Assign {
                rhs: PreHirExpr::Binary { lhs, rhs, .. },
                ..
            } if matches!(lhs.as_ref(), PreHirExpr::Var(name) if name == call_result)
                || matches!(rhs.as_ref(), PreHirExpr::Var(name) if name == call_result)
        )),
        "call result was not used by the following instruction: {stmts:?}"
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
    let options = crate::midend::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);

    let stmts = builder
        .lower_block_stmts(&second_duplicate)
        .expect("lower duplicate block");

    assert!(
        matches!(
            stmts.as_slice(),
            [
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var(def),
                    rhs: PreHirExpr::Const(7, _),
                },
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Deref { .. },
                    rhs: PreHirExpr::Cast { expr, .. },
                },
            ] if matches!(expr.as_ref(), PreHirExpr::Var(used) if used == def)
        ),
        "duplicate-address block did not retain its own definition: {stmts:?}"
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
    let options = crate::midend::builder::materialize::test_support::test_options();
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
fn recursive_stack_address_uses_producer_site() {
    let rsp = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x20, 8);
    let rdi = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x38, 8);
    let r12 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0xa0, 8);
    let pcode = pcode_function(vec![block_at(
        0x1000,
        0,
        vec![
            // Preserve the incoming output pointer in a callee-saved register.
            op(0, PcodeOpcode::Copy, Some(r12.clone()), vec![rdi.clone()]),
            // Reuse the argument register later as a local stack address.
            op(
                1,
                PcodeOpcode::IntAdd,
                Some(rdi.clone()),
                vec![rsp, Varnode::constant(0x30, 8)],
            ),
            op(
                2,
                PcodeOpcode::Store,
                None,
                vec![
                    Varnode::constant(3, 4),
                    r12.clone(),
                    Varnode::constant(0, 16),
                ],
            ),
            op(
                3,
                PcodeOpcode::Store,
                None,
                vec![
                    Varnode::constant(3, 4),
                    rdi.clone(),
                    Varnode::constant(0, 16),
                ],
            ),
        ],
    )]);
    let mut options = crate::midend::builder::materialize::test_support::test_options();
    options.calling_convention = CallingConvention::SystemVAmd64;
    options.selection_axis = fission_midend_core::ir::SelectionAxis::Jumps;
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.stack_frame_size = 0x40;
    builder.rsp_prologue_delta_table.insert(
        LoweringSite {
            block_idx: 0,
            op_idx: 1,
        },
        -0x10,
    );
    builder.rsp_prologue_delta_table.insert(
        LoweringSite {
            block_idx: 0,
            op_idx: 3,
        },
        -0x20,
    );

    builder.current_lowering_site = Some(LoweringSite {
        block_idx: 0,
        op_idx: 2,
    });
    assert_eq!(
        builder.resolve_stack_address(&r12),
        None,
        "the preserved output pointer must not inherit the argument register's later stack value"
    );

    builder.current_lowering_site = Some(LoweringSite {
        block_idx: 0,
        op_idx: 3,
    });
    assert_eq!(
        builder
            .resolve_stack_address(&rdi)
            .map(|(_, offset)| offset),
        Some(0x60),
        "the copied stack address must use the RSP value at its producer operation"
    );
}

#[test]
fn arm_immediate_through_unique_temp_recovers_stack_local_address() {
    // ARM lifts `add.w r3,sp,#0x6` as `Copy u <- const(6)` followed by
    // `IntAdd r3 <- sp, u`, so the displacement reaches the add as a
    // unique-space temp rather than a literal. `resolve_constant_operand`
    // gated on a bare `UNIQUE_SPACE_ID` (3) while Rust-Sleigh emits unique
    // temps in space 2, so the add never resolved to a stack address and an
    // escaping local's address printed as raw `sp + 6` with the slot itself
    // left undefined. x86's `lea` carries the literal inline, which is why
    // only ARM ever saw this.
    let sp = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x54, 4);
    let store_addr = register(RUST_SLEIGH_UNIQUE_SPACE_ID, 0x143600, 4);
    let stored = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x20, 1);
    let imm = register(RUST_SLEIGH_UNIQUE_SPACE_ID, 0x12f000, 4);
    let r3 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x2c, 4);
    let r0 = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x20, 4);

    let pcode = pcode_function(vec![block_at(
        0x1000,
        0,
        vec![
            // `strb.w r0,[sp,#0x6]` -- the memory-access form, whose literal
            // displacement is what registers the slot at offset 6.
            op(
                0,
                PcodeOpcode::IntAdd,
                Some(store_addr.clone()),
                vec![sp.clone(), constant(6)],
            ),
            op(
                1,
                PcodeOpcode::Store,
                None,
                vec![constant(3), store_addr, stored],
            ),
            // `add.w r3,sp,#0x6` -- the address-computation form under test.
            op(2, PcodeOpcode::Copy, Some(imm.clone()), vec![constant(6)]),
            op(
                3,
                PcodeOpcode::IntAdd,
                Some(r3.clone()),
                vec![sp.clone(), imm],
            ),
            // The address has to be read as a value, or the guard declines it.
            op(4, PcodeOpcode::Copy, Some(r0), vec![r3]),
        ],
    )]);

    let mut options = crate::midend::builder::materialize::test_support::test_options();
    options.calling_convention = CallingConvention::Arm32;
    options.is_64bit = false;
    options.pointer_size = 4;
    options.format = "ELF".to_string();
    options.pe_x64_only = false;

    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder
        .run_incremental_heritage()
        .expect("heritage registers the stack slot the store touches");
    builder.current_lowering_site = Some(LoweringSite {
        block_idx: 0,
        op_idx: 3,
    });

    let address = builder.stack_local_address_expr(&pcode.blocks[0].ops[3]);

    assert!(
        matches!(
            address,
            Some(PreHirExpr::AddressOfLocal(_) | PreHirExpr::PtrOffset { .. })
        ),
        "an immediate routed through a unique temp must still resolve to a \
         stack local's address, got {address:?}"
    );
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
    let mut options = crate::midend::builder::materialize::test_support::test_options();
    options.calling_convention = CallingConvention::AArch64;
    options.format = "ELF64".to_string();
    options.pe_x64_only = false;
    crate::midend::cspec::test_maps::sync_preview_cspec(&mut options);

    let code = render_mlil_preview(&pcode, "duplicate_merge", 0x1000, &options).expect("render");
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
    let mut options = crate::midend::builder::materialize::test_support::test_options();
    options.calling_convention = CallingConvention::AArch64;
    options.format = "ELF64".to_string();
    options.pe_x64_only = false;

    let code = render_mlil_preview(&pcode, "madd_addend", 0x1000, &options).expect("render");
    assert!(code.contains(" * "), "{code}");
    assert!(code.contains(" + "), "{code}");
    assert!(code.contains(" / "), "{code}");
    assert!(!code.contains("{\n    }"), "{code}");
}

#[test]
fn sat_o2_cmov_block_probe_materialize() {
    use crate::midend::PreviewBuilder;
    use crate::midend::ir::{MlilPreviewOptions, StructuringEngineKind};
    use crate::midend::support::{CallingConvention, RUST_SLEIGH_REGISTER_SPACE_ID};
    use crate::pcode::{PcodeBasicBlock, PcodeFunction, PcodeOp, PcodeOpcode, Varnode};
    use fission_midend_prehir::PreHirStmt;

    let eax = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let ecx = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 4,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let edx = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 8,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let of = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x20b,
        size: 1,
        is_constant: false,
        constant_val: 0,
    };
    let sf = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x207,
        size: 1,
        is_constant: false,
        constant_val: 0,
    };
    let uniq_a = Varnode {
        space_id: crate::midend::UNIQUE_SPACE_ID,
        offset: 0x66a00,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let uniq_b = Varnode {
        space_id: crate::midend::UNIQUE_SPACE_ID,
        offset: 0x64d00,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let ne = Varnode {
        space_id: crate::midend::UNIQUE_SPACE_ID,
        offset: 0x18700,
        size: 1,
        is_constant: false,
        constant_val: 0,
    };
    let neg = Varnode {
        space_id: crate::midend::UNIQUE_SPACE_ID,
        offset: 0x64e00,
        size: 1,
        is_constant: false,
        constant_val: 0,
    };
    let next = Varnode {
        space_id: 3,
        offset: 0x4016a2,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };

    // Minimal: cmp ecx,eax; mov edx,INT_MIN; cmovl eax,edx  (as pcode)
    let pcode = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x401698,
            successors: vec![1],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x401698,
                    output: Some(uniq_a.clone()),
                    inputs: vec![ecx.clone()],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntSBorrow,
                    address: 0x401698,
                    output: Some(of.clone()),
                    inputs: vec![uniq_a.clone(), eax.clone()],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::IntSLess,
                    address: 0x401698,
                    output: Some(sf.clone()),
                    inputs: vec![
                        Varnode {
                            space_id: crate::midend::UNIQUE_SPACE_ID,
                            offset: 0x66c00,
                            size: 4,
                            is_constant: false,
                            constant_val: 0,
                        },
                        Varnode::constant(0, 4),
                    ],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 3,
                    opcode: PcodeOpcode::Copy,
                    address: 0x40169a,
                    output: Some(edx.clone()),
                    inputs: vec![Varnode::constant(i64::from(i32::MIN), 4)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 4,
                    opcode: PcodeOpcode::IntNotEqual,
                    address: 0x40169f,
                    output: Some(ne.clone()),
                    inputs: vec![of, sf],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 5,
                    opcode: PcodeOpcode::Copy,
                    address: 0x40169f,
                    output: Some(uniq_b.clone()),
                    inputs: vec![edx.clone()],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 6,
                    opcode: PcodeOpcode::BoolNegate,
                    address: 0x40169f,
                    output: Some(neg.clone()),
                    inputs: vec![ne],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 7,
                    opcode: PcodeOpcode::CBranch,
                    address: 0x40169f,
                    output: None,
                    inputs: vec![next, neg],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 8,
                    opcode: PcodeOpcode::Copy,
                    address: 0x40169f,
                    output: Some(eax),
                    inputs: vec![uniq_b],
                    asm_mnemonic: None,
                },
            ],
        }],
    };
    let mut options = MlilPreviewOptions {
        pe_x64_only: false,
        is_64bit: false,
        pointer_size: 4,
        format: "PE32".to_string(),
        image_base: 0x401000,
        sections: vec![(0x401000, 0x402000)],
        calling_convention: CallingConvention::X86_32,
        structuring_engine: StructuringEngineKind::GraphCollapseV1,
        ..Default::default()
    };
    crate::midend::cspec::test_maps::apply_preview_cspec(&mut options);
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    // The instruction-local CBranch is not a block terminator; materialize
    // must still wrap the tail body so INT_MIN is not dropped or applied
    // unconditionally.
    assert_eq!(
        builder.block_terminator_index(&pcode.blocks[0]),
        None,
        "instruction-local CBranch must stay in the op stream"
    );
    let stmts = builder.lower_block_stmts(&pcode.blocks[0]).expect("lower");
    let dump = format!("{stmts:?}");
    assert!(
        stmts.iter().any(|s| matches!(s, PreHirStmt::If { .. })),
        "need if from terminator cmov, got {dump}"
    );
    assert!(
        dump.contains("2147483648") || dump.contains("-2147483648") || dump.contains("80000000"),
        "INT_MIN in guarded body: {dump}"
    );
}

/// A same-block-forward CBranch is the p-code shape of a conditional move.
/// When it rewrites an entry register, the guarded write and later reads must
/// share the entry register's stable carrier. Otherwise the post-cmov read
/// refers to a binding that is only initialized on the guarded path.
#[test]
fn same_block_cmov_entry_register_keeps_selected_value_for_later_read() {
    use crate::midend::PreviewBuilder;
    use crate::midend::builder::materialize::test_support::test_options;
    use crate::midend::support::RUST_SLEIGH_REGISTER_SPACE_ID;
    use crate::pcode::{PcodeBasicBlock, PcodeFunction, PcodeOp, PcodeOpcode, Varnode};
    use fission_midend_prehir::{PreHirLValue, PreHirStmt};

    let r8 = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x80,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let rcx = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x08,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let prior = Varnode {
        space_id: crate::midend::UNIQUE_SPACE_ID,
        offset: 0x100,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let cond = Varnode {
        space_id: crate::midend::UNIQUE_SPACE_ID,
        offset: 0x108,
        size: 1,
        is_constant: false,
        constant_val: 0,
    };
    let selected = Varnode {
        space_id: crate::midend::UNIQUE_SPACE_ID,
        offset: 0x110,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let cmov_target = Varnode {
        space_id: 3,
        offset: 0x1004,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let pcode = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: Vec::new(),
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1000,
                    output: Some(prior.clone()),
                    inputs: vec![r8.clone()],
                    asm_mnemonic: Some("cmp".to_string()),
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::IntLess,
                    address: 0x1001,
                    output: Some(cond.clone()),
                    inputs: vec![prior, rcx.clone()],
                    asm_mnemonic: Some("cmp".to_string()),
                },
                PcodeOp {
                    seq_num: 2,
                    opcode: PcodeOpcode::CBranch,
                    address: 0x1002,
                    output: None,
                    inputs: vec![cmov_target, cond],
                    asm_mnemonic: Some("cmov".to_string()),
                },
                PcodeOp {
                    seq_num: 3,
                    opcode: PcodeOpcode::Copy,
                    address: 0x1003,
                    output: Some(r8.clone()),
                    inputs: vec![rcx],
                    asm_mnemonic: Some("cmov".to_string()),
                },
                PcodeOp {
                    seq_num: 4,
                    opcode: PcodeOpcode::IntAdd,
                    address: 0x1004,
                    output: Some(selected.clone()),
                    inputs: vec![r8, Varnode::constant(1, 8)],
                    asm_mnemonic: Some("use-selected".to_string()),
                },
                PcodeOp {
                    seq_num: 5,
                    opcode: PcodeOpcode::Return,
                    address: 0x1005,
                    output: None,
                    inputs: vec![Varnode::constant(0, 8), selected],
                    asm_mnemonic: Some("ret".to_string()),
                },
            ],
        }],
    };
    let options = test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    let stmts = builder
        .lower_block_stmts(&pcode.blocks[0])
        .expect("lower conditional register carrier");
    let dump = format!("{stmts:?}");

    let guarded_lhs = stmts.iter().find_map(|stmt| match stmt {
        PreHirStmt::If { then_body, .. } => then_body.iter().find_map(|inner| match inner {
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(name),
                ..
            } => Some(name.clone()),
            _ => None,
        }),
        _ => None,
    });
    assert_eq!(
        guarded_lhs.as_deref(),
        Some("param_3"),
        "guarded entry-register write must update its stable carrier: {dump}"
    );
    assert!(
        dump.contains("Var(\"param_3\")"),
        "post-cmov read must consume the selected carrier: {dump}"
    );
}

/// A successor can be lowered before the block containing a same-block cmov.
/// The successor's register read must still refer to the cmov's entry-owned
/// carrier; re-expanding the cmov RHS would select the source register's ABI
/// parameter instead.
#[test]
fn successor_read_reuses_same_block_cmov_entry_register_carrier() {
    use crate::midend::PreviewBuilder;
    use crate::midend::builder::materialize::test_support::test_options;
    use crate::midend::support::{PreHirBinaryOp, RUST_SLEIGH_REGISTER_SPACE_ID};
    use crate::pcode::{PcodeBasicBlock, PcodeFunction, PcodeOp, PcodeOpcode, Varnode};
    use fission_midend_prehir::PreHirExpr;

    let r8 = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x80,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let rdx = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x10,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let rcx = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x08,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let selected = Varnode {
        space_id: crate::midend::UNIQUE_SPACE_ID,
        offset: 0x100,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let cond = Varnode {
        space_id: crate::midend::UNIQUE_SPACE_ID,
        offset: 0x108,
        size: 1,
        is_constant: false,
        constant_val: 0,
    };
    let cmov_target = Varnode {
        space_id: 3,
        offset: 0x1004,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let sum = Varnode {
        space_id: crate::midend::UNIQUE_SPACE_ID,
        offset: 0x110,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };

    let entry = PcodeBasicBlock {
        index: 0,
        start_address: 0x1000,
        successors: vec![1],
        ops: vec![
            PcodeOp {
                seq_num: 0,
                opcode: PcodeOpcode::Copy,
                address: 0x1000,
                output: Some(selected.clone()),
                inputs: vec![rdx.clone()],
                asm_mnemonic: Some("cmp".to_string()),
            },
            PcodeOp {
                seq_num: 1,
                opcode: PcodeOpcode::IntLess,
                address: 0x1001,
                output: Some(cond.clone()),
                inputs: vec![selected.clone(), r8.clone()],
                asm_mnemonic: Some("cmp".to_string()),
            },
            PcodeOp {
                seq_num: 2,
                opcode: PcodeOpcode::CBranch,
                address: 0x1002,
                output: None,
                inputs: vec![cmov_target, cond],
                asm_mnemonic: Some("cmov".to_string()),
            },
            PcodeOp {
                seq_num: 3,
                opcode: PcodeOpcode::Copy,
                address: 0x1003,
                output: Some(r8.clone()),
                inputs: vec![selected],
                asm_mnemonic: Some("cmov".to_string()),
            },
            PcodeOp {
                seq_num: 4,
                opcode: PcodeOpcode::Branch,
                address: 0x1004,
                output: None,
                inputs: vec![Varnode::constant(0x2000, 8)],
                asm_mnemonic: Some("jmp".to_string()),
            },
        ],
    };
    let successor = PcodeBasicBlock {
        index: 1,
        start_address: 0x2000,
        successors: Vec::new(),
        ops: vec![PcodeOp {
            seq_num: 5,
            opcode: PcodeOpcode::IntAdd,
            address: 0x2000,
            output: Some(sum),
            inputs: vec![r8, rcx],
            asm_mnemonic: Some("add".to_string()),
        }],
    };
    let pcode = PcodeFunction {
        blocks: vec![entry, successor.clone()],
    };
    let options = test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);

    // Match the real materializer ordering: lower the successor use before
    // visiting the predecessor's cmov definition.
    let rhs = builder
        .with_lowering_site(
            crate::midend::builder::LoweringSite {
                block_idx: 1,
                op_idx: 0,
            },
            |builder| {
                builder
                    .try_lower_materialized_output_rhs(successor.start_address, &successor.ops[0])
            },
        )
        .expect("lower successor arithmetic use")
        .expect("successor arithmetic should have a RHS");

    assert!(
        matches!(
            rhs,
            PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                ref lhs,
                ref rhs,
                ..
            } if **lhs == PreHirExpr::Var("param_3".to_string())
                && **rhs == PreHirExpr::Var("param_1".to_string())
        ),
        "successor read must reuse the cmov entry carrier, not re-expand its RDX source: {rhs:?}"
    );
}

/// saturating_add shape: primary return reg gets a+b; must keep the binding even
/// when the only same-block p-code consumer can inline the add into a compare.
#[test]
fn primary_return_add_is_materialized_despite_single_block_inline_consumer() {
    use crate::midend::ir::{MlilPreviewOptions, StructuringEngineKind};
    use crate::midend::support::{CallingConvention, RUST_SLEIGH_REGISTER_SPACE_ID};
    use crate::midend::{PreviewBuilder, render_mlil_preview};
    use crate::pcode::{PcodeBasicBlock, PcodeFunction, PcodeOp, PcodeOpcode, Varnode};

    let eax = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let ecx = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 4,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let edx = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 8,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let sum_tmp = Varnode {
        space_id: crate::midend::UNIQUE_SPACE_ID,
        offset: 0x6c00,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let sf = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x207,
        size: 1,
        is_constant: false,
        constant_val: 0,
    };
    let pcode = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                successors: vec![1],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::Copy,
                        address: 0x1000,
                        output: Some(ecx.clone()),
                        inputs: vec![Varnode::constant(3, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Copy,
                        address: 0x1001,
                        output: Some(edx.clone()),
                        inputs: vec![Varnode::constant(5, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x1002,
                        output: Some(sum_tmp.clone()),
                        inputs: vec![ecx.clone(), edx.clone()],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 3,
                        opcode: PcodeOpcode::Copy,
                        address: 0x1002,
                        output: Some(eax.clone()),
                        inputs: vec![sum_tmp],
                        asm_mnemonic: None,
                    },
                    // Only same-block "consumer" of eax for replacement analysis is this cmp.
                    PcodeOp {
                        seq_num: 4,
                        opcode: PcodeOpcode::IntSLess,
                        address: 0x1003,
                        output: Some(sf),
                        inputs: vec![eax.clone(), ecx],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 5,
                        opcode: PcodeOpcode::Branch,
                        address: 0x1004,
                        output: None,
                        inputs: vec![Varnode {
                            space_id: 3,
                            offset: 0x1010,
                            size: 4,
                            is_constant: false,
                            constant_val: 0,
                        }],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x1010,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 6,
                    opcode: PcodeOpcode::Return,
                    address: 0x1010,
                    output: None,
                    inputs: vec![Varnode {
                        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
                        offset: 0x284,
                        size: 4,
                        is_constant: false,
                        constant_val: 0,
                    }],
                    asm_mnemonic: None,
                }],
            },
        ],
    };
    let mut options = MlilPreviewOptions {
        pe_x64_only: false,
        is_64bit: false,
        pointer_size: 4,
        format: "PE32".to_string(),
        image_base: 0x1000,
        sections: vec![(0x1000, 0x2000)],
        calling_convention: CallingConvention::X86_32,
        structuring_engine: StructuringEngineKind::GraphCollapseV1,
        ..Default::default()
    };
    crate::midend::cspec::test_maps::apply_preview_cspec(&mut options);
    let code = render_mlil_preview(&pcode, "sum_ret", 0x1000, &options).expect("render");
    assert!(
        code.contains("eax =") || code.contains("return 8") || code.contains("return 3 + 5"),
        "expected sum materialization or constant fold of sum, got:\n{code}"
    );
    assert!(
        !code.contains("return eax;")
            || code.contains("eax =")
            || code.contains("return 8")
            || code.contains("return "),
        "bare return eax without dominating sum def:\n{code}"
    );
}

/// Multi-block cmov tail: guarded INT_MIN must materialize onto the primary
/// return register name (`eax`), not a dead temp, so epilogue `return eax` sees it.
#[test]
fn sat_o2_cmov_tail_renders_int_min_through_epilogue() {
    use crate::midend::PreviewBuilder;
    use crate::midend::ir::{MlilPreviewOptions, StructuringEngineKind};
    use crate::midend::support::{CallingConvention, RUST_SLEIGH_REGISTER_SPACE_ID};
    use crate::pcode::{PcodeBasicBlock, PcodeFunction, PcodeOp, PcodeOpcode, Varnode};
    use fission_midend_prehir::{PreHirExpr, PreHirLValue, PreHirStmt};

    let eax = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let ecx = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 4,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let edx = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 8,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let of = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x20b,
        size: 1,
        is_constant: false,
        constant_val: 0,
    };
    let sf = Varnode {
        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
        offset: 0x207,
        size: 1,
        is_constant: false,
        constant_val: 0,
    };
    let uniq_a = Varnode {
        space_id: crate::midend::UNIQUE_SPACE_ID,
        offset: 0x66a00,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let uniq_b = Varnode {
        space_id: crate::midend::UNIQUE_SPACE_ID,
        offset: 0x64d00,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let uniq_sub = Varnode {
        space_id: crate::midend::UNIQUE_SPACE_ID,
        offset: 0x66c00,
        size: 4,
        is_constant: false,
        constant_val: 0,
    };
    let ne = Varnode {
        space_id: crate::midend::UNIQUE_SPACE_ID,
        offset: 0x18700,
        size: 1,
        is_constant: false,
        constant_val: 0,
    };
    let neg = Varnode {
        space_id: crate::midend::UNIQUE_SPACE_ID,
        offset: 0x64e00,
        size: 1,
        is_constant: false,
        constant_val: 0,
    };

    let pcode = PcodeFunction {
        blocks: vec![
            PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                successors: vec![1],
                ops: vec![
                    PcodeOp {
                        seq_num: 0,
                        opcode: PcodeOpcode::IntAdd,
                        address: 0x1000,
                        output: Some(eax.clone()),
                        inputs: vec![ecx.clone(), edx.clone()],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 1,
                        opcode: PcodeOpcode::Branch,
                        address: 0x1001,
                        output: None,
                        inputs: vec![Varnode::constant(0x1010, 4)],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 1,
                start_address: 0x1010,
                successors: vec![2],
                ops: vec![
                    PcodeOp {
                        seq_num: 2,
                        opcode: PcodeOpcode::Copy,
                        address: 0x1010,
                        output: Some(uniq_a.clone()),
                        inputs: vec![ecx.clone()],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 3,
                        opcode: PcodeOpcode::IntSBorrow,
                        address: 0x1010,
                        output: Some(of.clone()),
                        inputs: vec![uniq_a.clone(), eax.clone()],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 4,
                        opcode: PcodeOpcode::IntSub,
                        address: 0x1010,
                        output: Some(uniq_sub.clone()),
                        inputs: vec![uniq_a.clone(), eax.clone()],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 5,
                        opcode: PcodeOpcode::IntSLess,
                        address: 0x1010,
                        output: Some(sf.clone()),
                        inputs: vec![uniq_sub, Varnode::constant(0, 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 6,
                        opcode: PcodeOpcode::Copy,
                        address: 0x1011,
                        output: Some(edx.clone()),
                        inputs: vec![Varnode::constant(i64::from(i32::MIN), 4)],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 7,
                        opcode: PcodeOpcode::IntNotEqual,
                        address: 0x1012,
                        output: Some(ne.clone()),
                        inputs: vec![of, sf],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 8,
                        opcode: PcodeOpcode::Copy,
                        address: 0x1012,
                        output: Some(uniq_b.clone()),
                        inputs: vec![edx.clone()],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 9,
                        opcode: PcodeOpcode::BoolNegate,
                        address: 0x1012,
                        output: Some(neg.clone()),
                        inputs: vec![ne],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 10,
                        opcode: PcodeOpcode::CBranch,
                        address: 0x1012,
                        output: None,
                        inputs: vec![
                            Varnode {
                                space_id: 3,
                                offset: 0x1020,
                                size: 4,
                                is_constant: false,
                                constant_val: 0,
                            },
                            neg,
                        ],
                        asm_mnemonic: None,
                    },
                    PcodeOp {
                        seq_num: 11,
                        opcode: PcodeOpcode::Copy,
                        address: 0x1012,
                        output: Some(eax.clone()),
                        inputs: vec![uniq_b],
                        asm_mnemonic: None,
                    },
                ],
            },
            PcodeBasicBlock {
                index: 2,
                start_address: 0x1020,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 12,
                    opcode: PcodeOpcode::Return,
                    address: 0x1020,
                    output: None,
                    inputs: vec![Varnode {
                        space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
                        offset: 0x284,
                        size: 4,
                        is_constant: false,
                        constant_val: 0,
                    }],
                    asm_mnemonic: None,
                }],
            },
        ],
    };
    let mut options = MlilPreviewOptions {
        pe_x64_only: false,
        is_64bit: false,
        pointer_size: 4,
        format: "PE32".to_string(),
        image_base: 0x1000,
        sections: vec![(0x1000, 0x2000)],
        calling_convention: CallingConvention::X86_32,
        structuring_engine: StructuringEngineKind::GraphCollapseV1,
        ..Default::default()
    };
    crate::midend::cspec::test_maps::apply_preview_cspec(&mut options);

    // Seed live-in primary return binding as the sum (from predecessor block).
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    let _ = builder.lower_block_stmts(&pcode.blocks[0]).expect("entry");
    let stmts = builder
        .lower_block_stmts(&pcode.blocks[1])
        .expect("cmov block");
    let dump = format!("{stmts:?}");
    assert!(
        stmts.iter().any(|s| matches!(s, PreHirStmt::If { .. })),
        "cmov block needs if, got {dump}"
    );
    let assigns_int_min_to_eax = stmts.iter().any(|s| match s {
        PreHirStmt::If { then_body, .. } => then_body.iter().any(|inner| matches!(
            inner,
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(name),
                rhs: PreHirExpr::Const(v, _),
            } if name == "eax" && (*v == i64::from(i32::MIN) as i64 || *v == 2147483648i64 || *v == -2147483648i64)
        )),
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            rhs: PreHirExpr::Const(v, _),
        } if name == "eax" && (*v == i64::from(i32::MIN) as i64 || *v == 2147483648i64 || *v == -2147483648i64) => true,
        _ => false,
    });
    assert!(
        assigns_int_min_to_eax,
        "INT_MIN must assign to eax (not a dead temp): {dump}"
    );
}

/// A tail CMOV's guarded return-register write reaches the next block only on
/// one path. It must not be promoted to the direct-successor merge binding,
/// because that binding would be read by the shared return join even when the
/// CMOV skipped the write.
#[test]
fn guarded_cmov_return_write_does_not_claim_successor_merge() {
    use crate::midend::support::CallingConvention;

    let rax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 8);
    let cond = register(crate::midend::UNIQUE_SPACE_ID, 0x100, 1);
    let target = Varnode {
        space_id: 3,
        offset: 0x1020,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let cmov = block_at(
        0x1010,
        1,
        vec![
            PcodeOp {
                seq_num: 1,
                opcode: PcodeOpcode::Copy,
                address: 0x1010,
                output: Some(rax.clone()),
                inputs: vec![constant(1)],
                asm_mnemonic: None,
            },
            PcodeOp {
                seq_num: 2,
                opcode: PcodeOpcode::CBranch,
                address: 0x1012,
                output: None,
                inputs: vec![target.clone(), cond.clone()],
                asm_mnemonic: None,
            },
            PcodeOp {
                seq_num: 3,
                opcode: PcodeOpcode::Copy,
                address: 0x1012,
                output: Some(rax.clone()),
                inputs: vec![constant(2)],
                asm_mnemonic: None,
            },
        ],
    );
    let join = block_at(
        0x1020,
        2,
        vec![PcodeOp {
            seq_num: 4,
            opcode: PcodeOpcode::Return,
            address: 0x1020,
            output: None,
            inputs: vec![Varnode::constant(0xdead, 8)],
            asm_mnemonic: None,
        }],
    );
    let alternate = block_at(
        0x1030,
        3,
        vec![
            PcodeOp {
                seq_num: 5,
                opcode: PcodeOpcode::Copy,
                address: 0x1030,
                output: Some(rax.clone()),
                inputs: vec![constant(3)],
                asm_mnemonic: None,
            },
            PcodeOp {
                seq_num: 6,
                opcode: PcodeOpcode::Branch,
                address: 0x1031,
                output: None,
                inputs: vec![target.clone()],
                asm_mnemonic: None,
            },
        ],
    );
    let entry = block_at(
        0x1000,
        0,
        vec![PcodeOp {
            seq_num: 0,
            opcode: PcodeOpcode::Branch,
            address: 0x1000,
            output: None,
            inputs: vec![Varnode {
                space_id: 3,
                offset: 0x1010,
                size: 8,
                is_constant: false,
                constant_val: 0,
            }],
            asm_mnemonic: None,
        }],
    );
    let pcode = pcode_function(vec![entry, cmov, join, alternate]);
    let mut options = crate::midend::builder::materialize::test_support::test_options();
    options.calling_convention = CallingConvention::WindowsX64;
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    assert!(
        builder.op_is_inside_same_block_forward_cmov_body(&pcode.blocks[1], 2),
        "the guarded definition must be recognized as a tail CMOV body"
    );
    assert_eq!(
        builder.successors[1],
        vec![2],
        "the instruction-local branch must leave the block with its layout successor"
    );
    assert_eq!(
        builder.predecessors[2],
        vec![1, 3],
        "the return block must be a real multi-predecessor join"
    );

    let name = builder.merge_binding_name_for_direct_successor_accumulator(
        &pcode.blocks[1],
        2,
        &rax,
        &PreHirExpr::Const(2, int(64)),
    );
    assert_eq!(
        name, None,
        "a conditional definition must not claim an unconditional join carrier"
    );

    // The same proof must reject an unconditional-looking definition from a
    // different predecessor when another incoming edge ends in a guarded
    // CMOV write. Otherwise that other predecessor creates the shared binding
    // and the return join consumes the CMOV RHS as if it were unconditional.
    let name = builder.merge_binding_name_for_direct_successor_accumulator(
        &pcode.blocks[3],
        0,
        &rax,
        &PreHirExpr::Const(3, int(64)),
    );
    assert_eq!(
        name, None,
        "a guarded definition on any incoming edge must reject the shared join carrier"
    );
}

/// Merge bindings are keyed by `(block, varnode)`, but the value they stand
/// for is the varnode: two blocks merging the same storage must agree on the
/// name. The hardware-name promotion only fires on a loop head, so a join
/// block merging the same register used to mint a second name -- registered
/// without an assignment, and picked up by return recovery. `list_sum` at
/// gcc -O1 returned `xVar16`, a declared local with no definition anywhere,
/// instead of the accumulator it had already named `rax`.
#[test]
fn a_varnode_merged_in_two_blocks_keeps_one_name() {
    let rax = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 8);
    let block = block(vec![op(
        0,
        PcodeOpcode::Copy,
        Some(rax.clone()),
        vec![constant(0)],
    )]);
    let pcode = pcode_function(vec![block]);
    let options = crate::midend::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);

    let first = builder.ensure_explicit_merge_binding_for_block(0, &rax);
    let second = builder.ensure_explicit_merge_binding_for_block(1, &rax);

    assert_eq!(
        first.name, second.name,
        "the same varnode merged at two blocks must keep one name, got {:?} then {:?}",
        first.name, second.name
    );
}

fn redefined_abi_merge_fixture(
    incoming_size: Option<u32>,
    guarded: bool,
    call_after_write: bool,
) -> (crate::PcodeFunction, MlilPreviewOptions, Varnode) {
    let carrier = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x08, 8);
    let entry_view = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x08, 4);
    let condition = register(UNIQUE_SPACE_ID, 0x108, 1);
    let mut entry = block_at(
        0x1000,
        0,
        vec![
            op(
                0,
                PcodeOpcode::IntEqual,
                Some(condition.clone()),
                vec![entry_view, Varnode::constant(0, 4)],
            ),
            op(
                1,
                PcodeOpcode::CBranch,
                None,
                vec![constant(0x1020), condition.clone()],
            ),
        ],
    );
    entry.successors = vec![1, 2];
    let mut first = block_at(
        0x1010,
        1,
        vec![
            op(
                2,
                PcodeOpcode::Copy,
                Some(carrier.clone()),
                vec![constant(0x123456789abcdef0)],
            ),
            op(7, PcodeOpcode::Branch, None, vec![constant(0x1030)]),
        ],
    );
    first.successors = vec![3];
    let mut second_ops = Vec::new();
    if guarded {
        second_ops.push(op(
            3,
            PcodeOpcode::CBranch,
            None,
            vec![
                Varnode {
                    space_id: 3,
                    offset: 0x1022,
                    size: 8,
                    is_constant: false,
                    constant_val: 0,
                },
                condition,
            ],
        ));
    }
    if let Some(size) = incoming_size {
        second_ops.push(op(
            4,
            PcodeOpcode::Copy,
            Some(register(carrier.space_id, carrier.offset, size)),
            vec![Varnode::constant(0x76543210, size)],
        ));
    }
    if call_after_write {
        second_ops.push(op(5, PcodeOpcode::Call, None, vec![constant(0x2000)]));
    }
    second_ops.push(op(8, PcodeOpcode::Branch, None, vec![constant(0x1030)]));
    let mut second = block_at(0x1020, 2, second_ops);
    second.successors = vec![3];
    let join = block_at(
        0x1030,
        3,
        vec![op(
            6,
            PcodeOpcode::Return,
            None,
            vec![constant(0), carrier.clone()],
        )],
    );
    let mut options = crate::midend::builder::materialize::test_support::test_options();
    options.calling_convention = CallingConvention::WindowsX64;
    let mut blocks = vec![entry, first, second, join];
    for block in &mut blocks {
        for (index, op) in block.ops.iter_mut().enumerate() {
            op.address = block.start_address + index as u64;
        }
    }
    (pcode_function(blocks), options, carrier)
}

#[test]
fn redefined_abi_merge_separates_fully_initialized_value_from_formal() {
    let (pcode, options, carrier) = redefined_abi_merge_fixture(Some(8), false, false);
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    let formal = builder
        .register_param(&register(carrier.space_id, carrier.offset, 4))
        .unwrap();
    assert!(builder.merge_has_independent_incoming_definitions(3, &carrier));
    let merge = builder.ensure_explicit_merge_binding_for_block(3, &carrier);
    assert_ne!(merge.name, formal);
    assert_eq!(
        merge.ty,
        type_from_size(8, false),
        "later pointer bits need full storage width"
    );
    assert_eq!(
        builder.params[&0].ty,
        type_from_size(4, false),
        "entry scalar remains narrow"
    );
    assert_eq!(
        builder
            .ensure_explicit_merge_binding_for_block(3, &carrier)
            .name,
        merge.name
    );
}

#[test]
fn redefined_abi_merge_retains_entry_state_without_complete_edge_proof() {
    for (size, guarded, call) in [
        (None, false, false),
        (Some(4), false, false),
        (Some(8), true, false),
        (Some(8), false, true),
    ] {
        let (pcode, options, carrier) = redefined_abi_merge_fixture(size, guarded, call);
        let mut builder = PreviewBuilder::new(&pcode, &options, None);
        assert!(
            !builder.merge_has_independent_incoming_definitions(3, &carrier),
            "size={size:?} guarded={guarded} call={call}"
        );
        let formal = builder.register_param(&carrier).unwrap();
        assert_eq!(
            builder
                .ensure_explicit_merge_binding_for_block(3, &carrier)
                .name,
            formal
        );
    }
}

#[test]
fn redefined_abi_merge_does_not_borrow_formal_merge_from_another_block() {
    let (pcode, options, carrier) = redefined_abi_merge_fixture(Some(8), false, false);
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    let entry_carrier = builder.ensure_explicit_merge_binding_for_block(0, &carrier);
    let later = builder.ensure_explicit_merge_binding_for_block(3, &carrier);
    assert_ne!(entry_carrier.name, later.name);
    assert_eq!(
        builder.existing_merge_binding_name_for_varnode(&carrier, true),
        Some(later.name),
        "an earlier formal must not hide the available independent carrier"
    );
}

#[test]
fn redefined_abi_merge_existing_carrier_selection_is_deterministic() {
    let (pcode, options, carrier) = redefined_abi_merge_fixture(Some(8), false, false);
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    let first = builder.ensure_explicit_merge_binding_for_block(3, &carrier);
    let mut second = first.clone();
    second.name = "independent_other".to_string();
    builder.temps.insert(second.name.clone(), second.clone());
    builder
        .explicit_merge_bindings
        .insert((4, VarnodeKey::from(&carrier)), second.name);
    for _ in 0..16 {
        assert_eq!(
            builder.existing_merge_binding_name_for_varnode(&carrier, true),
            Some(first.name.clone())
        );
    }
}

#[test]
fn redefined_abi_merge_checks_entry_path_even_if_structuring_pruned_it() {
    let (pcode, options, carrier) = redefined_abi_merge_fixture(Some(8), false, false);
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    // Model an original incoming edge removed only by structuring.
    builder.heritage_predecessors[3].push(0);
    builder.predecessors[3].push(0);
    builder.predecessors[3].retain(|&pred| pred != 0);
    assert!(!builder.merge_has_independent_incoming_definitions(3, &carrier));
    let formal = builder.register_param(&carrier).unwrap();
    assert_eq!(
        builder
            .ensure_explicit_merge_binding_for_block(3, &carrier)
            .name,
        formal
    );
}

#[test]
fn redefined_abi_merge_follows_forwarded_values_on_every_original_path() {
    let (pcode, options, carrier) = redefined_abi_merge_fixture(Some(8), false, false);
    let mut blocks = pcode.blocks;
    blocks[2].ops.last_mut().unwrap().inputs[0] = constant(0x1040);
    blocks[2].successors = vec![4];
    let mut forwarded = block_at(
        0x1040,
        4,
        vec![op(9, PcodeOpcode::Branch, None, vec![constant(0x1030)])],
    );
    forwarded.ops[0].address = 0x1040;
    forwarded.successors = vec![3];
    blocks.push(forwarded);
    let pcode = pcode_function(blocks);
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    assert!(builder.merge_has_independent_incoming_definitions(3, &carrier));
    let formal = builder.register_param(&carrier).unwrap();
    assert_ne!(
        builder
            .ensure_explicit_merge_binding_for_block(3, &carrier)
            .name,
        formal
    );

    // The forwarding block additionally receives an unchanged entry value.
    // One seeded predecessor is not enough to initialize every path.
    builder.heritage_predecessors[4].push(0);
    assert!(!builder.merge_has_independent_incoming_definitions(3, &carrier));
}

#[test]
fn ssa_phi_emission_initializes_entry_and_backedge_with_proven_coalescing() {
    let carrier = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x38, 8);
    let condition = crate::midend::builder::materialize::test_support::varnode(0x900);
    let mut entry = block_at(
        0x1000,
        0,
        vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(carrier.clone()),
                vec![constant(0)],
            ),
            op(5, PcodeOpcode::Branch, None, vec![constant(0x2000)]),
        ],
    );
    entry.successors = vec![1];
    let mut head = block_at(
        0x2000,
        1,
        vec![
            op(
                1,
                PcodeOpcode::IntLess,
                Some(condition.clone()),
                vec![carrier.clone(), constant(4)],
            ),
            op(
                2,
                PcodeOpcode::CBranch,
                None,
                vec![constant(0x4000), condition],
            ),
        ],
    );
    head.successors = vec![2, 3];
    let mut latch = block_at(
        0x3000,
        2,
        vec![
            op(
                3,
                PcodeOpcode::IntAdd,
                Some(carrier.clone()),
                vec![carrier.clone(), constant(1)],
            ),
            op(6, PcodeOpcode::Branch, None, vec![constant(0x2000)]),
        ],
    );
    latch.successors = vec![1];
    let exit = block_at(
        0x4000,
        3,
        vec![op(4, PcodeOpcode::Return, None, vec![carrier.clone()])],
    );
    let pcode = pcode_function(vec![entry, head, latch, exit]);
    let options = crate::midend::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.prepare_ssa_emission();
    let phi = builder.scalar_ssa.phis[&1]
        .iter()
        .find(|p| p.storage.offset == carrier.offset)
        .unwrap()
        .clone();
    let name = builder.ssa_emission.bindings[&phi.output].clone();
    let entry_name = builder.ssa_emission.bindings[&phi
        .operands
        .iter()
        .find(|p| p.predecessor == 0)
        .unwrap()
        .value]
        .clone();
    let latch_name = builder.ssa_emission.bindings[&phi
        .operands
        .iter()
        .find(|p| p.predecessor == 2)
        .unwrap()
        .value]
        .clone();
    assert!(!name.starts_with("param_"));
    let entry_body = builder.lower_block_stmts(&pcode.blocks[0]).unwrap();
    assert!(entry_body.iter().any(|stmt| matches!(stmt, PreHirStmt::Assign { lhs: PreHirLValue::Var(lhs), .. } if *lhs == name)));
    if name != entry_name {
        assert!(
            matches!(entry_body.last(), Some(PreHirStmt::Assign { lhs: PreHirLValue::Var(lhs), rhs: PreHirExpr::Var(rhs) }) if *lhs == name && *rhs == entry_name)
        );
    }
    let latch_body = builder.lower_block_stmts(&pcode.blocks[2]).unwrap();
    assert!(latch_body.iter().any(|stmt| matches!(stmt, PreHirStmt::Assign { lhs: PreHirLValue::Var(lhs), .. } if *lhs == name)));
    if name != latch_name {
        assert!(
            matches!(latch_body.last(), Some(PreHirStmt::Assign { lhs: PreHirLValue::Var(lhs), rhs: PreHirExpr::Var(rhs) }) if *lhs == name && *rhs == latch_name)
        );
    }
    let read = builder
        .with_lowering_site(
            LoweringSite {
                block_idx: 1,
                op_idx: 0,
            },
            |b| b.lower_varnode(&carrier, &mut HashSet::default()),
        )
        .unwrap();
    assert_eq!(read, PreHirExpr::Var(name));
}

fn ssa_fixture_eval(expr: &PreHirExpr, values: &std::collections::BTreeMap<String, u64>) -> u64 {
    match expr {
        PreHirExpr::Var(name) => values[name],
        PreHirExpr::Const(value, _) => *value as u64,
        PreHirExpr::Cast {
            ty: NirType::Int { bits, .. },
            expr,
        } => {
            let value = ssa_fixture_eval(expr, values);
            if *bits == 64 {
                value
            } else {
                value & ((1u64 << bits) - 1)
            }
        }
        PreHirExpr::Binary { op, lhs, rhs, .. } => {
            let (left, right) = (ssa_fixture_eval(lhs, values), ssa_fixture_eval(rhs, values));
            match op {
                PreHirBinaryOp::And => left & right,
                PreHirBinaryOp::Or => left | right,
                PreHirBinaryOp::Shl => left << right,
                PreHirBinaryOp::Shr => left >> right,
                PreHirBinaryOp::Add => left.wrapping_add(right),
                PreHirBinaryOp::Eq => u64::from(left == right),
                PreHirBinaryOp::Lt => u64::from(left < right),
                _ => panic!("unexpected fixture operation {op:?}"),
            }
        }
        _ => panic!("unexpected fixture expression {expr:?}"),
    }
}

fn ssa_fixture_execute(body: &[PreHirStmt], values: &mut std::collections::BTreeMap<String, u64>) {
    for stmt in body {
        match stmt {
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(lhs),
                rhs,
            } => {
                let value = ssa_fixture_eval(rhs, values);
                values.insert(lhs.clone(), value);
            }
            PreHirStmt::Block(body) => ssa_fixture_execute(body, values),
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                let taken = ssa_fixture_eval(cond, values) != 0;
                ssa_fixture_execute(if taken { then_body } else { else_body }, values);
            }
            _ => panic!("unexpected fixture statement {stmt:?}"),
        }
    }
}

#[test]
fn ssa_conditional_edge_initializes_skipped_redefinition_from_immutable_formal() {
    let carrier = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x08, 8);
    let returned = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 8);
    let condition = crate::midend::builder::materialize::test_support::varnode(0x900);
    let result = crate::midend::builder::materialize::test_support::varnode(0x908);
    let mut entry = block_at(
        0x1000,
        0,
        vec![
            op(
                0,
                PcodeOpcode::IntEqual,
                Some(condition.clone()),
                vec![carrier.clone(), constant(0)],
            ),
            op(
                1,
                PcodeOpcode::CBranch,
                None,
                vec![constant(0x3000), condition],
            ),
        ],
    );
    entry.successors = vec![1, 2];
    let mut redefine = block_at(
        0x2000,
        1,
        vec![
            op(
                2,
                PcodeOpcode::Copy,
                Some(carrier.clone()),
                vec![constant(0x1234_5678_1111_2222)],
            ),
            op(3, PcodeOpcode::Branch, None, vec![constant(0x3000)]),
        ],
    );
    redefine.successors = vec![2];
    let join = block_at(
        0x3000,
        2,
        vec![
            op(
                4,
                PcodeOpcode::IntAdd,
                Some(result.clone()),
                vec![carrier.clone(), constant(1)],
            ),
            // Real RETURN recovery reads the ABI return slot implicitly.
            // Writing only a unique result leaves that slot indeterminate.
            op(5, PcodeOpcode::Copy, Some(returned.clone()), vec![result]),
            op(6, PcodeOpcode::Return, None, vec![returned]),
        ],
    );
    let pcode = pcode_function(vec![entry, redefine, join]);
    let options = crate::midend::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.prepare_ssa_emission();
    let phi = builder.scalar_ssa.phis[&2]
        .iter()
        .find(|phi| phi.storage.offset == carrier.offset)
        .unwrap();
    let carrier_name = builder.ssa_emission.bindings[&phi.output].clone();
    let input = builder.scalar_ssa.inputs[&phi.storage];
    let formal = builder.ssa_emission.bindings[&input].clone();
    assert_ne!(carrier_name, formal);
    let entry_body = builder.lower_block_stmts(&pcode.blocks[0]).unwrap();
    let redefine_body = builder.lower_block_stmts(&pcode.blocks[1]).unwrap();
    let LoweredTerminator::Cond { cond, .. } = builder.lower_block_terminator(0).unwrap() else {
        panic!("condition");
    };
    let PreHirExpr::Var(snapshot) = cond else {
        panic!("captured decision");
    };
    for seed in [0, 1, 0xfeed_abcd_0000_0001] {
        let mut values = std::collections::BTreeMap::from([(formal.clone(), seed)]);
        ssa_fixture_execute(&entry_body, &mut values);
        if values[&snapshot] == 0 {
            ssa_fixture_execute(&redefine_body, &mut values);
        }
        assert_eq!(values[&formal], seed, "entry identity is immutable");
        assert_eq!(
            values[&carrier_name],
            if seed == 0 {
                seed
            } else {
                0x1234_5678_1111_2222
            }
        );
    }
    // A named aggregate's field provenance currently belongs to the ABI
    // binding. Do not apply a partial identity plan that loses that layout.
    let mut context = PreviewTypeContext::default();
    let mut hints = crate::midend::ir::NirFunctionHints::default();
    hints.param_type_names.insert(0, "Node*".into());
    context.function_hints = Some(hints);
    context.struct_types.insert(
        "Node".into(),
        crate::midend::ir::NirStructTypeHint {
            name: "Node".into(),
            size: 16,
            fields: Vec::new(),
        },
    );
    let mut declined = PreviewBuilder::new(&pcode, &options, Some(&context));
    declined.prepare_ssa_emission();
    assert!(declined.ssa_emission.bindings.is_empty());
}

#[test]
fn ssa_linear_input_redefinition_keeps_scalar_formal_and_wide_value_distinct() {
    let wide = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x08, 8);
    let narrow = Varnode {
        size: 4,
        ..wide.clone()
    };
    let saved = crate::midend::builder::materialize::test_support::varnode(0x900);
    let combined = crate::midend::builder::materialize::test_support::varnode(0x908);
    let seed = 0xfeed_abcd_1111_2222u64;
    let pcode = pcode_function(vec![block_at(
        0x1000,
        0,
        vec![
            op(0, PcodeOpcode::IntZExt, Some(saved.clone()), vec![narrow]),
            op(
                1,
                PcodeOpcode::Copy,
                Some(wide.clone()),
                vec![constant(seed as i64)],
            ),
            op(
                2,
                PcodeOpcode::IntAdd,
                Some(combined.clone()),
                vec![wide.clone(), saved],
            ),
            op(3, PcodeOpcode::Return, None, vec![combined]),
        ],
    )]);
    let options = crate::midend::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.prepare_ssa_emission();
    assert!(builder.scalar_ssa.phis.is_empty());
    assert_eq!(builder.ssa_emission.storages.len(), 2);
    let formal = builder.params[&0].name.clone();
    assert!(matches!(
        builder.params[&0].ty,
        NirType::Int { bits: 32, .. }
    ));
    let body = builder.lower_block_stmts(&pcode.blocks[0]).unwrap();
    let mut values = std::collections::BTreeMap::from([(formal.clone(), 7)]);
    ssa_fixture_execute(&body, &mut values);
    assert_eq!(values[&formal], 7);
    let read = builder
        .with_lowering_site(
            LoweringSite {
                block_idx: 0,
                op_idx: 2,
            },
            |builder| builder.lower_varnode(&wide, &mut HashSet::default()),
        )
        .unwrap();
    assert_eq!(
        ssa_fixture_eval(&read, &values),
        seed,
        "wide successor keeps its high bits"
    );
}

#[test]
fn ssa_lowering_probe_does_not_publish_unselected_parameter_bindings() {
    let selected = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x08, 8);
    let other = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x10, 8);
    let saved = crate::midend::builder::materialize::test_support::varnode(0x900);
    let result = crate::midend::builder::materialize::test_support::varnode(0x908);
    let pcode = pcode_function(vec![block_at(
        0x1000,
        0,
        vec![
            op(
                0,
                PcodeOpcode::IntAdd,
                Some(saved),
                vec![selected.clone(), constant(1)],
            ),
            op(
                1,
                PcodeOpcode::Copy,
                Some(selected.clone()),
                vec![constant(8)],
            ),
            op(
                2,
                PcodeOpcode::IntAdd,
                Some(result.clone()),
                vec![other, selected],
            ),
            op(3, PcodeOpcode::Return, None, vec![result]),
        ],
    )]);
    let options = crate::midend::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.prepare_ssa_emission();
    assert!(!builder.ssa_emission.bindings.is_empty());
    assert!(builder.params.contains_key(&0));
    assert!(
        !builder.params.contains_key(&1),
        "observational preflight may not publish an unrelated binding"
    );
    builder.lower_block_stmts(&pcode.blocks[0]).unwrap();
    assert!(
        builder.params.contains_key(&1),
        "actual lowering still owns the real read"
    );
}

#[test]
fn ssa_edge_decision_reuses_original_site_load_materialization() {
    let carrier = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0x08, 8);
    let loaded = register(RUST_SLEIGH_REGISTER_SPACE_ID, 0, 8);
    let condition = crate::midend::builder::materialize::test_support::varnode(0x900);
    let result = crate::midend::builder::materialize::test_support::varnode(0x908);
    let mut entry = block_at(
        0x1000,
        0,
        vec![
            op(
                0,
                PcodeOpcode::Load,
                Some(loaded.clone()),
                vec![constant(0), carrier.clone()],
            ),
            op(
                1,
                PcodeOpcode::IntEqual,
                Some(condition.clone()),
                vec![loaded, constant(0)],
            ),
            op(
                2,
                PcodeOpcode::CBranch,
                None,
                vec![constant(0x3000), condition],
            ),
        ],
    );
    entry.successors = vec![1, 2];
    let mut redefine = block_at(
        0x2000,
        1,
        vec![
            op(
                3,
                PcodeOpcode::Copy,
                Some(carrier.clone()),
                vec![constant(8)],
            ),
            op(4, PcodeOpcode::Branch, None, vec![constant(0x3000)]),
        ],
    );
    redefine.successors = vec![2];
    let join = block_at(
        0x3000,
        2,
        vec![
            op(
                5,
                PcodeOpcode::IntAdd,
                Some(result.clone()),
                vec![carrier, constant(1)],
            ),
            op(6, PcodeOpcode::Return, None, vec![result]),
        ],
    );
    let pcode = pcode_function(vec![entry, redefine, join]);
    let options = crate::midend::builder::materialize::test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.prepare_ssa_emission();
    assert!(!builder.ssa_emission.bindings.is_empty());
    let body = builder.lower_block_stmts(&pcode.blocks[0]).unwrap();
    let terminator = builder.lower_block_terminator(0).unwrap();
    let tree = format!("{body:?} {terminator:?}");
    assert_eq!(
        tree.matches("Load {").count(),
        1,
        "one original memory read: {tree}"
    );
    assert!(matches!(
        terminator,
        LoweredTerminator::Cond {
            cond: PreHirExpr::Var(_),
            ..
        }
    ));
}

#[test]
fn ssa_partitioned_carrier_preserves_high_bits_across_zero_and_repeated_updates() {
    let wide = Varnode {
        space_id: REGISTER_SPACE_ID,
        offset: 0x38,
        size: 8,
        is_constant: false,
        constant_val: 0,
    };
    let narrow = Varnode {
        size: 4,
        ..wide.clone()
    };
    let seed = 0x1234_5678_1122_3344u64;
    let mut entry = block_at(
        0x1000,
        0,
        vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(wide.clone()),
                vec![constant(seed as i64)],
            ),
            op(1, PcodeOpcode::Branch, None, vec![constant(0x2000)]),
        ],
    );
    entry.successors = vec![1];
    let mut head = block_at(
        0x2000,
        1,
        vec![
            op(
                2,
                PcodeOpcode::Copy,
                Some(test_support::varnode(0x9000)),
                vec![wide.clone()],
            ),
            op(
                3,
                PcodeOpcode::CBranch,
                None,
                vec![constant(0x4000), constant(1)],
            ),
        ],
    );
    head.successors = vec![2, 3];
    let mut latch = block_at(
        0x3000,
        2,
        vec![
            op(
                4,
                PcodeOpcode::IntAdd,
                Some(narrow.clone()),
                vec![narrow, Varnode::constant(1, 4)],
            ),
            op(5, PcodeOpcode::Branch, None, vec![constant(0x2000)]),
        ],
    );
    latch.successors = vec![1];
    let exit = block_at(
        0x4000,
        3,
        vec![op(6, PcodeOpcode::Return, None, vec![wide.clone()])],
    );
    let pcode = pcode_function(vec![entry, head, latch, exit]);
    let options = test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    builder.prepare_ssa_emission();
    assert_eq!(builder.ssa_emission.storages.len(), 2);
    let entry_body = builder.lower_block_stmts(&pcode.blocks[0]).unwrap();
    let latch_body = builder.lower_block_stmts(&pcode.blocks[2]).unwrap();
    // Both contained views still denote the initial complete producer. A
    // later narrow definition must instead keep the ordinary piece join.
    let whole = builder
        .ssa_emitted_definition(0, 0)
        .expect("whole producer binding");
    for (offset, expected) in [(0, seed & 0xffff_ffff), (4, seed >> 32)] {
        let view = Varnode {
            offset: wide.offset + offset,
            size: 4,
            ..wide.clone()
        };
        let read = builder
            .with_lowering_site(
                LoweringSite {
                    block_idx: 0,
                    op_idx: 1,
                },
                |b| b.ssa_emitted_read(&view),
            )
            .expect("contained snapshot view");
        // Only the original snapshot is available: reading a detached piece
        // binding would fail instead of silently reassembling another value.
        let values = std::collections::BTreeMap::from([(whole.clone(), seed)]);
        assert_eq!(ssa_fixture_eval(&read, &values), expected);
    }
    let read = builder
        .with_lowering_site(
            LoweringSite {
                block_idx: 1,
                op_idx: 0,
            },
            |builder| builder.ssa_emitted_read(&wide),
        )
        .expect("typed wide read");
    for iterations in [0, 1, 9] {
        let mut values = std::collections::BTreeMap::new();
        ssa_fixture_execute(&entry_body, &mut values);
        for _ in 0..iterations {
            ssa_fixture_execute(&latch_body, &mut values);
        }
        assert_eq!(ssa_fixture_eval(&read, &values), seed + iterations);
    }
}

#[test]
fn ssa_unused_phi_cycle_does_not_create_an_input_or_edge_move() {
    for offset in [0x38, 0x00] {
        let wide = register(REGISTER_SPACE_ID, offset, 8);
        let narrow = Varnode {
            size: 4,
            ..wide.clone()
        };
        let next = test_support::varnode(0x900);
        let mut entry = block_at(
            0x1000,
            0,
            vec![
                op(0, PcodeOpcode::Copy, Some(wide.clone()), vec![constant(7)]),
                op(1, PcodeOpcode::Branch, None, vec![constant(0x2000)]),
            ],
        );
        entry.successors = vec![1];
        let mut header = block_at(
            0x2000,
            1,
            vec![
                op(
                    2,
                    PcodeOpcode::Copy,
                    Some(next.clone()),
                    vec![narrow.clone()],
                ),
                op(
                    3,
                    PcodeOpcode::CBranch,
                    None,
                    vec![constant(0x4000), constant(1)],
                ),
            ],
        );
        header.successors = vec![2, 3];
        let mut latch = block_at(
            0x3000,
            2,
            vec![
                op(
                    4,
                    PcodeOpcode::IntAdd,
                    Some(narrow.clone()),
                    vec![narrow, Varnode::constant(1, 4)],
                ),
                op(
                    5,
                    PcodeOpcode::IntZExt,
                    Some(wide.clone()),
                    vec![Varnode {
                        size: 4,
                        ..wide.clone()
                    }],
                ),
                op(6, PcodeOpcode::Branch, None, vec![constant(0x2000)]),
            ],
        );
        latch.successors = vec![1];
        let exit = block_at(
            0x4000,
            3,
            vec![op(7, PcodeOpcode::Return, None, vec![next])],
        );
        let pcode = pcode_function(vec![entry, header, latch, exit]);
        let options = test_support::test_options();
        let mut builder = PreviewBuilder::new(&pcode, &options, None);
        builder.prepare_ssa_emission();
        assert!(!builder.ssa_emission.bindings.is_empty());
        let unused = builder.scalar_ssa.phis[&1]
            .iter()
            .find(|phi| phi.storage.offset == offset + 4)
            .expect("upper-piece phi");
        // A complete return register may be consumed implicitly. Its upper
        // piece is preserved despite the absence of an explicit operand.
        let implicit_return = builder.register_namer().is_primary_return_register(&wide);
        assert_eq!(
            builder.ssa_emission.bindings.contains_key(&unused.output),
            implicit_return
        );
        assert!(
            builder.params.is_empty(),
            "unused loop transport is not an ABI input"
        );
        assert_eq!(
            builder
                .ssa_emission
                .copies
                .values()
                .flatten()
                .any(|copy| copy.destination == unused.output),
            implicit_return
        );
        builder.lower_block_stmts(&pcode.blocks[0]).unwrap();
        builder.lower_block_stmts(&pcode.blocks[2]).unwrap();
    }
}

#[test]
fn rejected_ssa_emission_keeps_existing_bindings_unchanged() {
    let carrier = register(REGISTER_SPACE_ID, 0x38, 8);
    let mut entry = block_at(
        0x1000,
        0,
        vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(carrier.clone()),
                vec![constant(7)],
            ),
            op(1, PcodeOpcode::Branch, None, vec![constant(0x2000)]),
        ],
    );
    entry.successors = vec![1];
    let mut loop_block = block_at(
        0x2000,
        1,
        vec![
            op(
                2,
                PcodeOpcode::IntAdd,
                Some(carrier.clone()),
                vec![carrier.clone(), constant(1)],
            ),
            op(
                3,
                PcodeOpcode::CBranch,
                None,
                vec![constant(0x2000), carrier.clone()],
            ),
        ],
    );
    loop_block.successors = vec![1, 2];
    let exit = block_at(
        0x3000,
        2,
        vec![op(4, PcodeOpcode::Return, None, vec![carrier])],
    );
    let pcode = pcode_function(vec![entry, loop_block, exit]);
    let options = test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    let invalid = SsaValueId(builder.scalar_ssa.values.len() as u32 + 1);
    builder.scalar_ssa.out_of_ssa_copies[0].source = invalid;
    let before_temps = builder.temps.clone();
    let before_params = builder.params.clone();
    builder.prepare_ssa_emission();
    assert!(builder.ssa_emission.bindings.is_empty());
    assert_eq!(builder.temps, before_temps);
    assert_eq!(builder.params, before_params);
}

#[test]
fn ssa_multiple_backedges_keep_the_final_value_and_decline_unproven_clones() {
    let value = register(REGISTER_SPACE_ID, 0x38, 8);
    let mut entry = block_at(
        0x1000,
        0,
        vec![
            op(
                0,
                PcodeOpcode::Copy,
                Some(value.clone()),
                vec![constant(0x123456789abcdef0)],
            ),
            op(1, PcodeOpcode::Branch, None, vec![constant(0x2000)]),
        ],
    );
    entry.successors = vec![1];
    let mut header = block_at(
        0x2000,
        1,
        vec![
            op(
                2,
                PcodeOpcode::Copy,
                Some(test_support::varnode(0x9000)),
                vec![value.clone()],
            ),
            op(
                3,
                PcodeOpcode::CBranch,
                None,
                vec![constant(0x6000), constant(1)],
            ),
        ],
    );
    header.successors = vec![2, 5];
    let mut choose = block_at(
        0x3000,
        2,
        vec![op(
            4,
            PcodeOpcode::CBranch,
            None,
            vec![constant(0x4000), constant(1)],
        )],
    );
    choose.successors = vec![3, 4];
    let mut first = block_at(
        0x4000,
        3,
        vec![
            op(
                5,
                PcodeOpcode::IntAdd,
                Some(value.clone()),
                vec![value.clone(), constant(1)],
            ),
            op(6, PcodeOpcode::Branch, None, vec![constant(0x2000)]),
        ],
    );
    first.successors = vec![1];
    let mut second = block_at(
        0x5000,
        4,
        vec![
            op(
                7,
                PcodeOpcode::IntAdd,
                Some(value.clone()),
                vec![value.clone(), constant(2)],
            ),
            op(8, PcodeOpcode::Branch, None, vec![constant(0x2000)]),
        ],
    );
    second.successors = vec![1];
    let exit = block_at(
        0x6000,
        5,
        vec![op(9, PcodeOpcode::Return, None, vec![value.clone()])],
    );
    let pcode = pcode_function(vec![entry, header, choose, first, second, exit]);
    let options = test_support::test_options();
    let mut builder = PreviewBuilder::new(&pcode, &options, None);
    let mut cloned = builder.clone();
    cloned.virtual_block_map.push(1);
    let previous_temps = cloned.temps.clone();
    let previous_params = cloned.params.clone();
    cloned.prepare_ssa_emission();
    assert!(cloned.ssa_emission.bindings.is_empty());
    assert_eq!(cloned.temps, previous_temps);
    assert_eq!(cloned.params, previous_params);
    builder.prepare_ssa_emission();
    assert!(builder.ssa_emission.storages.contains(&SsaStorageKey {
        space_id: value.space_id,
        offset: value.offset,
        size: value.size
    }));
    let entry = builder.lower_block_stmts(&pcode.blocks[0]).unwrap();
    let first = builder.lower_block_stmts(&pcode.blocks[3]).unwrap();
    let second = builder.lower_block_stmts(&pcode.blocks[4]).unwrap();
    let read = builder
        .with_lowering_site(
            LoweringSite {
                block_idx: 5,
                op_idx: 0,
            },
            |b| b.ssa_emitted_read(&value),
        )
        .expect("post-loop identity");
    for steps in [vec![], vec![1], vec![2], vec![1, 2, 2, 1]] {
        let mut values = std::collections::BTreeMap::new();
        ssa_fixture_execute(&entry, &mut values);
        for step in &steps {
            ssa_fixture_execute(if *step == 1 { &first } else { &second }, &mut values);
        }
        assert_eq!(
            ssa_fixture_eval(&read, &values),
            0x123456789abcdef0 + steps.iter().sum::<u64>()
        );
    }
}
