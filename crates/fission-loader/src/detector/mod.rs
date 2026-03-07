//! Binary Detector Module - Detect packers, compilers, and languages (DiE-style)
//!
//! Analyzes PE/ELF binaries to identify:
//! - Packers (UPX, ASPack, etc.)
//! - Protectors (VMProtect, Themida, etc.)  
//! - Compilers (MSVC, GCC, Clang, etc.)
//! - Languages (C/C++, Delphi, Go, Rust, .NET, etc.)

pub mod die_engine;
mod signatures;

use crate::loader::LoadedBinary;

/// Helper function to search for byte pattern in data.
/// Uses sliding window search which is efficient for small patterns.
///
/// Performance: O(n*m) worst case, but typically much faster for short patterns.
#[inline]
fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    if needle.len() > haystack.len() {
        return false;
    }
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// Detection confidence level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
    /// Low confidence - single weak indicator
    Low,
    /// Medium confidence - multiple weak indicators or one strong
    Medium,
    /// High confidence - multiple strong indicators
    High,
}

impl std::fmt::Display for Confidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Confidence::Low => write!(f, "Low"),
            Confidence::Medium => write!(f, "Medium"),
            Confidence::High => write!(f, "High"),
        }
    }
}

/// Type of detection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectionType {
    Packer,
    Protector,
    Compiler,
    Linker,
    Language,
    Library,
    Installer,
    Sfx,
}

impl std::fmt::Display for DetectionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DetectionType::Packer => write!(f, "Packer"),
            DetectionType::Protector => write!(f, "Protector"),
            DetectionType::Compiler => write!(f, "Compiler"),
            DetectionType::Linker => write!(f, "Linker"),
            DetectionType::Language => write!(f, "Language"),
            DetectionType::Library => write!(f, "Library"),
            DetectionType::Installer => write!(f, "Installer"),
            DetectionType::Sfx => write!(f, "SFX"),
        }
    }
}

/// A single detection result
#[derive(Debug, Clone)]
pub struct Detection {
    /// Type of detection
    pub detection_type: DetectionType,
    /// Name of detected item (e.g., "UPX", "MSVC")
    pub name: String,
    /// Version if known (e.g., "3.96", "14.0")
    pub version: Option<String>,
    /// Confidence level
    pub confidence: Confidence,
    /// Additional details
    pub details: Option<String>,
}

impl Detection {
    pub fn new(
        detection_type: DetectionType,
        name: impl Into<String>,
        version: Option<String>,
        confidence: Confidence,
    ) -> Self {
        Self {
            detection_type,
            name: name.into(),
            version,
            confidence,
            details: None,
        }
    }

    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    /// Format as display string
    pub fn display(&self) -> String {
        let mut s = format!("{}: {}", self.detection_type, self.name);
        if let Some(ref ver) = self.version {
            s.push_str(&format!(" {}", ver));
        }
        s.push_str(&format!(" ({})", self.confidence));
        s
    }
}

/// Result of binary detection
#[derive(Debug, Clone, Default)]
pub struct DetectionResult {
    /// All detections found
    pub detections: Vec<Detection>,
}

impl DetectionResult {
    pub fn new() -> Self {
        Self {
            detections: Vec::new(),
        }
    }

    pub fn add(&mut self, detection: Detection) {
        self.detections.push(detection);
    }

    /// Get detections by type
    pub fn by_type(&self, dt: DetectionType) -> Vec<&Detection> {
        self.detections
            .iter()
            .filter(|d| d.detection_type == dt)
            .collect()
    }

    /// Check if packed
    pub fn is_packed(&self) -> bool {
        !self.by_type(DetectionType::Packer).is_empty()
    }

    /// Check if protected
    pub fn is_protected(&self) -> bool {
        !self.by_type(DetectionType::Protector).is_empty()
    }

    /// Get primary compiler
    pub fn compiler(&self) -> Option<&Detection> {
        self.by_type(DetectionType::Compiler)
            .into_iter()
            .max_by_key(|d| d.confidence)
    }

    /// Get primary language
    pub fn language(&self) -> Option<&Detection> {
        self.by_type(DetectionType::Language)
            .into_iter()
            .max_by_key(|d| d.confidence)
    }
}

