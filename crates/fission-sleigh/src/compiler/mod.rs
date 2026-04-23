mod ast;
mod codegen;
mod equivalence;
mod ir;
mod preprocessor;
mod token;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

pub use ast::parse_expanded_spec;
pub use ast::{AstConstructor, AstItem, SpecAst, WithContextFrame};
pub use codegen::{GeneratedArtifact, GeneratedArtifactSet};
pub use equivalence::{
    build_runtime_fixture_report, EquivalenceMismatchKind, RuntimeParityFixture,
    RuntimeParityRecord, RuntimeParityReport, RuntimeParityVarnodeShape,
};
pub use ir::{
    CompiledAddressSpace, CompiledArithmeticOpcode, CompiledConstTpl, CompiledConstructTpl,
    CompiledConstructTplKind, CompiledConstructor, CompiledConstructorTemplate,
    CompiledContextField, CompiledContextFieldRef, CompiledDecisionBucket, CompiledDecisionEdge,
    CompiledDecisionNode, CompiledDecisionProbe, CompiledDecisionTree, CompiledDisplayTemplate,
    CompiledExecutableConstructor, CompiledFixedRegister, CompiledFrontend, CompiledHandleTemplate,
    CompiledLabelRef, CompiledLanguageLayout, CompiledMacro, CompiledOpTpl, CompiledOpTplOpcode,
    CompiledOperandDecodeStep, CompiledOperandSpec, CompiledPatternMatcher, CompiledPatternNode,
    CompiledPcodeOp, CompiledRegister, CompiledSemanticOp, CompiledSemanticTemplate,
    CompiledSpaceRef, CompiledSpecDefinition, CompiledSubtable, CompiledTemplateSource,
    CompiledTokenField, CompiledTokenFieldRef, CompiledVarnodeTpl, ControlFlowClass,
};
pub use preprocessor::{expand_entry_spec, ExpandedSpec, IncludeManifestEntry, PreprocessedLine};
pub use token::{Token, TokenKind, TokenizedLine};

const X86_ARCH_DIR: &str = "x86";
const X86_64_ENTRY_SPEC: &str = "x86-64.slaspec";
const GHIDRA_LANGUAGE_MANIFEST_FILE: &str = "ghidra_language_manifest.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntrySpec {
    pub arch: String,
    pub path: PathBuf,
    pub entry_spec: String,
    pub entry_id: String,
    pub language_ids: Vec<String>,
    pub compatibility_aliases: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GhidraLanguageManifestEntry {
    pub processor: String,
    pub entry_spec: String,
    pub entry_id: String,
    pub language_id: Option<String>,
    #[serde(default)]
    pub language_ids: Vec<String>,
    pub endian: Option<String>,
    pub variant_class: String,
    #[serde(default)]
    pub imported_aux_files: Vec<String>,
    pub runtime_status: String,
    #[serde(default)]
    pub compatibility_aliases: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GhidraLanguageManifest {
    pub processor_count: usize,
    pub variant_count: usize,
    pub entries: Vec<GhidraLanguageManifestEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrontendCompileReportEntry {
    pub arch: String,
    pub processor: String,
    pub entry_spec: String,
    pub entry_id: String,
    pub generated_path: String,
    pub constructor_count: usize,
    pub pcodeop_count: usize,
    pub include_count: usize,
    pub runtime_ready: bool,
    pub decision_node_count: usize,
    pub constructor_template_count: usize,
    pub unsupported_template_count: usize,
    pub compile_status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrontendCompileManifest {
    pub variant_count: usize,
    pub entries: Vec<FrontendCompileReportEntry>,
}

pub fn spec_root_for_arch(arch: &str) -> PathBuf {
    let arch = canonical_processor_name(arch)
        .map(str::to_string)
        .unwrap_or_else(|| arch.to_string());
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("specs")
        .join("languages")
        .join(arch)
}

pub fn ghidra_language_manifest_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("specs")
        .join(GHIDRA_LANGUAGE_MANIFEST_FILE)
}

pub fn generated_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("generated")
}

pub fn generated_root_for_arch(arch: &str) -> PathBuf {
    generated_root().join(
        canonical_processor_name(arch)
            .map(str::to_string)
            .unwrap_or_else(|| arch.to_string()),
    )
}

pub fn entry_id_from_path(entry_spec: &Path) -> Result<String> {
    let stem = entry_spec
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| anyhow!("entry spec {} has no UTF-8 file stem", entry_spec.display()))?;
    Ok(stem.to_string())
}

