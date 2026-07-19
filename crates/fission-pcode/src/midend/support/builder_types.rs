use super::*;

pub(crate) const UNIQUE_SPACE_ID: u64 = 3;
pub(crate) const RUST_SLEIGH_UNIQUE_SPACE_ID: u64 = 2;
pub(crate) const REGISTER_SPACE_ID: u64 = 1;
pub(crate) const RUST_SLEIGH_REGISTER_SPACE_ID: u64 = 4;
pub(crate) const RUST_SLEIGH_ALT_REGISTER_SPACE_ID: u64 = 5;

pub(crate) fn is_register_space_id(space_id: u64) -> bool {
    space_id == REGISTER_SPACE_ID
        || space_id == RUST_SLEIGH_REGISTER_SPACE_ID
        || space_id == RUST_SLEIGH_ALT_REGISTER_SPACE_ID
}

pub(crate) fn is_unique_space_id(space_id: u64) -> bool {
    space_id == UNIQUE_SPACE_ID || space_id == RUST_SLEIGH_UNIQUE_SPACE_ID
}

pub(crate) fn is_register_varnode(vn: &Varnode) -> bool {
    is_register_space_id(vn.space_id)
}

pub(crate) use fission_midend_structuring::{
    CONDITION_RECOVERY_SUBCALL_LIMIT, ConditionalTailKey, IfLoweringBudget, LinearBodyCacheKey,
    LinearExit, LoweredTerminator, SWITCH_CHAIN_PARSE_BUDGET_MAX,
};
/// Initial SESE recovery is a proof pass, not the final fallback renderer.
/// Once proof-oriented recovery exceeds this ceiling, callers should fail
/// closed and let the cheaper whole-function linear fallback render payloads.
pub(crate) use fission_midend_structuring::SESE_REGION_PROOF_BUDGET_CALLS;
pub(crate) const BRANCH_CONDITION_RECOVERY_BUDGET_MIN: usize = 2048;
pub(crate) const BRANCH_CONDITION_RECOVERY_BUDGET_PER_BLOCK: usize = 32;
pub(crate) const BRANCH_CONDITION_RECOVERY_BUDGET_MAX: usize = 32768;
pub(crate) const PASSTHROUGH_PEEL_MAX_STEPS: usize = 48;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StackBase {
    Rsp,
    Rbp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StackSlot {
    pub(crate) id: StackSlotId,
    pub(crate) name: String,
    pub(crate) ty: NirType,
    pub(crate) origin: NirBindingOrigin,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct VarnodeKey {
    pub(crate) space_id: u64,
    pub(crate) offset: u64,
    pub(crate) size: u32,
    pub(crate) is_constant: bool,
    pub(crate) constant_val: i64,
}

impl From<&Varnode> for VarnodeKey {
    fn from(value: &Varnode) -> Self {
        Self {
            space_id: value.space_id,
            offset: value.offset,
            size: value.size,
            is_constant: value.is_constant,
            constant_val: value.constant_val,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct MaterializedVarnodeKey {
    pub(crate) varnode: VarnodeKey,
    pub(crate) def_addr: u64,
    pub(crate) def_seq: u32,
}

impl MaterializedVarnodeKey {
    pub(crate) fn new(vn: &Varnode, op: &PcodeOp) -> Self {
        Self {
            varnode: VarnodeKey::from(vn),
            def_addr: op.address,
            def_seq: op.seq_num,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct DefSite<'a> {
    pub(crate) block_idx: usize,
    pub(crate) op_idx: usize,
    pub(crate) _marker: std::marker::PhantomData<&'a PcodeOp>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct LoweringSite {
    pub(crate) block_idx: usize,
    pub(crate) op_idx: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct SubpieceOrigin {
    pub(crate) base: VarnodeKey,
    pub(crate) base_vn: Varnode,
    pub(crate) base_size: u32,
    pub(crate) byte_offset: i64,
    pub(crate) piece_size: u32,
}
