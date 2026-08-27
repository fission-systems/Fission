use super::*;
use fission_midend_core::ir::{
    SsaGuardRangePrecision, SsaMemoryRegion, SsaMemoryValueId, SsaOpSite, SsaValueDefinition,
};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::midend::builder) struct ResolvedGlobalPointer {
    pub name: String,
    pub byte_offset: i64,
}

/// A constant operand read as the *signed displacement* it is.
///
/// `const_offset` hands back `constant_val` as stored, and a 32-bit lifter
/// stores `add ebp, -168` as `0xffffff58`. Used unchanged that is a stack
/// offset of 4,294,967,128, which `incoming_stack_argument_index` then divides
/// into a parameter index of 1,073,741,780 -- and
/// `ensure_incoming_stack_param_binding` fills every slot from zero up to it.
/// Measured on `dexter.dll`: four of its 132 functions each reached 3GB in
/// about two seconds building `param_N` bindings, one 200MB allocation among
/// them, before anything else in the function was looked at.
///
/// Sign-extension is by the varnode's own width, the same rule
/// `signed_constant_delta` applies in the SSA builder. A four-byte
/// displacement whose top bit is set is negative in every calling convention
/// here; read as unsigned it would address four gigabytes up the stack.
fn signed_const_displacement(vn: &Varnode) -> Option<i64> {
    let raw = const_offset(vn)?;
    let bits = vn.size.checked_mul(8)?;
    if !(1..64).contains(&bits) {
        return Some(raw);
    }
    let mask = (1_u64 << bits) - 1;
    let value = (raw as u64) & mask;
    let sign_bit = 1_u64 << (bits - 1);
    Some(if value & sign_bit == 0 {
        value as i64
    } else {
        (value | !mask) as i64
    })
}

impl<'a> PreviewBuilder<'a> {
    /// Entry-SP-relative offset for a load whose reaching memory value is
    /// proven to come from function entry rather than a store in this
    /// function. This is the ownership proof required before an x64 stack
    /// access may become a formal parameter.
    fn current_load_entry_stack_value_offset(&self) -> Option<i64> {
        let site = self.current_lowering_site?;
        let op = self
            .pcode
            .blocks
            .get(site.block_idx)?
            .ops
            .get(site.op_idx)?;
        if op.opcode != PcodeOpcode::Load {
            return None;
        }
        let ssa_site = SsaOpSite {
            block: u32::try_from(site.block_idx).ok()?,
            op: u32::try_from(site.op_idx).ok()?,
        };
        let guard = self.scalar_ssa.dynamic_guards.get(&ssa_site)?;
        if guard.region != Some(SsaMemoryRegion::Stack)
            || guard.precision != SsaGuardRangePrecision::Exact
        {
            return None;
        }
        let pieces = self.scalar_ssa.memory_operation_inputs.get(&ssa_site)?;
        let piece = pieces.iter().find(|piece| piece.byte_offset == 0)?;
        let value = self.scalar_ssa.memory_value(piece.value)?;
        if value.storage.region != SsaMemoryRegion::Stack
            || !self.memory_value_is_entry_owned(piece.value, &mut BTreeSet::new(), 64)
        {
            return None;
        }
        i64::try_from(value.storage.offset).ok()
    }

    fn memory_value_is_entry_owned(
        &self,
        value_id: SsaMemoryValueId,
        visiting: &mut BTreeSet<SsaMemoryValueId>,
        budget: usize,
    ) -> bool {
        if budget == 0 || !visiting.insert(value_id) {
            return false;
        }
        let Some(value) = self.scalar_ssa.memory_value(value_id) else {
            visiting.remove(&value_id);
            return false;
        };
        let result = match value.definition {
            SsaValueDefinition::Input => true,
            SsaValueDefinition::Operation(_) => false,
            SsaValueDefinition::Phi { block } => self
                .scalar_ssa
                .memory_phis
                .get(&block)
                .and_then(|phis| phis.iter().find(|phi| phi.output == value_id))
                .is_some_and(|phi| {
                    !phi.operands.is_empty()
                        && phi.operands.iter().all(|operand| {
                            self.memory_value_is_entry_owned(operand.value, visiting, budget - 1)
                        })
                }),
        };
        visiting.remove(&value_id);
        result
    }

    fn refine_register_param_type_from_varnode(&mut self, param_index: usize, vn: &Varnode) {
        let Some(binding) = self.params.get_mut(&param_index) else {
            return;
        };
        if binding.surface_type_name.is_some() {
            return;
        }
        let new_bits = vn.size.saturating_mul(8);
        let NirType::Int {
            bits: current_bits,
            signed,
        } = binding.ty
        else {
            return;
        };
        if new_bits == 0 || new_bits >= current_bits {
            return;
        }
        binding.ty = NirType::Int {
            bits: new_bits,
            signed,
        };
    }

    pub(in crate::midend::builder) fn classify_stack_slot_origin(
        &self,
        base: StackBase,
        offset: i64,
    ) -> NirBindingOrigin {
        let abi = self.abi_state();
        if self.options.is_64bit
            && let Some(entry_offset) = self.current_load_entry_stack_value_offset()
            && let Some(index) =
                abi.incoming_x64_stack_parameter_index_from_entry_offset(entry_offset)
        {
            return NirBindingOrigin::ParamIndex(index);
        }
        abi.classify_stack_slot_origin(base, offset)
    }

