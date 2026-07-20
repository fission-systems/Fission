//! Ghidra-style `.dwarf` (DWARF register mapping) runtime parser.
//!
//! # Ghidra Design
//!
//! Ghidra resolves a DWARF `DW_OP_reg*`/`DW_OP_regx` register number to a Ghidra
//! register via a per-language `.dwarf` XML file referenced from `.ldefs` as
//! `<external_name tool="DWARF.register.mapping.file" name="x86-64.dwarf"/>`
//! (see `DWARFRegisterMappings.java`). The file maps DWARF register numbers to
//! Ghidra register name strings, with an `auto_count` shorthand for sequential
//! ranges (`dwarf="8" ghidra="R8" auto_count="8"` covers dwarf 8..16 → R8..R15,
//! by incrementing the trailing digit run of `ghidra`).
//!
//! # Fission Design
//!
//! We parse the same files to resolve `DwarfLocation::Register("regN")` locals
//! (see `fission_loader::loader::types::DwarfLocation`) to a Ghidra register
//! name, which callers then resolve to `(offset, size)` via
//! [`super::register_model::RegisterModel::lookup_name`] — same two-step
//! pipeline as `.cspec`/`.pspec`. Zero-dependency hand-written XML state
//! machine, same approach as `pspec.rs`.

use crate::midend::HashMap;
use std::path::Path;
use std::sync::OnceLock;

/// Parsed contents of a `.dwarf` register mapping file.
#[derive(Debug, Clone, Default)]
pub struct DwarfRegisterMappings {
    /// DWARF register number → Ghidra register name string (e.g. `"RDI"`).
    pub by_dwarf_num: HashMap<u32, String>,
}

impl DwarfRegisterMappings {
    /// Parse a `.dwarf` file at the given path.
    ///
    /// Returns `None` if the file cannot be read.
    pub fn parse_file(path: &Path) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;
        Some(Self::parse_str(&content))
    }

    /// Parse `.dwarf` XML from a string.
    pub fn parse_str(content: &str) -> Self {
        let mut doc = DwarfRegisterMappings::default();

        let mut rest = content;
        loop {
            let Some(lt) = rest.find('<') else { break };
            rest = &rest[lt + 1..];

            if rest.starts_with('?') || rest.starts_with('!') {
                if let Some(end) = rest.find('>') {
                    rest = &rest[end + 1..];
                } else {
                    break;
                }
                continue;
            }

            let tag_end = rest
                .find(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/')
                .unwrap_or(rest.len());
            let tag = &rest[..tag_end];

            let close = rest.find('>').unwrap_or(rest.len());
            let segment = &rest[..close];
            rest = &rest[close.saturating_add(1).min(rest.len())..];

            if tag == "register_mapping" {
                if let (Some(dwarf_str), Some(ghidra_name)) =
                    (extract_attr(segment, "dwarf"), extract_attr(segment, "ghidra"))
                {
                    let Some(base_dwarf) = parse_u32(dwarf_str) else {
                        continue;
                    };
                    let auto_count = extract_attr(segment, "auto_count").and_then(parse_u32);
                    match auto_count {
                        Some(count) if count > 0 => {
                            if let Some((prefix, base_num)) = split_trailing_digits(ghidra_name) {
                                for i in 0..count {
                                    doc.by_dwarf_num.insert(
                                        base_dwarf + i,
                                        format!("{prefix}{}", base_num + i),
                                    );
                                }
                            } else {
                                // Non-conforming auto_count name (Ghidra would
                                // reject this) — fall back to a single mapping.
                                doc.by_dwarf_num
                                    .insert(base_dwarf, ghidra_name.to_string());
                            }
                        }
                        _ => {
                            doc.by_dwarf_num
                                .insert(base_dwarf, ghidra_name.to_string());
                        }
                    }
                }
            }
        }

        doc
    }

    /// Resolve a DWARF register number to its Ghidra register name.
    pub fn ghidra_name_for(&self, dwarf_num: u32) -> Option<&str> {
        self.by_dwarf_num.get(&dwarf_num).map(String::as_str)
    }
}

