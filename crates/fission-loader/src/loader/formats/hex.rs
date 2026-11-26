use super::sparse::build_sparse_binary;
use crate::loader::pipeline::ExplicitLanguage;
use crate::loader::reader::{ByteReader, SparseImage};
use crate::loader::types::{DataBuffer, LoadedBinary};
use crate::prelude::*;

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

    /// Without a language this validates every record and then fails with
    /// `LoadSpecNotFound`: the file names no processor.
    pub fn parse(data: DataBuffer, path: String) -> Result<LoadedBinary> {
        Self::parse_with_language(data, path, None)
    }

    pub fn parse_with_language(
        data: DataBuffer,
        path: String,
        language: Option<&ExplicitLanguage>,
    ) -> Result<LoadedBinary> {
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

        build_hex_binary("Intel HEX", path, image, entry, language)
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

    /// Without a language this validates every record and then fails with
    /// `LoadSpecNotFound`: the file names no processor.
    pub fn parse(data: DataBuffer, path: String) -> Result<LoadedBinary> {
        Self::parse_with_language(data, path, None)
    }

    pub fn parse_with_language(
        data: DataBuffer,
        path: String,
        language: Option<&ExplicitLanguage>,
    ) -> Result<LoadedBinary> {
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

        build_hex_binary("Motorola S-record", path, image, entry, language)
    }
}

