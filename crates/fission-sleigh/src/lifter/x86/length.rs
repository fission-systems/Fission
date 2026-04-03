use anyhow::{bail, Context, Result};

pub(crate) fn decode_len(bytes: &[u8]) -> Result<u64> {
    if bytes.is_empty() {
        bail!("x86 decode received empty bytes");
    }

    let mut i = 0usize;
    while i < bytes.len() && is_prefix(bytes[i]) {
        i += 1;
    }
    if i >= bytes.len() {
        bail!("x86 decode found only prefixes");
    }

    let mut opcode = bytes[i];
    i += 1;
    let mut ext: Option<u8> = None;

    if opcode == 0x0F {
        if i >= bytes.len() {
            bail!("x86 truncated 0x0F escape opcode");
        }
        opcode = bytes[i];
        ext = Some(opcode);
        i += 1;
    }

    let needs_modrm = needs_modrm(opcode, ext);
    if needs_modrm {
        if i >= bytes.len() {
            bail!("x86 missing ModRM byte");
        }
        let modrm = bytes[i];
        i += 1;

        let mode = (modrm >> 6) & 0x3;
        let rm = modrm & 0x7;

        if mode != 3 && rm == 4 {
            if i >= bytes.len() {
                bail!("x86 missing SIB byte");
            }
            let sib = bytes[i];
            i += 1;
            let base = sib & 0x7;
            if mode == 0 && base == 5 {
                i = i.saturating_add(4);
            }
        }

        match mode {
            0 => {
                if rm == 5 {
                    i = i.saturating_add(4);
                }
            }
            1 => i = i.saturating_add(1),
            2 => i = i.saturating_add(4),
            _ => {}
        }
    }

    i = i.saturating_add(imm_len(opcode, ext));
    if i > bytes.len() {
        bail!(
            "x86 instruction truncated: need {} bytes, have {}",
            i,
            bytes.len()
        );
    }

    u64::try_from(i).context("x86 decoded length does not fit u64")
}

fn needs_modrm(opcode: u8, ext: Option<u8>) -> bool {
    if let Some(second) = ext {
        return !matches!(second, 0x80..=0x8F | 0x05 | 0x34 | 0x35);
    }

    !matches!(
        opcode,
        0x6A
            | 0x68
            | 0x90
            | 0xC3
            | 0xCB
            | 0xC2
            | 0xCA
            | 0xE8
            | 0xE9
            | 0xEB
            | 0x70..=0x7F
            | 0xA0
            | 0xA1
            | 0xA2
            | 0xA3
            | 0xB0..=0xBF
    )
}

fn imm_len(opcode: u8, ext: Option<u8>) -> usize {
    if let Some(second) = ext {
        if (0x80..=0x8F).contains(&second) {
            return 4;
        }
        return 0;
    }

    match opcode {
        0xC2 | 0xCA => 2,
        0x6A | 0xEB | 0x70..=0x7F | 0xCD => 1,
        0x68 | 0xE8 | 0xE9 | 0xA0 | 0xA1 | 0xA2 | 0xA3 => 4,
        0xB8..=0xBF => 4,
        _ => 0,
    }
}

fn is_prefix(byte: u8) -> bool {
    matches!(
        byte,
        0xF0
            | 0xF2
            | 0xF3
            | 0x2E
            | 0x36
            | 0x3E
            | 0x26
            | 0x64
            | 0x65
            | 0x66
            | 0x67
            | 0x40..=0x4F
    )
}
