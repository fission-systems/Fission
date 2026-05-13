use std::collections::{BTreeMap, BTreeSet};

use super::lowering::Collector;
use super::*;
use crate::compiler::{
    compile_frontend_for_entry_spec, discovery, spec_root_for_arch, x86_64_entry_spec_path,
};
use crate::packed_context::packed_context_word;

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
    assert!(!compiled.language_layout.token_fields.is_empty());
    assert!(!compiled.language_layout.context_fields.is_empty());
    assert!(
        compiled
            .language_layout
            .token_fields
            .iter()
            .all(|field| field.bit_width > 0),
        "token field layout must be sourced from define token ranges, not zero-filled"
    );
    assert!(
        compiled
            .language_layout
            .context_fields
            .iter()
            .all(|field| field.bit_width > 0),
        "context field layout must be sourced from define context ranges, not zero-filled"
    );
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
fn arm_frontends_preserve_thumb_context_layout_and_pspec_defaults() {
    if !discovery::ghidra_packaged_sla_available() {
        eprintln!("skip: packaged Ghidra .sla not available for ARM context layout check");
        return;
    }

    for entry_id in ["ARM8_le", "ARM8m_le", "ARM8m_be"] {
        let entry_spec = spec_root_for_arch("ARM").join(format!("{entry_id}.slaspec"));
        let compiled = compile_frontend_for_entry_spec(&entry_spec).expect("compile ARM frontend");
        for name in ["TMode", "T", "ISA_MODE", "LowBitCodeMode"] {
            assert!(
                compiled
                    .language_layout
                    .context_fields
                    .iter()
                    .any(|field| field.name == name && field.bit_width == 1),
                "{entry_id} must preserve ARM Thumb context field {name}"
            );
        }

        if entry_id.starts_with("ARM8m_") {
            let low_word =
                packed_context_word(compiled.default_context, 0).expect("default context low word");
            assert_ne!(
                low_word & 0x8000_0000,
                0,
                "{entry_id} must apply ARMCortex.pspec TMode=1 default context"
            );
            let known_low_word = packed_context_word(compiled.default_context_known_mask, 0)
                .expect("default context known-mask low word");
            assert_ne!(
                known_low_word & 0x8000_0000,
                0,
                "{entry_id} must mark ARMCortex.pspec TMode as known"
            );
        }
    }
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
        "classify_display_construct_kind",
        "normalize_executable_mnemonic",
    ] {
        assert!(
            !lowering.contains(forbidden),
            "manual .sla overlay/remap path must not re-enter canonical lowering: {forbidden}"
        );
    }
}

#[test]
fn compiler_lowering_diagnostics_are_trace_gated() {
    let lowering = include_str!("lowering.rs");
    let diagnostic = "Inferred Default Context for";
    assert!(
        lowering.contains("FISSION_TRACE_CONTEXT_DEFAULT"),
        "default context diagnostics must stay available behind an explicit trace flag"
    );
    assert!(
        !lowering.contains(&format!("eprintln!(\n        \"{diagnostic}")),
        "default context diagnostics must not be emitted unconditionally"
    );
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
    let mut total = 0usize;
    let mut runtime_ready = 0usize;
    let mut unsupported = Vec::new();
    for constructor in compiled
        .subtables
        .values()
        .flat_map(|subtable| subtable.constructors.iter())
    {
        total += 1;
        assert!(
            constructor.sla_identity.is_some(),
            "packaged .sla frontends must not retain non-.sla native constructors: {}",
            constructor.source
        );
        if constructor.runtime_ready {
            runtime_ready += 1;
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
        } else {
            unsupported.push((
                constructor.source.clone(),
                constructor.unsupported_template_kind.clone(),
            ));
        }
    }
    assert!(
        total > 0,
        "packaged x86-64 .sla should provide canonical constructors"
    );
    assert!(
        unsupported.is_empty(),
        "packaged x86-64 .sla constructors must all be runtime-ready: {:?}",
        unsupported
    );
    assert_eq!(
        runtime_ready, total,
        "all packaged x86-64 .sla constructors should be runtime-ready"
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
    for forbidden in ["mod_constraint", "operand_reg_values"] {
        assert!(
            !include_str!("types.rs").contains(forbidden),
            "CompiledExecutableConstructor must not expose legacy token selector field {forbidden}"
        );
    }

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
        }
        assert!(
            runtime_ready > 0,
            "packaged {entry_id} .sla should provide runtime-ready canonical constructors"
        );
    }
}

