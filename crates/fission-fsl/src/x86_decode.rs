//! Fission-owned bounded x86 stream decoding. No external instruction decoder.
//! FSL owns opcode selection; this module supplies shared prefix/operand syntax.
use crate::FslError;
use std::collections::BTreeSet;
fn error(s: impl Into<String>) -> FslError {
    FslError::at(1, 1, s)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Form {
    RmReg,
    RegRm,
    RmImm,
    RmSimm8,
    AccImm,
    OpregImm,
    Lea,
    Branch8,
    Branch,
    Ret,
    Nop,
}
impl Form {
    fn name(self) -> &'static str {
        match self {
            Self::RmReg => "rm_reg",
            Self::RegRm => "reg_rm",
            Self::RmImm => "rm_imm",
            Self::RmSimm8 => "rm_simm8",
            Self::AccImm => "acc_imm",
            Self::OpregImm => "opreg_imm",
            Self::Lea => "lea",
            Self::Branch8 => "branch8",
            Self::Branch => "branch",
            Self::Ret => "ret",
            Self::Nop => "nop",
        }
    }
    fn parse(s: &str) -> Result<Self, FslError> {
        [
            Self::RmReg,
            Self::RegRm,
            Self::RmImm,
            Self::RmSimm8,
            Self::AccImm,
            Self::OpregImm,
            Self::Lea,
            Self::Branch8,
            Self::Branch,
            Self::Ret,
            Self::Nop,
        ]
        .into_iter()
        .find(|f| f.name() == s)
        .ok_or_else(|| error("unknown x86 operand form"))
    }
    fn has_modrm(self) -> bool {
        matches!(
            self,
            Self::RmReg | Self::RegRm | Self::RmImm | Self::RmSimm8 | Self::Lea
        )
    }
}
#[derive(Clone, Debug)]
pub(crate) struct Rule {
    pub name: String,
    pub opcode: Vec<u8>,
    pub mask: u8,
    pub extension: Option<u8>,
    pub form: Form,
    pub body: String,
}
pub(crate) fn parse(source: &str) -> Result<Vec<Rule>, FslError> {
    let lines: Vec<_> = source
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
        .collect();
    if lines.len() < 4
        || lines.first() != Some(&"frontend x86.scalar {")
        || lines.last() != Some(&"}")
        || lines[1] != "decoder owned-x86-v1;"
    {
        return Err(error("expected Fission-owned x86 frontend grammar"));
    }
    let mut rules = Vec::new();
    for line in &lines[2..lines.len() - 1] {
        let w: Vec<_> = line
            .strip_suffix(';')
            .ok_or_else(|| error("rule requires semicolon"))?
            .split_whitespace()
            .collect();
        if w.len() != 7 || w[0] != "rule" {
            return Err(error("expected rule NAME OPCODE MASK EXT FORM BODY;"));
        }
        if !matches!(w[2].len(), 2 | 4) || !w[2].is_ascii() {
            return Err(error("opcode requires one byte or 0F plus one byte"));
        }
        let opcode = (0..w[2].len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&w[2][i..i + 2], 16).map_err(|_| error("bad opcode hex")))
            .collect::<Result<Vec<_>, _>>()?;
        rules.push(Rule {
            name: w[1].into(),
            opcode,
            mask: u8::from_str_radix(w[3], 16).map_err(|_| error("bad opcode mask"))?,
            extension: if w[4] == "-" {
                None
            } else {
                Some(w[4].parse().map_err(|_| error("bad ModRM extension"))?)
            },
            form: Form::parse(w[5])?,
            body: w[6].into(),
        });
    }
    validate(&rules)?;
    Ok(rules)
}
pub(crate) fn validate(rules: &[Rule]) -> Result<(), FslError> {
    if rules.is_empty() || rules.len() > 256 {
        return Err(error("expected 1..256 x86 rules"));
    }
    let mut names = BTreeSet::new();
    for r in rules {
        let id = |s: &str| {
            !s.is_empty()
                && s.len() <= 128
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'{' || b == b'}')
        };
        if !id(&r.name)
            || !id(&r.body)
            || !names.insert(&r.name)
            || !matches!(r.opcode.len(), 1 | 2)
            || r.mask == 0
            || r.opcode.last().is_none_or(|&b| b & !r.mask != 0)
            || r.extension.is_some_and(|e| e > 7)
        {
            return Err(error("invalid x86 rule metadata"));
        }
        if (r.opcode.len() == 2 && r.opcode[0] != 0x0f)
            || (r.opcode.len() == 1 && r.opcode[0] == 0x0f)
        {
            return Err(error("unsupported opcode escape grammar"));
        }
        if r.extension.is_some() != matches!(r.form, Form::RmImm | Form::RmSimm8) {
            return Err(error("group form requires an explicit ModRM extension"));
        }
        if r.form == Form::OpregImm && r.mask != 0xf8 {
            return Err(error("opcode register form requires mask f8"));
        }
    }
    for (i, a) in rules.iter().enumerate() {
        for b in &rules[i + 1..] {
            if a.opcode.len() == b.opcode.len()
                && a.opcode[..a.opcode.len() - 1] == b.opcode[..b.opcode.len() - 1]
                && ((a.opcode.last().unwrap() ^ b.opcode.last().unwrap()) & a.mask & b.mask) == 0
                && (a.extension.is_none() || b.extension.is_none() || a.extension == b.extension)
            {
                return Err(error("ambiguous x86 opcode rules"));
            }
        }
    }
    Ok(())
}
pub(crate) fn encode(rules: &[Rule]) -> Result<Vec<u8>, FslError> {
    validate(rules)?;
    let mut out = Vec::new();
    out.extend((rules.len() as u16).to_le_bytes());
    for r in rules {
        for s in [&r.name, &r.body] {
            out.extend((s.len() as u16).to_le_bytes());
            out.extend(s.as_bytes());
        }
        out.push(r.opcode.len() as u8);
        out.extend(&r.opcode);
        out.push(r.mask);
        out.push(r.extension.unwrap_or(255));
        out.push(r.form as u8);
    }
    Ok(out)
}
pub(crate) fn decode_rules(raw: &[u8]) -> Result<Vec<Rule>, FslError> {
    struct Reader<'a> {
        bytes: &'a [u8],
        pos: usize,
    }
    impl<'a> Reader<'a> {
        fn take(&mut self, n: usize) -> Result<&'a [u8], FslError> {
            let end = self
                .pos
                .checked_add(n)
                .ok_or_else(|| error("rule size overflow"))?;
            let x = self
                .bytes
                .get(self.pos..end)
                .ok_or_else(|| error("truncated rule record"))?;
            self.pos = end;
            Ok(x)
        }
        fn byte(&mut self) -> Result<u8, FslError> {
            Ok(self.take(1)?[0])
        }
        fn word(&mut self) -> Result<usize, FslError> {
            Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()) as usize)
        }
        fn text(&mut self) -> Result<String, FslError> {
            let n = self.word()?;
            if n > 128 {
                return Err(error("rule string too large"));
            }
            Ok(std::str::from_utf8(self.take(n)?)
                .map_err(|_| error("rule string UTF-8"))?
                .into())
        }
    }
    let mut rd = Reader { bytes: raw, pos: 0 };
    let count = rd.word()?;
    if count == 0 || count > 256 {
        return Err(error("bad rule count"));
    }
    let mut rules = Vec::new();
    for _ in 0..count {
        let name = rd.text()?;
        let body = rd.text()?;
        let n = usize::from(rd.byte()?);
        if !matches!(n, 1 | 2) {
            return Err(error("bad opcode length"));
        }
        let opcode = rd.take(n)?.to_vec();
        let mask = rd.byte()?;
        let ext = rd.byte()?;
        let tag = rd.byte()?;
        let form = [
            Form::RmReg,
            Form::RegRm,
            Form::RmImm,
            Form::RmSimm8,
            Form::AccImm,
            Form::OpregImm,
            Form::Lea,
            Form::Branch8,
            Form::Branch,
            Form::Ret,
            Form::Nop,
        ]
        .get(usize::from(tag))
        .copied()
        .ok_or_else(|| error("unknown operand form tag"))?;
        rules.push(Rule {
            name,
            body,
            opcode,
            mask,
            extension: if ext == 255 { None } else { Some(ext) },
            form,
        });
    }
    if rd.pos != raw.len() {
        return Err(error("trailing compiled rule bytes"));
    }
    validate(&rules)?;
    Ok(rules)
}

