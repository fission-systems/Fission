//! Does the `.sla` carry context-field *names*?
//!
//! The runtime's last dependency on `.slaspec` is the name of each context
//! field: `address_state.rs` looks one up by name, and the `.pspec` default
//! context is expressed in named fields. The parser already walks
//! `ELEM_CONTEXT_SYM` but keeps only the id and the pattern expression.
//! If the element carries ATTR_NAME, that dependency is removable.

use std::path::Path;

use fission_sleigh::compiler::sla::{load_compiled_sla, sla_format, PackedElement, PackedParser};

fn main() {
    let root_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../utils/sleigh-specs/compiled")
        .canonicalize()
        .expect("compiled sla root");

    let mut files = walk(&root_dir);
    files.sort();

    let mut with_names = 0usize;
    let mut without = 0usize;
    let mut files_with_ctx = 0usize;
    let mut shown = 0usize;

    for path in &files {
        let Ok(artifact) = load_compiled_sla(path) else { continue };
        let Ok(root) = PackedParser::new(&artifact.payload).parse_root() else {
            continue;
        };
        // Ghidra writes the symbol table in two passes: a header list carrying
        // id+name, then the bodies. The name should be on the head element.
        let heads: Vec<&PackedElement> =
            root.descendants_with_id(sla_format::ELEM_CONTEXT_SYM_HEAD);
        let syms: Vec<&PackedElement> = if heads.is_empty() {
            root.descendants_with_id(sla_format::ELEM_CONTEXT_SYM)
        } else {
            heads
        };
        if syms.is_empty() {
            continue;
        }
        files_with_ctx += 1;
        let mut names = Vec::new();
        for s in &syms {
            match s.attr_string(sla_format::ATTR_NAME) {
                Some(n) => {
                    with_names += 1;
                    names.push(n.to_string());
                }
                None => without += 1,
            }
        }
        if shown < 6 && !names.is_empty() {
            shown += 1;
            let f = path.file_name().unwrap().to_string_lossy();
            println!("  {f:<28} {} context syms: {:?}", syms.len(),
                     &names[..names.len().min(8)]);
        }
    }

    println!();
    println!("files with context symbols : {files_with_ctx} of {}", files.len());
    println!("context syms WITH a name   : {with_names}");
    println!("context syms WITHOUT a name: {without}");
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return out };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else if p.extension().and_then(|x| x.to_str()) == Some("sla") {
            out.push(p);
        }
    }
    out
}
