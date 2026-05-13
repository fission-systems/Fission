use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResolvedGlobalPointer {
    pub name: String,
    pub byte_offset: i64,
}

impl<'a> PreviewBuilder<'a> {
    fn classify_stack_slot_origin(&self, base: StackBase, offset: i64) -> NirBindingOrigin {
        self.abi_state().classify_stack_slot_origin(base, offset)
    }

    pub(super) fn register_param(&mut self, vn: &Varnode) -> Option<String> {
        if !self.options.is_64bit
            && !matches!(
                self.options.calling_convention,
                CallingConvention::Arm32 | CallingConvention::PowerPc32
            )
        {
            return None;
        }
        if self.suppress_entry_register_params {
            if !is_register_space_id(vn.space_id) {
                return None;
            }
            return Some(
                register_hardware_name_for_abi(vn.offset, vn.size, self.options.calling_convention)
                    .unwrap_or_else(|| register_name(vn.offset, vn.size))
                    .to_string(),
            );
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
            return None;
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

    pub(super) fn try_global_memory_lvalue(
        &self,
        op: &PcodeOp,
        ptr: &Varnode,
        ty: NirType,
    ) -> Option<HirLValue> {
        if let Some(name) = self.options.relocation_names.get(&op.address) {
            return Some(HirLValue::Var(name.clone()));
        }
        if let Some(global) = self.resolve_relocated_pointer(ptr, 16) {
            return Some(if global.byte_offset == 0 {
                HirLValue::Var(global.name)
            } else {
                HirLValue::Deref {
                    ptr: Box::new(HirExpr::PtrOffset {
                        base: Box::new(HirExpr::AddressOfGlobal(global.name)),
                        offset: global.byte_offset,
                    }),
                    ty,
                }
            });
        }
        let addr = self.resolve_global_address(ptr, 16)?;
        self.options
            .global_names
            .get(&addr)
            .cloned()
            .map(HirLValue::Var)
    }

    fn resolve_relocated_pointer_symbol(&self, ptr: &Varnode, budget: usize) -> Option<String> {
        self.resolve_relocated_pointer(ptr, budget)
            .filter(|global| global.byte_offset == 0)
            .map(|global| global.name)
    }

    fn resolve_relocated_pointer(
        &self,
        ptr: &Varnode,
        budget: usize,
    ) -> Option<ResolvedGlobalPointer> {
        if budget == 0 {
            return None;
        }
        if ptr.is_constant
            && let Some(name) = self.options.relocation_names.get(&ptr.offset)
        {
            return Some(ResolvedGlobalPointer {
                name: name.clone(),
                byte_offset: 0,
            });
        }
        let (_, op) = self.lookup_def_site(ptr)?;
        match op.opcode {
            PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt => {
                self.resolve_relocated_pointer(op.inputs.first()?, budget - 1)
            }
            PcodeOpcode::Load => self.resolve_relocated_load_pointer(op, budget - 1),
            PcodeOpcode::IntAdd | PcodeOpcode::PtrSub => {
                let mut base = self.resolve_relocated_pointer(op.inputs.first()?, budget - 1)?;
                let delta = const_offset(op.inputs.get(1)?)?;
                base.byte_offset = base.byte_offset.checked_add(delta)?;
                Some(base)
            }
            PcodeOpcode::IntSub => {
                let mut base = self.resolve_relocated_pointer(op.inputs.first()?, budget - 1)?;
                let delta = const_offset(op.inputs.get(1)?)?;
                base.byte_offset = base.byte_offset.checked_sub(delta)?;
                Some(base)
            }
            PcodeOpcode::PtrAdd => {
                let mut base = self.resolve_relocated_pointer(op.inputs.first()?, budget - 1)?;
                let index = const_offset(op.inputs.get(1)?)?;
                let scale = const_offset(op.inputs.get(2)?)?;
                let delta = index.checked_mul(scale)?;
                base.byte_offset = base.byte_offset.checked_add(delta)?;
                Some(base)
            }
            _ => {
                let addr = self.resolve_global_address(ptr, budget)?;
                let (&base_addr, name) = self
                    .options
                    .global_names
                    .iter()
                    .filter(|(base_addr, _)| **base_addr <= addr)
                    .max_by_key(|(base_addr, _)| **base_addr)?;
                let size = self
                    .options
                    .global_sizes
                    .get(&base_addr)
                    .copied()
                    .unwrap_or(0);
                let byte_offset = addr.checked_sub(base_addr)?;
                if size != 0 && byte_offset < size {
                    Some(ResolvedGlobalPointer {
                        name: name.clone(),
                        byte_offset: i64::try_from(byte_offset).ok()?,
                    })
                } else {
                    None
                }
            }
        }
    }

    pub(super) fn resolve_relocated_load_pointer(
        &self,
        op: &PcodeOp,
        budget: usize,
    ) -> Option<ResolvedGlobalPointer> {
        if op.opcode != PcodeOpcode::Load || op.inputs.len() < 2 {
            return None;
        }
        if let Some(name) = self.options.relocation_names.get(&op.address) {
            return Some(ResolvedGlobalPointer {
                name: name.clone(),
                byte_offset: 0,
            });
        }
        let literal_addr = self.resolve_global_address(op.inputs.get(1)?, budget)?;
        if let Some(name) = self.options.relocation_names.get(&literal_addr) {
            return Some(ResolvedGlobalPointer {
                name: name.clone(),
                byte_offset: 0,
            });
        }
        let target_addr = self.read_pointer_from_binary(literal_addr)?;
        self.options
            .global_names
            .get(&target_addr)
            .cloned()
            .map(|name| ResolvedGlobalPointer {
                name,
                byte_offset: 0,
            })
    }

    pub(super) fn read_readonly_scalar_from_binary(&self, address: u64, size: u32) -> Option<u64> {
        if self.options.relocation_names.contains_key(&address)
            || self.options.global_names.contains_key(&address)
        {
            return None;
        }
        self.read_scalar_from_binary(address, size, true)
    }

    fn read_pointer_from_binary(&self, address: u64) -> Option<u64> {
        self.read_scalar_from_binary(address, self.options.pointer_size, false)
    }

    fn read_scalar_from_binary(
        &self,
        address: u64,
        size: u32,
        require_readonly: bool,
    ) -> Option<u64> {
        let binary = self.binary?;
        let size = size as usize;
        let section = binary.inner().sections.iter().find(|section| {
            let start = section.virtual_address;
            start
                .checked_add(section.file_size.min(section.virtual_size))
                .is_some_and(|end| (start..end).contains(&address))
        })?;
        if require_readonly && section.is_writable {
            return None;
        }
        let offset_in_section = address.checked_sub(section.virtual_address)?;
        let file_offset = section.file_offset.checked_add(offset_in_section)? as usize;
        let bytes = binary.inner().data.as_slice();
        let raw = bytes.get(file_offset..file_offset.checked_add(size)?)?;
        let is_big_endian = binary
            .sleigh_language_id()
            .is_some_and(|language_id| language_id.contains(":BE:"));
        match size {
            1 => raw.first().copied().map(u64::from),
            2 => {
                let arr: [u8; 2] = raw.try_into().ok()?;
                Some(if is_big_endian {
                    u16::from_be_bytes(arr)
                } else {
                    u16::from_le_bytes(arr)
                } as u64)
            }
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

    pub(super) fn resolve_global_address(&self, ptr: &Varnode, budget: usize) -> Option<u64> {
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
                CallingConvention::PowerPc32 => match ptr.offset {
                    0x04 => Some((StackBase::Rsp, 0)),
                    _ => None,
                },
                CallingConvention::PowerPc64 => match ptr.offset {
                    0x08 => Some((StackBase::Rsp, 0)),
                    _ => None,
                },
                CallingConvention::LoongArch32 => match ptr.offset {
                    0x10c => Some((StackBase::Rsp, 0)),
                    0x158 => Some((StackBase::Rbp, 0)),
                    _ => None,
                },
                CallingConvention::LoongArch64 => match ptr.offset {
                    0x118 => Some((StackBase::Rsp, 0)),
                    0x1b0 => Some((StackBase::Rbp, 0)),
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

        if let Some(entry) = self.locals.get_mut(&offset) {
            if entry.ty == NirType::Unknown {
                entry.ty = ty.clone();
            }
            if matches!(entry.origin, NirBindingOrigin::StackOffset(_))
                && !matches!(origin, NirBindingOrigin::StackOffset(_))
            {
                entry.origin = origin;
            }
            return Some((entry.name.clone(), entry.ty.clone()));
        }

        let id = self.locals_next_id;
        self.locals_next_id += 1;
        let name = self.unique_stack_slot_binding_name(&kind_name, id);
        let entry = self.locals.entry(offset).or_insert_with(|| StackSlot {
            id,
            name,
            ty: ty.clone(),
            origin,
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

    fn unique_stack_slot_binding_name(&self, base_name: &str, id: StackSlotId) -> String {
        if !self.binding_name_in_use(base_name) {
            return base_name.to_string();
        }
        let mut candidate = format!("{base_name}_{id}");
        let mut suffix = id + 1;
        while self.binding_name_in_use(&candidate) {
            candidate = format!("{base_name}_{suffix}");
            suffix += 1;
        }
        candidate
    }

    fn binding_name_in_use(&self, name: &str) -> bool {
        self.params.values().any(|binding| binding.name == name)
            || self.locals.values().any(|slot| slot.name == name)
            || self.temps.contains_key(name)
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
