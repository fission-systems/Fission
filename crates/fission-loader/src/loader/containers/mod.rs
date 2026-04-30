use crate::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContainerFormat {
    CompoundDocument,
    ZipArchive,
    Gzip,
    Cabinet,
}

impl ContainerFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CompoundDocument => "CompoundDocument",
            Self::ZipArchive => "ZipArchive",
            Self::Gzip => "Gzip",
            Self::Cabinet => "Cabinet",
        }
    }
}

const CFB_MAGIC: [u8; 8] = [0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1];
const CFB_HEADER_LEN: usize = 512;
const CFB_BYTE_ORDER: u16 = 0xfffe;
const CFB_FREESECT: u32 = 0xffff_ffff;
const CFB_ENDOFCHAIN: u32 = 0xffff_fffe;

pub fn detect_container(bytes: &[u8]) -> Result<Option<ContainerFormat>> {
    if bytes.starts_with(&CFB_MAGIC) {
        validate_compound_document_header(bytes)?;
        return Ok(Some(ContainerFormat::CompoundDocument));
    }
    if looks_like_zip(bytes) {
        return Ok(Some(ContainerFormat::ZipArchive));
    }
    if looks_like_gzip(bytes) {
        return Ok(Some(ContainerFormat::Gzip));
    }
    if looks_like_cabinet(bytes) {
        return Ok(Some(ContainerFormat::Cabinet));
    }
    Ok(None)
}

fn looks_like_zip(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
        || bytes.starts_with(b"PK\x05\x06")
        || bytes.starts_with(b"PK\x07\x08")
}

fn looks_like_gzip(bytes: &[u8]) -> bool {
    bytes.len() >= 10 && bytes[0] == 0x1f && bytes[1] == 0x8b && bytes[2] == 0x08
}

fn looks_like_cabinet(bytes: &[u8]) -> bool {
    if bytes.len() < 36 || !bytes.starts_with(b"MSCF") {
        return false;
    }
    let cb_cabinet = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
    cb_cabinet >= 36 && cb_cabinet <= bytes.len()
}

fn validate_compound_document_header(bytes: &[u8]) -> Result<()> {
    if bytes.len() < CFB_HEADER_LEN {
        return Err(FissionError::loader(
            "MalformedHeader: CompoundDocument header is truncated",
        ));
    }
    let minor_version = u16_le(bytes, 0x18)?;
    let major_version = u16_le(bytes, 0x1a)?;
    let byte_order = u16_le(bytes, 0x1c)?;
    let sector_shift = u16_le(bytes, 0x1e)?;
    let mini_sector_shift = u16_le(bytes, 0x20)?;
    let num_fat_sectors = u32_le(bytes, 0x2c)?;
    let first_dir_sector = u32_le(bytes, 0x30)?;
    let mini_stream_cutoff = u32_le(bytes, 0x38)?;
    let first_mini_fat_sector = u32_le(bytes, 0x3c)?;
    let num_mini_fat_sectors = u32_le(bytes, 0x40)?;
    let first_difat_sector = u32_le(bytes, 0x44)?;
    let num_difat_sectors = u32_le(bytes, 0x48)?;

    if byte_order != CFB_BYTE_ORDER {
        return Err(FissionError::loader(
            "MalformedHeader: CompoundDocument byte order is invalid",
        ));
    }
    if !(major_version == 3 || major_version == 4) {
        return Err(FissionError::loader(
            "MalformedHeader: CompoundDocument major version is unsupported",
        ));
    }
    if (major_version == 3 && sector_shift != 9) || (major_version == 4 && sector_shift != 12) {
        return Err(FissionError::loader(
            "MalformedHeader: CompoundDocument sector size does not match major version",
        ));
    }
    if mini_sector_shift != 6 {
        return Err(FissionError::loader(
            "MalformedHeader: CompoundDocument mini sector size is invalid",
        ));
    }
    if mini_stream_cutoff != 4096 {
        return Err(FissionError::loader(
            "MalformedHeader: CompoundDocument mini stream cutoff is invalid",
        ));
    }
    if num_fat_sectors == 0 {
        return Err(FissionError::loader(
            "MalformedHeader: CompoundDocument has no FAT sectors",
        ));
    }
    let sector_size = 1usize
        .checked_shl(sector_shift as u32)
        .ok_or_else(|| FissionError::loader("MalformedHeader: CompoundDocument sector size overflow"))?;
    let sector_count = bytes
        .len()
        .saturating_sub(CFB_HEADER_LEN)
        .checked_div(sector_size)
        .unwrap_or(0) as u32;
    if num_fat_sectors > sector_count {
        return Err(FissionError::loader(
            "MalformedHeader: CompoundDocument FAT sector count exceeds file size",
        ));
    }
    validate_sector_id(first_dir_sector, sector_count, false, "directory")?;
    if num_mini_fat_sectors > 0 || first_mini_fat_sector != CFB_ENDOFCHAIN {
        validate_sector_id(first_mini_fat_sector, sector_count, true, "mini FAT")?;
    }
    if num_difat_sectors > 0 || first_difat_sector != CFB_ENDOFCHAIN {
        validate_sector_id(first_difat_sector, sector_count, true, "DIFAT")?;
    }

    // Minor version is not used for routing, but reading it keeps the validation
    // anchored to the CFB header shape rather than only the leading magic.
    let _ = minor_version;
    Ok(())
}

