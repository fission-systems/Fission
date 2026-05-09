use std::collections::BTreeMap;

use super::*;
use crate::compiler::{CompiledOperandSpec, CompiledSlaDecodeStatus};
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::Write;

fn packaged_sla_path(processor: &str, name: &str) -> Option<PathBuf> {
    crate::compiler::resolve_ghidra_install_paths().map(|paths| {
        paths
            .processors_root
            .join(processor)
            .join("data")
            .join("languages")
            .join(name)
    })
}

#[test]
fn decodes_ghidra_sla_header_and_zlib_payload() {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(b"template-payload").unwrap();
    let compressed = encoder.finish().unwrap();
    let mut artifact = Vec::from(&b"sla\x04"[..]);
    artifact.extend(compressed);

    let decoded = decode_compiled_sla(PathBuf::from("x86-64.sla"), &artifact).unwrap();
    assert_eq!(decoded.version, 4);
    assert_eq!(decoded.payload, b"template-payload");
}

#[test]
fn rejects_non_sla_artifact() {
    let err = decode_compiled_sla(PathBuf::from("x86-64.slaspec"), b"not-sla")
        .expect_err("slaspec text must not be treated as compiled SLEIGH");
    assert!(err.to_string().contains("missing sla magic"));
}

#[test]
fn decodes_real_x86_64_sla_construct_templates() {
    let Some(path) = packaged_sla_path("x86", "x86-64.sla") else {
        return;
    };
    if !path.exists() {
        return;
    }
    let library = load_construct_templates_from_sla(path).expect("decode x86-64.sla");
    assert!(!library.source_files.is_empty());
    assert!(!library.spaces.is_empty());
    assert!(!library.constructors_by_source.is_empty());
    assert!(!library.native.subtables.is_empty());
    let instruction = library
        .native
        .subtables
        .get("instruction")
        .expect("native instruction subtable");
    assert!(
        instruction.decision_tree.is_some(),
        "native instruction subtable must preserve decision root"
    );
    assert!(
        instruction
            .constructors
            .iter()
            .any(|ctor| !ctor.construct_tpl.ops.is_empty() || ctor.construct_tpl.result.is_some()),
        "native constructors must preserve decoded ConstructTpl payloads"
    );
}

#[test]
fn sla_template_decoder_does_not_synthesize_unknown_subtables() {
    let templates = include_str!("templates.rs");
    assert!(
        !templates.contains("unknown_subtable_"),
        "compiled .sla subtable decoding must fail closed instead of synthesizing subtable names"
    );
}

#[test]
fn sla_decision_decode_does_not_synthesize_always_true_patterns() {
    let templates = include_str!("templates.rs");
    assert!(
        !templates.contains("always_true_instruction_pattern"),
        "compiled .sla decision decode must fail closed instead of synthesizing match-all leaf patterns"
    );
    assert!(
        !templates.contains("decode_decision_tree(id, child).ok()"),
        "compiled .sla decision decode errors must not be silently dropped"
    );
}

#[test]
fn sla_named_section_decode_errors_are_not_dropped() {
    let templates = include_str!("templates.rs");
    assert!(
        !templates.contains("decode named section")
            || templates.contains("decode_named_construct_tpl"),
        "named ConstructTpl decode failures must be surfaced, not only traced"
    );
}

#[test]
fn sla_symbol_pattern_expression_decode_errors_are_not_dropped() {
    let symbols = include_str!("symbols.rs");
    let display = include_str!("display.rs");
    for (name, source) in [("symbols.rs", symbols), ("display.rs", display)] {
        assert!(
            !source.contains("decode_pattern_expression(child).ok()"),
            "{name} must not silently drop malformed SLA pattern expressions"
        );
    }
}

#[test]
fn sla_display_symbol_decode_does_not_synthesize_defaults() {
    let display = include_str!("display.rs");
    for forbidden in [
        "unwrap_or_default()",
        "let Some(space_index)",
        "let Some(name)",
        "let Some(space)",
        "filter_map(|child| child.attr_unsigned",
        "filter_map(|var_id| fixed_varnodes",
    ] {
        assert!(
            !display.contains(forbidden),
            "display symbol decode must fail closed instead of skipping/defaulting: {forbidden}"
        );
    }
    assert!(
        display.contains("fn decoded_name_table_entry"),
        "empty name table entries must be represented explicitly, not through broad defaulting"
    );
}

