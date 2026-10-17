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

#[test]
fn explicit_noise_roundtrip_probability_and_projection_contracts() {
    for name in ["bit_flip", "phase_flip", "amplitude_damping"] {
        for (n, d) in [(0, 1), (1, 1), (1, 2), (1, 255), (254, 255)] {
            let package = program(&format!("quantum.{name} 0,{n},{d};")).unwrap();
            assert_eq!(package.version, 13);
            let bytes = package.encode_binary().unwrap();
            let decoded = FslcPackage::decode_binary(&bytes).unwrap();
            assert_eq!(decoded, package);
            let mut old = package.clone();
            old.version = 12;
            assert!(old.encode_binary().is_err());
            // Malformed wire input cannot bypass version admission.
            let mut old_bytes = bytes;
            old_bytes[4..6].copy_from_slice(&12u16.to_le_bytes());
            assert!(FslcPackage::decode_binary(&old_bytes).is_err());
            assert!(emit_openqasm(&decoded.instructions[0], 1).is_err());
            let initial = PureState::basis(1, 1).unwrap();
            let result = simulate(&decoded.instructions[0], &initial).unwrap();
            close(result.total_probability(), 1.0);
            assert!(result
                .branches()
                .iter()
                .all(|b| b.measurements.is_empty() && b.values.is_empty()));
            assert!(result.diagnostic_json().contains("declared-noise"));
            let rho = result.density_matrix();
            let p = f64::from(n) / f64::from(d);
            close(rho[0].re, if name == "phase_flip" { 0.0 } else { p });
            close(rho[3].re, if name == "phase_flip" { 1.0 } else { 1.0 - p });
        }
        for probability in ["1,0", "2,1", "256,255", "-1,2"] {
            assert!(program(&format!("quantum.{name} 0,{probability};")).is_err());
        }
    }
}
#[test]
fn damping_of_entangled_subsystem_preserves_partner_and_coherence() {
    let instruction = body("quantum.h 0; quantum.cx 0,1; quantum.amplitude_damping 0,1,4;");
    let result = simulate(&instruction, &PureState::basis(2, 0).unwrap()).unwrap();
    let rho = result.density_matrix();
    close(rho[0].re, 0.5);
    close(rho[10].re, 0.125);
    close(rho[15].re, 0.375);
    close(rho[3].re, 0.75f64.sqrt() / 2.0);
    close(rho[12].re, 0.75f64.sqrt() / 2.0);
    close(rho[0].re + rho[5].re, 0.5);
    close(rho[10].re + rho[15].re, 0.5);
    assert!(result.branches().iter().all(|b| b.measurements.is_empty()));
    let phase = body("quantum.h 0; quantum.cx 0,1; quantum.phase_flip 0,1,2;");
    let rho = simulate(&phase, &PureState::basis(2, 0).unwrap())
        .unwrap()
        .density_matrix();
    close(rho[0].re, 0.5);
    close(rho[15].re, 0.5);
    close(rho[3].re, 0.0);
}
#[test]
fn measurement_cfg_branches_join_through_canonical_block_parameters() {
    let instruction = body("block entry() { quantum.h 0; quantum.cx 0,1; %a:u1=quantum.measure_z 0; branch.if %a, correct(%a), unchanged(%a); } block correct(%b:u1) { quantum.x 1; branch join(%b); } block unchanged(%c:u1) { branch join(%c); } block join(%d:u1) { %out:u1=quantum.measure_z 1; return; }");
    let package = program("block entry() { quantum.h 0; %a:u1=quantum.measure_z 0; branch end(%a); } block end(%b:u1) { quantum.x_if 0,%b; return; }").unwrap();
    assert_eq!(
        package,
        FslcPackage::decode_binary(&package.encode_binary().unwrap()).unwrap()
    );
    let initial = PureState::basis(2, 0).unwrap();
    let result = simulate(&instruction, &initial).unwrap();
    assert_eq!(result.branches().len(), 2);
    for b in result.branches() {
        close(b.probability, 0.5);
        assert_eq!(b.values[0], b.values[3]);
        assert_eq!(b.values[4], Some(0));
        assert_eq!(b.measurements.len(), 2);
        assert_eq!(b.values[1].is_some(), b.values[0] == Some(1));
        assert_eq!(b.values[2].is_some(), b.values[0] == Some(0));
    }
    assert!(emit_openqasm(&instruction, 2).is_err());
}
#[test]
fn cfg_parameter_transfer_is_parallel_and_noise_outcomes_are_hidden() {
    let instruction = body("block entry() { %a:u1=int.const 0; %b:u1=int.const 1; quantum.h 0; branch swap(%b,%a); } block swap(%c:u1,%d:u1) { quantum.x_if 0,%c; quantum.phase_flip 0,1,2; branch.if %d, yes(), no(); } block yes() { quantum.x 0; return; } block no() { quantum.z 0; return; }");
    let result = simulate(&instruction, &PureState::basis(1, 0).unwrap()).unwrap();
    for b in result.branches() {
        assert_eq!(b.values, vec![Some(0), Some(1), Some(1), Some(0)]);
        assert!(b.measurements.is_empty());
    }
    let rho = result.density_matrix();
    close(rho[0].re, 0.5);
    close(rho[3].re, 0.5);
    close(rho[1].re, 0.0);
}
#[test]
fn untaken_cycles_and_cross_block_undefined_values_refuse() {
    let instruction = body("block entry() { %c:u1=int.const 0; quantum.x 0; branch.if %c, forever(), done(); } block forever() { branch forever(); } block done() { return; }");
    assert!(instruction.validate().is_ok());
    let initial = PureState::basis(1, 0).unwrap();
    let saved = initial.clone();
    assert!(simulate(&instruction, &initial)
        .unwrap_err()
        .to_string()
        .contains("cyclic"));
    assert_eq!(initial, saved);
    assert!(program("block entry() { %c:u1=int.const 1; branch next(); } block next() { quantum.x_if 0,%c; return; }").is_err());
    let excessive = body(&"quantum.bit_flip 0,1,2;".repeat(13));
    assert!(simulate(&excessive, &initial).is_err());
}

