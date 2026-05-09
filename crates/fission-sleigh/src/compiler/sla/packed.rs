use std::collections::BTreeMap;

use anyhow::{anyhow, bail, Context, Result};

pub struct PackedElement {
    pub id: u32,
    pub attrs: BTreeMap<u32, PackedAttrValue>,
    pub children: Vec<PackedElement>,
}

impl PackedElement {
    pub fn descendants_with_id(&self, id: u32) -> Vec<&PackedElement> {
        let mut out = Vec::new();
        self.collect_descendants(id, &mut out);
        out
    }

    fn collect_descendants<'a>(&'a self, id: u32, out: &mut Vec<&'a PackedElement>) {
        for child in &self.children {
            if child.id == id {
                out.push(child);
            }
            child.collect_descendants(id, out);
        }
    }

    pub fn attr_unsigned(&self, id: u32) -> Option<u64> {
        match self.attrs.get(&id) {
            Some(PackedAttrValue::Unsigned(value)) => Some(*value),
            Some(PackedAttrValue::Signed(value)) if *value >= 0 => Some(*value as u64),
            Some(PackedAttrValue::SpaceIndex(value)) => Some(*value),
            _ => None,
        }
    }

    pub fn attr_signed(&self, id: u32) -> Option<i64> {
        match self.attrs.get(&id) {
            Some(PackedAttrValue::Signed(value)) => Some(*value),
            Some(PackedAttrValue::Unsigned(value)) => i64::try_from(*value).ok(),
            _ => None,
        }
    }

    pub fn attr_bool_value(&self, id: u32) -> Option<bool> {
        match self.attrs.get(&id) {
            Some(PackedAttrValue::Bool(value)) => Some(*value),
            _ => None,
        }
    }

    pub fn attr_space_index(&self, id: u32) -> Option<u64> {
        match self.attrs.get(&id) {
            Some(PackedAttrValue::SpaceIndex(value)) => Some(*value),
            _ => None,
        }
    }

    pub fn attr_string(&self, id: u32) -> Option<&str> {
        match self.attrs.get(&id) {
            Some(PackedAttrValue::String(value)) => Some(value.as_str()),
            _ => None,
        }
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn children(&self) -> &[PackedElement] {
        &self.children
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackedAttrValue {
    Bool(bool),
    Signed(i64),
    Unsigned(u64),
    SpaceIndex(u64),
    SpecialSpace(()),
    String(String),
}

pub struct PackedParser<'a> {
    pub bytes: &'a [u8],
    pub offset: usize,
}

impl<'a> PackedParser<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    pub fn parse_root(&mut self) -> Result<PackedElement> {
        self.parse_element()
    }

    fn parse_element(&mut self) -> Result<PackedElement> {
        let (kind, id) = self.read_header()?;
        if kind != packed::ELEMENT_START {
            bail!(
                "expected element start, found header kind {kind:#x} at offset {}",
                self.offset - 1
            );
        }
        let mut attrs = BTreeMap::new();
        loop {
            let Some(kind) = self.peek_header_kind() else {
                bail!("unterminated element {id}");
            };
            if kind != packed::ATTRIBUTE {
                break;
            }
            let (attr_id, value) = self.read_attribute()?;
            attrs.insert(attr_id, value);
        }
        let mut children = Vec::new();
        loop {
            let Some(kind) = self.peek_header_kind() else {
                bail!("unterminated element {id}");
            };
            match kind {
                packed::ELEMENT_START => children.push(self.parse_element()?),
                packed::ELEMENT_END => {
                    let (_, end_id) = self.read_header()?;
                    if end_id != id {
                        bail!("element end id {end_id} did not match start id {id}");
                    }
                    break;
                }
                packed::ATTRIBUTE => bail!("attribute appeared after child in element {id}"),
                other => bail!(
                    "unsupported packed header kind {other:#x} at offset {}",
                    self.offset
                ),
            }
        }
        Ok(PackedElement {
            id,
            attrs,
            children,
        })
    }

    fn read_attribute(&mut self) -> Result<(u32, PackedAttrValue)> {
        let (kind, id) = self.read_header()?;
        if kind != packed::ATTRIBUTE {
            bail!("expected attribute header, found {kind:#x}");
        }
        let type_byte = self.next_byte()?;
        let attr_type = type_byte >> packed::TYPECODE_SHIFT;
        let len = (type_byte & packed::LENGTHCODE_MASK) as usize;
        let value = match attr_type {
            packed::TYPECODE_BOOLEAN => PackedAttrValue::Bool(len != 0),
            packed::TYPECODE_SIGNEDINT_POSITIVE => {
                PackedAttrValue::Signed(self.read_integer(len)? as i64)
            }
            packed::TYPECODE_SIGNEDINT_NEGATIVE => {
                PackedAttrValue::Signed(-(self.read_integer(len)? as i64))
            }
            packed::TYPECODE_UNSIGNEDINT => PackedAttrValue::Unsigned(self.read_integer(len)?),
            packed::TYPECODE_ADDRESSSPACE => PackedAttrValue::SpaceIndex(self.read_integer(len)?),
            packed::TYPECODE_SPECIALSPACE => PackedAttrValue::SpecialSpace(()),
            packed::TYPECODE_STRING => {
                let str_len = self.read_integer(len)? as usize;
                let end = self.offset + str_len;
                if end > self.bytes.len() {
                    bail!("string attribute overflow");
                }
                let s = std::str::from_utf8(&self.bytes[self.offset..end])
                    .context("string attribute not UTF-8")?
                    .to_string();
                self.offset = end;
                PackedAttrValue::String(s)
            }
            other => bail!("unsupported packed attribute type {other}"),
        };
        Ok((id, value))
    }

    fn read_header(&mut self) -> Result<(u8, u32)> {
        let header = self.next_byte()?;
        let kind = header & packed::HEADER_MASK;
        let mut id = (header & packed::ELEMENTID_MASK) as u32;
        if header & packed::HEADEREXTEND_MASK != 0 {
            id <<= packed::RAWDATA_BITSPERBYTE;
            id |= (self.next_byte()? & packed::RAWDATA_MASK) as u32;
        }
        Ok((kind, id))
    }

    fn peek_header_kind(&self) -> Option<u8> {
        self.bytes
            .get(self.offset)
            .map(|byte| byte & packed::HEADER_MASK)
    }

    fn read_integer(&mut self, len: usize) -> Result<u64> {
        let mut value = 0u64;
        for _ in 0..len {
            value <<= packed::RAWDATA_BITSPERBYTE;
            value |= (self.next_byte()? & packed::RAWDATA_MASK) as u64;
        }
        Ok(value)
    }

    fn next_byte(&mut self) -> Result<u8> {
        let Some(byte) = self.bytes.get(self.offset).copied() else {
            bail!("unexpected end of packed SLEIGH payload");
        };
        self.offset += 1;
        Ok(byte)
    }
}

