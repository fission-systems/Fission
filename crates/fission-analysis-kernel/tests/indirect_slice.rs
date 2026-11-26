//! The first runtime-to-static slice (issue #1368 M1), on a repository-owned
//! guest executed only inside Fission's emulator.
//!
//! `indirect_dispatch.c` calls and tail-jumps through a table of
//! `op_increment` and `op_triple`; `argc` picks one, so a run observes one of
//! two feasible targets.

use fission_analysis_kernel::record::{IndirectRecorder, completeness, inputs_digest};
use fission_analysis_kernel::{ImageView, KernelError, import_trace, indirect_targets_view, store};
use fission_analysis_protocol::{
    IndirectSite, IndirectSiteKind, IndirectTargetsView, TargetAttribution, TargetSetCompleteness,
};
use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::os::LinuxEnv;
use fission_emulator::os::linux::ProcessArgs;
use fission_evidence::{
    EvidenceLedger, Lifecycle, Producer, RuntimeTrace, StatusChange, StatusEvent,
};
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;
use std::path::PathBuf;

const FIXTURES: [(&str, u32, u32); 2] = [
    ("indirect_dispatch_x64.elf", 39, 97),
    ("indirect_dispatch_aarch64.elf", 39, 97),
];

fn path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../fission-emulator/testdata")
        .join(name)
}

fn load(name: &str) -> LoadedBinary {
    LoadedBinary::from_file(path(name)).expect("load fixture")
}

struct Options {
    args: Vec<String>,
    interpret: bool,
    max_inst: Option<u64>,
    transfer_limit: Option<usize>,
}

impl Options {
    fn args(args: &[&str]) -> Self {
        Self {
            args: args.iter().map(|a| a.to_string()).collect(),
            interpret: false,
            max_inst: None,
            transfer_limit: None,
        }
    }
}

/// Run the fixture and return its trace and exit status.
fn capture(name: &str, options: Options) -> (RuntimeTrace, Option<u32>) {
    let binary = load(name);
    let mut argv = vec![name.to_string()];
    argv.extend(options.args.iter().cloned());
    let mut state = MachineState::new();
    let process = ProcessArgs {
        argv,
        ..ProcessArgs::default()
    };
    let info =
        fission_emulator::os::linux::loader::load_elf_with_args(&mut state, &binary, &process)
            .expect("map fixture");
    let load_spec = binary.load_spec().expect("load spec").clone();
    let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)
        .expect("frontends")
        .into_iter()
        .next()
        .expect("frontend");
    let arch = ArchInfo::from_language_id(load_spec.pair.language_id.as_str(), Some(&binary))
        .expect("arch");
    let mut emulator = Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new()))
        .expect("emulator")
        .with_max_inst(options.max_inst);
    emulator.apply_linux_image(info).expect("image");
    emulator.force_interpreter = options.interpret;
    let recorder = options
        .transfer_limit
        .map_or_else(IndirectRecorder::default, IndirectRecorder::with_limit);
    emulator.add_observer(Box::new(recorder));
    emulator.run().expect("run fixture");

    let recorder = emulator.observers()[0]
        .as_any()
        .downcast_ref::<IndirectRecorder>()
        .expect("recorder");
    let image = ImageView::new(&emulator.binary).expect("image view");
    let trace = recorder.trace(
        &image,
        inputs_digest(&options.args, None, options.max_inst),
        completeness(&emulator, None),
    );
    (trace, emulator.exit_code)
}

fn address(binary: &LoadedBinary, name: &str) -> u64 {
    binary
        .functions
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("fixture has no {name}"))
        .address
}

fn view(name: &str, function: &str, ledger: &EvidenceLedger) -> IndirectTargetsView {
    let binary = load(name);
    let image = ImageView::new(&binary).expect("image view");
    indirect_targets_view(&image, address(&binary, function), ledger).expect("view")
}

fn only_site(view: &IndirectTargetsView) -> &IndirectSite {
    assert_eq!(view.sites.len(), 1, "{view:#?}");
    &view.sites[0]
}

fn observed_names(site: &IndirectSite) -> Vec<&str> {
    site.observed_candidates
        .iter()
        .map(|c| c.name.as_deref().unwrap_or("?"))
        .collect()
}

fn import(name: &str, trace: &RuntimeTrace, ledger: &mut EvidenceLedger) -> (usize, usize) {
    let binary = load(name);
    let image = ImageView::new(&binary).expect("image view");
    let report = import_trace(&image, trace, ledger).expect("import");
    assert_eq!(report.unattributed_sites, 0);
    (report.added, report.duplicate)
}

