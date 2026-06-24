//! Path Configuration for Fission Resources
//!
//! Centralized path resolution for all signature files, type databases,
//! and other resources. Mirrors C++ fission::config::PathConfig.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

/// Global path configuration instance
pub static PATHS: LazyLock<PathConfig> = LazyLock::new(PathConfig::detect);

/// Search directories for FID databases (relative to working directory)
const FID_SEARCH_DIRS: &[&str] = &[
    "./signatures/fid/",
    "../signatures/fid/",
    "../../signatures/fid/",
    "./utils/signatures/fid/",
    "../utils/signatures/fid/",
    "../../utils/signatures/fid/",
];

/// Search directories for Java-packed FID databases (.fidb, Ghidra original format)
const FIDB_JAVA_SEARCH_DIRS: &[&str] = &[
    "./signatures/fidb_java/",
    "../signatures/fidb_java/",
    "../../signatures/fidb_java/",
    "./utils/signatures/fidb_java/",
    "../utils/signatures/fidb_java/",
    "../../utils/signatures/fidb_java/",
];

/// Search directories for DIE signatures
const DIE_SEARCH_DIRS: &[&str] = &[
    "./signatures/die/",
    "../signatures/die/",
    "../../signatures/die/",
    "./utils/signatures/die/",
    "../utils/signatures/die/",
    "../../utils/signatures/die/",
];

/// Search directories for GDT files
const GDT_SEARCH_PREFIXES: &[&str] = &[
    "../../signatures/typeinfo/win32/",
    "../signatures/typeinfo/win32/",
    "./signatures/typeinfo/win32/",
    "signatures/typeinfo/win32/",
    "../../utils/signatures/typeinfo/win32/",
    "../utils/signatures/typeinfo/win32/",
    "./utils/signatures/typeinfo/win32/",
    "utils/signatures/typeinfo/win32/",
];

/// Search directories for pattern signatures
const PATTERN_SEARCH_DIRS: &[&str] = &[
    "./signatures/patterns/",
    "../signatures/patterns/",
    "../../signatures/patterns/",
    "./utils/signatures/patterns/",
    "../utils/signatures/patterns/",
    "../../utils/signatures/patterns/",
];

/// MSVC FID database filenames by version (x64)
const MSVC_FID_FILES_X64: &[&str] = &[
    "vs2019_x64.fidbf",
    "vs2017_x64.fidbf",
    "vs2015_x64.fidbf",
    "vs2012_x64.fidbf",
    "vsOlder_x64.fidbf",
];

/// MSVC FID database filenames by version (x86)
const MSVC_FID_FILES_X86: &[&str] = &[
    "vs2019_x86.fidbf",
    "vs2017_x86.fidbf",
    "vs2015_x86.fidbf",
    "vs2012_x86.fidbf",
    "vsOlder_x86.fidbf",
];

/// GCC/MinGW FID database filenames
const GCC_FID_FILES_X64: &[&str] = &["gcc-x86.LE.64.default.fidbf", "gcc-AARCH64.LE.64.v8A.fidbf"];

const GCC_FID_FILES_X86: &[&str] = &["gcc-x86.LE.32.default.fidbf", "gcc-ARM.LE.32.v8.fidbf"];

/// Path configuration for Fission resources
#[derive(Debug, Clone)]
pub struct PathConfig {
    /// Base directory for signatures
    pub signatures_base: Option<PathBuf>,
    /// FID database directory (`.fidbf` files, both raw and Java-packed)
    pub fid_dir: Option<PathBuf>,
    /// Java-packed FID database directory (`.fidb` files, Ghidra original format)
    pub fidb_java_dir: Option<PathBuf>,
    /// GDT (type info) directory
    pub gdt_dir: Option<PathBuf>,
    /// DIE signatures directory
    pub die_dir: Option<PathBuf>,
    /// Pattern signatures directory
    pub patterns_dir: Option<PathBuf>,
    /// Workspace root (detected or from env)
    pub workspace_root: Option<PathBuf>,
}

impl Default for PathConfig {
    fn default() -> Self {
        Self::detect()
    }
}

fn workspace_signatures_base(workspace_root: &PathBuf) -> Option<PathBuf> {
    let direct = workspace_root.join("signatures");
    if direct.exists() {
        return Some(direct);
    }

    let legacy = workspace_root.join("utils").join("signatures");
    if legacy.exists() {
        return Some(legacy);
    }

    None
}