/// Detect binary characteristics
pub fn detect(binary: &LoadedBinary) -> DetectionResult {
    let mut result = DetectionResult::new();

    // Run all detection methods
    detect_by_sections(binary, &mut result);
    detect_by_imports(binary, &mut result);
    detect_by_strings(binary, &mut result);
    detect_by_entry_point(binary, &mut result);
    detect_by_symbols(binary, &mut result);

    // Run DIE signature-based detection
    die_engine::detect_with_die(binary, &mut result);

    // Sort by confidence (highest first)
    result
        .detections
        .sort_by(|a, b| b.confidence.cmp(&a.confidence));

    result
}

/// Detect by section names
fn detect_by_sections(binary: &LoadedBinary, result: &mut DetectionResult) {
    let section_names: Vec<&str> = binary.sections.iter().map(|s| s.name.as_str()).collect();

    // UPX detection
    if section_names.iter().any(|s| s.starts_with("UPX")) {
        result.add(
            Detection::new(DetectionType::Packer, "UPX", None, Confidence::High)
                .with_details("UPX section names detected"),
        );
    }

    // ASPack detection
    if section_names
        .iter()
        .any(|s| *s == ".aspack" || *s == ".adata")
    {
        result.add(Detection::new(
            DetectionType::Packer,
            "ASPack",
            None,
            Confidence::High,
        ));
    }

    // PECompact detection
    if section_names.iter().any(|s| s.starts_with("PEC")) {
        result.add(Detection::new(
            DetectionType::Packer,
            "PECompact",
            None,
            Confidence::High,
        ));
    }

    // MPRESS detection
    if section_names
        .iter()
        .any(|s| *s == ".MPRESS1" || *s == ".MPRESS2")
    {
        result.add(Detection::new(
            DetectionType::Packer,
            "MPRESS",
            None,
            Confidence::High,
        ));
    }

    // VMProtect detection
    if section_names.iter().any(|s| s.starts_with(".vmp")) {
        result.add(Detection::new(
            DetectionType::Protector,
            "VMProtect",
            None,
            Confidence::High,
        ));
    }

    // Themida/WinLicense detection
    if section_names
        .iter()
        .any(|s| *s == ".themida" || *s == ".winlice")
    {
        result.add(Detection::new(
            DetectionType::Protector,
            "Themida/WinLicense",
            None,
            Confidence::High,
        ));
    }

    // Enigma detection
    if section_names.iter().any(|s| s.starts_with(".enigma")) {
        result.add(Detection::new(
            DetectionType::Protector,
            "Enigma Protector",
            None,
            Confidence::High,
        ));
    }

    // NSIS installer detection
    if section_names.contains(&".ndata") {
        result.add(Detection::new(
            DetectionType::Installer,
            "NSIS",
            None,
            Confidence::High,
        ));
    }
}

/// Detect by imports
fn detect_by_imports(binary: &LoadedBinary, result: &mut DetectionResult) {
    let imports: Vec<&str> = binary.iat_symbols.values().map(|s| s.as_str()).collect();

    // MSVC runtime detection
    let has_msvcrt = imports
        .iter()
        .any(|s| s.starts_with("MSVCR") || s.starts_with("VCRUNTIME") || s.starts_with("ucrt"));
    if has_msvcrt {
        result.add(Detection::new(
            DetectionType::Compiler,
            "Microsoft Visual C++",
            None,
            Confidence::Medium,
        ));
    }

    // Delphi/C++ Builder detection
    if imports
        .iter()
        .any(|s| s.contains("borlndmm") || s.contains("cc32"))
    {
        result.add(Detection::new(
            DetectionType::Compiler,
            "Borland/Embarcadero",
            None,
            Confidence::High,
        ));
        result.add(Detection::new(
            DetectionType::Language,
            "Delphi/C++ Builder",
            None,
            Confidence::High,
        ));
    }

    // MFC detection
    if imports.iter().any(|s| s.starts_with("MFC")) {
        result.add(
            Detection::new(DetectionType::Library, "MFC", None, Confidence::High)
                .with_details("Microsoft Foundation Classes"),
        );
    }

    // Qt detection
    if imports
        .iter()
        .any(|s| s.starts_with("Qt") || s.contains("QT_"))
    {
        result.add(Detection::new(
            DetectionType::Library,
            "Qt",
            None,
            Confidence::High,
        ));
    }

    // Python runtime detection (embedded Python - PyInstaller, py2exe, etc.)
    let python_dll = imports
        .iter()
        .find(|s| s.to_lowercase().contains("python") && s.to_lowercase().ends_with(".dll"));
    if python_dll.is_some() {
        result.add(
            Detection::new(
                DetectionType::Language,
                "Python (Embedded)",
                None,
                Confidence::High,
            )
            .with_details("Python runtime embedded (likely PyInstaller/py2exe)"),
        );
    }
}

