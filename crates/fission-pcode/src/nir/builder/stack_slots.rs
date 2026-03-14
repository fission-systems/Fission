use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn register_param(&mut self, vn: &Varnode) -> Option<String> {
        if vn.space_id != REGISTER_SPACE_ID {
            return None;
        }
        let (name, index) = register_name_with_param(vn.offset, vn.size)?;
        if let Some(index) = index {
            self.params.entry(index).or_insert_with(|| NirBinding {
                name: name.to_string(),
                ty: type_from_size(vn.size, false),
                surface_type_name: None,
                initializer: None,
            });
        }
        Some(name.to_string())
    }

    pub(super) fn try_stack_slot_lvalue(
        &mut self,
        ptr: &Varnode,
        ty: NirType,
    ) -> Option<(String, NirType)> {
        let (base, offset) = self.resolve_stack_address(ptr)?;
        let kind_name = match base {
            StackBase::Rbp if offset > 0 => format!("param_{:x}", offset),
            StackBase::Rbp => format!("local_{:x}", offset.unsigned_abs()),
            StackBase::Rsp => format!("local_{:x}", offset.unsigned_abs()),
        };

        let entry = self.locals.entry(offset).or_insert_with(|| {
            let id = self.locals_next_id;
            self.locals_next_id += 1;
            StackSlot {
                id,
                name: kind_name.clone(),
                ty: ty.clone(),
            }
        });
        if entry.ty == NirType::Unknown {
            entry.ty = ty.clone();
        }
        Some((entry.name.clone(), entry.ty.clone()))
    }

    pub(super) fn resolve_stack_address(&self, ptr: &Varnode) -> Option<(StackBase, i64)> {
        self.resolve_stack_address_inner(ptr, &mut HashSet::new())
    }

    fn resolve_stack_address_inner(
        &self,
        ptr: &Varnode,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Option<(StackBase, i64)> {
        if ptr.space_id == REGISTER_SPACE_ID {
            return match ptr.offset {
                0x20 => Some((StackBase::Rsp, 0)),
                0x28 => Some((StackBase::Rbp, 0)),
                0x10 if !self.options.is_64bit => Some((StackBase::Rsp, 0)),
                0x14 if !self.options.is_64bit => Some((StackBase::Rbp, 0)),
                _ => None,
            };
        }

        let key = VarnodeKey::from(ptr);
        if !visiting.insert(key.clone()) {
            return None;
        }
        let resolved = match self.defs.get(&key).copied() {
            Some(op) => match op.opcode {
                PcodeOpcode::Copy
                | PcodeOpcode::Cast
                | PcodeOpcode::IntZExt
                | PcodeOpcode::IntSExt => self.resolve_stack_address_inner(&op.inputs[0], visiting),
                PcodeOpcode::IntAdd | PcodeOpcode::PtrAdd => {
                    if op.inputs.len() < 2 {
                        None
                    } else if let Some((base, offset)) =
                        self.resolve_stack_address_inner(&op.inputs[0], visiting)
                    {
                        const_offset(&op.inputs[1]).map(|delta| (base, offset + delta))
                    } else if let Some((base, offset)) =
                        self.resolve_stack_address_inner(&op.inputs[1], visiting)
                    {
                        const_offset(&op.inputs[0]).map(|delta| (base, offset + delta))
                    } else {
                        None
                    }
                }
                PcodeOpcode::IntSub => {
                    if op.inputs.len() < 2 {
                        None
                    } else if let Some((base, offset)) =
                        self.resolve_stack_address_inner(&op.inputs[0], visiting)
                    {
                        const_offset(&op.inputs[1]).map(|delta| (base, offset - delta))
                    } else {
                        None
                    }
                }
                PcodeOpcode::PtrSub => {
                    if op.inputs.len() < 2 {
                        None
                    } else if let Some((base, offset)) =
                        self.resolve_stack_address_inner(&op.inputs[0], visiting)
                    {
                        const_offset(&op.inputs[1]).map(|delta| (base, offset + delta))
                    } else {
                        None
                    }
                }
                _ => None,
            },
            None => None,
        };
        visiting.remove(&key);
        resolved
    }
}
