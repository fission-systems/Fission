//! Entry Point Signatures Database
//!
//! Contains byte patterns to identify compilers and packers by their entry point code.

use super::{Confidence, DetectionType};

/// An entry point signature
pub struct EntryPointSignature {
    /// Name of detected item
    pub name: &'static str,
    /// Version if known
    pub version: Option<&'static str>,
    /// Detection type
    pub detection_type: DetectionType,
    /// Confidence level
    pub confidence: Confidence,
    /// Signature bytes
    pub bytes: &'static [u8],
    /// Mask (optional, 0xFF = must match, 0x00 = wildcard)
    pub mask: Option<&'static [u8]>,
}

/// Collection of entry point signatures
pub static ENTRY_POINT_SIGNATURES: &[EntryPointSignature] = &[
    // MSVC x86 Debug
    EntryPointSignature {
        name: "Microsoft Visual C++",
        version: Some("Debug"),
        detection_type: DetectionType::Compiler,
        confidence: Confidence::High,
        bytes: &[0x55, 0x8B, 0xEC], // push ebp; mov ebp, esp
        mask: None,
    },
    // MSVC x64
    EntryPointSignature {
        name: "Microsoft Visual C++",
        version: Some("x64"),
        detection_type: DetectionType::Compiler,
        confidence: Confidence::Medium,
        bytes: &[0x48, 0x83, 0xEC], // sub rsp, XX
        mask: None,
    },
    // UPX packed
    EntryPointSignature {
        name: "UPX",
        version: None,
        detection_type: DetectionType::Packer,
        confidence: Confidence::High,
        bytes: &[0x60, 0xBE], // pushad; mov esi, XX
        mask: None,
    },
    // UPX 3.x
    EntryPointSignature {
        name: "UPX",
        version: Some("3.x"),
        detection_type: DetectionType::Packer,
        confidence: Confidence::High,
        bytes: &[0x60, 0xE8, 0x00, 0x00, 0x00, 0x00], // pushad; call $+5
        mask: None,
    },
    // ASPack
    EntryPointSignature {
        name: "ASPack",
        version: None,
        detection_type: DetectionType::Packer,
        confidence: Confidence::High,
        bytes: &[0x60, 0xE8, 0x03, 0x00, 0x00, 0x00],
        mask: None,
    },
    // Borland Delphi
    EntryPointSignature {
        name: "Borland Delphi",
        version: None,
        detection_type: DetectionType::Compiler,
        confidence: Confidence::High,
        bytes: &[0x55, 0x8B, 0xEC, 0x83, 0xC4, 0xF0], // push ebp; mov ebp,esp; add esp,-10h
        mask: None,
    },
    // Borland C++
    EntryPointSignature {
        name: "Borland C++",
        version: None,
        detection_type: DetectionType::Compiler,
        confidence: Confidence::High,
        bytes: &[0xEB, 0x10, 0x66, 0x62, 0x3A, 0x43], // jmp XX; "fb:C"
        mask: None,
    },
    // MinGW/GCC
    EntryPointSignature {
        name: "MinGW/GCC",
        version: None,
        detection_type: DetectionType::Compiler,
        confidence: Confidence::Medium,
        bytes: &[0x55, 0x89, 0xE5], // push ebp; mov ebp, esp
        mask: None,
    },
    // PECompact
    EntryPointSignature {
        name: "PECompact",
        version: None,
        detection_type: DetectionType::Packer,
        confidence: Confidence::High,
        bytes: &[0xB8, 0x00, 0x00, 0x00, 0x00, 0x50, 0x64, 0xFF, 0x35],
        mask: Some(&[0xFF, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF]),
    },
    // Themida/WinLicense
    EntryPointSignature {
        name: "Themida",
        version: None,
        detection_type: DetectionType::Protector,
        confidence: Confidence::High,
        bytes: &[0xB8, 0x00, 0x00, 0x00, 0x00, 0x60, 0x0B, 0xC0],
        mask: Some(&[0xFF, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF]),
    },
    // VMProtect
    EntryPointSignature {
        name: "VMProtect",
        version: None,
        detection_type: DetectionType::Protector,
        confidence: Confidence::High,
        bytes: &[0x68, 0x00, 0x00, 0x00, 0x00, 0xE8],
        mask: Some(&[0xFF, 0x00, 0x00, 0x00, 0x00, 0xFF]),
    },
    // MPRESS
    EntryPointSignature {
        name: "MPRESS",
        version: None,
        detection_type: DetectionType::Packer,
        confidence: Confidence::High,
        bytes: &[0x60, 0xE8, 0x00, 0x00, 0x00, 0x00, 0x58, 0x05],
        mask: None,
    },
    // Inno Setup
    EntryPointSignature {
        name: "Inno Setup",
        version: None,
        detection_type: DetectionType::Installer,
        confidence: Confidence::Medium,
        bytes: &[0x55, 0x8B, 0xEC, 0x83, 0xC4, 0xB8],
        mask: None,
    },
    // NSIS
    EntryPointSignature {
        name: "NSIS",
        version: None,
        detection_type: DetectionType::Installer,
        confidence: Confidence::Medium,
        bytes: &[0x83, 0xEC, 0x20, 0x53, 0x55, 0x56],
        mask: None,
    },
    // AutoIt
    EntryPointSignature {
        name: "AutoIt",
        version: None,
        detection_type: DetectionType::Language,
        confidence: Confidence::High,
        bytes: &[0x55, 0x8B, 0xEC, 0x81, 0xEC],
        mask: None,
    },
];
