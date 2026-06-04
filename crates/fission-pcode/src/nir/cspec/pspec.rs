//! Ghidra-style `.pspec` (Processor Specification) runtime parser.
//!
//! # Ghidra Design
//!
//! Ghidra's `ProgramDataTypeManager` loads `.pspec` at program open time to:
//! 1. Set `<programcounter register="..."/>` — the authoritative PC register name.
//! 2. Apply `<context_data><context_set>` — default context register values for decode.
//! 3. Apply `<context_data><tracked_set>` — register values treated as constants
//!    throughout the whole program (e.g. `DF=0` on x86-64, `spsr=0` on ARM).
//! 4. Register `<volatile>` memory ranges — prevent constant-folding reads from
//!    memory-mapped I/O or hardware register spaces.
//! 5. Annotate `<register_data>` — hidden registers (internal SLEIGH state variables)
//!    to suppress from output, and vector-lane sizes for SIMD register display.
//!
//! # Fission Design
//!
//! We parse the same information and expose it via [`PspecDocument`].  Callers in
//! `fission-decompiler` inject this into [`NirRenderOptions`] fields so that:
//! - `pspec_hidden_registers`: names to filter from NIR output (e.g. `bit64`, `segover`)
//! - `pspec_tracked_context`: `(name, value)` pairs → constant-fold in dataflow
//! - `pspec_programcounter`: authoritative PC register name for PC-rel patterns
//! - `pspec_volatile_ranges`: suppress optimisation across these address ranges
//!
//! Zero-dependency: hand-written XML state machine, same approach as `cspec.rs`.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::OnceLock;

// ── Public data structures ────────────────────────────────────────────────────

/// A volatile address range from `<volatile>/<range .../>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PspecVolatileRange {
    /// Address space name (e.g. `"register"`, `"ram"`).
    pub space: String,
    /// Inclusive first address in the space.
    pub first: u64,
    /// Inclusive last address in the space.
    pub last: u64,
}

/// Parsed contents of a `.pspec` file.
#[derive(Debug, Clone, Default)]
pub struct PspecDocument {
    /// Name of the program counter register (from `<programcounter register="..."/>`).
    ///
    /// Examples: `"RIP"` (x86-64), `"pc"` (ARM, MIPS), `"PC"` (PowerPC).
    pub programcounter: Option<String>,

    /// Context register values from `<context_data><context_set>`.
    ///
    /// Used to initialize the SLEIGH context at decode time — same as the compile-time
    /// `infer_default_context_from_pspec` path in `lowering.rs`, but consumed at runtime.
    pub context_set: Vec<(String, u64)>,

    /// Register values from `<context_data><tracked_set>`.
    ///
    /// Ghidra treats these as constants throughout the whole function during decompilation.
    /// The canonical example is `("DF", 0)` on x86-64, which collapses direction-flag
    /// dependent string operations to their single-direction form.
    pub tracked_set: Vec<(String, u64)>,

    /// Volatile address ranges from `<volatile>/<range .../>`.
    ///
    /// Reads from these ranges must not be cached or constant-folded.
    pub volatile_ranges: Vec<PspecVolatileRange>,

    /// Register names marked `hidden="true"` in `<register_data>`.
    ///
    /// These are internal SLEIGH state variables (e.g. `bit64`, `segover`,
    /// `repneprefx`, `rexWprefix`, context flags) that should never appear in
    /// decompiled output.
    pub hidden_registers: HashSet<String>,

    /// Register group names: `register_name → group_name`.
    ///
    /// Groups include `"FLAGS"`, `"DEBUG"`, `"CONTROL"`, `"AVX"`, `"ST"`, etc.
    /// Used for display grouping and to suppress flag/control registers from
    /// appearing as local variables.
    pub register_groups: HashMap<String, String>,

    /// Registers with SIMD vector lane sizes from `vector_lane_sizes="..."`.
    ///
    /// Used to annotate SIMD registers with valid lane widths for vector display.
    pub vector_lane_sizes: HashMap<String, Vec<u32>>,
}

