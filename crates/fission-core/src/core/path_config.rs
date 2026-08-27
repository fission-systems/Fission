//! Path Configuration for Fission Resources
//!
//! Centralized path resolution for all signature files, type databases,
//! and other resources. Mirrors C++ fission::config::PathConfig.

use crate::core::resource_layout::ResourceKind;
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

/// glibc FID database filenames -- covers statically-linked C-library
/// functions (`select`, `read`, `close`, `pause`, `__errno_location`, ...),
/// distinct from `GCC_FID_FILES_*` which only identifies compiler-emitted
/// helpers (CRT startup, stack-protector glue). Both categories exist in
/// Ghidra's own default FID configuration and are tried together there;
/// this pair of files sat on disk unreferenced by any path-selection
/// function until a real sample-set gcc/glibc-linked ELF (bin_000.elf)
/// showed Fission resolving under half the call sites Ghidra resolved for
/// the identical function, entirely because this database was never tried.
const LIBC_FID_FILES_X64: &[&str] = &[
    "libc-x86.LE.64.default.fidbf",
    "libc-AARCH64.LE.64.v8A.fidbf",
];

const LIBC_FID_FILES_X86: &[&str] = &["libc-x86.LE.32.default.fidbf", "libc-ARM.LE.32.v8.fidbf"];

/// Resolve the (compiler-ID, C-library) FID database pair for a non-x86
/// processor, if `utils/signatures/fid/` has one. Both `GCC_FID_FILES_X64`
/// and `LIBC_FID_FILES_X64`/`_X86` already *contain* an ARM/AArch64 entry
/// each -- but `get_preferred_fid_paths` only ever reads `.first()` from
/// those arrays, so the second entry was unreachable dead data. This is the
/// architecture-aware selector that actually reaches it, plus the other
/// processors `utils/signatures/fid/` has both a `gcc-*` and `libc-*`
/// database for (MIPS/PowerPC/SPARC/SuperH4/avr8/pa-risc/68000). Some pairs
/// mix endian/bitness variants because that reflects what's vendored, not a
/// deliberate match -- `FidDatabaseSet::discover_for_load_spec`'s own
/// `language_id` filter is the real safety net for a mismatched pick, so
/// being permissive here costs nothing but an extra skipped-database parse.
fn non_x86_fid_files(processor: &str, is_64bit: bool) -> Option<(&'static str, &'static str)> {
    let p = processor.to_ascii_lowercase();
    if p.contains("aarch64") || (p.contains("arm") && is_64bit) {
        Some((
            "gcc-AARCH64.LE.64.v8A.fidbf",
            "libc-AARCH64.LE.64.v8A.fidbf",
        ))
    } else if p.contains("arm") {
        Some(("gcc-ARM.LE.32.v8.fidbf", "libc-ARM.LE.32.v8.fidbf"))
    } else if p.contains("mips") {
        Some((
            "gcc-MIPS.BE.32.default.fidbf",
            "libc-MIPS.LE.32.default.fidbf",
        ))
    } else if p.contains("powerpc") || p.contains("ppc") {
        Some((
            "gcc-PowerPC.BE.32.default.fidbf",
            "libc-PowerPC.BE.32.default.fidbf",
        ))
    } else if p.contains("sparc") {
        Some((
            "gcc-sparc.BE.64.default.fidbf",
            "libc-sparc.BE.32.default.fidbf",
        ))
    } else if p.contains("superh") || p.contains("sh4") {
        Some((
            "gcc-SuperH4.BE.32.default.fidbf",
            "libc-SuperH4.LE.32.default.fidbf",
        ))
    } else if p.contains("avr") {
        Some((
            "gcc-avr8.LE.16.extended.fidbf",
            "libc-avr8.LE.16.extended.fidbf",
        ))
    } else if p.contains("pa-risc") || p.contains("parisc") {
        Some((
            "gcc-pa-risc.BE.32.default.fidbf",
            "libc-pa-risc.BE.32.default.fidbf",
        ))
    } else if p.contains("68000") || p.contains("coldfire") || p.contains("m68k") {
        Some((
            "gcc-68000.BE.32.Coldfire.fidbf",
            "libc-68000.BE.32.Coldfire.fidbf",
        ))
    } else {
        None
    }
}

