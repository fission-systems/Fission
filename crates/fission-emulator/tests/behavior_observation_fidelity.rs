//! Integration verification for issue #1269:
//! - Preserve syscall attempts for unmapped raw guest syscall numbers with guest ABI.
//! - Correlate structured completion outcomes, task IDs, event IDs, bounded/redacted arguments,
//!   and explicit `EffectScope` (`ModeledGuest` vs `None`).
//! - Guarantee failed, unsupported, blocked/interrupted, uncompleted, and HLE-miss events can
//!   never be promoted to successful behavior.

use std::path::PathBuf;

use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::observe::{
    BehaviorArgPolicy, BehaviorEvent, BehaviorLog, CallStatus, EffectScope, Observer, SyscallArg,
};
use fission_emulator::os::{HleResult, LinuxEnv, OsEnvironment};
use fission_emulator::{MachineState, SandboxMetricsReport};
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join(name)
}

fn load_linux_emulator(name: &str) -> Emulator {
    let path = fixture(name);
    let binary = LoadedBinary::from_file(&path).expect("load fixture");
    let mut state = MachineState::new();
    let image = fission_emulator::os::linux::loader::load_elf(&mut state, &binary)
        .expect("load fixture image");
    let load_spec = binary.load_spec().expect("load spec").clone();
    let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)
        .expect("Sleigh frontends")
        .into_iter()
        .next()
        .expect("a Sleigh frontend");
    let arch = ArchInfo::from_language_id(load_spec.pair.language_id.as_str(), Some(&binary))
        .expect("architecture");
    let mut emu = Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new()))
        .expect("emulator")
        .with_max_inst(Some(20_000));
    emu.set_host_stdout_echo(false);
    emu.apply_linux_image(image).expect("apply image");
    emu
}

fn dispatch_x64_syscall(
    env: &LinuxEnv,
    emu: &mut Emulator,
    number: u64,
    args: [u64; 6],
) -> HleResult {
    for (reg, val) in ["RDI", "RSI", "RDX", "R10", "R8", "R9"]
        .into_iter()
        .zip(args)
    {
        emu.write_register_u64(reg, val).expect("set syscall arg");
    }
    emu.write_register_u64("RAX", number)
        .expect("set syscall number");
    env.dispatch_hle(emu, "syscall").expect("dispatch syscall")
}

fn write_guest_bytes(emu: &mut Emulator, bytes: &[u8]) -> u64 {
    let addr = emu
        .heap_alloc(bytes.len().max(1) as u64)
        .expect("alloc guest buffer");
    let ram = emu.state.ram_space();
    emu.state
        .write_space(ram, addr, bytes)
        .expect("write guest buffer");
    addr
}