impl PspecDocument {
    /// Parse a `.pspec` file at the given path.
    ///
    /// Returns `None` if the file cannot be read.  Parse errors on individual
    /// elements are silently skipped — partial data is better than no data.
    pub fn parse_file(path: &Path) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;
        Some(Self::parse_str(&content))
    }

    /// Parse `.pspec` XML from a string.
    ///
    /// Hand-written state machine — no XML library required.
    pub fn parse_str(content: &str) -> Self {
        let mut doc = PspecDocument::default();

        // Track which block we are currently inside:
        // "context_set", "tracked_set", or "" (top-level / other).
        let mut in_block: &str = "";
        // Are we inside <volatile ...>?
        let mut in_volatile = false;
        // outputop / inputop from <volatile> tag (informational; not used yet).
        let mut _volatile_outputop: Option<String> = None;

        let mut rest = content;
        loop {
            let Some(lt) = rest.find('<') else { break };
            rest = &rest[lt + 1..];

            // Skip XML declaration / comments.
            if rest.starts_with('?') || rest.starts_with('!') {
                if let Some(end) = rest.find('>') {
                    rest = &rest[end + 1..];
                } else {
                    break;
                }
                continue;
            }

            // Read tag name (stop at whitespace, '>', '/').
            let tag_end = rest
                .find(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/')
                .unwrap_or(rest.len());
            let tag = &rest[..tag_end];

            let close = rest.find('>').unwrap_or(rest.len());
            let segment = &rest[..close];
            rest = &rest[close.saturating_add(1).min(rest.len())..];

            match tag {
                // ── Top-level single-element tags ─────────────────────────
                "programcounter" => {
                    if let Some(reg) = extract_attr(segment, "register") {
                        doc.programcounter = Some(reg.to_string());
                    }
                }

                // ── Block enter ───────────────────────────────────────────
                "context_set" => {
                    in_block = "context_set";
                }
                "tracked_set" => {
                    in_block = "tracked_set";
                }
                "volatile" => {
                    in_volatile = true;
                    _volatile_outputop =
                        extract_attr(segment, "outputop").map(str::to_string);
                }

                // ── Block-interior elements ───────────────────────────────
                "set" => {
                    if let (Some(name), Some(val_str)) =
                        (extract_attr(segment, "name"), extract_attr(segment, "val"))
                    {
                        if let Some(val) = parse_u64(val_str) {
                            match in_block {
                                "context_set" => {
                                    doc.context_set.push((name.to_string(), val));
                                }
                                "tracked_set" => {
                                    doc.tracked_set.push((name.to_string(), val));
                                }
                                _ => {}
                            }
                        }
                    }
                }
                "range" => {
                    if in_volatile {
                        if let (Some(space), Some(first_str), Some(last_str)) = (
                            extract_attr(segment, "space"),
                            extract_attr(segment, "first"),
                            extract_attr(segment, "last"),
                        ) {
                            if let (Some(first), Some(last)) =
                                (parse_u64(first_str), parse_u64(last_str))
                            {
                                doc.volatile_ranges.push(PspecVolatileRange {
                                    space: space.to_string(),
                                    first,
                                    last,
                                });
                            }
                        }
                    }
                }

                // ── Register data ─────────────────────────────────────────
                "register" => {
                    if let Some(name) = extract_attr(segment, "name") {
                        let name = name.to_string();
                        // hidden="true"
                        if extract_attr(segment, "hidden")
                            .map_or(false, |v| v.eq_ignore_ascii_case("true"))
                        {
                            doc.hidden_registers.insert(name.clone());
                        }
                        // group="..."
                        if let Some(group) = extract_attr(segment, "group") {
                            doc.register_groups
                                .insert(name.clone(), group.to_string());
                        }
                        // vector_lane_sizes="1,2,4,8"
                        if let Some(lanes_str) = extract_attr(segment, "vector_lane_sizes") {
                            let lanes: Vec<u32> = lanes_str
                                .split(',')
                                .filter_map(|s| s.trim().parse::<u32>().ok())
                                .collect();
                            if !lanes.is_empty() {
                                doc.vector_lane_sizes.insert(name, lanes);
                            }
                        }
                    }
                }

                // ── Block exit ────────────────────────────────────────────
                "/context_set" | "/tracked_set" => {
                    in_block = "";
                }
                "/volatile" => {
                    in_volatile = false;
                }

                _ => {}
            }
        }

        doc
    }

    /// Returns `true` if the named register is a hidden SLEIGH internal variable.
    ///
    /// Use this in the NIR builder/printer to suppress internal context registers
    /// (e.g. `bit64`, `segover`, `rexWprefix`, `xmmTmp1`) from output.
    pub fn is_hidden(&self, register_name: &str) -> bool {
        self.hidden_registers.contains(register_name)
    }

    /// Returns `true` if the register belongs to a FLAGS-class group.
    ///
    /// Flag registers (CF, ZF, SF, OF, DF, etc.) should typically be handled
    /// as sub-expressions within conditions rather than standalone variables.
    pub fn is_flag_register(&self, register_name: &str) -> bool {
        self.register_groups
            .get(register_name)
            .map_or(false, |g| g.eq_ignore_ascii_case("FLAGS"))
    }
}

