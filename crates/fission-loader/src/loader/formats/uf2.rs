//! UF2: the USB flashing format of RP2040/RP2350, nRF52, SAMD, ESP32-S2/S3,
//! STM32 and other microcontroller boards.
//!
//! A UF2 file is a sequence of 512-byte blocks, each carrying up to 476 bytes
//! for one flash address. Like Intel HEX it describes where bytes go and not
//! what runs them: the optional family ID names a chip (or, for RP2350, a chip
//! and a security state), never a SLEIGH language, so it is reported and the
//! language stays the caller's. Layout per the UF2 specification
//! (`github.com/microsoft/uf2`, `README.md`), all fields little-endian.

use super::sparse::build_sparse_binary;
use crate::loader::pipeline::ExplicitLanguage;
use crate::loader::reader::SparseImage;
use crate::loader::types::{DataBuffer, LoadedBinary};
use crate::prelude::*;

pub const UF2_BLOCK_SIZE: usize = 512;
const UF2_MAGIC_START0: u32 = 0x0A32_4655; // "UF2\n"
const UF2_MAGIC_START1: u32 = 0x9E5D_5157;
const UF2_MAGIC_END: u32 = 0x0AB1_6F30;
/// Bytes 32..508 of a block; `payloadSize` may not exceed it.
const UF2_MAX_PAYLOAD: usize = 476;
const UF2_DATA_OFFSET: usize = 32;

/// The block is not for the main flash (a comment, a bootloader hint); the
/// specification says to skip it when writing the device, so it is no part
/// of the image.
const UF2_FLAG_NOT_MAIN_FLASH: u32 = 0x0000_0001;
/// The blocks carry files of a file container, and `targetAddr` is an offset
/// in a file, not an address.
const UF2_FLAG_FILE_CONTAINER: u32 = 0x0000_1000;
/// `fileSize` holds a family ID instead.
const UF2_FLAG_FAMILY_ID_PRESENT: u32 = 0x0000_2000;

/// Family IDs from the specification's `utils/uf2families.json`, limited to
/// the common boards. An ID not listed here is still reported, by number.
const KNOWN_FAMILIES: &[(u32, &str)] = &[
    (0x00ff_6919, "STM32L4"),
    (0x1b57_745f, "NRF52"),
    (0x1c5f_21b0, "ESP32"),
    (0x2abc_77ec, "LPC55"),
    (0x300f_5633, "STM32G0"),
    (0x4fb2_d5bd, "MIMXRT10XX"),
    (0x53b8_0f00, "STM32F7"),
    (0x5511_4460, "SAMD51"),
    (0x5775_5a57, "STM32F4"),
    (0x5ee2_1072, "STM32F1"),
    (0x621e_937a, "NRF52833"),
    (0x6478_24b6, "STM32F0"),
    (0x68ed_2b88, "SAMD21"),
    (0x6db6_6082, "STM32H7"),
    (0xada5_2840, "NRF52840"),
    (0xbfdd_4eee, "ESP32S2"),
    (0xc47e_5767, "ESP32S3"),
    (0xd42b_a06c, "ESP32C3"),
    (0xe48b_ff56, "RP2040"),
    (0xe48b_ff57, "RP2XXX_ABSOLUTE"),
    (0xe48b_ff58, "RP2XXX_DATA"),
    (0xe48b_ff59, "RP2350_ARM_S"),
    (0xe48b_ff5a, "RP2350_RISCV"),
    (0xe48b_ff5b, "RP2350_ARM_NS"),
];

pub struct Uf2Loader;

/// One decoded block's header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Uf2Block {
    flags: u32,
    target_address: u32,
    payload_size: usize,
    family_id: Option<u32>,
}

impl Uf2Loader {
    /// The first block carries both start magics and the end magic.
    pub fn looks_like(bytes: &[u8]) -> bool {
        bytes.get(..UF2_BLOCK_SIZE).is_some_and(|block| {
            u32_at(block, 0) == UF2_MAGIC_START0
                && u32_at(block, 4) == UF2_MAGIC_START1
                && u32_at(block, UF2_BLOCK_SIZE - 4) == UF2_MAGIC_END
        })
    }

