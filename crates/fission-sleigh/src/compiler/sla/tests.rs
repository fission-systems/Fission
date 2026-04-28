use super::*;
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
