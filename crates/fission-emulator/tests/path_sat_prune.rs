//! Path-SAT Unsat prune quality: contradictory constraints → unsat stash.

use std::path::PathBuf;

use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::os::LinuxEnv;
use fission_emulator::sym::state::SimState;
use fission_emulator::sym::SimulationManager;
use fission_emulator::MachineState;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;
use fission_solver::SymExpr;

fn mini_emu() -> Emulator {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/linux_x64_hello_sys.elf");
    let binary = LoadedBinary::from_file(&path).expect("load");
    let mut state = MachineState::new();
    let info = fission_emulator::os::linux::loader::load_elf(&mut state, &binary).expect("elf");
    let load_spec = binary.load_spec().expect("spec").clone();
    let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)
        .expect("fe")
        .into_iter()
        .next()
        .expect("sleigh");
    let arch = ArchInfo::from_language_id(load_spec.pair.language_id.as_str(), Some(&binary))
        .expect("arch");
    let mut emu =
        Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new())).expect("emu");
    emu.apply_linux_image(info).expect("image");
    emu
}

/// Direct solver: Const false is UNSAT.
#[test]
fn path_sat_const_false_is_unsat() {
    let mut emu = mini_emu();
    let false_c = SymExpr::Const { val: 0, size: 1 };
    // Assert false as a path constraint via solver.assert then check_sat
    emu.solver.assert(false_c.clone());
    let sat = emu.solver.satisfiable(&[]);
    // After asserting false, should be unsat (or at least not proven sat for free)
    // Note: empty extra with false in assertions
    assert!(!sat, "asserting false must be unsat");
}

/// Manager path_sat: contradictory Eq(1,0) style constraint is unsat.
#[test]
fn path_sat_contradiction_via_manager() {
    let emu = mini_emu();
    // Build a constraint that is constantly false: Eq(Const(1), Const(0)) folded?
    let one = SymExpr::new_const(1, 1);
    let zero = SymExpr::new_const(0, 1);
    let bad = SymExpr::new_eq(one, zero);
    // new_eq on consts should fold to Const 0
    assert_eq!(bad, SymExpr::Const { val: 0, size: 1 });

    let ms = emu.state.clone();
    let mut mgr = SimulationManager::new(emu);
    let ok = mgr
        .emu
        .solver
        .satisfiable_with_oracle(std::slice::from_ref(&bad), Some(&ms));
    assert!(!ok, "Const 0 constraint must be unsat");

    // Also: true constraint is sat
    let good = SymExpr::Const { val: 1, size: 1 };
    let ok2 = mgr
        .emu
        .solver
        .satisfiable_with_oracle(&[good], Some(&ms));
    assert!(ok2, "Const 1 constraint must be sat");
}

/// Two forks with opposite concrete constants: one sat one unsat if constraints are const.
#[test]
fn path_sat_prune_keeps_sat_drops_const_false() {
    let emu = mini_emu();
    let ms = emu.state.clone();
    let mut mgr = SimulationManager::new(emu);

    let sat_c = SymExpr::Const { val: 1, size: 1 };
    let unsat_c = SymExpr::Const { val: 0, size: 1 };

    let base = SimState::new(0, 0x400000, ms.clone());
    let s_ok = base.with_constraint(sat_c, 1, 0x400010, ms.clone());
    let s_bad = base.with_constraint(unsat_c, 1, 0x400020, ms);

    let ok = mgr
        .emu
        .solver
        .satisfiable_with_oracle(&s_ok.history.constraints, Some(&s_ok.machine_state));
    let bad = mgr
        .emu
        .solver
        .satisfiable_with_oracle(&s_bad.history.constraints, Some(&s_bad.machine_state));
    assert!(ok);
    assert!(!bad);

    // Simulate prune classification
    let mut active = 0;
    let mut unsat = 0;
    for st in [s_ok, s_bad] {
        if mgr
            .emu
            .solver
            .satisfiable_with_oracle(&st.history.constraints, Some(&st.machine_state))
        {
            active += 1;
        } else {
            unsat += 1;
        }
    }
    assert_eq!(active, 1);
    assert_eq!(unsat, 1);
}

/// Free vars without contradictory constraints remain SAT (both forks kept).
#[test]
fn path_sat_free_vars_both_sat() {
    let mut emu = mini_emu();
    let x = SymExpr::new_var("x", 8);
    let zero = SymExpr::new_const(0, 8);
    // x == 0 is sat; x != 0 is also sat
    let eq0 = SymExpr::new_eq(x.clone(), zero.clone());
    let ne0 = SymExpr::new_neq(x, zero);
    let ms = emu.state.clone();
    assert!(emu.solver.satisfiable_with_oracle(&[eq0], Some(&ms)));
    assert!(emu.solver.satisfiable_with_oracle(&[ne0], Some(&ms)));
}