/// Detect by strings in binary
///
/// Performance optimizations:
/// - Uses memmem-style byte pattern searching instead of string conversion
/// - Searches raw bytes directly, avoiding UTF-8 lossy conversion allocation
/// - Early exits once all patterns for a category are found
fn detect_by_strings(binary: &LoadedBinary, result: &mut DetectionResult) {
    // Search in configurable range for better detection
    let search_limit = (512 * 1024) // 512KB limit
        .min(binary.data.as_slice().len());
    let data = &binary.data.as_slice()[..search_limit];

    // Go detection
    let mut is_go =
        contains_bytes(data, b"Go build ID:") || contains_bytes(data, b"runtime.gopanic");

    // Stronger detection using pclntab magic
    if !is_go {
        for section in &binary.sections {
            if section.name == ".gopclntab"
                || section.name == "__gopclntab"
                || section.name == "gopclntab"
            {
                is_go = true;
                break;
            }
            // Check first 16 bytes of data sections for magic
            if !section.is_executable && section.file_size >= 4 {
                if let Some(sdata) = binary.get_bytes(section.virtual_address, 16) {
                    let magic = u32::from_le_bytes([sdata[0], sdata[1], sdata[2], sdata[3]]);
                    if matches!(magic, 0xfffffffb | 0xfffffffa | 0xfffffff0 | 0xfffffff1) {
                        is_go = true;
                        break;
                    }
                }
            }
        }
    }

    if is_go {
        result.add(Detection::new(
            DetectionType::Language,
            "Go",
            None,
            Confidence::High,
        ));
        result.add(Detection::new(
            DetectionType::Compiler,
            "Go Compiler",
            None,
            Confidence::High,
        ));
    }

    // Rust detection
    if contains_bytes(data, b"rust_panic")
        || contains_bytes(data, b"rustc/")
        || contains_bytes(data, b".rustup")
    {
        result.add(Detection::new(
            DetectionType::Language,
            "Rust",
            None,
            Confidence::High,
        ));
        result.add(Detection::new(
            DetectionType::Compiler,
            "rustc",
            None,
            Confidence::High,
        ));
    }

    // Python detection (PyInstaller, py2exe, cx_Freeze)
    let has_pyinstaller = contains_bytes(data, b"PYZ-00")
        || contains_bytes(data, b"_MEIPASS")
        || contains_bytes(data, b"PyInstaller")
        || contains_bytes(data, b"pyi-runtime")
        || contains_bytes(data, b"PYTHONHOME");

    if has_pyinstaller {
        result.add(
            Detection::new(DetectionType::Packer, "PyInstaller", None, Confidence::High)
                .with_details("Python application packaged with PyInstaller"),
        );
        result.add(Detection::new(
            DetectionType::Language,
            "Python",
            None,
            Confidence::High,
        ));
    }

    // py2exe detection
    if contains_bytes(data, b"py2exe") || contains_bytes(data, b"PYTHONSCRIPT") {
        result.add(Detection::new(
            DetectionType::Packer,
            "py2exe",
            None,
            Confidence::High,
        ));
        result.add(Detection::new(
            DetectionType::Language,
            "Python",
            None,
            Confidence::High,
        ));
    }

    // cx_Freeze detection
    if contains_bytes(data, b"cx_Freeze") {
        result.add(Detection::new(
            DetectionType::Packer,
            "cx_Freeze",
            None,
            Confidence::High,
        ));
        result.add(Detection::new(
            DetectionType::Language,
            "Python",
            None,
            Confidence::High,
        ));
    }

    // AutoIt detection
    if contains_bytes(data, b"AutoIt") || contains_bytes(data, b"AU3!") {
        result.add(Detection::new(
            DetectionType::Language,
            "AutoIt",
            None,
            Confidence::High,
        ));
    }

    // Nim detection
    if contains_bytes(data, b"nimbase.h") || contains_bytes(data, b"@Nim") {
        result.add(Detection::new(
            DetectionType::Language,
            "Nim",
            None,
            Confidence::High,
        ));
    }

    // GCC detection - need string conversion only if found
    if contains_bytes(data, b"GCC:") {
        // Only allocate string for version extraction if GCC pattern found
        let data_str = String::from_utf8_lossy(data);
        let version = extract_gcc_version(&data_str);
        result.add(Detection::new(
            DetectionType::Compiler,
            "GCC",
            version,
            Confidence::High,
        ));
    }

    // Clang detection
    if contains_bytes(data, b"clang version") {
        result.add(Detection::new(
            DetectionType::Compiler,
            "Clang",
            None,
            Confidence::High,
        ));
    }

    // MinGW detection (case-sensitive checks for both variants)
    if contains_bytes(data, b"mingw") || contains_bytes(data, b"MinGW") {
        result.add(Detection::new(
            DetectionType::Compiler,
            "MinGW",
            None,
            Confidence::Medium,
        ));
    }
}

