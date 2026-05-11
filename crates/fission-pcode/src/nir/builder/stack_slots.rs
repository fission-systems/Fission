use super::*;

impl<'a> PreviewBuilder<'a> {
    fn classify_stack_slot_origin(&self, base: StackBase, offset: i64) -> NirBindingOrigin {
        self.abi_state().classify_stack_slot_origin(base, offset)
    }

    pub(super) fn register_param(&mut self, vn: &Varnode) -> Option<String> {
        if !self.options.is_64bit && self.options.calling_convention != CallingConvention::Arm32 {
            return None;
        }
        if self.suppress_entry_register_params {
            if !is_register_space_id(vn.space_id) {
                return None;
            }
            return Some(register_name(vn.offset, vn.size).to_string());
        }
        let abi = self.abi_state();
        if is_register_varnode(vn)
            && let Some(param_index) = self.register_param_aliases.get(&vn.offset).copied()
        {
            let alias_name = abi.param_name(param_index);
            self.params
                .entry(param_index)
                .or_insert_with(|| NirBinding {
                    name: alias_name.clone(),
                    ty: type_from_size(vn.size, false),
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::ParamIndex(param_index)),
                    initializer: None,
                });
            return Some(alias_name);
        }
        let Some(index) = abi.param_slot_for_varnode(vn) else {
            if vn.space_id != REGISTER_SPACE_ID {
                return None;
            }
            let (name, _) =
                register_name_with_param(vn.offset, vn.size, self.options.calling_convention)?;
            return Some(name.to_string());
        };
        let name = abi.param_name(index);
        self.params.entry(index).or_insert_with(|| NirBinding {
            name: name.clone(),
            ty: type_from_size(vn.size, false),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::ParamIndex(index)),
            initializer: None,
        });
        Some(name)
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

    pub(super) fn try_global_lvalue(&self, op: &PcodeOp, ptr: &Varnode) -> Option<String> {
        if let Some(name) = self.options.relocation_names.get(&op.address) {
            return Some(name.clone());
        }
        if let Some(name) = self.resolve_relocated_pointer_symbol(ptr, 16) {
            return Some(name);
        }
        let addr = self.resolve_global_address(ptr, 16)?;
        self.options.global_names.get(&addr).cloned()
    }

    fn resolve_relocated_pointer_symbol(&self, ptr: &Varnode, budget: usize) -> Option<String> {
        if budget == 0 {
            return None;
        }
        if ptr.is_constant
            && let Some(name) = self.options.relocation_names.get(&ptr.offset)
        {
            return Some(name.clone());
        }
        let (_, op) = self.lookup_def_site(ptr)?;
        match op.opcode {
            PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt => {
                self.resolve_relocated_pointer_symbol(op.inputs.first()?, budget - 1)
            }
            PcodeOpcode::Load => {
                if let Some(name) = self.options.relocation_names.get(&op.address) {
                    return Some(name.clone());
                }
                let literal_addr = self.resolve_global_address(op.inputs.get(1)?, budget - 1)?;
                if let Some(name) = self.options.relocation_names.get(&literal_addr) {
                    return Some(name.clone());
                }
                let target_addr = self.read_pointer_from_binary(literal_addr)?;
                self.options.global_names.get(&target_addr).cloned()
            }
            PcodeOpcode::IntAdd | PcodeOpcode::PtrSub => {
                if const_offset(op.inputs.get(1)?)? == 0 {
                    self.resolve_relocated_pointer_symbol(op.inputs.first()?, budget - 1)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn read_pointer_from_binary(&self, address: u64) -> Option<u64> {
        let binary = self.binary?;
        let pointer_size = self.options.pointer_size as usize;
        let section = binary.inner().sections.iter().find(|section| {
            let start = section.virtual_address;
            start
                .checked_add(section.file_size.min(section.virtual_size))
                .is_some_and(|end| (start..end).contains(&address))
        })?;
        let offset_in_section = address.checked_sub(section.virtual_address)?;
        let file_offset = section.file_offset.checked_add(offset_in_section)? as usize;
        let bytes = binary.inner().data.as_slice();
        let raw = bytes.get(file_offset..file_offset.checked_add(pointer_size)?)?;
        let is_big_endian = binary
            .sleigh_language_id()
            .is_some_and(|language_id| language_id.contains(":BE:"));
        match pointer_size {
            4 => {
                let arr: [u8; 4] = raw.try_into().ok()?;
                Some(if is_big_endian {
                    u32::from_be_bytes(arr)
                } else {
                    u32::from_le_bytes(arr)
                } as u64)
            }
            8 => {
                let arr: [u8; 8] = raw.try_into().ok()?;
                Some(if is_big_endian {
                    u64::from_be_bytes(arr)
                } else {
                    u64::from_le_bytes(arr)
                })
            }
            _ => None,
        }
    }

    fn resolve_global_address(&self, ptr: &Varnode, budget: usize) -> Option<u64> {
        if ptr.is_constant {
            return if ptr.offset != 0 {
                Some(ptr.offset)
            } else if ptr.constant_val >= 0 {
                Some(ptr.constant_val as u64)
            } else {
                None
            };
        }
        if budget == 0 {
            return None;
        }
        let (_, op) = self.lookup_def_site(ptr)?;
        match op.opcode {
            PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt => {
                self.resolve_global_address(op.inputs.first()?, budget - 1)
            }
            PcodeOpcode::IntAdd | PcodeOpcode::PtrSub => {
                let base = self.resolve_global_address(op.inputs.first()?, budget - 1)?;
                let delta = const_offset(op.inputs.get(1)?)?;
                if delta >= 0 {
                    base.checked_add(delta as u64)
                } else {
                    base.checked_sub(delta.unsigned_abs())
                }
            }
            PcodeOpcode::IntSub => {
                let base = self.resolve_global_address(op.inputs.first()?, budget - 1)?;
                let delta = const_offset(op.inputs.get(1)?)?;
                if delta >= 0 {
                    base.checked_sub(delta as u64)
                } else {
                    base.checked_add(delta.unsigned_abs())
                }
            }
            PcodeOpcode::PtrAdd => {
                let base = self.resolve_global_address(op.inputs.first()?, budget - 1)?;
                let index = const_offset(op.inputs.get(1)?)?;
                let scale = const_offset(op.inputs.get(2)?)?;
                let delta = index.checked_mul(scale)?;
                if delta >= 0 {
                    base.checked_add(delta as u64)
                } else {
                    base.checked_sub(delta.unsigned_abs())
                }
            }
            _ => None,
        }
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
        if is_register_space_id(ptr.space_id) {
            return match self.options.calling_convention {
                CallingConvention::Arm32 => match ptr.offset {
                    0x54 => Some((StackBase::Rsp, 0)),
                    0x4c => Some((StackBase::Rbp, 0)),
                    _ => None,
                },
                CallingConvention::AArch64 => match ptr.offset {
                    0x08 => Some((StackBase::Rsp, 0)),
                    0x40e8 => Some((StackBase::Rbp, 0)),
                    _ => None,
                },
                CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64 => match ptr.offset
                {
                    0x20 => Some((StackBase::Rsp, 0)),
                    0x28 => Some((StackBase::Rbp, 0)),
                    0x10 if !self.options.is_64bit => Some((StackBase::Rsp, 0)),
                    0x14 if !self.options.is_64bit => Some((StackBase::Rbp, 0)),
                    _ => None,
                },
            };
        }
        if ptr.space_id == UNIQUE_SPACE_ID
            && let Some(name) = unique_register_name(ptr.offset, ptr.size)
        {
            return match name {
                "rsp" | "esp" => Some((StackBase::Rsp, 0)),
                "rbp" | "ebp" => Some((StackBase::Rbp, 0)),
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
        let origin = self.classify_stack_slot_origin(base, offset);
        let kind_name = match origin {
            NirBindingOrigin::HomeSlot(home_offset) => format!("home_{home_offset:x}"),
            NirBindingOrigin::OutgoingArgSlot(arg_offset) => format!("arg_out_{arg_offset:x}"),
            NirBindingOrigin::ReturnScaffold => format!("ret_scaffold_{:x}", offset.unsigned_abs()),
            _ => match base {
                StackBase::Rbp if offset > 0 => format!("param_{:x}", offset),
                StackBase::Rbp => format!("local_{:x}", offset.unsigned_abs()),
                StackBase::Rsp => format!("local_{:x}", self.rsp_local_display_offset(offset)),
            },
        };

        let entry = self.locals.entry(offset).or_insert_with(|| {
            let id = self.locals_next_id;
            self.locals_next_id += 1;
            StackSlot {
                id,
                name: kind_name.clone(),
                ty: ty.clone(),
                origin,
            }
        });
        if entry.ty == NirType::Unknown {
            entry.ty = ty.clone();
        }
        if matches!(entry.origin, NirBindingOrigin::StackOffset(_))
            && !matches!(origin, NirBindingOrigin::StackOffset(_))
        {
            entry.origin = origin;
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

    pub(super) fn resolve_stack_address_from_memory_op(
        &self,
        op: &PcodeOp,
    ) -> Option<(StackBase, i64)> {
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