fn workspace_gdt_dir(workspace_root: &PathBuf) -> Option<PathBuf> {
    let direct = workspace_root
        .join("signatures")
        .join("typeinfo")
        .join("win32");
    if direct.exists() {
        return Some(direct);
    }

    let legacy = workspace_root
        .join("utils")
        .join("signatures")
        .join("typeinfo")
        .join("win32");
    if legacy.exists() {
        return Some(legacy);
    }

    None
}

fn workspace_die_mirror(workspace_root: &PathBuf) -> Option<PathBuf> {
    let direct = workspace_root
        .join("signatures")
        .join("die")
        .join("detect-it-easy");
    if direct.is_dir() {
        return Some(direct);
    }
    let legacy = workspace_root
        .join("utils")
        .join("signatures")
        .join("die")
        .join("detect-it-easy");
    legacy.is_dir().then_some(legacy)
}

impl PathConfig {
    /// Detect paths based on current working directory and environment
    pub fn detect() -> Self {
        let workspace_root = crate::core::utils::find_workspace_root("FISSION_ROOT");

        let signatures_base = crate::core::resource_roots::resolve_signatures_base_from_roots(
            crate::core::resource_roots::explicit_bundle_roots(),
        )
        .or_else(|| workspace_root.as_ref().and_then(workspace_signatures_base))
        .or_else(|| {
            crate::core::resource_roots::resolve_signatures_base_from_roots(
                crate::core::resource_roots::ambient_bundle_roots(),
            )
        });

        let fid_dir = signatures_base
            .as_ref()
            .map(|base| base.join("fid"))
            .filter(|p| p.exists())
            .or_else(|| crate::core::utils::find_existing_dir(FID_SEARCH_DIRS));

        let fidb_java_dir = signatures_base
            .as_ref()
            .map(|base| base.join("fidb_java"))
            .filter(|p| p.exists())
            .or_else(|| crate::core::utils::find_existing_dir(FIDB_JAVA_SEARCH_DIRS));

        let gdt_dir = signatures_base
            .as_ref()
            .map(|base| base.join("typeinfo").join("win32"))
            .filter(|p| p.exists())
            .or_else(|| workspace_root.as_ref().and_then(workspace_gdt_dir))
            .or_else(|| crate::core::utils::find_existing_dir(GDT_SEARCH_PREFIXES));

        let die_dir = signatures_base
            .as_ref()
            .map(|base| base.join("die"))
            .filter(|p| p.exists())
            .or_else(|| crate::core::utils::find_existing_dir(DIE_SEARCH_DIRS));

        let patterns_dir = signatures_base
            .as_ref()
            .map(|base| base.join("patterns"))
            .filter(|p| p.exists())
            .or_else(|| crate::core::utils::find_existing_dir(PATTERN_SEARCH_DIRS));

        Self {
            signatures_base,
            fid_dir,
            fidb_java_dir,
            gdt_dir,
            die_dir,
            patterns_dir,
            workspace_root,
        }
    }

    /// `win_api_signatures.txt` (pipe-separated API signatures), if present.
    pub fn get_win_api_signatures_path(&self) -> Option<PathBuf> {
        let filename = "win_api_signatures.txt";
        if let Some(ref gdt_dir) = self.gdt_dir {
            let path = gdt_dir.join(filename);
            if path.exists() {
                return Some(path);
            }
        }
        if let Some(ref base) = self.signatures_base {
            let path = base.join("typeinfo").join("win32").join(filename);
            if path.exists() {
                return Some(path);
            }
        }
        self.workspace_root.as_ref().and_then(|root| {
            let path = root
                .join("utils")
                .join("signatures")
                .join("typeinfo")
                .join("win32")
                .join(filename);
            path.exists().then_some(path)
        })
    }

    /// `ntoskrnl_signatures.txt` (Windows kernel ntoskrnl/HAL API signatures), if present.
    pub fn get_ntoskrnl_signatures_path(&self) -> Option<PathBuf> {
        let filename = "ntoskrnl_signatures.txt";
        if let Some(ref gdt_dir) = self.gdt_dir {
            let path = gdt_dir.join(filename);
            if path.exists() {
                return Some(path);
            }
        }
        if let Some(ref base) = self.signatures_base {
            let path = base.join("typeinfo").join("win32").join(filename);
            if path.exists() {
                return Some(path);
            }
        }
        self.workspace_root.as_ref().and_then(|root| {
            let path = root
                .join("utils")
                .join("signatures")
                .join("typeinfo")
                .join("win32")
                .join(filename);
            path.exists().then_some(path)
        })
    }

