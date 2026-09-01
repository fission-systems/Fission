//! Change bytes in a binary and write the result somewhere else.
//!
//! The loader has had `patch_bytes_va` and `save_as` all along with nothing
//! calling them. What was missing is the part that makes patching safe to
//! offer: showing the change before it is written, refusing a target the file
//! does not actually back, and never touching the input.

use fission_loader::loader::{LoadedBinary, SectionInfo};
use std::io::{self, Write};
use std::path::Path;

/// What to change, and whether to write it anywhere.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatchRequest {
    /// Virtual address of the first byte to change.
    pub address: u64,
    /// Replacement bytes as hex, e.g. `31 c0 c3`.
    pub bytes: Option<String>,
    /// Replace this many bytes with the architecture's NOP instead.
    pub nop: Option<usize>,
    /// Where to write the result. Without one this only reports the change.
    pub output: Option<std::path::PathBuf>,
    /// Allow writing over a file that already exists.
    pub force: bool,
}

pub(super) fn run_patch(
    binary: &mut LoadedBinary,
    request: &PatchRequest,
    json: bool,
) -> io::Result<()> {
    let outcome = patch_binary(binary, request, json);
    // Under `--json` a caller parses stdout, and every refusal here is an
    // ordinary result -- an address in a .bss, an output that already exists.
    // Reporting those only on stderr leaves the parser with nothing to read,
    // which is how `hex` already handles the same situation.
    if let (true, Err(error)) = (json, &outcome) {
        let mut stdout = io::stdout().lock();
        writeln!(
            stdout,
            "{}",
            serde_json::json!({ "error": error.to_string() })
        )?;
    }
    outcome
}

fn patch_binary(binary: &mut LoadedBinary, request: &PatchRequest, json: bool) -> io::Result<()> {
    let mut stdout = io::stdout().lock();

    // How long the patch is, before there is a patch. `--nop N` takes N from
    // the command line and would otherwise size an allocation with it: asking
    // for forty billion NOPs spent four seconds and 2.9GB to reach a bounds
    // check it was always going to fail, and asking for `usize::MAX` of them
    // aborted on capacity overflow before reaching one at all. The length is
    // all the bounds check needs, so it runs first and the bytes are built
    // only once the section has room for them.
    let length = match (&request.bytes, request.nop) {
        (Some(spec), _) => parse_patch_bytes(spec)?.len(),
        (None, Some(count)) => count,
        (None, None) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "nothing to write: pass --bytes <HEX> or --nop <N>",
            ));
        }
    };
    if length == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the patch is empty",
        ));
    }
    let offset = file_offset_for(&binary.sections, request.address, length)?;

    let patch = match &request.bytes {
        Some(spec) => parse_patch_bytes(spec)?,
        None => nop_fill(binary.sleigh_language_id().unwrap_or_default(), length)?,
    };

    let before = binary
        .get_bytes_at_offset(offset as u64, patch.len())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("0x{:x} runs past the end of the file", request.address),
            )
        })?;

    let output = match &request.output {
        None => {
            report(&mut stdout, request, offset, &before, &patch, json, None)?;
            return Ok(());
        }
        Some(output) => output,
    };

    // Never the input. A patch is not reversible from the result, and the
    // original is the only copy of what the bytes used to be.
    if same_file(output, Path::new(&binary.path)) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--output is the input binary; write the patched copy somewhere else",
        ));
    }
    if output.exists() && !request.force {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{} exists; pass --force to overwrite", output.display()),
        ));
    }

    binary.patch_bytes(offset as u64, &patch).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "the patch runs past the end of the file",
        )
    })?;
    write_atomically(output, binary.data.as_slice())?;

    report(
        &mut stdout,
        request,
        offset,
        &before,
        &patch,
        json,
        Some(output),
    )?;
    Ok(())
}

/// The file offset that actually stores `address`, or an error saying why not.
///
/// `va_to_file_offset` maps any address inside a section's *virtual* extent,
/// and a `.bss` has virtual bytes but no file bytes -- its `file_offset` is
/// zero, so an address in it maps to the start of the file. Patching there
/// would quietly rewrite the PE header instead of failing.
fn file_offset_for(sections: &[SectionInfo], address: u64, length: usize) -> io::Result<usize> {
    let Some(section) = containing_section(sections, address) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("0x{address:x} is not inside any section"),
        ));
    };
    if section.file_size == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "0x{address:x} is in {}, which occupies no space in the file -- there are no bytes there to change",
                section.name
            ),
        ));
    }
    let delta = address - section.virtual_address;
    // Saturating, because `length` comes from `--nop N` and N is whatever was
    // typed: a plain add overflows and panics long before the comparison.
    if delta.saturating_add(length as u64) > stored_size(section) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "{} bytes at 0x{address:x} run past the end of {} in the file",
                length, section.name
            ),
        ));
    }
    Ok((section.file_offset + delta) as usize)
}