pub fn generated_root_for_entry_spec(entry_spec: &Path) -> Result<PathBuf> {
    let arch = infer_arch_from_entry_spec(entry_spec)?;
    let entry_id = entry_id_from_path(entry_spec)?;
    Ok(generated_root_for_arch(&arch).join(entry_id))
}

fn generated_output_root_for_entry_spec(entry_spec: &Path, output_root: &Path) -> Result<PathBuf> {
    let arch = infer_arch_from_entry_spec(entry_spec)?;
    let entry_id = entry_id_from_path(entry_spec)?;
    if output_root
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name == entry_id)
        .unwrap_or(false)
    {
        return Ok(output_root.to_path_buf());
    }
    if output_root
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name == arch)
        .unwrap_or(false)
    {
        return Ok(output_root.join(entry_id));
    }
    Ok(output_root.join(arch).join(entry_id))
}

pub fn infer_arch_from_entry_spec(entry_spec: &Path) -> Result<String> {
    let languages_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("specs")
        .join("languages");
    let parent = entry_spec.parent().ok_or_else(|| {
        anyhow!(
            "entry spec {} has no parent directory",
            entry_spec.display()
        )
    })?;
    let arch_dir = parent
        .strip_prefix(&languages_root)
        .with_context(|| {
            format!(
                "entry spec {} is outside compiler spec root {}",
                entry_spec.display(),
                languages_root.display()
            )
        })?
        .components()
        .next()
        .ok_or_else(|| {
            anyhow!(
                "missing arch directory for entry spec {}",
                entry_spec.display()
            )
        })?;
    Ok(arch_dir.as_os_str().to_string_lossy().into_owned())
}

pub fn entry_spec_from_path(entry_spec: PathBuf) -> Result<EntrySpec> {
    let arch = infer_arch_from_entry_spec(&entry_spec)?;
    let entry_id = entry_id_from_path(&entry_spec)?;
    let entry_spec_name = entry_spec
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("entry spec {} has no UTF-8 file name", entry_spec.display()))?
        .to_string();
    Ok(EntrySpec {
        arch,
        path: entry_spec,
        entry_spec: entry_spec_name,
        entry_id,
        language_ids: Vec::new(),
        compatibility_aliases: Vec::new(),
    })
}

fn compatibility_aliases_for(processor: &str) -> Vec<String> {
    match processor {
        "AARCH64" => vec!["aarch64".to_string()],
        "ARM" => vec!["arm32".to_string()],
        "MIPS" => vec!["mips".to_string()],
        "PowerPC" => vec!["powerpc".to_string()],
        "RISCV" => vec!["riscv".to_string()],
        "x86" => vec!["x86".to_string()],
        _ => Vec::new(),
    }
}

fn canonical_processor_name(name: &str) -> Option<&'static str> {
    match name {
        "aarch64" => Some("AARCH64"),
        "arm32" => Some("ARM"),
        "mips" => Some("MIPS"),
        "powerpc" => Some("PowerPC"),
        "riscv" => Some("RISCV"),
        _ => None,
    }
}

