//! Point lookups over facts the loader and the type library already hold:
//! `types` (recovered classes and structs), `whatis` (what is at an
//! address) and `api` (a known API's prototype). Nothing here derives a
//! fact; each reads one its owner computed.

use crate::cli::args::{ApiArgs, OneShotArgs};
use anyhow::Result;
use fission_loader::loader::LoadedBinary;
use serde_json::{Value, json};

/// A query `types` or `whatis` makes of the loaded binary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LookupQuery {
    Types {
        source: Option<String>,
        name: Option<String>,
    },
    Whatis {
        address: u64,
    },
}

/// Every value `InferredTypeInfo::source` takes.
const TYPE_SOURCES: &[&str] = &[
    "cpp_rtti",
    "dwarf",
    "pdb",
    "go",
    "rust_vtable",
    "objc",
    "swift",
    "pe_header",
    "elf_header",
    "te_header",
    "user_sidecar",
];

/// Type recovery with no test behind it yet; reported, but marked.
const UNVERIFIED_TYPE_SOURCES: &[&str] = &["objc", "swift", "rust_vtable"];

fn hex(value: u64) -> String {
    format!("0x{value:x}")
}

pub(crate) fn run(cli: &OneShotArgs, binary: &LoadedBinary, query: &LookupQuery) -> Result<()> {
    if let LookupQuery::Types {
        source: Some(source),
        ..
    } = query
    {
        // A misspelt source would list nothing and look like "no such types".
        anyhow::ensure!(
            TYPE_SOURCES.contains(&source.as_str()),
            "unknown --source `{source}`; one of: {}",
            TYPE_SOURCES.join(", ")
        );
    }
    let value = match query {
        LookupQuery::Types { source, name } => {
            types_json(binary, source.as_deref(), name.as_deref())
        }
        LookupQuery::Whatis { address } => whatis_json(binary, *address),
    };
    if cli.json {
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        match query {
            LookupQuery::Types { .. } => print_types(&value),
            LookupQuery::Whatis { .. } => println!("{}", serde_json::to_string_pretty(&value)?),
        }
    }
    Ok(())
}