    /// Without a language this validates every block and then fails with
    /// `LoadSpecNotFound`, naming the families the file is for.
    pub fn parse(data: DataBuffer, path: String) -> Result<LoadedBinary> {
        Self::parse_with_language(data, path, None)
    }

    pub fn parse_with_language(
        data: DataBuffer,
        path: String,
        language: Option<&ExplicitLanguage>,
    ) -> Result<LoadedBinary> {
        let bytes = data.as_slice();
        if bytes.is_empty() || !bytes.len().is_multiple_of(UF2_BLOCK_SIZE) {
            return Err(err!(
                loader,
                "MalformedHeader: UF2 is {} bytes, not a whole number of {UF2_BLOCK_SIZE}-byte blocks",
                bytes.len()
            ));
        }

        let mut image = SparseImage::new();
        let mut families: Vec<u32> = Vec::new();
        for (index, block) in bytes.chunks_exact(UF2_BLOCK_SIZE).enumerate() {
            let header = parse_block(block)
                .map_err(|reason| err!(loader, "MalformedHeader: UF2 block {index}: {reason}"))?;
            if header.flags & UF2_FLAG_FILE_CONTAINER != 0 {
                return Err(err!(
                    loader,
                    "UnsupportedFormat: UF2 block {index} belongs to a file container, \
                     whose blocks carry files rather than a flash image"
                ));
            }
            if header.flags & UF2_FLAG_NOT_MAIN_FLASH != 0 {
                continue;
            }
            if let Some(family) = header.family_id
                && !families.contains(&family)
            {
                families.push(family);
            }
            let payload = &block[UF2_DATA_OFFSET..UF2_DATA_OFFSET + header.payload_size];
            image.write(u64::from(header.target_address), payload)?;
        }
        families.sort_unstable();

        if image.is_empty() {
            return Err(err!(
                loader,
                "MalformedHeader: UF2 has no main-flash blocks with data"
            ));
        }
        let Some(language) = language else {
            return Err(err!(
                loader,
                "LoadSpecNotFound: UF2 ({}) names no instruction set; an explicit language is required",
                describe_families(&families)
            ));
        };
        // Blocks for two families may share flash addresses; the sparse
        // image refuses them only when they disagree about the bytes there.
        build_sparse_binary("UF2", path, &image, None, language).map_err(|err| {
            if families.len() > 1 {
                err!(
                    loader,
                    "{err} (blocks for {})",
                    describe_families(&families)
                )
            } else {
                err
            }
        })
    }
}

fn parse_block(block: &[u8]) -> std::result::Result<Uf2Block, String> {
    if u32_at(block, 0) != UF2_MAGIC_START0 || u32_at(block, 4) != UF2_MAGIC_START1 {
        return Err("bad start magic".to_string());
    }
    if u32_at(block, UF2_BLOCK_SIZE - 4) != UF2_MAGIC_END {
        return Err("bad end magic".to_string());
    }
    let flags = u32_at(block, 8);
    let target_address = u32_at(block, 12);
    let payload_size = u32_at(block, 16) as usize;
    let block_no = u32_at(block, 20);
    let num_blocks = u32_at(block, 24);
    let family_or_size = u32_at(block, 28);
    if payload_size > UF2_MAX_PAYLOAD {
        return Err(format!(
            "payload of {payload_size} bytes exceeds {UF2_MAX_PAYLOAD}"
        ));
    }
    if block_no >= num_blocks {
        return Err(format!(
            "block number {block_no} is not below its count {num_blocks}"
        ));
    }
    if u64::from(target_address) + payload_size as u64 > u64::from(u32::MAX) + 1 {
        return Err(format!(
            "0x{target_address:x} + {payload_size} runs past the 32-bit address space"
        ));
    }
    Ok(Uf2Block {
        flags,
        target_address,
        payload_size,
        family_id: (flags & UF2_FLAG_FAMILY_ID_PRESENT != 0).then_some(family_or_size),
    })
}

