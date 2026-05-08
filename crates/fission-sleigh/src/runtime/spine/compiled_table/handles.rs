use super::*;

pub(super) fn compiled_space(name: &str, index: u64) -> CompiledSpaceRef {
    CompiledSpaceRef {
        name: name.to_string(),
        index,
        word_size: 0,
        addr_size: 0,
    }
}

pub(super) fn fixed_handle_for_const_value(value: u64, size: u32) -> RuntimeFixedHandle {
    RuntimeFixedHandle {
        space: Some(compiled_space("const", 0)),
        size,
        offset_space: None,
        offset_offset: value,
        offset_size: size,
        temp_space: None,
        temp_offset: 0,
        fixable: true,
    }
}

pub(super) fn fixed_handle_for_ram_target(target: u64, size: u32) -> RuntimeFixedHandle {
    RuntimeFixedHandle {
        space: Some(compiled_space("ram", 3)),
        size,
        offset_space: None,
        offset_offset: target,
        offset_size: size,
        temp_space: None,
        temp_offset: 0,
        fixable: true,
    }
}

pub(super) fn fixed_handle_from_resolved_varnode(
    varnode: &crate::compiler::CompiledResolvedVarnode,
) -> RuntimeFixedHandle {
    RuntimeFixedHandle {
        space: Some(varnode.space.clone()),
        size: varnode.size,
        offset_space: None,
        offset_offset: varnode.offset,
        offset_size: varnode.size,
        temp_space: None,
        temp_offset: 0,
        fixable: true,
    }
}

pub(super) fn display_operand_from_exported_fixed_handle(
    handle: &RuntimeFixedHandle,
) -> Result<BoundOperand> {
    let space = handle
        .space
        .as_ref()
        .ok_or_else(|| anyhow!("exported fixed handle missing space"))?;
    if space.index == 0 || space.name == "const" {
        return Ok(BoundOperand::Immediate {
            value: handle.offset_offset,
            encoded_size: handle.size.max(1),
            signed: false,
        });
    }
    if space.name == "register" || space.index == 4 {
        return Ok(BoundOperand::NamedVarnode {
            name: format!("register_{:x}", handle.offset_offset),
            display_index: None,
            size: handle.size,
        });
    }
    Ok(BoundOperand::Memory {
        base: None,
        index: None,
        scale: 1,
        displacement: handle.offset_offset as i64,
        rip_relative: false,
        absolute: Some(handle.offset_offset),
        size: handle.size,
    })
}

pub(super) fn varnode_from_fixed_handle(handle: &RuntimeFixedHandle) -> Result<Varnode> {
    if handle.offset_space.is_some() {
        bail!("dynamic fixed handle cannot materialize into a direct varnode");
    }
    let space = handle
        .space
        .as_ref()
        .ok_or_else(|| anyhow!("fixed handle missing space"))?;
    let size = if handle.size > 0 {
        handle.size
    } else {
        handle.offset_size
    };
    if space.name == "const" {
        Ok(Varnode::constant(handle.offset_offset as i64, size))
    } else {
        Ok(Varnode {
            space_id: space.index,
            offset: handle.offset_offset,
            size,
            is_constant: false,
            constant_val: 0,
        })
    }
}

pub(super) fn handle_selector_index_in_space(
    space: &CompiledSpaceTpl,
    selector: CompiledHandleSelector,
) -> Option<usize> {
    let CompiledSpaceTpl::Const(const_tpl) = space else {
        return None;
    };
    handle_selector_index(const_tpl, selector)
}

pub(super) fn negative_handle_selector_index_in_space(
    space: &CompiledSpaceTpl,
    selector: CompiledHandleSelector,
) -> Option<i64> {
    let CompiledSpaceTpl::Const(const_tpl) = space else {
        return None;
    };
    let CompiledConstTpl::Handle {
        handle_index,
        selector: actual_selector,
        plus,
    } = const_tpl.as_ref()
    else {
        return None;
    };
    if *actual_selector == selector && plus.is_none() && *handle_index < 0 {
        Some(*handle_index)
    } else {
        None
    }
}

pub(super) fn matches_handle_selector(
    const_tpl: &CompiledConstTpl,
    handle_index: usize,
    selector: CompiledHandleSelector,
) -> bool {
    handle_selector_index(const_tpl, selector).is_some_and(|idx| idx == handle_index)
}

pub(super) fn handle_selector_index(
    const_tpl: &CompiledConstTpl,
    expected_selector: CompiledHandleSelector,
) -> Option<usize> {
    let CompiledConstTpl::Handle {
        handle_index,
        selector,
        plus,
    } = const_tpl
    else {
        return None;
    };
    if *selector != expected_selector || plus.is_some() || *handle_index < 0 {
        return None;
    }
    Some(*handle_index as usize)
}

pub(super) fn matches_negative_handle_selector(
    const_tpl: &CompiledConstTpl,
    handle_index: i64,
    expected_selector: CompiledHandleSelector,
) -> bool {
    let CompiledConstTpl::Handle {
        handle_index: actual_handle_index,
        selector,
        plus,
    } = const_tpl
    else {
        return false;
    };
    *actual_handle_index == handle_index && *selector == expected_selector && plus.is_none()
}
