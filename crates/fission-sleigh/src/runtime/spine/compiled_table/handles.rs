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
    usize::try_from(*handle_index).ok()
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

pub(super) fn checked_handle_index(handle_index: i64, role: &str) -> Result<usize> {
    if handle_index < 0 {
        bail!("{role} handle index {handle_index} is negative");
    }
    usize::try_from(handle_index)
        .map_err(|_| anyhow!("{role} handle index {handle_index} does not fit usize"))
}

pub(super) fn build_operand_section_index(operand_index: usize) -> Result<i32> {
    i32::try_from(operand_index)
        .map_err(|_| anyhow!("BUILD operand index {operand_index} exceeds i32"))
}

pub(super) fn exported_build_handle_key(operand_index: usize) -> Result<i64> {
    let index = i64::try_from(operand_index)
        .map_err(|_| anyhow!("BUILD operand index {operand_index} exceeds i64"))?;
    index
        .checked_add(1)
        .and_then(i64::checked_neg)
        .ok_or_else(|| anyhow!("BUILD operand index {operand_index} cannot form handle key"))
}

#[cfg(test)]
mod tests {
    use super::{build_operand_section_index, checked_handle_index, exported_build_handle_key};

    #[test]
    fn handle_index_helpers_fail_closed_on_invalid_values() {
        assert_eq!(checked_handle_index(3, "test").unwrap(), 3);
        assert!(checked_handle_index(-1, "test").is_err());

        assert_eq!(build_operand_section_index(7).unwrap(), 7);
        assert!(build_operand_section_index(i32::MAX as usize + 1).is_err());

        assert_eq!(exported_build_handle_key(0).unwrap(), -1);
        assert_eq!(exported_build_handle_key(2).unwrap(), -3);
    }
}
