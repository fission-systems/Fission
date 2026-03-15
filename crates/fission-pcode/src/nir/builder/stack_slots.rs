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
            return Some(name.to_string());
        }
        if let Some(param_index) = self.register_param_aliases.get(&vn.offset).copied() {
            let alias_name = format!("param_{}", param_index + 1);
            self.params.entry(param_index).or_insert_with(|| NirBinding {
                name: alias_name.clone(),
                ty: type_from_size(vn.size, false),
                surface_type_name: None,
                initializer: None,
            });
            return Some(alias_name);
        }
        Some(name.to_string())
    }

    pub(super) fn try_stack_slot_lvalue_for_memory_op(
        &mut self,
        op: &PcodeOp,
        ptr: &Varnode,
        ty: NirType,
    ) -> Option<(String, NirType)> {
        if let Some((base, offset)) = self.resolve_stack_address_from_memory_op(op) {
            return self.ensure_stack_slot_binding(base, offset, ty);
        }
        self.try_stack_slot_lvalue(ptr, ty)
    }

    pub(super) fn try_stack_slot_lvalue(
        &mut self,
        ptr: &Varnode,
        ty: NirType,
    ) -> Option<(String, NirType)> {
        let (base, offset) = self.resolve_stack_address(ptr)?;
        self.ensure_stack_slot_binding(base, offset, ty)
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
        let resolved = match self.lookup_def_site(ptr).map(|(_, op)| op) {
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

    pub(super) fn ensure_stack_slot_binding(
        &mut self,
        base: StackBase,
        offset: i64,
        ty: NirType,
    ) -> Option<(String, NirType)> {
        let kind_name = match base {
            StackBase::Rbp if offset > 0 => format!("param_{:x}", offset),
            StackBase::Rbp => format!("local_{:x}", offset.unsigned_abs()),
            StackBase::Rsp => format!("local_{:x}", self.rsp_local_display_offset(offset)),
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

    fn rsp_local_display_offset(&self, offset: i64) -> i64 {
        if offset >= 0 && self.stack_frame_size > offset {
            self.stack_frame_size - offset
        } else {
            offset.unsigned_abs() as i64
        }
    }

    pub(super) fn resolve_stack_address_from_memory_op(&self, op: &PcodeOp) -> Option<(StackBase, i64)> {
        let asm = op.asm_mnemonic.as_deref()?.trim().to_ascii_uppercase();
        let start = asm.find('[')? + 1;
        let end = asm[start..].find(']')? + start;
        let mem = asm[start..end].replace(' ', "");

        if let Some(rest) = mem.strip_prefix("RSP") {
            return parse_stack_displacement(rest).map(|disp| (StackBase::Rsp, disp));
        }
        if let Some(rest) = mem.strip_prefix("RBP") {
            return parse_stack_displacement(rest).map(|disp| (StackBase::Rbp, disp));
        }
        if let Some(rest) = mem.strip_prefix("ESP") {
            return parse_stack_displacement(rest).map(|disp| (StackBase::Rsp, disp));
        }
        if let Some(rest) = mem.strip_prefix("EBP") {
            return parse_stack_displacement(rest).map(|disp| (StackBase::Rbp, disp));
        }
        None
    }
}

fn parse_stack_displacement(text: &str) -> Option<i64> {
    if text.is_empty() {
        return Some(0);
    }
    if let Some(rest) = text.strip_prefix('+') {
        return parse_stack_immediate(rest);
    }
    if let Some(rest) = text.strip_prefix('-') {
        return parse_stack_immediate(rest).map(|val| -val);
    }
    None
}

fn parse_stack_immediate(text: &str) -> Option<i64> {
    if let Some(hex) = text.strip_prefix("0X") {
        i64::from_str_radix(hex, 16).ok()
    } else {
        text.parse().ok()
    }
}