    /// `generic_clib_signatures.txt` (pipe-separated generic C library signatures), if present.
    pub fn get_generic_clib_signatures_path(&self) -> Option<PathBuf> {
        let filename = "generic_clib_signatures.txt";
        if let Some(ref gdt_dir) = self.gdt_dir {
            let path = gdt_dir.join(filename);
            if path.exists() {
                return Some(path);
            }
        }
        if let Some(ref base) = self.signatures_base {
            let path = base.join("typeinfo").join("generic").join(filename);
            if path.exists() {
                return Some(path);
            }
        }
        self.workspace_root.as_ref().and_then(|root| {
            let path = root
                .join("utils")
                .join("signatures")
                .join("typeinfo")
                .join("generic")
                .join(filename);
            path.exists().then_some(path)
        })
    }

    /// `generic_clib_64_signatures.txt` (x86-64 generic C library signatures), if present.
    pub fn get_generic_clib_64_signatures_path(&self) -> Option<PathBuf> {
        let filename = "generic_clib_64_signatures.txt";
        if let Some(ref base) = self.signatures_base {
            let path = base.join("typeinfo").join("generic").join(filename);
            if path.exists() {
                return Some(path);
            }
        }
        self.workspace_root.as_ref().and_then(|root| {
            let path = root
                .join("utils")
                .join("signatures")
                .join("typeinfo")
                .join("generic")
                .join(filename);
            path.exists().then_some(path)
        })
    }

    /// `mac_osx_signatures.txt` (macOS API signatures), if present.
    pub fn get_mac_osx_signatures_path(&self) -> Option<PathBuf> {
        let filename = "mac_osx_signatures.txt";
        if let Some(ref base) = self.signatures_base {
            let path = base.join("typeinfo").join("mac_10.9").join(filename);
            if path.exists() {
                return Some(path);
            }
        }
        self.workspace_root.as_ref().and_then(|root| {
            let path = root
                .join("utils")
                .join("signatures")
                .join("typeinfo")
                .join("mac_10.9")
                .join(filename);
            path.exists().then_some(path)
        })
    }

    /// Parent directory for Go API snapshot JSON files (`typeinfo/golang/`).
    ///
    /// Pass the result into [`fission_signatures::golang_typeinfo::GoTypeinfoDatabase::load_for_binary`]
    /// as `typeinfo_dir` (it will append `golang/` itself).
    pub fn get_golang_typeinfo_dir(&self) -> Option<PathBuf> {
        if let Some(ref base) = self.signatures_base {
            let path = base.join("typeinfo");
            if path.join("golang").exists() {
                return Some(path);
            }
        }
        self.workspace_root.as_ref().and_then(|root| {
            let path = root.join("utils").join("signatures").join("typeinfo");
            path.join("golang").exists().then_some(path)
        })
    }

    /// JSON file under the Windows typeinfo corpus (e.g. `base_types.json`).
    pub fn get_win32_typeinfo_json_path(&self, filename: &str) -> Option<PathBuf> {
        if let Some(ref gdt_dir) = self.gdt_dir {
            let path = gdt_dir.join(filename);
            if path.exists() {
                return Some(path);
            }
        }
        self.workspace_root.as_ref().and_then(|root| {
            let path = root
                .join("utils")
                .join("signatures")
                .join("typeinfo")
                .join("win32")
                .join(filename);
            path.exists().then_some(path)
        })
    }

    /// Detect It Easy `.sg` mirror root (`detect-it-easy/`), if present.
    ///
    /// Resolution uses resolved DIE paths and workspace layout only — no cwd upward walks.
    #[must_use]
    pub fn die_mirror_root(&self) -> Option<PathBuf> {
        if let Some(die_json) = self.get_die_signatures_path() {
            let candidate = die_json.parent()?.join("detect-it-easy");
            if candidate.is_dir() {
                return Some(candidate);
            }
        }
        if let Some(ref dd) = self.die_dir {
            let candidate = dd.join("detect-it-easy");
            if candidate.is_dir() {
                return Some(candidate);
            }
        }
        if let Some(ref sb) = self.signatures_base {
            let candidate = sb.join("die").join("detect-it-easy");
            if candidate.is_dir() {
                return Some(candidate);
            }
        }
        self.workspace_root.as_ref().and_then(workspace_die_mirror)
    }

