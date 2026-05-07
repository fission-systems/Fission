use std::collections::{BTreeMap, BTreeSet};

use super::lowering::Collector;
use super::*;
use crate::compiler::{
    compile_frontend_for_entry_spec, discovery, spec_root_for_arch, x86_64_entry_spec_path,
};

#[test]
fn compile_frontend_collects_pcode_ops_and_patterns() {
    if !discovery::ghidra_packaged_sla_available() {
        eprintln!("skip: packaged Ghidra .sla not available for ConstructTpl decode check");
        return;
    }
    let entry_spec = x86_64_entry_spec_path();
    let compiled = compile_frontend_for_entry_spec(&entry_spec).expect("compile frontend");
    assert!(
        compiled
            .construct_templates
            .iter()
            .any(|template| !template.ops.is_empty()),
        "compiled frontend should preserve decoded .sla ConstructTpl ops"
    );
    assert!(!compiled.pattern_nodes.is_empty());
    assert!(compiled
        .subtables
        .values()
        .flat_map(|subtable| subtable.constructors.iter())
        .any(|item| item.mnemonic.eq_ignore_ascii_case("RET")));
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
        .any(|node| matches!(
            node.probe,
            CompiledDecisionProbe::SlaInstructionBits { .. }
                | CompiledDecisionProbe::SlaContextBits { .. }
        )));
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

#[test]
fn sla_construct_template_cutover_has_no_source_line_or_opprint_remap_overlay() {
    let lowering = include_str!("lowering.rs");
    for forbidden in [
        "apply_sla_construct_templates",
        "remap_op_tpl_handles",
        "remap_build_operand_indices",
        "remap_display_template_operands",
        "source == sla_template.source_key",
        "rsplit(':')",
        "unsupported_placeholder",
    ] {
        assert!(
            !lowering.contains(forbidden),
            "manual .sla overlay/remap path must not re-enter canonical lowering: {forbidden}"
        );
    }
}

#[test]
fn sla_native_runtime_ready_constructors_are_canonical() {
    if !discovery::ghidra_packaged_sla_available() {
        eprintln!("skip: packaged Ghidra .sla not available for runtime-ready constructor check");
        return;
    }
    let entry_spec = x86_64_entry_spec_path();
    let compiled = crate::compiler::compile_frontend_for_entry_spec(&entry_spec)
        .expect("compile x86-64 frontend with packaged .sla");
    let mut runtime_ready = 0usize;
    for constructor in compiled
        .subtables
        .values()
        .flat_map(|subtable| subtable.constructors.iter())
    {
        if constructor.runtime_ready {
            runtime_ready += 1;
            assert!(
                constructor.sla_identity.is_some(),
                "runtime-ready constructor must be selected by .sla native identity: {}",
                constructor.source
            );
            assert_eq!(
                constructor.constructor_template.template_source,
                CompiledTemplateSource::SpecDerived
            );
            assert!(
                constructor
                    .constructor_template
                    .ghidra_template_shape_error()
                    .is_none(),
                "runtime-ready .sla constructor contains non-canonical template shape: {}",
                constructor.source
            );
        } else if constructor.sla_identity.is_none() {
            assert!(
                constructor.unsupported_template_kind.is_some(),
                "non-.sla constructors cannot become canonical runtime successes"
            );
        }
    }
    assert!(
        runtime_ready > 0,
        "packaged x86-64 .sla should provide runtime-ready canonical constructors"
    );
}

#[test]
fn runtime_ready_constructors_do_not_depend_on_compat_token_selectors() {
    if !discovery::ghidra_packaged_sla_available() {
        eprintln!("skip: packaged Ghidra .sla not available for token parser dependency check");
        return;
    }

    let legacy_decode_step = ["Consume", "TokenFields"].concat();
    assert!(
        !include_str!("types.rs").contains(&legacy_decode_step),
        "CompiledOperandDecodeStep must not expose legacy token-field decode variants"
    );

    let entry_specs = [
        ("x86-64", x86_64_entry_spec_path()),
        ("x86", spec_root_for_arch("x86").join("x86.slaspec")),
    ];

    for (entry_id, entry_spec) in entry_specs {
        let compiled = compile_frontend_for_entry_spec(&entry_spec)
            .unwrap_or_else(|error| panic!("compile {entry_id} frontend: {error:#}"));
        let mut runtime_ready = 0usize;
        for constructor in compiled
            .subtables
            .values()
            .flat_map(|subtable| subtable.constructors.iter())
            .filter(|constructor| constructor.runtime_ready)
        {
            runtime_ready += 1;
            assert!(
                constructor.mod_constraint.is_none(),
                "{entry_id} runtime-ready constructor still uses legacy mod selector: {}",
                constructor.source
            );
            assert!(
                constructor.operand_reg_values.is_empty(),
                "{entry_id} runtime-ready constructor still uses legacy reg selector: {}",
                constructor.source
            );
        }
        assert!(
            runtime_ready > 0,
            "packaged {entry_id} .sla should provide runtime-ready canonical constructors"
        );
    }
}

#[test]
fn compiled_operand_specs_have_no_compat_token_extraction_variant() {
    let types = include_str!("types.rs");
    let lowering = include_str!("lowering.rs");
    let codegen = include_str!("../codegen.rs");
    for (name, source) in [
        ("types.rs", types),
        ("lowering.rs", lowering),
        ("codegen.rs", codegen),
    ] {
        assert!(
            !source.contains("TokenFieldExtraction")
                && !source.contains("token_field_extraction"),
            "{name} still exposes compatibility token-field extraction"
        );
    }
}

#[test]
fn sla_operand_minimum_lengths_are_preserved_on_handle_templates() {
    if !discovery::ghidra_packaged_sla_available() {
        eprintln!("skip: packaged Ghidra .sla not available for operand minlen check");
        return;
    }
    let compiled = compile_frontend_for_entry_spec(&x86_64_entry_spec_path())
        .expect("compile x86-64 frontend");
    let mut handles = 0usize;
    let mut nonzero_minlen = 0usize;
    for constructor in compiled
        .subtables
        .values()
        .flat_map(|subtable| subtable.constructors.iter())
        .filter(|constructor| constructor.runtime_ready)
    {
        assert_eq!(
            constructor.constructor_template.handles.len(),
            constructor.operand_specs.len(),
            "runtime-ready constructor handle/spec count mismatch: {}",
            constructor.source
        );
        for handle in &constructor.constructor_template.handles {
            handles += 1;
            if handle.minimum_length > 0 {
                nonzero_minlen += 1;
            }
        }
    }
    assert!(handles > 0, "expected runtime-ready operand handles");
    assert!(
        nonzero_minlen > 0,
        "packaged x86-64 .sla should preserve nonzero OperandSymbol.minimumlength values"
    );
}