fn read_processors_from_spec_tree() -> Result<Vec<String>> {
    let languages_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("specs")
        .join("languages");
    let mut processors = Vec::new();
    for entry in fs::read_dir(&languages_root)
        .with_context(|| format!("read languages root {}", languages_root.display()))?
    {
        let entry = entry.with_context(|| {
            format!(
                "read language processor entry under {}",
                languages_root.display()
            )
        })?;
        if entry
            .file_type()
            .with_context(|| format!("read file type for {}", entry.path().display()))?
            .is_dir()
        {
            processors.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    processors.sort();
    Ok(processors)
}

fn parse_ldefs_language_attrs(contents: &str, key: &str) -> Vec<BTreeMap<String, String>> {
    let needle = format!("<{key} ");
    let mut out = Vec::new();
    let mut rest = contents;
    while let Some(start) = rest.find(&needle) {
        rest = &rest[start + needle.len()..];
        let Some(end) = rest.find('>') else {
            break;
        };
        let segment = &rest[..end];
        rest = &rest[end + 1..];
        let mut attrs = BTreeMap::new();
        let bytes = segment.as_bytes();
        let mut idx = 0usize;
        while idx < bytes.len() {
            while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
                idx += 1;
            }
            if idx >= bytes.len() {
                break;
            }
            let key_start = idx;
            while idx < bytes.len() && bytes[idx] != b'=' && !bytes[idx].is_ascii_whitespace() {
                idx += 1;
            }
            let name = segment[key_start..idx].trim();
            while idx < bytes.len() && (bytes[idx].is_ascii_whitespace() || bytes[idx] == b'=') {
                idx += 1;
            }
            if idx >= bytes.len() || bytes[idx] != b'"' {
                while idx < bytes.len() && bytes[idx] != b' ' {
                    idx += 1;
                }
                continue;
            }
            idx += 1;
            let value_start = idx;
            while idx < bytes.len() && bytes[idx] != b'"' {
                idx += 1;
            }
            if idx > bytes.len() {
                break;
            }
            let value = segment[value_start..idx].to_string();
            if !name.is_empty() {
                attrs.insert(name.to_string(), value);
            }
            if idx < bytes.len() {
                idx += 1;
            }
        }
        out.push(attrs);
    }
    out
}

fn ldefs_metadata_for_processor(
    processor_root: &Path,
) -> Result<BTreeMap<String, (Vec<String>, BTreeSet<String>, BTreeSet<String>)>> {
    let mut metadata: BTreeMap<String, (Vec<String>, BTreeSet<String>, BTreeSet<String>)> =
        BTreeMap::new();
    for entry in fs::read_dir(processor_root)
        .with_context(|| format!("read processor root {}", processor_root.display()))?
    {
        let entry = entry.with_context(|| {
            format!(
                "read processor file entry under {}",
                processor_root.display()
            )
        })?;
        let path = entry.path();
        let is_ldefs = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext == "ldefs")
            .unwrap_or(false);
        if !is_ldefs {
            continue;
        }
        let contents = fs::read_to_string(&path)
            .with_context(|| format!("read ldefs file {}", path.display()))?;
        for attrs in parse_ldefs_language_attrs(&contents, "language") {
            let Some(slafile) = attrs.get("slafile") else {
                continue;
            };
            let stem = Path::new(slafile)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or_default()
                .to_string();
            if stem.is_empty() {
                continue;
            }
            let entry = metadata
                .entry(stem)
                .or_insert_with(|| (Vec::new(), BTreeSet::new(), BTreeSet::new()));
            if let Some(language_id) = attrs.get("id") {
                if !entry.0.contains(language_id) {
                    entry.0.push(language_id.clone());
                }
            }
            if let Some(endian) = attrs.get("endian") {
                entry.1.insert(endian.clone());
            }
            if let Some(variant) = attrs.get("variant") {
                entry.2.insert(variant.clone());
            }
        }
    }
    Ok(metadata)
}

fn infer_endian_from_entry_id(entry_id: &str) -> Option<String> {
    let lower = entry_id.to_ascii_lowercase();
    if lower.contains("_be") || lower.ends_with("be") {
        Some("big".to_string())
    } else if lower.contains("_le") || lower.ends_with("le") {
        Some("little".to_string())
    } else {
        None
    }
}