#[test]
fn failed_and_successful_vfs_file_operations_keep_distinct_outcomes_and_rendered_projection() {
    let env = LinuxEnv::new();
    let mut emu = load_linux_emulator("linux_x64_hello_sys.elf");
    emu.add_observer(Box::new(BehaviorLog::new()));
    emu.vfs.seed_path("/tmp/benign.txt", b"hello-vfs".to_vec());

    let missing_path = write_guest_bytes(&mut emu, b"/tmp/does_not_exist.txt\0");
    let existing_path = write_guest_bytes(&mut emu, b"/tmp/benign.txt\0");
    let read_buf = emu.heap_alloc(32).expect("read buffer");

    // 1. Failed openat -> Error (ENOENT = 2), EffectScope::None.
    let res = dispatch_x64_syscall(
        &env,
        &mut emu,
        257,
        [(-100i64) as u64, missing_path, 0, 0, 0, 0],
    );
    assert!(matches!(res, HleResult::Continue));

    // 2. Successful openat -> Succeeded (fd >= 3), EffectScope::ModeledGuest.
    let res = dispatch_x64_syscall(
        &env,
        &mut emu,
        257,
        [(-100i64) as u64, existing_path, 0, 0, 0, 0],
    );
    assert!(matches!(res, HleResult::Continue));
    let fd = emu.read_register_u64("RAX").expect("openat fd");
    assert!(fd >= 3, "expected guest VFS fd, got {fd}");

    // 3. Successful read & close.
    let res = dispatch_x64_syscall(&env, &mut emu, 0, [fd, read_buf, 16, 0, 0, 0]);
    assert!(matches!(res, HleResult::Continue));
    let res = dispatch_x64_syscall(&env, &mut emu, 3, [fd, 0, 0, 0, 0, 0]);
    assert!(matches!(res, HleResult::Continue));

    let observers = emu.take_observers();
    let log = observers[0]
        .as_any()
        .downcast_ref::<BehaviorLog>()
        .expect("BehaviorLog");
    assert_eq!(log.events.len(), 4);

    // Failed openat: preserved as attempt with Error outcome, never promoted to success.
    match &log.events[0] {
        BehaviorEvent::Syscall {
            guest_abi,
            raw_number,
            canonical_number,
            name,
            detail,
            outcome,
            ..
        } => {
            assert_eq!(*guest_abi, Some("linux_x86_64"));
            assert_eq!(*raw_number, 257);
            assert_eq!(*canonical_number, Some(257));
            assert_eq!(*name, Some("openat"));
            assert!(matches!(
                detail.get(1),
                Some(SyscallArg::Str { text, .. }) if text == "/tmp/does_not_exist.txt"
            ));
            let out = outcome.as_ref().expect("failed openat outcome");
            assert_eq!(out.status, CallStatus::Error);
            assert_eq!(out.errno, Some(2)); // ENOENT
            assert_eq!(out.effect_scope, EffectScope::None);
            assert!(!out.is_successful_effect());
        }
        other => panic!("expected Syscall, got {other:?}"),
    }
    assert!(
        log.events[0]
            .render()
            .starts_with("openat(-100, \"/tmp/does_not_exist.txt\""),
        "compatibility render mismatch: {}",
        log.events[0].render()
    );

    // Successful open/read/close: ModeledGuest, never Host.
    for idx in 1..4 {
        let out = log.events[idx].outcome().expect("completed syscall");
        assert_eq!(out.status, CallStatus::Succeeded);
        assert_eq!(out.effect_scope, EffectScope::ModeledGuest);
        assert!(out.is_successful_effect());
    }

    let report = SandboxMetricsReport::from_run(
        "fixture".to_string(),
        "ELF64".to_string(),
        false,
        emu.pc,
        emu.metrics.clone(),
        None,
    )
    .with_behavior(log, None);
    let json: serde_json::Value = serde_json::to_value(&report).expect("serialize behavior report");
    let events = json["behavior"]["events"].as_array().expect("events array");
    assert_eq!(events.len(), 4);
    assert_eq!(events[0]["outcome"]["status"], "error");
    assert_eq!(events[0]["outcome"]["effect_scope"], "none");
    assert_eq!(events[0]["outcome"]["errno"], 2);
    assert_eq!(events[1]["outcome"]["status"], "succeeded");
    assert_eq!(events[1]["outcome"]["effect_scope"], "modeled_guest");
    assert_eq!(events[1]["args"][1]["kind"], "str");
}