pub mod packed {
    pub const HEADER_MASK: u8 = 0xc0;
    pub const ELEMENT_START: u8 = 0x40;
    pub const ELEMENT_END: u8 = 0x80;
    pub const ATTRIBUTE: u8 = 0xc0;
    pub const HEADEREXTEND_MASK: u8 = 0x20;
    pub const ELEMENTID_MASK: u8 = 0x1f;
    pub const RAWDATA_MASK: u8 = 0x7f;
    pub const RAWDATA_BITSPERBYTE: u32 = 7;
    pub const TYPECODE_SHIFT: u8 = 4;
    pub const LENGTHCODE_MASK: u8 = 0x0f;
    pub const TYPECODE_BOOLEAN: u8 = 1;
    pub const TYPECODE_SIGNEDINT_POSITIVE: u8 = 2;
    pub const TYPECODE_SIGNEDINT_NEGATIVE: u8 = 3;
    pub const TYPECODE_UNSIGNEDINT: u8 = 4;
    pub const TYPECODE_ADDRESSSPACE: u8 = 5;
    pub const TYPECODE_SPECIALSPACE: u8 = 6;
    pub const TYPECODE_STRING: u8 = 7;
}

pub mod sla_format {
    pub const ELEM_CONST_REAL: u32 = 1;
    pub const ELEM_VARNODE_TPL: u32 = 2;
    pub const ELEM_CONST_SPACEID: u32 = 3;
    pub const ELEM_CONST_HANDLE: u32 = 4;
    pub const ELEM_OP_TPL: u32 = 5;
    pub const ELEM_MASK_WORD: u32 = 6;
    pub const ELEM_PAT_BLOCK: u32 = 7;
    pub const ELEM_PRINT: u32 = 8;
    pub const ELEM_PAIR: u32 = 9;
    pub const ELEM_CONTEXT_PAT: u32 = 10;
    pub const ELEM_NULL: u32 = 11;
    pub const ELEM_OPERAND_EXP: u32 = 12;
    pub const ELEM_OPERAND_SYM: u32 = 13;
    pub const ELEM_OPER: u32 = 15;
    pub const ELEM_DECISION: u32 = 16;
    pub const ELEM_OPPRINT: u32 = 17;
    pub const ELEM_INSTRUCT_PAT: u32 = 18;
    pub const ELEM_COMBINE_PAT: u32 = 19;
    pub const ELEM_CONSTRUCTOR: u32 = 20;
    pub const ELEM_CONSTRUCT_TPL: u32 = 21;
    pub const ELEM_VARNODE_SYM: u32 = 23;
    pub const ELEM_VAR: u32 = 28;
    pub const ELEM_CONTEXTFIELD: u32 = 29;
    pub const ELEM_TOKENFIELD: u32 = 27;
    pub const ELEM_HANDLE_TPL: u32 = 30;
    pub const ELEM_CONST_RELATIVE: u32 = 31;
    pub const ELEM_CONTEXT_OP: u32 = 32;
    pub const ELEM_SLEIGH: u32 = 33;
    pub const ELEM_SPACES: u32 = 34;
    pub const ELEM_SOURCEFILES: u32 = 35;
    pub const ELEM_SOURCEFILE: u32 = 36;
    pub const ELEM_SPACE: u32 = 37;
    pub const ELEM_SYMBOL_TABLE: u32 = 38;
    pub const ELEM_VALUE_SYM: u32 = 39;
    pub const ELEM_CONTEXT_SYM: u32 = 41;
    pub const ELEM_SPACE_UNIQUE: u32 = 46;
    pub const ELEM_NAME_SYM: u32 = 64;
    pub const ELEM_NAMETAB: u32 = 66;
    pub const ELEM_SUBTABLE_SYM: u32 = 71;
    pub const ELEM_SUBTABLE_SYM_HEAD: u32 = 72;
    pub const ELEM_VALUEMAP_SYM: u32 = 73;
    pub const ELEM_VALUETAB: u32 = 75;
    pub const ELEM_VARLIST_SYM: u32 = 76;
    pub const ELEM_OR_PAT: u32 = 78;
    pub const ELEM_VARNODE_SYM_HEAD: u32 = 24;
    pub const ELEM_USEROP_HEAD: u32 = 26;
    pub const ELEM_CONTEXT_SYM_HEAD: u32 = 42;
    pub const ELEM_AND_EXP: u32 = 47;
    pub const ELEM_DIV_EXP: u32 = 48;
    pub const ELEM_LSHIFT_EXP: u32 = 49;
    pub const ELEM_MINUS_EXP: u32 = 50;
    pub const ELEM_MULT_EXP: u32 = 51;
    pub const ELEM_NOT_EXP: u32 = 52;
    pub const ELEM_OR_EXP: u32 = 53;
    pub const ELEM_PLUS_EXP: u32 = 54;
    pub const ELEM_RSHIFT_EXP: u32 = 55;
    pub const ELEM_SUB_EXP: u32 = 56;
    pub const ELEM_XOR_EXP: u32 = 57;
    pub const ELEM_INTB: u32 = 58;
    pub const ELEM_END_EXP: u32 = 59;
    pub const ELEM_NEXT2_EXP: u32 = 60;
    pub const ELEM_START_EXP: u32 = 61;
    pub const ELEM_COMMIT: u32 = 79;
    pub const ELEM_CONST_START: u32 = 80;
    pub const ELEM_CONST_NEXT: u32 = 81;
    pub const ELEM_CONST_NEXT2: u32 = 82;
    pub const ELEM_CONST_CURSPACE: u32 = 83;
    pub const ELEM_CONST_CURSPACE_SIZE: u32 = 84;
    pub const ELEM_CONST_FLOWREF: u32 = 85;
    pub const ELEM_CONST_FLOWREF_SIZE: u32 = 86;
    pub const ELEM_CONST_FLOWDEST: u32 = 87;
    pub const ELEM_CONST_FLOWDEST_SIZE: u32 = 88;

