use std::collections::BTreeMap;

use anyhow::Result;

use super::*;
use crate::compiler::ir::*;

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
        if let Some(id) = symbol
            .attr_unsigned(sla_format::ATTR_ID)
            .map(|value| value as u32)
        {
            let Some(space_index) = symbol.attr_unsigned(sla_format::ATTR_SPACE) else {
                continue;
            };
            let Some(name) = symbol_names.get(&id).cloned() else {
                continue;
            };
            let Some(space) = spaces.get(&space_index).cloned() else {
                continue;
            };
            fixed_varnodes.insert(
                id,
                CompiledResolvedVarnode {
                    name,
                    space,
                    offset: symbol
                        .attr_unsigned(sla_format::ATTR_OFF)
                        .unwrap_or_default(),
                    size: symbol
                        .attr_signed(sla_format::ATTR_SIZE)
                        .unwrap_or_default()
                        .max(0) as u32,
                },
            );
        }
    }
    for (id, varnode) in &fixed_varnodes {
        out.insert(*id, DecodedDisplaySymbol::FixedVarnode(varnode.clone()));
    }
    for subtable in root.descendants_with_id(sla_format::ELEM_SUBTABLE_SYM) {
        if let Some(id) = subtable
            .attr_unsigned(sla_format::ATTR_ID)
            .map(|value| value as u32)
        {
            if let Some(name) = subtable_names_by_id.get(&id) {
                out.insert(id, DecodedDisplaySymbol::Subtable(name.clone()));
            }
        }
    }
    for symbol in root.descendants_with_id(sla_format::ELEM_VALUE_SYM) {
        if let Some(id) = symbol
            .attr_unsigned(sla_format::ATTR_ID)
            .map(|value| value as u32)
        {
            let expression = symbol
                .children
                .first()
                .map(decode_pattern_expression)
                .transpose()?;
            out.insert(id, DecodedDisplaySymbol::ValueHex { expression });
        }
    }
    for symbol in root.descendants_with_id(sla_format::ELEM_CONTEXT_SYM) {
        if let Some(id) = symbol
            .attr_unsigned(sla_format::ATTR_ID)
            .map(|value| value as u32)
        {
            let expression = symbol
                .children
                .first()
                .map(decode_pattern_expression)
                .transpose()?;
            out.insert(id, DecodedDisplaySymbol::ValueHex { expression });
        }
    }
    for symbol in root.descendants_with_id(sla_format::ELEM_NAME_SYM) {
        if let Some(id) = symbol
            .attr_unsigned(sla_format::ATTR_ID)
            .map(|value| value as u32)
        {
            let token_field = symbol
                .children
                .first()
                .map(decode_token_field_if_direct)
                .transpose()?
                .flatten();
            let selector_expr = symbol
                .children
                .iter()
                .find_map(|child| decode_pattern_expression(child).ok());
            let names = symbol
                .children
                .iter()
                .filter(|child| child.id == sla_format::ELEM_NAMETAB)
                .map(|child| {
                    child
                        .attr_string(sla_format::ATTR_NAME)
                        .unwrap_or_default()
                        .to_string()
                })
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
    }
    for symbol in root.descendants_with_id(sla_format::ELEM_VALUEMAP_SYM) {
        if let Some(id) = symbol
            .attr_unsigned(sla_format::ATTR_ID)
            .map(|value| value as u32)
        {
            let token_field = symbol
                .children
                .first()
                .map(decode_token_field_if_direct)
                .transpose()?
                .flatten();
            let selector_expr = symbol
                .children
                .iter()
                .find_map(|child| decode_pattern_expression(child).ok());
            let values = symbol
                .children
                .iter()
                .filter(|child| child.id == sla_format::ELEM_VALUETAB)
                .map(|child| child.attr_signed(sla_format::ATTR_VAL).unwrap_or_default())
                .collect::<Vec<_>>();
            out.insert(
                id,
                DecodedDisplaySymbol::ValueMap {
                    values,
                    token_field,
                    selector_expr,
                },
            );
        }
    }
    for symbol in root.descendants_with_id(sla_format::ELEM_VARLIST_SYM) {
        if let Some(id) = symbol
            .attr_unsigned(sla_format::ATTR_ID)
            .map(|value| value as u32)
        {
            let token_field = symbol
                .children
                .first()
                .map(decode_token_field_if_direct)
                .transpose()?
                .flatten();
            let selector_expr = symbol
                .children
                .iter()
                .find_map(|child| decode_pattern_expression(child).ok());
            let entries = symbol
                .children
                .iter()
                .filter(|child| child.id == sla_format::ELEM_VAR)
                .filter_map(|child| child.attr_unsigned(sla_format::ATTR_ID))
                .filter_map(|var_id| fixed_varnodes.get(&(var_id as u32)).cloned())
                .collect::<Vec<_>>();
            out.insert(
                id,
                DecodedDisplaySymbol::VarnodeList {
                    entries,
                    token_field,
                    selector_expr,
                },
            );
        }
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
