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
    /// Ghidra `defaultspace` attribute resolved against `spaces` (`u64::MAX` if unknown).
    pub default_space_index: u64,
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
    pub context_changes: Vec<CompiledContextOp>,
    /// Deferred global context commits (Ghidra `globalset` / `ELEM_COMMIT` elements).
    pub context_commits: Vec<CompiledContextCommit>,
    pub flowthru_operand_index: Option<usize>,
    pub constructor_template: CompiledConstructTpl,
    /// Named p-code sections from Ghidra's `namedtempl` (ATTR_SECTION >= 0).
    /// Index corresponds to the section number. Used by CROSSBUILD and
    /// sectioned constructors to dispatch p-code from a specific named section.
    pub named_templates: Vec<Option<CompiledConstructTpl>>,
    /// Per-operand metadata from Ghidra `OperandSymbol.encode` (`ATTRIB_MINLEN`, `ATTRIB_CODE`).
    /// Index aligns with `operand_specs` / constructor handle order.
    pub operand_symbol_meta: Vec<SlaOperandSymbolMeta>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DecodedOperandSymbol {
    hand_index: usize,
    /// Byte offset of this operand's token from the start of the parent constructor.
    /// Corresponds to `ATTRIB_OFF` in Ghidra's SLA format (OperandSymbol.reloffset).
    reloffset: i32,
    /// Index of the base operand for the offset calculation, or -1 if relative to
    /// the constructor's own start. Corresponds to `ATTRIB_BASE` (OperandSymbol.offsetbase).
    offsetbase: i32,
    /// Ghidra `OperandSymbol.encode` fields carried on `ELEM_OPERAND_SYM` (see `SlaOperandSymbolMeta`).
    sla_encode_meta: SlaOperandSymbolMeta,
    subtable_name: Option<String>,
    display_kind: CompiledDisplayOperandKind,
    token_field: Option<DecodedTokenField>,
    pattern_expression: Option<CompiledPatternExpression>,
    varnode_list: Option<Vec<CompiledResolvedVarnode>>,
    value_map: Option<Vec<i64>>,
    fixed_varnode: Option<CompiledResolvedVarnode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DecodedTokenField {
    big_endian: bool,
    sign_bit: bool,
    bit_start: u32,
    bit_end: u32,
    byte_start: u32,
    byte_end: u32,
    shift: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DecodedContextField {
    sign_bit: bool,
    bit_start: u32,
    bit_end: u32,
    byte_start: u32,
    byte_end: u32,
    shift: i32,
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

pub(super) struct SlaSpaceDecodeResult {
    pub spaces: BTreeMap<u64, CompiledSpaceRef>,
    pub unique_space_index: u64,
    pub register_space_index: u64,
    /// Resolved index of Ghidra `defaultspace` on the `spaces` element (`u64::MAX` if unknown).
    pub default_space_index: u64,
}

fn walk_space_children(
    element: &PackedElement,
    spaces: &mut BTreeMap<u64, CompiledSpaceRef>,
    unique_space_index: &mut u64,
    register_space_index: &mut u64,
) -> Result<()> {
    if element.id == sla_format::ELEM_SPACE {
        ingest_packed_space(
            spaces,
            unique_space_index,
            register_space_index,
            element,
            false,
        )?;
    } else if element.id == sla_format::ELEM_SPACE_UNIQUE {
        ingest_packed_space(
            spaces,
            unique_space_index,
            register_space_index,
            element,
            true,
        )?;
    }
    for child in &element.children {
        walk_space_children(
            child,
            spaces,
            unique_space_index,
            register_space_index,
        )?;
    }
    Ok(())
}

fn ingest_packed_space(
    spaces: &mut BTreeMap<u64, CompiledSpaceRef>,
    unique_space_index: &mut u64,
    register_space_index: &mut u64,
    element: &PackedElement,
    is_unique_element: bool,
) -> Result<()> {
    let index = element
        .attr_unsigned(sla_format::ATTR_INDEX)
        .ok_or_else(|| anyhow!("space missing index"))?;
    let name = element
        .attr_string(sla_format::ATTR_NAME)
        .unwrap_or(if is_unique_element { "unique" } else { "space" })
        .to_string();
    let word_size = element
        .attr_unsigned(sla_format::ATTR_WORDSIZE)
        .map(|v| v as u32)
        .unwrap_or(1);
    let addr_size = element
        .attr_unsigned(sla_format::ATTR_SIZE)
        .map(|v| v as u32)
        .unwrap_or(0);
    let delay = element
        .attr_signed(sla_format::ATTR_DELAY)
        .map(|v| v as i32)
        .unwrap_or(-1);
    let (sleigh_is_ram_class, sleigh_is_unique_space, delay_slots) = if is_unique_element {
        (false, true, -1)
    } else {
        (delay > 0, false, delay)
    };
    if name == "register" {
        *register_space_index = index;
    }
    if is_unique_element {
        *unique_space_index = index;
    }
    spaces.insert(
        index,
        CompiledSpaceRef {
            name: name.clone(),
            index,
            word_size,
            addr_size,
            sleigh_delay_slots: delay_slots,
            sleigh_is_ram_class,
            sleigh_is_unique_space,
        },
    );
    Ok(())
}

fn decode_spaces(root: &PackedElement) -> Result<SlaSpaceDecodeResult> {
    let mut spaces = BTreeMap::new();
    spaces.insert(
        0,
        CompiledSpaceRef {
            name: "const".to_string(),
            index: 0,
            word_size: 0,
            addr_size: 0,
            sleigh_delay_slots: -1,
            sleigh_is_ram_class: false,
            sleigh_is_unique_space: false,
        },
    );
    let mut unique_space_index = u64::MAX;
    let mut register_space_index = u64::MAX;

    let spaces_root = root
        .children
        .iter()
        .find(|child| child.id == sla_format::ELEM_SPACES);
    let default_space_name = spaces_root
        .and_then(|node| node.attr_string(sla_format::ATTR_DEFAULTSPACE))
        .map(str::to_string);

    if let Some(node) = spaces_root {
        walk_space_children(
            node,
            &mut spaces,
            &mut unique_space_index,
            &mut register_space_index,
        )?;
    } else {
        for space in root.descendants_with_id(sla_format::ELEM_SPACE) {
            ingest_packed_space(
                &mut spaces,
                &mut unique_space_index,
                &mut register_space_index,
                space,
                false,
            )?;
        }
        for space in root.descendants_with_id(sla_format::ELEM_SPACE_UNIQUE) {
            ingest_packed_space(
                &mut spaces,
                &mut unique_space_index,
                &mut register_space_index,
                space,
                true,
            )?;
        }
    }

    let mut default_space_index = u64::MAX;
    if let Some(ref want) = default_space_name {
        if let Some(found) = spaces.values().find(|s| s.name == *want) {
            default_space_index = found.index;
        }
    }
    if default_space_index == u64::MAX {
        let ram_candidates: Vec<u64> = spaces
            .iter()
            .filter(|(_, s)| s.sleigh_is_ram_class)
            .map(|(idx, _)| *idx)
            .collect();
        if ram_candidates.len() == 1 {
            default_space_index = ram_candidates[0];
        }
    }

    Ok(SlaSpaceDecodeResult {
        spaces,
        unique_space_index,
        register_space_index,
        default_space_index,
    })
}

fn decode_operand_symbols(
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
        let reloffset = operand
            .attr_signed(sla_format::ATTR_OFF)
            .unwrap_or(0) as i32;
        let offsetbase = operand
            .attr_signed(sla_format::ATTR_BASE)
            .unwrap_or(-1) as i32;
        let sla_encode_meta = SlaOperandSymbolMeta {
            min_length: operand
                .attr_signed(sla_format::ATTR_MINLEN)
                .map(|value| value as i32),
            code_address: operand.attr_bool(sla_format::ATTR_CODE),
        };
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
                sla_encode_meta,
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

fn decode_token_field(element: &PackedElement) -> Result<DecodedTokenField> {
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

fn compiled_operand_spec_for_symbol(
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

fn decode_context_op(element: &PackedElement) -> Result<CompiledContextOp> {
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

fn decode_pattern_expression(element: &PackedElement) -> Result<CompiledPatternExpression> {
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

fn pattern_expression_references_operand(
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
        | CompiledPatternExpression::TokenField { .. }
        | CompiledPatternExpression::ContextField { .. } => false,
    }
}

fn decode_context_field(element: &PackedElement) -> Result<DecodedContextField> {
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
