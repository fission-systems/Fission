use fission_fsl::{compile_source, emit_instruction, FirOp, FslcPackage, OutputLayer, ValueId};

fn instruction(ty: &str, body: &str) -> fission_fsl::CompiledInstruction {
    let source = format!("language gpu.projection {{ byte_order little; address_unit byte; instruction sample {{ opcode 0x60; mnemonic \"sample\"; evidence \"synthetic\" \"fixture\" \"v1\" \"projection contract\"; semantics {{ {} }} }} }}", body.replace("TYPE", ty));
    compile_source(&source).unwrap().instructions.remove(0)
}

#[test]
fn gpu_outputs_consume_the_same_packaged_fir_and_widths() {
    let package = compile_source(include_str!("../specs/jvm-se26-iadd.fsl")).unwrap();
    let bytes = package.encode_binary().unwrap();
    let roundtrip = FslcPackage::decode_binary(&bytes).unwrap();
    for layer in [OutputLayer::CudaCpp, OutputLayer::Ptx] {
        assert_eq!(
            emit_instruction(&package.instructions[0], layer, "fsl_execute").unwrap(),
            emit_instruction(&roundtrip.instructions[0], layer, "fsl_execute").unwrap()
        );
    }
    for bits in [1, 8, 16, 32, 64] {
        for sign in ["i", "u"] {
            let sample = instruction(&format!("{sign}{bits}"), "%a: TYPE = stack.pop; %b: TYPE = stack.pop; %sum: TYPE = TYPE.add.wrap %a, %b; stack.push %sum;");
            let mask = if bits == 64 {
                u64::MAX
            } else {
                (1u64 << bits) - 1
            };
            let cuda = emit_instruction(&sample, OutputLayer::CudaCpp, "fsl_execute").unwrap();
            let ptx = emit_instruction(&sample, OutputLayer::Ptx, "fsl_execute").unwrap();
            assert!(cuda.contains(&format!("(v0 + v1) & 0x{mask:x}ULL")));
            assert!(ptx.contains(&format!("and.b64 %v2, %v2, 0x{mask:x}")));
            assert!(ptx.contains("add.u64 %v2, %v0, %v1"));
            assert!(ptx.contains(".version 7.0\n.target sm_70\n.address_size 64"));
            for axis in ["ctaid.x", "ctaid.y", "ctaid.z", "tid.x", "tid.y", "tid.z"] {
                assert!(cuda.contains(&format!("%%{axis}")));
                assert!(ptx.contains(&format!("%{axis}")));
            }
        }
    }
}

#[test]
fn gpu_contract_checks_peak_capacity_before_any_state_effects() {
    let sample = instruction("u32", "%a: TYPE = stack.pop; stack.push %a; stack.push %a;");
    let cuda = emit_instruction(&sample, OutputLayer::CudaCpp, "fsl_execute").unwrap();
    let ptx = emit_instruction(&sample, OutputLayer::Ptx, "fsl_execute").unwrap();
    assert!(cuda.find("if (1 > capacity - sp)").unwrap() < cuda.find("stack[--sp]").unwrap());
    assert!(
        ptx.find("setp.lt.u64 %p, %remaining, 1").unwrap() < ptx.find("ld.global.u64 %v0").unwrap()
    );
    assert!(cuda.contains("if (owner != 0 || status == nullptr) return"));
    assert!(ptx.contains("setp.ne.u32 %p, %owner, 0;\n    @%p bra DONE"));
    let pop = instruction("u8", "%a: TYPE = stack.pop;");
    assert!(emit_instruction(&pop, OutputLayer::Ptx, "fsl_execute").is_ok());
}

#[test]
fn gpu_outputs_refuse_unmodeled_effects_invalid_ssa_and_wide_values() {
    let bodies = [
        instruction("u128", "%a: TYPE = stack.pop; stack.push %a;"),
        compile_source(include_str!("../specs/amdgcn-gfx900-sadd-u32.fsl"))
            .unwrap()
            .instructions
            .remove(0),
        compile_source(include_str!("../specs/amdgcn-gfx900-vadd-u32-wave64.fsl"))
            .unwrap()
            .instructions
            .remove(0),
        compile_source(include_str!("../specs/amdgcn-gfx900.fsl"))
            .unwrap()
            .instructions
            .remove(0),
    ];
    for body in &bodies {
        for layer in [OutputLayer::CudaCpp, OutputLayer::Ptx] {
            assert!(emit_instruction(body, layer, "fsl_execute").is_err());
        }
    }
    let mut invalid = instruction("u32", "%a: TYPE = stack.pop; stack.push %a;");
    invalid.ops[1] = FirOp::VmStackPush { value: ValueId(99) };
    for layer in [OutputLayer::CudaCpp, OutputLayer::Ptx] {
        assert!(emit_instruction(&invalid, layer, "fsl_execute").is_err());
        assert!(emit_instruction(&bodies[0], layer, "bad\nsymbol").is_err());
    }
}