#[test]
fn decodes_x86_varnode_list_selector_expressions() {
    let Some(path) = packaged_sla_path("x86", "x86-64.sla") else {
        return;
    };
    if !path.exists() {
        return;
    }
    let library = load_construct_templates_from_sla(path).expect("decode x86-64.sla");
    let constructors = library
        .constructors_by_source
        .get("avx.sinc:6")
        .expect("AVX constructor using a varnode-list selector expression");
    assert!(
        constructors.iter().any(
            |ctor| ctor.decode_status == CompiledSlaDecodeStatus::Decoded
                && ctor.operand_specs.iter().any(|spec| matches!(
                    spec,
                    CompiledOperandSpec::SlaVarnodeListExpression { .. }
                ))
        ),
        "avx.sinc:6 should decode through a spec-derived varnode-list selector expression"
    );
}

#[test]
fn decodes_real_aarch64_rm_gpr64_subtable_without_placeholders() {
    let Some(path) = packaged_sla_path("AARCH64", "AARCH64.sla") else {
        return;
    };
    if !path.exists() {
        return;
    }
    let library = load_construct_templates_from_sla(path).expect("decode AARCH64.sla");
    let subtable = library
        .subtables
        .get("Rm_GPR64")
        .expect("Rm_GPR64 subtable");
    assert!(
        !subtable.constructors.is_empty(),
        "Rm_GPR64 must contain constructors"
    );
    assert!(
        subtable
            .constructors
            .iter()
            .all(|ctor| !ctor.source_key.starts_with("sla_decode_failed_constructor")),
        "Rm_GPR64 subtable should decode concrete constructors, got {:?}",
        subtable
            .constructors
            .iter()
            .map(|ctor| (&ctor.id, &ctor.source_key, &ctor.display_template.display))
            .collect::<Vec<_>>()
    );
    let native_subtable = library
        .native
        .subtables
        .get("Rm_GPR64")
        .expect("native Rm_GPR64 subtable");
    assert_eq!(
        native_subtable.constructors.len(),
        subtable.constructors.len()
    );
    assert!(
        native_subtable
            .constructors
            .iter()
            .all(|ctor| ctor.subtable_name == "Rm_GPR64"),
        "native constructors must carry subtable identity"
    );
}

#[test]
fn native_decision_terminal_pairs_use_sla_constructor_identity() {
    let Some(path) = packaged_sla_path("x86", "x86-64.sla") else {
        return;
    };
    if !path.exists() {
        return;
    }
    let library = load_construct_templates_from_sla(path).expect("decode x86-64.sla");
    let instruction = library
        .native
        .subtables
        .get("instruction")
        .expect("native instruction subtable");
    let tree = instruction
        .decision_tree
        .as_ref()
        .expect("native instruction decision tree");
    let terminal_pairs = tree
        .nodes
        .iter()
        .flat_map(|node| &node.terminal_pairs)
        .collect::<Vec<_>>();
    assert!(
        !terminal_pairs.is_empty(),
        "native decision tree must preserve terminal constructor pairs"
    );
    assert!(
        terminal_pairs
            .iter()
            .all(|pair| pair.subtable_id == instruction.id),
        "terminal pairs must be keyed by .sla subtable identity"
    );
    assert!(
        terminal_pairs.iter().any(|pair| {
            instruction
                .constructors
                .iter()
                .any(|ctor| ctor.constructor_id == pair.constructor_id)
        }),
        "terminal constructor ids must resolve within the same native subtable"
    );
}