    pub(in crate::midend::builder) fn register_param(&mut self, vn: &Varnode) -> Option<String> {
        if !self.options.is_64bit
            && !matches!(
                self.options.calling_convention,
                CallingConvention::Arm32 | CallingConvention::PowerPc32 | CallingConvention::Mips32
            )
        {
            return None;
        }
        if self.suppress_entry_register_params {
            if !is_register_space_id(vn.space_id) {
                return None;
            }
            return Some(
                self.sla_hw_name(vn.offset, vn.size)
                    .unwrap_or_else(|| "reg".to_string())
                    .to_string(),
            );
        }
        let abi = self.abi_state();
        let entry_arity = self.entry_arity;
        if is_register_varnode(vn)
            && let Some(param_index) = self.register_param_aliases.get(&vn.offset).copied()
        {
            if param_index >= entry_arity {
                return None;
            }
            let alias_name = abi.param_name(param_index);
            self.params
                .entry(param_index)
                .or_insert_with(|| PreHirBinding {
                    name: alias_name.clone(),
                    ty: type_from_size(vn.size, false),
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::ParamIndex(param_index)),
                    initializer: None,
                });
            self.used_param_local_names.insert(alias_name.clone());
            self.refine_register_param_type_from_varnode(param_index, vn);
            return Some(alias_name);
        }
        let Some(index) = abi.param_slot_for_varnode(vn) else {
            return None;
        };
        if index >= entry_arity {
            return None;
        }
        let name = abi.param_name(index);
        self.params.entry(index).or_insert_with(|| PreHirBinding {
            name: name.clone(),
            ty: type_from_size(vn.size, false),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::ParamIndex(index)),
            initializer: None,
        });
        self.used_param_local_names.insert(name.clone());
        self.refine_register_param_type_from_varnode(index, vn);
        Some(name)
    }

    pub(in crate::midend::builder) fn try_stack_slot_lvalue_for_memory_op(
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

    pub(in crate::midend::builder) fn try_global_lvalue(
        &self,
        op: &PcodeOp,
        ptr: &Varnode,
    ) -> Option<String> {
        if let Some(name) = self.relocation_name_for_pcode_op(op.address) {
            return Some(name);
        }
        if let Some(name) = self.resolve_relocated_pointer_symbol(ptr, 16) {
            return Some(name);
        }
        let addr = self.resolve_global_address(ptr, 16)?;
        self.options.global_names.get(&addr).cloned()
    }

    pub(in crate::midend::builder) fn try_global_memory_lvalue(
        &self,
        op: &PcodeOp,
        ptr: &Varnode,
        ty: NirType,
    ) -> Option<PreHirLValue> {
        if let Some(name) = self.relocation_name_for_pcode_op(op.address) {
            return Some(PreHirLValue::Var(name));
        }
        if let Some(global) = self.resolve_relocated_pointer(ptr, 16) {
            return Some(if global.byte_offset == 0 {
                PreHirLValue::Var(global.name)
            } else {
                PreHirLValue::Deref {
                    ptr: Box::new(PreHirExpr::PtrOffset {
                        base: Box::new(PreHirExpr::AddressOfGlobal(global.name)),
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
            .map(PreHirLValue::Var)
    }

    fn resolve_relocated_pointer_symbol(&self, ptr: &Varnode, budget: usize) -> Option<String> {
        self.resolve_relocated_pointer(ptr, budget)
            .filter(|global| global.byte_offset == 0)
            .map(|global| global.name)
    }

    pub(in crate::midend::builder) fn relocation_name_for_pcode_op(
        &self,
        address: u64,
    ) -> Option<String> {
        if let Some(name) = self.options.relocation_names.get(&address) {
            return Some(name.clone());
        }
        let max_inline_reloc_delta = u64::from(self.options.pointer_size.min(4));
        self.options
            .relocation_names
            .iter()
            .filter_map(|(&reloc_addr, name)| {
                let delta = reloc_addr.checked_sub(address)?;
                (delta > 0 && delta <= max_inline_reloc_delta).then_some((delta, reloc_addr, name))
            })
            .min_by_key(|(delta, reloc_addr, _)| (*delta, *reloc_addr))
            .map(|(_, _, name)| name.clone())
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

    pub(in crate::midend::builder) fn resolve_relocated_load_pointer(
        &self,
        op: &PcodeOp,
        budget: usize,
    ) -> Option<ResolvedGlobalPointer> {
        if op.opcode != PcodeOpcode::Load || op.inputs.len() < 2 {
            return None;
        }
        if let Some(name) = self.relocation_name_for_pcode_op(op.address) {
            return Some(ResolvedGlobalPointer {
                name,
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

    pub(in crate::midend::builder) fn read_readonly_scalar_from_binary(
        &self,
        address: u64,
        size: u32,
    ) -> Option<u64> {
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

    pub(in crate::midend::builder) fn resolve_global_address(
        &self,
        ptr: &Varnode,
        budget: usize,
    ) -> Option<u64> {
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
            PcodeOpcode::IntOr => {
                let lhs = self.resolve_global_address(op.inputs.first()?, budget - 1)?;
                let rhs = self.resolve_global_address(op.inputs.get(1)?, budget - 1)?;
                Some(lhs | rhs)
            }
            PcodeOpcode::IntAnd => {
                let lhs = self.resolve_global_address(op.inputs.first()?, budget - 1)?;
                let rhs = self.resolve_global_address(op.inputs.get(1)?, budget - 1)?;
                Some(lhs & rhs)
            }
            PcodeOpcode::IntXor => {
                let lhs = self.resolve_global_address(op.inputs.first()?, budget - 1)?;
                let rhs = self.resolve_global_address(op.inputs.get(1)?, budget - 1)?;
                Some(lhs ^ rhs)
            }
            PcodeOpcode::IntLeft => {
                let value = self.resolve_global_address(op.inputs.first()?, budget - 1)?;
                let shift = const_offset(op.inputs.get(1)?)?;
                u32::try_from(shift)
                    .ok()
                    .map(|shift| value.checked_shl(shift).unwrap_or(0))
            }
            PcodeOpcode::IntRight => {
                let value = self.resolve_global_address(op.inputs.first()?, budget - 1)?;
                let shift = const_offset(op.inputs.get(1)?)?;
                u32::try_from(shift)
                    .ok()
                    .map(|shift| value.checked_shr(shift).unwrap_or(0))
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

    pub(in crate::midend::builder) fn try_stack_slot_lvalue(
        &mut self,
        ptr: &Varnode,
        ty: NirType,
    ) -> Option<(String, NirType)> {
        let (base, offset) = self.resolve_stack_address(ptr)?;
        self.ensure_stack_slot_binding(base, offset, ty)
    }

    pub(in crate::midend::builder) fn resolve_stack_address(
        &self,
        ptr: &Varnode,
    ) -> Option<(StackBase, i64)> {
        self.resolve_stack_address_inner(ptr, &mut HashSet::default())
    }

    /// Resolves what a *bare* RSP/ESP register-space varnode's occurrence at
    /// `self.current_lowering_site` should map to in `self.locals`'
    /// steady-state-rsp-relative coordinate system (the same coordinate
    /// system `resolve_stack_address_from_memory_op`'s asm-text-parsed
    /// `[rsp+K]` displacements and `rsp_local_display_offset` already use --
    /// `0` means "current rsp equals the function's final, post-prologue
    /// steady-state depth").
    ///
    /// Outside the function's recognized entry-block prologue (i.e. no
    /// `rsp_prologue_delta_table` entry for this exact op site -- covers
    /// every access after the prologue, and any function whose prologue
    /// didn't fit the recognized push/sub pattern in the first place),
    /// falls back to the old hardcoded `0`, matching pre-existing behavior
    /// exactly.
    ///
    /// Inside the prologue (e.g. a `push reg`'s `Store` half, which reads
    /// `rsp` *after* that same push's own `IntSub` already ran), converts
    /// the table's entry-rsp-relative delta `d` (negative) into the
    /// steady-state-relative coordinate via `d + stack_frame_size`: the
    /// steady-state rsp sits at `entry_rsp - stack_frame_size`, so an
    /// address at `entry_rsp + d` sits `d + stack_frame_size` bytes above
    /// it. This is what turns a `push r14; push r13; push r12; push rbp;
    /// push rbx` chain's five `Store`s -- which previously all resolved to
    /// the identical hardcoded `(StackBase::Rsp, 0)` and got silently
    /// collapsed onto one `self.locals` entry -- into five distinct,
    /// correctly-ordered offsets instead.
    /// `None` when a bare RSP read can't be trusted to sit at the function's
    /// single post-prologue steady-state depth -- specifically, when the
    /// current op's own block (anything but the recognized entry-block
    /// prologue at index 0) *itself* writes to RSP. Ordinary functions never
    /// trip this (a normal epilogue's `add rsp, N` has no further
    /// rsp-relative access after it in the same block to resolve), but an
    /// inlined stack-probe loop -- GCC/Linux's `lea r11,[rsp-N]; loop: sub
    /// rsp,0x1000; or [rsp],0; cmp rsp,r11; jnz loop` for a >1-page frame --
    /// repeats `sub rsp` on every iteration, so treating a bare RSP read
    /// inside that same block as "still at steady state" silently aliased
    /// the probe's `or [rsp],0` touch onto whatever real local happens to
    /// sit at offset 0, and (since nothing here modeled the loop condition's
    /// own `rsp` read either) rendered the whole loop as an empty,
    /// meaningless `while (rsp != r11) {}` -- confirmed on a real
    /// GCC-compiled >16KB-local fixture. `None` here makes the caller treat
    /// the access as an ordinary (non-stack-slot) pointer dereference
    /// instead, same as Ghidra's own spacebase heuristic silently declining
    /// to fire when RSP's SSA value isn't provably a prologue-constant
    /// offset (falls back to raw pointer arithmetic on a normal local
    /// rather than fabricating a wrong stack-slot name).
    fn rsp_register_space_offset(&self) -> Option<i64> {
        if let Some(site) = self.current_lowering_site {
            if let Some(delta) = self.rsp_prologue_delta_table.get(&site) {
                return Some(delta + self.stack_frame_size);
            }
            if site.block_idx != 0 && self.block_writes_rsp_register(site.block_idx) {
                return None;
            }
        }
        Some(0)
    }

    /// Whether p-code block `block_idx` (assumed not to be the recognized
    /// entry-block prologue, index 0) both branches back to itself and
    /// contains a direct arithmetic op (`IntAdd`/`IntSub`/`PtrAdd`/`PtrSub`)
    /// writing the bare RSP/ESP register-space varnode for the current
    /// calling convention -- the self-loop requirement is what excludes
    /// ordinary `Call`/`CallInd` ops' own implicit rsp bookkeeping in most
    /// cases, but a self-looping block that itself *contains a call* (any
    /// loop calling a function) still has that same implicit per-call rsp
    /// arithmetic, so those are excluded outright too: a real inlined
    /// stack-probe loop is pure register/memory arithmetic with no call in
    /// it at all. Scoped to x86/x86-64 (the only conventions
    /// `rsp_register_space_offset` ever calls this for) since that's the
    /// only ISA where this steady-state coordinate system applies at all.
    pub(in crate::midend::builder) fn block_writes_rsp_register(&self, block_idx: usize) -> bool {
        let rsp_offset = match self.options.calling_convention {
            CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64 => {
                if self.options.is_64bit { 0x20 } else { 0x10 }
            }
            CallingConvention::X86_32 => 0x10,
            _ => return false,
        };
        let Some(block) = self.pcode.blocks.get(block_idx) else {
            return false;
        };
        if !block.successors.contains(&block.index)
            || block.ops.iter().any(|op| {
                matches!(
                    op.opcode,
                    PcodeOpcode::Call | PcodeOpcode::CallInd | PcodeOpcode::CallOther
                )
            })
        {
            return false;
        }
        block.ops.iter().any(|op| {
            matches!(
                op.opcode,
                PcodeOpcode::IntAdd
                    | PcodeOpcode::IntSub
                    | PcodeOpcode::PtrAdd
                    | PcodeOpcode::PtrSub
            ) && op
                .output
                .as_ref()
                .is_some_and(|out| is_register_space_id(out.space_id) && out.offset == rsp_offset)
        })
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
                CallingConvention::Mips32 => match ptr.offset {
                    0x74 => Some((StackBase::Rsp, 0)),
                    0x78 => Some((StackBase::Rbp, 0)),
                    _ => None,
                },
                CallingConvention::Mips64 => match ptr.offset {
                    0xe8 => Some((StackBase::Rsp, 0)),
                    0xf0 => Some((StackBase::Rbp, 0)),
                    _ => None,
                },
                CallingConvention::AArch64 => match ptr.offset {
                    0x08 => Some((StackBase::Rsp, 0)),
                    0x40e8 => Some((StackBase::Rbp, 0)),
                    _ => None,
                },
                CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64 => match ptr.offset
                {
                    0x20 => self
                        .rsp_register_space_offset()
                        .map(|off| (StackBase::Rsp, off)),
                    0x28 => Some((StackBase::Rbp, self.rbp_frame_bias)),
                    0x10 if !self.options.is_64bit => self
                        .rsp_register_space_offset()
                        .map(|off| (StackBase::Rsp, off)),
                    0x14 if !self.options.is_64bit => Some((StackBase::Rbp, self.rbp_frame_bias)),
                    _ => None,
                },
                CallingConvention::X86_32 => match ptr.offset {
                    0x10 => self
                        .rsp_register_space_offset()
                        .map(|off| (StackBase::Rsp, off)),
                    0x14 => Some((StackBase::Rbp, self.rbp_frame_bias)),
                    _ => None,
                },
            };
        }
        if ptr.space_id == UNIQUE_SPACE_ID
            && let Some(name) = crate::arch::x86::unique_x86_register_name(ptr.offset, ptr.size)
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
                        self.resolve_constant_operand(&op.inputs[1])
                            .map(|delta| (base, offset + delta))
                    } else if let Some((base, offset)) =
                        self.resolve_stack_address_inner(&op.inputs[1], visiting)
                    {
                        self.resolve_constant_operand(&op.inputs[0])
                            .map(|delta| (base, offset + delta))
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
                        self.resolve_constant_operand(&op.inputs[1])
                            .map(|delta| (base, offset - delta))
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
                        self.resolve_constant_operand(&op.inputs[1])
                            .map(|delta| (base, offset + delta))
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

    /// Resolves a pointer to a byte offset from the x86 `FS_OFFSET`/
    /// `GS_OFFSET` pseudo-register -- the Windows Thread Environment
    /// Block (TEB) segment base (`fs:` on 32-bit, `gs:` on 64-bit) --
    /// when it's computed as `(FS_OFFSET|GS_OFFSET) + K` for a compile-
    /// time-constant `K`, mirroring `resolve_stack_address_inner`'s own
    /// recursive `Copy`/`Cast`/`IntZExt`/`IntSExt`/`IntAdd`/`PtrAdd`
    /// structure (reusing `resolve_constant_operand` for the delta) but
    /// against a single fixed base register instead of a
    /// calling-convention-specific table, since there's exactly one
    /// segment-base register that matters here regardless of `x86_32`
    /// vs `WindowsX64`. Confirmed via a real `x86_64-w64-mingw32-gcc`
    /// build (`movq %gs:0x60, %rax`) that SLEIGH lifts this exactly as
    /// `IntAdd(GS_OFFSET, const(0x60))` -- `FS_OFFSET`/`GS_OFFSET` are
    /// real named registers in `utils/sleigh-specs/languages/x86/
    /// ia.sinc`, both at register-space offset `0x110` (the applicable
    /// one selected by bitness at assemble time, so no separate 32-
    /// vs-64-bit branch is needed here).
    pub(in crate::midend::builder) fn resolve_teb_field_offset(
        &self,
        ptr: &Varnode,
    ) -> Option<i64> {
        self.resolve_teb_field_offset_inner(ptr, &mut HashSet::default())
    }

    /// [`resolve_teb_field_offset`] plus [`teb_field_name`]'s lookup, for
    /// the direct "does this pointer resolve to a *known, named* TEB
    /// field" query the `Load`-lowering path needs. Returns the name
    /// wrapped in a `Cast` to the field's real type -- a bare, type-less
    /// `PreHirExpr::Var` (matching the untyped `DAT_XXXXXXXX` convention)
    /// left downstream return-type inference with nothing to work with
    /// (`undefined is_debugged(void)` on a real fixture); registering a
    /// full `self.temps`/`self.locals` binding instead (mirroring how a
    /// stack slot's `Var` is backed by one) was tried and rejected -- it
    /// makes the renderer treat the name as a genuine local needing a
    /// declaration + initializer, which it isn't (there's no assigning
    /// `PreHirStmt` for it anywhere in the body, since it's a read from a
    /// fixed location, not a computed value), producing a *worse*,
    /// uninitialized-looking declaration. A `Cast` gives the use site a
    /// real type without implying local storage that needs to exist.
    pub(in crate::midend::builder) fn try_teb_field_var(
        &self,
        ptr: &Varnode,
    ) -> Option<PreHirExpr> {
        let offset = self.resolve_teb_field_offset(ptr)?;
        let (name, is_pointer) = teb_field_name(offset, self.options.is_64bit)?;
        let ty = if is_pointer {
            type_from_size(self.options.pointer_size, false)
        } else {
            type_from_size(4, false)
        };
        Some(PreHirExpr::Cast {
            ty,
            expr: Box::new(PreHirExpr::Var(name.to_string())),
        })
    }

    fn resolve_teb_field_offset_inner(
        &self,
        ptr: &Varnode,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Option<i64> {
        // `FS_OFFSET`/`GS_OFFSET` are declared as a 2-entry register array
        // starting at 0x110 (`ia.sinc`: `define register offset=0x110
        // size=$(SIZE) [ FS_OFFSET GS_OFFSET ]`) -- `FS_OFFSET` occupies
        // `[0x110, 0x110+size)`, `GS_OFFSET` immediately follows at
        // `0x110+size`. Confirmed empirically: a real 64-bit build's
        // `gs:0x60` access lifts with base register offset `0x118`
        // (`0x110 + 8`), not `0x110` itself -- checking only the literal
        // `0x110` (which would only ever match a 32-bit `fs:` access)
        // silently failed to recognize the far more common 64-bit `gs:`
        // case entirely.
        if is_register_space_id(ptr.space_id) && matches!(ptr.offset, 0x110 | 0x114 | 0x118) {
            return Some(0);
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
                | PcodeOpcode::IntSExt => {
                    self.resolve_teb_field_offset_inner(&op.inputs[0], visiting)
                }
                PcodeOpcode::IntAdd | PcodeOpcode::PtrAdd => {
                    if op.inputs.len() < 2 {
                        None
                    } else if let Some(offset) =
                        self.resolve_teb_field_offset_inner(&op.inputs[0], visiting)
                    {
                        self.resolve_constant_operand(&op.inputs[1])
                            .map(|delta| offset + delta)
                    } else if let Some(offset) =
                        self.resolve_teb_field_offset_inner(&op.inputs[1], visiting)
                    {
                        self.resolve_constant_operand(&op.inputs[0])
                            .map(|delta| offset + delta)
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

    /// Second hop past [`resolve_teb_field_offset`]/[`try_teb_field_var`]:
    /// the classic anti-debug check doesn't stop at
    /// `TEB.ProcessEnvironmentBlock` -- it dereferences that pointer and
    /// reads `PEB.BeingDebugged` two bytes in. Recognizes a pointer built
    /// as `Load(teb_ProcessEnvironmentBlock_address) + K` for a compile-
    /// time-constant `K`, i.e. the *value* loaded from the TEB field
    /// (not its address) used as a base for further arithmetic. Reuses
    /// [`resolve_teb_field_offset_inner`] to identify the inner `Load`'s
    /// address as specifically the `ProcessEnvironmentBlock` field
    /// (rather than some other pointer-typed TEB field) so this doesn't
    /// misfire on arithmetic over `teb_StackBase`/`teb_Self`/etc, which
    /// have no well-known fields of their own worth naming here.
    pub(in crate::midend::builder) fn resolve_peb_field_offset(
        &self,
        ptr: &Varnode,
    ) -> Option<i64> {
        self.resolve_peb_field_offset_inner(ptr, &mut HashSet::default())
    }

    /// [`resolve_peb_field_offset`] plus [`peb_field_name`]'s lookup,
    /// mirroring [`try_teb_field_var`]'s `Cast`-wrapping approach (same
    /// rationale: a real type at the use site without implying a local
    /// declaration that doesn't exist).
    pub(in crate::midend::builder) fn try_peb_field_var(
        &self,
        ptr: &Varnode,
    ) -> Option<PreHirExpr> {
        let offset = self.resolve_peb_field_offset(ptr)?;
        let (name, is_pointer) = peb_field_name(offset)?;
        let ty = if is_pointer {
            type_from_size(self.options.pointer_size, false)
        } else {
            type_from_size(1, false)
        };
        Some(PreHirExpr::Cast {
            ty,
            expr: Box::new(PreHirExpr::Var(name.to_string())),
        })
    }

    fn resolve_peb_field_offset_inner(
        &self,
        ptr: &Varnode,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Option<i64> {
        let key = VarnodeKey::from(ptr);
        if !visiting.insert(key.clone()) {
            return None;
        }
        let resolved = match self.lookup_def_site(ptr).map(|(_, op)| op) {
            Some(op) => match op.opcode {
                PcodeOpcode::Copy
                | PcodeOpcode::Cast
                | PcodeOpcode::IntZExt
                | PcodeOpcode::IntSExt => {
                    self.resolve_peb_field_offset_inner(&op.inputs[0], visiting)
                }
                PcodeOpcode::IntAdd | PcodeOpcode::PtrAdd => {
                    if op.inputs.len() < 2 {
                        None
                    } else if let Some(offset) =
                        self.resolve_peb_field_offset_inner(&op.inputs[0], visiting)
                    {
                        self.resolve_constant_operand(&op.inputs[1])
                            .map(|delta| offset + delta)
                    } else if let Some(offset) =
                        self.resolve_peb_field_offset_inner(&op.inputs[1], visiting)
                    {
                        self.resolve_constant_operand(&op.inputs[0])
                            .map(|delta| offset + delta)
                    } else {
                        None
                    }
                }
                PcodeOpcode::Load if op.inputs.len() >= 2 => {
                    let teb_offset = self
                        .resolve_teb_field_offset_inner(&op.inputs[1], &mut HashSet::default())?;
                    let (name, _) = teb_field_name(teb_offset, self.options.is_64bit)?;
                    if name == "teb_ProcessEnvironmentBlock" {
                        Some(0)
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

    /// Resolves a p-code operand to a compile-time-known constant, either
    /// directly (a literal `const(...)` varnode) or by walking a short
    /// `Copy`/`Cast`/`IntZExt`/`IntSExt` def chain back to one -- e.g. a
    /// `mov eax, 0x2000` a few ops before a `sub rsp, rax`. Without this,
    /// `IntAdd`/`IntSub`/`PtrAdd`/`PtrSub`'s delta operand only resolves via
    /// [`const_offset`], which requires the operand to already be a literal
    /// varnode -- so any stack adjustment computed through a register
    /// (rather than encoded directly in the instruction) resolved to
    /// `None`, and every later stack-relative access whose address depended
    /// on it (transitively, since `resolve_stack_address_inner` recurses)
    /// failed to resolve to a `(StackBase, offset)` pair at all.
    ///
    /// This is exactly the shape Windows/mingw's stack-probe idiom for a
    /// large (>1 page) stack frame produces: `mov eax, SIZE` /
    /// `call __chkstk`-or-`___chkstk_ms` / `sub rsp, rax` -- on x64, the
    /// probe routine itself only touches guard pages, it does *not* adjust
    /// `rsp` (confirmed against a real `x86_64-w64-mingw32-gcc`-compiled
    /// fixture with an 8KB local array: the raw p-code `Call` op to
    /// `___chkstk_ms` has no output at all, so it doesn't shadow `eax`'s
    /// def in between); the actual subtraction is ordinary, already-lifted
    /// caller code. Confirmed the corruption this fixes is real, not
    /// theoretical: before this, every `rbp`-relative local *past* the
    /// probe in that fixture misclassified as an incoming parameter
    /// (`classify_stack_slot_origin`'s positive-rbp-offset heuristic) since
    /// `rbp = rsp(after the unresolved sub) + 0x80` couldn't resolve to a
    /// real offset at all.
    fn resolve_constant_operand(&self, vn: &Varnode) -> Option<i64> {
        self.resolve_constant_operand_inner(vn, &mut HashSet::default())
    }

    fn resolve_constant_operand_inner(
        &self,
        vn: &Varnode,
        visiting: &mut HashSet<VarnodeKey>,
    ) -> Option<i64> {
        if let Some(value) = signed_const_displacement(vn) {
            return Some(value);
        }
        if !is_register_space_id(vn.space_id) && vn.space_id != UNIQUE_SPACE_ID {
            return None;
        }
        let key = VarnodeKey::from(vn);
        if !visiting.insert(key.clone()) {
            return None;
        }
        let resolved = match self.lookup_def_site(vn).map(|(_, op)| op) {
            Some(op) => match op.opcode {
                PcodeOpcode::Copy
                | PcodeOpcode::Cast
                | PcodeOpcode::IntZExt
                | PcodeOpcode::IntSExt => op
                    .inputs
                    .first()
                    .and_then(|input| self.resolve_constant_operand_inner(input, visiting)),
                _ => None,
            },
            None => None,
        };
        visiting.remove(&key);
        resolved
    }

    pub(in crate::midend::builder) fn ensure_stack_slot_binding(
        &mut self,
        base: StackBase,
        offset: i64,
        ty: NirType,
    ) -> Option<(String, NirType)> {
        let origin = self.classify_stack_slot_origin(base, offset);
        if let NirBindingOrigin::ParamIndex(index) = origin {
            return Some(self.ensure_incoming_stack_param_binding(index, ty));
        }
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

        if self.locals.contains_key(&offset) {
            if self.cover_proves_stack_slot_reuse_unsafe(offset) {
                // The SSA memory Cover positively proves this access belongs
                // to a different, interfering logical variable from whatever
                // already owns this offset's canonical name -- give it its
                // own one-off name instead of silently conflating the two
                // under `self.locals[offset]`'s name (the stack-slot analog
                // of the register name-collision bug fixed earlier this
                // session; see docs/proposals/2026-08-05-memory-ssa-stack-pointer-recognition-fix.md).
                // Deliberately not cached into `self.locals` or
                // `stack_slot_memory_owners`: as of this session's real-corpus
                // testing this path has never been observed to fire, so a
                // fresh name per occurrence (rather than a second persistent
                // "alternate" slot) keeps this a minimal, provably-safe
                // guard rather than a speculative redesign of `self.locals`'
                // one-name-per-offset shape.
                let name = self.next_unused_temp_binding_name(&ty);
                return Some((name, ty));
            }
            self.record_stack_slot_memory_owner(offset);
            let entry = self.locals.get_mut(&offset).expect("checked above");
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
        self.used_param_local_names.insert(name.clone());
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
        let result = (entry.name.clone(), entry.ty.clone());
        self.record_stack_slot_memory_owner(offset);
        Some(result)
    }

    pub(in crate::midend::builder) fn ensure_incoming_stack_param_binding(
        &mut self,
        index: usize,
        ty: NirType,
    ) -> (String, NirType) {
        let placeholder_ty = type_from_size(self.options.pointer_size, false);
        for slot in 0..=index {
            let slot_ty = if slot == index {
                ty.clone()
            } else {
                placeholder_ty.clone()
            };
            let entry = self.params.entry(slot).or_insert_with(|| PreHirBinding {
                name: format!("param_{}", slot + 1),
                ty: slot_ty,
                surface_type_name: None,
                origin: Some(NirBindingOrigin::ParamIndex(slot)),
                initializer: None,
            });
            self.used_param_local_names.insert(entry.name.clone());
        }

        let binding = self
            .params
            .get_mut(&index)
            .expect("incoming stack parameter was inserted");
        if binding.ty == NirType::Unknown || (binding.ty == placeholder_ty && ty != placeholder_ty)
        {
            binding.ty = ty;
        }
        (binding.name.clone(), binding.ty.clone())
    }

    /// Ghidra's `X86FunctionPurgeAnalyzer` scorecard item: recovers the
    /// exact stack-argument byte count a callee-cleanup x86-32 function
    /// (stdcall/fastcall/thiscall) purges via its own `RET imm16`, directly
    /// from the function's already-lifted p-code -- no cross-crate
    /// plumbing needed, since the purge amount is entirely a property of
    /// the callee's own epilogue. Used to force a minimum incoming-
    /// parameter count on the final signature: usage-based stack-slot
    /// recovery only ever sees a parameter that's actually *read*
    /// somewhere in the body, so a trailing stdcall parameter the callee
    /// never touches (dead but still part of the real signature, and
    /// still purged by the callee at return) would otherwise be silently
    /// dropped. Confirmed against a real `i686-w64-mingw32-gcc`-compiled
    /// `__stdcall` fixture with an unused third parameter (`ret $0xc`,
    /// only params 1-2 read in the body): before this, the recovered
    /// signature silently dropped to two parameters; after, it correctly
    /// shows three.
    pub(in crate::midend::builder) fn apply_x86_32_stack_purge_arity_floor(&mut self) {
        if self.options.is_64bit || self.options.calling_convention != CallingConvention::X86_32 {
            return;
        }
        let pointer_size = self.options.pointer_size;
        if pointer_size == 0 {
            return;
        }
        let purge = (0..self.pcode.blocks.len())
            .filter_map(|idx| x86_32_stack_purge_for_block(self.pcode, idx, pointer_size))
            .max();
        let Some(purge) = purge else {
            return;
        };
        let min_stack_params = (purge as u64 / u64::from(pointer_size)) as usize;
        if min_stack_params == 0 {
            return;
        }
        let placeholder_ty = type_from_size(pointer_size, false);
        self.ensure_incoming_stack_param_binding(min_stack_params - 1, placeholder_ty);
    }

    pub(in crate::midend::builder) fn unique_stack_slot_binding_name(
        &self,
        base_name: &str,
        id: StackSlotId,
    ) -> String {
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
        self.used_param_local_names.contains(name) || self.temps.contains_key(name)
    }

    pub(in crate::midend::builder) fn rsp_local_display_offset(&self, offset: i64) -> i64 {
        // `local_X` names X bytes below entry-rsp, which is the same
        // coordinate the rbp-relative branch uses (`offset.unsigned_abs()` on
        // a negative displacement). An `rsp`-relative displacement is measured
        // from steady-state rsp, and steady-state rsp sits `stack_frame_size`
        // below entry-rsp, so the conversion is one subtraction -- for
        // displacements on either side of it, since a negative one simply
        // lands further below.
        //
        // This used to apply that only to `offset >= 0 && stack_frame_size >
        // offset` and fall back to `offset.unsigned_abs()` otherwise, which
        // named different storage identically: `rsp-16` and `rsp+16` are
        // thirty-two bytes apart and both displayed as `local_10`, so the
        // second slot to arrive took a `_N` suffix to avoid the collision.
        // Measured on the sample-set, 214 such suffixed duplicates across 59
        // functions.
        //
        // It is one problem with the prologue scan, not two. Every one of the
        // 63 collisions instrumented occurred while `stack_frame_size` was
        // zero -- and repairing that alone made this *worse*, 214 duplicates
        // to 243, because a real frame size started remapping the positive
        // branch into the range the fallback occupied. Both together: 7.
        let below_entry = self.stack_frame_size.saturating_sub(offset);
        if below_entry >= 0 {
            below_entry
        } else {
            // Above entry-rsp: the caller's frame, not a local of ours. Keep
            // it distinct from anything below rather than folding the sign.
            offset.saturating_add(self.stack_frame_size)
        }
    }

    pub(in crate::midend::builder) fn resolve_stack_address_from_memory_op(
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

/// Name for a well-known Windows TEB field at the given `FS_OFFSET`/
/// `GS_OFFSET`-relative byte offset, if any -- `None` for anything else
/// (still renders as a plain, correct `PreHirExpr::Load`, just without a
/// descriptive name; this table isn't meant to be exhaustive, only to
/// cover the handful of fields that come up often enough in practice to
/// be worth naming, especially the classic `PEB.BeingDebugged` anti-debug
/// check chain: `TEB.ProcessEnvironmentBlock` then `+0x2`). Offsets are
/// part of the stable, publicly-documented (if unofficial) portion of the
/// TEB/TIB layout that hasn't changed across Windows versions; x86 and
/// x64 have different offsets for the same field throughout, purely from
/// every preceding pointer-sized field differing in size.
fn teb_field_name(offset: i64, is_64bit: bool) -> Option<(&'static str, bool)> {
    // (name, is_pointer) -- `is_pointer` picks the `Cast` type
    // `try_teb_field_var` wraps the name in: most TEB fields worth
    // naming are themselves pointers (`ExceptionList`,
    // `StackBase`/`StackLimit`, `Self`, `ThreadLocalStoragePointer`,
    // `ProcessEnvironmentBlock`); `ClientId`'s two fields are `HANDLE`s
    // (pointer-sized but not really pointers) and `LastErrorValue` is a
    // plain `DWORD` -- both rendered as a 4-byte unsigned int rather than
    // a pointer type.
    Some(if is_64bit {
        match offset {
            0x00 => ("teb_ExceptionList", true),
            0x08 => ("teb_StackBase", true),
            0x10 => ("teb_StackLimit", true),
            0x30 => ("teb_Self", true),
            0x40 => ("teb_ClientId_ProcessId", false),
            0x48 => ("teb_ClientId_ThreadId", false),
            0x58 => ("teb_ThreadLocalStoragePointer", true),
            0x60 => ("teb_ProcessEnvironmentBlock", true),
            0x68 => ("teb_LastErrorValue", false),
            _ => return None,
        }
    } else {
        match offset {
            0x00 => ("teb_ExceptionList", true),
            0x04 => ("teb_StackBase", true),
            0x08 => ("teb_StackLimit", true),
            0x18 => ("teb_Self", true),
            0x20 => ("teb_ClientId_ProcessId", false),
            0x24 => ("teb_ClientId_ThreadId", false),
            0x2c => ("teb_ThreadLocalStoragePointer", true),
            0x30 => ("teb_ProcessEnvironmentBlock", true),
            0x34 => ("teb_LastErrorValue", false),
            _ => return None,
        }
    })
}

/// Name for a well-known field of the Process Environment Block (PEB),
/// reached via `TEB.ProcessEnvironmentBlock` (see [`teb_field_name`]), at
/// the given byte offset -- currently just `BeingDebugged`, the field the
/// classic `fs:/gs: -> TEB.ProcessEnvironmentBlock -> PEB+0x2` anti-debug
/// check chain reads. Unlike [`teb_field_name`], the offset is the same
/// on x86 and x64: `InheritedAddressSpace` and `ReadImageFileExecOptions`
/// are both single bytes at offsets 0x0/0x1 ahead of it, and neither
/// field's size depends on pointer width.
fn peb_field_name(offset: i64) -> Option<(&'static str, bool)> {
    match offset {
        0x02 => Some(("peb_BeingDebugged", false)),
        _ => None,
    }
}

fn parse_stack_immediate(text: &str) -> Option<i64> {
    if let Some(hex) = text.strip_prefix("0X") {
        i64::from_str_radix(hex, 16).ok()
    } else {
        text.parse().ok()
    }
}

/// [`PreviewBuilder::apply_x86_32_stack_purge_arity_floor`]'s per-block
/// scan: `ret imm16` lifts as an extra `IntAdd(ESP, imm16)` sharing the
/// same originating-instruction address as the return-address-pop
/// `IntAdd(ESP, pointer_size)` and the `Return` op itself (confirmed via
/// raw p-code dump of a real `ret $0xc`: two same-address `IntAdd
/// v(ESP)` ops immediately before `Return`). Summing every constant ESP
/// adjustment that shares the `Return` op's address and subtracting the
/// pointer-size pop baseline isolates just the `RET` instruction's own
/// effect -- a plain `ret` (cdecl, caller cleanup) has only the pop, so
/// this returns `None` for it, correctly contributing no forced minimum
/// arity. Restricting to same-address ops (rather than summing every
/// ESP adjustment in the block) also means this can't be thrown off by
/// an unrelated, differently-addressed `add esp,N` used elsewhere in the
/// same block for local-variable stack cleanup.
fn x86_32_stack_purge_for_block(
    pcode: &PcodeFunction,
    block_idx: usize,
    pointer_size: u32,
) -> Option<i64> {
    let block = pcode.blocks.get(block_idx)?;
    let ret_addr = block
        .ops
        .iter()
        .find(|op| op.opcode == PcodeOpcode::Return)?
        .address;
    let mut total: i64 = 0;
    for op in &block.ops {
        if op.address != ret_addr || !matches!(op.opcode, PcodeOpcode::IntAdd | PcodeOpcode::IntSub)
        {
            continue;
        }
        let Some(out) = op.output.as_ref() else {
            continue;
        };
        if !(is_register_space_id(out.space_id) && out.offset == 0x10) {
            continue;
        }
        let Some(delta) = op.inputs.get(1).and_then(const_offset) else {
            continue;
        };
        total += if op.opcode == PcodeOpcode::IntAdd {
            delta
        } else {
            -delta
        };
    }
    let purge = total - i64::from(pointer_size);
    (purge > 0).then_some(purge)
}