/// Split a Ghidra register name like `"R8"` into (`"R"`, `8`) — mirrors
/// Ghidra's `([a-zA-Z]+)([0-9]+)` full-string match for `auto_count` expansion.
fn split_trailing_digits(name: &str) -> Option<(&str, u32)> {
    let first_digit = name.find(|c: char| c.is_ascii_digit())?;
    let (prefix, digits) = name.split_at(first_digit);
    if !prefix.is_empty()
        && prefix.chars().all(|c| c.is_ascii_alphabetic())
        && !digits.is_empty()
        && digits.chars().all(|c| c.is_ascii_digit())
    {
        digits.parse::<u32>().ok().map(|n| (prefix, n))
    } else {
        None
    }
}

// ── Global per-path cache ─────────────────────────────────────────────────────

type DwarfRegsCache = std::collections::HashMap<std::path::PathBuf, Option<DwarfRegisterMappings>>;

static DWARF_REGS_CACHE: OnceLock<std::sync::Mutex<DwarfRegsCache>> = OnceLock::new();

fn dwarf_regs_cache() -> &'static std::sync::Mutex<DwarfRegsCache> {
    DWARF_REGS_CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::default()))
}

/// Load and cache a `.dwarf` register mapping document by absolute path.
///
/// Returns `None` if the file cannot be read. Cached per process lifetime.
pub fn load_dwarf_regs_path(path: &Path) -> Option<DwarfRegisterMappings> {
    let key = path.to_path_buf();
    {
        let guard = dwarf_regs_cache().lock().ok()?;
        if let Some(cached) = guard.get(&key) {
            return cached.clone();
        }
    }
    let doc = DwarfRegisterMappings::parse_file(path);
    if let Ok(mut guard) = dwarf_regs_cache().lock() {
        guard.entry(key).or_insert_with(|| doc.clone());
    }
    doc
}

/// Load a `.dwarf` register mapping using the pre-resolved path from a
/// [`super::ldefs::LdefsEntry`].
///
/// Convenience wrapper — returns `None` when the entry declares no
/// `DWARF.register.mapping.file` external name.
pub fn load_dwarf_regs_for_entry(
    entry: &super::ldefs::LdefsEntry,
) -> Option<DwarfRegisterMappings> {
    let path = entry.dwarf_mapping_path()?;
    load_dwarf_regs_path(&path)
}

/// Load `.dwarf` register mappings for a `(language_id, compiler_spec_id)` pair
/// via the `.ldefs` index.
///
/// This is the **primary** entry point — mirrors `pspec::load_pspec_for_pair`.
pub fn load_dwarf_regs_for_pair(
    languages_root: &Path,
    language_id: &str,
    compiler_spec_id: &str,
) -> Option<DwarfRegisterMappings> {
    let index = super::ldefs::global_ldefs_index(languages_root);
    let key = (language_id.to_string(), compiler_spec_id.to_string());
    let entry = index.get(&key)?;
    load_dwarf_regs_for_entry(entry)
}

// ── XML helpers ───────────────────────────────────────────────────────────────

/// Extract `key="value"` from an XML attribute string.
fn extract_attr<'a>(segment: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("{key}=\"");
    let start = segment.find(needle.as_str())? + needle.len();
    let end = segment[start..].find('"')? + start;
    Some(&segment[start..end])
}

