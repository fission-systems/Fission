use super::*;

pub(super) fn display_value_for_exported_handle(
    exported: &RuntimeHandle,
    sub_state: &RuntimeConstructState,
) -> Result<BoundOperand> {
    let exported_value = display_value_from_handle(exported, "exported subtable handle")?;
    let exported_is_direct_memory = matches!(
        exported_value,
        BoundOperand::Memory {
            base: None,
            index: None,
            rip_relative: false,
            absolute: Some(_),
            ..
        }
    );
    if exported_is_direct_memory {
        if let Some(rip_relative_operand) = display_template_rip_relative_memory(sub_state) {
            return Ok(rip_relative_operand.clone());
        }
        if let Some(relative_target) = display_template_relative_target(sub_state) {
            if let BoundOperand::Memory { size, .. } = exported_value {
                return Ok(BoundOperand::Memory {
                    base: None,
                    index: None,
                    scale: 1,
                    displacement: 0,
                    rip_relative: true,
                    absolute: Some(relative_target),
                    size,
                });
            }
        }
    }
    Ok(exported_value)
}

fn display_template_operand_indices(state: &RuntimeConstructState) -> Vec<usize> {
    if let Some(flowthru_index) = state.display_template.flowthru_operand_index {
        return vec![flowthru_index];
    }
    state
        .display_template
        .pieces
        .iter()
        .filter_map(|piece| match piece {
            crate::compiler::CompiledDisplayPiece::OperandRef(index) => Some(*index),
            crate::compiler::CompiledDisplayPiece::Literal(_) => None,
        })
        .collect()
}

fn display_template_rip_relative_memory(state: &RuntimeConstructState) -> Option<&BoundOperand> {
    display_template_operand_indices(state)
        .into_iter()
        .filter_map(|index| state.handles.get(index))
        .find_map(|handle| {
            if let Some(
                operand @ BoundOperand::Memory {
                    rip_relative: true, ..
                },
            ) = handle.debug_value.as_ref()
            {
                return Some(operand);
            }
            handle
                .subtable_state
                .as_deref()
                .and_then(display_template_rip_relative_memory)
        })
}

fn display_template_relative_target(state: &RuntimeConstructState) -> Option<u64> {
    display_template_operand_indices(state)
        .into_iter()
        .filter_map(|index| state.handles.get(index))
        .find_map(|handle| {
            if let Some(BoundOperand::Relative { target }) = handle.debug_value.as_ref() {
                return Some(*target);
            }
            handle
                .subtable_state
                .as_deref()
                .and_then(display_template_relative_target)
        })
}

fn display_value_from_handle(handle: &RuntimeHandle, role: &str) -> Result<BoundOperand> {
    if let Some(value) = handle.debug_value.clone() {
        return Ok(value);
    }
    bail!("{role} {} has no display debug value", handle.operand_index)
}

pub(super) fn flow_kind_for_state(state: &RuntimeConstructState) -> DecodedFlowKind {
    if state
        .constructor_template
        .ops
        .iter()
        .any(|op| matches!(op.opcode, CompiledOpTplOpcode::Return))
    {
        return DecodedFlowKind::Return;
    }
    if state
        .constructor_template
        .ops
        .iter()
        .any(|op| matches!(op.opcode, CompiledOpTplOpcode::Call))
    {
        return DecodedFlowKind::Call;
    }
    if state
        .constructor_template
        .ops
        .iter()
        .any(|op| matches!(op.opcode, CompiledOpTplOpcode::CBranch))
    {
        return DecodedFlowKind::ConditionalJump;
    }
    DecodedFlowKind::None
}

pub(super) fn disasm_mnemonic(state: &RuntimeConstructState) -> String {
    state.mnemonic.replace('^', "").to_ascii_lowercase()
}

pub(super) fn render_instruction_display(
    state: &RuntimeConstructState,
) -> Result<(String, String)> {
    if state.display_template.pieces.is_empty() {
        return Ok((
            disasm_mnemonic(state),
            state
                .operands
                .iter()
                .map(format_operand)
                .collect::<Vec<_>>()
                .join(", "),
        ));
    }

    let (mnemonic, body) = render_display_template_parts(state)?;
    let mnemonic = if mnemonic.is_empty() {
        disasm_mnemonic(state)
    } else {
        mnemonic.replace('^', "").to_ascii_lowercase()
    };
    Ok((mnemonic, body))
}

