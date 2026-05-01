//! Ghidra-style language/compiler selection contracts.
//!
//! Binary format metadata is resolved through Ghidra opinion data before any
//! SLEIGH runtime is selected. This mirrors the loader-facing boundary:
//! `Loader.findSupportedLoadSpecs -> QueryOpinionService -> *.opinion ->
//! LanguageService/*.ldefs -> LoadSpec`.

use crate::constants::binary_format::{
    MACHO_CPU_TYPE_ARM, MACHO_CPU_TYPE_ARM64, MACHO_CPU_TYPE_X86, MACHO_CPU_TYPE_X86_64,
};
use crate::core_constants::{ELFCLASS32, ELFCLASS64, ELFDATA2LSB, ELFDATA2MSB};
use rkyv::{Archive, Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use thiserror::Error;

const PE_LOADER_NAME: &str = "Portable Executable (PE)";
const COFF_LOADER_NAME: &str = "Common Object File Format (COFF)";
const MS_COFF_LOADER_NAME: &str = "MS Common Object File Format (COFF)";
const ELF_LOADER_NAME: &str = "Executable and Linking Format (ELF)";
const MACHO_LOADER_NAME: &str = "Mac OS X Mach-O";

#[derive(Debug, Clone, PartialEq, Eq, Archive, Deserialize, Serialize)]
#[archive(check_bytes)]
pub struct GhidraLanguageId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Archive, Deserialize, Serialize)]
#[archive(check_bytes)]
pub struct CompilerSpecId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Archive, Deserialize, Serialize)]
#[archive(check_bytes)]
pub struct LanguageCompilerSpecPair {
    pub language_id: GhidraLanguageId,
    pub compiler_spec_id: CompilerSpecId,
}

#[derive(Debug, Clone, PartialEq, Eq, Archive, Deserialize, Serialize)]
#[archive(check_bytes)]
pub struct BinaryLoadSpec {
    pub format: String,
    pub image_base: u64,
    pub pair: LanguageCompilerSpecPair,
    pub preferred: bool,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Archive, Deserialize, Serialize)]
#[archive(check_bytes)]
pub struct ArchitectureDescriptor {
    pub processor: String,
    pub endian: String,
    pub bitness: u8,
    pub variant: String,
    pub abi: Option<String>,
    pub raw_machine: String,
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ArchitectureSelectionError {
    #[error("unsupported machine for {format}: {machine}")]
    UnsupportedMachine { format: String, machine: String },
    #[error("ambiguous load spec for {format}: {machine}")]
    AmbiguousLoadSpec { format: String, machine: String },
    #[error("missing language for {format}: {machine}")]
    MissingLanguage { format: String, machine: String },
}

impl GhidraLanguageId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl CompilerSpecId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl LanguageCompilerSpecPair {
    pub fn new(language_id: impl Into<String>, compiler_spec_id: impl Into<String>) -> Self {
        Self {
            language_id: GhidraLanguageId(language_id.into()),
            compiler_spec_id: CompilerSpecId(compiler_spec_id.into()),
        }
    }
}

impl BinaryLoadSpec {
    pub fn new(
        format: impl Into<String>,
        image_base: u64,
        language_id: impl Into<String>,
        compiler_spec_id: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        Self {
            format: format.into(),
            image_base,
            pair: LanguageCompilerSpecPair::new(language_id, compiler_spec_id),
            preferred: true,
            source: source.into(),
        }
    }

