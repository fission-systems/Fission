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
    assert!(
        compiled
            .subtables
            .values()
            .flat_map(|subtable| subtable.constructors.iter())
            .any(|item| item.mnemonic.eq_ignore_ascii_case("RET"))
    );
    assert!(!compiled.language_layout.address_spaces.is_empty());
    assert!(!compiled.language_layout.registers.is_empty());
    assert!(!compiled.language_layout.display_templates.is_empty());
    assert!(!compiled.construct_templates.is_empty());
    assert!(
        compiled
            .subtables
            .get("instruction")
            .unwrap()
            .decision_tree
            .nodes
            .iter()
            .any(|node| {
                matches!(
                    node.probe,
                    CompiledDecisionProbe::ContextBitSlice { .. }
                        | CompiledDecisionProbe::SlaInstructionBits { .. }
                        | CompiledDecisionProbe::SlaContextBits { .. }
                )
            }),
        "instruction decision tree should carry spec-derived or SLA-native probes"
    );
}

#[test]
fn sla_construct_template_cutover_has_no_source_line_or_opprint_remap_overlay() {
    let lowering = concat!(
        include_str!("lowering/compile_and_collector_type.rs"),
        include_str!("lowering/collector_impl.rs"),
        include_str!("lowering/lowering_helpers.rs"),
    );
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