pub(crate) struct Decoded {
    pub rule: usize,
    pub fields: [u64; 6],
    pub width: u32,
    pub address_bits: u32,
    pub length: usize,
}
struct Cursor<'a> {
    input: &'a [u8],
    pos: usize,
    start: usize,
}
impl Cursor<'_> {
    fn byte(&mut self) -> Result<u8, FslError> {
        if self.pos - self.start >= 15 {
            return Err(error("x86 instruction exceeds 15 bytes"));
        }
        let b = *self
            .input
            .get(self.pos)
            .ok_or_else(|| error("truncated x86 operand"))?;
        self.pos += 1;
        Ok(b)
    }
    fn unsigned(&mut self, bits: u32) -> Result<u64, FslError> {
        let mut n = 0;
        for j in 0..bits / 8 {
            n |= u64::from(self.byte()?) << (j * 8);
        }
        Ok(n)
    }
    fn signed(&mut self, bits: u32) -> Result<u64, FslError> {
        let n = self.unsigned(bits)?;
        let sign = 1u64 << (bits - 1);
        Ok((n ^ sign).wrapping_sub(sign))
    }
}
fn mask(bits: u32) -> u64 {
    if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    }
}
pub(crate) fn decode(
    rules: &[Rule],
    input: &[u8],
    offset: usize,
    base: u64,
    mode: u32,
) -> Result<Decoded, FslError> {
    let mut c = Cursor {
        input,
        pos: offset,
        start: offset,
    };
    let mut operand = false;
    let mut address = false;
    let mut rex = 0u8;
    let first = loop {
        let b = c.byte()?;
        match b {
            0x66 => {
                if operand || rex != 0 {
                    return Err(error("repeated/out-of-order operand prefix unsupported"));
                }
                operand = true;
            }
            0x67 => {
                if address || rex != 0 {
                    return Err(error("repeated/out-of-order address prefix unsupported"));
                }
                address = true;
            }
            0x40..=0x4f if mode == 64 => {
                if rex != 0 {
                    return Err(error("multiple REX prefixes unsupported"));
                }
                rex = b;
            }
            0xf0 | 0xf2 | 0xf3 | 0x26 | 0x2e | 0x36 | 0x3e | 0x64 | 0x65 => {
                return Err(error("LOCK/REP/segment prefix semantics not migrated"))
            }
            _ => break b,
        }
    };
    let mut opcode = vec![first];
    if first == 0x0f {
        opcode.push(c.byte()?);
    }
    let modrm_needed = rules.iter().any(|r| {
        r.opcode.len() == opcode.len()
            && r.opcode[..opcode.len() - 1] == opcode[..opcode.len() - 1]
            && (opcode[opcode.len() - 1] & r.mask) == r.opcode[opcode.len() - 1]
            && r.form.has_modrm()
    });
    let modrm = if modrm_needed { Some(c.byte()?) } else { None };
    let hits: Vec<_> = rules
        .iter()
        .enumerate()
        .filter(|(_, r)| {
            r.opcode.len() == opcode.len()
                && r.opcode[..opcode.len() - 1] == opcode[..opcode.len() - 1]
                && (opcode[opcode.len() - 1] & r.mask) == r.opcode[opcode.len() - 1]
                && r.extension
                    .is_none_or(|ext| modrm.is_some_and(|m| (m >> 3) & 7 == ext))
        })
        .collect();
    if hits.len() != 1 {
        return Err(error(format!(
            "no unique migrated opcode rule: {:02x?}",
            opcode
        )));
    }
    let (rule, r) = hits[0];
    let width = if mode == 64 && rex & 8 != 0 {
        64
    } else if operand {
        if mode == 16 {
            32
        } else {
            16
        }
    } else if mode == 64 {
        32
    } else {
        mode
    };
    let address_bits = if address {
        if mode == 16 {
            32
        } else if mode == 32 {
            16
        } else {
            32
        }
    } else {
        mode
    };
    let mut fields = [0u64; 6];
    let mut rip_relative = false;
    let reg_extension = u64::from((rex >> 2) & 1) * 8;
    let rm_extension = u64::from(rex & 1) * 8;
    if let Some(m) = modrm {
        let mod_ = m >> 6;
        let reg = u64::from((m >> 3) & 7) + reg_extension;
        let rm = u64::from(m & 7) + rm_extension;
        if r.extension.is_some() && reg_extension != 0 {
            return Err(error("REX.R on opcode extension not admitted"));
        }
        match r.form {
            Form::RmReg => {
                if mod_ != 3 {
                    return Err(error("memory destination not migrated"));
                }
                fields[0] = rm;
                fields[1] = reg;
            }
            Form::RegRm => {
                if mod_ != 3 {
                    return Err(error("memory source not migrated"));
                }
                fields[0] = reg;
                fields[1] = rm;
            }
            Form::RmImm | Form::RmSimm8 => {
                if mod_ != 3 {
                    return Err(error("memory ALU/immediate form not migrated"));
                }
                fields[0] = rm;
            }
            Form::Lea => {
                if mod_ == 3 {
                    return Err(error("LEA requires an effective address"));
                }
                fields[0] = reg;
                fields[2] = 16;
                fields[3] = 16;
                if address_bits == 16 {
                    let (b, i) = match m & 7 {
                        0 => (3, 6),
                        1 => (3, 7),
                        2 => (5, 6),
                        3 => (5, 7),
                        4 => (6, 16),
                        5 => (7, 16),
                        6 if mod_ == 0 => (16, 16),
                        6 => (5, 16),
                        _ => (3, 16),
                    };
                    fields[2] = b;
                    fields[3] = i;
                    fields[4] = match mod_ {
                        0 if m & 7 == 6 => c.unsigned(16)?,
                        1 => c.signed(8)?,
                        2 => c.signed(16)?,
                        _ => 0,
                    };
                } else {
                    let mut disp32 = false;
                    if m & 7 == 4 {
                        let sib = c.byte()?;
                        fields[5] = u64::from(sib >> 6);
                        let idx = (sib >> 3) & 7;
                        if idx != 4 || rex & 2 != 0 {
                            fields[3] = u64::from(idx) + u64::from((rex >> 1) & 1) * 8;
                        }
                        if sib & 7 == 5 && mod_ == 0 {
                            disp32 = true;
                        } else {
                            fields[2] = u64::from(sib & 7) + rm_extension;
                        }
                    } else if m & 7 == 5 && mod_ == 0 {
                        disp32 = true;
                        rip_relative = mode == 64;
                    } else {
                        fields[2] = rm;
                    }
                    fields[4] = match mod_ {
                        0 if disp32 => {
                            if address_bits == 64 {
                                c.signed(32)?
                            } else {
                                c.unsigned(32)?
                            }
                        }
                        1 => c.signed(8)?,
                        2 => c.signed(32)?,
                        _ => 0,
                    };
                }
            }
            _ => return Err(error("unexpected ModRM operand form")),
        }
    }
    match r.form {
        Form::RmImm | Form::AccImm => {
            fields[4] = if width == 64 {
                c.signed(32)?
            } else {
                c.unsigned(width)?
            };
        }
        Form::RmSimm8 => fields[4] = c.signed(8)?,
        Form::OpregImm => {
            fields[0] = u64::from(opcode[opcode.len() - 1] & 7) + rm_extension;
            fields[4] = c.unsigned(width)?;
        }
        Form::Branch | Form::Branch8 => {
            if operand || address || rex != 0 {
                return Err(error("prefixed guest branches not migrated"));
            }
            let displacement = if r.form == Form::Branch8 {
                c.signed(8)?
            } else {
                c.signed(if mode == 16 { 16 } else { 32 })?
            };
            fields[4] = base.wrapping_add(c.pos as u64).wrapping_add(displacement) & mask(mode);
        }
        Form::Ret => {
            if operand || address || rex != 0 {
                return Err(error("prefixed RET requires a separate stack contract"));
            }
            fields[0] = 4;
        }
        Form::Nop => {
            if rex & 7 != 0 {
                return Err(error("extended XCHG is not a NOP"));
            }
            fields[4] = base + c.pos as u64;
        }
        _ => {}
    }
    if rip_relative {
        fields[4] = base.wrapping_add(c.pos as u64).wrapping_add(fields[4]) & mask(address_bits);
    }
    if mode != 64 && (fields[..4].iter().any(|&r| r > 7 && r != 16)) {
        return Err(error("extended register requires long mode"));
    }
    Ok(Decoded {
        rule,
        fields,
        width: if r.form == Form::Ret { mode } else { width },
        address_bits,
        length: c.pos - offset,
    })
}
