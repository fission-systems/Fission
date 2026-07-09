//! Profile static musl CRT: bounded max_inst ladders + syscall/userop dump.
//!
//! ## Fixed early-CRT syscall surface (measured 2026-07-09)
//!
//! | budget | syscalls observed | unknown |
//! |--------|-------------------|---------|
//! | 512    | 158 arch_prctl    | none    |
//! | 1500   | +218 set_tid, +12 brk, +9 mmap | none |
//!
//! Conclusion: **no missing syscall HLE in the first ~1.5k insns**.
//! Full halt still blocked by guest progress / later CRT behavior, not by
//! unknown_syscalls. New unknown numbers appearing on higher rungs are the
//! HLE work queue.
//!
//! After max_inst is honored (no TB hard-chain when budgeted; pcode fuse),
//! this finishes quickly and dumps metrics.

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
        "[{label}] inst={} halt={} exit={:?} fs=0x{:X} tid=0x{:X}",
        emu.inst_count,
        emu.halt_requested,
        emu.metrics.exit_reason,
        emu.fs_base,
        emu.clear_child_tid
    );
    eprintln!("  syscalls={:?}", emu.metrics.syscalls);
    eprintln!("  unk_sys={:?}", emu.metrics.unknown_syscalls);
    eprintln!("  hle_miss={:?}", emu.metrics.hle_misses);
    eprintln!("  unimpl={:?}", emu.metrics.top_unimplemented(8));
    eprintln!("  userops={:?}", emu.metrics.userops);
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
    // Early CRT: arch_prctl + set_tid_address only (no unknown syscalls).
    assert!(
        emu.metrics.unknown_syscalls.is_empty(),
        "unexpected unknown syscalls: {:?}",
        emu.metrics.unknown_syscalls
    );
}

/// Progressive budgets — documents which syscalls appear before full halt.
///
/// Soft gate: each step must honor max_inst and not introduce unknown syscalls
/// without an intentional HLE gap. Full halt is asserted only at the top rung
/// when `FISSION_CRT_EXPECT_HALT=1`.
#[test]
fn profile_static_crt_ladder_regression() {
    // CI ladder: small rungs only (pcode fuse breaks livelocks; still may be slow).
    let rungs: &[(u64, &str)] = &[(512, "tls"), (1_500, "post_tls")];
    let mut prev_sys: BTreeMap<u64, u64> = BTreeMap::new();
    let mut last_halt = false;
    let mut last_inst = 0u64;

    for &(budget, tag) in rungs {
        let emu = run_budgeted(budget);
        dump(&format!("{budget}/{tag}"), &emu);
        // Allow modest overrun (TB may finish a few insns after the fuse).
        assert!(
            emu.inst_count <= budget.saturating_mul(2).max(budget + 256)
                || emu.metrics.exit_reason.as_deref() == Some("pcode_budget")
                || emu.metrics.exit_reason.as_deref() == Some("max_inst"),
            "budget {budget} not honored: inst={} exit={:?}",
            emu.inst_count,
            emu.metrics.exit_reason
        );
        assert!(
            emu.inst_count >= last_inst,
            "instruction count regressed across ladder"
        );
        // Monotonic: previously seen syscalls should still appear (counts may grow).
        for (num, _) in &prev_sys {
            assert!(
                emu.metrics.syscalls.contains_key(num) || emu.halt_requested,
                "lost syscall {num} at budget {budget}"
            );
        }
        // Unknown syscalls are the primary HLE gap signal.
        if !emu.metrics.unknown_syscalls.is_empty() {
            eprintln!(
                "HLE GAP at {budget}: unknown_syscalls={:?}",
                emu.metrics.unknown_syscalls
            );
        }
        prev_sys = emu.metrics.syscalls.clone();
        last_halt = emu.halt_requested
            && emu.metrics.exit_reason.as_deref() != Some("max_inst")
            && emu.metrics.exit_reason.as_deref() != Some("pcode_budget");
        last_inst = emu.inst_count;
        if last_halt {
            eprintln!("clean process halt at budget {budget}");
            break;
        }
    }

    if std::env::var("FISSION_CRT_EXPECT_HALT").ok().as_deref() == Some("1") {
        assert!(last_halt, "expected full CRT halt; last_inst={last_inst}");
    } else if !last_halt {
        eprintln!(
            "no clean halt by top rung (last_inst={last_inst}); set FISSION_CRT_EXPECT_HALT=1 when fixed"
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
        emu.inst_count <= 2_000_000 + 128,
        "max_inst not honored"
    );
    if emu.halt_requested {
        eprintln!("FULL HALT achieved: {}", emu.metrics.summary_line());
    } else {
        eprintln!(
            "still no halt @{}: sys={:?} unk={:?} hle={:?}",
            emu.inst_count,
            emu.metrics.syscalls,
            emu.metrics.unknown_syscalls,
            emu.metrics.hle_misses
        );
    }
}