#[test]
fn channel_cfg_fixture_is_portable_and_all_bodies_have_bounded_execution() {
    let package = compile_source(include_str!("../specs/quantum-channels-cfg.fsl")).unwrap();
    assert_eq!(package.version, 13);
    assert_eq!(package.instructions.len(), 33);
    let decoded = FslcPackage::decode_binary(&package.encode_binary().unwrap()).unwrap();
    assert_eq!(package, decoded);
    for body in &decoded.instructions {
        let n = u8::try_from(
            QuantumContract::for_instruction(body)
                .unwrap()
                .minimum_qubits,
        )
        .unwrap();
        for basis in 0..1usize << n {
            let result = simulate(body, &PureState::basis(n, basis).unwrap()).unwrap();
            close(result.total_probability(), 1.0);
        }
    }
}

#[test]
fn bounded_loop_returns_only_after_every_path_terminates() {
    let body = body("block entry() { branch loop(); } block loop() { quantum.x 0; %m:u1=quantum.measure_z 0; branch.if %m, loop(), done(%m); } block done(%out:u1) { return; }");
    assert!(simulate(&body, &PureState::basis(1, 0).unwrap()).is_err());
    for initial in [
        PureState::basis(1, 0).unwrap(),
        PureState::basis(1, 1).unwrap(),
        PureState::new(
            1,
            vec![Complex { re: 0.6, im: 0.0 }, Complex { re: 0.0, im: 0.8 }],
        )
        .unwrap(),
    ] {
        let run = simulate_bounded(&body, &initial, QuantumLimits::default()).unwrap();
        assert!(run.is_complete());
        assert!(run.pending().is_empty());
        let result = run.into_complete().unwrap();
        close(result.total_probability(), 1.0);
        close(result.density_matrix()[0].re, 1.0);
        for branch in result.branches() {
            assert_eq!(branch.values, vec![Some(0), Some(0)]);
            assert!(branch.measurements.len() <= 2);
        }
    }
}
#[test]
fn stopped_loop_preserves_unfinished_probability_without_renormalizing() {
    let body=body("block entry() { branch loop(); } block loop() { quantum.h 0; %m:u1=quantum.measure_z 0; branch.if %m, loop(), done(); } block done() { return; }");
    let run = simulate_bounded(
        &body,
        &PureState::basis(1, 0).unwrap(),
        QuantumLimits {
            steps: 1000,
            splits: 4,
            branches: 4096,
        },
    )
    .unwrap();
    assert!(!run.is_complete());
    assert_eq!(run.limit(), Some(QuantumLimit::Splits));
    let finished: f64 = run.completed().iter().map(|b| b.probability).sum();
    let frontier: f64 = run.pending().iter().map(|b| b.branch.probability).sum();
    close(finished, 15.0 / 16.0);
    close(frontier, 1.0 / 16.0);
    assert!(run.diagnostic_json().contains("\"result\":null"));
    assert!(run.into_complete().is_err());
}
#[test]
fn all_budgets_stop_before_the_unexecuted_effect() {
    let empty =
        body("block entry() { quantum.x 0; branch again(); } block again() { branch again(); }");
    let initial = PureState::basis(1, 0).unwrap();
    let zero = simulate_bounded(
        &empty,
        &initial,
        QuantumLimits {
            steps: 0,
            ..QuantumLimits::default()
        },
    )
    .unwrap();
    assert_eq!(zero.limit(), Some(QuantumLimit::Steps));
    assert_eq!(zero.pending()[0].branch.state, initial);
    let stopped = simulate_bounded(
        &empty,
        &initial,
        QuantumLimits {
            steps: 7,
            ..QuantumLimits::default()
        },
    )
    .unwrap();
    assert_eq!(stopped.steps(), 7);
    assert_eq!(stopped.limit(), Some(QuantumLimit::Steps));
    let split = body("quantum.h 0; %m:u1=quantum.measure_z 0;");
    let cap = simulate_bounded(
        &split,
        &initial,
        QuantumLimits {
            branches: 1,
            ..QuantumLimits::default()
        },
    )
    .unwrap();
    assert_eq!(cap.limit(), Some(QuantumLimit::Branches));
    assert!(cap.pending()[0].branch.measurements.is_empty());
    assert_eq!(cap.pending()[0].operation, 1);
    let split_bound = simulate_bounded(
        &split,
        &initial,
        QuantumLimits {
            splits: 0,
            ..QuantumLimits::default()
        },
    )
    .unwrap();
    assert_eq!(split_bound.limit(), Some(QuantumLimit::Splits));
    assert!(simulate_bounded(
        &split,
        &initial,
        QuantumLimits {
            branches: 0,
            ..QuantumLimits::default()
        }
    )
    .is_err());
    assert!(simulate_bounded(
        &split,
        &initial,
        QuantumLimits {
            steps: 1_000_001,
            ..QuantumLimits::default()
        }
    )
    .is_err());
}
#[test]
fn bounded_cycles_transfer_block_arguments_in_parallel() {
    let body=body("block entry() { %a:u1=int.const 0; %b:u1=int.const 1; quantum.z 0; branch again(%a,%b); } block again(%x:u1,%y:u1) { branch.if %x, done(%x,%y), again(%y,%x); } block done(%outx:u1,%outy:u1) { return; }");
    let result = simulate_bounded(
        &body,
        &PureState::basis(1, 0).unwrap(),
        QuantumLimits::default(),
    )
    .unwrap()
    .into_complete()
    .unwrap();
    assert_eq!(
        result.branches()[0].values,
        vec![Some(0), Some(1), Some(1), Some(0), Some(1), Some(0)]
    );
}