#[test]
fn debug_aarch64_rm_gpr64_constructor_shape() {
    let Some(path) = packaged_sla_path("AARCH64", "AARCH64.sla") else {
        return;
    };
    if !path.exists() {
        return;
    }
    let artifact = load_compiled_sla(&path).expect("load AARCH64.sla");
    let mut parser = PackedParser::new(&artifact.payload);
    let root = parser.parse_root().expect("parse root");
    let mut symbol_names = BTreeMap::new();
    if let Some(sym_tab) = root
        .descendants_with_id(sla_format::ELEM_SYMBOL_TABLE)
        .into_iter()
        .next()
    {
        for head in &sym_tab.children {
            if let Some(id) = head.attr_unsigned(sla_format::ATTR_ID) {
                if let Some(name) = head.attr_string(sla_format::ATTR_NAME) {
                    symbol_names.insert(id as u32, name.to_string());
                }
            }
        }
    }
    let rm = root
        .descendants_with_id(sla_format::ELEM_SUBTABLE_SYM)
        .into_iter()
        .find(|sub| {
            sub.attr_unsigned(sla_format::ATTR_ID)
                .map(|id| {
                    symbol_names
                        .get(&(id as u32))
                        .map(|n| n == "Rm_GPR64")
                        .unwrap_or(false)
                })
                .unwrap_or(false)
                || sub.attr_string(sla_format::ATTR_NAME) == Some("Rm_GPR64")
        })
        .expect("Rm_GPR64 subtable");
    for (idx, ctor) in rm
        .children
        .iter()
        .filter(|child| child.id == sla_format::ELEM_CONSTRUCTOR)
        .enumerate()
    {
        eprintln!("CTOR {idx} attrs={:?}", ctor.attrs);
        for child in &ctor.children {
            eprintln!("  child id={} attrs={:?}", child.id, child.attrs);
        }
    }
}

#[test]
fn debug_aarch64_rm_gpr64_operand_symbol_shape() {
    let Some(path) = packaged_sla_path("AARCH64", "AARCH64.sla") else {
        return;
    };
    if !path.exists() {
        return;
    }
    let artifact = load_compiled_sla(&path).expect("load AARCH64.sla");
    let mut parser = PackedParser::new(&artifact.payload);
    let root = parser.parse_root().expect("parse root");
    for operand in root.descendants_with_id(sla_format::ELEM_OPERAND_SYM) {
        let id = operand
            .attr_unsigned(sla_format::ATTR_ID)
            .unwrap_or(u64::MAX);
        if matches!(id, 1227 | 1228 | 1254 | 1255) {
            eprintln!("OPERAND_SYM id={id} attrs={:?}", operand.attrs);
            for child in &operand.children {
                eprintln!("  child id={} attrs={:?}", child.id, child.attrs);
                for grand in &child.children {
                    eprintln!("    grandchild id={} attrs={:?}", grand.id, grand.attrs);
                }
            }
        }
    }
    for head in root.descendants_with_id(sla_format::ELEM_VARNODE_SYM_HEAD) {
        let id = head.attr_unsigned(sla_format::ATTR_ID).unwrap_or(u64::MAX);
        if matches!(id, 1227 | 1228 | 1254 | 1255) {
            eprintln!("VARNODE_HEAD id={id} attrs={:?}", head.attrs);
            for child in &head.children {
                eprintln!("  child id={} attrs={:?}", child.id, child.attrs);
                for grand in &child.children {
                    eprintln!("    grandchild id={} attrs={:?}", grand.id, grand.attrs);
                }
            }
        }
    }
    for head in root.descendants_with_id(sla_format::ELEM_NAME_SYM) {
        let id = head.attr_unsigned(sla_format::ATTR_ID).unwrap_or(u64::MAX);
        if matches!(id, 1227 | 1228 | 1254 | 1255) {
            eprintln!("NAME_SYM id={id} attrs={:?}", head.attrs);
            for child in &head.children {
                eprintln!("  child id={} attrs={:?}", child.id, child.attrs);
            }
        }
    }
    for head in root.descendants_with_id(sla_format::ELEM_VARLIST_SYM) {
        let id = head.attr_unsigned(sla_format::ATTR_ID).unwrap_or(u64::MAX);
        if matches!(id, 1227 | 1228 | 1254 | 1255) {
            eprintln!("VARLIST_SYM id={id} attrs={:?}", head.attrs);
            for child in &head.children {
                eprintln!("  child id={} attrs={:?}", child.id, child.attrs);
            }
        }
    }
}
