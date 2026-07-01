mod ast;
mod codegen;
pub mod discovery;
mod equivalence;
mod ir;
mod policy;
mod preprocessor;
pub mod sla;
mod token;

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use policy::{canonical_processor_name, is_executable_candidate_entry, language_aliases_for};
use serde::{Deserialize, Serialize};

pub use ast::parse_expanded_spec;
pub use ast::{AstConstructor, AstItem, SpecAst, WithContextFrame};
pub use codegen::{GeneratedArtifact, GeneratedArtifactSet};
use discovery::generated_output_root_for_entry_spec;
pub use discovery::{
    checked_in_compiled_sla_available, entry_id_from_path, entry_spec_from_path, generated_root,
    generated_root_for_arch, generated_root_for_entry_spec, ghidra_language_manifest_path,
    infer_arch_from_entry_spec, packaged_sla_for_entry_spec, require_packaged_sla_for_entry_spec,
    sleigh_build_cache_root, sleigh_compiled_root, sleigh_languages_root, sleigh_specs_root,
    spec_root_for_arch,
};
pub use equivalence::{
    build_runtime_fixture_report, EquivalenceMismatchKind, RuntimeParityFixture,
    RuntimeParityRecord, RuntimeParityReport, RuntimeParityVarnodeShape,
};
pub use ir::{
    CompiledAddressSpace, CompiledArithmeticOpcode, CompiledConstTpl, CompiledConstructTpl,
    CompiledConstructTplKind, CompiledConstructor, CompiledConstructorTemplate,
    CompiledContextCommit, CompiledContextCommitTarget, CompiledContextField, CompiledContextOp,
    CompiledDecisionBucket, CompiledDecisionEdge, CompiledDecisionLeafEntry, CompiledDecisionNode,
    CompiledDecisionProbe, CompiledDecisionTree, CompiledDisjointPattern, CompiledDisplayOperand,
    CompiledDisplayOperandKind, CompiledDisplayPiece, CompiledDisplayTemplate,
    CompiledExecutableConstructor, CompiledFrontend, CompiledHandleSelector,
    CompiledHandleTemplate, CompiledHandleTpl, CompiledLabelRef, CompiledLanguageLayout,
    CompiledMacro, CompiledOpTpl, CompiledOpTplOpcode, CompiledOperandDecodeStep,
    CompiledOperandSpec, CompiledPatternBlock, CompiledPatternExpression, CompiledPatternMatcher,
    CompiledPatternNode, CompiledPcodeOp, CompiledRegister, CompiledResolvedVarnode,
    CompiledSemanticTemplate, CompiledSlaConstructorIdentity, CompiledSlaDecodeStatus,
    CompiledSpaceRef, CompiledSpaceTpl, CompiledSpecDefinition, CompiledSubtable,
    CompiledSubtableDefinition, CompiledTemplateSource, CompiledTokenField, CompiledVarnodeTpl,
    ControlFlowClass, PatternConstraint,
};
pub use preprocessor::{expand_entry_spec, ExpandedSpec, IncludeManifestEntry, PreprocessedLine};
pub use sla::{
    load_compiled_sla, load_construct_templates_from_sla, load_native_language_from_sla,
    CompiledSlaArtifact, CompiledSlaConstructorTemplate, CompiledSlaTemplateLibrary,
    SlaConstructTpl, SlaConstructor, SlaDecisionNode, SlaDecisionPair, SlaDecisionTree,
    SlaDisjointPattern, SlaLanguage, SlaOperandSymbol, SlaSubtable, GHIDRA_SLA_MAGIC,
};
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
    pub language_aliases: Vec<String>,
    pub processor_spec: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GhidraLanguageManifestEntry {
    pub processor: String,
    pub entry_spec: String,
    pub entry_id: String,
    pub language_id: Option<String>,
    #[serde(default)]
    pub language_ids: Vec<String>,
    pub endian: Option<String>,
    pub processor_spec: Option<String>,
    pub variant_class: String,
    #[serde(default)]
    pub imported_aux_files: Vec<String>,
    pub runtime_status: String,
    #[serde(default)]
    pub language_aliases: Vec<String>,
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

fn read_processors_from_spec_tree() -> Result<Vec<String>> {
    let languages_root = sleigh_languages_root();
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
        if is_hidden_or_appledouble_path(&entry.path()) {
            continue;
        }
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

#[derive(Debug, Clone, Default)]
struct LdefsLanguageMetadata {
    language_ids: Vec<String>,
    endians: BTreeSet<String>,
    variants: BTreeSet<String>,
    processor_specs: BTreeSet<String>,
}

fn ldefs_metadata_for_processor(
    processor_root: &Path,
) -> Result<BTreeMap<String, LdefsLanguageMetadata>> {
    let mut metadata: BTreeMap<String, LdefsLanguageMetadata> = BTreeMap::new();
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
        if is_hidden_or_appledouble_path(&path) {
            continue;
        }
        let is_ldefs = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext == "ldefs");
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
            let entry = metadata.entry(stem).or_default();
            if let Some(language_id) = attrs.get("id") {
                if !entry.language_ids.contains(language_id) {
                    entry.language_ids.push(language_id.clone());
                }
            }
            if let Some(endian) = attrs.get("endian") {
                entry.endians.insert(endian.clone());
            }
            if let Some(variant) = attrs.get("variant") {
                entry.variants.insert(variant.clone());
            }
            if let Some(processor_spec) = attrs.get("processorspec") {
                entry.processor_specs.insert(processor_spec.clone());
            }
        }
    }
    Ok(metadata)
}

