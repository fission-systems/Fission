use fission_fsl::{compile_source, quantum::*, FirOp, FslcPackage, OutputLayer, ValueId};
const SOURCE: &str = include_str!("../specs/quantum-circuits.fsl");
fn program(source: &str) -> Result<FslcPackage, fission_fsl::FslError> {
    compile_source(&format!("language quantum.test {{ byte_order little; address_unit byte; instruction test {{ opcode 0; mnemonic \"test\"; evidence \"authored\" \"synthetic\" \"v1\" \"logical quantum fixture\"; semantics {{ {source} }} }} }}"))
}
fn body(source: &str) -> fission_fsl::CompiledInstruction {
    program(source).unwrap().instructions.remove(0)
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-11, "{a} != {b}");
}
#[test]
fn package_and_qasm_roundtrip_preserve_canonical_operations() {
    let package = compile_source(SOURCE).unwrap();
    assert_eq!(package.version, 12);
    assert_eq!(
        package,
        FslcPackage::decode_binary(&package.encode_binary().unwrap()).unwrap()
    );
    for instruction in &package.instructions {
        let n: u8 = QuantumContract::for_instruction(instruction)
            .unwrap()
            .minimum_qubits
            .try_into()
            .unwrap();
        let text = emit_openqasm(instruction, n).unwrap();
        let imported = import_openqasm(&text).unwrap();
        assert_eq!(imported.qubits, n);
        assert_eq!(imported.package.instructions[0].ops, instruction.ops);
        for index in 0..1usize << n {
            let initial = PureState::basis(n, index).unwrap();
            assert_eq!(
                simulate(instruction, &initial).unwrap(),
                simulate(&imported.package.instructions[0], &initial).unwrap()
            );
        }
    }
    let mut old = package;
    old.version = 11;
    assert!(old.encode_binary().is_err());
}
#[test]
fn h_involution_and_interference_on_complex_inputs() {
    let hh = body("quantum.h 0; quantum.h 0;");
    let hzh = body("quantum.h 0; quantum.z 0; quantum.h 0;");
    for i in 0..65 {
        let angle = (i as f64) * 0.071;
        let initial = PureState::new(
            1,
            vec![
                Complex {
                    re: angle.cos(),
                    im: 0.0,
                },
                Complex {
                    re: 0.0,
                    im: angle.sin(),
                },
            ],
        )
        .unwrap();
        for (instruction, expected) in [
            (&hh, initial.amplitudes().to_vec()),
            (&hzh, initial.amplitudes().iter().rev().copied().collect()),
        ] {
            let result = simulate(instruction, &initial).unwrap();
            close(result.total_probability(), 1.0);
            for (a, b) in result.branches()[0].state.amplitudes().iter().zip(expected) {
                close(a.re, b.re);
                close(a.im, b.im);
            }
        }
    }
}
#[test]
fn bell_coherence_measurement_and_reset_mixture_are_distinct() {
    let package = compile_source(SOURCE).unwrap();
    let initial = PureState::basis(2, 0).unwrap();
    let find = |name| {
        package
            .instructions
            .iter()
            .find(|i| i.name == name)
            .unwrap()
    };
    let coherent = simulate(find("bell_coherent"), &initial).unwrap();
    close(coherent.density_matrix()[3].re, 0.5); // |00><11| coherence.
    let measured = simulate(find("bell"), &initial).unwrap();
    assert_eq!(measured.branches().len(), 2);
    for b in measured.branches() {
        close(b.probability, 0.5);
        assert_eq!(b.values[0], b.values[1]);
    }
    close(measured.density_matrix()[3].re, 0.0);
    let reset = simulate(find("reset_entangled"), &initial).unwrap();
    let density = reset.density_matrix();
    for (i, value) in density.iter().enumerate() {
        close(value.re, if i == 0 || i == 10 { 0.5 } else { 0.0 });
        close(value.im, 0.0);
    }
    assert!(reset.branches().iter().all(|b| b.measurements.is_empty()));
}
#[test]
fn repeated_measurement_and_feedforward_use_post_measurement_state() {
    let repeated = body("quantum.h 0; %a:u1=quantum.measure_z 0; %b:u1=quantum.measure_z 0;");
    let initial = PureState::basis(1, 0).unwrap();
    let result = simulate(&repeated, &initial).unwrap();
    assert_eq!(result.branches().len(), 2);
    for b in result.branches() {
        assert_eq!(b.values[0], b.values[1]);
    }
    let package = compile_source(SOURCE).unwrap();
    let ff = package
        .instructions
        .iter()
        .find(|i| i.name == "feedforward")
        .unwrap();
    let result = simulate(ff, &initial).unwrap();
    for b in result.branches() {
        assert_eq!(b.values[1], Some(0));
        close(b.state.amplitudes()[0].norm_for_test(), 1.0);
    }
}
trait Norm {
    fn norm_for_test(&self) -> f64;
}
impl Norm for Complex {
    fn norm_for_test(&self) -> f64 {
        self.re * self.re + self.im * self.im
    }
}
#[test]
fn invalid_resources_types_and_foreign_backends_refuse() {
    for source in [
        "quantum.cx 0,0;",
        "%a:u32=quantum.measure_z 0;",
        "quantum.x_if 0,%unknown;",
        "%a:u8=int.const 1; quantum.x_if 0,%a;",
    ] {
        assert!(program(source).is_err());
    }
    let package = compile_source(SOURCE).unwrap();
    let instruction = &package.instructions[0];
    for layer in [
        OutputLayer::C,
        OutputLayer::Rust,
        OutputLayer::CudaCpp,
        OutputLayer::Ptx,
    ] {
        assert!(fission_fsl::emit_instruction(instruction, layer, "fsl_quantum").is_err());
    }
    use fission_fsl::backend::{BackendRegistry, ProjectionInput, TargetProfile};
    let registry = BackendRegistry::with_builtin_sources();
    let go = registry.get("go").unwrap();
    assert!(go
        .plan(
            ProjectionInput::Instruction {
                body: instruction,
                symbol: "fsl_quantum"
            },
            &TargetProfile::for_backend(go)
        )
        .is_err());
    assert!(fission_fsl::JitDecoder::compile(&package).is_err());
    let mut stack = vec![7];
    assert!(fission_fsl::execute_instruction(instruction, &mut stack, 8).is_err());
    assert_eq!(stack, vec![7]);
    let mut bad = instruction.clone();
    bad.ops[0] = FirOp::Quantum(QuantumOp::MeasureZ {
        qubit: 0,
        output: ValueId(65535),
    });
    assert!(bad.validate().is_err());
}
#[test]
fn input_and_expansion_bounds_refuse() {
    assert!(PureState::basis(9, 0).is_err());
    assert!(PureState::basis(1, 2).is_err());
    assert!(PureState::new(
        1,
        vec![
            Complex {
                re: f64::NAN,
                im: 0.0
            };
            2
        ]
    )
    .is_err());
    assert!(PureState::new(1, vec![Complex::default(); 2]).is_err());
    let instruction = body("quantum.x 1;");
    assert!(simulate(&instruction, &PureState::basis(1, 0).unwrap()).is_err());
    let too_many = body(&"quantum.reset 0;".repeat(13));
    assert!(simulate(&too_many, &PureState::basis(1, 0).unwrap()).is_err());
    let mut mixed = body("%v:u1=int.const 1; quantum.x_if 0,%v;");
    mixed.ops.push(FirOp::VmStackPush { value: ValueId(0) });
    assert!(QuantumContract::for_instruction(&mixed).is_err());
}
#[test]
fn qasm_unknown_semantics_and_mutable_classical_slots_refuse() {
    let source = emit_openqasm(&body("quantum.h 0; %a:u1=quantum.measure_z 0;"), 1).unwrap();
    for bad in [
        source.replace("h q[0];", "t q[0];"),
        source.replace("qubit[1]", "qubit[9]"),
        source.replace("m[0] = measure", "m[1] = measure"),
        source.clone() + "m[0] = 0;",
        source.clone() + "if (m[1] == 1) { x q[0]; }",
        source.replace("3.0", "2.0"),
        source.clone() + "reset $0;",
    ] {
        assert!(import_openqasm(&bad).is_err(), "{bad}");
    }
}

