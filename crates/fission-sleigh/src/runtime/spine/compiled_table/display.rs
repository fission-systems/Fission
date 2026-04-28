pub(super) fn display_value_for_exported_handle(
    exported: &RuntimeHandle,
    sub_state: &RuntimeConstructState,
) -> BoundOperand {
    let exported_is_direct_memory = matches!(
        exported.value,
        BoundOperand::Memory {
            base: None,
            index: None,
            rip_relative: false,
            absolute: Some(_),
            ..
        }
    );
    if exported_is_direct_memory {
        if let Some(rip_relative_operand) = first_rip_relative_memory(sub_state) {
            return rip_relative_operand.clone();
        }
        if let Some(relative_target) = first_relative_target(sub_state) {
            if let BoundOperand::Memory { size, .. } = exported.value {
                return BoundOperand::Memory {
                    base: None,
                    index: None,
                    scale: 1,
                    displacement: 0,
                    rip_relative: true,
                    absolute: Some(relative_target),
                    size,
                };
            }
        }
    }
    exported.value.clone()
}

pub(super) fn flow_kind_for(kind: CompiledConstructTplKind) -> DecodedFlowKind {
    match kind {
        CompiledConstructTplKind::Unsupported => DecodedFlowKind::None,
        CompiledConstructTplKind::Call => DecodedFlowKind::Call,
        CompiledConstructTplKind::Jmp => DecodedFlowKind::Jump,
        CompiledConstructTplKind::Jcc => DecodedFlowKind::ConditionalJump,
        CompiledConstructTplKind::Ret => DecodedFlowKind::Return,
        _ => DecodedFlowKind::None,
    }
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
    flow_kind_for(state.construct_tpl_kind)
}

pub(super) fn disasm_mnemonic(state: &RuntimeConstructState) -> String {
    // Final rendering must come from SLEIGH display templates. Until that
    // template IR is executable, keep condition-code rendering isolated to
    // the display-only Jcc compatibility holdout. This must not affect p-code
    // template execution.
    if matches!(state.construct_tpl_kind, CompiledConstructTplKind::Jcc) {
        if let Some(cc) = state.condition_code {
            if let Some(mnemonic) = jcc_mnemonic(cc) {
                return mnemonic.to_string();
            }
        }
    }
    state.mnemonic.replace('^', "").to_ascii_lowercase()
}

pub(super) fn render_instruction_display(state: &RuntimeConstructState) -> (String, String) {
    if state.display_template.pieces.is_empty() {
        return (
            disasm_mnemonic(state),
            state
                .operands
                .iter()
                .map(format_operand)
                .collect::<Vec<_>>()
                .join(", "),
        );
    }

    let (mnemonic, body) = render_display_template_parts(state);
    let mnemonic = if mnemonic.is_empty() {
        disasm_mnemonic(state)
    } else {
        mnemonic.replace('^', "").to_ascii_lowercase()
    };
    (mnemonic, body)
}

pub(super) fn render_display_template_parts(state: &RuntimeConstructState) -> (String, String) {
    if let Some(flow_index) = state.display_template.flowthru_operand_index {
        if let Some(child) = state
            .handles
            .get(flow_index)
            .and_then(|handle| handle.subtable_state.as_deref())
        {
            return render_display_template_parts(child);
        }
    }

    let split = state
        .display_template
        .first_whitespace
        .unwrap_or(state.display_template.pieces.len());
    let mnemonic = render_display_pieces(state, &state.display_template.pieces[..split]);
    let body = if state.display_template.first_whitespace.is_some() && split < state.display_template.pieces.len() {
        render_display_pieces(state, &state.display_template.pieces[split + 1..])
    } else {
        String::new()
    };
    (mnemonic, body)
}

pub(super) fn render_display_pieces(
    state: &RuntimeConstructState,
    pieces: &[crate::compiler::CompiledDisplayPiece],
) -> String {
    let mut rendered = String::new();
    for piece in pieces {
        match piece {
            crate::compiler::CompiledDisplayPiece::Literal(literal) => rendered.push_str(literal),
            crate::compiler::CompiledDisplayPiece::OperandRef(index) => {
                rendered.push_str(&render_operand_display(state, *index));
            }
        }
    }
    rendered
}

pub(super) fn render_operand_display(state: &RuntimeConstructState, operand_index: usize) -> String {
    let Some(handle) = state.handles.get(operand_index) else {
        return String::new();
    };
    if let Some(child) = handle.subtable_state.as_deref() {
        let (mnemonic, body) = render_display_template_parts(child);
        return if body.is_empty() {
            mnemonic
        } else {
            format!("{mnemonic} {body}")
        };
    }
    let display_kind = state
        .display_operands
        .get(operand_index)
        .map(|operand| &operand.kind);
    format_operand_with_display_kind(&handle.value, display_kind)
}

pub(super) fn jcc_mnemonic(cc: u8) -> Option<&'static str> {
    Some(match cc {
        0 => "jo",
        1 => "jno",
        2 => "jb",
        3 => "jnb",
        4 => "jz",
        5 => "jnz",
        6 => "jbe",
        7 => "ja",
        8 => "js",
        9 => "jns",
        10 => "jp",
        11 => "jnp",
        12 => "jl",
        13 => "jge",
        14 => "jle",
        15 => "jg",
        _ => return None,
    })
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
        BoundOperand::Memory { absolute, displacement, .. } => {
            absolute.map(|value| value as usize).or_else(|| usize::try_from(*displacement).ok())
        }
    }
}

pub(super) fn operand_display_value(operand: &BoundOperand) -> Option<i64> {
    match operand {
        BoundOperand::Immediate { value, .. } => Some(*value as i64),
        BoundOperand::Register { index, .. } => Some(i64::from(*index)),
        BoundOperand::NamedVarnode { display_index, .. } => display_index.map(i64::from),
        BoundOperand::Relative { target } => Some(*target as i64),
        BoundOperand::Memory { absolute, displacement, .. } => {
            absolute.map(|value| value as i64).or(Some(*displacement))
        }
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
    operands: &[BoundOperand],
    handles: &[RuntimeHandle],
) -> Vec<DecodedReference> {
    let mut refs = Vec::new();
    for (operand_index, operand) in operands.iter().enumerate() {
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
                            handles.iter().find_map(|handle| {
                                let relative = handle
                                    .subtable_state
                                    .as_deref()
                                    .and_then(first_relative_target)?;
                                (relative == target).then_some(relative)
                            })
                        })
                    });
                let is_rip_relative = *rip_relative || subtable_relative_target.is_some();
                let target = if is_rip_relative {
                    subtable_relative_target.or(*absolute).or_else(|| Some(add_signed(
                        address.saturating_add(length as u64),
                        *displacement,
                    )))
                } else if *rip_relative {
                    absolute.or_else(|| Some(add_signed(
                        address.saturating_add(length as u64),
                        *displacement,
                    )))
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

pub(super) fn first_rip_relative_memory(state: &RuntimeConstructState) -> Option<&BoundOperand> {
    state
        .operands
        .iter()
        .find(|operand| matches!(operand, BoundOperand::Memory { rip_relative: true, .. }))
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

pub(super) fn add_signed(base: u64, delta: i64) -> u64 {
    if delta >= 0 {
        base.saturating_add(delta as u64)
    } else {
        base.saturating_sub(delta.unsigned_abs())
    }
}