pub(super) fn render_display_template_parts(
    state: &RuntimeConstructState,
) -> Result<(String, String)> {
    if let Some(flow_index) = state.display_template.flowthru_operand_index {
        if let Some(child) = state
            .handles
            .get(flow_index)
            .and_then(|handle| handle.subtable_state.as_deref())
        {
            return render_display_template_parts(child);
        }
    }

    let split = display_template_split_index(state)?;
    let mnemonic = render_display_pieces(state, &state.display_template.pieces[..split])?;
    let body = if state.display_template.first_whitespace.is_some()
        && split < state.display_template.pieces.len()
    {
        render_display_pieces(state, &state.display_template.pieces[split + 1..])?
    } else {
        String::new()
    };
    Ok((mnemonic, body))
}

fn display_template_split_index(state: &RuntimeConstructState) -> Result<usize> {
    match state.display_template.first_whitespace {
        Some(index) if index <= state.display_template.pieces.len() => Ok(index),
        Some(index) => bail!(
            "display template first_whitespace {index} exceeds piece count {} for constructor {}",
            state.display_template.pieces.len(),
            state.constructor_id
        ),
        None => Ok(state.display_template.pieces.len()),
    }
}

pub(super) fn render_display_pieces(
    state: &RuntimeConstructState,
    pieces: &[crate::compiler::CompiledDisplayPiece],
) -> Result<String> {
    let mut rendered = String::new();
    for piece in pieces {
        match piece {
            crate::compiler::CompiledDisplayPiece::Literal(literal) => rendered.push_str(literal),
            crate::compiler::CompiledDisplayPiece::OperandRef(index) => {
                rendered.push_str(&render_operand_display(state, *index)?);
            }
        }
    }
    Ok(rendered)
}

pub(super) fn render_operand_display(
    state: &RuntimeConstructState,
    operand_index: usize,
) -> Result<String> {
    let Some(handle) = state.handles.get(operand_index) else {
        bail!(
            "display operand {operand_index} is missing for constructor {}",
            state.constructor_id
        );
    };
    if let Some(child) = handle.subtable_state.as_deref() {
        let (mnemonic, body) = render_display_template_parts(child)?;
        return Ok(if body.is_empty() {
            mnemonic
        } else {
            format!("{mnemonic} {body}")
        });
    }
    let display_kind = state
        .display_operands
        .get(operand_index)
        .map(|operand| &operand.kind);
    let value = display_value_from_handle(handle, "display operand")?;
    Ok(format_operand_with_display_kind(&value, display_kind))
}

pub(super) fn format_operand(operand: &BoundOperand) -> String {
    format_operand_with_display_kind(operand, None)
}

pub(super) fn format_operand_with_display_kind(
    operand: &BoundOperand,
    display_kind: Option<&crate::compiler::CompiledDisplayOperandKind>,
) -> String {
    if let Some(kind) = display_kind {
        match kind {
            crate::compiler::CompiledDisplayOperandKind::NameTable(names)
            | crate::compiler::CompiledDisplayOperandKind::VarnodeList(names) => {
                if let Some(index) = operand_display_index(operand) {
                    if let Some(name) = names.get(index) {
                        return name.clone();
                    }
                }
            }
            crate::compiler::CompiledDisplayOperandKind::ValueMap(values) => {
                if let Some(index) = operand_display_index(operand) {
                    if let Some(value) = values.get(index) {
                        return format_signed_hex(*value);
                    }
                }
            }
            crate::compiler::CompiledDisplayOperandKind::ValueHex => {
                if let Some(value) = operand_display_value(operand) {
                    return format_signed_hex(value);
                }
            }
            crate::compiler::CompiledDisplayOperandKind::Generic
            | crate::compiler::CompiledDisplayOperandKind::Subtable => {}
        }
    }

    match operand {
        BoundOperand::Register { index, size } => format!("reg{size}_{index}"),
        BoundOperand::NamedVarnode { name, .. } => name.clone(),
        BoundOperand::Immediate { value, .. } => format!("0x{value:x}"),
        BoundOperand::Relative { target } => format!("0x{target:x}"),
        BoundOperand::Memory {
            base,
            index,
            scale,
            displacement,
            rip_relative,
            ..
        } => {
            let base = base
                .map(|value| format!("reg8_{value}"))
                .unwrap_or_else(|| "none".to_string());
            let index = index
                .map(|value| format!("reg8_{value}"))
                .unwrap_or_else(|| "none".to_string());
            format!(
                "mem[base={base},index={index},scale={scale},disp={displacement},rip={rip_relative}]"
            )
        }
    }
}