/// Write `data` to `path` by way of a neighbouring temporary file.
///
/// Writing to the path directly follows a hard link back to its inode, so an
/// `--output` that is a hard link to the input rewrites the input in place --
/// the one file this command must never touch. `canonicalize` cannot see it:
/// the two paths are genuinely different and both real. Renaming a fresh file
/// over the link breaks the link instead of following it, and as a bonus the
/// result is atomic: a patched binary is never half-written.
fn write_atomically(path: &Path, data: &[u8]) -> io::Result<()> {
    let directory = path.parent().filter(|p| !p.as_os_str().is_empty());
    let mut temporary = match directory {
        Some(directory) => directory.join(""),
        None => Path::new(".").to_path_buf(),
    };
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    temporary.push(format!(".{name}.fission-patch.{}", std::process::id()));

    let cleanup = |error: io::Error| {
        let _ = std::fs::remove_file(&temporary);
        error
    };
    std::fs::write(&temporary, data).map_err(|e| {
        cleanup(io::Error::other(format!(
            "failed to write {}: {e}",
            path.display()
        )))
    })?;
    std::fs::rename(&temporary, path).map_err(|e| {
        cleanup(io::Error::other(format!(
            "failed to write {}: {e}",
            path.display()
        )))
    })
}

/// Whether a section is loaded into memory at all.
///
/// An ELF's `.debug_*`, `.symtab`, and `.comment` are in the file but never
/// mapped, and every one of them reports a virtual address of zero -- so they
/// all claim the same low addresses, and a number that is not an address at
/// all lands in whichever happens to come first. They carry no permissions,
/// which is what actually distinguishes them.
fn is_mapped(section: &SectionInfo) -> bool {
    section.is_readable || section.is_writable || section.is_executable
}

/// How many of a section's bytes the file actually stores.
///
/// The file range is padded up to the file alignment, so it is usually larger
/// than the content; the virtual extent is the content. Neither alone is the
/// answer -- the smaller of the two is.
fn stored_size(section: &SectionInfo) -> u64 {
    match section.virtual_size {
        0 => section.file_size,
        virtual_size => virtual_size.min(section.file_size),
    }
}

/// The section `address` is mapped into.
///
/// Containment goes by the *virtual* extent, because that is what an address
/// means. A section's file range is usually larger -- padded up to the file
/// alignment -- and an address past the virtual size is in that padding: it
/// exists in the file but is never loaded, so writing there changes nothing
/// that runs. Bounding containment by the virtual size rejects it here rather
/// than reporting a patch that has no effect.
fn containing_section(sections: &[SectionInfo], address: u64) -> Option<&SectionInfo> {
    sections
        .iter()
        .filter(|section| is_mapped(section))
        .find(|section| {
            let extent = match section.virtual_size {
                0 => section.file_size,
                virtual_size => virtual_size,
            };
            section
                .virtual_address
                .checked_add(extent)
                .is_some_and(|end| (section.virtual_address..end).contains(&address))
        })
}

fn report(
    stdout: &mut io::StdoutLock<'_>,
    request: &PatchRequest,
    offset: usize,
    before: &[u8],
    after: &[u8],
    json: bool,
    written: Option<&Path>,
) -> io::Result<()> {
    let hex = |bytes: &[u8]| {
        bytes
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    if json {
        writeln!(
            stdout,
            "{}",
            serde_json::json!({
                "address": format!("0x{:x}", request.address),
                "file_offset": format!("0x{offset:x}"),
                "before": hex(before),
                "after": hex(after),
                "written": written.map(|path| path.display().to_string()),
            })
        )?;
        return Ok(());
    }
    writeln!(
        stdout,
        "0x{:x} (file offset 0x{offset:x}), {} bytes:",
        request.address,
        after.len()
    )?;
    writeln!(stdout, "  before  {}", hex(before))?;
    writeln!(stdout, "  after   {}", hex(after))?;
    match written {
        Some(path) => writeln!(stdout, "written to {}", path.display())?,
        None => writeln!(
            stdout,
            "nothing written -- pass --output <FILE> to write the patched copy"
        )?,
    }
    Ok(())
}

/// Whether two paths name the same file, falling back to a textual compare.
fn same_file(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        // An output that does not exist yet cannot be canonicalised, and is
        // therefore not the input, which does.
        _ => left == right,
    }
}