    /// Find a file within search paths
    fn find_file_in_dirs(dirs: &[&str], filename: &str) -> Option<PathBuf> {
        crate::core::utils::find_file_in_dirs(dirs, filename)
    }

    // ========================================================================
    // FID Database Resolution
    // ========================================================================

    /// Get FID database path for a specific compiler/architecture
    pub fn get_fid_path(&self, is_64bit: bool, compiler_id: Option<&str>) -> Option<PathBuf> {
        let filename = Self::get_fid_filename(is_64bit, compiler_id);

        // Try FID directory first
        if let Some(ref fid_dir) = self.fid_dir {
            let path = fid_dir.join(&filename);
            if path.exists() {
                return Some(path);
            }
        }

        // Fallback to search paths
        Self::find_file_in_dirs(FID_SEARCH_DIRS, &filename)
    }

    /// Get all available FID database paths for an architecture
    pub fn get_all_fid_paths(&self, is_64bit: bool) -> Vec<PathBuf> {
        let file_lists: Vec<&[&str]> = if is_64bit {
            vec![MSVC_FID_FILES_X64, GCC_FID_FILES_X64]
        } else {
            vec![MSVC_FID_FILES_X86, GCC_FID_FILES_X86]
        };

        let mut result = Vec::new();
        for list in file_lists {
            for filename in list {
                if let Some(path) = self.find_fid_file(filename) {
                    result.push(path);
                }
            }
        }
        result
    }

    /// Get preferred FID database paths for a specific target.
    ///
    /// This intentionally returns a smaller, target-aware subset than
    /// [`Self::get_all_fid_paths`] so prepare-time initialization does not
    /// eagerly load unrelated FID databases.
    pub fn get_preferred_fid_paths(
        &self,
        is_64bit: bool,
        format: Option<&str>,
        compiler_id: Option<&str>,
    ) -> Vec<PathBuf> {
        let compiler = compiler_id.unwrap_or_default().to_ascii_lowercase();
        let is_pe = format
            .map(|value| value.to_ascii_uppercase().starts_with("PE"))
            .unwrap_or(false);

        if compiler.contains("gcc") || compiler.contains("mingw") {
            let primary = if is_64bit {
                GCC_FID_FILES_X64.first().copied()
            } else {
                GCC_FID_FILES_X86.first().copied()
            };
            return primary
                .into_iter()
                .filter_map(|name| self.find_fid_file(name))
                .collect();
        }

        if compiler.contains("clang") && !is_pe {
            let primary = if is_64bit {
                GCC_FID_FILES_X64.first().copied()
            } else {
                GCC_FID_FILES_X86.first().copied()
            };
            return primary
                .into_iter()
                .filter_map(|name| self.find_fid_file(name))
                .collect();
        }

        if let Some(primary) = self.get_fid_path(is_64bit, compiler_id) {
            return vec![primary];
        }

        let family = if is_64bit {
            MSVC_FID_FILES_X64
        } else {
            MSVC_FID_FILES_X86
        };
        family
            .iter()
            .filter_map(|name| self.find_fid_file(name))
            .collect()
    }

    /// Find a specific FID file. Looks for `.fidbf` in `fid_dir` first, then
    /// falls back to `fidb_java_dir` with the `.fidb` extension (same basename).
    pub fn find_fid_file(&self, filename: &str) -> Option<PathBuf> {
        if let Some(ref fid_dir) = self.fid_dir {
            let path = fid_dir.join(filename);
            if path.exists() {
                return Some(path);
            }
        }
        if let Some(ref java_dir) = self.fidb_java_dir {
            let fidb_name = filename.strip_suffix(".fidbf").unwrap_or(filename);
            let fidb_name = format!("{fidb_name}.fidb");
            let path = java_dir.join(&fidb_name);
            if path.exists() {
                return Some(path);
            }
        }
        if let Some(p) = Self::find_file_in_dirs(FID_SEARCH_DIRS, filename) {
            return Some(p);
        }
        let fidb_name = filename.strip_suffix(".fidbf").unwrap_or(filename);
        let fidb_name = format!("{fidb_name}.fidb");
        Self::find_file_in_dirs(FIDB_JAVA_SEARCH_DIRS, &fidb_name)
    }