pub(super) fn operand_display_index(operand: &BoundOperand) -> Option<usize> {
    match operand {
        BoundOperand::Immediate { value, .. } => Some(*value as usize),
        BoundOperand::Register { index, .. } => Some(*index as usize),
        BoundOperand::NamedVarnode { display_index, .. } => display_index.map(|idx| idx as usize),
        BoundOperand::Relative { target } => Some(*target as usize),
        BoundOperand::Memory {
            absolute,
            displacement,
            ..
        } => absolute
            .map(|value| value as usize)
            .or_else(|| usize::try_from(*displacement).ok()),
    }
}

pub(super) fn operand_display_value(operand: &BoundOperand) -> Option<i64> {
    match operand {
        BoundOperand::Immediate { value, .. } => Some(*value as i64),
        BoundOperand::Register { index, .. } => Some(i64::from(*index)),
        BoundOperand::NamedVarnode { display_index, .. } => display_index.map(i64::from),
        BoundOperand::Relative { target } => Some(*target as i64),
        BoundOperand::Memory {
            absolute,
            displacement,
            ..
        } => absolute.map(|value| value as i64).or(Some(*displacement)),
    }
}

pub(super) fn format_signed_hex(value: i64) -> String {
    if value >= 0 {
        format!("0x{:x}", value as u64)
    } else {
        format!("-0x{:x}", value.unsigned_abs())
    }
}

pub(super) fn decoded_references(
    address: u64,
    length: usize,
    flow_kind: DecodedFlowKind,
    handles: &[RuntimeHandle],
) -> Vec<DecodedReference> {
    let mut refs = Vec::new();
    for (operand_index, handle) in handles.iter().enumerate() {
        if let Some(reference) = handle.subtable_state.as_deref().and_then(|state| {
            reference_from_subtable_state(address, length, flow_kind, operand_index, state)
                .or_else(|| inst_next_relative_reference_from_handle(operand_index, handle, state))
        }) {
            refs.push(reference);
            continue;
        }
        let Some(operand) = handle.debug_value.as_ref() else {
            continue;
        };
        match operand {
            BoundOperand::Relative { target } => {
                let kind = match flow_kind {
                    DecodedFlowKind::Call => DecodedReferenceKind::CallTarget,
                    DecodedFlowKind::Jump | DecodedFlowKind::ConditionalJump => {
                        DecodedReferenceKind::BranchTarget
                    }
                    _ => continue,
                };
                refs.push(DecodedReference {
                    target: *target,
                    kind,
                    operand_index,
                });
            }
            BoundOperand::Memory {
                base,
                index,
                displacement,
                rip_relative,
                absolute,
                ..
            } => {
                let subtable_relative_target = handles
                    .get(operand_index)
                    .and_then(|handle| handle.subtable_state.as_deref())
                    .and_then(first_relative_target)
                    .or_else(|| {
                        absolute.and_then(|target| {
                            handles
                                .get(operand_index)
                                .and_then(|handle| handle.subtable_state.as_deref())
                                .and_then(|state| first_materialized_address_target(state, target))
                        })
                    })
                    .or_else(|| {
                        absolute.and_then(|target| {
                            handles.iter().find_map(|handle| {
                                let relative = handle
                                    .subtable_state
                                    .as_deref()
                                    .and_then(first_relative_target)?;
                                (relative == target).then_some(relative)
                            })
                        })
                    })
                    .or_else(|| {
                        absolute.and_then(|target| {
                            handles.iter().find_map(|handle| {
                                let state = handle.subtable_state.as_deref()?;
                                first_materialized_address_target(state, target)
                            })
                        })
                    });
                let is_rip_relative = *rip_relative || subtable_relative_target.is_some();
                let target = if is_rip_relative {
                    subtable_relative_target.or(*absolute).or_else(|| {
                        address
                            .checked_add(length as u64)
                            .and_then(|base| add_signed(base, *displacement))
                    })
                } else if *displacement > 0 {
                    Some(*displacement as u64)
                } else {
                    None
                };
                if let Some(target) = target {
                    let kind = if is_rip_relative {
                        DecodedReferenceKind::RipRelativeAddress
                    } else if base.is_none() && index.is_none() {
                        DecodedReferenceKind::MemoryAddress
                    } else {
                        DecodedReferenceKind::MemoryAddress
                    };
                    refs.push(DecodedReference {
                        target,
                        kind,
                        operand_index,
                    });
                }
            }
            BoundOperand::Immediate { value, .. } if *value != 0 => {
                refs.push(DecodedReference {
                    target: *value,
                    kind: DecodedReferenceKind::ImmediateAddress,
                    operand_index,
                });
            }
            _ => {}
        }
    }
    refs
}

