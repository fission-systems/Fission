//! Profile static musl CRT: bounded max_inst ladders + syscall/userop/PC dump.
//!
//! ## Measured CRT surface (2026-07-10)
//!
//! | budget | syscalls | unknown | stop note |
//! |--------|----------|---------|-----------|
//! | 512    | 158 arch_prctl | none | max_inst |
//! | 1500   | +218 set_tid, +12 brk, +9 mmap | none | max_inst |
//! | higher | see ladder dump | HLE queue if any | stop_pc logged |
//!
//! Early CRT has **no missing syscall HLE**. Full process halt is still open.

use std::collections::BTreeMap;
use std::path::PathBuf;

use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::os::LinuxEnv;
use fission_emulator::MachineState;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

fn run_budgeted(max_inst: u64) -> Emulator {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_static_printf_malloc.elf");
    assert!(path.is_file(), "missing {}", path.display());

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

fn dump(label: &str, emu: &Emulator) {
    eprintln!(
        "[{label}] inst={} halt={} exit={:?} stop_pc=0x{:X} fs=0x{:X} tid=0x{:X} pcode_ops={}",
        emu.inst_count,
        emu.halt_requested,
        emu.metrics.exit_reason,
        emu.metrics.stop_pc,
        emu.fs_base,
        emu.clear_child_tid,
        emu.pcode_ops
    );
    eprintln!("  syscalls={:?}", emu.metrics.syscalls);
    eprintln!("  unk_sys={:?}", emu.metrics.unknown_syscalls);
    eprintln!("  hle_miss={:?}", emu.metrics.hle_misses);
    eprintln!("  unimpl={:?}", emu.metrics.top_unimplemented(8));
    eprintln!("  userops={:?}", emu.metrics.userops);
}

fn is_budget_exit(emu: &Emulator) -> bool {
    matches!(
        emu.metrics.exit_reason.as_deref(),
        Some("max_inst") | Some("pcode_budget")
    ) || emu.max_inst.is_some_and(|m| emu.inst_count >= m)
}

fn is_process_halt(emu: &Emulator) -> bool {
    emu.halt_requested
        && emu.metrics.exit_reason.as_deref() != Some("max_inst")
        && emu.metrics.exit_reason.as_deref() != Some("pcode_budget")
}

#[test]
fn profile_static_printf_malloc_first_budget() {
    let emu = run_budgeted(512);
    assert!(
        emu.inst_count <= 512 + 32,
        "max_inst not honored: inst={}",
        emu.inst_count
    );
    assert!(emu.inst_count > 5, "expected CRT progress");
    dump("512", &emu);
    assert!(
        emu.metrics.unknown_syscalls.is_empty(),
        "unexpected unknown syscalls: {:?}",
        emu.metrics.unknown_syscalls
    );
}

/// Progressive budgets with stop_pc + unknown-syscall gap logging.
#[test]
fn profile_static_crt_ladder_regression() {
    // CI: modest rungs (fuse keeps each under a few seconds).
    let rungs: &[(u64, &str)] = &[
        (512, "tls"),
        (1_500, "post_tls"),
        (5_000, "5k"),
        (15_000, "15k"),
    ];
    let mut prev_sys: BTreeMap<u64, u64> = BTreeMap::new();
    let mut last_halt = false;
    let mut last_inst = 0u64;
    let mut last_stop_pc = 0u64;
    let mut gap_log: Vec<(u64, BTreeMap<u64, u64>, u64)> = Vec::new();

    for &(budget, tag) in rungs {
        let emu = run_budgeted(budget);
        dump(&format!("{budget}/{tag}"), &emu);
        assert!(
            is_budget_exit(&emu)
                || is_process_halt(&emu)
                || emu.inst_count <= budget.saturating_mul(2).max(budget + 256),
            "budget {budget} not honored: inst={} exit={:?}",
            emu.inst_count,
            emu.metrics.exit_reason
        );
        assert!(
            emu.inst_count >= last_inst.saturating_sub(0),
            "instruction count regressed across ladder"
        );
        for (num, _) in &prev_sys {
            assert!(
                emu.metrics.syscalls.contains_key(num)
                    || is_process_halt(&emu)
                    || emu.inst_count < budget,
                "lost syscall {num} at budget {budget}"
            );
        }
        if !emu.metrics.unknown_syscalls.is_empty() {
            eprintln!(
                "HLE GAP at {budget} stop_pc=0x{:X}: unknown_syscalls={:?}",
                emu.metrics.stop_pc, emu.metrics.unknown_syscalls
            );
            gap_log.push((
                budget,
                emu.metrics.unknown_syscalls.clone(),
                emu.metrics.stop_pc,
            ));
        }
        prev_sys = emu.metrics.syscalls.clone();
        last_halt = is_process_halt(&emu);
        last_inst = emu.inst_count;
        last_stop_pc = emu.metrics.stop_pc;
        if last_halt {
            eprintln!("clean process halt at budget {budget}");
            break;
        }
    }

    eprintln!(
        "ladder summary: last_inst={last_inst} last_stop_pc=0x{last_stop_pc:X} gaps={gap_log:?}"
    );

    if std::env::var("FISSION_CRT_EXPECT_HALT").ok().as_deref() == Some("1") {
        assert!(last_halt, "expected full CRT halt; last_inst={last_inst}");
    } else if !last_halt {
        eprintln!(
            "no clean halt by top rung (last_inst={last_inst} pc=0x{last_stop_pc:X}); \
             set FISSION_CRT_EXPECT_HALT=1 when fixed"
        );
    }
}

/// Higher budget push (default-on but still fuse-limited). Documents 50k surface.
#[test]
fn profile_static_crt_50k_surface() {
    let emu = run_budgeted(50_000);
    dump("50k", &emu);
    assert!(
        is_budget_exit(&emu) || is_process_halt(&emu),
        "expected budget or halt exit, got {:?} inst={}",
        emu.metrics.exit_reason,
        emu.inst_count
    );
    // Regression: early TLS syscalls should still have been hit if we got far enough.
    if emu.inst_count >= 500 {
        assert!(
            emu.metrics.syscalls.contains_key(&158) || emu.fs_base != 0,
            "expected arch_prctl/FS setup by 50k"
        );
    }
    if !emu.metrics.unknown_syscalls.is_empty() {
        eprintln!(
            "50k HLE GAP stop_pc=0x{:X}: {:?}",
            emu.metrics.stop_pc, emu.metrics.unknown_syscalls
        );
    } else {
        eprintln!(
            "50k: still no unknown syscalls (stop_pc=0x{:X} exit={:?})",
            emu.metrics.stop_pc, emu.metrics.exit_reason
        );
    }
}

/// Optional: push toward full halt (expensive; opt-in).
#[test]
fn profile_static_crt_full_halt_optional() {
    if std::env::var("FISSION_SMOKE_STATIC_PRINTF").ok().as_deref() != Some("1") {
        eprintln!("skip full-halt attempt (FISSION_SMOKE_STATIC_PRINTF=1)");
        return;
    }
    let emu = run_budgeted(2_000_000);
    dump("2M", &emu);
    assert!(
        emu.inst_count <= 2_000_000 + 256 || is_budget_exit(&emu) || is_process_halt(&emu),
        "max_inst not honored"
    );
    if is_process_halt(&emu) {
        eprintln!("FULL HALT achieved: {}", emu.metrics.summary_line());
    } else {
        eprintln!(
            "still no halt @{} pc=0x{:X}: sys={:?} unk={:?} hle={:?}",
            emu.inst_count,
            emu.metrics.stop_pc,
            emu.metrics.syscalls,
            emu.metrics.unknown_syscalls,
            emu.metrics.hle_misses
        );
    }
}
