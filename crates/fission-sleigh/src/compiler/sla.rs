use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use fission_pcode::PcodeOpcode;
use flate2::read::ZlibDecoder;

use super::ir::{
    CompiledConstTpl, CompiledConstructTpl, CompiledHandleSelector, CompiledHandleTpl,
    CompiledLabelRef, CompiledOpTpl, CompiledOpTplOpcode, CompiledSpaceRef, CompiledSpaceTpl,
    CompiledTemplateSource, CompiledVarnodeTpl,
};

pub const GHIDRA_SLA_MAGIC: &[u8; 3] = b"sla";
pub const GHIDRA_SLA_FORMAT_VERSION: u8 = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledSlaArtifact {
    pub path: PathBuf,
    pub version: u8,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledSlaTemplateLibrary {
    pub path: PathBuf,
    pub version: u8,
    pub source_files: BTreeMap<u64, String>,
    pub spaces: BTreeMap<u64, CompiledSpaceRef>,
    pub constructors_by_source: BTreeMap<String, Vec<CompiledSlaConstructorTemplate>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledSlaConstructorTemplate {
    pub source_key: String,
    pub source_file: String,
    pub line: u64,
    pub opprint_indices: Vec<usize>,
    pub constructor_template: CompiledConstructTpl,
}

pub fn load_compiled_sla(path: impl AsRef<Path>) -> Result<CompiledSlaArtifact> {
    let path = path.as_ref();
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read compiled SLEIGH artifact {path:?}"))?;
    decode_compiled_sla(path.to_path_buf(), &bytes)
}

pub fn load_construct_templates_from_sla(
    path: impl AsRef<Path>,
) -> Result<CompiledSlaTemplateLibrary> {
    let artifact = load_compiled_sla(path)?;
    decode_construct_templates(&artifact)
}

fn decode_compiled_sla(path: PathBuf, bytes: &[u8]) -> Result<CompiledSlaArtifact> {
    if bytes.len() < 5 {
        return Err(anyhow!("compiled SLEIGH artifact is too short: {path:?}"));
    }
    if &bytes[..3] != GHIDRA_SLA_MAGIC {
        return Err(anyhow!(
            "compiled SLEIGH artifact missing sla magic: {path:?}"
        ));
    }
    let version = bytes[3];
    let mut decoder = ZlibDecoder::new(&bytes[4..]);
    let mut payload = Vec::new();
    decoder
        .read_to_end(&mut payload)
        .with_context(|| format!("failed to decompress compiled SLEIGH payload {path:?}"))?;
    if payload.is_empty() {
        return Err(anyhow!("compiled SLEIGH payload is empty: {path:?}"));
    }
    Ok(CompiledSlaArtifact {
        path,
        version,
        payload,
    })
}

fn decode_construct_templates(
    artifact: &CompiledSlaArtifact,
) -> Result<CompiledSlaTemplateLibrary> {
    if artifact.version != GHIDRA_SLA_FORMAT_VERSION {
        bail!(
            "unsupported SLEIGH format version {} in {}",
            artifact.version,
            artifact.path.display()
        );
    }
    let mut parser = PackedParser::new(&artifact.payload);
    let root = parser.parse_root()?;
    if root.id != sla_format::ELEM_SLEIGH {
        bail!(
            "compiled SLEIGH root element was {}, expected sleigh",
            root.id
        );
    }

    let source_files = decode_source_files(&root)?;
    let spaces = decode_spaces(&root)?;
    let mut constructors_by_source: BTreeMap<String, Vec<CompiledSlaConstructorTemplate>> =
        BTreeMap::new();
    for constructor in root.descendants_with_id(sla_format::ELEM_CONSTRUCTOR) {
        let Some(source_index) = constructor.attr_unsigned(sla_format::ATTR_SOURCE) else {
            continue;
        };
        let Some(line) = constructor.attr_unsigned(sla_format::ATTR_LINE) else {
            continue;
        };
        let Some(source_file) = source_files.get(&source_index).cloned() else {
            continue;
        };
        let source_key = format!("{}:{line}", basename(&source_file));
        let Some(main_tpl) = constructor
            .children
            .iter()
            .filter(|child| child.id == sla_format::ELEM_CONSTRUCT_TPL)
            .find(|child| child.attr_unsigned(sla_format::ATTR_SECTION).is_none())
        else {
            continue;
        };
        let template = decode_construct_tpl(main_tpl, &spaces)
            .with_context(|| format!("decode construct_tpl for {source_key}"))?;
            
        let mut opprint_indices = Vec::new();
        for child in &constructor.children {
            if child.id == sla_format::ELEM_OPPRINT {
                if let Some(index) = child.attr_signed(sla_format::ATTR_ID).map(|x| x as usize) {
                    opprint_indices.push(index);
                }
            }
        }

        constructors_by_source
            .entry(source_key.clone())
            .or_default()
            .push(CompiledSlaConstructorTemplate {
                source_key,
                source_file,
                line,
                opprint_indices,
                constructor_template: template,
            });
    }

    Ok(CompiledSlaTemplateLibrary {
        path: artifact.path.clone(),
        version: artifact.version,
        source_files,
        spaces,
        constructors_by_source,
    })
}

fn basename(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

fn decode_source_files(root: &PackedElement) -> Result<BTreeMap<u64, String>> {
    let mut out = BTreeMap::new();
    for source in root.descendants_with_id(sla_format::ELEM_SOURCEFILE) {
        let index = source
            .attr_unsigned(sla_format::ATTR_INDEX)
            .ok_or_else(|| anyhow!("sourcefile missing index"))?;
        let name = source
            .attr_string(sla_format::ATTR_NAME)
            .ok_or_else(|| anyhow!("sourcefile missing name"))?;
        out.insert(index, name.to_string());
    }
    Ok(out)
}

fn decode_spaces(root: &PackedElement) -> Result<BTreeMap<u64, CompiledSpaceRef>> {
    let mut out = BTreeMap::new();
    out.insert(
        0,
        CompiledSpaceRef {
            name: "const".to_string(),
            index: 0,
        },
    );
    for space in root
        .descendants_with_id(sla_format::ELEM_SPACE)
        .into_iter()
        .chain(root.descendants_with_id(sla_format::ELEM_SPACE_UNIQUE))
    {
        let index = space
            .attr_unsigned(sla_format::ATTR_INDEX)
            .ok_or_else(|| anyhow!("space missing index"))?;
        let name = space
            .attr_string(sla_format::ATTR_NAME)
            .ok_or_else(|| anyhow!("space missing name"))?;
        out.insert(
            index,
            CompiledSpaceRef {
                name: name.to_string(),
                index,
            },
        );
    }
    Ok(out)
}

fn decode_construct_tpl(
    element: &PackedElement,
    spaces: &BTreeMap<u64, CompiledSpaceRef>,
) -> Result<CompiledConstructTpl> {
    let mut children = element.children.iter();
    let _result = match children.next() {
        Some(child) if child.id == sla_format::ELEM_NULL => None,
        Some(child) if child.id == sla_format::ELEM_HANDLE_TPL => {
            Some(decode_handle_tpl(child, spaces)?)
        }
        Some(child) => bail!("construct_tpl result is unexpected element {}", child.id),
        None => None,
    };
    let mut op_templates = Vec::new();
    for child in children {
        if child.id == sla_format::ELEM_OP_TPL {
            op_templates.push(decode_op_tpl(child, spaces)?);
        }
    }
    Ok(CompiledConstructTpl {
        constructor_hash: 0,
        ops: Vec::new(),
        op_templates,
        template_source: CompiledTemplateSource::SpecDerived,
    })
}

fn decode_op_tpl(
    element: &PackedElement,
    spaces: &BTreeMap<u64, CompiledSpaceRef>,
) -> Result<CompiledOpTpl> {
    let opcode_code = element
        .attr_unsigned(sla_format::ATTR_CODE)
        .ok_or_else(|| anyhow!("op_tpl missing opcode"))?;
    let opcode = map_pcode_opcode(opcode_code as u32);
    let mut children = element.children.iter();
    let output = match children.next() {
        Some(child) if child.id == sla_format::ELEM_NULL => None,
        Some(child) if child.id == sla_format::ELEM_VARNODE_TPL => {
            Some(decode_varnode_tpl(child, spaces)?)
        }
        Some(child) => bail!("op_tpl output is unexpected element {}", child.id),
        None => None,
    };
    let mut inputs = Vec::new();
    for child in children {
        if child.id == sla_format::ELEM_VARNODE_TPL {
            inputs.push(decode_varnode_tpl(child, spaces)?);
        } else {
            bail!("op_tpl input is unexpected element {}", child.id);
        }
    }
    Ok(CompiledOpTpl {
        opcode,
        output,
        inputs,
        label: if matches!(opcode, CompiledOpTplOpcode::Label) {
            Some(CompiledLabelRef {
                name: format!("label_{opcode_code}"),
            })
        } else {
            None
        },
    })
}

fn map_pcode_opcode(code: u32) -> CompiledOpTplOpcode {
    match PcodeOpcode::from_flat_u32(code) {
        PcodeOpcode::Copy => CompiledOpTplOpcode::Copy,
        PcodeOpcode::Load => CompiledOpTplOpcode::Load,
        PcodeOpcode::Store => CompiledOpTplOpcode::Store,
        PcodeOpcode::Branch => CompiledOpTplOpcode::Branch,
        PcodeOpcode::CBranch => CompiledOpTplOpcode::CBranch,
        PcodeOpcode::Call => CompiledOpTplOpcode::Call,
        PcodeOpcode::CallOther => CompiledOpTplOpcode::CallOther,
        PcodeOpcode::Return => CompiledOpTplOpcode::Return,
        PcodeOpcode::IntEqual => CompiledOpTplOpcode::IntEqual,
        PcodeOpcode::IntNotEqual => CompiledOpTplOpcode::IntNotEqual,
        PcodeOpcode::IntSLess => CompiledOpTplOpcode::IntSLess,
        PcodeOpcode::IntLess => CompiledOpTplOpcode::IntLess,
        PcodeOpcode::IntZExt => CompiledOpTplOpcode::IntZExt,
        PcodeOpcode::IntSExt => CompiledOpTplOpcode::IntSExt,
        PcodeOpcode::IntAdd => CompiledOpTplOpcode::IntAdd,
        PcodeOpcode::IntSub => CompiledOpTplOpcode::IntSub,
        PcodeOpcode::IntCarry => CompiledOpTplOpcode::IntCarry,
        PcodeOpcode::IntSCarry => CompiledOpTplOpcode::IntSCarry,
        PcodeOpcode::IntSBorrow => CompiledOpTplOpcode::IntSBorrow,
        PcodeOpcode::IntXor => CompiledOpTplOpcode::IntXor,
        PcodeOpcode::IntAnd => CompiledOpTplOpcode::IntAnd,
        PcodeOpcode::IntOr => CompiledOpTplOpcode::IntOr,
        PcodeOpcode::IntLeft => CompiledOpTplOpcode::IntLeft,
        PcodeOpcode::IntRight => CompiledOpTplOpcode::IntRight,
        PcodeOpcode::IntSRight => CompiledOpTplOpcode::IntSRight,
        PcodeOpcode::IntMult => CompiledOpTplOpcode::IntMult,
        PcodeOpcode::BoolNegate => CompiledOpTplOpcode::BoolNegate,
        PcodeOpcode::BoolAnd => CompiledOpTplOpcode::BoolAnd,
        PcodeOpcode::BoolOr => CompiledOpTplOpcode::BoolOr,
        PcodeOpcode::MultiEqual => CompiledOpTplOpcode::Build,
        PcodeOpcode::Piece => CompiledOpTplOpcode::Piece,
        PcodeOpcode::SubPiece => CompiledOpTplOpcode::Subpiece,
        PcodeOpcode::PtrAdd => CompiledOpTplOpcode::Label,
        PcodeOpcode::PopCount => CompiledOpTplOpcode::PopCount,
        _ => CompiledOpTplOpcode::Unsupported,
    }
}

fn decode_varnode_tpl(
    element: &PackedElement,
    spaces: &BTreeMap<u64, CompiledSpaceRef>,
) -> Result<CompiledVarnodeTpl> {
    if element.children.len() != 3 {
        bail!("varnode_tpl expected 3 const_tpl children");
    }
    let space = decode_space_tpl(&element.children[0], spaces)?;
    let offset = decode_const_tpl(&element.children[1], spaces)?;
    let size = decode_const_tpl(&element.children[2], spaces)?;
    Ok(CompiledVarnodeTpl::Varnode {
        space,
        offset: Box::new(offset),
        size: Box::new(size),
    })
}

fn decode_space_tpl(
    element: &PackedElement,
    spaces: &BTreeMap<u64, CompiledSpaceRef>,
) -> Result<CompiledSpaceTpl> {
    match element.id {
        sla_format::ELEM_CONST_SPACEID => Ok(CompiledSpaceTpl::SpaceRef(decode_space_ref(
            element, spaces,
        )?)),
        _ => Ok(CompiledSpaceTpl::Const(Box::new(decode_const_tpl(
            element, spaces,
        )?))),
    }
}

fn decode_handle_tpl(
    element: &PackedElement,
    spaces: &BTreeMap<u64, CompiledSpaceRef>,
) -> Result<CompiledHandleTpl> {
    if element.children.len() != 7 {
        bail!("handle_tpl expected 7 const_tpl children");
    }
    Ok(CompiledHandleTpl {
        space: Some(decode_space_tpl(&element.children[0], spaces)?),
        size: Some(decode_const_tpl(&element.children[1], spaces)?),
        ptr_space: Some(decode_space_tpl(&element.children[2], spaces)?),
        ptr_offset: Some(decode_const_tpl(&element.children[3], spaces)?),
        ptr_size: Some(decode_const_tpl(&element.children[4], spaces)?),
        temp_space: Some(decode_space_tpl(&element.children[5], spaces)?),
        temp_offset: Some(decode_const_tpl(&element.children[6], spaces)?),
    })
}

fn decode_const_tpl(
    element: &PackedElement,
    spaces: &BTreeMap<u64, CompiledSpaceRef>,
) -> Result<CompiledConstTpl> {
    match element.id {
        sla_format::ELEM_CONST_REAL => Ok(CompiledConstTpl::Real {
            value: element
                .attr_unsigned(sla_format::ATTR_VAL)
                .ok_or_else(|| anyhow!("const_real missing value"))?,
        }),
        sla_format::ELEM_CONST_HANDLE => {
            let handle_index = element
                .attr_signed(sla_format::ATTR_VAL)
                .ok_or_else(|| anyhow!("const_handle missing handle index"))?;
            let selector_code = element
                .attr_signed(sla_format::ATTR_S)
                .ok_or_else(|| anyhow!("const_handle missing selector"))?;
            let selector = match selector_code {
                0 => CompiledHandleSelector::Space,
                1 => CompiledHandleSelector::Offset,
                2 => CompiledHandleSelector::Size,
                3 => CompiledHandleSelector::OffsetPlus,
                other => bail!("unsupported const_handle selector {other}"),
            };
            Ok(CompiledConstTpl::Handle {
                handle_index,
                selector,
                plus: element.attr_unsigned(sla_format::ATTR_PLUS),
            })
        }
        sla_format::ELEM_CONST_SPACEID => Ok(CompiledConstTpl::SpaceId(decode_space_ref(
            element, spaces,
        )?)),
        sla_format::ELEM_CONST_RELATIVE => Ok(CompiledConstTpl::Relative {
            value: element
                .attr_unsigned(sla_format::ATTR_VAL)
                .ok_or_else(|| anyhow!("const_relative missing value"))?,
        }),
        sla_format::ELEM_CONST_START => Ok(CompiledConstTpl::InstStart),
        sla_format::ELEM_CONST_NEXT => Ok(CompiledConstTpl::InstNext),
        sla_format::ELEM_CONST_NEXT2 => Ok(CompiledConstTpl::InstNext2),
        sla_format::ELEM_CONST_CURSPACE => Ok(CompiledConstTpl::CurSpace),
        sla_format::ELEM_CONST_CURSPACE_SIZE => Ok(CompiledConstTpl::CurSpaceSize),
        sla_format::ELEM_CONST_FLOWREF => Ok(CompiledConstTpl::FlowRef),
        sla_format::ELEM_CONST_FLOWREF_SIZE => Ok(CompiledConstTpl::FlowRefSize),
        sla_format::ELEM_CONST_FLOWDEST => Ok(CompiledConstTpl::FlowDest),
        sla_format::ELEM_CONST_FLOWDEST_SIZE => Ok(CompiledConstTpl::FlowDestSize),
        other => bail!("unsupported ConstTpl element {other}"),
    }
}

fn decode_space_ref(
    element: &PackedElement,
    spaces: &BTreeMap<u64, CompiledSpaceRef>,
) -> Result<CompiledSpaceRef> {
    let index = element
        .attr_space_index(sla_format::ATTR_SPACE)
        .or_else(|| element.attr_unsigned(sla_format::ATTR_SPACE))
        .ok_or_else(|| anyhow!("spaceid missing space attribute"))?;
    spaces
        .get(&index)
        .cloned()
        .ok_or_else(|| anyhow!("unknown space index {index}"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PackedElement {
    id: u32,
    attrs: BTreeMap<u32, PackedAttrValue>,
    children: Vec<PackedElement>,
}

impl PackedElement {
    fn descendants_with_id(&self, id: u32) -> Vec<&PackedElement> {
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

    fn attr_unsigned(&self, id: u32) -> Option<u64> {
        match self.attrs.get(&id) {
            Some(PackedAttrValue::Unsigned(value)) => Some(*value),
            Some(PackedAttrValue::Signed(value)) if *value >= 0 => Some(*value as u64),
            Some(PackedAttrValue::SpaceIndex(value)) => Some(*value),
            _ => None,
        }
    }

    fn attr_signed(&self, id: u32) -> Option<i64> {
        match self.attrs.get(&id) {
            Some(PackedAttrValue::Signed(value)) => Some(*value),
            Some(PackedAttrValue::Unsigned(value)) => i64::try_from(*value).ok(),
            _ => None,
        }
    }

    fn attr_space_index(&self, id: u32) -> Option<u64> {
        match self.attrs.get(&id) {
            Some(PackedAttrValue::SpaceIndex(value)) => Some(*value),
            _ => None,
        }
    }

    fn attr_string(&self, id: u32) -> Option<&str> {
        match self.attrs.get(&id) {
            Some(PackedAttrValue::String(value)) => Some(value.as_str()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PackedAttrValue {
    Bool(()),
    Signed(i64),
    Unsigned(u64),
    SpaceIndex(u64),
    SpecialSpace(()),
    String(String),
}

struct PackedParser<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> PackedParser<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn parse_root(&mut self) -> Result<PackedElement> {
        let root = self.parse_element()?;
        Ok(root)
    }

    fn parse_element(&mut self) -> Result<PackedElement> {
        let (kind, id) = self.read_header()?;
        if kind != packed::ELEMENT_START {
            bail!("expected element start, found header kind {kind:#x}");
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
                other => bail!("unsupported packed header kind {other:#x}"),
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
            packed::TYPECODE_BOOLEAN => PackedAttrValue::Bool(()),
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
                let end = self
                    .offset
                    .checked_add(str_len)
                    .ok_or_else(|| anyhow!("packed string length overflow"))?;
                if end > self.bytes.len() {
                    bail!("packed string extends past end of payload");
                }
                let value = std::str::from_utf8(&self.bytes[self.offset..end])
                    .context("packed string is not UTF-8")?
                    .to_string();
                self.offset = end;
                PackedAttrValue::String(value)
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

mod packed {
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

mod sla_format {
    pub const ELEM_CONST_REAL: u32 = 1;
    pub const ELEM_VARNODE_TPL: u32 = 2;
    pub const ELEM_CONST_SPACEID: u32 = 3;
    pub const ELEM_CONST_HANDLE: u32 = 4;
    pub const ELEM_OP_TPL: u32 = 5;
    pub const ELEM_NULL: u32 = 11;
    pub const ELEM_OPPRINT: u32 = 17;
    pub const ELEM_CONSTRUCTOR: u32 = 20;
    pub const ELEM_CONSTRUCT_TPL: u32 = 21;
    pub const ELEM_HANDLE_TPL: u32 = 30;
    pub const ELEM_CONST_RELATIVE: u32 = 31;
    pub const ELEM_SPACES: u32 = 34;
    pub const ELEM_SOURCEFILES: u32 = 35;
    pub const ELEM_SOURCEFILE: u32 = 36;
    pub const ELEM_SPACE: u32 = 37;
    pub const ELEM_SLEIGH: u32 = 33;
    pub const ELEM_SPACE_UNIQUE: u32 = 46;
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
    pub const ATTR_CODE: u32 = 7;
    pub const ATTR_INDEX: u32 = 9;
    pub const ATTR_NAME: u32 = 12;
    pub const ATTR_PARENT: u32 = 22;
    pub const ATTR_LINE: u32 = 24;
    pub const ATTR_SOURCE: u32 = 25;
    pub const ATTR_PLUS: u32 = 28;
    pub const ATTR_SECTION: u32 = 54;
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;

    #[test]
    fn decodes_ghidra_sla_header_and_zlib_payload() {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(b"template-payload").unwrap();
        let compressed = encoder.finish().unwrap();
        let mut artifact = Vec::from(&b"sla\x04"[..]);
        artifact.extend(compressed);

        let decoded = decode_compiled_sla(PathBuf::from("x86-64.sla"), &artifact).unwrap();
        assert_eq!(decoded.version, 4);
        assert_eq!(decoded.payload, b"template-payload");
    }

    #[test]
    fn rejects_non_sla_artifact() {
        let err = decode_compiled_sla(PathBuf::from("x86-64.slaspec"), b"not-sla")
            .expect_err("slaspec text must not be treated as compiled SLEIGH");
        assert!(err.to_string().contains("missing sla magic"));
    }

    #[test]
    fn decodes_real_x86_64_sla_construct_templates() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap()
            .join("vendor/ghidra/ghidra_12.0.4_PUBLIC/Ghidra/Processors/x86/data/languages/x86-64.sla");
        if !path.exists() {
            return;
        }
        let library = load_construct_templates_from_sla(path).expect("decode x86-64.sla");
        assert!(!library.source_files.is_empty());
        assert!(!library.spaces.is_empty());
        assert!(!library.constructors_by_source.is_empty());
        assert!(library
            .constructors_by_source
            .values()
            .any(|templates| templates
                .iter()
                .any(|template| !template.constructor_template.op_templates.is_empty())));
    }
}