#[test]
fn legacy_source_context_changes_preserve_high_context_words() {
    if !discovery::ghidra_packaged_sla_available() {
        eprintln!("skip: packaged Ghidra .sla not available for JVM context-word check");
        return;
    }

    let entry_spec = spec_root_for_arch("JVM").join("JVM.slaspec");
    let compiled = compile_frontend_for_entry_spec(&entry_spec).expect("compile JVM frontend");
    let high_word_change = compiled
        .constructors
        .iter()
        .flat_map(|constructor| &constructor.context_changes)
        .find(|change| change.word_index == 3 && change.mask == 0x3000_0000)
        .expect("JVM in_lookup_switch context change should preserve word 3 mask");
    assert_eq!(high_word_change.bit_offset, 98);
    assert_eq!(high_word_change.bit_width, 2);
}

#[test]
fn legacy_spec_matcher_lowering_does_not_synthesize_zero_on_parse_failure() {
    let lowering = include_str!("lowering.rs");
    for forbidden in [
        "from_str_radix(&value_str[2..], 16).unwrap_or(0)",
        "from_str_radix(&value_str[2..], 2).unwrap_or(0)",
        "value_str.parse::<u64>().unwrap_or(0)",
        "from_str_radix(&val_str[2..], 16).unwrap_or(0)",
        "val_str.parse::<u64>().unwrap_or(0)",
        "unwrap_or_else(|| CompiledPatternMatcher::BitConstraints(vec![]))",
        "native_matcher_minimum_length(&matcher) as u32",
        ".max()\n            .unwrap_or(0)",
        "start_str.trim().parse::<u32>().unwrap_or(0)",
        "end_str.trim().parse::<u32>().unwrap_or(0)",
        "value.checked_shl(info.bit_offset).unwrap_or(0)",
        "checked_shl(info.bit_offset)\n                    .unwrap_or(0)",
        "value.checked_shl(end_bit).unwrap_or(0)",
        ".operand_minimum_lengths\n                        .get(operand_index)\n                        .copied()\n                        .unwrap_or(0)",
        "info.map(|i| i.bit_offset).unwrap_or(0)",
        "info.map(|i| i.bit_width).unwrap_or(0)",
        ".map(|constructor| constructor.subtable_id)\n                    .next()\n                    .unwrap_or(0)",
        ".unwrap_or_else(|| CompiledDecisionTree",
        "score = score.saturating_sub(500)",
    ] {
        assert!(
            !lowering.contains(forbidden),
            "legacy matcher lowering must fail closed instead of synthesizing zero: {forbidden}"
        );
    }
}

#[test]
fn legacy_operand_lowering_does_not_synthesize_unknown_subtable() {
    let lowering = include_str!("lowering.rs");
    assert!(
        !lowering.contains("table_name: \"unknown\".to_string()"),
        "legacy operand lowering must fail closed instead of creating an unknown subtable"
    );
}

#[test]
fn compiled_frontend_space_helpers_do_not_guess_pointer_width() {
    let types = include_str!("types.rs");

    assert!(
        !types.contains(".unwrap_or(8)"),
        "compiled frontend space helpers must use decoded SLA addr_size, not a 64-bit fallback"
    );
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
            !source.contains("TokenFieldExtraction") && !source.contains("token_field_extraction"),
            "{name} still exposes compatibility token-field extraction"
        );
        for forbidden in [
            "CompiledFixedRegister",
            "FixedRegister",
            "fixed_register",
            "RelativeAddress",
            "relative_address",
            "CompiledOperandSpec::Immediate",
            "CompiledOperandSpec::Relative",
            "CompiledConstructTplKind::Call",
            "CompiledConstructTplKind::Jmp",
            "CompiledConstructTplKind::Ret",
            "CompiledConstructTplKind::Mov",
        ] {
            assert!(
                !source.contains(forbidden),
                "{name} still exposes legacy non-SLA operand spec: {forbidden}"
            );
        }
    }
}

#[test]
fn generated_native_backend_does_not_zero_pad_short_instruction_bytes() {
    let codegen = include_str!("../codegen.rs");
    for forbidden in [
        "bytes.get({offset}).copied().unwrap_or(0)",
        "bytes.get(i as usize).unwrap_or(&0)",
    ] {
        assert!(
            !codegen.contains(forbidden),
            "generated native backend must fail closed on short instruction bytes: {forbidden}"
        );
    }
    assert!(
        codegen.contains("if bytes.len() < byte_cnt as usize { return -1; }"),
        "generated native backend should reject short decision probes instead of padding"
    );
}

#[test]
fn sla_template_parser_does_not_promote_named_sections_to_main() {
    let templates = include_str!("../sla/templates.rs");
    for forbidden in [
        "constructor.children.iter().find(|child| child.id == sla_format::ELEM_CONSTRUCT_TPL)",
        "if no section-less template exists",
    ] {
        assert!(
            !templates.contains(forbidden),
            "SLA parser must fail closed when the main ConstructTpl is missing, not promote a named section: {forbidden}"
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
