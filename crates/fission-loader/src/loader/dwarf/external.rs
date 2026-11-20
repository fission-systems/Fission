//! Resolves an externally-split DWARF debug companion file, following the
//! standard GNU `.gnu_debuglink`/`.note.gnu.build-id` conventions Ghidra's
//! `DWARFExternalDebugFilesPlugin` also implements: a large fraction of
//! real-world binaries -- every Debian/Ubuntu `-dbgsym` package, every
//! Fedora/RHEL `debuginfo` package, and the local `objcopy
//! --only-keep-debug` + `--strip-debug` + `--add-gnu-debuglink` workflow --
//! ship their DWARF sections in a *separate* file from the one actually
//! being analyzed. Without this, `DwarfAnalyzer` never gets a chance to
//! run on any of them at all: `.debug_info` simply isn't in the file, so
//! `DwarfAnalyzer::has_debug_info` was always `false` and every downstream
//! type/function/line extraction silently produced nothing.

use super::super::types::LoadedBinary;
use std::path::{Path, PathBuf};

/// The bytes of the section named `name`, as they sit in the file.
fn section_file_bytes<'a>(binary: &'a LoadedBinary, name: &str) -> Option<&'a [u8]> {
    let section = binary.sections.iter().find(|s| s.name == name)?;
    let start = usize::try_from(section.file_offset).ok()?;
    let end = start.checked_add(usize::try_from(section.file_size).ok()?)?;
    binary.data.as_slice().get(start..end)
}

/// A `u32` at `at`, in the byte order of the file that carries it.
fn read_u32(data: &[u8], at: usize, little_endian: bool) -> Option<u32> {
    let bytes: [u8; 4] = data.get(at..at.checked_add(4)?)?.try_into().ok()?;
    Some(if little_endian {
        u32::from_le_bytes(bytes)
    } else {
        u32::from_be_bytes(bytes)
    })
}

/// Parses `.gnu_debuglink`: a NUL-terminated companion filename, padded
/// with zero bytes to the next 4-byte boundary, followed by a `u32` CRC32
/// (the IEEE 802.3 polynomial `crc32fast` implements) of the companion
/// file's *entire* contents -- used below to reject a same-named file that
/// doesn't actually match (e.g. a stale leftover from a previous build). The
/// CRC is stored in the byte order of the file itself.
fn parse_gnu_debuglink(data: &[u8], little_endian: bool) -> Option<(String, u32)> {
    let nul = data.iter().position(|&b| b == 0)?;
    let filename = String::from_utf8_lossy(&data[..nul]).to_string();
    let crc_offset = (nul + 1).div_ceil(4) * 4;
    Some((filename, read_u32(data, crc_offset, little_endian)?))
}

/// Parses `.note.gnu.build-id`: a standard ELF note (`namesz: u32`,
/// `descsz: u32`, `type: u32`, then `name` and `desc` each padded to a
/// 4-byte boundary) with `name == "GNU\0"` and `type == NT_GNU_BUILD_ID
/// (3)`; `desc` is the raw build-id bytes (a SHA-1 hash in practice, but
/// callers shouldn't assume a fixed length). The three header words are in
/// the byte order of the file: read as little-endian on a big-endian target,
/// `type` comes out as `0x0300_0000` and the note was never recognised.
fn parse_gnu_build_id(data: &[u8], little_endian: bool) -> Option<Vec<u8>> {
    const NT_GNU_BUILD_ID: u32 = 3;

    let namesz = usize::try_from(read_u32(data, 0, little_endian)?).ok()?;
    let descsz = usize::try_from(read_u32(data, 4, little_endian)?).ok()?;
    let note_type = read_u32(data, 8, little_endian)?;
    if note_type != NT_GNU_BUILD_ID {
        return None;
    }
    let name_start: usize = 12;
    let name = data.get(name_start..name_start.checked_add(namesz)?)?;
    if name != b"GNU\0" {
        return None;
    }
    let desc_start = name_start.checked_add(namesz.checked_next_multiple_of(4)?)?;
    let desc = data.get(desc_start..desc_start.checked_add(descsz)?)?;
    Some(desc.to_vec())
}

/// Loads and validates a candidate companion file: must exist, must (if
/// `expected_crc` is given -- always true for a `.gnu_debuglink` match,
/// never for a build-id path match since the build-id itself already
/// identifies the file) match the recorded CRC32, and must actually
/// contain `.debug_info` once loaded (guards against a same-named but
/// unrelated file, or a build-id directory hit that isn't really a debug
/// companion).
fn try_load_debug_companion(path: &Path, expected_crc: Option<u32>) -> Option<LoadedBinary> {
    if !path.is_file() {
        return None;
    }
    let file_bytes = std::fs::read(path).ok()?;
    if let Some(expected) = expected_crc {
        let actual = crc32fast::hash(&file_bytes);
        if actual != expected {
            tracing::debug!(
                "[DwarfExternal] {} CRC mismatch (expected {:#x}, got {:#x}), skipping",
                path.display(),
                expected,
                actual
            );
            return None;
        }
    }

    let loaded = LoadedBinary::auto_detect_and_parse_inner(
        super::super::types::DataBuffer::Heap(file_bytes),
        path.to_string_lossy().to_string(),
        false,
    )
    .ok()?;
    let has_debug_info = loaded
        .sections
        .iter()
        .any(|s| s.name == ".debug_info" || s.name == "__debug_info");
    has_debug_info.then_some(loaded)
}

