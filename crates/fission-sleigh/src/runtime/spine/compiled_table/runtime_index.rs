use super::*;

pub(super) fn constructor_template_handle_reference_bitmap(
    template: &crate::compiler::CompiledConstructorTemplate,
) -> Vec<bool> {
    let mut refs = vec![false; template.handles.len()];
    if let Some(handle) = &template.result {
        mark_handle_tpl_references(handle, &mut refs);
    }
    for op in &template.ops {
        mark_op_references(op, &mut refs);
    }
    refs
}

pub(super) fn constructor_template_references_handle(bitmap: &[bool], handle_index: usize) -> bool {
    bitmap.get(handle_index).copied().unwrap_or(false)
}

fn mark_op_references(op: &CompiledOpTpl, refs: &mut [bool]) {
    if let Some(output) = &op.output {
        mark_varnode_tpl_references(output, refs);
    }
    for input in &op.inputs {
        mark_varnode_tpl_references(input, refs);
    }
}

fn mark_varnode_tpl_references(varnode: &CompiledVarnodeTpl, refs: &mut [bool]) {
    match varnode {
        CompiledVarnodeTpl::Varnode {
            space,
            offset,
            size,
        } => {
            mark_space_tpl_references(space, refs);
            mark_const_tpl_references(offset, refs);
            mark_const_tpl_references(size, refs);
        }
        CompiledVarnodeTpl::HandleTpl(handle) => mark_handle_tpl_references(handle, refs),
    }
}

fn mark_handle_tpl_references(handle: &CompiledHandleTpl, refs: &mut [bool]) {
    if let Some(space) = &handle.space {
        mark_space_tpl_references(space, refs);
    }
    if let Some(value) = &handle.size {
        mark_const_tpl_references(value, refs);
    }
    if let Some(space) = &handle.ptr_space {
        mark_space_tpl_references(space, refs);
    }
    if let Some(value) = &handle.ptr_offset {
        mark_const_tpl_references(value, refs);
    }
    if let Some(value) = &handle.ptr_size {
        mark_const_tpl_references(value, refs);
    }
    if let Some(space) = &handle.temp_space {
        mark_space_tpl_references(space, refs);
    }
    if let Some(value) = &handle.temp_offset {
        mark_const_tpl_references(value, refs);
    }
}

fn mark_space_tpl_references(space: &CompiledSpaceTpl, refs: &mut [bool]) {
    match space {
        CompiledSpaceTpl::SpaceRef(_) => {}
        CompiledSpaceTpl::Const(value) => mark_const_tpl_references(value, refs),
    }
}

fn mark_const_tpl_references(value: &CompiledConstTpl, refs: &mut [bool]) {
    if let CompiledConstTpl::Handle { handle_index, .. } = value {
        if *handle_index >= 0 {
            if let Some(reference) = refs.get_mut(*handle_index as usize) {
                *reference = true;
            }
        }
    }
}
