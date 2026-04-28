use super::*;
use crate::compiler::{
    compile_frontend_for_entry_spec, expand_entry_spec, infer_arch_from_entry_spec,
    parse_expanded_spec, x86_64_entry_spec_path,
};

#[test]
fn compile_frontend_collects_pcode_ops_and_patterns() {
    let entry_spec = x86_64_entry_spec_path();
    let expanded = expand_entry_spec(&entry_spec).expect("expand spec");
    let ast_result = parse_expanded_spec(&expanded);
    let arch = infer_arch_from_entry_spec(&entry_spec).expect("infer arch");
    let compiled =
        compile_frontend(&arch, &expanded, ast_result, &entry_spec).expect("compile frontend");
    assert!(!compiled.pcode_ops.is_empty());
    assert!(!compiled.pattern_nodes.is_empty());
    assert!(compiled
        .constructors
        .iter()
        .any(|item| item.mnemonic.eq_ignore_ascii_case("RET")
            || item.control_flow != ControlFlowClass::None));
    assert!(!compiled.language_layout.address_spaces.is_empty());
    assert!(!compiled.language_layout.registers.is_empty());
    assert!(!compiled.language_layout.display_templates.is_empty());
    assert!(!compiled.construct_templates.is_empty());
    assert!(compiled
        .subtables
        .get("instruction")
        .unwrap()
        .decision_tree
        .nodes
        .iter()
        .any(|node| matches!(node.probe, CompiledDecisionProbe::ContextBitSlice { .. })));
}

#[test]
fn test_parse_aarch64_token_definition() {
    let mut collector = Collector {
        definitions: Vec::new(),
        macros: Vec::new(),
        constructors: Vec::new(),
        subtable_executables: BTreeMap::new(),
        pcode_ops: BTreeSet::new(),
        pcode_op_sources: BTreeMap::new(),
        default_context: 0,
        pattern_nodes: Vec::new(),
        field_info: BTreeMap::new(),
    };
    collector.parse_define_bits(
        "define token instrAARCH64 (32) endian = little Rm = (16,20) Rn = (5,9) sf = (31,31);",
        "token",
    );
    assert_eq!(collector.field_info.get("Rm").unwrap().bit_offset, 16);
    assert_eq!(collector.field_info.get("sf").unwrap().bit_offset, 31);
}