// ── Global per-path cache ─────────────────────────────────────────────────────

type PspecCache = std::collections::HashMap<std::path::PathBuf, Option<PspecDocument>>;

static PSPEC_CACHE: OnceLock<std::sync::Mutex<PspecCache>> = OnceLock::new();

fn pspec_cache() -> &'static std::sync::Mutex<PspecCache> {
    PSPEC_CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Load and cache a `.pspec` document by absolute path.
///
/// Returns `None` if the file cannot be read. Cached per process lifetime.
pub fn load_pspec_path(path: &Path) -> Option<PspecDocument> {
    let key = path.to_path_buf();
    {
        let guard = pspec_cache().lock().ok()?;
        if let Some(cached) = guard.get(&key) {
            return cached.clone();
        }
    }
    let doc = PspecDocument::parse_file(path);
    if let Ok(mut guard) = pspec_cache().lock() {
        guard.entry(key).or_insert_with(|| doc.clone());
    }
    doc
}

/// Load a `.pspec` using the pre-resolved path from a [`LdefsEntry`].
///
/// Convenience wrapper — returns `None` when the entry has no `pspec_filename`.
pub fn load_pspec_for_entry(entry: &super::ldefs::LdefsEntry) -> Option<PspecDocument> {
    let path = entry.pspec_path()?;
    load_pspec_path(&path)
}

/// Load a `.pspec` for a `(language_id, compiler_spec_id)` pair via the `.ldefs` index.
///
/// This is the **primary** entry point for all pspec loading.  It mirrors the design
/// of `loader::load_cspec_for_pair` — same `.ldefs` index, same languages root.
pub fn load_pspec_for_pair(
    languages_root: &Path,
    language_id: &str,
    compiler_spec_id: &str,
) -> Option<PspecDocument> {
    let index = super::ldefs::global_ldefs_index(languages_root);
    let key = (language_id.to_string(), compiler_spec_id.to_string());
    let entry = index.get(&key)?;
    load_pspec_for_entry(entry)
}

// ── XML helpers ───────────────────────────────────────────────────────────────

/// Extract `key="value"` from an XML attribute string.
fn extract_attr<'a>(segment: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("{key}=\"");
    let start = segment.find(needle.as_str())? + needle.len();
    let end = segment[start..].find('"')? + start;
    Some(&segment[start..end])
}

/// Parse a `u64` value from decimal or `0x`-prefixed hex.
fn parse_u64(s: &str) -> Option<u64> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).ok()
    } else {
        s.parse::<u64>().ok()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_x86_64_pspec_tracked_set_and_hidden() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<processor_spec>
  <programcounter register="RIP"/>
  <context_data>
    <context_set space="ram">
      <set name="addrsize" val="2"/>
      <set name="longMode" val="1"/>
    </context_set>
    <tracked_set space="ram">
      <set name="DF" val="0"/>
    </tracked_set>
  </context_data>
  <register_data>
    <register name="bit64" hidden="true"/>
    <register name="segover" hidden="true"/>
    <register name="CF" group="FLAGS"/>
    <register name="ZF" group="FLAGS"/>
    <register name="XMM0" group="AVX" vector_lane_sizes="1,2,4,8"/>
  </register_data>
</processor_spec>"#;

        let doc = PspecDocument::parse_str(xml);

        assert_eq!(doc.programcounter.as_deref(), Some("RIP"));

        assert_eq!(doc.context_set.len(), 2);
        assert!(doc
            .context_set
            .iter()
            .any(|(n, v)| n == "addrsize" && *v == 2));
        assert!(doc
            .context_set
            .iter()
            .any(|(n, v)| n == "longMode" && *v == 1));

        assert_eq!(doc.tracked_set.len(), 1);
        assert_eq!(doc.tracked_set[0], ("DF".to_string(), 0));

        assert!(doc.is_hidden("bit64"));
        assert!(doc.is_hidden("segover"));
        assert!(!doc.is_hidden("CF"));

        assert!(doc.is_flag_register("CF"));
        assert!(doc.is_flag_register("ZF"));
        assert!(!doc.is_flag_register("XMM0"));

        assert_eq!(
            doc.vector_lane_sizes.get("XMM0").map(|v| v.as_slice()),
            Some(&[1u32, 2, 4, 8][..])
        );
    }

    #[test]
    fn parse_aarch64_volatile_range() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<processor_spec>
  <programcounter register="pc"/>
  <volatile outputop="cWrite" inputop="cRead">
    <range space="register" first="0x1000" last="0x2fff"/>
  </volatile>