fn is_hidden_or_appledouble_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.starts_with('.') || name.starts_with("._"))
        .unwrap_or(false)
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
            let language_ids = ldef
                .map(|metadata| metadata.language_ids.clone())
                .unwrap_or_default();
            let language_id = if language_ids.len() == 1 {
                language_ids.first().cloned()
            } else {
                None
            };
            let endian = ldef
                .and_then(|metadata| {
                    if metadata.endians.len() == 1 {
                        metadata.endians.iter().next().cloned()
                    } else {
                        None
                    }
                })
                .or_else(|| infer_endian_from_entry_id(&spec.entry_id));
            let processor_spec = ldef.and_then(|metadata| {
                if metadata.processor_specs.len() == 1 {
                    metadata.processor_specs.iter().next().cloned()
                } else {
                    None
                }
            });
            let variant_class = variant_class_for_entry(
                &spec.entry_id,
                &ldef
                    .map(|metadata| metadata.variants.clone())
                    .unwrap_or_default(),
            );
            entries.push(GhidraLanguageManifestEntry {
                processor: spec.arch.clone(),
                entry_spec: spec.entry_spec.clone(),
                entry_id: spec.entry_id.clone(),
                language_id,
                language_ids: language_ids.clone(),
                endian,
                processor_spec,
                variant_class,
                imported_aux_files: aux_files.clone(),
                runtime_status: if is_executable_candidate_entry(&spec.entry_id)? {
                    "executable_candidate".to_string()
                } else {
                    "registered_compile_only".to_string()
                },
                language_aliases: language_aliases_for(&spec.arch),
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
            language_aliases: entry.language_aliases,
            processor_spec: entry.processor_spec,
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
    let metadata = ldefs_metadata_for_processor(&arch_root).unwrap_or_default();
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
            .is_some_and(|extension| extension == "slaspec");
        if is_slaspec {
            let mut spec = entry_spec_from_path(path)?;
            spec.language_aliases = language_aliases_for(&spec.arch);
            if let Some(ldef) = metadata.get(&spec.entry_id) {
                spec.language_ids = ldef.language_ids.clone();
                if ldef.processor_specs.len() == 1 {
                    spec.processor_spec = ldef.processor_specs.iter().next().cloned();
                }
            }
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
    let entry_id = entry_id_from_path(entry_spec)?;
    let cache_dir = sleigh_build_cache_root().join("cache");
    let cache_path = cache_dir.join(format!("{}.bin", entry_id));

    // Try to load from cache
    if let Ok(cache_file) = fs::File::open(&cache_path) {
        let cache_metadata = cache_file.metadata().ok();
        let cache_mtime = cache_metadata.and_then(|m| m.modified().ok());
        if let Some(cache_time) = cache_mtime {
            let reader = std::io::BufReader::new(cache_file);
            if let Ok(compiled) = bincode::deserialize_from::<_, CompiledFrontend>(reader) {
                if !frontend_cache_is_stale(entry_spec, &compiled, cache_time) {
                    tracing::info!(
                        "Loaded compiled Sleigh frontend for {} from cache",
                        entry_id
                    );
                    return Ok(compiled);
                }
            }
        }
    }

    // Cache miss: perform full compilation
    tracing::info!(
        "Cache miss/invalid for {}, performing full compilation",
        entry_id
    );
    let arch = infer_arch_from_entry_spec(entry_spec)?;
    let expanded = preprocessor::expand_entry_spec(entry_spec)
        .with_context(|| format!("expand entry spec {}", entry_spec.display()))?;
    let ast_result = ast::parse_expanded_spec(&expanded);
    let processor_spec = processor_spec_for_entry_spec(entry_spec)?;
    let mut compiled = ir::compile_frontend(
        &arch,
        &expanded,
        ast_result,
        entry_spec,
        processor_spec.as_deref(),
    )
    .with_context(|| format!("compile frontend {}", entry_spec.display()))?;
    apply_required_sla_overlay(&mut compiled, entry_spec)?;

    // Save to cache
    if let Err(e) = fs::create_dir_all(&cache_dir) {
        tracing::warn!("Failed to create Sleigh cache directory: {}", e);
    } else {
        match fs::File::create(&cache_path) {
            Ok(file) => {
                let writer = std::io::BufWriter::new(file);
                if let Err(e) = bincode::serialize_into(writer, &compiled) {
                    tracing::warn!("Failed to write Sleigh cache to bincode: {}", e);
                } else {
                    tracing::info!("Cached compiled Sleigh frontend for {}", entry_id);
                }
            }
            Err(e) => {
                tracing::warn!("Failed to create Sleigh cache file: {}", e);
            }
        }
    }

    Ok(compiled)
}

fn processor_spec_for_entry_spec(entry_spec: &Path) -> Result<Option<PathBuf>> {
    let arch = infer_arch_from_entry_spec(entry_spec)?;
    let entry_id = entry_id_from_path(entry_spec)?;
    let arch_root = spec_root_for_arch(&arch);
    let metadata = ldefs_metadata_for_processor(&arch_root)?;
    let processor_spec = metadata
        .get(&entry_id)
        .and_then(|metadata| {
            if metadata.processor_specs.len() == 1 {
                metadata.processor_specs.iter().next().cloned()
            } else {
                None
            }
        })
        .map(|name| arch_root.join(name));
    Ok(processor_spec.or_else(|| {
        let sibling = entry_spec.with_extension("pspec");
        sibling.exists().then_some(sibling)
    }))
}

fn path_modified_after(path: &Path, since: std::time::SystemTime) -> bool {
    fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .is_some_and(|mtime| mtime > since)
}

fn frontend_cache_is_stale(
    entry_spec: &Path,
    compiled: &CompiledFrontend,
    cache_time: std::time::SystemTime,
) -> bool {
    let sla_path = match discovery::packaged_sla_for_entry_spec(entry_spec) {
        Ok(Some(path)) => path,
        _ => {
            tracing::debug!(
                "Cache invalid for {}: checked-in .sla overlay is required",
                entry_spec.display()
            );
            return true;
        }
    };
    if path_modified_after(&sla_path, cache_time) {
        tracing::debug!(
            "Cache invalid for {}: .sla modified since cache creation ({})",
            entry_spec.display(),
            sla_path.display()
        );
        return true;
    }

    if path_modified_after(entry_spec, cache_time) {
        tracing::debug!(
            "Cache invalid for {}: entry spec modified since cache creation",
            entry_spec.display()
        );
        return true;
    }

    let Some(spec_dir) = entry_spec.parent() else {
        return true;
    };
    for include_file in &compiled.include_manifest {
        let path_part = match include_file.rsplit_once('@') {
            Some((path, _depth)) => path,
            None => include_file.as_str(),
        };
        let include_path = spec_dir.join(path_part);
        if !include_path.is_file() {
            tracing::debug!(
                "Cache invalid for {}: include file {} missing",
                entry_spec.display(),
                include_file
            );
            return true;
        }
        if path_modified_after(&include_path, cache_time) {
            tracing::debug!(
                "Cache invalid for {}: include file {} modified since cache creation",
                entry_spec.display(),
                include_file
            );
            return true;
        }
    }

    false
}

fn apply_required_sla_overlay(compiled: &mut CompiledFrontend, entry_spec: &Path) -> Result<()> {
    let sla_path = discovery::require_packaged_sla_for_entry_spec(entry_spec)?;
    let library = sla::load_construct_templates_from_sla(&sla_path)
        .with_context(|| format!("decode compiled SLEIGH artifact {}", sla_path.display()))?;
    ir::build_frontend_from_sla_native_model(compiled, &library)
        .with_context(|| format!("lower compiled SLEIGH artifact {}", sla_path.display()))?;
    Ok(())
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
                        .subtables
                        .values()
                        .any(|subtable| subtable.constructors.iter().any(|c| c.runtime_ready)),
                    decision_node_count: compiled
                        .subtables
                        .values()
                        .map(|s| s.decision_tree.decision_node_count)
                        .sum(),
                    constructor_template_count: compiled
                        .subtables
                        .values()
                        .map(|s| s.constructors.len())
                        .sum(),
                    unsupported_template_count: compiled
                        .subtables
                        .values()
                        .flat_map(|s| &s.constructors)
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
        .is_some_and(|name| name == "x86-64")
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
        assert!(path.ends_with("languages/x86/x86-64.slaspec"));
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
    fn hidden_or_appledouble_paths_are_skipped() {
        assert!(is_hidden_or_appledouble_path(Path::new(".hidden")));
        assert!(is_hidden_or_appledouble_path(Path::new("._x86.ldefs")));
        assert!(!is_hidden_or_appledouble_path(Path::new("x86.ldefs")));
    }

    #[test]
    fn compile_frontend_for_entry_spec_collects_inventory() {
        let compiled =
            compile_frontend_for_entry_spec(&x86_64_entry_spec_path()).expect("compile frontend");
        assert_eq!(compiled.arch, "x86");
        assert_eq!(compiled.entry_spec, "x86-64.slaspec");
        assert!(compiled.include_manifest.len() >= 3);
        assert!(compiled
            .subtables
            .values()
            .any(|subtable| !subtable.constructors.is_empty()));
        assert!(!compiled.construct_templates.is_empty());
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
        let aarch64 = manifest
            .entries
            .iter()
            .find(|entry| entry.entry_id == "AARCH64")
            .expect("AARCH64 manifest entry");
        assert_eq!(aarch64.runtime_status, "executable_candidate");
        let arm7_le = manifest
            .entries
            .iter()
            .find(|entry| entry.entry_id == "ARM7_le")
            .expect("ARM7_le manifest entry");
        assert_eq!(arm7_le.runtime_status, "executable_candidate");
        let riscv = manifest
            .entries
            .iter()
            .find(|entry| entry.entry_id == "riscv.lp64d")
            .expect("riscv.lp64d manifest entry");
        assert_eq!(riscv.runtime_status, "executable_candidate");
    }

    #[test]
    fn runtime_status_checked_in_manifest_matches_policy_allowlist() {
        let manifest: GhidraLanguageManifest =
            serde_json::from_str(&fs::read_to_string(ghidra_language_manifest_path()).unwrap())
                .expect("parse checked-in manifest");
        for entry in manifest.entries {
            assert_eq!(
                policy::runtime_status_for_entry(&entry.entry_id).expect("runtime status"),
                entry.runtime_status,
                "{} runtime_status must match policy allowlist",
                entry.entry_id
            );
        }
    }

    #[test]
    fn language_manifest_rejects_legacy_compatibility_aliases_field() {
        let payload = r#"{
            "processor_count": 1,
            "variant_count": 1,
            "entries": [{
                "processor": "x86",
                "entry_spec": "x86-64.slaspec",
                "entry_id": "x86-64",
                "language_id": "x86:LE:64:default",
                "language_ids": [],
                "endian": "little",
                "processor_spec": "x86-64.pspec",
                "variant_class": "default",
                "imported_aux_files": [],
                "runtime_status": "executable_candidate",
                "compatibility_aliases": ["x86:LE:64:default"]
            }]
        }"#;

        serde_json::from_str::<GhidraLanguageManifest>(payload)
            .expect_err("legacy compatibility_aliases must not be accepted");
    }

    #[test]
    fn compile_frontend_requires_checked_in_sla_overlay() {
        let entry_spec = spec_root_for_arch("Toy").join("toy_le.slaspec");
        let err = compile_frontend_for_entry_spec(&entry_spec)
            .expect_err("Toy slaspecs have no checked-in .sla overlay");
        assert!(
            err.to_string().contains("missing checked-in compiled .sla"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn packaged_sla_discovery_uses_utils_compiled_root() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let source = fs::read_to_string(manifest_dir.join("src/compiler/discovery.rs"))
            .expect("read discovery module");
        let processor_specific_path = ["Ghidra", "/", "Processors", "/", "x86"].concat();
        assert!(
            !source.contains(&processor_specific_path),
            "discovery module must not construct processor-specific Ghidra install paths"
        );
        assert!(
            source.contains("sleigh_compiled_root"),
            "discovery module should route packaged SLA lookup through utils/sleigh-specs/compiled"
        );
        let x86_64_sla = sleigh_compiled_root().join("x86").join("x86-64.sla");
        assert!(
            x86_64_sla.is_file(),
            "expected checked-in x86-64.sla at {}",
            x86_64_sla.display()
        );
    }

    #[test]
    #[ignore = "manual developer smoke: regenerates aarch64 generated artifacts"]
    fn force_regenerate_aarch64() {
        let aarch64_spec = spec_root_for_arch("AARCH64").join("AARCH64.slaspec");
        println!("Compiling spec: {}", aarch64_spec.display());
        let compiled = compile_frontend_for_entry_spec(&aarch64_spec).expect("compile aarch64");
        println!("Compiled AARCH64: {} subtables", compiled.subtables.len());
        let artifacts = codegen::render_generated_artifacts(&compiled).expect("render artifacts");
        let entry_output_root =
            generated_root_for_entry_spec(&aarch64_spec).expect("get generated root");
        println!("Writing artifacts to: {}", entry_output_root.display());
        codegen::write_generated_artifacts(&entry_output_root, &artifacts)
            .expect("write artifacts");
    }

    #[test]
    fn compiles_all_checked_in_entry_specs() {
        for entry in discover_all_entry_specs().expect("discover entry specs") {
            if discovery::packaged_sla_for_entry_spec(&entry.path)
                .expect("resolve packaged sla")
                .is_none()
            {
                continue;
            }
            let compiled = compile_frontend_for_entry_spec(&entry.path).unwrap_or_else(|error| {
                panic!("compile {} failed: {error:#}", entry.path.display())
            });
            assert_eq!(compiled.arch, entry.arch);
            assert_eq!(compiled.entry_spec, entry.entry_spec);
            assert!(
                compiled
                    .subtables
                    .values()
                    .any(|subtable| !subtable.constructors.is_empty()),
                "{} produced no executable constructor inventory",
                entry.path.display()
            );
        }
    }
}