/// Extract GCC version from string
fn extract_gcc_version(data: &str) -> Option<String> {
    if let Some(pos) = data.find("GCC:") {
        let rest = &data[pos..];
        // Look for version pattern like "GCC: (GNU) 10.2.0"
        if let Some(end) = rest.find('\0').or(Some(50.min(rest.len()))) {
            let snippet = &rest[..end];
            // Extract version number
            for word in snippet.split_whitespace() {
                if word.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                    return Some(word.to_string());
                }
            }
        }
    }
    None
}

/// Detect by entry point bytes
fn detect_by_entry_point(binary: &LoadedBinary, result: &mut DetectionResult) {
    // Get entry point bytes
    let ep_bytes = binary.get_bytes(binary.entry_point, 32);
    let Some(bytes) = ep_bytes else { return };

    // Check against known signatures
    for sig in signatures::ENTRY_POINT_SIGNATURES {
        if matches_signature(&bytes, sig.bytes, sig.mask) {
            result.add(Detection::new(
                sig.detection_type,
                sig.name,
                sig.version.map(|s| s.to_string()),
                sig.confidence,
            ));
        }
    }
}

/// Check if bytes match signature with mask
fn matches_signature(bytes: &[u8], sig: &[u8], mask: Option<&[u8]>) -> bool {
    if bytes.len() < sig.len() {
        return false;
    }

    for (i, &s) in sig.iter().enumerate() {
        let m = mask.map_or(0xFF, |m| m.get(i).copied().unwrap_or(0xFF));
        if (bytes[i] & m) != (s & m) {
            return false;
        }
    }
    true
}

/// Detect by symbols
fn detect_by_symbols(binary: &LoadedBinary, result: &mut DetectionResult) {
    let mut has_rust = false;
    let mut has_cpp = false;
    let mut has_swift = false;

    for func in &binary.functions {
        let name = &func.name;
        if name.starts_with("_R")
            || name.starts_with("_ZN")
            || name.starts_with("__R")
            || name.starts_with("__ZN")
        {
            has_rust = true;
        }
        if (name.starts_with("_Z") || name.starts_with("__Z")) && !has_rust {
            has_cpp = true;
        }
        if name.starts_with("_$s") || name.starts_with("_$S") || name.starts_with("__$s") {
            has_swift = true;
        }
    }

    if has_rust {
        result.add(Detection::new(
            DetectionType::Language,
            "Rust",
            None,
            Confidence::High,
        ));
    }
    if has_cpp {
        result.add(Detection::new(
            DetectionType::Language,
            "C++",
            None,
            Confidence::Medium,
        ));
    }
    if has_swift {
        result.add(Detection::new(
            DetectionType::Language,
            "Swift",
            None,
            Confidence::High,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_signature() {
        let bytes = [0x55, 0x8B, 0xEC, 0x83];
        let sig = [0x55, 0x8B, 0xEC];
        assert!(matches_signature(&bytes, &sig, None));

        // With mask
        let mask = [0xFF, 0xFF, 0x00]; // Ignore third byte
        let bytes2 = [0x55, 0x8B, 0xFF, 0x83];
        assert!(matches_signature(&bytes2, &sig, Some(&mask)));
    }
}
