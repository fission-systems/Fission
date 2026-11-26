//! Mapping for formats that describe memory as address-tagged records and say
//! nothing about the processor: Intel HEX and Motorola S-record.
//!
//! Each maximal run of contiguous record bytes becomes one section. The
//! loaded data is those runs laid end to end, so a section's `file_offset`
//! indexes the decoded image, not the text file.

use crate::loader::pipeline::ExplicitLanguage;
use crate::loader::reader::SparseImage;
use crate::loader::types::START_ADDRESS_RECORD_ORIGIN;
use crate::loader::types::{
    DataBuffer, FunctionInfo, LoadedBinary, LoadedBinaryBuilder, SectionInfo,
};
use crate::prelude::*;
use fission_core::architecture::{ArchitectureDescriptor, BinaryLoadSpec};

/// Build a `LoadedBinary` from decoded records.
///
/// `entry` is the file's own start address (Intel HEX type 03/05, S-record
/// S7/S8/S9). It becomes the entry point, and seeds a function, only when it
/// lands on a decoded byte; on an ISA that encodes its code mode in the low
/// bit ([`has_low_bit_code_mode`]) that bit is ignored for the test. A file
/// without such an address has no entry point (`entry_point` is 0, and
/// [`LoadedBinary::declared_entry_point`] is `None`) and seeds nothing: the
/// lowest address of a Cortex-M image is its vector table, not code, so
/// guessing would decode data.
///
/// A run that ends beyond what the language can address is refused.
pub(crate) fn build_sparse_binary(
    format: &str,
    path: String,
    image: &SparseImage,
    entry: Option<u64>,
    language: &ExplicitLanguage,
) -> Result<LoadedBinary> {
    let runs = image.coalesced_runs()?;
    let Some(&(image_base, _)) = runs.first() else {
        return Err(err!(
            loader,
            "MalformedHeader: {format} contains no data records"
        ));
    };

    if let Some(limit) = address_space_limit(&language.architecture) {
        for (address, bytes) in &runs {
            // `SparseImage::write` rejected any range whose end overflows.
            let end = address + bytes.len() as u64;
            if end > limit {
                return Err(err!(
                    loader,
                    "SectionOutOfBounds: {format} data at 0x{address:x}..0x{end:x} ends past 0x{limit:x}, the address space of {}",
                    language.language_id
                ));
            }
        }
    }

    let total: usize = runs.iter().map(|(_, bytes)| bytes.len()).sum();
    let mut flat = Vec::with_capacity(total);
    let mut sections = Vec::with_capacity(runs.len());
    for (address, bytes) in &runs {
        let size = bytes.len() as u64;
        sections.push(SectionInfo {
            name: format!("load_{address:08x}"),
            virtual_address: *address,
            virtual_size: size,
            file_offset: flat.len() as u64,
            file_size: size,
            // The records carry no permissions. Code has to be fetchable, so
            // every run is executable, as for a raw image.
            is_executable: true,
            is_readable: true,
            is_writable: false,
        });
        flat.extend_from_slice(bytes);
    }

    // The run holding the start address, if any.
    let entry_run = entry.and_then(|address| {
        let target = if has_low_bit_code_mode(&language.architecture) {
            address & !1
        } else {
            address
        };
        runs.iter()
            .find(|(start, bytes)| target >= *start && target - *start < bytes.len() as u64)
            .map(|(start, _)| (address, *start))
    });

    let load_spec = BinaryLoadSpec::new(
        format,
        image_base,
        language.language_id.as_str(),
        "default",
        language.source.as_str(),
    );
    let mut builder = LoadedBinaryBuilder::new(path, DataBuffer::Heap(flat))
        .format(format)
        .architecture(language.architecture.clone())
        .load_spec(load_spec)
        .image_base(image_base)
        .entry_point(entry_run.map_or(0, |(address, _)| address))
        .add_sections(sections);
    if let Some((address, run_start)) = entry_run {
        builder = builder.add_function(FunctionInfo {
            name: "entry".to_string(),
            address,
            size: 0,
            is_export: false,
            is_import: false,
            origin: Some(START_ADDRESS_RECORD_ORIGIN.to_string()),
            kind: Some("entry".to_string()),
            source_section: Some(format!("load_{run_start:08x}")),
            external_library: None,
            is_thunk_like: false,
            thunk_target: None,
        });
    }
    builder.build()
}

/// Whether a code address of this processor may carry a mode bit in bit 0:
/// ARM/Thumb and MIPS/MIPS16/microMIPS.
fn has_low_bit_code_mode(architecture: &ArchitectureDescriptor) -> bool {
    matches!(architecture.processor.as_str(), "ARM" | "MIPS")
}

/// One past the highest byte address of the language's code space, or `None`
/// for a 64-bit space.
///
/// A language ID's size is not always that of its code space. Word-addressed
/// and wider-than-word parts are listed with the `define space ... default`
/// line of their spec (`utils/sleigh-specs/languages/`): bytes = 2^(8 * size)
/// * wordsize.
fn address_space_limit(architecture: &ArchitectureDescriptor) -> Option<u64> {
    let bits = u32::from(architecture.bitness);
    let byte_bits = match (architecture.processor.as_str(), bits) {
        // size=4: segmented 16-bit x86 addresses a 32-bit linear space.
        ("x86", 16) | ("CR16C", 16) => 32,
        // size=3
        ("CR16AB" | "M16C/60" | "M16C/80", 16) => 24,
        // size=2 wordsize=2
        ("PIC-12" | "PIC-16" | "PIC-17" | "CP1600", 16) => 17,
        // wordsize=2 over the ID's own width (avr8: 16 or 24; PIC-24 and
        // dsPIC: 24).
        ("avr8" | "PIC-24E" | "PIC-24F" | "PIC-24H", _) => bits + 1,
        ("dsPIC30F" | "dsPIC33C" | "dsPIC33E" | "dsPIC33F", _) => bits + 1,
        _ => bits,
    };
    1u64.checked_shl(byte_bits)
}