#[test]
fn unmapped_raw_and_unimplemented_syscalls_emit_attempts_without_faking_success() {
    let env = LinuxEnv::new();

    // 1. i386 unmapped raw syscall number (x86_32_to_x86_64 returns None for e.g. 999).
    let mut i386_emu = load_linux_emulator("linux_x86_32_abi.elf");
    i386_emu.add_observer(Box::new(BehaviorLog::new()));
    for (reg, val) in [
        ("EBX", 0x11u64),
        ("ECX", 0x22),
        ("EDX", 0x33),
        ("ESI", 0x44),
        ("EDI", 0x55),
        ("EBP", 0x66),
    ] {
        i386_emu.write_register_u64(reg, val).expect("i386 arg");
    }
    i386_emu
        .write_register_u64("EAX", 999)
        .expect("raw i386 syscall");
    let res = env
        .dispatch_hle(&mut i386_emu, "syscall")
        .expect("dispatch unmapped i386 syscall");
    assert!(matches!(res, HleResult::Continue));
    assert_eq!(i386_emu.metrics.guest_syscalls.get(&999), Some(&1));
    assert_eq!(i386_emu.metrics.unknown_syscalls.get(&999), Some(&1));

    let i386_obs = i386_emu.take_observers();
    let i386_log = i386_obs[0]
        .as_any()
        .downcast_ref::<BehaviorLog>()
        .expect("log");
    assert_eq!(
        i386_log.events.len(),
        1,
        "unmapped raw syscall must emit an attempt event instead of silently dropping"
    );
    match &i386_log.events[0] {
        BehaviorEvent::Syscall {
            guest_abi,
            raw_number,
            canonical_number,
            name,
            args,
            outcome,
            ..
        } => {
            assert_eq!(*guest_abi, Some("linux_i386"));
            assert_eq!(*raw_number, 999);
            assert_eq!(*canonical_number, None);
            assert_eq!(*name, None);
            assert_eq!(*args, [0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
            let out = outcome.as_ref().expect("outcome");
            assert_eq!(out.status, CallStatus::Unsupported);
            assert_eq!(out.errno, Some(38)); // ENOSYS
            assert_eq!(out.effect_scope, EffectScope::None);
            assert!(!out.is_successful_effect());
        }
        other => panic!("expected Syscall, got {other:?}"),
    }
    assert!(i386_log.events[0].render().starts_with("syscall_999("));

    // 2. x86-64 mapped-to-self but unregistered canonical syscall (e.g. 998).
    let mut x64_emu = load_linux_emulator("linux_x64_hello_sys.elf");
    x64_emu.add_observer(Box::new(BehaviorLog::new()));
    let _ = dispatch_x64_syscall(&env, &mut x64_emu, 998, [1, 2, 3, 4, 5, 6]);
    assert_eq!(x64_emu.metrics.unknown_syscalls.get(&998), Some(&1));
    let x64_obs = x64_emu.take_observers();
    let x64_log = x64_obs[0]
        .as_any()
        .downcast_ref::<BehaviorLog>()
        .expect("log");
    assert_eq!(x64_log.events.len(), 1);
    let out = x64_log.events[0].outcome().expect("outcome");
    assert_eq!(out.status, CallStatus::Unsupported);
    assert_eq!(out.errno, Some(38));
    assert_eq!(out.effect_scope, EffectScope::None);
    assert!(!out.is_successful_effect());
}

#[test]
fn blocked_and_resumed_futex_and_exit_preserve_task_correlated_outcomes() {
    let mut emu = load_linux_emulator("linux_guest_futex.elf");
    emu.add_observer(Box::new(BehaviorLog::new()));
    emu.run().expect("run futex guest");

    assert!(emu.halt_requested);
    assert_eq!(emu.exit_code, Some(0));

    let observers = emu.take_observers();
    let log = observers[0]
        .as_any()
        .downcast_ref::<BehaviorLog>()
        .expect("log");

    let futex_events: Vec<_> = log
        .events
        .iter()
        .filter(|e| {
            matches!(
                e,
                BehaviorEvent::Syscall {
                    name: Some("futex"),
                    ..
                }
            )
        })
        .collect();
    assert_eq!(
        futex_events.len(),
        2,
        "expected wait + wake futex events, got {:?}",
        log.events
    );

    // At least one futex event waited/resumed and one succeeded; neither is HostEffect.
    assert!(futex_events.iter().all(|e| {
        let out = e.outcome().expect("futex outcome");
        out.effect_scope == EffectScope::ModeledGuest && out.is_successful_effect()
    }));
    assert!(
        futex_events
            .iter()
            .any(|e| matches!(e.outcome(), Some(out) if out.status == CallStatus::Resumed)),
        "expected resumed futex waiter outcome correlated across scheduler switch: {futex_events:?}"
    );
    assert!(
        log.events
            .iter()
            .any(|e| matches!(e.outcome(), Some(out) if out.status == CallStatus::Halted)),
        "expected thread exit halted outcome"
    );

    // Multiple distinct guest task_ids observed across clone -> child -> parent.
    let task_ids: std::collections::BTreeSet<u64> =
        log.events.iter().map(|e| e.task_id()).collect();
    assert!(
        task_ids.len() >= 2,
        "expected at least 2 task_ids across cloned tasks, got {task_ids:?}"
    );
}

#[test]
fn hle_supported_calls_and_hle_misses_are_distinguished_on_run_and_jit_traps() {
    // 1. Supported dynamic AArch64 `puts` via HLE trap -> Succeeded / ModeledGuest.
    let mut a64_emu = load_linux_emulator("aarch64_dyn_import.elf");
    a64_emu.add_observer(Box::new(BehaviorLog::new()));
    a64_emu.run().expect("run aarch64 dyn puts");
    let a64_obs = a64_emu.take_observers();
    let a64_log = a64_obs[0]
        .as_any()
        .downcast_ref::<BehaviorLog>()
        .expect("log");
    let puts_event = a64_log
        .events
        .iter()
        .find(|e| matches!(e, BehaviorEvent::Hle { name, .. } if name == "puts"))
        .expect("HLE puts event");
    let puts_outcome = puts_event.outcome().expect("puts outcome");
    assert_eq!(puts_outcome.status, CallStatus::Succeeded);
    assert_eq!(puts_outcome.effect_scope, EffectScope::ModeledGuest);
    assert!(puts_outcome.is_successful_effect());

    // 2. Unimplemented HLE procedure -> HleMiss / EffectScope::None, never Succeeded.
    let mut miss_emu = load_linux_emulator("x64_dyn_puts.elf");
    miss_emu.add_observer(Box::new(BehaviorLog::new()));
    // Patch the first IAT symbol name to an unimplemented libc stub and set PC to its magic slot
    // via a deterministic return address that stops at an exit stub.
    let first_got = *miss_emu
        .binary
        .inner()
        .iat_symbols
        .keys()
        .min()
        .expect("iat slot");
    miss_emu
        .binary
        .inner_mut()
        .iat_symbols
        .insert(first_got, "totally_unimplemented_mal_api".to_string());

    let ram = miss_emu.state.ram_space();
    let magic_bytes = miss_emu
        .state
        .read_space(ram, first_got, 8)
        .expect("read patched GOT trampoline");
    let magic = u64::from_le_bytes(magic_bytes.try_into().expect("8-byte GOT slot"));
    let sp = miss_emu.read_register_u64("RSP").expect("rsp") - 8;
    let stop_pc = 0x401000u64;
    miss_emu
        .state
        .write_space(ram, sp, &stop_pc.to_le_bytes())
        .expect("push return address");
    miss_emu.write_register_u64("RSP", sp).expect("set rsp");
    miss_emu.pc = magic;

    let outcome = miss_emu
        .run_until_pc(stop_pc)
        .expect("run through unimplemented HLE miss");
    assert_eq!(outcome, fission_emulator::RunOutcome::Returned);
    assert_eq!(
        miss_emu
            .metrics
            .hle_misses
            .get("totally_unimplemented_mal_api"),
        Some(&1)
    );

    let miss_obs = miss_emu.take_observers();
    let miss_log = miss_obs[0]
        .as_any()
        .downcast_ref::<BehaviorLog>()
        .expect("log");
    let miss_event = miss_log
        .events
        .iter()
        .find(|e| {
            matches!(e, BehaviorEvent::Hle { name, .. } if name == "totally_unimplemented_mal_api")
        })
        .expect("HLE miss event");
    let miss_outcome = miss_event.outcome().expect("miss outcome");
    assert_eq!(miss_outcome.status, CallStatus::HleMiss);
    assert_eq!(miss_outcome.effect_scope, EffectScope::None);
    assert!(!miss_outcome.is_successful_effect());
    assert_eq!(miss_event.render(), "totally_unimplemented_mal_api()");
}

#[test]
fn log_cap_omitted_completion_and_redaction_policy_are_explicit() {
    // 1. Log cap preserves dropped and dropped_completions counters cleanly.
    let capped = BehaviorLog::with_limit(1);
    let env = LinuxEnv::new();
    let mut emu = load_linux_emulator("linux_x64_hello_sys.elf");
    emu.add_observer(Box::new(capped));

    // Call 1 (recorded with completion), Call 2 (attempt + completion both dropped).
    let _ = dispatch_x64_syscall(&env, &mut emu, 39, [0; 6]); // getpid
    let _ = dispatch_x64_syscall(&env, &mut emu, 39, [0; 6]); // getpid (dropped)

    let obs = emu.take_observers();
    let log = obs[0].as_any().downcast_ref::<BehaviorLog>().expect("log");
    assert_eq!(log.events.len(), 1);
    assert_eq!(log.dropped, 1);
    assert_eq!(log.dropped_completions, 1);
    assert!(log.events[0].outcome().is_some());

    // 2. Redaction policy minimizes sensitive strings/buffers without losing metadata,
    // and omitted completions remain explicitly flagged in the serialized report.
    let mut redacted = BehaviorLog::with_policy(10, BehaviorArgPolicy::Redacted);
    redacted.on_syscall_attempt(&fission_emulator::observe::SyscallAttempt {
        event_id: 42,
        task_id: 1000,
        pc: 0x401234,
        guest_abi: "linux_x86_64",
        raw_number: 1,
        canonical_number: Some(1),
        name: Some("write"),
        args: [1, 0x5000, 11, 0, 0, 0],
        detail: vec![
            SyscallArg::Fd(1),
            SyscallArg::Buf {
                ptr: 0x5000,
                len: 11,
                preview: b"TOP_SECRET!".to_vec(),
            },
            SyscallArg::Str {
                ptr: 0x6000,
                text: "/etc/shadow".to_string(),
                truncated: false,
            },
        ],
    });
    // Intentionally omit `on_syscall_complete` for event_id 42.

    let report = SandboxMetricsReport::from_run(
        "fixture".to_string(),
        "ELF64".to_string(),
        false,
        0x401234,
        Default::default(),
        None,
    )
    .with_behavior(&redacted, None);
    let json_text = report.to_json_pretty().expect("serialize");
    assert!(
        !json_text.contains("TOP_SECRET!") && !json_text.contains("/etc/shadow"),
        "sensitive payloads must not leak under Redacted policy:\n{json_text}"
    );
    let json: serde_json::Value = serde_json::from_str(&json_text).expect("parse");
    assert_eq!(json["behavior"]["arg_policy"], "redacted");
    assert_eq!(json["behavior"]["events"][0]["completion_omitted"], true);
    assert!(json["behavior"]["events"][0]["outcome"].is_null());
    assert_eq!(
        json["behavior"]["events"][0]["args"][1]["kind"],
        "redacted_buf"
    );
    assert_eq!(json["behavior"]["events"][0]["args"][1]["value"]["len"], 11);
    assert_eq!(
        json["behavior"]["events"][0]["args"][2]["kind"],
        "redacted_str"
    );
    assert_eq!(
        json["behavior"]["events"][0]["args"][2]["value"]["byte_len"],
        11
    );
    assert_eq!(
        json["behavior"]["dropped_completions"],
        serde_json::Value::Null
    );
}