</processor_spec>"#;

        let doc = PspecDocument::parse_str(xml);
        assert_eq!(doc.programcounter.as_deref(), Some("pc"));
        assert_eq!(doc.volatile_ranges.len(), 1);
        let vr = &doc.volatile_ranges[0];
        assert_eq!(vr.space, "register");
        assert_eq!(vr.first, 0x1000);
        assert_eq!(vr.last, 0x2fff);
        assert!(doc.hidden_registers.is_empty());
    }

    #[test]
    fn parse_arm_cortex_default_symbols_ignored() {
        // Ensure we don't crash or emit junk for <default_symbols>.
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<processor_spec>
  <programcounter register="pc"/>
  <context_data>
    <context_set space="ram">
      <set name="TMode" val="1" description="0 for ARM 32-bit, 1 for THUMB 16-bit"/>
    </context_set>
    <tracked_set space="ram">
      <set name="spsr" val="0"/>
    </tracked_set>
  </context_data>
  <default_symbols>
    <symbol name="Reset" address="ram:0x4" entry="true" type="code_ptr"/>
  </default_symbols>
</processor_spec>"#;

        let doc = PspecDocument::parse_str(xml);
        assert_eq!(doc.programcounter.as_deref(), Some("pc"));
        assert_eq!(doc.context_set.len(), 1);
        assert_eq!(doc.context_set[0], ("TMode".to_string(), 1));
        assert_eq!(doc.tracked_set.len(), 1);
        assert_eq!(doc.tracked_set[0], ("spsr".to_string(), 0));
    }

    /// Integration test: load the real x86-64.pspec and verify key invariants.
    #[test]
    fn real_x86_64_pspec_has_df_tracked_and_hidden_registers() {
        let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap()
            .to_path_buf();
        let pspec_path = repo_root.join("utils/sleigh-specs/languages/x86/x86-64.pspec");
        if !pspec_path.exists() {
            return; // skip if not in full repo checkout
        }

        let doc = PspecDocument::parse_file(&pspec_path).expect("parse x86-64.pspec");
        assert_eq!(doc.programcounter.as_deref(), Some("RIP"));
        // DF=0 must be in tracked_set
        assert!(
            doc.tracked_set.iter().any(|(n, v)| n == "DF" && *v == 0),
            "x86-64 pspec must have DF=0 in tracked_set: {:?}",
            doc.tracked_set
        );
        // Internal SLEIGH variables must be hidden
        for hidden in ["bit64", "segover", "repneprefx", "rexWprefix"] {
            assert!(
                doc.is_hidden(hidden),
                "{hidden} must be hidden in x86-64.pspec"
            );
        }
        // FLAGS group must be populated
        for flag in ["CF", "ZF", "SF", "OF", "DF"] {
            assert!(
                doc.is_flag_register(flag),
                "{flag} must be in FLAGS group in x86-64.pspec"
            );
        }
    }

    /// Integration test: load the real AARCH64.pspec and verify volatile range.
    #[test]
    fn real_aarch64_pspec_has_volatile_register_range() {
        let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap()
            .to_path_buf();
        let pspec_path =
            repo_root.join("utils/sleigh-specs/languages/AARCH64/AARCH64.pspec");
        if !pspec_path.exists() {
            return;
        }

        let doc = PspecDocument::parse_file(&pspec_path).expect("parse AARCH64.pspec");
        assert_eq!(doc.programcounter.as_deref(), Some("pc"));
        assert!(
            doc.volatile_ranges
                .iter()
                .any(|vr| vr.space == "register" && vr.first == 0x1000),
            "AARCH64 pspec should have register-space volatile range: {:?}",
            doc.volatile_ranges
        );
    }

    /// Integration test: build ldefs index and verify pspec_filename is captured.
    #[test]
    fn ldefs_index_captures_pspec_filename_for_x86_64() {
        let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap()
            .to_path_buf();
        let languages_root = repo_root.join("utils/sleigh-specs/languages");
        if !languages_root.exists() {
            return;
        }
        let index = super::super::ldefs::build_ldefs_index(&languages_root);
        let entry = index
            .get(&("x86:LE:64:default".to_string(), "gcc".to_string()))
            .expect("x86:LE:64:default/gcc in ldefs index");
        assert_eq!(
            entry.pspec_filename.as_deref(),
            Some("x86-64.pspec"),
            "pspec_filename from ldefs"
        );
        let pspec_path = entry.pspec_path().expect("pspec_path from entry");
        assert!(
            pspec_path.exists(),
            "pspec file must exist: {}",
            pspec_path.display()
        );
    }
}
