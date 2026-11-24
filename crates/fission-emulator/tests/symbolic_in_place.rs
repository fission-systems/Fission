//! Symbolic expressions survive the interpreter's in-place ops and stores.
//!
//! The interpreter runs each p-code op concretely first and builds its
//! symbolic expression after. A concrete write clears the shadow it covers,
//! so an op whose output overlaps an input -- `ZEXT RAX <- EAX`, which
//! every x86-64 32-bit register write lifts to -- read its own cleared
//! output and dropped the expression. `movzx eax, byte [rsp]; cmp eax,
//! 'A'; jne` therefore recorded no symbolic branch at all, though the
//! branch depends on the stdin byte. Found by CSIC's joint search (PR
//! #1106), which asks the solver about exactly these branches.
//!
//! A second defect in the same path: the interpreter called the shadow
//! `STORE` callback with its arguments in the wrong order (the pointer's
//! varnode where the value's belongs, sizes swapped), so a value stored
//! through a computed address -- every spill to the stack -- lost its
//! expression.
//!
//! The programs run after the fixture's `read(0, rsp, 1)` of one stdin byte
//! (`'A'`), so `[rsp]` is the symbolic source. Built from this repository's
//! own fixture, never a corpus binary.

use std::path::PathBuf;

use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::observe::ShadowMode;
use fission_emulator::os::LinuxEnv;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;
use fission_solver::SymExpr;

/// Where the inserted code starts in the fixture's file.
const INSERT_AT: usize = 0x96;

fn build(code: &[u8]) -> (Emulator, u64) {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_concolic_branch_sys.elf");
    let mut bytes = std::fs::read(&path).expect("read fixture");
    // Right after the fixture's `read` syscall; see `taint_flow.rs`.
    bytes.splice(INSERT_AT..INSERT_AT, code.iter().copied());
    for field in [0x60, 0x68] {
        let old = u64::from_le_bytes(bytes[field..field + 8].try_into().unwrap());
        bytes[field..field + 8].copy_from_slice(&(old + code.len() as u64).to_le_bytes());
    }
    let binary =
        LoadedBinary::from_bytes(bytes, format!("{}-symbolic-in-place", path.display())).unwrap();
    let mut state = MachineState::new();
    let info = fission_emulator::os::linux::loader::load_elf(&mut state, &binary).expect("elf");
    let load_spec = binary.load_spec().expect("spec").clone();
    let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)
        .expect("frontend")
        .into_iter()
        .next()
        .expect("sleigh");
    let arch = ArchInfo::from_language_id(load_spec.pair.language_id.as_str(), Some(&binary))
        .expect("arch");
    let entry = binary.entry_point;
    let mut emu = Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new()))
        .expect("emulator")
        .with_max_inst(Some(4096));
    emu.force_interpreter = true;
    emu.apply_linux_image(info).expect("image");
    emu.seed_stdin(b"A");
    emu.set_shadow_mode(ShadowMode::Symbolic);
    // The inserted code's address: the entry point's page offset maps the
    // file offset the same way for this single-segment fixture.
    let base = entry - (entry & 0xFFF);
    (emu, base + INSERT_AT as u64)
}

/// The symbolic branches the inserted code made: `(pc, condition)`.
fn branches(code: &[u8]) -> Vec<(u64, SymExpr)> {
    let (mut emu, start) = build(code);
    emu.run().expect("run");
    let end = start + code.len() as u64;
    emu.sym_events
        .iter()
        .filter(|e| e.pc >= start && e.pc < end)
        .filter_map(|e| {
            let node = e.condition_node?;
            Some((e.pc, emu.solver.nodes.get(&node)?.clone()))
        })
        .collect()
}

fn mentions_stdin(expr: &SymExpr) -> bool {
    let mut found = false;
    expr.visit_preorder(&mut |e| {
        if let SymExpr::Var { name, .. } = e {
            found |= name.starts_with("stdin_");
        }
    });
    found
}

/// Positive control: a compare straight against memory goes through a
/// temporary, no op reads what it writes, and the branch was symbolic
/// before the fix too.
#[test]
fn a_branch_on_a_byte_compared_in_memory_is_symbolic() {
    let code = [
        0x80, 0x3C, 0x24, 0x41, // cmp byte ptr [rsp], 'A'
        0x75, 0x00, // jne +0
    ];
    let found = branches(&code);
    assert_eq!(found.len(), 1, "one symbolic branch: {found:?}");
    assert!(mentions_stdin(&found[0].1), "{:?}", found[0].1);
}

/// The regression: the byte goes through `movzx eax`, whose lift ends in
/// the in-place `ZEXT RAX <- EAX`.
#[test]
fn a_branch_on_a_byte_loaded_into_a_32_bit_register_is_symbolic() {
    let code = [
        0x0F, 0xB6, 0x04, 0x24, // movzx eax, byte ptr [rsp]
        0x83, 0xF8, 0x41, // cmp eax, 'A'
        0x75, 0x00, // jne +0
    ];
    let found = branches(&code);
    assert_eq!(found.len(), 1, "one symbolic branch: {found:?}");
    assert!(mentions_stdin(&found[0].1), "{:?}", found[0].1);
}

/// An in-place arithmetic op keeps both its expression and the operand's
/// value from *before* the op: `add eax, 1` then compare.
#[test]
fn an_in_place_add_keeps_its_expression() {
    let code = [
        0x0F, 0xB6, 0x04, 0x24, // movzx eax, byte ptr [rsp]
        0x83, 0xC0, 0x01, // add eax, 1
        0x83, 0xF8, 0x42, // cmp eax, 'B'
        0x75, 0x00, // jne +0
    ];
    let found = branches(&code);
    assert_eq!(found.len(), 1, "one symbolic branch: {found:?}");
    let cond = &found[0].1;
    assert!(mentions_stdin(cond), "{cond:?}");
    // The branch was not taken ('A' + 1 == 'B'), so the condition -- the
    // `jne` predicate -- must be satisfiable by a different byte and
    // refuted by 'A'. The solver says which value it names.
    let mut solver = fission_solver::Solver::new();
    let mut var = None;
    cond.visit_preorder(&mut |e| {
        if let SymExpr::Var { id, .. } = e {
            var = Some((*id, e.clone()));
        }
    });
    let (id, expr) = var.expect("a variable");
    // A model reports the variables the solver knows by node.
    solver.nodes.insert(id, expr);
    let sat = solver
        .check_sat_with_oracle(None, std::slice::from_ref(cond))
        .expect("solve");
    assert_eq!(sat, fission_solver::SatResult::Sat, "{cond:?}");
    let value = solver.get_value(id).expect("model");
    assert_ne!(value as u8, b'A', "the model takes the other way");
}

/// The byte goes through a stack slot (`STORE` through `rsp + 8`, then a
/// load back), as every `-O0` local does.
#[test]
fn a_branch_on_a_byte_spilled_through_a_pointer_is_symbolic() {
    let code = [
        0x0F, 0xB6, 0x04, 0x24, // movzx eax, byte ptr [rsp]
        0x88, 0x44, 0x24, 0x08, // mov byte ptr [rsp+8], al
        0x0F, 0xB6, 0x4C, 0x24, 0x08, // movzx ecx, byte ptr [rsp+8]
        0x83, 0xF9, 0x41, // cmp ecx, 'A'
        0x75, 0x00, // jne +0
    ];
    let found = branches(&code);
    assert_eq!(found.len(), 1, "one symbolic branch: {found:?}");
    assert!(mentions_stdin(&found[0].1), "{:?}", found[0].1);
}