#[test]
fn one_run_adds_the_observed_target_and_leaves_the_other_possible() {
    for (fixture, first_status, _) in FIXTURES {
        let (trace, status) = capture(fixture, Options::args(&[]));
        assert_eq!(status, Some(first_status), "{fixture}");
        assert!(trace.completeness.is_complete(), "{fixture}");
        assert_eq!(trace.transfers.len(), 2, "{fixture}: one call, one jump");

        let empty = view(fixture, "dispatch_call", &EvidenceLedger::default());
        let site = only_site(&empty);
        assert_eq!(site.kind, IndirectSiteKind::Call);
        assert!(site.in_static_lift);
        assert!(site.static_candidates.is_empty(), "{fixture}: {site:#?}");
        assert!(site.observed_candidates.is_empty());

        let mut ledger = EvidenceLedger::default();
        assert_eq!(import(fixture, &trace, &mut ledger), (2, 0));
        for (function, kind) in [
            ("dispatch_call", IndirectSiteKind::Call),
            ("dispatch_jump", IndirectSiteKind::Jump),
        ] {
            let view = view(fixture, function, &ledger);
            let site = only_site(&view);
            assert_eq!(site.kind, kind, "{fixture} {function}");
            assert!(site.in_static_lift, "{fixture} {function}");
            assert_eq!(site.target_set, TargetSetCompleteness::Open);
            assert_eq!(
                observed_names(site),
                ["op_increment"],
                "{fixture} {function}"
            );
            let candidate = &site.observed_candidates[0];
            assert_eq!(candidate.attribution, TargetAttribution::Image);

            // The provenance names who saw it, in which run, from which
            // record.
            let [observation] = candidate.observations.as_slice() else {
                panic!("expected one observation: {candidate:#?}");
            };
            assert_eq!(observation.source, "runtime_observation");
            assert_eq!(observation.verification, "unchecked");
            assert_eq!(observation.producer, "fission-emulator");
            let run = trace.run_id().0.to_string();
            assert_eq!(observation.run.as_deref(), Some(run.as_str()));
            assert!(
                observation.artifacts[0].starts_with(&format!("fission-runtime-trace-v1 {run}#")),
                "{observation:#?}"
            );
            assert!(view.excluded.is_empty(), "{view:#?}");
        }
    }
}

#[test]
fn a_second_run_adds_evidence_without_changing_the_first() {
    for (fixture, _, second_status) in FIXTURES {
        let (first, _) = capture(fixture, Options::args(&[]));
        let (second, status) = capture(fixture, Options::args(&["other"]));
        assert_eq!(status, Some(second_status), "{fixture}");
        assert_ne!(first.run_id(), second.run_id());

        let mut ledger = EvidenceLedger::default();
        import(fixture, &first, &mut ledger);
        let before: Vec<_> = ledger.claims().map(|(id, _)| id.clone()).collect();
        assert_eq!(import(fixture, &second, &mut ledger), (2, 0));
        assert_eq!(ledger.len(), 4);
        for id in &before {
            assert!(ledger.claims().any(|(held, _)| held == id));
        }
        for function in ["dispatch_call", "dispatch_jump"] {
            let view = view(fixture, function, &ledger);
            let site = only_site(&view);
            assert_eq!(
                observed_names(site),
                ["op_increment", "op_triple"],
                "{fixture} {function}"
            );
            // Both table entries have now been seen and the set stays open.
            assert_eq!(site.target_set, TargetSetCompleteness::Open);
        }

        // The order of import does not show in the result.
        let mut reversed = EvidenceLedger::default();
        import(fixture, &second, &mut reversed);
        import(fixture, &first, &mut reversed);
        assert_eq!(ledger.to_json().unwrap(), reversed.to_json().unwrap());
    }
}

