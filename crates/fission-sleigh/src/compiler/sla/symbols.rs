use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{bail, Result};

use super::*;
use crate::compiler::ir::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledSlaArtifact {
    pub path: PathBuf,
    pub version: u8,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledSlaSubtable {
    pub name: String,
    pub constructors: Vec<CompiledSlaConstructorTemplate>,
    pub decision_tree: Option<crate::compiler::ir::CompiledDecisionTree>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledSlaTemplateLibrary {
    pub path: PathBuf,
    pub version: u8,
    pub source_files: BTreeMap<u64, String>,
    pub spaces: BTreeMap<u64, CompiledSpaceRef>,
    pub unique_space_index: u64,
    pub register_space_index: u64,
    pub uniqbase: u64,
    pub uniqmask: u64,
    pub constructors_by_source: BTreeMap<String, Vec<CompiledSlaConstructorTemplate>>,
    pub subtables: BTreeMap<String, CompiledSlaSubtable>,
    pub native: SlaLanguage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledSlaConstructorTemplate {
    pub id: u32,
    pub subtable_id: u32,
    pub subtable_name: String,
    pub constructor_slot: usize,
    pub decode_status: CompiledSlaDecodeStatus,
    pub source_key: String,
    pub source_file: String,
    pub line: u64,
    pub minimum_length: u32,
    pub display_template: CompiledDisplayTemplate,
    pub display_operands: Vec<CompiledDisplayOperand>,
    pub opprint_indices: Vec<usize>,
    pub operand_specs: Vec<CompiledOperandSpec>,
    pub operand_minimum_lengths: Vec<u32>,
    pub context_changes: Vec<CompiledContextOp>,
    /// Deferred global context commits (Ghidra `globalset` / `ELEM_COMMIT` elements).
    pub context_commits: Vec<CompiledContextCommit>,
    pub flowthru_operand_index: Option<usize>,
    pub constructor_template: CompiledConstructTpl,
    /// Named p-code sections from Ghidra's `namedtempl` (ATTR_SECTION >= 0).
    /// Index corresponds to the section number. Used by CROSSBUILD and
    /// sectioned constructors to dispatch p-code from a specific named section.
    pub named_templates: Vec<Option<CompiledConstructTpl>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DecodedOperandSymbol {
    pub(super) hand_index: usize,
    /// Byte offset of this operand's token from the start of the parent constructor.
    /// Corresponds to `ATTRIB_OFF` in Ghidra's SLA format (OperandSymbol.reloffset).
    pub(super) reloffset: i32,
    /// Index of the base operand for the offset calculation, or -1 if relative to
    /// the constructor's own start. Corresponds to `ATTRIB_BASE` (OperandSymbol.offsetbase).
    pub(super) offsetbase: i32,
    /// Minimum byte length of this operand state. Corresponds to `ATTRIB_MINLEN`
    /// (OperandSymbol.minimumlength), used by Ghidra before parent `calcCurrentLength()`.
    pub(super) minimum_length: u32,
    pub(super) subtable_name: Option<String>,
    pub(super) display_kind: CompiledDisplayOperandKind,
    pub(super) token_field: Option<DecodedTokenField>,
    pub(super) pattern_expression: Option<CompiledPatternExpression>,
    pub(super) varnode_list: Option<Vec<CompiledResolvedVarnode>>,
    pub(super) value_map: Option<Vec<i64>>,
    pub(super) fixed_varnode: Option<CompiledResolvedVarnode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DecodedTokenField {
    pub(super) big_endian: bool,
    pub(super) sign_bit: bool,
    pub(super) bit_start: u32,
    pub(super) bit_end: u32,
    pub(super) byte_start: u32,
    pub(super) byte_end: u32,
    pub(super) shift: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DecodedContextField {
    pub(super) sign_bit: bool,
    pub(super) bit_start: u32,
    pub(super) bit_end: u32,
    pub(super) byte_start: u32,
    pub(super) byte_end: u32,
    pub(super) shift: i32,
}

pub(super) fn decode_source_files(root: &PackedElement) -> Result<BTreeMap<u64, String>> {
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

pub(super) struct SlaSpaceDecodeResult {
    pub spaces: BTreeMap<u64, CompiledSpaceRef>,
    pub unique_space_index: u64,
    pub register_space_index: u64,
}

pub(super) fn decode_spaces(root: &PackedElement) -> Result<SlaSpaceDecodeResult> {
    let mut spaces = BTreeMap::new();
    spaces.insert(
        0,
        CompiledSpaceRef {
            name: "const".to_string(),
            index: 0,
            word_size: 0,
            addr_size: 0,
        },
    );
    let mut unique_space_index = u64::MAX;
    let mut register_space_index = u64::MAX;

    for space in root.descendants_with_id(sla_format::ELEM_SPACE) {
        let index = space
            .attr_unsigned(sla_format::ATTR_INDEX)
            .ok_or_else(|| anyhow!("space missing index"))?;
        let name = space
            .attr_string(sla_format::ATTR_NAME)
            .ok_or_else(|| anyhow!("space missing name"))?;
        // Ghidra: ATTRIB_WORDSIZE is the addressable unit size (1 for byte-addressed spaces).
        // ATTRIB_WORDSIZE is only written to the SLA when > 1; default is 1.
        // ATTRIB_SIZE is the address/pointer size (e.g., 4 for x86-32).
        let word_size = space
            .attr_unsigned(sla_format::ATTR_WORDSIZE)
            .map(|v| v as u32)
            .unwrap_or(1);
        let addr_size = space
            .attr_unsigned(sla_format::ATTR_SIZE)
            .map(|v| v as u32)
            .unwrap_or(0);
        if name == "register" {
            register_space_index = index;
        }
        spaces.insert(
            index,
            CompiledSpaceRef {
                name: name.to_string(),
                index,
                word_size,
                addr_size,
            },
        );
    }
    for space in root.descendants_with_id(sla_format::ELEM_SPACE_UNIQUE) {
        let index = space
            .attr_unsigned(sla_format::ATTR_INDEX)
            .ok_or_else(|| anyhow!("unique space missing index"))?;
        let name = space.attr_string(sla_format::ATTR_NAME).unwrap_or("unique");
        unique_space_index = index;
        spaces.insert(
            index,
            CompiledSpaceRef {
                name: name.to_string(),
                index,
                word_size: 0,
                addr_size: 0,
            },
        );
    }
    Ok(SlaSpaceDecodeResult {
        spaces,
        unique_space_index,
        register_space_index,
    })
}

pub(super) fn decode_operand_symbols(
    root: &PackedElement,
    display_symbols: &BTreeMap<u32, DecodedDisplaySymbol>,
) -> Result<BTreeMap<u32, DecodedOperandSymbol>> {
    let mut out = BTreeMap::new();
    for operand in root.descendants_with_id(sla_format::ELEM_OPERAND_SYM) {
        let id = operand
            .attr_unsigned(sla_format::ATTR_ID)
            .ok_or_else(|| anyhow!("operand_sym missing id"))? as u32;
        let hand_index = operand
            .attr_signed(sla_format::ATTR_INDEX)
            .ok_or_else(|| anyhow!("operand_sym missing index"))? as usize;
        let reloffset = operand.attr_signed(sla_format::ATTR_OFF).unwrap_or(0) as i32;
        let offsetbase = operand.attr_signed(sla_format::ATTR_BASE).unwrap_or(-1) as i32;
        let minimum_length = operand.attr_unsigned(sla_format::ATTR_MINLEN).unwrap_or(0) as u32;
        let direct_pattern_expression = operand
            .children
            .iter()
            .rev()
            .find_map(|child| decode_pattern_expression(child).ok())
            .filter(|expr| !pattern_expression_references_operand(expr, hand_index));
        let (
            subtable_name,
            display_kind,
            fallback_token_field,
            pattern_expression,
            varnode_list,
            value_map,
            fixed_varnode,
        ) = operand
            .attr_unsigned(sla_format::ATTR_SUBSYM)
            .and_then(|value| display_symbols.get(&(value as u32)))
            .map(|symbol| match symbol {
                DecodedDisplaySymbol::Subtable(name) => (
                    Some(name.clone()),
                    CompiledDisplayOperandKind::Subtable,
                    None,
                    None,
                    None,
                    None,
                    None,
                ),
                DecodedDisplaySymbol::ValueHex { expression } => (
                    None,
                    decoded_display_kind(symbol),
                    None,
                    expression.clone(),
                    None,
                    None,
                    None,
                ),
                DecodedDisplaySymbol::NameTable { token_field, .. } => (
                    None,
                    decoded_display_kind(symbol),
                    token_field.clone(),
                    None,
                    None,
                    None,
                    None,
                ),
                DecodedDisplaySymbol::ValueMap {
                    token_field,
                    values,
                } => (
                    None,
                    decoded_display_kind(symbol),
                    token_field.clone(),
                    None,
                    None,
                    Some(values.clone()),
                    None,
                ),
                DecodedDisplaySymbol::VarnodeList {
                    entries,
                    token_field,
                } => (
                    None,
                    decoded_display_kind(symbol),
                    token_field.clone(),
                    None,
                    Some(entries.clone()),
                    None,
                    None,
                ),
                DecodedDisplaySymbol::FixedVarnode(varnode) => (
                    None,
                    decoded_display_kind(symbol),
                    None,
                    None,
                    None,
                    None,
                    Some(varnode.clone()),
                ),
            })
            .unwrap_or((
                None,
                CompiledDisplayOperandKind::Generic,
                None,
                direct_pattern_expression.clone(),
                None,
                None,
                None,
            ));
        let token_field = operand
            .children
            .iter()
            .find(|child| child.id == sla_format::ELEM_TOKENFIELD)
            .map(decode_token_field)
            .transpose()?
            .or(fallback_token_field);
        out.insert(
            id,
            DecodedOperandSymbol {
                hand_index,
                reloffset,
                offsetbase,
                minimum_length,
                subtable_name,
                display_kind,
                token_field,
                pattern_expression: pattern_expression.or(direct_pattern_expression),
                varnode_list,
                value_map,
                fixed_varnode,
            },
        );
    }
    Ok(out)
}

pub(super) fn decode_token_field(element: &PackedElement) -> Result<DecodedTokenField> {
    Ok(DecodedTokenField {
        big_endian: element.attr_bool(sla_format::ATTR_BIGENDIAN),
        sign_bit: element.attr_bool(sla_format::ATTR_SIGNBIT),
        bit_start: element
            .attr_signed(sla_format::ATTR_STARTBIT)
            .ok_or_else(|| anyhow!("tokenfield missing startbit"))? as u32,
        bit_end: element
            .attr_signed(sla_format::ATTR_ENDBIT)
            .ok_or_else(|| anyhow!("tokenfield missing endbit"))? as u32,
        byte_start: element
            .attr_signed(sla_format::ATTR_STARTBYTE)
            .ok_or_else(|| anyhow!("tokenfield missing startbyte"))? as u32,
        byte_end: element
            .attr_signed(sla_format::ATTR_ENDBYTE)
            .ok_or_else(|| anyhow!("tokenfield missing endbyte"))? as u32,
        shift: element
            .attr_signed(sla_format::ATTR_SHIFT)
            .ok_or_else(|| anyhow!("tokenfield missing shift"))? as i32,
    })
}

pub(super) fn compiled_operand_spec_for_symbol(
    symbol: &DecodedOperandSymbol,
    _subtable_names_by_id: &BTreeMap<u32, String>,
) -> Option<CompiledOperandSpec> {
    if let Some(table_name) = &symbol.subtable_name {
        return Some(CompiledOperandSpec::SubtableEvaluation {
            table_name: table_name.clone(),
            reloffset: symbol.reloffset,
            offsetbase: symbol.offsetbase,
        });
    }
    if let Some(varnode) = &symbol.fixed_varnode {
        return Some(CompiledOperandSpec::SlaFixedVarnode {
            varnode: varnode.clone(),
        });
    }
    if let Some(expr) = &symbol.pattern_expression {
        return Some(CompiledOperandSpec::SlaPatternExpression {
            expr: expr.clone(),
            reloffset: symbol.reloffset,
        });
    }
    if let (Some(token_field), Some(entries)) = (&symbol.token_field, &symbol.varnode_list) {
        return Some(CompiledOperandSpec::SlaVarnodeList {
            big_endian: token_field.big_endian,
            sign_bit: token_field.sign_bit,
            bit_start: token_field.bit_start,
            bit_end: token_field.bit_end,
            byte_start: token_field.byte_start,
            byte_end: token_field.byte_end,
            shift: token_field.shift,
            entries: entries.clone(),
            reloffset: symbol.reloffset,
        });
    }
    if let (Some(token_field), Some(values)) = (&symbol.token_field, &symbol.value_map) {
        return Some(CompiledOperandSpec::SlaValueMap {
            big_endian: token_field.big_endian,
            sign_bit: token_field.sign_bit,
            bit_start: token_field.bit_start,
            bit_end: token_field.bit_end,
            byte_start: token_field.byte_start,
            byte_end: token_field.byte_end,
            shift: token_field.shift,
            values: values.clone(),
            reloffset: symbol.reloffset,
        });
    }
    symbol
        .token_field
        .as_ref()
        .map(|field| CompiledOperandSpec::SlaTokenField {
            big_endian: field.big_endian,
            sign_bit: field.sign_bit,
            bit_start: field.bit_start,
            bit_end: field.bit_end,
            byte_start: field.byte_start,
            byte_end: field.byte_end,
            shift: field.shift,
            reloffset: symbol.reloffset,
        })
}

pub(super) fn decode_context_op(element: &PackedElement) -> Result<CompiledContextOp> {
    let word_index = element
        .attr_signed(sla_format::ATTR_I)
        .ok_or_else(|| anyhow!("context_op missing word index"))? as u32;
    let shift = element
        .attr_signed(sla_format::ATTR_SHIFT)
        .ok_or_else(|| anyhow!("context_op missing shift"))? as i32;
    let mask = element
        .attr_unsigned(sla_format::ATTR_MASK)
        .ok_or_else(|| anyhow!("context_op missing mask"))?;
    let expr = element
        .children
        .first()
        .map(decode_pattern_expression)
        .transpose()?;
    Ok(CompiledContextOp {
        bit_offset: shift.max(0) as u32,
        bit_width: mask.count_ones(),
        value: 0,
        word_index,
        mask,
        shift,
        expr,
    })
}

pub(super) fn decode_pattern_expression(
    element: &PackedElement,
) -> Result<CompiledPatternExpression> {
    let mut binary = |ctor: fn(
        Box<CompiledPatternExpression>,
        Box<CompiledPatternExpression>,
    ) -> CompiledPatternExpression|
     -> Result<CompiledPatternExpression> {
        if element.children.len() != 2 {
            bail!("pattern expression {} expected two children", element.id);
        }
        Ok(ctor(
            Box::new(decode_pattern_expression(&element.children[0])?),
            Box::new(decode_pattern_expression(&element.children[1])?),
        ))
    };
    match element.id {
        sla_format::ELEM_INTB => Ok(CompiledPatternExpression::Constant(
            element
                .attr_signed(sla_format::ATTR_VAL)
                .ok_or_else(|| anyhow!("intb missing val"))?,
        )),
        sla_format::ELEM_START_EXP => Ok(CompiledPatternExpression::InstStart),
        sla_format::ELEM_END_EXP => Ok(CompiledPatternExpression::InstNext),
        sla_format::ELEM_NEXT2_EXP => Ok(CompiledPatternExpression::InstNext2),
        sla_format::ELEM_TOKENFIELD => {
            let field = decode_token_field(element)?;
            Ok(CompiledPatternExpression::TokenField {
                big_endian: field.big_endian,
                sign_bit: field.sign_bit,
                bit_start: field.bit_start,
                bit_end: field.bit_end,
                byte_start: field.byte_start,
                byte_end: field.byte_end,
                shift: field.shift,
            })
        }
        sla_format::ELEM_CONTEXTFIELD => {
            let field = decode_context_field(element)?;
            Ok(CompiledPatternExpression::ContextField {
                sign_bit: field.sign_bit,
                bit_start: field.bit_start,
                bit_end: field.bit_end,
                byte_start: field.byte_start,
                byte_end: field.byte_end,
                shift: field.shift,
            })
        }
        sla_format::ELEM_OPERAND_EXP => Ok(CompiledPatternExpression::OperandValue {
            index: element
                .attr_signed(sla_format::ATTR_INDEX)
                .ok_or_else(|| anyhow!("operand_exp missing index"))? as usize,
        }),
        sla_format::ELEM_PLUS_EXP => binary(CompiledPatternExpression::Add),
        sla_format::ELEM_SUB_EXP => binary(CompiledPatternExpression::Sub),
        sla_format::ELEM_MULT_EXP => binary(CompiledPatternExpression::Mul),
        sla_format::ELEM_DIV_EXP => binary(CompiledPatternExpression::Div),
        sla_format::ELEM_LSHIFT_EXP => binary(CompiledPatternExpression::LeftShift),
        sla_format::ELEM_RSHIFT_EXP => binary(CompiledPatternExpression::RightShift),
        sla_format::ELEM_AND_EXP => binary(CompiledPatternExpression::And),
        sla_format::ELEM_OR_EXP => binary(CompiledPatternExpression::Or),
        sla_format::ELEM_XOR_EXP => binary(CompiledPatternExpression::Xor),
        sla_format::ELEM_MINUS_EXP => {
            if element.children.len() != 1 {
                bail!("minus_exp expected one child");
            }
            Ok(CompiledPatternExpression::Negate(Box::new(
                decode_pattern_expression(&element.children[0])?,
            )))
        }
        sla_format::ELEM_NOT_EXP => {
            if element.children.len() != 1 {
                bail!("not_exp expected one child");
            }
            Ok(CompiledPatternExpression::Not(Box::new(
                decode_pattern_expression(&element.children[0])?,
            )))
        }
        other => bail!("unsupported pattern expression element {other}"),
    }
}

pub(super) fn pattern_expression_references_operand(
    expr: &CompiledPatternExpression,
    operand_index: usize,
) -> bool {
    match expr {
        CompiledPatternExpression::OperandValue { index } => *index == operand_index,
        CompiledPatternExpression::Add(lhs, rhs)
        | CompiledPatternExpression::Sub(lhs, rhs)
        | CompiledPatternExpression::Mul(lhs, rhs)
        | CompiledPatternExpression::Div(lhs, rhs)
        | CompiledPatternExpression::LeftShift(lhs, rhs)
        | CompiledPatternExpression::RightShift(lhs, rhs)
        | CompiledPatternExpression::And(lhs, rhs)
        | CompiledPatternExpression::Or(lhs, rhs)
        | CompiledPatternExpression::Xor(lhs, rhs) => {
            pattern_expression_references_operand(lhs, operand_index)
                || pattern_expression_references_operand(rhs, operand_index)
        }
        CompiledPatternExpression::Negate(inner) | CompiledPatternExpression::Not(inner) => {
            pattern_expression_references_operand(inner, operand_index)
        }
        CompiledPatternExpression::Constant(_)
        | CompiledPatternExpression::InstStart
        | CompiledPatternExpression::InstNext
        | CompiledPatternExpression::InstNext2
        | CompiledPatternExpression::TokenField { .. }
        | CompiledPatternExpression::ContextField { .. } => false,
    }
}

pub(super) fn decode_context_field(element: &PackedElement) -> Result<DecodedContextField> {
    Ok(DecodedContextField {
        sign_bit: element.attr_bool(sla_format::ATTR_SIGNBIT),
        bit_start: element
            .attr_signed(sla_format::ATTR_STARTBIT)
            .ok_or_else(|| anyhow!("contextfield missing startbit"))? as u32,
        bit_end: element
            .attr_signed(sla_format::ATTR_ENDBIT)
            .ok_or_else(|| anyhow!("contextfield missing endbit"))? as u32,
        byte_start: element
            .attr_signed(sla_format::ATTR_STARTBYTE)
            .ok_or_else(|| anyhow!("contextfield missing startbyte"))? as u32,
        byte_end: element
            .attr_signed(sla_format::ATTR_ENDBYTE)
            .ok_or_else(|| anyhow!("contextfield missing endbyte"))? as u32,
        shift: element
            .attr_signed(sla_format::ATTR_SHIFT)
            .ok_or_else(|| anyhow!("contextfield missing shift"))? as i32,
    })
}

pub(super) fn decode_space_ref(
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