/// Parse `90 90` or `9090` into bytes. No wildcards: you cannot write one.
fn parse_patch_bytes(spec: &str) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    for token in spec.split_whitespace() {
        // Non-hex is rejected whole, by the token the operator actually typed.
        // Splitting first and reporting the fragment names nothing useful: a
        // four-byte character lands astride a `chunks(2)` boundary, and both
        // halves are invalid UTF-8, so `--bytes 🔥` complained about ``.
        if let Some(bad) = token.chars().find(|c| !c.is_ascii_hexdigit()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("`{token}` is not hex: `{bad}` is not a hex digit"),
            ));
        }
        if token.len() % 2 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("`{token}` has an odd number of hex digits"),
            ));
        }
        for pair in token.as_bytes().chunks(2) {
            let text = std::str::from_utf8(pair).unwrap_or_default();
            bytes.push(u8::from_str_radix(text, 16).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("`{text}` is not a hex byte"),
                )
            })?);
        }
    }
    if bytes.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the patch is empty",
        ));
    }
    Ok(bytes)
}

/// The instruction this architecture uses to do nothing, repeated.
///
/// Filling with `0x90` on AArch64 would be `0x90` as a byte, not a NOP -- the
/// point of the flag is to leave something that runs.
fn nop_fill(language: &str, count: usize) -> io::Result<Vec<u8>> {
    if language.starts_with("x86:") {
        return Ok(vec![0x90; count]);
    }
    if language.starts_with("AARCH64:") {
        if count % 4 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("--nop {count} is not a multiple of 4; AArch64 instructions are 4 bytes"),
            ));
        }
        let nop: [u8; 4] = if language.contains(":BE:") {
            [0xd5, 0x03, 0x20, 0x1f]
        } else {
            [0x1f, 0x20, 0x03, 0xd5]
        };
        return Ok(nop.iter().copied().cycle().take(count).collect());
    }
    // 32-bit ARM is deliberately not here. Which NOP is correct depends on
    // whether the address is ARM or Thumb code, the language id does not say,
    // and the two are different widths -- guessing writes a four-byte NOP over
    // two Thumb instructions, or leaves half of one behind. Naming both
    // encodings lets the operator pick the one they know applies.
    if language.starts_with("ARM:") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "`{language}` does not say whether the address is ARM or Thumb code, and their \
                 NOPs are different widths; pass --bytes \"00 f0 20 e3\" for ARM or \
                 --bytes \"00 bf\" for Thumb (little-endian)"
            ),
        ));
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("no NOP encoding known for `{language}`; pass --bytes instead"),
    ))
}

#[cfg(test)]
mod tests {
    use super::{file_offset_for, nop_fill, parse_patch_bytes, same_file};
    use fission_loader::loader::SectionInfo;
    use std::path::Path;

    /// `.text` holds 119 bytes of content in a 512-byte file slot; `.bss` is
    /// 2928 bytes that the file does not store at all. Both shapes are taken
    /// from real fixtures in this repo.
    fn sections() -> Vec<SectionInfo> {
        vec![
            SectionInfo {
                name: ".text".into(),
                virtual_address: 0x140001000,
                virtual_size: 119,
                file_offset: 0x400,
                file_size: 512,
                is_executable: true,
                is_readable: true,
                is_writable: false,
            },
            SectionInfo {
                name: ".bss".into(),
                virtual_address: 0x14000d000,
                virtual_size: 2928,
                file_offset: 0,
                file_size: 0,
                is_executable: false,
                is_readable: true,
                is_writable: true,
            },
        ]
    }

    #[test]
    fn a_bss_address_does_not_resolve_to_the_start_of_the_file() {
        // `va_to_file_offset` answers `0 + 0x10` here, which is inside the PE
        // header -- so patching a .bss address through the loader's own API
        // rewrites the header instead of failing. That is the whole reason
        // this command resolves offsets itself.
        let error = file_offset_for(&sections(), 0x14000d010, 4)
            .expect_err("a section the file does not store has nothing to patch");
        let message = error.to_string();
        assert!(message.contains(".bss"), "{message}");
        assert!(!message.contains("0x10"), "{message}");
    }

    #[test]
    fn an_address_resolves_to_its_own_section_not_the_file_start() {
        assert_eq!(
            file_offset_for(&sections(), 0x140001010, 4).expect("mapped"),
            0x410
        );
    }

    #[test]
    fn a_patch_stops_at_the_end_of_the_content_not_the_file_slot() {
        // 0x140001074 + 4 reaches 120, one past .text's 119 bytes of content.
        // The file slot is 512 bytes, so a bound taken from `file_size` alone
        // would accept this and write into alignment padding that never loads.
        assert!(file_offset_for(&sections(), 0x140001074, 4).is_err());
        assert!(file_offset_for(&sections(), 0x140001073, 4).is_ok());
        // And an address wholly inside that padding is not an address at all.
        assert!(file_offset_for(&sections(), 0x140001100, 4).is_err());
    }

