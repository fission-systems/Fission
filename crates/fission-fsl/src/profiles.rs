//! Owned profile source and binary catalog. No Ghidra IDs or ABI inference.
use crate::{sequence::hex, FslError};
use fission_loader::loader::{
    pipeline::DetectedFormat,
    profiles::{OwnedProfile, OwnedProfileCatalog},
    reader::Endian,
};
use sha2::{Digest, Sha256};

const MAGIC: &[u8; 8] = b"FSLPROF1";
const LIMIT: usize = 1024 * 1024;

fn invalid(message: impl Into<String>) -> FslError {
    FslError::at(1, 1, message)
}

#[derive(Clone, Debug)]
pub struct ProfilePackage {
    pub catalog: OwnedProfileCatalog,
    pub source_sha256: [u8; 32],
}

impl ProfilePackage {
    /// Version-one grammar uses one declaration per line in the documented order.
    pub fn compile(source: &str) -> Result<Self, FslError> {
        if source.len() > LIMIT || !source.is_ascii() {
            return Err(invalid("profile source must be ASCII and at most 1 MiB"));
        }
        let lines: Vec<_> = source
            .lines()
            .enumerate()
            .filter_map(|(i, line)| {
                let line = line.split('#').next().unwrap_or("").trim();
                (!line.is_empty()).then_some((i + 1, line))
            })
            .collect();
        if lines.first().map(|l| l.1) != Some("profiles 1 {")
            || lines.last().map(|l| l.1) != Some("}")
        {
            return Err(invalid("expected profiles 1 { ... }"));
        }
        let mut profiles = Vec::new();
        let mut i = 1;
        while i + 1 < lines.len() {
            let fields = |offset: usize, count: usize| -> Result<Vec<&str>, FslError> {
                let (number, line) = *lines
                    .get(i + offset)
                    .ok_or_else(|| invalid("truncated profile"))?;
                let line = if offset == 0 {
                    line
                } else {
                    line.strip_suffix(';').ok_or_else(|| {
                        FslError::at(number, 1, "profile declaration requires semicolon")
                    })?
                };
                let words: Vec<_> = line.split_whitespace().collect();
                if words.len() != count {
                    return Err(FslError::at(number, 1, "invalid profile declaration arity"));
                }
                Ok(words)
            };
            let header = fields(0, 3)?;
            let target_match = fields(1, 11)?;
            let target = fields(2, 3)?;
            let decoder = fields(3, 2)?;
            let package = fields(4, 2)?;
            let abi = fields(5, 2)?;
            if header[0] != "profile"
                || header[2] != "{"
                || target_match[0] != "match"
                || target_match[1] != "elf"
                || target_match[2] != "machine"
                || target_match[4] != "bits"
                || target_match[6] != "endian"
                || target_match[8] != "flags"
                || target[0] != "target"
                || decoder[0] != "decoder"
                || package[0] != "package"
                || abi != ["abi", "unselected"]
                || lines.get(i + 6).map(|l| l.1) != Some("}")
            {
                return Err(FslError::at(
                    lines[i].0,
                    1,
                    "unsupported profile syntax or ABI selection",
                ));
            }
            let number = |s: &str| -> Result<u32, FslError> {
                if let Some(s) = s.strip_prefix("0x") {
                    u32::from_str_radix(s, 16)
                } else {
                    s.parse()
                }
                .map_err(|_| invalid("invalid profile integer"))
            };
            profiles.push(OwnedProfile {
                id: header[1].into(),
                format: DetectedFormat::Elf,
                machine: u16::try_from(number(target_match[3])?)
                    .map_err(|_| invalid("machine exceeds u16"))?,
                bitness: u8::try_from(number(target_match[5])?)
                    .map_err(|_| invalid("bitness exceeds u8"))?,
                endian: match target_match[7] {
                    "little" => Endian::Little,
                    "big" => Endian::Big,
                    _ => return Err(invalid("invalid byte order")),
                },
                flags_mask: number(target_match[9])?,
                flags_value: number(target_match[10])?,
                processor: target[1].into(),
                variant: target[2].into(),
                decoder: decoder[1].into(),
                package_sha256: package[1].into(),
            });
            if profiles.len() > 1024 {
                return Err(invalid("too many profiles"));
            }
            i += 7;
        }
        if i != lines.len() - 1 {
            return Err(invalid("trailing profile declarations"));
        }
        Ok(Self {
            catalog: OwnedProfileCatalog::new(profiles).map_err(|e| invalid(e.to_string()))?,
            source_sha256: Sha256::digest(source.as_bytes()).into(),
        })
    }

