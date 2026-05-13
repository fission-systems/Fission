use std::collections::BTreeMap;

use anyhow::{anyhow, Result};

use super::*;
use crate::compiler::ir::*;

fn required_unsigned_u32(element: &PackedElement, attr: u32, label: &str) -> Result<u32> {
    let value = element
        .attr_unsigned(attr)
        .ok_or_else(|| anyhow!("{label} missing"))?;
    u32::try_from(value).map_err(|_| anyhow!("{label} out of u32 range: {value}"))
}

fn required_signed_u32(element: &PackedElement, attr: u32, label: &str) -> Result<u32> {
    let value = element
        .attr_signed(attr)
        .ok_or_else(|| anyhow!("{label} missing"))?;
    u32::try_from(value).map_err(|_| anyhow!("{label} out of u32 range: {value}"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum DecodedDisplaySymbol {
    Subtable(String),
    ValueHex {
        expression: Option<CompiledPatternExpression>,
    },
    NameTable {
        names: Vec<String>,
        token_field: Option<DecodedTokenField>,
        selector_expr: Option<CompiledPatternExpression>,
    },
    ValueMap {
        values: Vec<i64>,
        token_field: Option<DecodedTokenField>,
        selector_expr: Option<CompiledPatternExpression>,
    },
    VarnodeList {
        entries: Vec<CompiledResolvedVarnode>,
        token_field: Option<DecodedTokenField>,
        selector_expr: Option<CompiledPatternExpression>,
    },
    FixedVarnode(CompiledResolvedVarnode),
}

pub(super) fn decode_display_symbols(
    root: &PackedElement,
    spaces: &BTreeMap<u64, CompiledSpaceRef>,
    symbol_names: &BTreeMap<u32, String>,
    subtable_names_by_id: &BTreeMap<u32, String>,
) -> Result<BTreeMap<u32, DecodedDisplaySymbol>> {
    let mut out = BTreeMap::new();
    let mut fixed_varnodes = BTreeMap::new();
    for symbol in root.descendants_with_id(sla_format::ELEM_VARNODE_SYM) {
        let id = required_unsigned_u32(symbol, sla_format::ATTR_ID, "varnode_sym id")?;
        let space_index = symbol
            .attr_unsigned(sla_format::ATTR_SPACE)
            .ok_or_else(|| anyhow!("varnode_sym {id} missing space"))?;
        let name = symbol_names
            .get(&id)
            .cloned()
            .ok_or_else(|| anyhow!("varnode_sym {id} missing symbol name"))?;
        let space = spaces
            .get(&space_index)
            .cloned()
            .ok_or_else(|| anyhow!("varnode_sym {id} references unknown space {space_index}"))?;
        let offset = symbol
            .attr_unsigned(sla_format::ATTR_OFF)
            .ok_or_else(|| anyhow!("varnode_sym {id} missing offset"))?;
        let size = required_signed_u32(symbol, sla_format::ATTR_SIZE, "varnode_sym size")?;
        fixed_varnodes.insert(
            id,
            CompiledResolvedVarnode {
                name,
                space,
                offset,
                size,
            },
        );
    }
    for (id, varnode) in &fixed_varnodes {
        out.insert(*id, DecodedDisplaySymbol::FixedVarnode(varnode.clone()));
    }
    for subtable in root.descendants_with_id(sla_format::ELEM_SUBTABLE_SYM) {
        let id = required_unsigned_u32(subtable, sla_format::ATTR_ID, "subtable_sym id")?;
        let name = subtable_names_by_id
            .get(&id)
            .cloned()
            .ok_or_else(|| anyhow!("subtable_sym {id} missing decoded subtable name"))?;
        out.insert(id, DecodedDisplaySymbol::Subtable(name));
    }
    for symbol in root.descendants_with_id(sla_format::ELEM_VALUE_SYM) {
        let id = required_unsigned_u32(symbol, sla_format::ATTR_ID, "value_sym id")?;
        let expression = symbol
            .children
            .first()
            .map(decode_pattern_expression)
            .transpose()?;
        out.insert(id, DecodedDisplaySymbol::ValueHex { expression });
    }
    for symbol in root.descendants_with_id(sla_format::ELEM_CONTEXT_SYM) {
        let id = required_unsigned_u32(symbol, sla_format::ATTR_ID, "context_sym id")?;
        let expression = symbol
            .children
            .first()
            .map(decode_pattern_expression)
            .transpose()?;
        out.insert(id, DecodedDisplaySymbol::ValueHex { expression });
    }
    for symbol in root.descendants_with_id(sla_format::ELEM_NAME_SYM) {
        let id = required_unsigned_u32(symbol, sla_format::ATTR_ID, "name_sym id")?;
        let token_field = symbol
            .children
            .first()
            .map(decode_token_field_if_direct)
            .transpose()?
            .flatten();
        let selector_expr = first_decoded_pattern_expression(symbol.children.iter())?;
        let names = symbol
            .children
            .iter()
            .filter(|child| child.id == sla_format::ELEM_NAMETAB)
            .map(decoded_name_table_entry)
            .collect::<Vec<_>>();
        out.insert(
            id,
            DecodedDisplaySymbol::NameTable {
                names,
                token_field,
                selector_expr,
            },
        );
    }
    for symbol in root.descendants_with_id(sla_format::ELEM_VALUEMAP_SYM) {
        let id = required_unsigned_u32(symbol, sla_format::ATTR_ID, "valuemap_sym id")?;
        let token_field = symbol
            .children
            .first()
            .map(decode_token_field_if_direct)
            .transpose()?
            .flatten();
        let selector_expr = first_decoded_pattern_expression(symbol.children.iter())?;
        let values = symbol
            .children
            .iter()
            .filter(|child| child.id == sla_format::ELEM_VALUETAB)
            .map(|child| {
                child
                    .attr_signed(sla_format::ATTR_VAL)
                    .ok_or_else(|| anyhow!("valuemap_sym {id} has valuetab without value"))
            })
            .collect::<Result<Vec<_>>>()?;
        out.insert(
            id,
            DecodedDisplaySymbol::ValueMap {
                values,
                token_field,
                selector_expr,
            },
        );
    }
    for symbol in root.descendants_with_id(sla_format::ELEM_VARLIST_SYM) {
        let id = required_unsigned_u32(symbol, sla_format::ATTR_ID, "varlist_sym id")?;
        let token_field = symbol
            .children
            .first()
            .map(decode_token_field_if_direct)
            .transpose()?
            .flatten();
        let selector_expr = first_decoded_pattern_expression(symbol.children.iter())?;
        let entries = symbol
            .children
            .iter()
            .filter(|child| child.id == sla_format::ELEM_VAR)
            .map(|child| {
                let var_id = required_unsigned_u32(child, sla_format::ATTR_ID, "var id")
                    .map_err(|err| anyhow!("varlist_sym {id} has malformed var id: {err}"))?;
                fixed_varnodes
                    .get(&var_id)
                    .cloned()
                    .ok_or_else(|| anyhow!("varlist_sym {id} references unknown varnode {var_id}"))
            })
            .collect::<Result<Vec<_>>>()?;
        out.insert(
            id,
            DecodedDisplaySymbol::VarnodeList {
                entries,
                token_field,
                selector_expr,
            },
        );
    }
    Ok(out)
}

pub(super) fn decoded_display_kind(symbol: &DecodedDisplaySymbol) -> CompiledDisplayOperandKind {
    match symbol {
        DecodedDisplaySymbol::Subtable(_) => CompiledDisplayOperandKind::Subtable,
        DecodedDisplaySymbol::ValueHex { .. } => CompiledDisplayOperandKind::ValueHex,
        DecodedDisplaySymbol::NameTable { names, .. } => {
            CompiledDisplayOperandKind::NameTable(names.clone())
        }
        DecodedDisplaySymbol::ValueMap { values, .. } => {
            CompiledDisplayOperandKind::ValueMap(values.clone())
        }
        DecodedDisplaySymbol::VarnodeList { entries, .. } => {
            CompiledDisplayOperandKind::VarnodeList(
                entries.iter().map(|entry| entry.name.clone()).collect(),
            )
        }
        DecodedDisplaySymbol::FixedVarnode(_) => CompiledDisplayOperandKind::Generic,
    }
}

fn decoded_name_table_entry(element: &PackedElement) -> String {
    element
        .attr_string(sla_format::ATTR_NAME)
        .unwrap_or("")
        .to_string()
}

pub(super) fn decode_token_field_if_direct(
    element: &PackedElement,
) -> Result<Option<DecodedTokenField>> {
    if element.id == sla_format::ELEM_TOKENFIELD {
        return Ok(Some(decode_token_field(element)?));
    }
    Ok(None)
}

pub(super) fn operand_piece_label(index: usize) -> char {
    ((index as u8) + b'A') as char
}