/// Tries to find and load `binary`'s split-out DWARF companion, checking
/// (in order): the `.gnu_debuglink`-named file next to `own_path`, that
/// same filename under a `.debug/` subdirectory (both real, commonly-used
/// local conventions), then the system-wide `.note.gnu.build-id`
/// convention (`/usr/lib/debug/.build-id/xx/yyyy...debug`) distro packages
/// use. Returns `None` if `binary` has neither section, or nothing at any
/// candidate path resolves to a genuine debug companion.
pub(in crate::loader) fn resolve_external_debug_binary(
    binary: &LoadedBinary,
    own_path: &str,
) -> Option<LoadedBinary> {
    let own_dir = Path::new(own_path)
        .parent()
        .unwrap_or_else(|| Path::new("."));

    let little_endian = binary.is_little_endian();
    if let Some((filename, expected_crc)) = section_file_bytes(binary, ".gnu_debuglink")
        .and_then(|data| parse_gnu_debuglink(data, little_endian))
    {
        for candidate in [
            own_dir.join(&filename),
            own_dir.join(".debug").join(&filename),
        ] {
            if let Some(loaded) = try_load_debug_companion(&candidate, Some(expected_crc)) {
                tracing::info!(
                    "[DwarfExternal] resolved debug companion via .gnu_debuglink: {}",
                    candidate.display()
                );
                return Some(loaded);
            }
        }
    }

    if let Some(build_id) = section_file_bytes(binary, ".note.gnu.build-id")
        .and_then(|data| parse_gnu_build_id(data, little_endian))
    {
        if build_id.len() >= 2 {
            let hex: String = build_id.iter().map(|b| format!("{b:02x}")).collect();
            let (dir_part, file_part) = hex.split_at(2);
            let candidate = PathBuf::from("/usr/lib/debug/.build-id")
                .join(dir_part)
                .join(format!("{file_part}.debug"));
            if let Some(loaded) = try_load_debug_companion(&candidate, None) {
                tracing::info!(
                    "[DwarfExternal] resolved debug companion via build-id: {}",
                    candidate.display()
                );
                return Some(loaded);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::super::analyzer::DwarfAnalyzer;
    use super::{parse_gnu_build_id, parse_gnu_debuglink};
    use crate::loader::LoadedBinary;

    /// A `.note.gnu.build-id` section with the given build-id, its three
    /// header words in the requested byte order.
    fn build_id_note(id: &[u8], little_endian: bool) -> Vec<u8> {
        let word = |v: u32| {
            if little_endian {
                v.to_le_bytes()
            } else {
                v.to_be_bytes()
            }
        };
        let mut note = Vec::new();
        note.extend(word(4));
        note.extend(word(id.len() as u32));
        note.extend(word(3));
        note.extend(b"GNU\0");
        note.extend(id);
        note
    }

    #[test]
    fn build_id_note_is_read_in_the_byte_order_of_the_file() {
        let id = [0xde, 0xad, 0xbe, 0xef, 0x01, 0x02, 0x03, 0x04];
        assert_eq!(
            parse_gnu_build_id(&build_id_note(&id, true), true).as_deref(),
            Some(&id[..])
        );
        // Big-endian targets (PowerPC, MIPS BE, s390x, SPARC) were read as
        // little-endian and the note type never matched.
        assert_eq!(
            parse_gnu_build_id(&build_id_note(&id, false), false).as_deref(),
            Some(&id[..])
        );
        // And the wrong order is still a mismatch, not a lucky parse.
        assert_eq!(parse_gnu_build_id(&build_id_note(&id, false), true), None);
    }

    #[test]
    fn build_id_note_with_sizes_past_the_section_is_ignored() {
        let mut note = build_id_note(&[1, 2, 3, 4], true);
        note[4..8].copy_from_slice(&u32::MAX.to_le_bytes()); // descsz
        assert_eq!(parse_gnu_build_id(&note, true), None);
        let mut note = build_id_note(&[1, 2, 3, 4], true);
        note[0..4].copy_from_slice(&u32::MAX.to_le_bytes()); // namesz
        assert_eq!(parse_gnu_build_id(&note, true), None);
        assert_eq!(parse_gnu_build_id(&[], true), None);
    }

    #[test]
    fn debuglink_crc_is_read_in_the_byte_order_of_the_file() {
        let mut link = b"a.debug\0".to_vec();
        link.extend([0x12, 0x34, 0x56, 0x78]);
        assert_eq!(
            parse_gnu_debuglink(&link, false),
            Some(("a.debug".to_string(), 0x1234_5678))
        );
        assert_eq!(
            parse_gnu_debuglink(&link, true),
            Some(("a.debug".to_string(), 0x7856_3412))
        );
        assert_eq!(parse_gnu_debuglink(b"a.debug\0", true), None);
    }

    /// Real split-debug fixture, produced the standard way: `objcopy
    /// --only-keep-debug sample.elf sample.elf.debug`, `strip
    /// --strip-debug sample.elf`, `objcopy
    /// --add-gnu-debuglink=sample.elf.debug sample.elf` (matching every
    /// distro `-dbgsym`/`debuginfo` package's own build step). Loading the
    /// *stripped* `x64_dyn_split_debug_test.elf` alone must still recover
    /// full DWARF function info, resolved transparently from the sibling
    /// `x64_dyn_split_debug_test.elf.debug` this test never opens directly
    /// -- confirming `.gnu_debuglink` resolution, not just that the debug
    /// file happens to parse on its own.
    #[test]
    fn stripped_binary_recovers_dwarf_via_gnu_debuglink_companion() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("testdata/x64_dyn_split_debug_test.elf");
        let binary = LoadedBinary::from_file(&path).expect("load stripped split-debug ELF");

        // The stripped binary itself must genuinely have no .debug_info --
        // otherwise this test wouldn't be exercising external resolution
        // at all.
        assert!(
            !binary
                .sections
                .iter()
                .any(|s| s.name == ".debug_info" || s.name == "__debug_info"),
            "fixture should be stripped of its own debug sections"
        );
        assert!(
            binary.external_debug_binary.is_some(),
            "expected .gnu_debuglink resolution to find the sibling .debug file"
        );

        let analyzer = DwarfAnalyzer::new(&binary);
        assert!(analyzer.has_debug_info());

        let funcs = analyzer.analyze_functions();
        let compute = funcs
            .iter()
            .find(|f| f.name == "compute")
            .unwrap_or_else(|| panic!("no `compute` in {funcs:#x?}"));
        assert_eq!(compute.address, 0x400351);

        let main = funcs
            .iter()
            .find(|f| f.name == "main")
            .unwrap_or_else(|| panic!("no `main` in {funcs:#x?}"));
        assert_eq!(main.address, 0x400379);
    }

    /// The `.debug/<name>` subdirectory convention (the other real,
    /// commonly-used local split-debug layout, distinct from a sibling
    /// file in the same directory) must also resolve.
    #[test]
    fn debug_subdirectory_convention_resolves() {
        let testdata = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata");
        let dir = tempfile::tempdir().expect("tempdir");

        let exe_name = "x64_dyn_split_debug_test.elf";
        std::fs::copy(testdata.join(exe_name), dir.path().join(exe_name)).expect("copy exe");
        std::fs::create_dir(dir.path().join(".debug")).expect("mkdir .debug");
        std::fs::copy(
            testdata.join(format!("{exe_name}.debug")),
            dir.path().join(".debug").join(format!("{exe_name}.debug")),
        )
        .expect("copy companion into .debug/");

        let binary = LoadedBinary::from_file(dir.path().join(exe_name)).expect("load stripped ELF");
        assert!(binary.external_debug_binary.is_some());
        assert!(DwarfAnalyzer::new(&binary).has_debug_info());
    }

    /// A same-named file at the expected `.gnu_debuglink` path whose CRC32
    /// doesn't match the recorded one (a stale leftover from a previous
    /// build, the most realistic way this goes wrong in practice) must be
    /// rejected, not silently loaded and treated as this binary's real
    /// debug info.
    #[test]
    fn crc_mismatched_companion_is_rejected() {
        let testdata = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata");
        let dir = tempfile::tempdir().expect("tempdir");

        let exe_name = "x64_dyn_split_debug_test.elf";
        std::fs::copy(testdata.join(exe_name), dir.path().join(exe_name)).expect("copy exe");
        // Same expected filename, deliberately wrong content -> CRC won't match.
        std::fs::write(
            dir.path().join(format!("{exe_name}.debug")),
            b"not a real debug companion",
        )
        .expect("write bogus companion");

        let binary = LoadedBinary::from_file(dir.path().join(exe_name)).expect("load stripped ELF");
        assert!(
            binary.external_debug_binary.is_none(),
            "CRC-mismatched companion must not be accepted"
        );
        assert!(!DwarfAnalyzer::new(&binary).has_debug_info());
    }
}