#[test]
fn numerical_support_and_ir_extent_do_not_silently_truncate() {
    let large = body("quantum.x 255;");
    assert_eq!(
        QuantumContract::for_instruction(&large)
            .unwrap()
            .minimum_qubits,
        256
    );
    assert!(simulate(&large, &PureState::basis(8, 0).unwrap()).is_err());
    let edge = simulate(&body("quantum.x 7;"), &PureState::basis(8, 128).unwrap()).unwrap();
    close(edge.branches()[0].state.amplitudes()[0].re, 1.0);
    let initial = PureState::new(
        1,
        vec![Complex { re: 1.0, im: 0.0 }, Complex { re: 1e-10, im: 0.0 }],
    )
    .unwrap();
    let measured = simulate(&body("%m:u1=quantum.measure_z 0;"), &initial).unwrap();
    assert_eq!(measured.branches().len(), 2);
    let rare = measured
        .branches()
        .iter()
        .find(|b| b.values[0] == Some(1))
        .unwrap();
    assert!((rare.probability - 1e-20).abs() < 1e-30);
}

#[test]
fn bit_constants_roundtrip_and_quantum_effects_never_enter_classical_control() {
    let instruction = body("%v:u1=int.const 1; quantum.x_if 0,%v;");
    let state = PureState::basis(1, 0).unwrap();
    let result = simulate(&instruction, &state).unwrap();
    close(result.branches()[0].state.amplitudes()[1].re, 1.0);
    let imported = import_openqasm(&emit_openqasm(&instruction, 1).unwrap()).unwrap();
    assert_eq!(
        result,
        simulate(&imported.package.instructions[0], &state).unwrap()
    );
    let mut stack = vec![7];
    assert!(fission_fsl::execute_instruction(&instruction, &mut stack, 8).is_err());
    assert_eq!(stack, vec![7]);
    for layer in [OutputLayer::C, OutputLayer::Rust] {
        assert!(fission_fsl::emit_instruction(&instruction, layer, "fsl_quantum").is_err());
    }
}
