use fission_fsl::{backend::*, compile_source, emit_instruction, OutputLayer};
fn sample() -> fission_fsl::CompiledInstruction {
    compile_source(include_str!("../specs/jvm-se26-iadd.fsl"))
        .unwrap()
        .instructions
        .remove(0)
}
#[test]
fn admitted_borrowed_plan_preserves_source_and_reports_unperformed_validation() {
    let body = sample();
    for (backend, layer) in [
        (SourceBackend::C, OutputLayer::C),
        (SourceBackend::Rust, OutputLayer::Rust),
    ] {
        let plan = backend
            .plan(
                ProjectionInput::Instruction {
                    body: &body,
                    symbol: "fsl_execute",
                },
                &TargetProfile::for_backend(&backend),
            )
            .unwrap();
        assert!(plan.requirements().contains(&Requirement::VmStack));
        let bundle = backend.emit(&plan).unwrap();
        assert_eq!(
            bundle.sources[0].source,
            emit_instruction(&body, layer, "fsl_execute").unwrap()
        );
        assert_eq!(bundle.origins[0].guest_address, None);
        assert!(bundle
            .report
            .contains("compilation=not-performed equivalence=not-performed"));
        assert!(!bundle.build.arguments.is_empty());
    }
}
#[test]
fn profile_runtime_memory_and_foreign_backend_plans_refuse() {
    let body = sample();
    let backend = SourceBackend::C;
    let input = ProjectionInput::Instruction {
        body: &body,
        symbol: "fsl_execute",
    };
    let target = TargetProfile::for_backend(&backend);
    for (profile, code) in [
        (
            TargetProfile {
                dialect: "go1".into(),
                ..target.clone()
            },
            RejectionCode::ProfileMismatch,
        ),
        (
            TargetProfile {
                runtime: RuntimePolicy::ForbidEmbedded,
                ..target.clone()
            },
            RejectionCode::RuntimeForbidden,
        ),
        (
            TargetProfile {
                memory: MemoryModel::ArchitecturalFaults,
                ..target.clone()
            },
            RejectionCode::MemoryModelUnsupported,
        ),
    ] {
        assert_eq!(backend.plan(input, &profile).unwrap_err().code, code);
    }
    let plan = backend.plan(input, &target).unwrap();
    assert_eq!(
        SourceBackend::Rust.emit(&plan).unwrap_err().code,
        RejectionCode::BackendMismatch
    );
    assert_eq!(
        backend
            .plan(
                ProjectionInput::Instruction {
                    body: &body,
                    symbol: "fsl_bad\n"
                },
                &target
            )
            .unwrap_err()
            .code,
        RejectionCode::InvalidSymbol
    );
}
#[test]
fn unsupported_operation_and_invalid_fir_never_produce_a_plan() {
    let mut body = sample();
    body.ops = vec![fission_fsl::FirOp::Unsupported];
    body.values.clear();
    let backend = SourceBackend::Rust;
    let target = TargetProfile::for_backend(&backend);
    let e = backend
        .plan(
            ProjectionInput::Instruction {
                body: &body,
                symbol: "fsl_case",
            },
            &target,
        )
        .unwrap_err();
    assert_eq!(e.code, RejectionCode::UnsupportedSemantics);
    assert_eq!(e.operation, Some(0));
    assert_eq!(e.body, Some("iadd".into()));
    let mut body = sample();
    body.values[0].ty.bits = 0;
    assert_eq!(
        backend
            .plan(
                ProjectionInput::Instruction {
                    body: &body,
                    symbol: "fsl_case"
                },
                &target
            )
            .unwrap_err()
            .code,
        RejectionCode::InvalidFir
    );
}
struct TestProvider {
    descriptor: BackendDescriptor,
}
impl OutputBackend for TestProvider {
    fn descriptor(&self) -> BackendDescriptor {
        self.descriptor
    }
    fn emit(&self, _: &ProjectionPlan<'_>) -> Result<ArtifactBundle, UnsupportedReport> {
        unreachable!("test provider has no emitter")
    }
}
#[test]
fn registry_extension_is_open_and_duplicate_or_invalid_ids_refuse() {
    let mut r = BackendRegistry::with_builtin_sources();
    assert_eq!(
        r.descriptors().iter().map(|d| d.id).collect::<Vec<_>>(),
        ["c", "rust"]
    );
    assert!(r.get("go").is_none());
    let d = BackendDescriptor {
        id: "test-provider",
        dialect: "test1",
        extension: "txt",
        revision: 1,
        contexts: &[ProjectionContext::VmStackInstruction],
    };
    r.register(Box::new(TestProvider { descriptor: d }))
        .unwrap();
    assert_eq!(
        r.register(Box::new(TestProvider { descriptor: d }))
            .unwrap_err()
            .code,
        RejectionCode::BackendAlreadyRegistered
    );
    let bad = BackendDescriptor {
        id: "bad",
        extension: "../source",
        ..d
    };
    assert_eq!(
        r.register(Box::new(TestProvider { descriptor: bad }))
            .unwrap_err()
            .code,
        RejectionCode::InvalidDescriptor
    );
    let body = sample();
    let provider = r.get("test-provider").unwrap();
    let plan = provider
        .plan(
            ProjectionInput::Instruction {
                body: &body,
                symbol: "fsl_case",
            },
            &TargetProfile::for_backend(provider),
        )
        .unwrap();
    assert_eq!(plan.descriptor().id, "test-provider");
}
#[test]
fn descriptor_contexts_are_admission_limits() {
    let body = sample();
    let provider = TestProvider {
        descriptor: BackendDescriptor {
            id: "register-only",
            dialect: "test1",
            extension: "txt",
            revision: 1,
            contexts: &[ProjectionContext::RegisterInstruction],
        },
    };
    assert_eq!(
        provider
            .plan(
                ProjectionInput::Instruction {
                    body: &body,
                    symbol: "fsl_case"
                },
                &TargetProfile::for_backend(&provider)
            )
            .unwrap_err()
            .code,
        RejectionCode::ContextUnsupported
    );
}
#[cfg(feature = "x86")]
#[test]
fn x86_window_plans_keep_origin_mapping_and_transaction_contract() {
    use fission_fsl::x86::{X86Package, X86Program};
    let package = X86Package::compile(
        include_str!("../specs/x86/scalar-immediate.fslx"),
        include_str!("../specs/x86/scalar-immediate-bodies.fsl"),
    )
    .unwrap();
    let program = X86Program::lift(
        &package,
        64,
        0x1000,
        &[0x48, 0xc7, 0x47, 8, 0xf9, 0xff, 0xff, 0xff, 0xc3],
    )
    .unwrap();
    for backend in [SourceBackend::C, SourceBackend::Rust] {
        let plan = backend
            .plan(
                ProjectionInput::X86Window(&program),
                &TargetProfile::for_backend(&backend),
            )
            .unwrap();
        assert!(plan
            .requirements()
            .contains(&Requirement::TransactionalByteMemory));
        assert!(plan.requirements().contains(&Requirement::GuestPc));
        assert!(!plan
            .requirements()
            .contains(&Requirement::ReadOnlyByteMemory));
        let bundle = backend.emit(&plan).unwrap();
        assert_eq!(bundle.origins.len(), 2);
        assert_eq!(bundle.origins[0].guest_address, Some(0x1000));
        assert_eq!(bundle.origins[0].input_offset, Some(0));
        assert_eq!(bundle.origins[0].byte_length, Some(8));
        assert_eq!(bundle.origins[1].guest_address, Some(0x1008));
        assert_eq!(
            bundle.origins[0].raw_hex.as_deref(),
            Some("48c74708f9ffffff")
        );
        assert!(bundle.report.contains("statement positions unavailable"));
        assert_eq!(
            bundle.sources[0].source,
            program
                .emit(if backend == SourceBackend::C {
                    OutputLayer::C
                } else {
                    OutputLayer::Rust
                })
                .unwrap()
        );
    }
}
