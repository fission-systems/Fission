use super::support::{
    StackBase, aarch64_ghidra_reg_name, aarch64_gpr_family_index, arm32_ghidra_reg_name,
    arm32_gpr_family_index, is_register_varnode, loongarch_ghidra_reg_name_for_abi,
    loongarch_gpr_family_index, mips_ghidra_reg_name_for_abi, mips_gpr_family_index,
    powerpc_ghidra_reg_name, powerpc_gpr_family_index, register_name_with_param,
    unique_register_name, x64_ghidra_reg_name,
};
use super::{CallingConvention, NirBindingOrigin, UNIQUE_SPACE_ID, Varnode};

fn x64_param_slot_for_name_family(name: &str, abi: CallingConvention) -> Option<usize> {
    let name_family = crate::arch::x86::x86_gpr_family_index(name)?;
    abi.param_offsets().iter().position(|&off| {
        x64_ghidra_reg_name(off)
            .and_then(crate::arch::x86::x86_gpr_family_index)
            .is_some_and(|family| family == name_family)
    })
}

fn aarch64_param_slot_for_name_family(name: &str, abi: CallingConvention) -> Option<usize> {
    let name_family = aarch64_gpr_family_index(name)?;
    abi.param_offsets().iter().position(|&off| {
        aarch64_ghidra_reg_name(off, 8)
            .and_then(aarch64_gpr_family_index)
            .is_some_and(|family| family == name_family)
    })
}

fn arm32_param_slot_for_name_family(name: &str, abi: CallingConvention) -> Option<usize> {
    let name_family = arm32_gpr_family_index(name)?;
    abi.param_offsets().iter().position(|&off| {
        arm32_ghidra_reg_name(off, 4)
            .and_then(arm32_gpr_family_index)
            .is_some_and(|family| family == name_family)
    })
}

fn powerpc_param_slot_for_name_family(name: &str, abi: CallingConvention) -> Option<usize> {
    let name_family = powerpc_gpr_family_index(name)?;
    let slot_size = match abi {
        CallingConvention::PowerPc64 => 8,
        _ => 4,
    };
    abi.param_offsets().iter().position(|&off| {
        powerpc_ghidra_reg_name(off, slot_size)
            .and_then(powerpc_gpr_family_index)
            .is_some_and(|family| family == name_family)
    })
}

fn loongarch_param_slot_for_name_family(name: &str, abi: CallingConvention) -> Option<usize> {
    let name_family = loongarch_gpr_family_index(name)?;
    let slot_size = match abi {
        CallingConvention::LoongArch64 => 8,
        _ => 4,
    };
    abi.param_offsets().iter().position(|&off| {
        loongarch_ghidra_reg_name_for_abi(off, slot_size, abi)
            .and_then(loongarch_gpr_family_index)
            .is_some_and(|family| family == name_family)
    })
}

