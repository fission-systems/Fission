use crate::loader::reader::{ByteReader, SparseImage};
use crate::loader::types::{
    DataBuffer, FunctionInfo, LoadedBinary, LoadedBinaryBuilder, SectionInfo,
};
use crate::prelude::*;
use fission_core::architecture::{ArchitectureDescriptor, BinaryLoadSpec};

pub struct IntelHexLoader;
pub struct MotorolaHexLoader;

impl IntelHexLoader {
    pub fn looks_like(bytes: &[u8]) -> bool {
        first_ascii_line(bytes).is_some_and(|line| {
            line.starts_with(':')
                && line.len() >= 11
                && line[1..].chars().all(|c| c.is_ascii_hexdigit())
        })
    }

    pub fn parse(data: DataBuffer, path: String) -> Result<LoadedBinary> {
        let reader = ByteReader::little(data.as_slice());
        let mut image = SparseImage::new();
        let mut upper_linear = 0u64;
        let mut upper_segment = 0u64;
        let mut entry = None;

        for (line_no, raw_line) in reader.ascii_lines().enumerate() {
            let line = raw_line.trim();
            if line.is_empty() {
                continue;
            }
            if !line.starts_with(':') {
                return Err(err!(loader, "MalformedHeader: Intel HEX line missing ':'"));
            }
            let record = decode_hex_bytes(&line[1..]).map_err(|msg| {
                err!(
                    loader,
                    "MalformedHeader: Intel HEX line {}: {msg}",
                    line_no + 1
                )
            })?;
            if record.len() < 5 {
                return Err(err!(loader, "MalformedHeader: Intel HEX record too short"));
            }
            verify_checksum(&record).map_err(|msg| {
                err!(
                    loader,
                    "MalformedHeader: Intel HEX line {}: {msg}",
                    line_no + 1
                )
            })?;
            let count = record[0] as usize;
            if record.len() != count + 5 {
                return Err(err!(
                    loader,
                    "MalformedHeader: Intel HEX byte count does not match record length"
                ));
            }
            let address = u16::from_be_bytes([record[1], record[2]]) as u64;
            let record_type = record[3];
            let payload = &record[4..4 + count];
            match record_type {
                0x00 => {
                    let base = if upper_linear != 0 {
                        upper_linear
                    } else {
                        upper_segment
                    };
                    image.write(base + address, payload)?;
                }
                0x01 => break,
                0x02 if payload.len() == 2 => {
                    upper_segment = (u16::from_be_bytes([payload[0], payload[1]]) as u64) << 4;
                    upper_linear = 0;
                }
                0x03 if payload.len() == 4 => {
                    let cs = u16::from_be_bytes([payload[0], payload[1]]) as u64;
                    let ip = u16::from_be_bytes([payload[2], payload[3]]) as u64;
                    entry = Some((cs << 4) + ip);
                }
                0x04 if payload.len() == 2 => {
                    upper_linear = (u16::from_be_bytes([payload[0], payload[1]]) as u64) << 16;
                    upper_segment = 0;
                }
                0x05 if payload.len() == 4 => {
                    entry =
                        Some(
                            u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]])
                                as u64,
                        );
                }
                _ => {
                    return Err(err!(
                        loader,
                        "UnsupportedRelocationMetadata: unsupported Intel HEX record type 0x{record_type:02x}"
                    ));
                }
            }
        }

        build_hex_binary("Intel HEX", data, path, image, entry)
    }
}

impl MotorolaHexLoader {
    pub fn looks_like(bytes: &[u8]) -> bool {
        first_ascii_line(bytes).is_some_and(|line| {
            matches!(
                line.as_bytes(),
                [b'S', b'0'..=b'9', ..] | [b's', b'0'..=b'9', ..]
            ) && line.len() >= 4
        })
    }

    pub fn parse(data: DataBuffer, path: String) -> Result<LoadedBinary> {
        let reader = ByteReader::little(data.as_slice());
        let mut image = SparseImage::new();
        let mut entry = None;

        for (line_no, raw_line) in reader.ascii_lines().enumerate() {
            let line = raw_line.trim();
            if line.is_empty() {
                continue;
            }
            let bytes = line.as_bytes();
            if bytes.len() < 4 || !matches!(bytes[0], b'S' | b's') {
                return Err(err!(
                    loader,
                    "MalformedHeader: Motorola S-record line missing S-type"
                ));
            }
            let record_type = bytes[1].to_ascii_uppercase();
            let record = decode_hex_bytes(&line[2..]).map_err(|msg| {
                err!(
                    loader,
                    "MalformedHeader: Motorola S-record line {}: {msg}",
                    line_no + 1
                )
            })?;
            if record.len() < 2 {
                return Err(err!(loader, "MalformedHeader: Motorola S-record too short"));
            }
            let count = record[0] as usize;
            if record.len() != count + 1 {
                return Err(err!(
                    loader,
                    "MalformedHeader: Motorola S-record byte count does not match record length"
                ));
            }
            verify_ones_complement_checksum(&record).map_err(|msg| {
                err!(
                    loader,
                    "MalformedHeader: Motorola S-record line {}: {msg}",
                    line_no + 1
                )
            })?;

            let addr_len = match record_type {
                b'1' | b'9' => 2,
                b'2' | b'8' => 3,
                b'3' | b'7' => 4,
                b'0' | b'5' | b'6' => continue,
                _ => {
                    return Err(err!(
                        loader,
                        "UnsupportedRelocationMetadata: unsupported Motorola S-record type S{}",
                        record_type as char
                    ));
                }
            };
            if record.len() < 1 + addr_len + 1 {
                return Err(err!(
                    loader,
                    "MalformedHeader: Motorola S-record address truncated"
                ));
            }
            let address = record[1..1 + addr_len]
                .iter()
                .fold(0u64, |acc, byte| (acc << 8) | *byte as u64);
            let payload_end = record.len() - 1;
            let payload = &record[1 + addr_len..payload_end];
            match record_type {
                b'1' | b'2' | b'3' => image.write(address, payload)?,
                b'7' | b'8' | b'9' => entry = Some(address),
                _ => {}
            }
        }

        build_hex_binary("Motorola S-record", data, path, image, entry)
    }
}