fn validate_sector_id(id: u32, sector_count: u32, allow_end: bool, label: &str) -> Result<()> {
    if id == CFB_FREESECT || (allow_end && id == CFB_ENDOFCHAIN) {
        return Ok(());
    }
    if id < sector_count {
        return Ok(());
    }
    Err(FissionError::loader(format!(
        "MalformedHeader: CompoundDocument {label} sector is out of bounds"
    )))
}

fn u16_le(bytes: &[u8], offset: usize) -> Result<u16> {
    let raw = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| FissionError::loader("MalformedHeader: CompoundDocument u16 out of bounds"))?;
    Ok(u16::from_le_bytes([raw[0], raw[1]]))
}

fn u32_le(bytes: &[u8], offset: usize) -> Result<u32> {
    let raw = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| FissionError::loader("MalformedHeader: CompoundDocument u32 out of bounds"))?;
    Ok(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_cfb_header() -> Vec<u8> {
        let mut bytes = vec![0u8; 1536];
        bytes[0..8].copy_from_slice(&CFB_MAGIC);
        bytes[0x18..0x1a].copy_from_slice(&0x003eu16.to_le_bytes());
        bytes[0x1a..0x1c].copy_from_slice(&3u16.to_le_bytes());
        bytes[0x1c..0x1e].copy_from_slice(&CFB_BYTE_ORDER.to_le_bytes());
        bytes[0x1e..0x20].copy_from_slice(&9u16.to_le_bytes());
        bytes[0x20..0x22].copy_from_slice(&6u16.to_le_bytes());
        bytes[0x2c..0x30].copy_from_slice(&1u32.to_le_bytes());
        bytes[0x30..0x34].copy_from_slice(&1u32.to_le_bytes());
        bytes[0x38..0x3c].copy_from_slice(&4096u32.to_le_bytes());
        bytes[0x3c..0x40].copy_from_slice(&CFB_ENDOFCHAIN.to_le_bytes());
        bytes[0x44..0x48].copy_from_slice(&CFB_ENDOFCHAIN.to_le_bytes());
        bytes[0x4c..0x50].copy_from_slice(&0u32.to_le_bytes());
        bytes
    }

    #[test]
    fn detects_valid_compound_document() {
        assert_eq!(
            detect_container(&valid_cfb_header()).unwrap(),
            Some(ContainerFormat::CompoundDocument)
        );
    }

    #[test]
    fn rejects_malformed_compound_document_header() {
        let mut bytes = valid_cfb_header();
        bytes[0x1c..0x1e].copy_from_slice(&0xfeffu16.to_le_bytes());
        let err = detect_container(&bytes).expect_err("invalid CFB must be malformed");
        assert!(format!("{err}").contains("MalformedHeader: CompoundDocument"));
    }
}