    pub fn compatibility_from_language_id(
        format: impl Into<String>,
        image_base: u64,
        language_id: impl Into<String>,
    ) -> Self {
        Self::new(
            format,
            image_base,
            language_id,
            "default",
            "compatibility-arch-spec",
        )
    }
}

impl ArchitectureDescriptor {
    pub fn new(
        processor: impl Into<String>,
        endian: impl Into<String>,
        bitness: u8,
        variant: impl Into<String>,
        abi: Option<String>,
        raw_machine: impl Into<String>,
    ) -> Self {
        Self {
            processor: processor.into(),
            endian: endian.into(),
            bitness,
            variant: variant.into(),
            abi,
            raw_machine: raw_machine.into(),
        }
    }
}

pub type ArchitectureSelectionResult =
    Result<(ArchitectureDescriptor, BinaryLoadSpec), ArchitectureSelectionError>;

#[derive(Debug, Clone, Default)]
struct OpinionQuery {
    loader: Option<String>,
    primary: Option<String>,
    secondary: Option<String>,
    processor: Option<String>,
    endian: Option<String>,
    size: Option<u8>,
    variant: Option<String>,
    compiler_spec_id: Option<String>,
}

#[derive(Debug, Clone)]
struct OpinionEntry {
    loader: String,
    primary: String,
    secondary: Option<String>,
    query: OpinionQuery,
}

#[derive(Debug, Clone)]
struct LanguageDescription {
    processor: String,
    endian: String,
    size: u8,
    variant: String,
    id: String,
    compiler_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryResult {
    pub language_id: String,
    pub compiler_spec_id: String,
    pub preferred: bool,
}

#[derive(Debug)]
struct OpinionDatabase {
    opinions: Vec<OpinionEntry>,
    languages: Vec<LanguageDescription>,
}

#[derive(Debug, Clone)]
struct SelectionCandidate {
    language: LanguageDescription,
    compiler_spec_id: String,
    preferred: bool,
}

#[derive(Debug, Clone, Copy)]
struct ExpectedLanguageFacts {
    bitness: Option<u8>,
    endian: Option<&'static str>,
}

static OPINION_DATABASE: OnceLock<Result<OpinionDatabase, String>> = OnceLock::new();

impl OpinionDatabase {
    fn load() -> Result<Self, String> {
        let mut db = Self {
            opinions: Vec::new(),
            languages: Vec::new(),
        };

        for path in collect_files_with_extension(&ghidra_data_root(), "opinion") {
            db.parse_opinion_file(&path)?;
        }
        for path in collect_files_with_extension(&sleigh_specs_root().join("languages"), "ldefs") {
            db.parse_ldefs_file(&path)?;
        }

        if db.languages.is_empty() {
            return Err("no Ghidra language definitions were loaded".to_string());
        }
        Ok(db)
    }

    pub fn query(
        &self,
        loader: &str,
        primary_key: &str,
        secondary_key: Option<&str>,
    ) -> Vec<QueryResult> {
        self.resolve_candidates(
            loader,
            primary_key,
            secondary_key,
            ExpectedLanguageFacts::any(),
        )
        .into_iter()
        .map(|candidate| QueryResult {
            language_id: candidate.language.id,
            compiler_spec_id: candidate.compiler_spec_id,
            preferred: candidate.preferred,
        })
        .collect()
    }

    fn select(
        &self,
        loader: &str,
        primary_key: &str,
        secondary_key: Option<&str>,
        expected: ExpectedLanguageFacts,
        format: &str,
        image_base: u64,
        raw_machine: String,
    ) -> ArchitectureSelectionResult {
        let mut candidates = self.resolve_candidates(loader, primary_key, secondary_key, expected);
        if candidates.is_empty() {
            return Err(ArchitectureSelectionError::UnsupportedMachine {
                format: format.to_string(),
                machine: raw_machine,
            });
        }

        let preferred_count = candidates
            .iter()
            .filter(|candidate| candidate.preferred)
            .count();
        if preferred_count > 0 {
            candidates.retain(|candidate| candidate.preferred);
        }
        dedup_candidates(&mut candidates);

        if candidates.len() != 1 {
            return Err(ArchitectureSelectionError::AmbiguousLoadSpec {
                format: format.to_string(),
                machine: format!("{raw_machine}, candidates={}", candidates.len()),
            });
        }

        let candidate = candidates.remove(0);
        Ok(selection_from_candidate(
            format,
            image_base,
            candidate,
            raw_machine,
            loader,
            primary_key,
            secondary_key,
        ))
    }

