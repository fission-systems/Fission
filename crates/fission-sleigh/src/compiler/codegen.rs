use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use super::ir::CompiledFrontend;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedArtifact {
    pub relative_path: String,
    pub contents: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedArtifactSet {
    pub artifacts: Vec<GeneratedArtifact>,
}

pub fn render_generated_artifacts(compiled: &CompiledFrontend) -> Result<GeneratedArtifactSet> {
    Ok(GeneratedArtifactSet {
        artifacts: vec![
            GeneratedArtifact {
                relative_path: "include_expanded_manifest.json".to_string(),
                contents: render_include_manifest(compiled),
            },
            GeneratedArtifact {
                relative_path: "parsed_inventory.json".to_string(),
                contents: render_inventory(compiled),
            },
            GeneratedArtifact {
                relative_path: "normalized_pattern_graph.json".to_string(),
                contents: render_pattern_graph(compiled),
            },
            GeneratedArtifact {
                relative_path: "semantic_action_ir.txt".to_string(),
                contents: render_semantic_ir(compiled),
            },
            GeneratedArtifact {
                relative_path: "generated_frontend.rs".to_string(),
                contents: render_rust_codegen(compiled),
            },
        ],
    })
}

pub fn write_generated_artifacts(root: &Path, artifacts: &GeneratedArtifactSet) -> Result<()> {
    fs::create_dir_all(root)
        .with_context(|| format!("create generated artifact root {}", root.display()))?;
    for artifact in &artifacts.artifacts {
        let path = root.join(&artifact.relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create generated artifact parent {}", parent.display()))?;
        }
        fs::write(&path, &artifact.contents)
            .with_context(|| format!("write generated artifact {}", path.display()))?;
    }
    Ok(())
}

fn render_include_manifest(compiled: &CompiledFrontend) -> String {
    let entries = compiled
        .include_manifest
        .iter()
        .map(|entry| format!("    {}", json_string(entry)))
        .collect::<Vec<_>>()
        .join(",\n");
    format!(
        "{{\n  \"arch\": {},\n  \"entry_spec\": {},\n  \"include_manifest\": [\n{}\n  ]\n}}\n",
        json_string(&compiled.arch),
        json_string(&compiled.entry_spec),
        entries
    )
}

fn render_inventory(compiled: &CompiledFrontend) -> String {
    let constructor_lines = compiled
        .constructors
        .iter()
        .map(|ctor| {
            format!(
                "    {{\"mnemonic\": {}, \"source\": {}, \"control_flow\": {}, \"with_depth\": {}, \"signature_hash\": \"{:016x}\"}}",
                json_string(&ctor.mnemonic),
                json_string(&ctor.source),
                json_string(ctor.control_flow.as_str()),
                ctor.with_stack.len(),
                ctor.signature_hash
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let definition_lines = compiled
        .definitions
        .iter()
        .map(|definition| {
            format!(
                "    {{\"kind\": {}, \"source\": {}}}",
                json_string(&definition.kind),
                json_string(&definition.source)
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    format!(
        "{{\n  \"arch\": {},\n  \"entry_spec\": {},\n  \"definition_count\": {},\n  \"macro_count\": {},\n  \"constructor_count\": {},\n  \"pcodeop_count\": {},\n  \"definitions\": [\n{}\n  ],\n  \"constructors\": [\n{}\n  ]\n}}\n",
        json_string(&compiled.arch),
        json_string(&compiled.entry_spec),
        compiled.definitions.len(),
        compiled.macros.len(),
        compiled.constructors.len(),
        compiled.pcode_ops.len(),
        definition_lines,
        constructor_lines
    )
}

fn render_pattern_graph(compiled: &CompiledFrontend) -> String {
    let lines = compiled
        .pattern_nodes
        .iter()
        .map(|node| {
            format!(
                "    {{\"node_id\": {}, \"mnemonic\": {}, \"source\": {}, \"with_depth\": {}, \"control_flow\": {}}}",
                json_string(&node.node_id),
                json_string(&node.mnemonic),
                json_string(&node.source),
                node.with_depth,
                json_string(node.control_flow.as_str())
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    format!(
        "{{\n  \"arch\": {},\n  \"entry_spec\": {},\n  \"pattern_nodes\": [\n{}\n  ]\n}}\n",
        json_string(&compiled.arch),
        json_string(&compiled.entry_spec),
        lines
    )
}

fn render_semantic_ir(compiled: &CompiledFrontend) -> String {
    let mut output = String::new();
    output.push_str("# semantic action inventory\n");
    output.push_str(&format!("arch: {}\n", compiled.arch));
    output.push_str(&format!("entry_spec: {}\n\n", compiled.entry_spec));
    for constructor in &compiled.constructors {
        output.push_str(&format!(
            "- {} [{}] {}\n",
            constructor.mnemonic,
            constructor.control_flow.as_str(),
            constructor.source
        ));
        if constructor.semantic_ops.is_empty() {
            output.push_str("  semantic_ops: <none>\n");
        } else {
            output.push_str(&format!(
                "  semantic_ops: {}\n",
                constructor.semantic_ops.join(", ")
            ));
        }
        if !constructor.with_stack.is_empty() {
            output.push_str(&format!(
                "  with_stack: {}\n",
                constructor.with_stack.join(" -> ")
            ));
        }
    }
    output
}

fn render_rust_codegen(compiled: &CompiledFrontend) -> String {
    let constructor_rows = compiled
        .constructors
        .iter()
        .take(256)
        .map(|ctor| {
            format!(
                "    GeneratedConstructor {{ mnemonic: {}, source: {}, control_flow: {} }},",
                rust_string(&ctor.mnemonic),
                rust_string(&ctor.source),
                rust_string(ctor.control_flow.as_str())
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "// Auto-generated by fission-sleigh compiler-only wave.\n\
         // Source: {} / {}\n\n\
         #[derive(Debug, Clone, Copy)]\n\
         pub struct GeneratedConstructor {{\n\
             pub mnemonic: &'static str,\n\
             pub source: &'static str,\n\
             pub control_flow: &'static str,\n\
         }}\n\n\
         pub const GENERATED_ARCH: &str = {};\n\
         pub const GENERATED_ENTRY_SPEC: &str = {};\n\
         pub const GENERATED_CONSTRUCTORS: &[GeneratedConstructor] = &[\n\
{}\n\
         ];\n",
        compiled.arch,
        compiled.entry_spec,
        rust_string(&compiled.arch),
        rust_string(&compiled.entry_spec),
        constructor_rows
    )
}

fn json_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn rust_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;
    use crate::compiler::{compile_x86_64_frontend, generated_root_for_arch};

    #[test]
    fn generated_output_is_deterministic() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let lhs = render_generated_artifacts(&compiled).expect("render lhs");
        let rhs = render_generated_artifacts(&compiled).expect("render rhs");
        assert_eq!(lhs, rhs);
    }

    #[test]
    fn writes_artifacts_to_directory() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let artifacts = render_generated_artifacts(&compiled).expect("render artifacts");
        let dir = tempdir().expect("tempdir");
        write_generated_artifacts(dir.path(), &artifacts).expect("write artifacts");
        assert!(dir.path().join("include_expanded_manifest.json").exists());
        assert!(dir.path().join("generated_frontend.rs").exists());
    }

    #[test]
    fn checked_in_generated_artifacts_match_renderer_output() {
        let compiled = compile_x86_64_frontend().expect("compile frontend");
        let artifacts = render_generated_artifacts(&compiled).expect("render artifacts");
        let root = generated_root_for_arch("x86");
        for artifact in artifacts.artifacts {
            let path = root.join(&artifact.relative_path);
            let checked_in = fs::read_to_string(&path)
                .unwrap_or_else(|_| panic!("missing checked-in artifact {}", path.display()));
            assert_eq!(checked_in, artifact.contents, "artifact mismatch at {}", path.display());
        }
    }
}
