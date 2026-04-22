mod ast;
mod codegen;
mod equivalence;
mod ir;
mod preprocessor;
mod token;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub use ast::{AstConstructor, AstItem, SpecAst, WithContextFrame};
pub use codegen::{GeneratedArtifact, GeneratedArtifactSet};
pub use equivalence::{
    build_x86_64_equivalence_report, default_unit_seed_samples, EquivalenceMismatchKind,
    EquivalenceRecord, EquivalenceReport, InstructionSample,
};
pub use ir::{
    CompiledConstructor, CompiledFrontend, CompiledMacro, CompiledPatternNode, CompiledPcodeOp,
    CompiledSpecDefinition, ControlFlowClass,
};
pub use preprocessor::{expand_entry_spec, ExpandedSpec, IncludeManifestEntry, PreprocessedLine};
pub use token::{Token, TokenKind, TokenizedLine};
pub use ast::parse_expanded_spec;

const X86_ARCH_DIR: &str = "x86";
const X86_64_ENTRY_SPEC: &str = "x86-64.slaspec";

pub fn spec_root_for_arch(arch: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("specs")
        .join("languages")
        .join(arch)
}

pub fn generated_root_for_arch(arch: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("generated")
        .join(arch)
}

pub fn x86_64_entry_spec_path() -> PathBuf {
    spec_root_for_arch(X86_ARCH_DIR).join(X86_64_ENTRY_SPEC)
}

pub fn compile_x86_64_frontend() -> Result<CompiledFrontend> {
    let entry_spec = x86_64_entry_spec_path();
    let expanded = preprocessor::expand_entry_spec(&entry_spec)
        .with_context(|| format!("expand x86-64 entry spec {}", entry_spec.display()))?;
    let ast = ast::parse_expanded_spec(&expanded)
        .with_context(|| format!("parse expanded x86-64 spec {}", entry_spec.display()))?;
    ir::compile_frontend(&expanded, &ast)
        .with_context(|| format!("compile x86-64 frontend {}", entry_spec.display()))
}

pub fn render_x86_64_generated_artifacts(
    compiled: &CompiledFrontend,
) -> Result<GeneratedArtifactSet> {
    codegen::render_generated_artifacts(compiled)
}

pub fn write_x86_64_generated_artifacts(output_root: &Path) -> Result<GeneratedArtifactSet> {
    let compiled = compile_x86_64_frontend()?;
    let artifacts = render_x86_64_generated_artifacts(&compiled)?;
    codegen::write_generated_artifacts(output_root, &artifacts)?;
    Ok(artifacts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x86_64_entry_spec_exists_under_arch_tree() {
        let path = x86_64_entry_spec_path();
        assert!(path.ends_with("specs/languages/x86/x86-64.slaspec"));
        assert!(path.exists(), "expected x86-64 entry spec at {}", path.display());
    }

    #[test]
    fn compile_x86_64_frontend_collects_inventory() {
        let compiled = compile_x86_64_frontend().expect("compile x86-64 frontend");
        assert_eq!(compiled.arch, "x86");
        assert_eq!(compiled.entry_spec, "x86-64.slaspec");
        assert!(compiled.include_manifest.len() >= 3);
        assert!(!compiled.constructors.is_empty());
        assert!(!compiled.definitions.is_empty());
        assert!(!compiled.pattern_nodes.is_empty());
    }
}