    #[test]
    fn an_unmapped_address_is_refused_rather_than_guessed_at() {
        assert!(file_offset_for(&sections(), 0x140009000, 1).is_err());
    }

    #[test]
    fn a_section_that_is_never_loaded_does_not_claim_low_addresses() {
        // An ELF's non-alloc sections all report a virtual address of zero,
        // so without the permission check a number like 0x50 -- not an
        // address at all -- resolves into whichever of them comes first and
        // reports a patchable byte inside the debug info.
        let mut sections = sections();
        sections.insert(
            0,
            SectionInfo {
                name: ".debug_loc".into(),
                virtual_address: 0,
                virtual_size: 127,
                file_offset: 0x5f8,
                file_size: 127,
                is_executable: false,
                is_readable: false,
                is_writable: false,
            },
        );
        assert!(file_offset_for(&sections, 0x50, 1).is_err());
    }

    #[test]
    fn a_nop_is_the_architecture_s_nop() {
        assert_eq!(
            nop_fill("x86:LE:64:default", 3).expect("x86"),
            vec![0x90; 3]
        );
        // Filling AArch64 with 0x90 would leave data, not an instruction.
        assert_eq!(
            nop_fill("AARCH64:LE:64:v8A", 8).expect("aarch64"),
            vec![0x1f, 0x20, 0x03, 0xd5, 0x1f, 0x20, 0x03, 0xd5]
        );
        assert_eq!(
            nop_fill("AARCH64:BE:64:v8A", 4).expect("aarch64 be"),
            vec![0xd5, 0x03, 0x20, 0x1f]
        );
        // A count that cannot be whole instructions would leave a fragment.
        assert!(nop_fill("AARCH64:LE:64:v8A", 6).is_err());
        // Better to say so than to invent an encoding.
        assert!(nop_fill("MIPS:BE:32:default", 4).is_err());
        // 32-bit ARM is refused on purpose: ARM and Thumb NOPs are different
        // widths and the language id does not say which the address holds.
        // The refusal has to name both, or it is just a dead end.
        let message = nop_fill("ARM:LE:32:v8", 4)
            .expect_err("ARM/Thumb is ambiguous")
            .to_string();
        assert!(message.contains("00 f0 20 e3"), "{message}");
        assert!(message.contains("00 bf"), "{message}");
    }

    #[test]
    fn a_huge_length_is_refused_rather_than_overflowing() {
        // `--nop N` takes N straight from the command line, and the bound is
        // checked on the length alone so that N never sizes an allocation:
        // forty billion NOPs used to spend 2.9GB reaching a check they were
        // always going to fail. `usize::MAX` reaches the check itself, where
        // a plain `delta + length` overflows and panics.
        assert!(file_offset_for(&sections(), 0x140001010, 40_000_000_000).is_err());
        // The address must be past the section start, or `delta` is zero and
        // `0 + usize::MAX` is simply `usize::MAX` -- no overflow, and the
        // guard this pins goes untested.
        assert!(file_offset_for(&sections(), 0x140001010, usize::MAX).is_err());
    }

    #[test]
    fn a_bad_byte_is_named_by_the_token_that_was_typed() {
        // A four-byte character straddles a `chunks(2)` boundary and leaves
        // two invalid-UTF-8 halves, so reporting the fragment named nothing:
        // `--bytes 🔥` used to complain about ``.
        let message = parse_patch_bytes("🔥")
            .expect_err("an emoji is not hex")
            .to_string();
        assert!(message.contains('🔥'), "{message}");
    }

    #[test]
    fn a_patch_is_hex_bytes_with_no_wildcards() {
        assert_eq!(
            parse_patch_bytes("90 90").expect("parses"),
            vec![0x90, 0x90]
        );
        assert_eq!(parse_patch_bytes("9090").expect("parses"), vec![0x90, 0x90]);
        // `search` accepts `??`; a patch cannot -- there is no byte to write.
        assert!(parse_patch_bytes("90 ??").is_err());
        assert!(parse_patch_bytes("9 0").is_err());
        assert!(parse_patch_bytes("  ").is_err());
        // `0x90` is one byte written the other way round, not two.
        assert!(parse_patch_bytes("0x90").is_err());
    }

    #[test]
    fn a_path_is_only_the_same_file_as_itself() {
        assert!(same_file(Path::new("/tmp"), Path::new("/tmp")));
        assert!(!same_file(Path::new("/tmp"), Path::new("/usr")));
    }
}
