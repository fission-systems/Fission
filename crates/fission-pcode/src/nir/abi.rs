use crate::arch::x86::unique_x86_register_name;
use crate::nir::cspec::{RegisterNamer, register_namer_for_abi};

use super::support::{StackBase, is_register_varnode};
use super::{CallingConvention, NirBindingOrigin, UNIQUE_SPACE_ID, Varnode};

fn param_namer(abi: CallingConvention, int_param_offsets: &[u64]) -> RegisterNamer {
    let mut namer = register_namer_for_abi(abi);
    namer.int_param_offsets = int_param_offsets.to_vec();
    namer
}

fn param_slot_for_name_family(
    namer: &RegisterNamer,
    name: &str,
    param_offsets: &[u64],
) -> Option<usize> {
    let name_family = namer.gpr_family_index_for_name(name)?;
    param_offsets.iter().position(|&off| {
        namer
            .hw_name_at(off, namer.param_slot_size())
            .and_then(|hw| namer.gpr_family_index_for_name(&hw))
            == Some(name_family)
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
    fn classify_stack_slot_origin(
        &self,
        is_64bit: bool,
        stack_frame_size: i64,
        base: StackBase,
        offset: i64,
    ) -> NirBindingOrigin;
}

#[derive(Debug, Clone)]
pub(crate) struct AbiState {
    pub(crate) abi: CallingConvention,
    pub(crate) is_64bit: bool,
    pub(crate) pointer_size: u32,
    pub(crate) stack_frame_size: i64,
    /// Ghidra-style .cspec-resolved integer param register offsets (REGISTER-space).
    pub(crate) cspec_param_offsets: Option<Vec<u64>>,
    /// Stack argument base offset from .cspec (overrides ABI-specific default).
    pub(crate) cspec_stack_arg_base: Option<i64>,
    /// Return-address stack size from .cspec prototype (`extrapop`).
    pub(crate) cspec_extrapop: Option<i64>,
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
            cspec_param_offsets: None,
            cspec_stack_arg_base: None,
            cspec_extrapop: None,
        }
    }

    /// Creates an `AbiState` with Ghidra-style .cspec overrides applied.
    pub(crate) fn new_with_cspec(
        abi: CallingConvention,
        is_64bit: bool,
        pointer_size: u32,
        stack_frame_size: i64,
        cspec_param_offsets: Option<Vec<u64>>,
        cspec_stack_arg_base: Option<i64>,
        cspec_extrapop: Option<i64>,
    ) -> Self {
        Self {
            abi,
            is_64bit,
            pointer_size,
            stack_frame_size,
            cspec_param_offsets,
            cspec_stack_arg_base,
            cspec_extrapop,
        }
    }

    fn register_namer(&self) -> RegisterNamer {
        let mut namer = register_namer_for_abi(self.abi);
        namer.int_param_offsets = self.effective_param_offsets().to_vec();
        namer.pointer_size = self.pointer_size;
        namer
    }

    /// Integer parameter register offsets from `.cspec` (empty when not loaded).
    pub(crate) fn effective_param_offsets(&self) -> &[u64] {
        self.cspec_param_offsets.as_deref().unwrap_or(&[])
    }

    /// `(offset, size)` slots derived from `.cspec` integer params.
    pub(crate) fn effective_param_reg_slots(&self) -> Vec<(u64, u32)> {
        self.effective_param_offsets()
            .iter()
            .map(|&off| (off, self.pointer_size))
            .collect()
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
        let offsets = self.effective_param_offsets();
        if is_register_varnode(vn) {
            return offsets.iter().position(|&off| off == vn.offset);
        }
        if vn.space_id == UNIQUE_SPACE_ID
            && let Some(name) = unique_x86_register_name(vn.offset, vn.size)
        {
            return self.param_slot_for_name(name);
        }
        None
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
        param_slot_for_name_family(&self.register_namer(), name, self.effective_param_offsets())
    }

    pub(crate) fn param_name(&self, slot: usize) -> String {
        format!("param_{}", slot + 1)
    }

    pub(crate) fn param_hw_name(&self, slot: usize) -> Option<String> {
        let offset = *self.effective_param_offsets().get(slot)?;
        self.register_namer()
            .hw_name_at(offset, self.register_namer().param_slot_size())
    }

    pub(crate) fn stack_argument_index(&self, offset: i64) -> Option<usize> {
        if !self.is_64bit {
            return None;
        }
        let base = self.cspec_stack_arg_base?;
        let shift = self.cspec_extrapop.unwrap_or(0);
        let ghidra_offset = offset + shift;
        if ghidra_offset < base || (ghidra_offset - base) % i64::from(self.pointer_size) != 0 {
            return None;
        }
        Some(((ghidra_offset - base) / i64::from(self.pointer_size)) as usize)
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
    namer: &RegisterNamer,
) -> Option<usize> {
    use crate::pcode::PcodeOpcode;
    use std::collections::{HashMap, HashSet, VecDeque};

    let entry = pcode.blocks.first()?;
    let num_params = namer.int_param_offsets.len();
    if num_params == 0 {
        return None;
    }

    let block_map: HashMap<u32, &crate::pcode::PcodeBasicBlock> =
        pcode.blocks.iter().map(|b| (b.index, b)).collect();

    let has_no_succs = pcode.blocks.iter().all(|b| b.successors.is_empty());
    let mut block_successors = HashMap::new();
    if has_no_succs && pcode.blocks.len() > 1 {
        let address_to_index = super::cfg::build_address_to_index_map(pcode);
        let layout_fallthrough = super::cfg::build_layout_fallthrough_map(pcode);
        let succ_indices =
            super::cfg::build_successor_index_map(pcode, &address_to_index, &layout_fallthrough);
        for (idx, block) in pcode.blocks.iter().enumerate() {
            let succs_for_block = succ_indices[idx]
                .iter()
                .map(|&s_idx| pcode.blocks[s_idx].index)
                .collect::<Vec<u32>>();
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
                    namer.register_name_with_param_owned(input.offset, input.size)
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
                        namer.register_name_with_param_owned(output.offset, output.size)
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
