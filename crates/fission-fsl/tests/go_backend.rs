use fission_fsl::{backend::*, compile_source, FirOp};
fn emit(source: &str) -> ArtifactBundle {
    let p = compile_source(source).unwrap();
    let backend = GoBackend;
    let plan = backend
        .plan(
            ProjectionInput::Instruction {
                body: &p.instructions[0],
                symbol: "fsl_execute",
            },
            &TargetProfile::for_backend(&backend),
        )
        .unwrap();
    backend.emit(&plan).unwrap()
}
#[test]
fn go_uses_borrowed_fir_for_stack_register_and_wave_contexts() {
    for source in [
        include_str!("../specs/jvm-se26-iadd.fsl"),
        include_str!("../specs/stack-branch-join.fsl"),
        include_str!("../specs/register-branch-convert.fsl"),
        include_str!("../specs/amdgcn-gfx900-saddc-u32.fsl"),
        include_str!("../specs/amdgcn-gfx900-vadd-u32-wave64.fsl"),
    ] {
        let bundle = emit(source);
        assert_eq!(bundle.descriptor.dialect, "go1.25");
        assert_eq!(bundle.sources[0].name, "output.go");
        assert_eq!(bundle.sources[0].source.matches("package fsl").count(), 1);
        assert!(!bundle.sources[0].source.contains("unsafe"));
        assert!(bundle.report.contains("equivalence=not-performed"));
        assert_eq!(bundle.build.tool, "go");
    }
}
#[test]
fn go_refuses_invalid_domains_and_foreign_plans() {
    let p = compile_source(include_str!("../specs/jvm-se26-iadd.fsl")).unwrap();
    let backend = GoBackend;
    let target = TargetProfile::for_backend(&backend);
    let input = ProjectionInput::Instruction {
        body: &p.instructions[0],
        symbol: "fsl_execute",
    };
    let plan = SourceBackend::C
        .plan(input, &TargetProfile::for_backend(&SourceBackend::C))
        .unwrap();
    assert_eq!(
        backend.emit(&plan).unwrap_err().code,
        RejectionCode::BackendMismatch
    );
    let mut body = p.instructions[0].clone();
    body.ops = vec![FirOp::Unsupported];
    body.values.clear();
    assert_eq!(
        backend
            .plan(
                ProjectionInput::Instruction {
                    body: &body,
                    symbol: "fsl_execute"
                },
                &target
            )
            .unwrap_err()
            .operation,
        Some(0)
    );
    let bad = ProjectionInput::Instruction {
        body: &p.instructions[0],
        symbol: "fsl_runtime_bool",
    };
    assert_eq!(
        backend.plan(bad, &target).unwrap_err().code,
        RejectionCode::InvalidSymbol
    );
    // Direct shared admission cannot bypass provider-specific namespace rules.
    let forged = ProjectionPlan::checked(backend.descriptor(), bad, &target).unwrap();
    assert_eq!(
        backend.emit(&forged).unwrap_err().code,
        RejectionCode::InvalidSymbol
    );
    assert_eq!(
        backend
            .plan(
                input,
                &TargetProfile {
                    runtime: RuntimePolicy::ForbidEmbedded,
                    ..target
                }
            )
            .unwrap_err()
            .code,
        RejectionCode::RuntimeForbidden
    );
}
#[cfg(feature = "x86")]
#[test]
fn go_x86_window_preserves_origins_and_mutable_transaction_requirement() {
    use fission_fsl::x86::{X86Package, X86Program};
    let p = X86Package::compile(
        include_str!("../specs/x86/scalar-immediate.fslx"),
        include_str!("../specs/x86/scalar-immediate-bodies.fsl"),
    )
    .unwrap();
    for mode in [16, 32, 64] {
        let bytes = if mode == 64 {
            vec![0x48, 0xc7, 0x47, 8, 0xf9, 0xff, 0xff, 0xff, 0xc3]
        } else if mode == 32 {
            vec![0xc7, 0x47, 8, 1, 0, 0, 0, 0xc3]
        } else {
            vec![0xc7, 0x47, 8, 1, 0, 0xc3]
        };
        let window = X86Program::lift(&p, mode, 0x1000, &bytes).unwrap();
        let b = GoBackend;
        let plan = b
            .plan(
                ProjectionInput::X86Window(&window),
                &TargetProfile::for_backend(&b),
            )
            .unwrap();
        assert!(plan
            .requirements()
            .contains(&Requirement::TransactionalByteMemory));
        let bundle = b.emit(&plan).unwrap();
        assert_eq!(bundle.origins.len(), 2);
        assert_eq!(bundle.origins[0].guest_address, Some(0x1000));
        assert!(bundle.sources[0]
            .source
            .contains("destination_memory:=memory"));
        assert!(bundle.sources[0].source.contains("func fsl_x86"));
    }
}