    fn resolve_candidates(
        &self,
        loader: &str,
        primary_key: &str,
        secondary_key: Option<&str>,
        expected: ExpectedLanguageFacts,
    ) -> Vec<SelectionCandidate> {
        let entries = self.matching_opinion_entries(loader, primary_key, secondary_key);
        let mut candidates = Vec::new();
        for entry in entries {
            for language in self.languages_for_query(&entry.query, expected) {
                for compiler_spec_id in &language.compiler_ids {
                    candidates.push(SelectionCandidate {
                        preferred: entry
                            .query
                            .compiler_spec_id
                            .as_deref()
                            .map(|preferred| preferred == compiler_spec_id)
                            .unwrap_or(false),
                        compiler_spec_id: compiler_spec_id.clone(),
                        language: language.clone(),
                    });
                }
            }
        }
        dedup_candidates(&mut candidates);
        candidates
    }

    fn matching_opinion_entries(
        &self,
        loader: &str,
        primary_key: &str,
        secondary_key: Option<&str>,
    ) -> Vec<&OpinionEntry> {
        let primary_matches: Vec<&OpinionEntry> = self
            .opinions
            .iter()
            .filter(|entry| entry.loader == loader && primary_matches(&entry.primary, primary_key))
            .collect();

        let secondary_exact: Vec<&OpinionEntry> = primary_matches
            .iter()
            .copied()
            .filter(|entry| entry.secondary.as_deref() == secondary_key)
            .collect();
        if !secondary_exact.is_empty() {
            return secondary_exact;
        }

        if let Some(secondary_key) = secondary_key {
            let secondary_masked: Vec<&OpinionEntry> = primary_matches
                .iter()
                .copied()
                .filter(|entry| {
                    entry
                        .secondary
                        .as_deref()
                        .map(|constraint| secondary_attribute_matches(secondary_key, constraint))
                        .unwrap_or(false)
                })
                .collect();
            if !secondary_masked.is_empty() {
                return secondary_masked;
            }
        }

        primary_matches
            .into_iter()
            .filter(|entry| entry.secondary.is_none())
            .collect()
    }

    fn languages_for_query(
        &self,
        query: &OpinionQuery,
        expected: ExpectedLanguageFacts,
    ) -> Vec<LanguageDescription> {
        self.languages
            .iter()
            .filter(|language| {
                query
                    .processor
                    .as_deref()
                    .map(|processor| processor == language.processor)
                    .unwrap_or(true)
                    && query
                        .endian
                        .as_deref()
                        .map(|endian| normalize_endian(endian) == language.endian)
                        .unwrap_or(true)
                    && query.size.map(|size| size == language.size).unwrap_or(true)
                    && query
                        .variant
                        .as_deref()
                        .map(|variant| variant == language.variant)
                        .unwrap_or(true)
                    && expected
                        .bitness
                        .map(|bitness| bitness == language.size)
                        .unwrap_or(true)
                    && expected
                        .endian
                        .map(|endian| endian == language.endian)
                        .unwrap_or(true)
            })
            .cloned()
            .collect()
    }

    fn parse_opinion_file(&mut self, path: &Path) -> Result<(), String> {
        let contents = fs::read_to_string(path)
            .map_err(|err| format!("read opinion {}: {err}", path.display()))?;
        let mut stack = vec![OpinionQuery::default()];
        for tag in scan_xml_tags(&contents) {
            if tag.name != "constraint" {
                continue;
            }
            if tag.closing {
                if stack.len() > 1 {
                    stack.pop();
                }
                continue;
            }

            let parent = stack.last().cloned().unwrap_or_default();
            let query = merge_query(parent, &tag.attrs)?;
            if let (Some(loader), Some(primary), Some(processor)) = (
                query.loader.as_ref(),
                query.primary.as_ref(),
                query.processor.as_ref(),
            ) {
                if !loader.is_empty() && !primary.is_empty() && !processor.is_empty() {
                    self.opinions.push(OpinionEntry {
                        loader: loader.clone(),
                        primary: primary.clone(),
                        secondary: query.secondary.clone(),
                        query: query.clone(),
                    });
                }
            }
            if !tag.self_closing {
                stack.push(query);
            }
        }
        Ok(())
    }

