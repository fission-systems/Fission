use fission_core::common::types::FunctionInfo;
use gimli::{BaseAddresses, EhFrame, RunTimeEndian, UnwindSection};

pub(crate) fn parse_eh_frame(
    eh_frame_data: &[u8],
    eh_frame_addr: u64,
    text_addr: u64,
    _data_addr: u64,
    is_little_endian: bool,
    _is_64bit: bool,
) -> Vec<FunctionInfo> {
    let mut functions = Vec::new();

    let endian = if is_little_endian {
        RunTimeEndian::Little
    } else {
        RunTimeEndian::Big
    };

    let eh_frame = EhFrame::new(eh_frame_data, endian);

    let mut bases = BaseAddresses::default().set_eh_frame(eh_frame_addr);
    if text_addr != 0 {
        bases = bases.set_text(text_addr);
    }

    // We assume the native pointer size for FDE decoding based on is_64bit.
    // Gimli handles parsing CIE/FDE automatically, but we need to provide bases.

    let mut entries = eh_frame.entries(&bases);
    loop {
        match entries.next() {
            Ok(Some(entry)) => {
                match entry {
                    gimli::CieOrFde::Fde(partial_fde) => {
                        match partial_fde
                            .parse(|_, bases, offset| eh_frame.cie_from_offset(bases, offset))
                        {
                            Ok(fde) => {
                                let start = fde.initial_address();
                                let size = fde.len() as u64;

                                functions.push(FunctionInfo {
                                    address: start,
                                    size,
                                    origin: Some("elf-eh_frame".to_string()),
                                    kind: Some("eh_frame".to_string()),
                                    source_section: Some(".eh_frame".to_string()),
                                    ..Default::default()
                                });
                            }
                            Err(e) => {
                                eprintln!("Failed to parse FDE: {:?}", e);
                            }
                        }
                    }
                    gimli::CieOrFde::Cie(_) => {
                        // Ignore CIEs
                    }
                }
            }
            Ok(None) => break,
            Err(e) => {
                eprintln!("Failed to read next entry: {:?}", e);
                break;
            }
        }
    }

    functions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_eh_frame() {
        // A simple valid .eh_frame byte stream with 1 CIE and 1 FDE for x86_64
        // Generated typically by gcc/clang.
        let eh_frame_data: [u8; 48] = [
            // CIE (24 bytes total: 4 length + 20 payload)
            0x14, 0x00, 0x00, 0x00, // Length (20)
            0x00, 0x00, 0x00, 0x00, // CIE ID
            0x01, // Version
            0x7a, 0x52, 0x00, // Augmentation String "zR"
            0x01, // Code alignment factor
            0x78, // Data alignment factor
            0x10, // Return address register
            0x01, // Augmentation data length
            0x1b, // pcrel | sdata4
            0x0c, 0x07, 0x08, 0x90, 0x01, 0x00, 0x00, // Instructions
            // FDE (24 bytes total: 4 length + 20 payload)
            0x14, 0x00, 0x00, 0x00, // Length (20)
            0x1c, 0x00, 0x00, 0x00, // CIE pointer (relative, 28)
            0x10, 0x10, 0x00, 0x00, // Initial location (PC relative offset)
            0x50, 0x00, 0x00, 0x00, // Address range (0x50 bytes)
            0x00, // Augmentation length
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, // Padding (7 bytes to reach 20 bytes payload)
        ];

        let funcs = parse_eh_frame(&eh_frame_data, 0x2000, 0x1000, 0x3000, true, true);

        // Given initial location is relative, we should just verify gimli parses it successfully.
        // It should extract at least 1 function.
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].size, 0x50);
        assert_eq!(funcs[0].source_section.as_deref(), Some(".eh_frame"));
    }
}
