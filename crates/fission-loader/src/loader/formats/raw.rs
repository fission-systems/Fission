use crate::loader::types::{
    DataBuffer, FunctionInfo, LoadedBinary, LoadedBinaryBuilder, SectionInfo,
};
use crate::prelude::*;
use fission_core::architecture::{ArchitectureDescriptor, BinaryLoadSpec};

pub struct BinaryLoader;

#[derive(Clone, Debug)]
pub struct RawLoadHint {
    pub image_base: u64,
    pub entry_point: u64,
    pub architecture: ArchitectureDescriptor,
    pub load_spec: BinaryLoadSpec,
}

impl BinaryLoader {
    pub fn parse(data: DataBuffer, path: String) -> Result<LoadedBinary> {
        let _ = (data, path);
        Err(err!(
            loader,
            "LoadSpecNotFound: BinaryLoader requires an explicit raw load hint"
        ))
    }

    pub fn parse_with_hint(
        data: DataBuffer,
        path: String,
        hint: RawLoadHint,
    ) -> Result<LoadedBinary> {
        build_raw_binary(
            data,
            path,
            hint.image_base,
            hint.entry_point,
            hint.architecture,
            hint.load_spec,
        )
    }
}

fn build_raw_binary(
    data: DataBuffer,
    path: String,
    image_base: u64,
    entry_point: u64,
    architecture: ArchitectureDescriptor,
    load_spec: BinaryLoadSpec,
) -> Result<LoadedBinary> {
    let size = data.as_slice().len() as u64;
    if size == 0 {
        return Err(err!(loader, "MalformedHeader: empty raw binary"));
    }
    let function = FunctionInfo {
        name: "entry".to_string(),
        address: entry_point,
        size: 0,
        is_export: false,
        is_import: false,
        origin: Some("raw-entry".to_string()),
        kind: Some("entry".to_string()),
        source_section: Some("image".to_string()),
        external_library: None,
        is_thunk_like: false,
        thunk_target: None,
    };
    let section = SectionInfo {
        name: "image".to_string(),
        virtual_address: image_base,
        virtual_size: size,
        file_offset: 0,
        file_size: size,
        is_executable: true,
        is_readable: true,
        is_writable: false,
    };

    // `architecture` sets the word size from the hint's bitness. It used to be
    // overridden with `is_64bit(false)`, so an x86-64 or AArch64 raw image was
    // loaded as a 32-bit one.
    LoadedBinaryBuilder::new(path, data)
        .format("Raw Binary")
        .architecture(architecture)
        .load_spec(load_spec)
        .entry_point(entry_point)
        .image_base(image_base)
        .add_section(section)
        .add_function(function)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_binary_requires_explicit_hint_by_default() {
        let err = BinaryLoader::parse(DataBuffer::Heap(vec![0x90, 0xc3]), "raw.bin".to_string())
            .expect_err("raw without hint");
        assert!(format!("{err}").contains("LoadSpecNotFound"));
    }

    #[test]
    fn raw_binary_loads_with_explicit_hint() {
        let hint = RawLoadHint {
            image_base: 0x1000,
            entry_point: 0x1000,
            architecture: ArchitectureDescriptor::new(
                "x86",
                "little",
                32,
                "default",
                Some("raw".to_string()),
                "explicit raw test hint",
            ),
            load_spec: BinaryLoadSpec::new(
                "Raw Binary",
                0x1000,
                "x86:LE:32:default",
                "default",
                "explicit raw test hint",
            ),
        };
        let binary = BinaryLoader::parse_with_hint(
            DataBuffer::Heap(vec![0x90, 0xc3]),
            "raw.bin".to_string(),
            hint,
        )
        .expect("load raw");
        assert_eq!(binary.format, "Raw Binary");
        assert_eq!(binary.sections[0].virtual_address, 0x1000);
        assert_eq!(binary.entry_point, 0x1000);
    }

    fn hint(processor: &str, bitness: u8, language: &str) -> RawLoadHint {
        RawLoadHint {
            image_base: 0x1000,
            entry_point: 0x1000,
            architecture: ArchitectureDescriptor::new(
                processor,
                "little",
                bitness,
                "default",
                Some("raw".to_string()),
                "explicit raw test hint",
            ),
            load_spec: BinaryLoadSpec::new(
                "Raw Binary",
                0x1000,
                language,
                "default",
                "explicit raw test hint",
            ),
        }
    }

    fn load_with(hint: RawLoadHint) -> LoadedBinary {
        BinaryLoader::parse_with_hint(DataBuffer::Heap(vec![0x90, 0xc3]), "raw.bin".into(), hint)
            .expect("load raw")
    }

    /// The word size follows the hinted architecture. It was hardcoded to
    /// 32-bit, so a 64-bit raw image read its pointers four bytes wide.
    #[test]
    fn raw_binary_word_size_follows_the_hinted_architecture() {
        let wide = load_with(hint("x86", 64, "x86:LE:64:default"));
        assert!(wide.is_64bit);
        let arm = load_with(hint("AARCH64", 64, "AARCH64:LE:64:v8A"));
        assert!(arm.is_64bit);
        let narrow = load_with(hint("x86", 32, "x86:LE:32:default"));
        assert!(!narrow.is_64bit);
    }
}