/// Third-party statically-linked library FID databases, keyed by the DIE
/// (Detect-It-Easy) `DetectionType::Library` name that should trigger them
/// (see `get_library_fid_paths`). All three OpenSSL builds are tried since
/// the detected library name alone doesn't reliably pin a version.
const OPENSSL_FID_FILES_X64: &[&str] = &[
    "sigmoid-openssl-1.1.0f-x86.LE.64.default.fidbf",
    "sigmoid-openssl-1.0.2l-x86.LE.64.default.fidbf",
    "sigmoid-openssl-1.0.1u-x86.LE.64.default.fidbf",
];
const OPENSSL_FID_FILES_X86: &[&str] = &[
    "sigmoid-openssl-1.1.0f-x86.LE.32.default.fidbf",
    "sigmoid-openssl-1.0.2l-x86.LE.32.default.fidbf",
    "sigmoid-openssl-1.0.1u-x86.LE.32.default.fidbf",
];
const SDL_FID_FILES_X64: &[&str] = &["SDL-el-x86.LE.64.default.fidbf"];
const SDL_FID_FILES_X86: &[&str] = &["SDL-el-x86.LE.32.default.fidbf"];
const QT5_FID_FILES_X64: &[&str] = &["qt5-el7-x86.LE.64.default.fidbf"];
const QT5_FID_FILES_X86: &[&str] = &["qt5-el7-x86.LE.32.default.fidbf"];
const LIBSODIUM_FID_FILES_X64: &[&str] = &["libsodium-x86.LE.64.default.fidbf"];
const LIBSODIUM_FID_FILES_X86: &[&str] = &["libsodium-x86.LE.32.default.fidbf"];

/// Path configuration for Fission resources
#[derive(Debug, Clone)]
pub struct PathConfig {
    /// The `utils/` directory itself, from which every resource kind resolves
    /// through `resource_layout`. Present for both the split and legacy trees.
    pub utils_root: Option<PathBuf>,
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

        // `utils/` itself, for resolving the packer sources under `source/`.
        let utils_root = signatures_base
            .as_ref()
            .and_then(|base| base.parent().map(Path::to_path_buf))
            .or_else(|| workspace_root.as_ref().map(|root| root.join("utils")));

        let fid_dir = signatures_base
            .as_ref()
            .map(|base| base.join("fid"))
            .filter(|p| p.exists())
            .or_else(|| crate::core::utils::find_existing_dir(FID_SEARCH_DIRS));

