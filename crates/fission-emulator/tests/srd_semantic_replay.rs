//! Semantic Replay Diff (SRD): dual-policy fixture capture + structured delta.
//!
//! Runs the same CRT binary under two instruction budgets, captures owner-native
//! snapshots (stop_pc / syscalls / optional mallocng probes), and asserts the
//! delta is non-identical with layered owner classification.

use std::path::PathBuf;

use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::os::LinuxEnv;
use fission_emulator::{
    CaptureOpts, OwnerLayer, SemanticReplayDelta, SemanticReplaySnapshot, MachineState,
};
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

fn run_budgeted(max_inst: u64) -> Emulator {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_static_printf_malloc.elf");
    assert!(path.is_file(), "missing fixture {}", path.display());

    let binary = LoadedBinary::from_file(&path).expect("load");
    let mut state = MachineState::new();
    let info = fission_emulator::os::linux::loader::load_elf(&mut state, &binary).expect("elf");
    let load_spec = binary.load_spec().expect("spec").clone();
    let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)
        .expect("fe")
        .into_iter()
        .next()
        .expect("sleigh");
    let lang_id = load_spec.pair.language_id.as_str();
    let arch = ArchInfo::from_language_id(lang_id, Some(&binary)).expect("arch");
    let mut emu = Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new()))
        .expect("emu")
        .with_max_inst(Some(max_inst));
    emu.apply_linux_image(info).expect("image");
    emu.run().expect("run");
    emu
}

fn capture(label: &str, max_inst: u64) -> SemanticReplaySnapshot {
    let mut emu = run_budgeted(max_inst);
    SemanticReplaySnapshot::capture(
        &mut emu,
        CaptureOpts {
            label: label.into(),
            binary: "x64_static_printf_malloc.elf".into(),
            probe_mallocng: true,
            ..Default::default()
        },
    )
}

/// Two different instruction budgets must produce a non-identical SRD delta
/// with at least control-flow (inst_count / stop_pc) and a classified owner.
#[test]
fn srd_dual_budget_not_identical() {
    let left = capture("budget_512", 512);
    let right = capture("budget_2500", 2_500);

    assert!(left.inst_count > 0, "left run made no progress");
    assert!(
        right.inst_count >= left.inst_count,
        "higher budget should not retire fewer guest insts"
    );

    let delta = SemanticReplayDelta::diff(&left, &right);
    eprintln!("SRD summary: {}", delta.summary);
    for f in &delta.field_deltas {
        eprintln!("  {:?} {} : {} -> {}", f.owner, f.field, f.left, f.right);
    }

    assert!(
        !delta.identical,
        "expected dual-budget runs to differ; left stop=0x{:X} right stop=0x{:X}",
        left.stop_pc,
        right.stop_pc
    );
    assert!(
        !delta.owners_touched.is_empty(),
        "owners_touched should list at least one layer"
    );
    assert!(
        delta.field_deltas.iter().any(|f| {
            matches!(
                f.field.as_str(),
                "inst_count" | "stop_pc" | "pc" | "pcode_ops"
            ) || f.field.starts_with("syscalls")
                || f.field.starts_with("mallocng")
        }),
        "expected control-flow / syscall / mallocng fields in delta"
    );
    // Primary owner must be a real triage layer (never Mixed for empty; here non-empty).
    assert_ne!(
        delta.primary_owner,
        OwnerLayer::Mixed,
        "non-empty delta should pick a concrete primary owner"
    );

    // Round-trip JSON so CLI offline --srd-diff stays compatible.
    let json = left.to_json_pretty().expect("serialize left");
    let parsed = SemanticReplaySnapshot::from_json(&json).expect("parse left");
    assert_eq!(parsed.label, left.label);
    assert_eq!(parsed.stop_pc, left.stop_pc);
    assert_eq!(parsed.inst_count, left.inst_count);
    assert!(parsed.mallocng.is_some(), "mallocng probe should be present");
}

/// Same capture twice → identical SRD (stable serialization surface).
#[test]
fn srd_identical_self_diff() {
    let snap = capture("self", 512);
    let delta = SemanticReplayDelta::diff(&snap, &snap);
    assert!(delta.identical, "self-diff must be identical: {}", delta.summary);
    assert!(delta.field_deltas.is_empty());
    assert_eq!(delta.primary_owner, OwnerLayer::Mixed);
}