fn inst_next_relative_reference_from_handle(
    operand_index: usize,
    handle: &RuntimeHandle,
    state: &RuntimeConstructState,
) -> Option<DecodedReference> {
    if !state_uses_inst_next_pattern_expression(state) {
        return None;
    }
    let target = match handle.debug_value.as_ref() {
        Some(BoundOperand::Immediate { value, .. }) => Some(*value),
        Some(BoundOperand::Memory { absolute, .. }) => *absolute,
        _ => handle
            .fixed
            .offset_space
            .is_none()
            .then_some(handle.fixed.offset_offset),
    }?;
    Some(DecodedReference {
        target,
        kind: DecodedReferenceKind::RipRelativeAddress,
        operand_index,
    })
}

fn reference_from_subtable_state(
    address: u64,
    length: usize,
    flow_kind: DecodedFlowKind,
    operand_index: usize,
    state: &RuntimeConstructState,
) -> Option<DecodedReference> {
    if let Some(BoundOperand::Memory {
        displacement,
        absolute,
        ..
    }) = first_rip_relative_memory(state)
    {
        let target = absolute.or_else(|| {
            address
                .checked_add(length as u64)
                .and_then(|base| add_signed(base, *displacement))
        })?;
        return Some(DecodedReference {
            target,
            kind: DecodedReferenceKind::RipRelativeAddress,
            operand_index,
        });
    }

    let target = first_relative_target(state)?;
    let kind = match flow_kind {
        DecodedFlowKind::Call => DecodedReferenceKind::CallTarget,
        DecodedFlowKind::Jump | DecodedFlowKind::ConditionalJump => {
            DecodedReferenceKind::BranchTarget
        }
        _ => DecodedReferenceKind::RipRelativeAddress,
    };
    Some(DecodedReference {
        target,
        kind,
        operand_index,
    })
}

fn state_uses_inst_next_pattern_expression(state: &RuntimeConstructState) -> bool {
    state.handles.iter().any(|handle| {
        operand_spec_uses_inst_next_pattern_expression(&handle.spec)
            || handle
                .subtable_state
                .as_deref()
                .is_some_and(state_uses_inst_next_pattern_expression)
    })
}

fn operand_spec_uses_inst_next_pattern_expression(spec: &CompiledOperandSpec) -> bool {
    match spec {
        CompiledOperandSpec::SlaVarnodeListExpression { expr, .. }
        | CompiledOperandSpec::SlaValueMapExpression { expr, .. }
        | CompiledOperandSpec::SlaPatternExpression { expr, .. } => {
            pattern_expression_uses_inst_next(expr)
        }
        _ => false,
    }
}

fn pattern_expression_uses_inst_next(expr: &CompiledPatternExpression) -> bool {
    match expr {
        CompiledPatternExpression::InstNext => true,
        CompiledPatternExpression::Add(lhs, rhs)
        | CompiledPatternExpression::Sub(lhs, rhs)
        | CompiledPatternExpression::Mul(lhs, rhs)
        | CompiledPatternExpression::Div(lhs, rhs)
        | CompiledPatternExpression::LeftShift(lhs, rhs)
        | CompiledPatternExpression::RightShift(lhs, rhs)
        | CompiledPatternExpression::And(lhs, rhs)
        | CompiledPatternExpression::Or(lhs, rhs)
        | CompiledPatternExpression::Xor(lhs, rhs) => {
            pattern_expression_uses_inst_next(lhs) || pattern_expression_uses_inst_next(rhs)
        }
        CompiledPatternExpression::Negate(inner) | CompiledPatternExpression::Not(inner) => {
            pattern_expression_uses_inst_next(inner)
        }
        _ => false,
    }
}

pub(super) fn first_rip_relative_memory(state: &RuntimeConstructState) -> Option<&BoundOperand> {
    state
        .operands
        .iter()
        .find(|operand| {
            matches!(
                operand,
                BoundOperand::Memory {
                    rip_relative: true,
                    ..
                }
            )
        })
        .or_else(|| {
            state.handles.iter().find_map(|handle| {
                handle
                    .subtable_state
                    .as_deref()
                    .and_then(first_rip_relative_memory)
            })
        })
}