        // Ghidra's Java-packed FID databases now live under `source/`, unshipped:
        // every one has a `.fidbf` sibling, so `find_fid_file` never falls back
        // to them. The search dirs stay for a tree that predates the move.
        let fidb_java_dir = utils_root
            .as_ref()
            .map(|root| ResourceKind::SourceFidbJava.dir(root))
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
            utils_root,
            signatures_base,
            fid_dir,
            fidb_java_dir,
            gdt_dir,
            die_dir,
            patterns_dir,
            workspace_root,
        }
    }

    /// Whether `dir` holds `filename`, counting the packed form as present.
    ///
    /// `.fidbf` moved to `utils/source/fid` and is not shipped; what ships is
    /// the packed pair `<stem>.fn.fpk` / `<stem>.lib.fpk` that
    /// `LazyFidDatabase::open` derives from the `.fidbf` path it is handed. So
    /// resolution has to answer "is this database available" rather than "is
    /// this file here" -- checking only the `.fidbf` turns FID matching off
    /// entirely in every tree without the packer inputs, which is CI, the
    /// container image, and any clone.
    fn fid_database_present(dir: &Path, filename: &str) -> bool {
        if dir.join(filename).exists() {
            return true;
        }
        Path::new(filename)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .is_some_and(|stem| dir.join(format!("{stem}.fn.fpk")).exists())
    }

    /// A path under `subdir` naming `filename` whose `.fpk` sibling exists.
    fn signature_table_path(&self, filename: &str, subdir: &str) -> Option<PathBuf> {
        let packed = Path::new(filename).with_extension("fpk");
        let mut roots: Vec<PathBuf> = Vec::new();
        if let Some(ref base) = self.signatures_base {
            roots.push(base.clone());
        }
        if let Some(ref root) = self.workspace_root {
            roots.push(root.join("utils").join("signatures"));
        }
        for base in roots {
            let mut dir = base;
            for part in subdir.split('/') {
                dir = dir.join(part);
            }
            if dir.join(&packed).exists() {
                return Some(dir.join(filename));
            }
        }
        None
    }
    /// Where `win_api_signatures.txt` would be.
    ///
    /// The text itself moved to `utils/source/typeinfo` and is not shipped; the
    /// caller opens `path.with_extension("fpk")` first, so what matters is that
    /// this names the directory the packed table lives in. It returns a path
    /// whose `.fpk` sibling exists before falling back to a real `.txt`.
    pub fn get_win_api_signatures_path(&self) -> Option<PathBuf> {
        let filename = "win_api_signatures.txt";
        if let Some(path) = self.signature_table_path(filename, "typeinfo/win32") {
            return Some(path);
        }
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
        if let Some(path) = self.signature_table_path("ntoskrnl_signatures.txt", "typeinfo/win32") {
            return Some(path);
        }
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

    /// `wdk_signatures.txt` (pipe-separated Windows Driver Kit kernel-mode API
    /// signatures, extracted from angr's vendored WDK prototype JSON via
    /// `scripts/angr_wdk_extract_signatures.py`), if present.
    pub fn get_wdk_signatures_path(&self) -> Option<PathBuf> {
        if let Some(path) = self.signature_table_path("wdk_signatures.txt", "typeinfo/win32") {
            return Some(path);
        }
        let filename = "wdk_signatures.txt";
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
        if let Some(path) =
            self.signature_table_path("generic_clib_signatures.txt", "typeinfo/generic")
        {
            return Some(path);
        }
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
        if let Some(path) =
            self.signature_table_path("generic_clib_64_signatures.txt", "typeinfo/generic")
        {
            return Some(path);
        }
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
        if let Some(path) = self.signature_table_path("mac_osx_signatures.txt", "typeinfo/mac_10.9")
        {
            return Some(path);
        }
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

    /// JSON file under the DLL-ordinal-export-name corpus (e.g.
    /// `x86_ordinals.json`, extracted from RetDec's vendored ordinal tables
    /// via `scripts/retdec_ordinals_extract.py`).
    pub fn get_ordinals_json_path(&self, filename: &str) -> Option<PathBuf> {
        if let Some(ref base) = self.signatures_base {
            let path = base.join("ordinals").join(filename);
            if path.exists() {
                return Some(path);
            }
        }
        self.workspace_root.as_ref().and_then(|root| {
            let path = root
                .join("utils")
                .join("signatures")
                .join("ordinals")
                .join(filename);
            path.exists().then_some(path)
        })
    }

    /// JSON file under the Rust typeinfo corpus (e.g. `rust_structures.json`,
    /// extracted from `rust-common.gdt` via `scripts/gdt_extract_structs.py`).
    pub fn get_rust_typeinfo_json_path(&self, filename: &str) -> Option<PathBuf> {
        self.get_typeinfo_json_path_in("rust", filename)
    }

    /// JSON file under the generic (cross-platform C library) typeinfo
    /// corpus (e.g. `generic_clib_structures.json`).
    pub fn get_generic_typeinfo_json_path(&self, filename: &str) -> Option<PathBuf> {
        self.get_typeinfo_json_path_in("generic", filename)
    }

    /// JSON file under the mac_10.9 typeinfo corpus (e.g.
    /// `mac_osx_structures.json`).
    pub fn get_mac_typeinfo_json_path(&self, filename: &str) -> Option<PathBuf> {
        self.get_typeinfo_json_path_in("mac_10.9", filename)
    }

    fn get_typeinfo_json_path_in(&self, subdir: &str, filename: &str) -> Option<PathBuf> {
        self.signatures_base
            .as_ref()
            .and_then(|base| {
                let path = base.join("typeinfo").join(subdir).join(filename);
                path.exists().then_some(path)
            })
            .or_else(|| {
                self.workspace_root.as_ref().and_then(|root| {
                    let path = root
                        .join("utils")
                        .join("signatures")
                        .join("typeinfo")
                        .join(subdir)
                        .join(filename);
                    path.exists().then_some(path)
                })
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

    /// `find_file_in_dirs` for FID databases, counting the packed form present.
    fn find_fid_in_dirs(dirs: &[&str], filename: &str) -> Option<PathBuf> {
        if let Some(path) = crate::core::utils::find_file_in_dirs(dirs, filename) {
            return Some(path);
        }
        let stem = Path::new(filename).file_stem()?.to_str()?;
        let packed = format!("{stem}.fn.fpk");
        let found = crate::core::utils::find_file_in_dirs(dirs, &packed)?;
        Some(found.with_file_name(filename))
    }

    // ========================================================================
    // FID Database Resolution
    // ========================================================================

    /// Get FID database path for a specific compiler/architecture
    pub fn get_fid_path(&self, is_64bit: bool, compiler_id: Option<&str>) -> Option<PathBuf> {
        let filename = Self::get_fid_filename(is_64bit, compiler_id);

        // Try FID directory first
        if let Some(ref fid_dir) = self.fid_dir {
            if Self::fid_database_present(fid_dir, &filename) {
                return Some(fid_dir.join(&filename));
            }
        }

        // Fallback to search paths
        Self::find_fid_in_dirs(FID_SEARCH_DIRS, &filename)
    }

    /// Get all available FID database paths for an architecture
    pub fn get_all_fid_paths(&self, is_64bit: bool) -> Vec<PathBuf> {
        let file_lists: Vec<&[&str]> = if is_64bit {
            vec![MSVC_FID_FILES_X64, GCC_FID_FILES_X64, LIBC_FID_FILES_X64]
        } else {
            vec![MSVC_FID_FILES_X86, GCC_FID_FILES_X86, LIBC_FID_FILES_X86]
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

    /// FID database paths for a specific known statically-linked library, if
    /// Fission bundles one (`utils/signatures/fid/`'s `sigmoid-openssl-*`,
    /// `SDL-el-*`, `qt5-el7-*`, `libsodium-*` files). `library_name` is
    /// matched case-insensitively by substring against DIE's
    /// `DetectionType::Library` detection names (e.g. "OpenSSL", "SDL",
    /// "Qt") -- returns an empty `Vec` for any library Fission doesn't have
    /// a dedicated FID database for.
    pub fn get_library_fid_paths(&self, is_64bit: bool, library_name: &str) -> Vec<PathBuf> {
        let name = library_name.to_ascii_lowercase();
        let files: &[&str] = if name.contains("openssl") {
            if is_64bit {
                OPENSSL_FID_FILES_X64
            } else {
                OPENSSL_FID_FILES_X86
            }
        } else if name.contains("sdl") {
            if is_64bit {
                SDL_FID_FILES_X64
            } else {
                SDL_FID_FILES_X86
            }
        } else if name.contains("qt") {
            if is_64bit {
                QT5_FID_FILES_X64
            } else {
                QT5_FID_FILES_X86
            }
        } else if name.contains("sodium") {
            if is_64bit {
                LIBSODIUM_FID_FILES_X64
            } else {
                LIBSODIUM_FID_FILES_X86
            }
        } else {
            return Vec::new();
        };
        files.iter().filter_map(|f| self.find_fid_file(f)).collect()
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
        processor: Option<&str>,
    ) -> Vec<PathBuf> {
        let compiler = compiler_id.unwrap_or_default().to_ascii_lowercase();
        let is_pe = format
            .map(|value| value.to_ascii_uppercase().starts_with("PE"))
            .unwrap_or(false);

        // Non-x86 processors (ARM/AArch64/MIPS/...) never reach the x86-
        // centric branches below, since those only ever pick `.first()` off
        // arrays that put an x86 file first -- check the actual CPU first.
        if let Some(processor) = processor
            && let Some((gcc, libc)) = non_x86_fid_files(processor, is_64bit)
        {
            return [gcc, libc]
                .into_iter()
                .filter_map(|name| self.find_fid_file(name))
                .collect();
        }

        if compiler.contains("gcc") || compiler.contains("mingw") {
            let (gcc, libc): (Option<&str>, Option<&str>) = if is_64bit {
                (
                    GCC_FID_FILES_X64.first().copied(),
                    LIBC_FID_FILES_X64.first().copied(),
                )
            } else {
                (
                    GCC_FID_FILES_X86.first().copied(),
                    LIBC_FID_FILES_X86.first().copied(),
                )
            };
            // The glibc FID database only matches statically-linked
            // *glibc* code; MinGW targets link a different C runtime
            // (msvcrt/ucrt), so only try it for the non-MinGW (typically
            // ELF/Linux) gcc case.
            let names: Vec<&str> = if compiler.contains("mingw") {
                gcc.into_iter().collect()
            } else {
                gcc.into_iter().chain(libc).collect()
            };
            return names
                .into_iter()
                .filter_map(|name| self.find_fid_file(name))
                .collect();
        }

        if compiler.contains("clang") && !is_pe {
            let primary = if is_64bit {
                GCC_FID_FILES_X64
                    .first()
                    .copied()
                    .into_iter()
                    .chain(LIBC_FID_FILES_X64.first().copied())
            } else {
                GCC_FID_FILES_X86
                    .first()
                    .copied()
                    .into_iter()
                    .chain(LIBC_FID_FILES_X86.first().copied())
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
            if Self::fid_database_present(fid_dir, filename) {
                return Some(fid_dir.join(filename));
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
        if let Some(p) = Self::find_fid_in_dirs(FID_SEARCH_DIRS, filename) {
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
            // Pick the latest golang GDT available. The two hard-coded names
            // this used to try, 1.25 and 1.24, are the two the archive does not
            // ship -- it carries 1.15 through 1.23 -- so a Go binary got no GDT
            // at all. Ask what is there instead of naming versions.
            if let Some(p) = self.latest_golang_gdt() {
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

    /// Highest-versioned `golang_1.<minor>_anybit_any.gdt` present, if any.
    ///
    /// Versions are compared numerically on the minor component: sorting the
    /// names as text would rank `golang_1.9` above `golang_1.23`.
    fn latest_golang_gdt(&self) -> Option<PathBuf> {
        let mut best: Option<(u32, PathBuf)> = None;
        for dir in self
            .signatures_base
            .as_ref()
            .map(|base| base.join("typeinfo").join("golang"))
            .into_iter()
            .chain(self.workspace_root.as_ref().map(|root| {
                root.join("utils")
                    .join("signatures")
                    .join("typeinfo")
                    .join("golang")
            }))
        {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let name = entry.file_name();
                let Some(name) = name.to_str() else { continue };
                let Some(minor) = name
                    .strip_prefix("golang_1.")
                    .and_then(|rest| rest.strip_suffix("_anybit_any.gdt"))
                    .and_then(|minor| minor.parse::<u32>().ok())
                else {
                    continue;
                };
                if best.as_ref().is_none_or(|(seen, _)| minor > *seen) {
                    best = Some((minor, entry.path()));
                }
            }
            if best.is_some() {
                return best.map(|(_, path)| path);
            }
        }
        best.map(|(_, path)| path)
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

    /// Get gate policy configuration path
    pub fn get_gate_policy_path(&self) -> Option<PathBuf> {
        self.workspace_root
            .as_ref()
            .map(|root| {
                root.join("benchmark")
                    .join("config")
                    .join("gate_policy.toml")
            })
            .filter(|p| p.exists())
    }

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
    fn library_fid_paths_resolve_for_known_libraries() {
        let config = PathConfig::detect();
        for (name, expect_hit) in [
            ("OpenSSL", true),
            ("SDL", true),
            ("Qt", true),
            ("libsodium", true),
            ("nonexistent_lib_xyz", false),
        ] {
            let found = config.get_library_fid_paths(true, name);
            if expect_hit {
                assert!(
                    !found.is_empty(),
                    "expected at least one FID path for {name}"
                );
            } else {
                assert!(found.is_empty(), "expected no FID path for {name}");
            }
        }
    }

    /// Regression test for the bin_000.elf finding: a statically-linked
    /// glibc ELF resolved under half the call sites Ghidra resolved for the
    /// identical function, because `libc-*.fidbf` (present on disk) was
    /// never referenced by any path-selection function -- only the
    /// compiler-ID `gcc-*.fidbf` database was tried. MinGW is excluded
    /// since it links a different C runtime (msvcrt/ucrt), not glibc.
    #[test]
    fn preferred_fid_paths_include_libc_for_gcc_but_not_mingw() {
        let config = PathConfig::detect();

        let gcc_paths = config.get_preferred_fid_paths(true, Some("ELF"), Some("gcc"), None);
        assert!(
            gcc_paths.iter().any(|p| p
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("libc-"))),
            "gcc/ELF should try the glibc FID database, got {gcc_paths:?}"
        );

        let mingw_paths = config.get_preferred_fid_paths(true, Some("PE"), Some("mingw"), None);
        assert!(
            mingw_paths.iter().all(|p| !p
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("libc-"))),
            "MinGW (msvcrt/ucrt, not glibc) should not pull in the glibc FID database, got {mingw_paths:?}"
        );
    }

    /// Regression test for a real sample-set finding: 58/224 DecBench
    /// sample-set binaries are 32-bit ARM (not x86), and
    /// `get_preferred_fid_paths` only ever read `.first()` off
    /// `GCC_FID_FILES_X64`/`LIBC_FID_FILES_X64` -- both of which *contain*
    /// an ARM/AArch64 entry, just unreachably second in the array -- so
    /// every non-x86 binary got the x86 FID database (or none at all,
    /// since `ingest_signature_matches`'s `is_x86` gate skipped FID
    /// matching for them entirely before this fix).
    #[test]
    fn preferred_fid_paths_select_arm_databases_for_arm_processor() {
        let config = PathConfig::detect();

        let arm32_paths =
            config.get_preferred_fid_paths(false, Some("ELF"), Some("gcc"), Some("ARM"));
        assert!(
            arm32_paths.iter().any(|p| p
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.contains("ARM") && !n.contains("AARCH64"))),
            "32-bit ARM should select the ARM (not x86) FID databases, got {arm32_paths:?}"
        );

        let aarch64_paths =
            config.get_preferred_fid_paths(true, Some("ELF"), Some("gcc"), Some("AArch64"));
        assert!(
            aarch64_paths.iter().any(|p| p
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.contains("AARCH64"))),
            "64-bit AArch64 should select the AARCH64 FID databases, got {aarch64_paths:?}"
        );
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