    pub fn encode_binary(&self) -> Result<Vec<u8>, FslError> {
        // Revalidate public catalog replacements; the catalog itself is immutable.
        let catalog = OwnedProfileCatalog::new(self.catalog.profiles().to_vec())
            .map_err(|e| invalid(e.to_string()))?;
        let mut out = MAGIC.to_vec();
        out.extend(1u16.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out.extend(self.source_sha256);
        out.extend((catalog.profiles().len() as u16).to_le_bytes());
        for p in catalog.profiles() {
            for s in [
                &p.id,
                &p.processor,
                &p.variant,
                &p.decoder,
                &p.package_sha256,
            ] {
                out.extend((s.len() as u16).to_le_bytes());
                out.extend(s.as_bytes());
            }
            out.push(1); // format ELF
            out.extend(p.machine.to_le_bytes());
            out.push(p.bitness);
            out.push(if p.endian == Endian::Little { 1 } else { 2 });
            out.extend(p.flags_mask.to_le_bytes());
            out.extend(p.flags_value.to_le_bytes());
        }
        Ok(out)
    }

    pub fn decode_binary(raw: &[u8]) -> Result<Self, FslError> {
        struct Reader<'a> {
            bytes: &'a [u8],
            pos: usize,
        }
        impl<'a> Reader<'a> {
            fn take(&mut self, n: usize) -> Result<&'a [u8], FslError> {
                let end = self
                    .pos
                    .checked_add(n)
                    .ok_or_else(|| invalid("profile extent overflow"))?;
                let v = self
                    .bytes
                    .get(self.pos..end)
                    .ok_or_else(|| invalid("truncated profile package"))?;
                self.pos = end;
                Ok(v)
            }
            fn u16(&mut self) -> Result<u16, FslError> {
                Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
            }
            fn u32(&mut self) -> Result<u32, FslError> {
                Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
            }
            fn string(&mut self) -> Result<String, FslError> {
                let n = usize::from(self.u16()?);
                if n == 0 || n > 128 {
                    return Err(invalid("invalid profile string length"));
                }
                std::str::from_utf8(self.take(n)?)
                    .map(str::to_owned)
                    .map_err(|_| invalid("invalid profile UTF-8"))
            }
        }
        if raw.len() > LIMIT {
            return Err(invalid("profile package exceeds 1 MiB"));
        }
        let mut r = Reader { bytes: raw, pos: 0 };
        if r.take(8)? != MAGIC || r.u16()? != 1 || r.u16()? != 0 {
            return Err(invalid("unsupported profile header"));
        }
        let source_sha256 = r.take(32)?.try_into().unwrap();
        let count = r.u16()?;
        if !(1..=1024).contains(&count) {
            return Err(invalid("invalid profile count"));
        }
        let mut profiles = Vec::with_capacity(usize::from(count));
        for _ in 0..count {
            let id = r.string()?;
            let processor = r.string()?;
            let variant = r.string()?;
            let decoder = r.string()?;
            let package_sha256 = r.string()?;
            if r.take(1)? != [1] {
                return Err(invalid("unsupported profile format"));
            }
            let machine = r.u16()?;
            let bitness = r.take(1)?[0];
            let endian = match r.take(1)?[0] {
                1 => Endian::Little,
                2 => Endian::Big,
                _ => return Err(invalid("invalid profile endian tag")),
            };
            profiles.push(OwnedProfile {
                id,
                format: DetectedFormat::Elf,
                machine,
                bitness,
                endian,
                flags_mask: r.u32()?,
                flags_value: r.u32()?,
                processor,
                variant,
                decoder,
                package_sha256,
            });
        }
        if r.pos != raw.len() {
            return Err(invalid("trailing profile package bytes"));
        }
        Ok(Self {
            catalog: OwnedProfileCatalog::new(profiles).map_err(|e| invalid(e.to_string()))?,
            source_sha256,
        })
    }

    pub fn describe(&self) -> String {
        let mut out = format!(
            "profile-package=1 source-sha256={} profiles={} abi=unselected\n",
            hex(&self.source_sha256),
            self.catalog.profiles().len()
        );
        for p in self.catalog.profiles() {
            out.push_str(&format!("profile={} format=ELF machine={} bits={} endian={:?} flags-mask=0x{:x} flags-value=0x{:x} processor={} variant={} decoder={} package-sha256={}\n",p.id,p.machine,p.bitness,p.endian,p.flags_mask,p.flags_value,p.processor,p.variant,p.decoder,p.package_sha256));
        }
        out
    }
}