    fn parse_ldefs_file(&mut self, path: &Path) -> Result<(), String> {
        let contents = fs::read_to_string(path)
            .map_err(|err| format!("read ldefs {}: {err}", path.display()))?;
        let mut current: Option<LanguageDescription> = None;
        for tag in scan_xml_tags(&contents) {
            match tag.name.as_str() {
                "language" if !tag.closing => {
                    current = Some(LanguageDescription {
                        processor: required_attr(&tag.attrs, "processor", path)?,
                        endian: normalize_endian(&required_attr(&tag.attrs, "endian", path)?),
                        size: required_attr(&tag.attrs, "size", path)?
                            .parse::<u8>()
                            .map_err(|err| {
                                format!("invalid language size in {}: {err}", path.display())
                            })?,
                        variant: tag
                            .attrs
                            .get("variant")
                            .cloned()
                            .unwrap_or_else(|| "default".to_string()),
                        id: required_attr(&tag.attrs, "id", path)?,
                        compiler_ids: Vec::new(),
                    });
                }
                "language" if tag.closing => {
                    if let Some(language) = current.take() {
                        self.languages.push(language);
                    }
                }
                "compiler" if !tag.closing => {
                    if let Some(language) = current.as_mut() {
                        if let Some(id) = tag.attrs.get("id") {
                            language.compiler_ids.push(id.clone());
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

impl ExpectedLanguageFacts {
    const fn any() -> Self {
        Self {
            bitness: None,
            endian: None,
        }
    }

    const fn new(bitness: u8, endian: &'static str) -> Self {
        Self {
            bitness: Some(bitness),
            endian: Some(endian),
        }
    }
}

#[derive(Debug)]
struct XmlTag {
    name: String,
    attrs: BTreeMap<String, String>,
    closing: bool,
    self_closing: bool,
}

fn opinion_database() -> Result<&'static OpinionDatabase, String> {
    OPINION_DATABASE
        .get_or_init(OpinionDatabase::load)
        .as_ref()
        .map_err(Clone::clone)
}

pub fn query_opinion_database(
    loader: &str,
    primary_key: &str,
    secondary_key: Option<&str>,
) -> Result<Vec<QueryResult>, ArchitectureSelectionError> {
    Ok(opinion_database()
        .map_err(|err| ArchitectureSelectionError::MissingLanguage {
            format: loader.to_string(),
            machine: err,
        })?
        .query(loader, primary_key, secondary_key))
}

pub fn select_pe_load_spec(
    machine: u16,
    is_64bit: bool,
    image_base: u64,
) -> ArchitectureSelectionResult {
    let bitness = if is_64bit { 64 } else { 32 };
    let primary = u32::from(machine).to_string();
    select_from_opinion(
        PE_LOADER_NAME,
        &primary,
        None,
        ExpectedLanguageFacts::new(bitness, "little"),
        "PE",
        image_base,
        format!("PE Machine=0x{machine:04x}, is_64bit={is_64bit}"),
    )
}

pub fn select_coff_load_spec(
    machine: u16,
    is_64bit: bool,
    image_base: u64,
) -> ArchitectureSelectionResult {
    let bitness = if is_64bit { 64 } else { 32 };
    let primary = i16::from_ne_bytes(machine.to_ne_bytes()).to_string();
    select_from_opinion(
        MS_COFF_LOADER_NAME,
        &primary,
        None,
        ExpectedLanguageFacts::new(bitness, "little"),
        "COFF",
        image_base,
        format!("COFF Machine=0x{machine:04x}, is_64bit={is_64bit}"),
    )
    .or_else(|_| {
        select_from_opinion(
            COFF_LOADER_NAME,
            &primary,
            None,
            ExpectedLanguageFacts::new(bitness, "little"),
            "COFF",
            image_base,
            format!("COFF Machine=0x{machine:04x}, is_64bit={is_64bit}"),
        )
    })
}

pub fn select_elf_load_spec(
    machine: u16,
    class: u8,
    data_encoding: u8,
    flags: u32,
    image_base: u64,
) -> ArchitectureSelectionResult {
    let bitness = match class {
        ELFCLASS32 => 32,
        ELFCLASS64 => 64,
        _ => {
            return Err(ArchitectureSelectionError::UnsupportedMachine {
                format: "ELF".to_string(),
                machine: format!("class={class}, machine=0x{machine:04x}"),
            });
        }
    };
    let endian = match data_encoding {
        ELFDATA2LSB => "little",
        ELFDATA2MSB => "big",
        _ => {
            return Err(ArchitectureSelectionError::UnsupportedMachine {
                format: "ELF".to_string(),
                machine: format!("data_encoding={data_encoding}, machine=0x{machine:04x}"),
            });
        }
    };

    select_from_opinion(
        ELF_LOADER_NAME,
        &u32::from(machine).to_string(),
        Some(&flags.to_string()),
        ExpectedLanguageFacts::new(bitness, endian),
        "ELF",
        image_base,
        format!(
            "ELF class={class}, data_encoding={data_encoding}, e_machine=0x{machine:04x}, e_flags=0x{flags:08x}"
        ),
    )
}

pub fn select_macho_load_spec(
    cputype: i32,
    cpusubtype: i32,
    is_64bit: bool,
    image_base: u64,
) -> ArchitectureSelectionResult {
    let bitness = if is_64bit { 64 } else { 32 };
    let primary = macho_primary_key(cputype, cpusubtype);
    select_from_opinion(
        MACHO_LOADER_NAME,
        &primary,
        None,
        ExpectedLanguageFacts::new(bitness, "little"),
        "Mach-O",
        image_base,
        format!("Mach-O cputype=0x{cputype:x}, cpusubtype=0x{cpusubtype:x}, is_64bit={is_64bit}"),
    )
}

fn select_from_opinion(
    loader: &str,
    primary_key: &str,
    secondary_key: Option<&str>,
    expected: ExpectedLanguageFacts,
    format: &str,
    image_base: u64,
    raw_machine: String,
) -> ArchitectureSelectionResult {
    let db = opinion_database().map_err(|err| ArchitectureSelectionError::MissingLanguage {
        format: format.to_string(),
        machine: err,
    })?;
    db.select(
        loader,
        primary_key,
        secondary_key,
        expected,
        format,
        image_base,
        raw_machine,
    )
}

fn selection_from_candidate(
    format: &str,
    image_base: u64,
    candidate: SelectionCandidate,
    raw_machine: String,
    loader: &str,
    primary_key: &str,
    secondary_key: Option<&str>,
) -> (ArchitectureDescriptor, BinaryLoadSpec) {
    let language = candidate.language;
    let architecture = ArchitectureDescriptor::new(
        language.processor,
        language.endian,
        language.size,
        language.variant,
        Some(candidate.compiler_spec_id.clone()),
        raw_machine.clone(),
    );
    let mut load_spec = BinaryLoadSpec::new(
        format,
        image_base,
        language.id,
        candidate.compiler_spec_id,
        format!(
            "opinion: loader={loader}, primary={primary_key}, secondary={}",
            secondary_key.unwrap_or("<none>")
        ),
    );
    load_spec.preferred = candidate.preferred;
    (architecture, load_spec)
}

fn merge_query(
    mut parent: OpinionQuery,
    attrs: &BTreeMap<String, String>,
) -> Result<OpinionQuery, String> {
    assign_string(&mut parent.loader, attrs, "loader");
    assign_string(&mut parent.primary, attrs, "primary");
    assign_string(&mut parent.secondary, attrs, "secondary");
    assign_string(&mut parent.processor, attrs, "processor");
    assign_string(&mut parent.endian, attrs, "endian");
    assign_string(&mut parent.variant, attrs, "variant");
    assign_string(&mut parent.compiler_spec_id, attrs, "compilerSpecID");
    if let Some(size) = attrs.get("size") {
        parent.size = Some(
            size.parse::<u8>()
                .map_err(|err| format!("invalid opinion size {size}: {err}"))?,
        );
    }
    Ok(parent)
}

fn assign_string(target: &mut Option<String>, attrs: &BTreeMap<String, String>, key: &str) {
    if let Some(value) = attrs.get(key) {
        *target = Some(value.clone());
    }
}

fn scan_xml_tags(contents: &str) -> Vec<XmlTag> {
    let mut tags = Vec::new();
    let bytes = contents.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        let Some(end) = contents[i + 1..].find('>').map(|offset| i + 1 + offset) else {
            break;
        };
        let raw = contents[i + 1..end].trim();
        i = end + 1;
        if raw.is_empty()
            || raw.starts_with('?')
            || raw.starts_with("!--")
            || raw.starts_with("!DOCTYPE")
        {
            continue;
        }
        let closing = raw.starts_with('/');
        let body = raw.trim_start_matches('/').trim();
        let self_closing = body.ends_with('/');
        let body = body.trim_end_matches('/').trim();
        let (name, attrs) = parse_tag_body(body);
        if !name.is_empty() {
            tags.push(XmlTag {
                name,
                attrs,
                closing,
                self_closing,
            });
        }
    }
    tags
}

fn parse_tag_body(body: &str) -> (String, BTreeMap<String, String>) {
    let mut chars = body.char_indices();
    let name_end = chars
        .find_map(|(idx, ch)| ch.is_whitespace().then_some(idx))
        .unwrap_or(body.len());
    let name = body[..name_end].to_string();
    let mut attrs = BTreeMap::new();
    let mut rest = body[name_end..].trim();
    while !rest.is_empty() {
        let Some(eq_idx) = rest.find('=') else {
            break;
        };
        let key = rest[..eq_idx].trim();
        let after_eq = rest[eq_idx + 1..].trim_start();
        if !after_eq.starts_with('"') {
            break;
        }
        let Some(value_end) = after_eq[1..].find('"') else {
            break;
        };
        let value = &after_eq[1..1 + value_end];
        if !key.is_empty() {
            attrs.insert(key.to_string(), unescape_xml_attr(value));
        }
        rest = after_eq[1 + value_end + 1..].trim_start();
    }
    (name, attrs)
}

fn unescape_xml_attr(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn required_attr(
    attrs: &BTreeMap<String, String>,
    key: &str,
    path: &Path,
) -> Result<String, String> {
    attrs
        .get(key)
        .cloned()
        .ok_or_else(|| format!("missing {key} in {}", path.display()))
}

fn primary_matches(constraint: &str, primary_key: &str) -> bool {
    constraint
        .replace(char::is_whitespace, "")
        .split(',')
        .any(|token| token == primary_key)
}

fn secondary_attribute_matches(secondary_key: &str, constraint: &str) -> bool {
    let Ok(secondary_key_int) = secondary_key.parse::<u32>() else {
        return false;
    };
    let cleaned = constraint
        .chars()
        .filter(|ch| !ch.is_whitespace() && *ch != '_')
        .collect::<String>()
        .to_lowercase();
    if let Some(hex) = cleaned.strip_prefix("0x") {
        return u32::from_str_radix(hex, 16)
            .map(|value| value == secondary_key_int)
            .unwrap_or(false);
    }
    if let Some(binary) = cleaned.strip_prefix("0b") {
        let key_bits = format!("{secondary_key_int:032b}");
        let constraint_bits = format!("{binary:0>32}");
        return constraint_bits
            .chars()
            .zip(key_bits.chars())
            .all(|(constraint_bit, key_bit)| constraint_bit == '.' || constraint_bit == key_bit);
    }
    false
}

fn dedup_candidates(candidates: &mut Vec<SelectionCandidate>) {
    let mut seen = HashSet::new();
    candidates.retain(|candidate| {
        seen.insert((
            candidate.language.id.clone(),
            candidate.compiler_spec_id.clone(),
            candidate.preferred,
        ))
    });
}

fn normalize_endian(endian: &str) -> String {
    match endian {
        "LE" | "le" | "little" => "little".to_string(),
        "BE" | "be" | "big" => "big".to_string(),
        other => other.to_string(),
    }
}

fn macho_primary_key(cputype: i32, cpusubtype: i32) -> String {
    match cputype {
        MACHO_CPU_TYPE_ARM => format!("{cputype}.{cpusubtype}"),
        MACHO_CPU_TYPE_X86 | MACHO_CPU_TYPE_X86_64 | MACHO_CPU_TYPE_ARM64 => cputype.to_string(),
        _ => cputype.to_string(),
    }
}

fn ghidra_data_root() -> PathBuf {
    if let Some(path) = env::var_os("FISSION_GHIDRA_DATA_DIR") {
        return PathBuf::from(path);
    }
    repo_root().join("utils").join("ghidra-data")
}

fn sleigh_specs_root() -> PathBuf {
    if let Some(path) = env::var_os("FISSION_SLEIGH_SPEC_DIR") {
        let path = PathBuf::from(path);
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name == "languages")
            .unwrap_or(false)
        {
            return path.parent().unwrap_or(&path).to_path_buf();
        }
        return path;
    }
    repo_root().join("utils").join("sleigh-specs")
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .to_path_buf()
}

fn collect_files_with_extension(root: &Path, extension: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let Ok(entries) = fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext == extension)
                .unwrap_or(false)
            {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::binary_format::MACHO_CPU_TYPE_ARM64;
    use crate::core_constants::{
        ELFCLASS32, ELFCLASS64, ELFDATA2LSB, ELFDATA2MSB, EM_AARCH64, EM_ARM, EM_LOONGARCH,
        EM_MIPS, EM_PPC64, EM_RISCV, EM_X86_64, IMAGE_FILE_MACHINE_AMD64, IMAGE_FILE_MACHINE_ARM64,
        IMAGE_FILE_MACHINE_I386,
    };

    #[test]
    fn selects_pe_amd64_from_opinion_data() {
        let (_, spec) = select_pe_load_spec(IMAGE_FILE_MACHINE_AMD64, true, 0x140000000)
            .expect("select PE amd64");
        assert_eq!(spec.pair.language_id.as_str(), "x86:LE:64:default");
        assert_eq!(spec.pair.compiler_spec_id.as_str(), "windows");
        assert!(
            spec.source
                .starts_with("opinion: loader=Portable Executable (PE)")
        );
    }

    #[test]
    fn selects_pe_i386_from_opinion_data() {
        let (_, spec) =
            select_pe_load_spec(IMAGE_FILE_MACHINE_I386, false, 0x400000).expect("select PE i386");
        assert_eq!(spec.pair.language_id.as_str(), "x86:LE:32:default");
        assert_eq!(spec.pair.compiler_spec_id.as_str(), "windows");
    }

    #[test]
    fn selects_pe_aarch64_from_opinion_data() {
        let (_, spec) =
            select_pe_load_spec(IMAGE_FILE_MACHINE_ARM64, true, 0).expect("select PE arm64");
        assert_eq!(spec.pair.language_id.as_str(), "AARCH64:LE:64:v8A");
        assert_eq!(spec.pair.compiler_spec_id.as_str(), "windows");
    }

    #[test]
    fn rejects_unknown_pe_machine() {
        assert!(matches!(
            select_pe_load_spec(0xffff, true, 0),
            Err(ArchitectureSelectionError::UnsupportedMachine { .. })
        ));
    }

    #[test]
    fn selects_elf_x86_64_from_opinion_data() {
        let (_, spec) =
            select_elf_load_spec(EM_X86_64, ELFCLASS64, ELFDATA2LSB, 0, 0).expect("select ELF");
        assert_eq!(spec.pair.language_id.as_str(), "x86:LE:64:default");
        assert_eq!(spec.pair.compiler_spec_id.as_str(), "gcc");
    }

    #[test]
    fn selects_elf_aarch64_little_endian_from_opinion_data() {
        let (_, spec) =
            select_elf_load_spec(EM_AARCH64, ELFCLASS64, ELFDATA2LSB, 0, 0).expect("select ELF");
        assert_eq!(spec.pair.language_id.as_str(), "AARCH64:LE:64:v8A");
    }

    #[test]
    fn selects_elf_arm_secondary_mask_from_opinion_data() {
        let (_, spec) =
            select_elf_load_spec(EM_ARM, ELFCLASS32, ELFDATA2LSB, 0, 0).expect("select ELF");
        assert_eq!(spec.pair.language_id.as_str(), "ARM:LE:32:v8");

        let (_, be8_spec) = select_elf_load_spec(EM_ARM, ELFCLASS32, ELFDATA2MSB, 0x0080_0000, 0)
            .expect("select ARM BE8 ELF");
        assert_eq!(
            be8_spec.pair.language_id.as_str(),
            "ARM:LEBE:32:v8LEInstruction"
        );
    }

    #[test]
    fn selects_elf_riscv64_little_endian() {
        let (_, spec) =
            select_elf_load_spec(EM_RISCV, ELFCLASS64, ELFDATA2LSB, 0x5, 0).expect("select ELF");
        assert_eq!(spec.pair.language_id.as_str(), "RISCV:LE:64:default");
    }

    #[test]
    fn selects_elf_mips32_r6_little_endian() {
        let (_, spec) = select_elf_load_spec(EM_MIPS, ELFCLASS32, ELFDATA2LSB, 0x9000_1405, 0)
            .expect("select ELF");
        assert_eq!(spec.pair.language_id.as_str(), "MIPS:LE:32:R6");
    }

    #[test]
    fn ppc64_little_endian_fails_closed_when_opinion_is_ambiguous() {
        assert!(matches!(
            select_elf_load_spec(EM_PPC64, ELFCLASS64, ELFDATA2LSB, 0x2, 0),
            Err(ArchitectureSelectionError::AmbiguousLoadSpec { .. })
        ));
    }

    #[test]
    fn selects_elf_loongarch64_lp64d() {
        let (_, spec) = select_elf_load_spec(EM_LOONGARCH, ELFCLASS64, ELFDATA2LSB, 0x43, 0)
            .expect("select ELF");
        assert_eq!(spec.pair.language_id.as_str(), "Loongarch:LE:64:lp64d");
    }

    #[test]
    fn rejects_endian_mismatch_for_x86_64_elf() {
        assert!(matches!(
            select_elf_load_spec(EM_X86_64, ELFCLASS64, ELFDATA2MSB, 0, 0),
            Err(ArchitectureSelectionError::UnsupportedMachine { .. })
        ));
    }

    #[test]
    fn selects_macho_apple_silicon_from_opinion_data() {
        let (_, spec) =
            select_macho_load_spec(MACHO_CPU_TYPE_ARM64, 0, true, 0).expect("select Mach-O");
        assert_eq!(spec.pair.language_id.as_str(), "AARCH64:LE:64:AppleSilicon");
    }

    #[test]
    fn opinion_query_supports_secondary_binary_masks() {
        let results =
            query_opinion_database(ELF_LOADER_NAME, &u32::from(EM_ARM).to_string(), Some("0"))
                .expect("query opinion");
        assert!(
            results
                .iter()
                .any(|result| result.language_id == "ARM:LE:32:v8")
        );
    }

    #[test]
    fn checked_in_ghidra_manifest_keeps_expected_coverage() {
        let manifest: serde_json::Value = serde_json::from_str(include_str!(
            "../../../utils/sleigh-specs/ghidra_language_manifest.json"
        ))
        .expect("parse Ghidra language manifest");
        assert_eq!(manifest["processor_count"], 38);
        assert_eq!(manifest["variant_count"], 146);

        let entries = manifest["entries"].as_array().expect("entries array");
        for language_id in [
            "x86:LE:64:default",
            "x86:LE:32:default",
            "AARCH64:LE:64:v8A",
            "AARCH64:LE:64:AppleSilicon",
            "ARM:LE:32:v7",
        ] {
            assert!(
                entries.iter().any(|entry| {
                    entry["language_id"].as_str() == Some(language_id)
                        || entry["language_ids"]
                            .as_array()
                            .map(|ids| ids.iter().any(|id| id.as_str() == Some(language_id)))
                            .unwrap_or(false)
                }),
                "missing selectable language {language_id}"
            );
        }
    }
}
