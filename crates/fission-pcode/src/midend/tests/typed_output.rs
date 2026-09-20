use super::*;

#[test]
fn typed_render_output_owns_all_pipeline_observations() {
    let result = uniq(0x200, 8);
    let func = PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x2000,
            successors: vec![],
            ops: vec![
                PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::IntSRem,
                    address: 0x2000,
                    output: Some(result.clone()),
                    inputs: vec![reg(0x08, 8), cst(2, 8)],
                    asm_mnemonic: None,
                },
                PcodeOp {
                    seq_num: 1,
                    opcode: PcodeOpcode::Return,
                    address: 0x2001,
                    output: None,
                    inputs: vec![cst(0, 8), result],
                    asm_mnemonic: None,
                },
            ],
        }],
    };
    let options = preview_options();

    let legacy_code = render_nir_with_context(&func, "mod_ll", 0x2000, &options, None, None)
        .expect("legacy preview render");

    let output = render_nir_with_context_output(&func, "mod_ll", 0x2000, &options, None, None)
        .expect("typed preview render");
    let layered = output.layered.as_ref().expect("layered output");

    assert_eq!(output.code, layered.nir);
    assert_eq!(legacy_code, output.code);
    assert!(output.raw_hir.is_some());
    assert!(output.prehir.is_some());
    assert!(output.hir_function.is_some());
    assert!(output.recovered_variables.is_some());
    assert!(output.build_stats.is_some());
}