fn build_hex_binary(
    format: &str,
    path: String,
    image: SparseImage,
    entry: Option<u64>,
    language: Option<&ExplicitLanguage>,
) -> Result<LoadedBinary> {
    if image.is_empty() {
        return Err(err!(
            loader,
            "MalformedHeader: {format} contains no data records"
        ));
    }
    let Some(language) = language else {
        return Err(err!(
            loader,
            "LoadSpecNotFound: {format} names no processor; an explicit language is required"
        ));
    };
    build_sparse_binary(format, path, &image, entry, language)
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
    use fission_core::architecture::ArchitectureDescriptor;

    fn language(id: &str, processor: &str, bitness: u8) -> ExplicitLanguage {
        ExplicitLanguage {
            language_id: id.to_string(),
            architecture: ArchitectureDescriptor::new(
                processor,
                "little",
                bitness,
                "default",
                None,
                "test language",
            ),
            source: "test-explicit-language".to_string(),
        }
    }

    fn x86() -> ExplicitLanguage {
        language("x86:LE:32:default", "x86", 32)
    }

    /// Intel HEX record text with its checksum, built so a fixture names
    /// what it holds instead of carrying opaque digits.
    fn ihex(address: u16, record_type: u8, payload: &[u8]) -> String {
        let mut bytes = vec![
            payload.len() as u8,
            (address >> 8) as u8,
            address as u8,
            record_type,
        ];
        bytes.extend_from_slice(payload);
        let sum = bytes.iter().fold(0u8, |acc, b| acc.wrapping_add(*b));
        bytes.push(sum.wrapping_neg());
        format!(
            ":{}",
            bytes.iter().map(|b| format!("{b:02X}")).collect::<String>()
        )
    }

    /// S-record text with its checksum; `address_len` picks S1/S2/S3 widths.
    fn srec(kind: u8, address: u32, address_len: usize, payload: &[u8]) -> String {
        let mut body = address.to_be_bytes()[4 - address_len..].to_vec();
        body.extend_from_slice(payload);
        let mut bytes = vec![(body.len() + 1) as u8];
        bytes.extend_from_slice(&body);
        let sum = bytes.iter().fold(0u8, |acc, b| acc.wrapping_add(*b));
        bytes.push(!sum);
        format!(
            "S{kind}{}",
            bytes.iter().map(|b| format!("{b:02X}")).collect::<String>()
        )
    }

    fn lines(records: &[String]) -> Vec<u8> {
        let mut text = records.join("\n");
        text.push('\n');
        text.into_bytes()
    }

    /// `mov eax, [esp+4]; add eax, eax; ret`
    const X86_DOUBLE: [u8; 7] = [0x8b, 0x44, 0x24, 0x04, 0x01, 0xc0, 0xc3];

    #[test]
    fn intel_hex_with_language_maps_runs_and_seeds_the_start_address() {
        let text = lines(&[
            ihex(0, 0x04, &[0x00, 0x01]),
            ihex(0x0000, 0x00, &X86_DOUBLE[..4]),
            // Continues the first record: one run, not two sections.
            ihex(0x0004, 0x00, &X86_DOUBLE[4..]),
            // A second, separate run.
            ihex(0x0100, 0x00, &[0xaa, 0xbb]),
            ihex(0, 0x05, &0x0001_0000u32.to_be_bytes()),
            ihex(0, 0x01, &[]),
        ]);
        let binary = IntelHexLoader::parse_with_language(
            DataBuffer::Heap(text),
            "fw.hex".to_string(),
            Some(&x86()),
        )
        .expect("loads with an explicit language");

        assert_eq!(binary.format, "Intel HEX");
        assert_eq!(binary.image_base, 0x1_0000);
        assert_eq!(binary.entry_point, 0x1_0000);
        assert_eq!(binary.sleigh_language_id(), Some("x86:LE:32:default"));
        assert_eq!(
            binary.load_spec().map(|spec| spec.source.as_str()),
            Some("test-explicit-language")
        );
        let layout: Vec<_> = binary
            .sections
            .iter()
            .map(|s| {
                (
                    s.name.as_str(),
                    s.virtual_address,
                    s.file_offset,
                    s.file_size,
                )
            })
            .collect();
        assert_eq!(
            layout,
            vec![
                ("load_00010000", 0x1_0000, 0, 7),
                ("load_00010100", 0x1_0100, 7, 2),
            ]
        );
        // The loaded data is the decoded image, not the text.
        assert_eq!(&binary.data.as_slice()[..7], &X86_DOUBLE);
        assert_eq!(&binary.data.as_slice()[7..], &[0xaa, 0xbb]);
        let entry = binary
            .functions
            .iter()
            .find(|f| f.name == "entry")
            .expect("entry seeded");
        assert_eq!(entry.address, 0x1_0000);
        assert_eq!(entry.origin.as_deref(), Some("start-address-record"));
    }

    #[test]
    fn intel_hex_start_address_outside_every_record_seeds_nothing() {
        let text = lines(&[
            ihex(0x1000, 0x00, &X86_DOUBLE),
            ihex(0, 0x05, &0x9000u32.to_be_bytes()),
            ihex(0, 0x01, &[]),
        ]);
        let binary = IntelHexLoader::parse_with_language(
            DataBuffer::Heap(text),
            "fw.hex".to_string(),
            Some(&x86()),
        )
        .expect("loads");
        assert!(
            binary.functions.is_empty(),
            "no code is known to start anywhere"
        );
        assert_eq!(binary.entry_point, 0x1000);
    }

    #[test]
    fn intel_hex_without_start_record_seeds_nothing() {
        let text = lines(&[ihex(0x1000, 0x00, &X86_DOUBLE), ihex(0, 0x01, &[])]);
        let binary = IntelHexLoader::parse_with_language(
            DataBuffer::Heap(text),
            "fw.hex".to_string(),
            Some(&x86()),
        )
        .expect("loads");
        assert!(binary.functions.is_empty());
    }

    #[test]
    fn intel_hex_far_apart_records_do_not_allocate_the_gap() {
        // 0x0000 and 0xffff_fff0: materializing the span would be 4 GiB.
        let text = lines(&[
            ihex(0, 0x04, &[0x00, 0x00]),
            ihex(0x0000, 0x00, &[0x90]),
            ihex(0, 0x04, &[0xff, 0xff]),
            ihex(0xfff0, 0x00, &[0xc3]),
            ihex(0, 0x01, &[]),
        ]);
        let binary = IntelHexLoader::parse_with_language(
            DataBuffer::Heap(text),
            "fw.hex".to_string(),
            Some(&x86()),
        )
        .expect("loads");
        assert_eq!(binary.data.as_slice(), &[0x90, 0xc3]);
        assert_eq!(binary.sections.len(), 2);
        assert_eq!(binary.sections[1].virtual_address, 0xffff_fff0);
    }

    #[test]
    fn intel_hex_conflicting_overlap_is_malformed() {
        let text = lines(&[
            ihex(0x1000, 0x00, &[1, 2, 3, 4]),
            ihex(0x1002, 0x00, &[9, 9]),
            ihex(0, 0x01, &[]),
        ]);
        let err = IntelHexLoader::parse_with_language(
            DataBuffer::Heap(text),
            "fw.hex".to_string(),
            Some(&x86()),
        )
        .expect_err("two records disagree about 0x1002");
        assert!(format!("{err}").contains("MalformedHeader"), "{err}");
    }

    #[test]
    fn intel_hex_identical_overlap_merges() {
        let text = lines(&[
            ihex(0x1000, 0x00, &[1, 2, 3, 4]),
            ihex(0x1002, 0x00, &[3, 4, 5]),
            ihex(0, 0x01, &[]),
        ]);
        let binary = IntelHexLoader::parse_with_language(
            DataBuffer::Heap(text),
            "fw.hex".to_string(),
            Some(&x86()),
        )
        .expect("agreeing records merge");
        assert_eq!(binary.data.as_slice(), &[1, 2, 3, 4, 5]);
        assert_eq!(binary.sections.len(), 1);
    }

    #[test]
    fn intel_hex_truncated_record_is_malformed_even_with_a_language() {
        let mut text = lines(&[ihex(0x1000, 0x00, &X86_DOUBLE)]);
        // Cut the record mid-payload: the byte count no longer matches.
        text.truncate(text.len() - 7);
        text.push(b'\n');
        let err = IntelHexLoader::parse_with_language(
            DataBuffer::Heap(text),
            "cut.hex".to_string(),
            Some(&x86()),
        )
        .expect_err("truncated record");
        assert!(format!("{err}").contains("MalformedHeader"), "{err}");
    }

    #[test]
    fn intel_hex_with_only_an_eof_record_is_malformed() {
        let err = IntelHexLoader::parse_with_language(
            DataBuffer::Heap(lines(&[ihex(0, 0x01, &[])])),
            "empty.hex".to_string(),
            Some(&x86()),
        )
        .expect_err("no data");
        assert!(format!("{err}").contains("no data records"), "{err}");
    }

    #[test]
    fn motorola_srecord_with_language_keeps_the_thumb_start_address() {
        let thumb = [0x40, 0x18, 0x70, 0x47]; // adds r0, r0, r1; bx lr
        let text = lines(&[
            srec(0, 0, 2, b"fx"),
            srec(3, 0x0800_0000, 4, &thumb),
            srec(7, 0x0800_0001, 4, &[]),
        ]);
        let binary = MotorolaHexLoader::parse_with_language(
            DataBuffer::Heap(text),
            "fw.srec".to_string(),
            Some(&language("ARM:LE:32:Cortex", "ARM", 32)),
        )
        .expect("loads");
        assert_eq!(binary.format, "Motorola S-record");
        assert_eq!(binary.image_base, 0x0800_0000);
        // The start address keeps its Thumb bit; the bit is only ignored when
        // asking whether a record covers it.
        assert_eq!(binary.entry_point, 0x0800_0001);
        assert_eq!(binary.data.as_slice(), &thumb);
        assert!(binary.functions.iter().any(|f| f.address == 0x0800_0001));
    }

    #[test]
    fn motorola_srecord_truncated_address_is_malformed() {
        // An S3 record whose count covers only two of its four address bytes.
        let err = MotorolaHexLoader::parse_with_language(
            DataBuffer::Heap(b"S3030800F4\n".to_vec()),
            "cut.srec".to_string(),
            Some(&x86()),
        )
        .expect_err("address truncated");
        assert!(format!("{err}").contains("MalformedHeader"), "{err}");
    }

    #[test]
    fn intel_hex_maps_data_and_entry() {
        let text = b":020000040001F9\n:0400100001020304E2\n:0400000500010010E6\n:00000001FF\n";
        let err = IntelHexLoader::parse(DataBuffer::Heap(text.to_vec()), "test.hex".to_string())
            .expect_err("Intel HEX needs an explicit load spec");
        assert!(format!("{err}").contains("LoadSpecNotFound"));
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
        let err =
            MotorolaHexLoader::parse(DataBuffer::Heap(text.to_vec()), "test.srec".to_string())
                .expect_err("Motorola S-record needs an explicit load spec");
        assert!(format!("{err}").contains("LoadSpecNotFound"));
    }
}
