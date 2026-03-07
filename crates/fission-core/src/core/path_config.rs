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
    /// FID database directory
    pub fid_dir: Option<PathBuf>,
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

impl PathConfig {
    /// Detect paths based on current working directory and environment
    pub fn detect() -> Self {
        let workspace_root = crate::core::utils::find_workspace_root("FISSION_ROOT");

        let signatures_base = workspace_root.as_ref().and_then(|root| {
            let direct = root.join("signatures");
            if direct.exists() {
                return Some(direct);
            }

            let legacy = root.join("utils").join("signatures");
            if legacy.exists() {
                return Some(legacy);
            }

            None
        });

        let fid_dir = signatures_base
            .as_ref()
            .map(|base| base.join("fid"))
            .filter(|p| p.exists())
            .or_else(|| crate::core::utils::find_existing_dir(FID_SEARCH_DIRS));

        let gdt_dir = workspace_root
            .as_ref()
            .and_then(|root| {
                let direct = root.join("signatures").join("typeinfo").join("win32");
                if direct.exists() {
                    return Some(direct);
                }

                let legacy = root
                    .join("utils")
                    .join("signatures")
                    .join("typeinfo")
                    .join("win32");
                if legacy.exists() {
                    return Some(legacy);
                }

                None
            })
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
            gdt_dir,
            die_dir,
            patterns_dir,
            workspace_root,
        }
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

    /// Find a specific FID file
    pub fn find_fid_file(&self, filename: &str) -> Option<PathBuf> {
        if let Some(ref fid_dir) = self.fid_dir {
            let path = fid_dir.join(filename);
            if path.exists() {
                return Some(path);
            }
        }
        Self::find_file_in_dirs(FID_SEARCH_DIRS, filename)
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

    /// Get GDT (Ghidra Data Type) file path
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

        // Fallback search
        for prefix in GDT_SEARCH_PREFIXES {
            let path = Path::new(prefix).join(filename);
            if path.exists() {
                return Some(path);
            }
        }
        None
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