fn mips_param_slot_for_name_family(name: &str, abi: CallingConvention) -> Option<usize> {
    let name_family = mips_gpr_family_index(name)?;
    let slot_size = match abi {
        CallingConvention::Mips64 => 8,
        _ => 4,
    };
    abi.param_offsets().iter().position(|&off| {
        mips_ghidra_reg_name_for_abi(off, slot_size, abi)
            .and_then(mips_gpr_family_index)
            .is_some_and(|family| family == name_family)
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct CarrierResource {
    pub(crate) class: super::CarrierClass,
    pub(crate) slot: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CarrierAssignment {
    pub(crate) resource: CarrierResource,
    pub(crate) coverage_penalty: usize,
    pub(crate) hole_penalty: usize,
    pub(crate) mismatch_penalty: usize,
    pub(crate) duplication_penalty: usize,
    pub(crate) use_after_call_penalty: usize,
    pub(crate) stack_role_conflict_penalty: usize,
}

impl CarrierAssignment {
    pub(crate) fn total_cost(&self) -> usize {
        self.coverage_penalty
            + self.hole_penalty
            + self.mismatch_penalty
            + self.duplication_penalty
            + self.use_after_call_penalty
            + self.stack_role_conflict_penalty
    }
}

pub(crate) trait AbiProvider {
    fn abi(&self) -> CallingConvention;
    fn param_slot_for_varnode(&self, vn: &Varnode) -> Option<usize>;
    fn param_slot_for_name(&self, name: &str) -> Option<usize>;
    fn param_name(&self, slot: usize) -> String {
        format!("param_{}", slot + 1)
    }
    fn param_hw_name(&self, slot: usize) -> Option<&'static str>;
    fn stack_argument_index(&self, pointer_size: u32, offset: i64) -> Option<usize>;
    fn classify_stack_slot_origin(
        &self,
        is_64bit: bool,
        stack_frame_size: i64,
        base: StackBase,
        offset: i64,
    ) -> NirBindingOrigin;
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AbiState {
    pub(crate) abi: CallingConvention,
    pub(crate) is_64bit: bool,
    pub(crate) pointer_size: u32,
    pub(crate) stack_frame_size: i64,
}

impl AbiState {
    pub(crate) fn new(
        abi: CallingConvention,
        is_64bit: bool,
        pointer_size: u32,
        stack_frame_size: i64,
    ) -> Self {
        Self {
            abi,
            is_64bit,
            pointer_size,
            stack_frame_size,
        }
    }

    pub(crate) fn provider(&self) -> AbiKind {
        AbiKind::for_calling_convention(self.abi)
    }

    pub(crate) fn param_slot_for_varnode(&self, vn: &Varnode) -> Option<usize> {
        if !self.is_64bit
            && !matches!(
                self.abi,
                CallingConvention::Arm32
                    | CallingConvention::PowerPc32
                    | CallingConvention::LoongArch32
                    | CallingConvention::Mips32
            )
        {
            return None;
        }
        self.provider().param_slot_for_varnode(vn)
    }

    pub(crate) fn param_slot_for_name(&self, name: &str) -> Option<usize> {
        if !self.is_64bit
            && !matches!(
                self.abi,
                CallingConvention::Arm32
                    | CallingConvention::PowerPc32
                    | CallingConvention::LoongArch32
                    | CallingConvention::Mips32
            )
        {
            return None;
        }
        self.provider().param_slot_for_name(name)
    }

    pub(crate) fn param_name(&self, slot: usize) -> String {
        self.provider().param_name(slot)
    }

    pub(crate) fn param_hw_name(&self, slot: usize) -> Option<&'static str> {
        self.provider().param_hw_name(slot)
    }

    pub(crate) fn stack_argument_index(&self, offset: i64) -> Option<usize> {
        if !self.is_64bit {
            return None;
        }
        self.provider()
            .stack_argument_index(self.pointer_size, offset)
    }

    pub(crate) fn classify_stack_slot_origin(
        &self,
        base: StackBase,
        offset: i64,
    ) -> NirBindingOrigin {
        self.provider().classify_stack_slot_origin(
            self.is_64bit,
            self.stack_frame_size,
            base,
            offset,
        )
    }

    pub(crate) fn assign_carriers<'a, I>(&self, carriers: I) -> Vec<CarrierAssignment>
    where
        I: IntoIterator<Item = &'a Varnode>,
    {
        let mut assignments = Vec::new();
        let mut seen_slots = std::collections::BTreeSet::new();
        let mut expected_next = 0usize;
        for carrier in carriers {
            let Some(slot) = self.param_slot_for_varnode(carrier) else {
                continue;
            };
            let duplication_penalty = usize::from(!seen_slots.insert(slot));
            let hole_penalty = slot.saturating_sub(expected_next);
            expected_next = expected_next.max(slot.saturating_add(1));
            let class = if is_register_varnode(carrier) || carrier.space_id == UNIQUE_SPACE_ID {
                super::CarrierClass::Gpr
            } else {
                super::CarrierClass::LocalSlot
            };
            assignments.push(CarrierAssignment {
                resource: CarrierResource { class, slot },
                coverage_penalty: 0,
                hole_penalty,
                mismatch_penalty: 0,
                duplication_penalty,
                use_after_call_penalty: 0,
                stack_role_conflict_penalty: 0,
            });
        }
        assignments.sort_by(|lhs, rhs| {
            lhs.total_cost()
                .cmp(&rhs.total_cost())
                .then_with(|| lhs.resource.slot.cmp(&rhs.resource.slot))
        });
        assignments
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum AbiKind {
    WindowsX64(WindowsX64AbiProvider),
    Generic(GenericAbiProvider),
}

impl AbiKind {
    pub(crate) fn for_calling_convention(abi: CallingConvention) -> Self {
        match abi {
            CallingConvention::WindowsX64 => Self::WindowsX64(WindowsX64AbiProvider),
            _ => Self::Generic(GenericAbiProvider { abi }),
        }
    }
}

impl AbiProvider for AbiKind {
    fn abi(&self) -> CallingConvention {
        match self {
            Self::WindowsX64(provider) => provider.abi(),
            Self::Generic(provider) => provider.abi(),
        }
    }

    fn param_slot_for_varnode(&self, vn: &Varnode) -> Option<usize> {
        match self {
            Self::WindowsX64(provider) => provider.param_slot_for_varnode(vn),
            Self::Generic(provider) => provider.param_slot_for_varnode(vn),
        }
    }

    fn param_slot_for_name(&self, name: &str) -> Option<usize> {
        match self {
            Self::WindowsX64(provider) => provider.param_slot_for_name(name),
            Self::Generic(provider) => provider.param_slot_for_name(name),
        }
    }

    fn param_hw_name(&self, slot: usize) -> Option<&'static str> {
        match self {
            Self::WindowsX64(provider) => provider.param_hw_name(slot),
            Self::Generic(provider) => provider.param_hw_name(slot),
        }
    }

    fn stack_argument_index(&self, pointer_size: u32, offset: i64) -> Option<usize> {
        match self {
            Self::WindowsX64(provider) => provider.stack_argument_index(pointer_size, offset),
            Self::Generic(provider) => provider.stack_argument_index(pointer_size, offset),
        }
    }

    fn classify_stack_slot_origin(
        &self,
        is_64bit: bool,
        stack_frame_size: i64,
        base: StackBase,
        offset: i64,
    ) -> NirBindingOrigin {
        match self {
            Self::WindowsX64(provider) => {
                provider.classify_stack_slot_origin(is_64bit, stack_frame_size, base, offset)
            }
            Self::Generic(provider) => {
                provider.classify_stack_slot_origin(is_64bit, stack_frame_size, base, offset)
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct WindowsX64AbiProvider;

impl AbiProvider for WindowsX64AbiProvider {
    fn abi(&self) -> CallingConvention {
        CallingConvention::WindowsX64
    }

    fn param_slot_for_varnode(&self, vn: &Varnode) -> Option<usize> {
        if is_register_varnode(vn) {
            return register_name_with_param(vn.offset, vn.size, CallingConvention::WindowsX64)
                .and_then(|(_, index)| index);
        }
        if vn.space_id == UNIQUE_SPACE_ID
            && let Some(name) = unique_register_name(vn.offset, vn.size)
        {
            return self.param_slot_for_name(name);
        }
        None
    }

    fn param_slot_for_name(&self, name: &str) -> Option<usize> {
        x64_param_slot_for_name_family(name, CallingConvention::WindowsX64)
    }

    fn param_hw_name(&self, slot: usize) -> Option<&'static str> {
        CallingConvention::WindowsX64
            .param_offsets()
            .get(slot)
            .copied()
            .and_then(x64_ghidra_reg_name)
    }

    fn stack_argument_index(&self, pointer_size: u32, offset: i64) -> Option<usize> {
        if offset < 0x20 || (offset - 0x20) % i64::from(pointer_size) != 0 {
            return None;
        }
        Some(((offset - 0x20) / i64::from(pointer_size)) as usize)
    }

    fn classify_stack_slot_origin(
        &self,
        is_64bit: bool,
        stack_frame_size: i64,
        base: StackBase,
        offset: i64,
    ) -> NirBindingOrigin {
        match base {
            StackBase::Rsp if is_64bit && offset >= stack_frame_size => {
                NirBindingOrigin::HomeSlot(offset)
            }
            _ => NirBindingOrigin::StackOffset(offset),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct GenericAbiProvider {
    abi: CallingConvention,
}

impl AbiProvider for GenericAbiProvider {
    fn abi(&self) -> CallingConvention {
        self.abi
    }

    fn param_slot_for_varnode(&self, vn: &Varnode) -> Option<usize> {
        if !is_register_varnode(vn) {
            return None;
        }
        register_name_with_param(vn.offset, vn.size, self.abi).and_then(|(_, index)| index)
    }

    fn param_slot_for_name(&self, name: &str) -> Option<usize> {
        match self.abi {
            CallingConvention::AArch64 => aarch64_param_slot_for_name_family(name, self.abi),
            CallingConvention::Arm32 => arm32_param_slot_for_name_family(name, self.abi),
            CallingConvention::PowerPc32 | CallingConvention::PowerPc64 => {
                powerpc_param_slot_for_name_family(name, self.abi)
            }
            CallingConvention::LoongArch32 | CallingConvention::LoongArch64 => {
                loongarch_param_slot_for_name_family(name, self.abi)
            }
            CallingConvention::Mips32 | CallingConvention::Mips64 => {
                mips_param_slot_for_name_family(name, self.abi)
            }
            CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64 => {
                x64_param_slot_for_name_family(name, self.abi)
            }
        }
    }

    fn param_hw_name(&self, slot: usize) -> Option<&'static str> {
        let offset = self.abi.param_offsets().get(slot).copied()?;
        match self.abi {
            CallingConvention::AArch64 => aarch64_ghidra_reg_name(offset, 8),
            CallingConvention::Arm32 => arm32_ghidra_reg_name(offset, 4),
            CallingConvention::PowerPc32 => powerpc_ghidra_reg_name(offset, 4),
            CallingConvention::PowerPc64 => powerpc_ghidra_reg_name(offset, 8),
            CallingConvention::LoongArch32 => {
                loongarch_ghidra_reg_name_for_abi(offset, 4, self.abi)
            }
            CallingConvention::LoongArch64 => {
                loongarch_ghidra_reg_name_for_abi(offset, 8, self.abi)
            }
            CallingConvention::Mips32 => mips_ghidra_reg_name_for_abi(offset, 4, self.abi),
            CallingConvention::Mips64 => mips_ghidra_reg_name_for_abi(offset, 8, self.abi),
            CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64 => {
                x64_ghidra_reg_name(offset)
            }
        }
    }

    fn stack_argument_index(&self, _pointer_size: u32, _offset: i64) -> Option<usize> {
        None
    }

    fn classify_stack_slot_origin(
        &self,
        _is_64bit: bool,
        _stack_frame_size: i64,
        _base: StackBase,
        offset: i64,
    ) -> NirBindingOrigin {
        NirBindingOrigin::StackOffset(offset)
    }
}

pub fn infer_entry_register_param_arity(
    pcode: &crate::pcode::PcodeFunction,
    abi: CallingConvention,
) -> Option<usize> {
    use crate::pcode::PcodeOpcode;
    use std::collections::{HashMap, HashSet, VecDeque};

    let entry = pcode.blocks.first()?;
    let num_params = abi.param_offsets().len();
    if num_params == 0 {
        return None;
    }

    let block_map: HashMap<u32, &crate::pcode::PcodeBasicBlock> = pcode
        .blocks
        .iter()
        .map(|b| (b.index, b))
        .collect();

    let has_no_succs = pcode.blocks.iter().all(|b| b.successors.is_empty());
    let mut block_successors = HashMap::new();
    if has_no_succs && pcode.blocks.len() > 1 {
        let address_to_index = super::cfg::build_address_to_index_map(pcode);
        let layout_fallthrough = super::cfg::build_layout_fallthrough_map(pcode);
        let succ_indices = super::cfg::build_successor_index_map(pcode, &address_to_index, &layout_fallthrough);
        for (idx, block) in pcode.blocks.iter().enumerate() {
            let succs_for_block = succ_indices[idx].iter().map(|&s_idx| pcode.blocks[s_idx].index).collect::<Vec<u32>>();
            block_successors.insert(block.index, succs_for_block);
        }
    } else {
        for block in &pcode.blocks {
            block_successors.insert(block.index, block.successors.clone());
        }
    }

    let mut detected_params = HashSet::new();
    let mut visited_active_params: HashMap<u32, HashSet<usize>> = HashMap::new();
    let mut queue = VecDeque::new();

    let initial_active: HashSet<usize> = (0..num_params).collect();
    visited_active_params.insert(entry.index, initial_active.clone());
    queue.push_back((entry.index, initial_active));

    while let Some((block_idx, active_params)) = queue.pop_front() {
        let Some(block) = block_map.get(&block_idx) else {
            continue;
        };

        let mut current_active = active_params;
        for op in &block.ops {
            // 1. Check inputs first
            for input in &op.inputs {
                if input.is_constant || !is_register_varnode(input) {
                    continue;
                }
                if let Some((_, Some(param_index))) =
                    register_name_with_param(input.offset, input.size, abi)
                {
                    if current_active.contains(&param_index) {
                        detected_params.insert(param_index);
                    }
                }
            }

            // 2. If call, clobber scratch/parameter registers (remove from current_active)
            if matches!(op.opcode, PcodeOpcode::Call | PcodeOpcode::CallInd) {
                current_active.clear();
            }

            // 3. Check output to see if it writes/defines a parameter register
            if let Some(output) = &op.output {
                if !output.is_constant && is_register_varnode(output) {
                    if let Some((_, Some(param_index))) =
                        register_name_with_param(output.offset, output.size, abi)
                    {
                        current_active.remove(&param_index);
                    }
                }
            }
        }

        // Propagate current_active to successors
        if let Some(succs) = block_successors.get(&block_idx) {
            for &succ_idx in succs {
                let succ_visited = visited_active_params.entry(succ_idx).or_default();
                let mut new_active = HashSet::new();
                for &param in &current_active {
                    if !succ_visited.contains(&param) {
                        new_active.insert(param);
                    }
                }
                if !new_active.is_empty() {
                    for &param in &new_active {
                        succ_visited.insert(param);
                    }
                    queue.push_back((succ_idx, new_active));
                }
            }
        }
    }

    detected_params.iter().max().map(|&index| index + 1)
}
