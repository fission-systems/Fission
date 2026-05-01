use std::collections::{BTreeMap, BTreeSet};

use super::lowering::Collector;
use super::*;
use crate::compiler::{compile_frontend_for_entry_spec, x86_64_entry_spec_path};

#[test]
fn compile_frontend_collects_pcode_ops_and_patterns() {
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