pub(crate) fn types_json(binary: &LoadedBinary, source: Option<&str>, name: Option<&str>) -> Value {
    let needle = name.map(str::to_ascii_lowercase);
    let mut types: Vec<Value> = binary
        .inferred_types
        .iter()
        .filter(|ty| source.is_none_or(|s| ty.source == s))
        .filter(|ty| {
            needle
                .as_deref()
                .is_none_or(|n| ty.name.to_ascii_lowercase().contains(n))
        })
        .map(|ty| {
            json!({
                "name": ty.name,
                "mangled_name": ty.mangled_name,
                "kind": ty.kind,
                "source": ty.source,
                "verified": !UNVERIFIED_TYPE_SOURCES.contains(&ty.source.as_str()),
                "size": ty.size,
                "metadata_address": (ty.metadata_address != 0).then(|| hex(ty.metadata_address)),
                "fields": ty.fields.iter().map(|f| json!({
                    "name": f.name,
                    "type": f.type_name,
                    "offset": f.offset,
                    "size": f.size,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    types.sort_by(|a, b| {
        (a["source"].as_str(), a["name"].as_str()).cmp(&(b["source"].as_str(), b["name"].as_str()))
    });
    Value::Array(types)
}

fn print_types(types: &Value) {
    let types = types.as_array().map(Vec::as_slice).unwrap_or_default();
    println!("Types ({}):", types.len());
    for ty in types {
        println!(
            "  {:<12} {:<10} {}{}  ({} bytes, {} fields)",
            ty["source"].as_str().unwrap_or(""),
            ty["kind"].as_str().unwrap_or(""),
            ty["name"].as_str().unwrap_or(""),
            if ty["verified"] == false {
                "  [unverified]"
            } else {
                ""
            },
            ty["size"],
            ty["fields"].as_array().map_or(0, Vec::len),
        );
    }
}

pub(crate) fn whatis_json(binary: &LoadedBinary, address: u64) -> Value {
    let mut out = json!({ "address": hex(address) });
    match fission_analysis_db::ProgramSnapshot::try_from_loaded_binary(binary) {
        Ok(snapshot) => {
            out["function"] = snapshot
                .function_containing(address)
                .map(|f| {
                    let mut f = serde_json::to_value(f).unwrap_or(Value::Null);
                    if let Some(entry) = f["entry"].as_u64() {
                        f["offset"] = json!(address - entry);
                    }
                    f
                })
                .unwrap_or(Value::Null);
            out["memory_block"] = snapshot
                .memory_block_containing(address)
                .map(|b| serde_json::to_value(b).unwrap_or(Value::Null))
                .unwrap_or(Value::Null);
            out["symbols"] = snapshot
                .symbols_at(address)
                .map(|s| serde_json::to_value(s).unwrap_or(Value::Null))
                .collect::<Vec<_>>()
                .into();
            out["relocations"] = snapshot
                .relocations_at(address)
                .map(|r| serde_json::to_value(r).unwrap_or(Value::Null))
                .collect::<Vec<_>>()
                .into();
        }
        Err(report) => {
            out["error"] = json!(format!(
                "program metadata failed its integrity check: {report:?}"
            ));
        }
    }
    out["source"] = super::disasm::source_line(binary, address).unwrap_or(Value::Null);
    out
}

/// `fission_cli api NAME`: the prototype the bundled type library holds.
pub(crate) fn run_api(args: &ApiArgs) -> Result<()> {
    let resources = &fission_signatures::SIGNATURE_RESOURCES;
    let Some(signature) = resources.api_signature_for_bitness(&args.name, !args.bits32) else {
        anyhow::bail!(
            "`{}` is not in the bundled {}-bit API type library",
            args.name,
            if args.bits32 { 32 } else { 64 }
        );
    };
    if args.json {
        println!("{}", serde_json::to_string_pretty(signature)?);
    } else {
        let mut params: Vec<String> = signature
            .params
            .iter()
            .map(|p| format!("{} {}", p.type_name, p.name).trim().to_string())
            .collect();
        if signature.is_variadic {
            params.push("...".into());
        }
        println!(
            "{} {}({});",
            signature.return_type,
            signature.name,
            params.join(", ")
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_loader::loader::{DataBuffer, InferredTypeInfo, LoadedBinaryBuilder};

    fn with_types(types: &[(&str, &str)]) -> LoadedBinary {
        let mut binary = LoadedBinaryBuilder::new("t.bin".into(), DataBuffer::Heap(vec![0; 16]))
            .build()
            .expect("build");
        binary.inner_mut().inferred_types = types
            .iter()
            .map(|(name, source)| InferredTypeInfo {
                name: name.to_string(),
                mangled_name: String::new(),
                kind: "Struct".into(),
                source: source.to_string(),
                fields: Vec::new(),
                size: 0,
                metadata_address: 0,
            })
            .collect();
        binary
    }

    /// Go and Swift both say `Struct`; only the source tells which recovery
    /// is tested and which is not.
    #[test]
    fn untested_recoveries_are_marked_unverified() {
        let binary = with_types(&[("GoThing", "go"), ("SwiftThing", "swift"), ("Cls", "objc")]);
        let types = types_json(&binary, None, None);
        let verified = |name: &str| {
            types
                .as_array()
                .unwrap()
                .iter()
                .find(|t| t["name"] == name)
                .map(|t| t["verified"].clone())
        };
        assert_eq!(verified("GoThing"), Some(json!(true)));
        assert_eq!(verified("SwiftThing"), Some(json!(false)));
        assert_eq!(verified("Cls"), Some(json!(false)));
    }

    #[test]
    fn filters_by_source_and_name() {
        let binary = with_types(&[("point", "dwarf"), ("Elf64_Ehdr", "elf_header")]);
        assert_eq!(
            types_json(&binary, Some("dwarf"), None)
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            types_json(&binary, None, Some("POINT"))
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            types_json(&binary, Some("pdb"), None)
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }
}