/// Parse a decimal `u32` attribute value.
fn parse_u32(s: &str) -> Option<u32> {
    s.trim().parse::<u32>().ok()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_and_stackpointer_mappings() {
        let xml = r#"<dwarf>
	<register_mappings>
		<register_mapping dwarf="0" ghidra="RAX"/>
		<register_mapping dwarf="5" ghidra="RDI"/>
		<register_mapping dwarf="7" ghidra="RSP" stackpointer="true"/>
	</register_mappings>
</dwarf>"#;
        let doc = DwarfRegisterMappings::parse_str(xml);
        assert_eq!(doc.ghidra_name_for(0), Some("RAX"));
        assert_eq!(doc.ghidra_name_for(5), Some("RDI"));
        assert_eq!(doc.ghidra_name_for(7), Some("RSP"));
        assert_eq!(doc.ghidra_name_for(99), None);
    }

    #[test]
    fn parse_auto_count_expands_sequential_registers() {
        let xml = r#"<dwarf>
	<register_mappings>
		<register_mapping dwarf="8" ghidra="R8" auto_count="8"/> <!-- R8..R15 -->
		<register_mapping dwarf="17" ghidra="XMM0" auto_count="16"/> <!-- XMM0..XMM15 -->
	</register_mappings>
</dwarf>"#;
        let doc = DwarfRegisterMappings::parse_str(xml);
        assert_eq!(doc.ghidra_name_for(8), Some("R8"));
        assert_eq!(doc.ghidra_name_for(9), Some("R9"));
        assert_eq!(doc.ghidra_name_for(15), Some("R15"));
        assert_eq!(doc.ghidra_name_for(16), None);
        assert_eq!(doc.ghidra_name_for(17), Some("XMM0"));
        assert_eq!(doc.ghidra_name_for(32), Some("XMM15"));
        assert_eq!(doc.ghidra_name_for(33), None);
    }

    #[test]
    fn parse_ignores_comments_and_unrelated_elements() {
        let xml = r#"<dwarf>
	<register_mappings>
		<!-- <register_mapping dwarf="58" ghidra="FSBASE"/> **not implemented** -->
		<register_mapping dwarf="49" ghidra="rflags"/>
	</register_mappings>
	<call_frame_cfa value="8"/>
	<stack_frame register="RBP" offset="-8" />
</dwarf>"#;
        let doc = DwarfRegisterMappings::parse_str(xml);
        assert_eq!(doc.ghidra_name_for(58), None);
        assert_eq!(doc.ghidra_name_for(49), Some("rflags"));
        assert_eq!(doc.by_dwarf_num.len(), 1);
    }

    /// Integration test: load the real x86-64.dwarf and verify System V ABI
    /// argument registers resolve correctly (used by the register-locals
    /// naming feature).
    #[test]
    fn real_x86_64_dwarf_maps_arg_registers() {
        let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap()
            .to_path_buf();
        let dwarf_path = repo_root.join("utils/sleigh-specs/languages/x86/x86-64.dwarf");
        if !dwarf_path.exists() {
            return; // skip if not in full repo checkout
        }
        let doc = DwarfRegisterMappings::parse_file(&dwarf_path).expect("parse x86-64.dwarf");
        assert_eq!(doc.ghidra_name_for(5), Some("RDI"));
        assert_eq!(doc.ghidra_name_for(4), Some("RSI"));
        assert_eq!(doc.ghidra_name_for(1), Some("RDX"));
        assert_eq!(doc.ghidra_name_for(8), Some("R8"));
        assert_eq!(doc.ghidra_name_for(15), Some("R15"));
    }

    /// Integration test: resolve x86-64.dwarf via the `.ldefs` index, the same
    /// path the register-locals feature will use at runtime.
    #[test]
    fn ldefs_resolves_dwarf_mapping_for_x86_64() {
        let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap()
            .to_path_buf();
        let languages_root = repo_root.join("utils/sleigh-specs/languages");
        if !languages_root.exists() {
            return;
        }
        let doc = load_dwarf_regs_for_pair(&languages_root, "x86:LE:64:default", "gcc")
            .expect("x86-64.dwarf via .ldefs index");
        assert_eq!(doc.ghidra_name_for(5), Some("RDI"));
    }
}
