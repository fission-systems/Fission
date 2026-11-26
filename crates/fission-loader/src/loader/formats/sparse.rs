//! Mapping for formats that describe memory as address-tagged records and say
//! nothing about the processor: Intel HEX and Motorola S-record.
//!
//! Each maximal run of contiguous record bytes becomes one section. The
//! loaded data is those runs laid end to end, so a section's `file_offset`
//! indexes the decoded image, not the text file.

use crate::loader::pipeline::ExplicitLanguage;
use crate::loader::reader::SparseImage;
use crate::loader::types::{
    DataBuffer, FunctionInfo, LoadedBinary, LoadedBinaryBuilder, SectionInfo,
};
use crate::prelude::*;
use fission_core::architecture::BinaryLoadSpec;

/// Build a `LoadedBinary` from decoded records.
///
/// `entry` is the file's own start address (Intel HEX type 03/05, S-record
/// S7/S8/S9). A function is seeded there only when it lands on a decoded byte;
/// the low bit is ignored for that test, since Thumb start addresses carry it.
/// A file without one seeds nothing: the lowest address of a Cortex-M image is
/// its vector table, not code, so guessing would decode data.
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
        let target = address & !1;
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
        .entry_point(entry_run.map_or(image_base, |(address, _)| address))
        .add_sections(sections);
    if let Some((address, run_start)) = entry_run {
        builder = builder.add_function(FunctionInfo {
            name: "entry".to_string(),
            address,
            size: 0,
            is_export: false,
            is_import: false,
            origin: Some("start-address-record".to_string()),
            kind: Some("entry".to_string()),
            source_section: Some(format!("load_{run_start:08x}")),
            external_library: None,
            is_thunk_like: false,
            thunk_target: None,
        });
    }
    builder.build()
}
