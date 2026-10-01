use fission_fsl::{
    compile_source, emit_aot_object, emit_instruction, execute_instruction, FslcPackage,
    JitDecoder, OutputLayer,
};

fn gpu() -> FslcPackage {
    compile_source(include_str!("../specs/amdgcn-gfx900.fsl")).unwrap()
}

#[test]
fn gpu_decode_roundtrip_and_edits_preserve_encoding() {
    let package = gpu();
    let bytes = [1, 5, 0, 0x68];
    let decoded = package
        .decode_bytes("amdgcn.gfx900", &bytes)
        .unwrap()
        .unwrap();
    assert_eq!(
        decoded.fields,
        vec![
            ("source0".into(), 257),
            ("source1".into(), 2),
            ("destination".into(), 0)
        ]
    );
    assert_eq!(package.reencode(&decoded, &[]).unwrap(), bytes);
    assert_eq!(
        package.reencode(&decoded, &[("destination", 3)]).unwrap(),
        [1, 5, 6, 0x68]
    );
    let binary = package.encode_binary().unwrap();
    let restored = FslcPackage::decode_binary(&binary).unwrap();
    assert_eq!(
        restored.decode_bytes("amdgcn.gfx900", &bytes).unwrap(),
        Some(decoded)
    );
    assert_eq!(restored.encode_binary().unwrap(), binary);
}

#[test]
fn unknown_profiles_truncation_extensions_and_invalid_edits_fail() {
    let package = gpu();
    assert!(package.decode_bytes("wrong", &[1, 5, 0, 0x68]).is_err());
    assert!(package.decode_bytes("amdgcn.gfx900", &[1, 5, 0]).is_err());
    assert!(package
        .decode_bytes("amdgcn.gfx900", &[0; 4])
        .unwrap()
        .is_none());
    for selector in [249u16, 250, 255] {
        let word = 0x6800_0400u32 | u32::from(selector);
        assert!(package
            .decode_bytes("amdgcn.gfx900", &word.to_le_bytes())
            .unwrap()
            .is_none());
    }
    let decoded = package
        .decode_bytes("amdgcn.gfx900", &[1, 5, 0, 0x68])
        .unwrap()
        .unwrap();
    for edits in [
        vec![("source0", 255)],
        vec![("destination", 256)],
        vec![("missing", 1)],
        vec![("destination", 1), ("destination", 2)],
    ] {
        assert!(package.reencode(&decoded, &edits).is_err());
    }
    let barrier = package
        .decode_bytes("amdgcn.gfx900", &[0, 0, 0x8a, 0xbf])
        .unwrap()
        .unwrap();
    assert!(package.reencode(&barrier, &[("raw_imm16", 1)]).is_err());
}

#[test]
fn foreign_or_tampered_observations_fail_before_reencoding() {
    let package = gpu();
    let decoded = package
        .decode_bytes("amdgcn.gfx900", &[1, 5, 0, 0x68])
        .unwrap()
        .unwrap();
    let mut tampered = decoded.clone();
    tampered.fields[0].1 = 1;
    assert!(package.reencode(&tampered, &[]).is_err());
    tampered = decoded.clone();
    tampered.instruction_index = usize::MAX;
    assert!(package.reencode(&tampered, &[]).is_err());
    tampered = decoded;
    tampered.raw[0] = 2;
    assert!(package.reencode(&tampered, &[]).is_err());
}

#[test]
fn decoded_gpu_semantics_cannot_execute_or_emit_native_code() {
    let package = gpu();
    for instruction in &package.instructions {
        let mut state = vec![17, 23];
        assert!(execute_instruction(instruction, &mut state, 2).is_err());
        assert_eq!(state, vec![17, 23]);
        assert!(emit_instruction(instruction, OutputLayer::Fir, "f")
            .unwrap()
            .contains("unsupported semantics"));
        for layer in [OutputLayer::C, OutputLayer::Rust] {
            assert!(emit_instruction(instruction, layer, "f").is_err());
        }
    }
    assert!(JitDecoder::compile(&package).is_err());
    assert!(emit_aot_object(&package).is_err());
}

#[test]
fn full_width_values_roundtrip_with_both_byte_orders() {
    for bits in [32u16, 64, 128] {
        for order in ["little", "big"] {
            let source = format!("language wide {{ byte_order {order}; address_unit byte; instruction sample {{ encoding {bits} mask 0x80000000 value 0x80000000 {{ field low offset 0 bits 8; }} mnemonic \"sample\"; evidence \"fixture\" \"synthetic\" \"v1\" \"encoding contract\"; semantics unsupported; }} }}");
            let package = compile_source(&source).unwrap();
            let word = u128::MAX;
            let raw = if order == "little" {
                word.to_le_bytes()[..usize::from(bits / 8)].to_vec()
            } else {
                word.to_be_bytes()[16 - usize::from(bits / 8)..].to_vec()
            };
            let decoded = package.decode_bytes("wide", &raw).unwrap().unwrap();
            assert_eq!(package.reencode(&decoded, &[]).unwrap(), raw);
            let edited = package.reencode(&decoded, &[("low", 3)]).unwrap();
            let observed = package.decode_bytes("wide", &edited).unwrap().unwrap();
            assert_eq!(observed.fields, vec![("low".into(), 3)]);
            let index = if order == "little" { 0 } else { raw.len() - 1 };
            for i in 0..raw.len() {
                assert_eq!(edited[i], if i == index { 3 } else { raw[i] });
            }
        }
    }
}
