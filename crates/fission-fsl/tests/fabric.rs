use fission_fsl::{
    compile_source, emit_instruction,
    fabric::{FabricPlan, AXES},
    FslcPackage, OutputLayer,
};

const SOURCE: &str = include_str!("../specs/fabric-kernels.fsl");
fn profile(semantics: &str) -> FslcPackage {
    compile_source(&format!("language authored {{ byte_order little; address_unit byte; instruction probe {{ opcode 1; mnemonic \"authored\"; evidence \"self-authored\" \"synthetic\" \"v1\" \"Bit-vector fixture\"; semantics {{ {semantics} }} }} }}")).unwrap()
}

#[test]
fn v15_package_and_eight_axes_keep_older_packages() {
    let package = compile_source(SOURCE).unwrap();
    assert_eq!(package.version, 15);
    let bytes = package.encode_binary().unwrap();
    assert_eq!(FslcPackage::decode_binary(&bytes).unwrap(), package);
    for version in 1..15 {
        let mut old = package.clone();
        old.version = version;
        assert!(old.encode_binary().is_err());
        let mut old_bytes = bytes.clone();
        old_bytes[8..10].copy_from_slice(&version.to_le_bytes());
        assert!(FslcPackage::decode_binary(&old_bytes).is_err());
    }
    let legacy = profile("%a:u8=stack.pop; %b:u8=stack.pop; %v:u8=u8.sub %a,%b; stack.push %v;");
    assert_eq!(legacy.version, 9);
    let legacy_quantum = compile_source(include_str!("../specs/quantum-circuits.fsl")).unwrap();
    assert_eq!(legacy_quantum.version, 12);
    assert_eq!(AXES.len(), 8);
    let mut ids: Vec<_> = AXES.iter().map(|axis| axis.id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 8);
}

#[test]
fn kernels_use_one_body_for_reference_hdl_and_dependencies() {
    let package = compile_source(SOURCE).unwrap();
    let examples: &[(&str, &[u64], &[u64])] = &[
        ("fpga_add", &[250, 10], &[4]),
        ("dsp_sat_add", &[250, 10], &[255]),
        ("tensor_dot2", &[2, 3, 5, 7], &[31]),
        ("asic_mac", &[65535, 2, 7], &[5]),
        ("lif_step", &[80, 50], &[0, 1]),
        ("pim_bank_xor", &[0x1234, 0xffff], &[0xedcb]),
        ("rc_euler_step", &[256, 512], &[320]),
        ("dataflow_diamond", &[10, 3], &[91]),
    ];
    for &(name, inputs, expected) in examples {
        let body = package
            .instructions
            .iter()
            .find(|body| body.name == name)
            .unwrap();
        let plan = FabricPlan::new(body).unwrap();
        assert_eq!(plan.evaluate(inputs).unwrap(), expected);
        assert_eq!(plan.input_slots(), inputs.len());
        assert_eq!(plan.output_slots(), expected.len());
        let hdl = plan.emit_verilog("fsl_example").unwrap();
        let dot = plan.emit_dot("fsl_example").unwrap();
        assert!(hdl.contains("module fsl_example("));
        assert!(dot.contains("digraph fsl_example"));
        assert!(plan.emit_verilog("module").is_err());
        assert!(plan.emit_dot("fsl_bad;injection").is_err());
        assert!(plan.evaluate(&[]).is_err());
        for layer in [OutputLayer::C, OutputLayer::Rust] {
            assert!(emit_instruction(body, layer, "fsl_example").is_ok());
        }
    }
}

#[test]
fn multiply_and_logical_shifts_mask_width_and_large_counts() {
    for bits in [1u16, 8, 16, 32, 64] {
        let mask = ((1u128 << bits) - 1) as u64;
        for operation in ["mul.wrap", "shl", "lshr"] {
            let package = profile(&format!("%b:u{bits}=stack.pop; %a:u{bits}=stack.pop; %v:u{bits}=u{bits}.{operation} %a,%b; stack.push %v;"));
            let plan = FabricPlan::new(&package.instructions[0]).unwrap();
            for left in [0, 1, 3, 1u64 << 63, u64::MAX] {
                for right in [0, 1, 7, 8, 31, 32, 63, 64, 65, 255, u64::MAX] {
                    let (a, b) = (left & mask, right & mask);
                    let expected = match operation {
                        "mul.wrap" => ((u128::from(a) * u128::from(b)) & u128::from(mask)) as u64,
                        "shl" if b < u64::from(bits) => {
                            (((u128::from(a)) << b) & u128::from(mask)) as u64
                        }
                        "lshr" if b < u64::from(bits) => a >> b,
                        _ => 0,
                    };
                    assert_eq!(
                        plan.evaluate(&[left, right]).unwrap(),
                        vec![expected],
                        "{bits} {operation} {left} {right}"
                    );
                }
            }
        }
    }
    for operation in ["shl", "lshr"] {
        let source = format!("language bad {{ byte_order little; address_unit byte; instruction bad {{ opcode 1; mnemonic \"bad\"; evidence \"self\" \"synthetic\" \"v1\" \"bad\"; semantics {{ %a:i8=stack.pop; %b:i8=stack.pop; %x:i8=i8.{operation} %a,%b; stack.push %x; }} }} }}");
        assert!(compile_source(&source).is_err());
    }
}

#[test]
fn fabric_refuses_state_quantum_cfg_and_unsupported_effects() {
    for semantics in [
        "quantum.h 0;",
        "%v:u1=flag.read 0; flag.write 0,%v;",
        "block entry() { %v:u8=int.const 1; stack.push %v; return; }",
        "%a:u64=stack.pop; %v:u8=memory.load.le %a; stack.push %v;",
    ] {
        let package = profile(semantics);
        assert!(FabricPlan::new(&package.instructions[0]).is_err());
    }
    let mut package = profile("%v:u8=stack.pop; stack.push %v;");
    package.instructions[0].ops = vec![fission_fsl::FirOp::Unsupported];
    package.instructions[0].values.clear();
    assert!(FabricPlan::new(&package.instructions[0]).is_err());
}