#[test]
fn replay_and_reimport_are_deterministic() {
    for (fixture, ..) in FIXTURES {
        let (first, _) = capture(fixture, Options::args(&[]));
        let (replayed, _) = capture(fixture, Options::args(&[]));
        assert_eq!(first.to_json().unwrap(), replayed.to_json().unwrap());
        assert_eq!(first.run_id(), replayed.run_id());

        // Through the artifact on disk, as the CLI does it.
        let parsed = RuntimeTrace::from_json(&first.to_json().unwrap()).unwrap();
        assert_eq!(parsed, first);

        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("evidence.json");
        let mut ledger = store::load_or_empty(&file).unwrap();
        assert_eq!(import(fixture, &parsed, &mut ledger), (2, 0));
        store::save(&file, &ledger).unwrap();
        let written = std::fs::read(&file).unwrap();

        let mut reopened = store::load(&file).unwrap();
        assert_eq!(reopened, ledger);
        assert_eq!(import(fixture, &replayed, &mut reopened), (0, 2));
        store::save(&file, &reopened).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), written);
        assert_eq!(
            std::fs::read_dir(directory.path()).unwrap().count(),
            1,
            "no temporary file is left behind"
        );
    }
}

#[test]
fn the_interpreter_and_the_jit_record_the_same_trace() {
    for (fixture, ..) in FIXTURES {
        let (compiled, _) = capture(fixture, Options::args(&["other"]));
        let mut options = Options::args(&["other"]);
        options.interpret = true;
        let (interpreted, _) = capture(fixture, options);
        assert_eq!(compiled, interpreted, "{fixture}");
    }
}

#[test]
fn a_trace_of_another_image_is_refused_whole() {
    let (trace, _) = capture(FIXTURES[0].0, Options::args(&[]));

    // Another architecture, and the same program with one byte changed.
    let other = load(FIXTURES[1].0);
    let mut bytes = std::fs::read(path(FIXTURES[0].0)).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    let directory = tempfile::tempdir().unwrap();
    let patched_path = directory.path().join("patched.elf");
    std::fs::write(&patched_path, bytes).unwrap();
    let patched = LoadedBinary::from_file(&patched_path).unwrap();

    for binary in [&other, &patched] {
        let image = ImageView::new(binary).unwrap();
        let mut ledger = EvidenceLedger::default();
        assert!(matches!(
            import_trace(&image, &trace, &mut ledger),
            Err(KernelError::IncompatibleImage { .. })
        ));
        assert!(ledger.is_empty());
    }

    // Evidence already imported for the original does not move to the patched
    // file either.
    let mut ledger = EvidenceLedger::default();
    import(FIXTURES[0].0, &trace, &mut ledger);
    let image = ImageView::new(&patched).unwrap();
    let view = indirect_targets_view(&image, address(&patched, "dispatch_call"), &ledger).unwrap();
    assert!(only_site(&view).observed_candidates.is_empty());
    assert_eq!(view.other_image_claims, 2);
}

#[test]
fn code_that_differs_at_run_time_is_not_credited_to_the_file() {
    let fixture = FIXTURES[0].0;
    let (trace, _) = capture(fixture, Options::args(&[]));
    let call = trace
        .transfers
        .iter()
        .position(|t| t.kind == fission_evidence::TransferKind::IndirectCall)
        .unwrap();

    // The target was rewritten before it ran.
    let mut rewritten_target = trace.clone();
    rewritten_target.transfers[call].target_bytes.0[0] ^= 0xff;
    let mut ledger = EvidenceLedger::default();
    import(fixture, &rewritten_target, &mut ledger);
    let seen = view(fixture, "dispatch_call", &ledger);
    let candidate = &only_site(&seen).observed_candidates[0];
    assert_eq!(candidate.attribution, TargetAttribution::RuntimeCode);
    assert_eq!(candidate.name, None);

    // The call instruction itself was rewritten.
    let mut rewritten_site = trace.clone();
    rewritten_site.transfers[call].site_bytes.0[0] ^= 0xff;
    let mut ledger = EvidenceLedger::default();
    import(fixture, &rewritten_site, &mut ledger);
    let seen = view(fixture, "dispatch_call", &ledger);
    assert!(only_site(&seen).observed_candidates.is_empty());
    assert_eq!(seen.excluded.len(), 1);
    assert!(seen.excluded[0].reason.contains("modified at run time"));
}

#[test]
fn an_incomplete_capture_says_so_on_every_claim() {
    let fixture = FIXTURES[0].0;
    let mut options = Options::args(&[]);
    options.transfer_limit = Some(1);
    let (trace, _) = capture(fixture, options);
    assert_eq!(trace.completeness.dropped_transfers, 1);
    assert!(!trace.completeness.is_complete());

    let mut ledger = EvidenceLedger::default();
    assert_eq!(import(fixture, &trace, &mut ledger), (1, 0));
    assert!(ledger.claims().all(|(_, c)| {
        c.assumptions
            .iter()
            .any(|a| a.starts_with("capture incomplete"))
    }));

    // Stopped by the instruction budget before the guest exits.
    let mut options = Options::args(&[]);
    options.max_inst = Some(3);
    let (trace, status) = capture(fixture, options);
    assert_eq!(status, None);
    assert!(!trace.completeness.guest_finished);
}