    /// Get FID filename based on compiler and architecture
    fn get_fid_filename(is_64bit: bool, compiler_id: Option<&str>) -> String {
        let suffix = if is_64bit { "_x64.fidbf" } else { "_x86.fidbf" };

        let compiler = compiler_id.unwrap_or("");
        let base = if compiler.contains("vs2017") {
            "vs2017"
        } else if compiler.contains("vs2015") {
            "vs2015"
        } else if compiler.contains("vs2012") {
            "vs2012"
        } else if compiler.contains("gcc") || compiler.contains("mingw") {
            return if is_64bit {
                GCC_FID_FILES_X64.first().map(|s| s.to_string())
            } else {
                GCC_FID_FILES_X86.first().map(|s| s.to_string())
            }
            .unwrap_or_else(|| format!("gcc{}", suffix));
        } else {
            "vs2019" // Default
        };

        format!("{}{}", base, suffix)
    }

    // ========================================================================
    // GDT Resolution
    // ========================================================================

    /// Get primary GDT (Ghidra Data Type) file path.
    pub fn get_gdt_path(&self, is_64bit: bool) -> Option<PathBuf> {
        let filename = if is_64bit {
            "windows_vs12_64.gdt"
        } else {
            "windows_vs12_32.gdt"
        };

        if let Some(ref gdt_dir) = self.gdt_dir {
            let path = gdt_dir.join(filename);
            if path.exists() {
                return Some(path);
            }
        }

        for prefix in GDT_SEARCH_PREFIXES {
            let path = Path::new(prefix).join(filename);
            if path.exists() {
                return Some(path);
            }
        }
        None
    }

    /// Discover all applicable GDT files for the target platform and compiler.
    ///
    /// Returns an ordered list: primary platform GDT first, then supplementary
    /// GDTs (generic C, Rust, Go, macOS) if present in the typeinfo tree.
    pub fn get_all_gdt_paths(
        &self,
        is_64bit: bool,
        format: Option<&str>,
        compiler_id: Option<&str>,
    ) -> Vec<PathBuf> {
        let mut paths = Vec::new();

        let is_pe = format
            .map(|f| f.to_ascii_uppercase().starts_with("PE"))
            .unwrap_or(false);
        let is_macho = format
            .map(|f| {
                f.to_ascii_uppercase().starts_with("MACH")
                    || f.to_ascii_uppercase().starts_with("MACHO")
            })
            .unwrap_or(false);
        let compiler = compiler_id.unwrap_or("").to_ascii_lowercase();

        // Primary platform GDT
        if is_pe {
            if let Some(p) = self.get_gdt_path(is_64bit) {
                paths.push(p);
            }
        }

        // Generic C library GDT — applicable to all platforms
        let generic_name = if is_64bit {
            "generic_clib_64.gdt"
        } else {
            "generic_clib.gdt"
        };
        if let Some(p) = self.find_typeinfo_file(generic_name) {
            paths.push(p);
        }

        // Rust
        if compiler.contains("rust") || compiler.contains("rustc") {
            if let Some(p) = self.find_typeinfo_file("rust-common.gdt") {
                paths.push(p);
            }
        }

        // Go
        if compiler.contains("go") || compiler.contains("golang") {
            // Pick the latest golang GDT available (version-agnostic naming).
            if let Some(p) = self.find_typeinfo_file("golang_1.25_anybit_any.gdt") {
                paths.push(p);
            } else if let Some(p) = self.find_typeinfo_file("golang_1.24_anybit_any.gdt") {
                paths.push(p);
            }
        }

        // macOS
        if is_macho {
            if let Some(p) = self.find_typeinfo_file("mac_osx.gdt") {
                paths.push(p);
            }
        }

        paths
    }

    /// Search all `typeinfo/` subdirectories for a specific GDT or JSON file.
    fn find_typeinfo_file(&self, filename: &str) -> Option<PathBuf> {
        let subdirs = ["win32", "generic", "golang", "mac_10.9", "rust"];
        if let Some(ref base) = self.signatures_base {
            for subdir in &subdirs {
                let path = base.join("typeinfo").join(subdir).join(filename);
                if path.exists() {
                    return Some(path);
                }
            }
        }
        self.workspace_root.as_ref().and_then(|root| {
            for subdir in &subdirs {
                let path = root
                    .join("utils")
                    .join("signatures")
                    .join("typeinfo")
                    .join(subdir)
                    .join(filename);
                if path.exists() {
                    return Some(path);
                }
            }
            None
        })
    }

    // ========================================================================
    // DIE Signatures Resolution
    // ========================================================================