#[test]
fn relaxation_package_and_decay_contract() {
    let p = program("quantum.relaxation 0,1000000,2000000,3000000;").unwrap();
    assert_eq!(p.version, 14);
    assert_eq!(
        p,
        FslcPackage::decode_binary(&p.encode_binary().unwrap()).unwrap()
    );
    let mut old = p.clone();
    old.version = 13;
    assert!(old.encode_binary().is_err());
    let mut bytes = p.encode_binary().unwrap();
    bytes[4..6].copy_from_slice(&13u16.to_le_bytes());
    assert!(FslcPackage::decode_binary(&bytes).is_err());
    for invalid in ["0,1,0,1", "0,1,1,0", "0,1,2,5"] {
        assert!(program(&format!("quantum.relaxation {invalid};")).is_err());
    }
    let initial = PureState::new(
        1,
        vec![
            Complex {
                re: std::f64::consts::FRAC_1_SQRT_2,
                im: 0.0,
            },
            Complex {
                re: 0.0,
                im: std::f64::consts::FRAC_1_SQRT_2,
            },
        ],
    )
    .unwrap();
    let result = simulate(&p.instructions[0], &initial).unwrap();
    let rho = result.density_matrix();
    close(rho[3].re, 0.5 * (-0.5f64).exp());
    close(rho[1].im, -0.5 * (-1.0f64 / 3.0).exp());
    close(result.total_probability(), 1.0);
    assert!(result.branches().iter().all(|b| b.measurements.is_empty()));
    assert!(emit_openqasm(&p.instructions[0], 1).is_err());
    let run = simulate_bounded(
        &p.instructions[0],
        &initial,
        QuantumLimits {
            splits: 1,
            ..QuantumLimits::default()
        },
    )
    .unwrap();
    assert_eq!(run.limit(), Some(QuantumLimit::Splits));
    assert_eq!(run.pending()[0].branch.state, initial);
    let zero = body("quantum.relaxation 0,0,1,2;");
    close(
        simulate(&zero, &initial).unwrap().density_matrix()[1].im,
        -0.5,
    );
    let extreme = body("quantum.relaxation 0,18446744073709551615,1,2;");
    close(
        simulate(&extreme, &initial).unwrap().density_matrix()[0].re,
        1.0,
    );
}

#[test]
fn upstream_qisa_word_formats_decode_but_execution_remains_unsupported() {
    let p = compile_source(include_str!("../specs/qisa-qelab-9b024494.fsl")).unwrap();
    for (raw, field, value) in [
        (0x600fffffu32, "cycles", 1048575),
        (0x70038000, "source", 7),
        (0x813e0200, "separator", 0),
    ] {
        let decoded = p
            .decode_bytes(&p.language, &raw.to_le_bytes())
            .unwrap()
            .unwrap();
        assert_eq!(
            decoded
                .fields
                .iter()
                .find(|(name, _)| name == field)
                .unwrap()
                .1,
            value
        );
        assert_eq!(p.reencode(&decoded, &[]).unwrap(), raw.to_le_bytes());
        let body = &p.instructions[decoded.instruction_index];
        assert_eq!(body.ops, vec![FirOp::Unsupported]);
        assert!(simulate(body, &PureState::basis(1, 0).unwrap()).is_err());
        assert!(simulate_bounded(
            body,
            &PureState::basis(1, 0).unwrap(),
            QuantumLimits::default()
        )
        .is_err());
    }
}