fn variant_class_for_entry(entry_id: &str, variants: &BTreeSet<String>) -> String {
    if variants.len() == 1 {
        return variants.iter().next().cloned().unwrap_or_default();
    }
    let lower = entry_id.to_ascii_lowercase();
    if lower.contains("be") {
        "big_endian_variant".to_string()
    } else if lower.contains("le") {
        "little_endian_variant".to_string()
    } else {
        "default".to_string()
    }
}

pub fn build_ghidra_language_manifest() -> Result<GhidraLanguageManifest> {
    let mut entries = Vec::new();
    let processors = read_processors_from_spec_tree()?;
    for processor in &processors {
        let processor_root = spec_root_for_arch(processor);
        let metadata = ldefs_metadata_for_processor(&processor_root)?;
        let mut aux_files = Vec::new();
        for path in processor_root
            .read_dir()
            .with_context(|| format!("read processor root {}", processor_root.display()))?
            .flat_map(|entry| entry.map(|entry| entry.path()))
        {
            if path.is_dir() {
                for nested in walk_aux_files(&path)? {
                    aux_files.push(
                        nested
                            .strip_prefix(&processor_root)
                            .unwrap_or(&nested)
                            .to_string_lossy()
                            .replace('\\', "/"),
                    );
                }
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) == Some("slaspec") {
                continue;
            }
            aux_files.push(
                path.strip_prefix(&processor_root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
        aux_files.sort();
        for spec in discover_entry_specs_for_arch(processor)? {
            let ldef = metadata.get(&spec.entry_id);
            let language_ids = ldef.map(|(ids, _, _)| ids.clone()).unwrap_or_default();
            let language_id = if language_ids.len() == 1 {
                language_ids.first().cloned()
            } else {
                None
            };
            let endian = ldef
                .and_then(|(_, endians, _)| {
                    if endians.len() == 1 {
                        endians.iter().next().cloned()
                    } else {
                        None
                    }
                })
                .or_else(|| infer_endian_from_entry_id(&spec.entry_id));
            let variant_class = variant_class_for_entry(
                &spec.entry_id,
                &ldef
                    .map(|(_, _, variants)| variants.clone())
                    .unwrap_or_default(),
            );
            entries.push(GhidraLanguageManifestEntry {
                processor: spec.arch.clone(),
                entry_spec: spec.entry_spec.clone(),
                entry_id: spec.entry_id.clone(),
                language_id,
                language_ids: language_ids.clone(),
                endian,
                variant_class,
                imported_aux_files: aux_files.clone(),
                runtime_status: if spec.arch == "x86" && spec.entry_id == "x86-64" {
                    "executable_candidate".to_string()
                } else {
                    "registered_compile_only".to_string()
                },
                compatibility_aliases: compatibility_aliases_for(&spec.arch),
            });
        }
    }
    Ok(GhidraLanguageManifest {
        processor_count: processors.len(),
        variant_count: entries.len(),
        entries,
    })
}

fn walk_aux_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in
        fs::read_dir(root).with_context(|| format!("read nested root {}", root.display()))?
    {
        let entry = entry.with_context(|| format!("read nested entry under {}", root.display()))?;
        let path = entry.path();
        if path.is_dir() {
            files.extend(walk_aux_files(&path)?);
        } else if path.extension().and_then(|ext| ext.to_str()) != Some("slaspec") {
            files.push(path);
        }
    }
    Ok(files)
}

pub fn write_ghidra_language_manifest() -> Result<GhidraLanguageManifest> {
    let manifest = build_ghidra_language_manifest()?;
    fs::write(
        ghidra_language_manifest_path(),
        serde_json::to_string_pretty(&manifest)? + "\n",
    )
    .with_context(|| {
        format!(
            "write Ghidra language manifest {}",
            ghidra_language_manifest_path().display()
        )
    })?;
    Ok(manifest)
}

fn discover_all_entry_specs_from_manifest() -> Result<Option<Vec<EntrySpec>>> {
    let manifest_path = ghidra_language_manifest_path();
    if !manifest_path.exists() {
        return Ok(None);
    }
    let manifest: GhidraLanguageManifest = serde_json::from_str(
        &fs::read_to_string(&manifest_path)
            .with_context(|| format!("read manifest {}", manifest_path.display()))?,
    )
    .with_context(|| format!("parse manifest {}", manifest_path.display()))?;
    let mut entries = manifest
        .entries
        .into_iter()
        .map(|entry| EntrySpec {
            arch: entry.processor.clone(),
            path: spec_root_for_arch(&entry.processor).join(&entry.entry_spec),
            entry_spec: entry.entry_spec,
            entry_id: entry.entry_id,
            language_ids: entry.language_ids,
            compatibility_aliases: entry.compatibility_aliases,
        })
        .collect::<Vec<_>>();
    entries.sort_by(|lhs, rhs| {
        lhs.arch
            .cmp(&rhs.arch)
            .then_with(|| lhs.entry_spec.cmp(&rhs.entry_spec))
    });
    Ok(Some(entries))
}

pub fn discover_entry_specs_for_arch(arch: &str) -> Result<Vec<EntrySpec>> {
    let arch_root = spec_root_for_arch(arch);
    let mut entries = Vec::new();
    for entry in fs::read_dir(&arch_root)
        .with_context(|| format!("read spec arch root {}", arch_root.display()))?
    {
        let entry =
            entry.with_context(|| format!("read spec dir entry under {}", arch_root.display()))?;
        let path = entry.path();
        let is_slaspec = path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension == "slaspec")
            .unwrap_or(false);
        if is_slaspec {
            let mut spec = entry_spec_from_path(path)?;
            spec.compatibility_aliases = compatibility_aliases_for(&spec.arch);
            entries.push(spec);
        }
    }
    entries.sort_by(|lhs, rhs| lhs.entry_spec.cmp(&rhs.entry_spec));
    Ok(entries)
}

pub fn discover_all_entry_specs() -> Result<Vec<EntrySpec>> {
    if let Some(entries) = discover_all_entry_specs_from_manifest()? {
        return Ok(entries);
    }
    let mut entries = Vec::new();
    for arch in read_processors_from_spec_tree()? {
        entries.extend(discover_entry_specs_for_arch(&arch)?);
    }
    Ok(entries)
}

pub fn compile_frontend_for_entry_spec(entry_spec: &Path) -> Result<CompiledFrontend> {
    let arch = infer_arch_from_entry_spec(entry_spec)?;
    let expanded = preprocessor::expand_entry_spec(entry_spec)
        .with_context(|| format!("expand entry spec {}", entry_spec.display()))?;
    let ast = ast::parse_expanded_spec(&expanded)
        .with_context(|| format!("parse expanded spec {}", entry_spec.display()))?;
    ir::compile_frontend(&arch, &expanded, &ast)
        .with_context(|| format!("compile frontend {}", entry_spec.display()))
}

pub fn compile_frontends_for_arch(arch: &str) -> Result<Vec<CompiledFrontend>> {
    discover_entry_specs_for_arch(arch)?
        .into_iter()
        .map(|entry| compile_frontend_for_entry_spec(&entry.path))
        .collect()
}

pub fn render_generated_artifacts_for_entry_spec(
    compiled: &CompiledFrontend,
) -> Result<GeneratedArtifactSet> {
    codegen::render_generated_artifacts(compiled)
}

pub fn write_generated_artifacts_for_entry_spec(
    entry_spec: &Path,
    output_root: &Path,
) -> Result<GeneratedArtifactSet> {
    let compiled = compile_frontend_for_entry_spec(entry_spec)?;
    let artifacts = render_generated_artifacts_for_entry_spec(&compiled)?;
    let entry_output_root = generated_output_root_for_entry_spec(entry_spec, output_root)?;
    codegen::write_generated_artifacts(&entry_output_root, &artifacts)?;
    Ok(artifacts)
}

pub fn write_all_generated_artifacts(output_root: &Path) -> Result<FrontendCompileManifest> {
    if output_root.exists() {
        fs::remove_dir_all(output_root)
            .with_context(|| format!("clear generated root {}", output_root.display()))?;
    }
    fs::create_dir_all(output_root)
        .with_context(|| format!("create generated root {}", output_root.display()))?;

    let mut report_entries = Vec::new();
    let mut failure_messages = Vec::new();
    for entry in discover_all_entry_specs()? {
        let arch_output_root = output_root.join(&entry.arch);
        write_arch_generated_readme(&entry.arch, &arch_output_root)?;
        let entry_output_root = generated_output_root_for_entry_spec(&entry.path, output_root)?;
        match compile_frontend_for_entry_spec(&entry.path) {
            Ok(compiled) => {
                let artifacts = render_generated_artifacts_for_entry_spec(&compiled)?;
                codegen::write_generated_artifacts(&entry_output_root, &artifacts)?;
                report_entries.push(FrontendCompileReportEntry {
                    processor: entry.arch.clone(),
                    arch: entry.arch,
                    entry_spec: entry.entry_spec,
                    entry_id: entry.entry_id,
                    generated_path: entry_output_root
                        .strip_prefix(output_root)
                        .unwrap_or(&entry_output_root)
                        .to_string_lossy()
                        .replace('\\', "/"),
                    constructor_count: compiled.constructors.len(),
                    pcodeop_count: compiled.pcode_ops.len(),
                    include_count: compiled.include_manifest.len(),
                    runtime_ready: compiled
                        .executable_constructors
                        .iter()
                        .any(|constructor| constructor.runtime_ready),
                    decision_node_count: compiled.decision_tree.decision_node_count,
                    constructor_template_count: compiled.executable_constructors.len(),
                    unsupported_template_count: compiled
                        .executable_constructors
                        .iter()
                        .filter(|constructor| !constructor.runtime_ready)
                        .count(),
                    compile_status: "ok".to_string(),
                });
            }
            Err(error) => {
                failure_messages.push(format!("{} {}: {error:#}", entry.arch, entry.entry_spec));
                report_entries.push(FrontendCompileReportEntry {
                    processor: entry.arch.clone(),
                    arch: entry.arch,
                    entry_spec: entry.entry_spec,
                    entry_id: entry.entry_id,
                    generated_path: entry_output_root
                        .strip_prefix(output_root)
                        .unwrap_or(&entry_output_root)
                        .to_string_lossy()
                        .replace('\\', "/"),
                    constructor_count: 0,
                    pcodeop_count: 0,
                    include_count: 0,
                    runtime_ready: false,
                    decision_node_count: 0,
                    constructor_template_count: 0,
                    unsupported_template_count: 0,
                    compile_status: format!("unsupported_syntax_family: {error:#}"),
                });
            }
        }
    }
    let manifest = FrontendCompileManifest {
        variant_count: report_entries.len(),
        entries: report_entries,
    };
    fs::write(
        output_root.join("compiler_manifest.json"),
        render_compiler_manifest(&manifest),
    )
    .with_context(|| {
        format!(
            "write generated compiler manifest {}",
            output_root.join("compiler_manifest.json").display()
        )
    })?;
    if !failure_messages.is_empty() {
        return Err(anyhow!(
            "failed to compile {} variants; see compiler_manifest.json for typed status\n{}",
            failure_messages.len(),
            failure_messages.join("\n")
        ));
    }
    Ok(manifest)
}

fn write_arch_generated_readme(arch: &str, arch_output_root: &Path) -> Result<()> {
    fs::create_dir_all(arch_output_root)
        .with_context(|| format!("create generated arch root {}", arch_output_root.display()))?;
    fs::write(
        arch_output_root.join("README.md"),
        format!(
            "# generated {arch} Sleigh front-ends\n\n\
             This directory is generated by `cargo run -p fission-sleigh --example generate_sleigh_frontends`.\n\n\
             Each child directory corresponds to one checked-in `.slaspec` entry variant for `{arch}`.\n\
             Generated artifacts are compiler-only products and are not the canonical runtime decoder path yet.\n"
        ),
    )
    .with_context(|| {
        format!(
            "write generated arch README {}",
            arch_output_root.join("README.md").display()
        )
    })
}

pub fn render_compiler_manifest(manifest: &FrontendCompileManifest) -> String {
    serde_json::to_string_pretty(manifest).expect("serialize compiler manifest") + "\n"
}

pub fn x86_64_entry_spec_path() -> PathBuf {
    spec_root_for_arch(X86_ARCH_DIR).join(X86_64_ENTRY_SPEC)
}

pub fn compile_x86_64_frontend() -> Result<CompiledFrontend> {
    compile_frontend_for_entry_spec(&x86_64_entry_spec_path())
}

pub fn render_x86_64_generated_artifacts(
    compiled: &CompiledFrontend,
) -> Result<GeneratedArtifactSet> {
    render_generated_artifacts_for_entry_spec(compiled)
}

pub fn write_x86_64_generated_artifacts(output_root: &Path) -> Result<GeneratedArtifactSet> {
    let entry_spec = x86_64_entry_spec_path();
    let root = if output_root
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name == "x86-64")
        .unwrap_or(false)
    {
        output_root.to_path_buf()
    } else {
        output_root.join("x86-64")
    };
    write_generated_artifacts_for_entry_spec(&entry_spec, &root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x86_64_entry_spec_exists_under_arch_tree() {
        let path = x86_64_entry_spec_path();
        assert!(path.ends_with("specs/languages/x86/x86-64.slaspec"));
        assert!(
            path.exists(),
            "expected x86-64 entry spec at {}",
            path.display()
        );
    }

    #[test]
    fn infer_arch_from_entry_spec_uses_arch_subdirectory() {
        let path = x86_64_entry_spec_path();
        let arch = infer_arch_from_entry_spec(&path).expect("infer arch");
        assert_eq!(arch, "x86");
    }

    #[test]
    fn compile_frontend_for_entry_spec_collects_inventory() {
        let compiled =
            compile_frontend_for_entry_spec(&x86_64_entry_spec_path()).expect("compile frontend");
        assert_eq!(compiled.arch, "x86");
        assert_eq!(compiled.entry_spec, "x86-64.slaspec");
        assert!(compiled.include_manifest.len() >= 3);
        assert!(!compiled.constructors.is_empty());
        assert!(!compiled.definitions.is_empty());
        assert!(!compiled.pattern_nodes.is_empty());
    }

    #[test]
    fn discovers_all_checked_in_entry_specs_in_stable_order() {
        let entries = discover_all_entry_specs().expect("discover all entry specs");
        assert_eq!(entries.len(), 146);
        assert_eq!(
            entries.first().map(|entry| entry.entry_spec.as_str()),
            Some("6502.slaspec")
        );
        assert_eq!(
            entries.get(1).map(|entry| entry.entry_spec.as_str()),
            Some("65c02.slaspec")
        );
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.arch == "PowerPC")
                .count(),
            18
        );
    }

    #[test]
    fn ghidra_language_manifest_covers_all_checked_in_variants() {
        let manifest = build_ghidra_language_manifest().expect("build ghidra language manifest");
        assert_eq!(manifest.processor_count, 38);
        assert_eq!(manifest.variant_count, 146);
        let x86_64 = manifest
            .entries
            .iter()
            .find(|entry| entry.processor == "x86" && entry.entry_id == "x86-64")
            .expect("x86-64 manifest entry");
        assert_eq!(x86_64.runtime_status, "executable_candidate");
        assert!(x86_64
            .language_ids
            .iter()
            .any(|id| id == "x86:LE:64:default"));
    }

    #[test]
    fn compiles_all_checked_in_entry_specs() {
        for entry in discover_all_entry_specs().expect("discover entry specs") {
            let compiled = compile_frontend_for_entry_spec(&entry.path).unwrap_or_else(|error| {
                panic!("compile {} failed: {error:#}", entry.path.display())
            });
            assert_eq!(compiled.arch, entry.arch);
            assert_eq!(compiled.entry_spec, entry.entry_spec);
            assert!(
                !compiled.constructors.is_empty(),
                "{} produced no constructors",
                entry.path.display()
            );
        }
    }
}