pub(super) fn first_relative_target(state: &RuntimeConstructState) -> Option<u64> {
    state
        .operands
        .iter()
        .find_map(|operand| match operand {
            BoundOperand::Relative { target } => Some(*target),
            _ => None,
        })
        .or_else(|| {
            state.handles.iter().find_map(|handle| {
                handle
                    .subtable_state
                    .as_deref()
                    .and_then(first_relative_target)
            })
        })
}

fn first_materialized_address_target(state: &RuntimeConstructState, target: u64) -> Option<u64> {
    state
        .operands
        .iter()
        .find_map(|operand| match operand {
            BoundOperand::Immediate { value, .. } if *value == target => Some(target),
            BoundOperand::Memory {
                absolute: Some(value),
                ..
            } if *value == target => Some(target),
            _ => None,
        })
        .or_else(|| {
            state.handles.iter().find_map(|handle| {
                handle
                    .subtable_state
                    .as_deref()
                    .and_then(|state| first_materialized_address_target(state, target))
            })
        })
}

pub(super) fn add_signed(base: u64, delta: i64) -> Option<u64> {
    if delta >= 0 {
        base.checked_add(delta as u64)
    } else {
        base.checked_sub(delta.unsigned_abs())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::{CompiledConstructorTemplate, CompiledDisplayTemplate};

    #[test]
    fn display_renderer_has_no_zero_bound_operand_fallback() {
        let source = include_str!("display.rs");
        let dummy_immediate_fallback = ["unwrap_or", "(BoundOperand::Immediate"].concat();
        let dummy_zero_size = ["encoded_size: ", "0"].concat();
        let fixed_handle_fallback = ["bound_operand", "from_fixed_handle"].join("_");
        let whitespace_len_fallback = "first_whitespace\n        .unwrap_or";

        assert!(
            !source.contains(&dummy_immediate_fallback),
            "display rendering must fail on unresolved handles instead of inventing dummy immediates"
        );
        assert!(
            !source.contains(&dummy_zero_size),
            "display rendering must not materialize zero-size dummy operands"
        );
        assert!(
            !source.contains(&fixed_handle_fallback),
            "display rendering must use decoded debug operands, not fixed-handle BoundOperand fallback"
        );
        assert!(
            !source.contains(whitespace_len_fallback),
            "display rendering must validate first_whitespace instead of silently slicing at pieces.len()"
        );
    }

    #[test]
    fn display_renderer_has_no_decoded_cc_mnemonic_table() {
        let source = include_str!("display.rs");
        let cc_mnemonic_symbol = ["jcc", "_mnemonic"].concat();
        let decoded_cc_field = ["condition", "_code"].concat();
        assert!(
            !source.contains(&cc_mnemonic_symbol),
            "display rendering must come from SLEIGH display templates, not a condition-code table"
        );
        assert!(
            !source.contains(&decoded_cc_field),
            "display rendering must not carry decoded condition-code side channels"
        );
    }

    #[test]
    fn exported_direct_memory_display_uses_referenced_display_operand_only() {
        let exported = handle(BoundOperand::Memory {
            base: None,
            index: None,
            scale: 1,
            displacement: 0x3000,
            rip_relative: false,
            absolute: Some(0x3000),
            size: 8,
        });
        let unreferenced_rip = handle(BoundOperand::Memory {
            base: None,
            index: None,
            scale: 1,
            displacement: 4,
            rip_relative: true,
            absolute: Some(0x2000),
            size: 8,
        });
        let referenced_relative = handle(BoundOperand::Relative { target: 0x3000 });
        let state = state_with_display(
            vec![
                crate::compiler::CompiledDisplayPiece::OperandRef(1),
                crate::compiler::CompiledDisplayPiece::Literal(" ".to_string()),
            ],
            vec![unreferenced_rip, referenced_relative],
        );

        let value =
            display_value_for_exported_handle(&exported, &state).expect("exported display value");

        assert!(matches!(
            value,
            BoundOperand::Memory {
                rip_relative: true,
                absolute: Some(0x3000),
                ..
            }
        ));
    }

    #[test]
    fn exported_direct_memory_display_uses_referenced_child_display_operand() {
        let exported = handle(BoundOperand::Memory {
            base: None,
            index: None,
            scale: 1,
            displacement: 0x4000,
            rip_relative: false,
            absolute: Some(0x4000),
            size: 8,
        });
        let child = state_with_display(
            vec![crate::compiler::CompiledDisplayPiece::OperandRef(0)],
            vec![handle(BoundOperand::Memory {
                base: None,
                index: None,
                scale: 1,
                displacement: -8,
                rip_relative: true,
                absolute: Some(0x4000),
                size: 8,
            })],
        );
        let parent_operand = RuntimeHandle {
            subtable_state: Some(Box::new(child)),
            ..handle(BoundOperand::Immediate {
                value: 0,
                encoded_size: 1,
                signed: false,
            })
        };
        let state = state_with_display(
            vec![crate::compiler::CompiledDisplayPiece::OperandRef(0)],
            vec![parent_operand],
        );

        let value =
            display_value_for_exported_handle(&exported, &state).expect("exported display value");

        assert!(matches!(
            value,
            BoundOperand::Memory {
                rip_relative: true,
                absolute: Some(0x4000),
                ..
            }
        ));
    }

    #[test]
    fn rip_relative_reference_uses_checked_instruction_end_address() {
        let refs = decoded_references(
            0x1000,
            4,
            DecodedFlowKind::None,
            &[handle(BoundOperand::Memory {
                base: None,
                index: None,
                scale: 1,
                displacement: -4,
                rip_relative: true,
                absolute: None,
                size: 8,
            })],
        );

        assert_eq!(
            refs,
            vec![DecodedReference {
                target: 0x1000,
                kind: DecodedReferenceKind::RipRelativeAddress,
                operand_index: 0,
            }]
        );
    }

    #[test]
    fn rip_relative_reference_does_not_saturate_on_overflow() {
        let refs = decoded_references(
            u64::MAX - 1,
            4,
            DecodedFlowKind::None,
            &[handle(BoundOperand::Memory {
                base: None,
                index: None,
                scale: 1,
                displacement: 8,
                rip_relative: true,
                absolute: None,
                size: 8,
            })],
        );

        assert!(
            refs.is_empty(),
            "overflowing RIP-relative reference must not be clipped into a false target"
        );
    }

    #[test]
    fn display_template_rejects_out_of_range_first_whitespace() {
        let mut state = state_with_display(
            vec![crate::compiler::CompiledDisplayPiece::Literal(
                "nop".to_string(),
            )],
            Vec::new(),
        );
        state.display_template.first_whitespace = Some(2);

        let err = render_display_template_parts(&state).expect_err("invalid split should fail");

        assert!(
            err.to_string().contains("first_whitespace"),
            "unexpected display error: {err:#}"
        );
    }

    fn handle(value: BoundOperand) -> RuntimeHandle {
        RuntimeHandle {
            operand_index: 0,
            spec: CompiledOperandSpec::SubtableEvaluation {
                table_name: "test".to_string(),
                reloffset: 0,
                offsetbase: -1,
            },
            fixed: RuntimeFixedHandle::default(),
            debug_value: Some(value),
            subtable_state: None,
        }
    }

    fn state_with_display(
        pieces: Vec<crate::compiler::CompiledDisplayPiece>,
        mut handles: Vec<RuntimeHandle>,
    ) -> RuntimeConstructState {
        for (index, handle) in handles.iter_mut().enumerate() {
            handle.operand_index = index;
        }
        RuntimeConstructState {
            subtable_id: 0,
            constructor_id: 0,
            constructor_slot: 0,
            mnemonic: "test".to_string(),
            construct_tpl_kind: CompiledConstructTplKind::Generic,
            constructor_template: CompiledConstructorTemplate {
                handles: Vec::new(),
                decode_steps: Vec::new(),
                num_labels: 0,
                result: None,
                ops: Vec::new(),
                template_source: CompiledTemplateSource::SpecDerived,
            },
            named_templates: Vec::new(),
            context_commits: Vec::new(),
            display_template: CompiledDisplayTemplate {
                constructor_hash: 0,
                pieces,
                first_whitespace: None,
                flowthru_operand_index: None,
                display: String::new(),
            },
            display_operands: Vec::new(),
            construct_nodes: Vec::new(),
            operands: handles
                .iter()
                .filter_map(|handle| handle.debug_value.clone())
                .collect(),
            handles,
            exported_handle: None,
            context_register: 0,
            context_known_mask: 0,
            absolute_offset: 0,
            relative_length: 0,
            length: 0,
            match_trace: spine::RuntimeMatchTrace {
                root_bucket: "test".to_string(),
                probes: Vec::new(),
                leaf_constructor_indexes: Vec::new(),
                matched_leaf_pattern: None,
            },
        }
    }
}
