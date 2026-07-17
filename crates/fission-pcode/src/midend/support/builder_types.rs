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

pub(crate) const CONDITION_RECOVERY_BUDGET_MS: f64 = 10.0;
pub(crate) const CONDITION_RECOVERY_SUBCALL_LIMIT: usize = 512;
/// Initial SESE recovery is a proof pass, not the final fallback renderer.
/// Once proof-oriented recovery exceeds this ceiling, callers should fail
/// closed and let the cheaper whole-function linear fallback render payloads.
pub(crate) const SESE_REGION_PROOF_BUDGET_MS: f64 = 500.0;
pub(crate) const BRANCH_CONDITION_RECOVERY_BUDGET_MIN: usize = 2048;
pub(crate) const BRANCH_CONDITION_RECOVERY_BUDGET_PER_BLOCK: usize = 32;
pub(crate) const BRANCH_CONDITION_RECOVERY_BUDGET_MAX: usize = 32768;
pub(crate) const PASSTHROUGH_PEEL_MAX_STEPS: usize = 48;
pub(crate) const SWITCH_CHAIN_PARSE_BUDGET_MAX: usize = 16;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LoweredTerminator {
    Fallthrough(Option<u64>),
    Goto(u64),
    Cond {
        cond: HirExpr,
        true_target: u64,
        false_target: Option<u64>,
    },
    Switch {
        expr: HirExpr,
        targets: Vec<u64>,
        default_target: Option<u64>, // Usually the last target or something specific
        /// Offset to add to ordinal case indices when the switch selector was
        /// adjusted by the compiler (e.g. `sel = orig - min_val`).
        /// case value = `min_val + ordinal_index`.  Zero when unknown/unrecovered.
        min_val: i64,
        proof: Option<DispatcherProofUnit>,
    },
    Return(Option<HirExpr>),
    Unsupported {
        evidence: UnsupportedControlEvidence,
        target_expr: Option<HirExpr>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum LinearExit {
    Join(usize),
    Return,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct LinearBodyCacheKey {
    pub(crate) start_idx: usize,
    pub(crate) exit: LinearExit,
    pub(crate) region_recovery: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ConditionalTailKey {
    pub(crate) true_idx: usize,
    pub(crate) false_idx: usize,
    pub(crate) exit: LinearExit,
    pub(crate) region_recovery: bool,
}

#[derive(Debug)]
pub(crate) struct IfLoweringBudget {
    pub(crate) enabled: bool,
    pub(crate) start: Instant,
    pub(crate) subcalls: usize,
    pub(crate) tripped: bool,
    pub(crate) idx: usize,
    pub(crate) block_addr: u64,
    pub(crate) label: &'static str,
    pub(crate) structuring_start: Option<Instant>,
}

#[derive(Debug, Clone)]
pub(crate) struct SubpieceOrigin {
    pub(crate) base: VarnodeKey,
    pub(crate) base_vn: Varnode,
    pub(crate) base_size: u32,
    pub(crate) byte_offset: i64,
    pub(crate) piece_size: u32,
}
