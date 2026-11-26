//! The indirect-transfer hook: both engines report the same calls and jumps,
//! and a run nobody asks is compiled without it.
//!
//! `indirect_dispatch.c` is repository-owned and only ever run in this
//! emulator. `argc` selects which of two functions both dispatchers reach.

use std::path::PathBuf;

use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::observe::{IndirectKind, IndirectTransfer, ObserveMask, Observer};
use fission_emulator::os::LinuxEnv;
use fission_emulator::os::linux::ProcessArgs;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

#[derive(Default)]
struct Transfers(Vec<IndirectTransfer>);

impl Observer for Transfers {
    fn interest(&self) -> ObserveMask {
        ObserveMask::indirect()
    }

    fn on_indirect_transfer(&mut self, transfer: &IndirectTransfer) {
        self.0.push(transfer.clone());
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn run(fixture: &str, args: &[&str], interpret: bool) -> (LoadedBinary, Vec<IndirectTransfer>) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join(fixture);
    let binary = LoadedBinary::from_file(&path).expect("load");
    let mut argv = vec![fixture.to_string()];
    argv.extend(args.iter().map(|a| a.to_string()));
    let mut state = MachineState::new();
    let process = ProcessArgs {
        argv,
        ..ProcessArgs::default()
    };
    let info =
        fission_emulator::os::linux::loader::load_elf_with_args(&mut state, &binary, &process)
            .expect("elf");
    let load_spec = binary.load_spec().expect("spec").clone();
    let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)
        .expect("frontend")
        .into_iter()
        .next()
        .expect("sleigh");
    let arch = ArchInfo::from_language_id(load_spec.pair.language_id.as_str(), Some(&binary))
        .expect("arch");
    let mut emu = Emulator::new(
        state,
        binary.clone(),
        sleigh,
        arch,
        Box::new(LinuxEnv::new()),
    )
    .expect("emulator");
    emu.apply_linux_image(info).expect("image");
    emu.force_interpreter = interpret;
    emu.add_observer(Box::new(Transfers::default()));
    assert!(emu.observe_mask().indirect);
    assert!(!emu.observe_mask().insn && !emu.observe_mask().block);
    emu.run().expect("run");
    assert!(emu.exit_code.is_some(), "the guest exits");
    let transfers = emu.observers()[0]
        .as_any()
        .downcast_ref::<Transfers>()
        .expect("observer")
        .0
        .clone();
    (binary, transfers)
}

#[test]
fn both_engines_report_each_indirect_call_and_jump_with_its_target() {
    for fixture in ["indirect_dispatch_x64.elf", "indirect_dispatch_aarch64.elf"] {
        for (args, reached) in [(&[][..], "op_increment"), (&["x"][..], "op_triple")] {
            for interpret in [false, true] {
                let (binary, transfers) = run(fixture, args, interpret);
                let at = |name: &str| {
                    binary
                        .functions
                        .iter()
                        .find(|f| f.name == name)
                        .expect("symbol")
                        .address
                };
                let context = format!("{fixture} {args:?} interpret={interpret}");
                // Returns are indirect too and must not be reported.
                let kinds: Vec<_> = transfers.iter().map(|t| t.kind).collect();
                assert_eq!(kinds, [IndirectKind::Call, IndirectKind::Jump], "{context}");
                for (transfer, from) in transfers.iter().zip(["dispatch_call", "dispatch_jump"]) {
                    assert_eq!(transfer.target, at(reached), "{context}");
                    assert!(
                        transfer.site > at(from) && transfer.site < at(from) + 64,
                        "{context}: site 0x{:x} is not in {from}",
                        transfer.site
                    );
                    assert_eq!(
                        binary.view_bytes(transfer.site, transfer.site_bytes.len()),
                        Some(transfer.site_bytes.as_slice()),
                        "{context}"
                    );
                    assert!(!transfer.site_bytes.is_empty(), "{context}");
                    let seen = transfer.target_bytes.len();
                    assert!(seen > 0, "{context}");
                    let on_disk = binary.view_bytes(transfer.target, seen).expect("code");
                    assert_eq!(
                        on_disk,
                        &transfer.target_bytes[..on_disk.len()],
                        "{context}"
                    );
                }
            }
        }
    }
}
