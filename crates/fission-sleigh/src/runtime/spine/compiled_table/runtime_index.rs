use super::*;

pub(super) fn constructor_template_handle_reference_bitmap(
    template: &crate::compiler::CompiledConstructorTemplate,
    refs: &mut Vec<bool>,
) -> Result<()> {
    refs.clear();
    refs.resize(template.handles.len(), false);
    if let Some(handle) = &template.result {
        mark_handle_tpl_references(handle, refs)?;
    }
    for op in &template.ops {
        mark_op_references(op, refs)?;
    }
    Ok(())
}

pub(super) fn constructor_template_references_handle(
    bitmap: &[bool],
    handle_index: usize,
) -> Result<bool> {
    bitmap
        .get(handle_index)
        .copied()
        .ok_or_else(|| anyhow!("construct_tpl references missing operand handle {handle_index}"))
}

fn mark_op_references(op: &CompiledOpTpl, refs: &mut [bool]) -> Result<()> {
    if let Some(output) = &op.output {
        mark_varnode_tpl_references(output, refs)?;
    }
    for input in &op.inputs {
        mark_varnode_tpl_references(input, refs)?;
    }
    Ok(())
}

fn mark_varnode_tpl_references(varnode: &CompiledVarnodeTpl, refs: &mut [bool]) -> Result<()> {
    match varnode {
        CompiledVarnodeTpl::Varnode {
            space,
            offset,
            size,
        } => {
            mark_space_tpl_references(space, refs)?;
            mark_const_tpl_references(offset, refs)?;
            mark_const_tpl_references(size, refs)?;
        }
        CompiledVarnodeTpl::HandleTpl(handle) => mark_handle_tpl_references(handle, refs)?,
    }
    Ok(())
}

fn mark_handle_tpl_references(handle: &CompiledHandleTpl, refs: &mut [bool]) -> Result<()> {
    if let Some(space) = &handle.space {
        mark_space_tpl_references(space, refs)?;
    }
    if let Some(value) = &handle.size {
        mark_const_tpl_references(value, refs)?;
    }
    if let Some(space) = &handle.ptr_space {
        mark_space_tpl_references(space, refs)?;
    }
    if let Some(value) = &handle.ptr_offset {
        mark_const_tpl_references(value, refs)?;
    }
    if let Some(value) = &handle.ptr_size {
        mark_const_tpl_references(value, refs)?;
    }
    if let Some(space) = &handle.temp_space {
        mark_space_tpl_references(space, refs)?;
    }
    if let Some(value) = &handle.temp_offset {
        mark_const_tpl_references(value, refs)?;
    }
    Ok(())
}

fn mark_space_tpl_references(space: &CompiledSpaceTpl, refs: &mut [bool]) -> Result<()> {
    match space {
        CompiledSpaceTpl::SpaceRef(_) => {}
        CompiledSpaceTpl::Const(value) => mark_const_tpl_references(value, refs)?,
    }
    Ok(())
}

fn mark_const_tpl_references(value: &CompiledConstTpl, refs: &mut [bool]) -> Result<()> {
    if let CompiledConstTpl::Handle { handle_index, .. } = value {
        let index = usize::try_from(*handle_index).map_err(|_| {
            anyhow!("construct_tpl references negative operand handle {handle_index}")
        })?;
        let reference = refs
            .get_mut(index)
            .ok_or_else(|| anyhow!("construct_tpl references missing operand handle {index}"))?;
        *reference = true;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_reference_bitmap_rejects_out_of_range_handle_indices() {
        let template = crate::compiler::CompiledConstructorTemplate {
            handles: Vec::new(),
            decode_steps: Vec::new(),
            num_labels: 0,
            result: Some(CompiledHandleTpl {
                space: None,
                size: Some(CompiledConstTpl::Handle {
                    handle_index: 0,
                    selector: CompiledHandleSelector::Size,
                    plus: None,
                }),
                ptr_space: None,
                ptr_offset: None,
                ptr_size: None,
                temp_space: None,
                temp_offset: None,
            }),
            ops: Vec::new(),
            template_source: CompiledTemplateSource::SpecDerived,
        };

        let mut refs = Vec::new();
        let error = constructor_template_handle_reference_bitmap(&template, &mut refs)
            .expect_err("missing operand handle must fail closed");
        assert!(
            error
                .to_string()
                .contains("construct_tpl references missing operand handle 0"),
            "{error:#}"
        );
    }

    #[test]
    fn handle_reference_query_rejects_out_of_range_operand_indices() {
        let error = constructor_template_references_handle(&[], 0)
            .expect_err("missing bitmap entry must fail closed");
        assert!(
            error
                .to_string()
                .contains("construct_tpl references missing operand handle 0"),
            "{error:#}"
        );
    }

    #[test]
    fn handle_reference_bitmap_rejects_negative_handle_indices() {
        let template = crate::compiler::CompiledConstructorTemplate {
            handles: Vec::new(),
            decode_steps: Vec::new(),
            num_labels: 0,
            result: Some(CompiledHandleTpl {
                space: None,
                size: Some(CompiledConstTpl::Handle {
                    handle_index: -1,
                    selector: CompiledHandleSelector::Size,
                    plus: None,
                }),
                ptr_space: None,
                ptr_offset: None,
                ptr_size: None,
                temp_space: None,
                temp_offset: None,
            }),
            ops: Vec::new(),
            template_source: CompiledTemplateSource::SpecDerived,
        };

        let mut refs = Vec::new();
        let error = constructor_template_handle_reference_bitmap(&template, &mut refs)
            .expect_err("negative operand handle must fail closed");
        assert!(
            error
                .to_string()
                .contains("construct_tpl references negative operand handle -1"),
            "{error:#}"
        );
    }
}