    pub const ATTR_VAL: u32 = 2;
    pub const ATTR_ID: u32 = 3;
    pub const ATTR_SPACE: u32 = 4;
    pub const ATTR_S: u32 = 5;
    pub const ATTR_OFF: u32 = 6;
    pub const ATTR_CODE: u32 = 7;
    pub const ATTR_MASK: u32 = 8;
    pub const ATTR_INDEX: u32 = 9;
    pub const ATTR_NONZERO: u32 = 10;
    pub const ATTR_PIECE: u32 = 11;
    pub const ATTR_NAME: u32 = 12;
    pub const ATTR_SCOPE: u32 = 13;
    pub const ATTR_STARTBIT: u32 = 14;
    pub const ATTR_SIZE: u32 = 15;
    pub const ATTR_TABLE: u32 = 16;
    pub const ATTR_CT: u32 = 17;
    pub const ATTR_MINLEN: u32 = 18;
    pub const ATTR_BASE: u32 = 19;
    pub const ATTR_NUMBER: u32 = 20;
    pub const ATTR_CONTEXT: u32 = 21;
    pub const ATTR_PARENT: u32 = 22;
    pub const ATTR_SUBSYM: u32 = 23;
    pub const ATTR_LINE: u32 = 24;
    pub const ATTR_SOURCE: u32 = 25;
    pub const ATTR_LENGTH: u32 = 26;
    pub const ATTR_FIRST: u32 = 27;
    pub const ATTR_PLUS: u32 = 28;
    pub const ATTR_SHIFT: u32 = 29;
    pub const ATTR_ENDBIT: u32 = 30;
    pub const ATTR_SIGNBIT: u32 = 31;
    pub const ATTR_ENDBYTE: u32 = 32;
    pub const ATTR_STARTBYTE: u32 = 33;
    pub const ATTR_BIGENDIAN: u32 = 35;
    pub const ATTR_ALIGN: u32 = 36;
    pub const ATTR_UNIQBASE: u32 = 37;
    pub const ATTR_UNIQMASK: u32 = 39;
    pub const ATTR_WORDSIZE: u32 = 43;
    pub const ATTR_I: u32 = 52;
    pub const ATTR_SECTION: u32 = 54;
    pub const ATTR_LABELS: u32 = 55;
}