fn describe_families(families: &[u32]) -> String {
    if families.is_empty() {
        return "no family ID".to_string();
    }
    let names: Vec<String> = families
        .iter()
        .map(
            |&id| match KNOWN_FAMILIES.iter().find(|(known, _)| *known == id) {
                Some((_, name)) => format!("family 0x{id:08x} {name}"),
                None => format!("family 0x{id:08x}"),
            },
        )
        .collect();
    names.join(", ")
}

/// Callers pass a whole 512-byte block and offsets below 509.
fn u32_at(block: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        block[offset],
        block[offset + 1],
        block[offset + 2],
        block[offset + 3],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_core::architecture::ArchitectureDescriptor;

    fn thumb() -> ExplicitLanguage {
        ExplicitLanguage {
            language_id: "ARM:LE:32:Cortex".to_string(),
            architecture: ArchitectureDescriptor::new(
                "ARM",
                "little",
                32,
                "Cortex",
                None,
                "test language",
            ),
            source: "test-explicit-language".to_string(),
        }
    }

    /// One UF2 block, laid out field by field from the specification.
    fn block(
        flags: u32,
        target: u32,
        payload: &[u8],
        block_no: u32,
        num_blocks: u32,
        family: u32,
    ) -> Vec<u8> {
        let mut out = vec![0u8; UF2_BLOCK_SIZE];
        let fields = [
            UF2_MAGIC_START0,
            UF2_MAGIC_START1,
            flags,
            target,
            payload.len() as u32,
            block_no,
            num_blocks,
            family,
        ];
        for (index, value) in fields.iter().enumerate() {
            out[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
        out[UF2_DATA_OFFSET..UF2_DATA_OFFSET + payload.len()].copy_from_slice(payload);
        out[UF2_BLOCK_SIZE - 4..].copy_from_slice(&UF2_MAGIC_END.to_le_bytes());
        out
    }

    const RP2040: u32 = 0xe48b_ff56;
    /// adds r0, r0, r1; bx lr
    const THUMB_ADD: [u8; 4] = [0x40, 0x18, 0x70, 0x47];

    fn rp2040_image() -> Vec<u8> {
        let mut file = block(
            UF2_FLAG_FAMILY_ID_PRESENT,
            0x1000_0000,
            &THUMB_ADD,
            0,
            2,
            RP2040,
        );
        // A second block continuing the first: one run, not two sections.
        file.extend(block(
            UF2_FLAG_FAMILY_ID_PRESENT,
            0x1000_0004,
            &[0x00, 0xbf],
            1,
            2,
            RP2040,
        ));
        file
    }

    #[test]
    fn detects_by_all_three_magics() {
        let file = rp2040_image();
        assert!(Uf2Loader::looks_like(&file));
        let mut no_end = file.clone();
        no_end[UF2_BLOCK_SIZE - 1] ^= 0xff;
        assert!(!Uf2Loader::looks_like(&no_end));
        assert!(!Uf2Loader::looks_like(&file[..UF2_BLOCK_SIZE - 1]));
    }

    #[test]
    fn maps_blocks_under_an_explicit_language() {
        let binary = Uf2Loader::parse_with_language(
            DataBuffer::Heap(rp2040_image()),
            "fw.uf2".to_string(),
            Some(&thumb()),
        )
        .expect("loads");
        assert_eq!(binary.format, "UF2");
        assert_eq!(binary.image_base, 0x1000_0000);
        assert_eq!(binary.sleigh_language_id(), Some("ARM:LE:32:Cortex"));
        assert_eq!(binary.sections.len(), 1);
        assert_eq!(binary.sections[0].virtual_address, 0x1000_0000);
        assert_eq!(
            binary.data.as_slice(),
            &[0x40, 0x18, 0x70, 0x47, 0x00, 0xbf]
        );
        // UF2 has no entry point and the first bytes of a Cortex-M image are
        // its vector table, so nothing is seeded.
        assert!(binary.functions.is_empty());
    }

    #[test]
    fn without_a_language_names_the_family() {
        let err = Uf2Loader::parse(DataBuffer::Heap(rp2040_image()), "fw.uf2".to_string())
            .expect_err("no language");
        let message = format!("{err}");
        assert!(message.contains("LoadSpecNotFound"), "{message}");
        assert!(message.contains("0xe48bff56 RP2040"), "{message}");
    }

    #[test]
    fn not_main_flash_blocks_are_not_part_of_the_image() {
        let mut file = rp2040_image();
        file.extend(block(
            UF2_FLAG_NOT_MAIN_FLASH,
            0x2000_0000,
            b"comment",
            0,
            1,
            0,
        ));
        let binary = Uf2Loader::parse_with_language(
            DataBuffer::Heap(file),
            "fw.uf2".to_string(),
            Some(&thumb()),
        )
        .expect("loads");
        assert_eq!(binary.sections.len(), 1);
    }

    #[test]
    fn families_that_disagree_about_an_address_are_refused_by_name() {
        let mut file = rp2040_image();
        // An RP2350 RISC-V block over the same flash address.
        file.extend(block(
            UF2_FLAG_FAMILY_ID_PRESENT,
            0x1000_0000,
            &[0x13, 0x00, 0x00, 0x00],
            0,
            1,
            0xe48b_ff5a,
        ));
        let err = Uf2Loader::parse_with_language(
            DataBuffer::Heap(file),
            "fw.uf2".to_string(),
            Some(&thumb()),
        )
        .expect_err("conflicting families");
        let message = format!("{err}");
        assert!(message.contains("MalformedHeader"), "{message}");
        assert!(message.contains("RP2350_RISCV"), "{message}");
    }

    #[test]
    fn a_file_container_is_refused() {
        let file = block(UF2_FLAG_FILE_CONTAINER, 0, b"hello", 0, 1, 5);
        let err = Uf2Loader::parse_with_language(
            DataBuffer::Heap(file),
            "files.uf2".to_string(),
            Some(&thumb()),
        )
        .expect_err("file container");
        assert!(format!("{err}").contains("file container"), "{err}");
    }

    #[test]
    fn truncated_and_malformed_blocks_are_refused() {
        let file = rp2040_image();
        let cut = Uf2Loader::parse_with_language(
            DataBuffer::Heap(file[..UF2_BLOCK_SIZE + 100].to_vec()),
            "cut.uf2".to_string(),
            Some(&thumb()),
        )
        .expect_err("truncated");
        assert!(format!("{cut}").contains("whole number"), "{cut}");

        let mut oversized = rp2040_image();
        oversized[16..20].copy_from_slice(&477u32.to_le_bytes());
        let err = Uf2Loader::parse_with_language(
            DataBuffer::Heap(oversized),
            "big.uf2".to_string(),
            Some(&thumb()),
        )
        .expect_err("payload too large");
        assert!(format!("{err}").contains("block 0"), "{err}");

        let mut past_end = block(0, 0xffff_ff00, &[0u8; 257], 0, 1, 0);
        past_end[16..20].copy_from_slice(&257u32.to_le_bytes());
        let err = Uf2Loader::parse_with_language(
            DataBuffer::Heap(past_end),
            "wrap.uf2".to_string(),
            Some(&thumb()),
        )
        .expect_err("address wraps");
        assert!(format!("{err}").contains("32-bit address space"), "{err}");

        let mut bad_number = rp2040_image();
        bad_number[20..24].copy_from_slice(&9u32.to_le_bytes());
        let err = Uf2Loader::parse_with_language(
            DataBuffer::Heap(bad_number),
            "num.uf2".to_string(),
            Some(&thumb()),
        )
        .expect_err("block number past count");
        assert!(format!("{err}").contains("block number"), "{err}");
    }
}