fn build_hex_binary(
    format: &str,
    _original_data: DataBuffer,
    path: String,
    image: SparseImage,
    entry: Option<u64>,
) -> Result<LoadedBinary> {
    if image.is_empty() {
        return Err(err!(
            loader,
            "MalformedHeader: {format} contains no data records"
        ));
    }
    let (image_base, materialized) = image.materialize(0xff)?;
    let data = DataBuffer::Heap(materialized);
    let size = data.as_slice().len() as u64;
    let entry_point = entry.unwrap_or(image_base);
    let (architecture, load_spec) = generic_unknown_load_spec(format, image_base);
    let function = FunctionInfo {
        name: "entry".to_string(),
        address: entry_point,
        size: 0,
        is_export: false,
        is_import: false,
        origin: Some(format!("{format}-entry")),
        kind: Some("entry".to_string()),
        source_section: Some("image".to_string()),
        external_library: None,
        is_thunk_like: false,
    };
    let section = SectionInfo {
        name: "image".to_string(),
        virtual_address: image_base,
        virtual_size: size,
        file_offset: 0,
        file_size: size,
        is_executable: true,
        is_readable: true,
        is_writable: true,
    };

    LoadedBinaryBuilder::new(path, data)
        .format(format)
        .architecture(architecture)
        .load_spec(load_spec)
        .entry_point(entry_point)
        .image_base(image_base)
        .is_64bit(false)
        .add_section(section)
        .add_function(function)
        .build()
}

pub(crate) fn generic_unknown_load_spec(
    format: &str,
    image_base: u64,
) -> (ArchitectureDescriptor, BinaryLoadSpec) {
    let architecture = ArchitectureDescriptor::new(
        "unknown",
        "little",
        32,
        "default",
        Some("unknown".to_string()),
        format!("{format} generic binary image"),
    );
    let load_spec = BinaryLoadSpec::new(
        format,
        image_base,
        "unknown:LE:32:default",
        "default",
        format!("{format} generic binary image"),
    );
    (architecture, load_spec)
}

fn first_ascii_line(bytes: &[u8]) -> Option<&str> {
    let line_end = bytes
        .iter()
        .position(|&b| b == b'\n' || b == b'\r')
        .unwrap_or(bytes.len());
    std::str::from_utf8(&bytes[..line_end]).ok()
}

fn decode_hex_bytes(text: &str) -> std::result::Result<Vec<u8>, &'static str> {
    if !text.len().is_multiple_of(2) {
        return Err("odd number of hex digits");
    }
    let mut out = Vec::with_capacity(text.len() / 2);
    let bytes = text.as_bytes();
    for pair in bytes.chunks_exact(2) {
        let hi = hex_nibble(pair[0]).ok_or("non-hex digit")?;
        let lo = hex_nibble(pair[1]).ok_or("non-hex digit")?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn verify_checksum(record: &[u8]) -> std::result::Result<(), &'static str> {
    if record.iter().fold(0u8, |acc, byte| acc.wrapping_add(*byte)) == 0 {
        Ok(())
    } else {
        Err("checksum mismatch")
    }
}

fn verify_ones_complement_checksum(record: &[u8]) -> std::result::Result<(), &'static str> {
    if record.iter().fold(0u8, |acc, byte| acc.wrapping_add(*byte)) == 0xff {
        Ok(())
    } else {
        Err("checksum mismatch")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intel_hex_maps_data_and_entry() {
        let text = b":020000040001F9\n:0400100001020304E2\n:0400000500010010E6\n:00000001FF\n";
        let binary = IntelHexLoader::parse(DataBuffer::Heap(text.to_vec()), "test.hex".to_string())
            .expect("load intel hex");
        assert_eq!(binary.format, "Intel HEX");
        assert_eq!(binary.image_base, 0x10010);
        assert_eq!(binary.entry_point, 0x10010);
        assert_eq!(binary.sections[0].virtual_size, 4);
    }

    #[test]
    fn intel_hex_rejects_bad_checksum() {
        let err = IntelHexLoader::parse(
            DataBuffer::Heap(b":0400100001020304E3\n".to_vec()),
            "bad.hex".to_string(),
        )
        .expect_err("bad checksum");
        assert!(format!("{err}").contains("MalformedHeader"));
    }

    #[test]
    fn motorola_hex_maps_data_and_entry() {
        let text = b"S3090000100001020304DC\nS70500001000EA\n";
        let binary =
            MotorolaHexLoader::parse(DataBuffer::Heap(text.to_vec()), "test.srec".to_string())
                .expect("load srec");
        assert_eq!(binary.format, "Motorola S-record");
        assert_eq!(binary.image_base, 0x1000);
        assert_eq!(binary.entry_point, 0x1000);
        assert_eq!(binary.sections[0].virtual_size, 4);
    }
}