#[test]
fn withdrawn_evidence_is_shown_as_excluded() {
    let fixture = FIXTURES[0].0;
    let (trace, _) = capture(fixture, Options::args(&[]));
    let mut ledger = EvidenceLedger::default();
    import(fixture, &trace, &mut ledger);
    let seen = view(fixture, "dispatch_call", &ledger);
    let withdrawn = seen.sites[0].observed_candidates[0].observations[0]
        .evidence_id
        .clone();
    let target = ledger
        .claims()
        .map(|(id, _)| id.clone())
        .find(|id| id.to_string() == withdrawn)
        .unwrap();
    ledger
        .record(StatusEvent {
            target,
            change: StatusChange::Lifecycle(Lifecycle::Withdrawn {
                reason: "recorder defect".into(),
            }),
            by: Producer {
                name: "analyst".into(),
                version: "1".into(),
            },
        })
        .unwrap();

    let seen = view(fixture, "dispatch_call", &ledger);
    assert!(only_site(&seen).observed_candidates.is_empty());
    assert_eq!(seen.excluded[0].reason, "withdrawn: recorder defect");
    assert_eq!(ledger.len(), 2, "the claim is kept");
}

#[test]
fn a_run_without_the_recorder_compiles_no_indirect_callback() {
    use fission_emulator::observe::{Coverage, Observer};
    assert!(!Coverage::new().interest().indirect);
    assert!(IndirectRecorder::default().interest().indirect);
}

/// ADR 0020: engines never depend on the kernel, and no canonical decompiler
/// crate depends on the dynamic backends.
#[test]
fn only_surfaces_depend_on_the_kernel() {
    let crates = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let surfaces = ["fission-cli", "fission-script", "fission-analysis-kernel"];
    for entry in std::fs::read_dir(&crates).unwrap() {
        let directory = entry.unwrap().path();
        let name = directory
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let Ok(manifest) = std::fs::read_to_string(directory.join("Cargo.toml")) else {
            continue;
        };
        let depends_on = |other: &str| {
            manifest
                .lines()
                .any(|line| line.trim_start().starts_with(&format!("{other} ")))
        };
        if !surfaces.contains(&name.as_str()) {
            assert!(!depends_on("fission-analysis-kernel"), "{name}");
        }
        if [
            "fission-decompiler",
            "fission-pcode",
            "fission-static",
            "fission-sleigh",
        ]
        .contains(&name.as_str())
            || name.starts_with("fission-midend-")
        {
            assert!(!depends_on("fission-dynamic"), "{name}");
        }
    }
}

#[test]
fn a_claim_from_another_decoder_context_or_spec_is_not_applied() {
    let fixture = FIXTURES[0].0;
    let (trace, _) = capture(fixture, Options::args(&[]));
    let mut imported = EvidenceLedger::default();
    import(fixture, &trace, &mut imported);
    let call = imported
        .claims()
        .map(|(_, claim)| claim.clone())
        .find(|c| c.predicate == fission_evidence::Predicate::IndirectCallTarget)
        .unwrap();

    let mut ledger = EvidenceLedger::default();
    let mut other_context = call.clone();
    let fission_evidence::Entity::Code(location) = &mut other_context.entity else {
        panic!("the site is image code");
    };
    location.decode.context_override = Some("1/1".into());
    ledger.insert(other_context).unwrap();
    let mut other_spec = call;
    let fission_evidence::Entity::Code(location) = &mut other_spec.entity else {
        panic!("the site is image code");
    };
    location.image.spec.compiler_spec_id = Some("windows".into());
    ledger.insert(other_spec).unwrap();

    let view = view(fixture, "dispatch_call", &ledger);
    assert!(only_site(&view).observed_candidates.is_empty());
    let mut reasons: Vec<&str> = view.excluded.iter().map(|e| e.reason.as_str()).collect();
    reasons.sort_unstable();
    assert_eq!(
        reasons,
        [
            "decoded in another decoder context",
            "decoded under another language or compiler spec"
        ]
    );
}