    /// Get DIE signature database path
    pub fn get_die_signatures_path(&self) -> Option<PathBuf> {
        let filename = "pe_signatures.json";

        if let Some(ref die_dir) = self.die_dir {
            let path = die_dir.join(filename);
            if path.exists() {
                return Some(path);
            }
        }

        Self::find_file_in_dirs(DIE_SEARCH_DIRS, filename)
    }

    // ========================================================================
    // Pattern Signatures Resolution
    // ========================================================================

    /// Get pattern signature file path
    pub fn get_pattern_file(&self, filename: &str) -> Option<PathBuf> {
        if let Some(ref patterns_dir) = self.patterns_dir {
            let path = patterns_dir.join(filename);
            if path.exists() {
                return Some(path);
            }
        }
        Self::find_file_in_dirs(PATTERN_SEARCH_DIRS, filename)
    }

    /// Get all available pattern signature files
    pub fn get_all_pattern_files(&self) -> Vec<PathBuf> {
        let patterns_dir = match &self.patterns_dir {
            Some(dir) => dir,
            None => return Vec::new(),
        };

        std::fs::read_dir(patterns_dir)
            .ok()
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|ext| ext == "json"))
                    .collect()
            })
            .unwrap_or_default()
    }

    // ========================================================================
    // Common Symbol Files
    // ========================================================================

    /// Get common symbol file paths
    pub fn get_common_symbol_files(&self) -> Vec<PathBuf> {
        let files = ["common_symbols_win32.txt", "common_symbols_win64.txt"];

        files.iter().filter_map(|f| self.find_fid_file(f)).collect()
    }

    // ========================================================================
    // Utility
    // ========================================================================

    /// Check if paths are properly configured
    pub fn is_configured(&self) -> bool {
        self.fid_dir.is_some() || self.gdt_dir.is_some() || self.die_dir.is_some()
    }

    /// Get summary of configured paths
    pub fn summary(&self) -> String {
        let mut lines = Vec::new();
        lines.push(format!("Workspace: {:?}", self.workspace_root));
        lines.push(format!("FID Dir:   {:?}", self.fid_dir));
        lines.push(format!("GDT Dir:   {:?}", self.gdt_dir));
        lines.push(format!("DIE Dir:   {:?}", self.die_dir));
        lines.push(format!("Patterns:  {:?}", self.patterns_dir));
        lines.join("\n")
    }
}

/// Find the Sleigh specification directory for the Ghidra decompiler.
///
/// Search order:
/// 1. `FISSION_SLA_DIR` environment variable
/// 2. CWD / `ghidra_decompiler/languages` (and `../` parent)
/// 3. Executable parent dir / same relative candidates  
/// 4. Falls back to the literal string `"ghidra_decompiler/languages"`
pub fn find_sla_dir() -> String {
    const RELATIVE_CANDIDATES: &[&str] = &[
        "ghidra_decompiler/languages",
        "../ghidra_decompiler/languages",
        "../../ghidra_decompiler/languages",
        "../../../ghidra_decompiler/languages",
    ];

    // 1. Environment variable
    if let Ok(env_path) = std::env::var("FISSION_SLA_DIR") {
        let p = Path::new(&env_path);
        if p.is_dir() {
            return env_path;
        }
    }

    // 2. CWD-relative
    if let Ok(cwd) = std::env::current_dir() {
        for candidate in RELATIVE_CANDIDATES {
            let path = cwd.join(candidate);
            if path.is_dir() {
                return path.to_string_lossy().into_owned();
            }
        }
    }

    // 3. Exe-relative
    if let Ok(exe) = std::env::current_exe()
        && let Some(exe_dir) = exe.parent()
    {
        for candidate in RELATIVE_CANDIDATES {
            let path = exe_dir.join(candidate);
            if path.is_dir() {
                return path.to_string_lossy().into_owned();
            }
        }
    }

    // 4. Fallback
    RELATIVE_CANDIDATES[0].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_config_detect() {
        let config = PathConfig::detect();
        // Should at least detect workspace if running from project
        println!("PathConfig:\n{}", config.summary());
    }

    #[test]
    fn test_fid_filename_generation() {
        assert_eq!(
            PathConfig::get_fid_filename(true, Some("vs2019")),
            "vs2019_x64.fidbf"
        );
        assert_eq!(
            PathConfig::get_fid_filename(false, Some("vs2017")),
            "vs2017_x86.fidbf"
        );
        assert!(PathConfig::get_fid_filename(true, Some("gcc")).contains("gcc"));
    }
}
