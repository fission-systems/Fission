use crate::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Endian {
    Little,
    Big,
}

/// Bounds-checked byte reader used by canonical loader paths.
///
/// Format loaders own interpretation and provenance; this helper only provides
/// primitive endian-aware reads and string extraction.
#[derive(Clone, Copy, Debug)]
pub struct ByteReader<'a> {
    bytes: &'a [u8],
    endian: Endian,
}

impl<'a> ByteReader<'a> {
    pub fn new(bytes: &'a [u8], endian: Endian) -> Self {
        Self { bytes, endian }
    }

    pub fn little(bytes: &'a [u8]) -> Self {
        Self::new(bytes, Endian::Little)
    }

    pub fn big(bytes: &'a [u8]) -> Self {
        Self::new(bytes, Endian::Big)
    }

    pub fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn slice(&self, offset: usize, len: usize) -> Result<&'a [u8]> {
        let end = offset
            .checked_add(len)
            .ok_or_else(|| err!(loader, "byte range overflow"))?;
        self.bytes
            .get(offset..end)
            .ok_or_else(|| err!(loader, "byte range out of bounds at 0x{offset:x}"))
    }

    pub fn u8(&self, offset: usize) -> Result<u8> {
        self.bytes
            .get(offset)
            .copied()
            .ok_or_else(|| err!(loader, "u8 out of bounds at 0x{offset:x}"))
    }

    pub fn u16(&self, offset: usize) -> Result<u16> {
        let raw: [u8; 2] = self.slice(offset, 2)?.try_into().unwrap();
        Ok(match self.endian {
            Endian::Little => u16::from_le_bytes(raw),
            Endian::Big => u16::from_be_bytes(raw),
        })
    }

    pub fn i16(&self, offset: usize) -> Result<i16> {
        Ok(self.u16(offset)? as i16)
    }

    pub fn u32(&self, offset: usize) -> Result<u32> {
        let raw: [u8; 4] = self.slice(offset, 4)?.try_into().unwrap();
        Ok(match self.endian {
            Endian::Little => u32::from_le_bytes(raw),
            Endian::Big => u32::from_be_bytes(raw),
        })
    }

    pub fn i32(&self, offset: usize) -> Result<i32> {
        Ok(self.u32(offset)? as i32)
    }

    pub fn u64(&self, offset: usize) -> Result<u64> {
        let raw: [u8; 8] = self.slice(offset, 8)?.try_into().unwrap();
        Ok(match self.endian {
            Endian::Little => u64::from_le_bytes(raw),
            Endian::Big => u64::from_be_bytes(raw),
        })
    }

    pub fn fixed_string(&self, offset: usize, len: usize) -> Result<String> {
        let bytes = self.slice(offset, len)?;
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        Ok(String::from_utf8_lossy(&bytes[..end]).to_string())
    }

    pub fn cstring(&self, offset: usize) -> String {
        if offset >= self.bytes.len() {
            return String::new();
        }
        let tail = &self.bytes[offset..];
        let end = tail.iter().position(|&b| b == 0).unwrap_or(tail.len());
        String::from_utf8_lossy(&tail[..end]).to_string()
    }

    pub fn ascii_lines(&self) -> impl Iterator<Item = &'a str> + 'a {
        self.bytes
            .split(|&b| b == b'\n')
            .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
            .filter_map(|line| std::str::from_utf8(line).ok())
    }
}

#[derive(Clone, Debug, Default)]
pub struct SparseImage {
    ranges: Vec<(u64, Vec<u8>)>,
}

impl SparseImage {
    pub fn new() -> Self {
        Self { ranges: Vec::new() }
    }

    pub fn write(&mut self, address: u64, bytes: &[u8]) -> Result<()> {
        if bytes.is_empty() {
            return Ok(());
        }
        address
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| err!(loader, "address range overflow"))?;
        self.ranges.push((address, bytes.to_vec()));
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    pub fn bounds(&self) -> Option<(u64, u64)> {
        let start = self.ranges.iter().map(|(addr, _)| *addr).min()?;
        let end = self
            .ranges
            .iter()
            .filter_map(|(addr, bytes)| addr.checked_add(bytes.len() as u64))
            .max()?;
        Some((start, end))
    }

    pub fn materialize(&self, fill: u8) -> Result<(u64, Vec<u8>)> {
        let (start, end) = self
            .bounds()
            .ok_or_else(|| err!(loader, "empty sparse image"))?;
        let size = end
            .checked_sub(start)
            .ok_or_else(|| err!(loader, "sparse image range underflow"))?;
        if size > usize::MAX as u64 {
            return Err(err!(loader, "sparse image too large"));
        }
        let mut out = vec![fill; size as usize];
        for (addr, bytes) in &self.ranges {
            let offset = addr
                .checked_sub(start)
                .ok_or_else(|| err!(loader, "sparse range before image base"))?
                as usize;
            let end = offset
                .checked_add(bytes.len())
                .ok_or_else(|| err!(loader, "sparse range overflow"))?;
            out.get_mut(offset..end)
                .ok_or_else(|| err!(loader, "sparse range out of materialized bounds"))?
                .copy_from_slice(bytes);
        }
        Ok((start, out))
    }
}
