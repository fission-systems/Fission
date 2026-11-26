//! Memory-oracle identity, domain, byte-order, and address-boundary contracts (#1273).
//!
//! Oracle refinements are scoped strictly to a single query and bound to an
//! explicit memory snapshot identity, byte order, address wrap rule, and
//! missing-byte policy. Unread addresses are never claimed to match concrete
//! memory, and ordinary symbolic arrays retain standard uninterpreted array
//! semantics unless explicitly bound.

/// Immutable identifier for a concrete memory snapshot consulted by a query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MemorySnapshotId(pub u64);

impl MemorySnapshotId {
    #[must_use]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }
}

/// Byte ordering used when packing multibyte reads from byte-addressed oracle memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum MemoryByteOrder {
    #[default]
    LittleEndian,
    BigEndian,
}

/// Policy for multibyte oracle reads whose `base_addr + offset` crosses `2^index_bits - 1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum AddressWrapPolicy {
    /// Reject overflow past the declared index width as `Unknown(OracleBoundaryOverflow)`.
    #[default]
    RejectOverflow,
    /// Wrap byte addresses modulo `2^index_bits`.
    WrapAtIndexWidth,
}

/// Policy when the oracle returns `None` (unmapped/missing byte) at a queried concrete address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum MissingBytePolicy {
    /// Leave the unread/missing byte unconstrained (symbolic free) without claiming oracle agreement.
    #[default]
    UnconstrainedFree,
    /// Refuse to answer when a queried concrete address lacks oracle backing (`Unknown(OracleMissingByte)`).
    RejectUnknown,
}

/// Explicit binding between a symbolic array variable and a concrete oracle memory snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryBinding {
    pub array_name: String,
    pub space_id: u64,
    pub snapshot_id: MemorySnapshotId,
    pub index_bits: u32,
    pub value_bits: u32,
    pub byte_order: MemoryByteOrder,
    pub wrap_policy: AddressWrapPolicy,
    pub missing_byte_policy: MissingBytePolicy,
}

impl MemoryBinding {
    #[must_use]
    pub fn new(
        array_name: impl Into<String>,
        space_id: u64,
        snapshot_id: MemorySnapshotId,
        index_bits: u32,
        value_bits: u32,
    ) -> Self {
        Self {
            array_name: array_name.into(),
            space_id,
            snapshot_id,
            index_bits,
            value_bits,
            byte_order: MemoryByteOrder::LittleEndian,
            wrap_policy: AddressWrapPolicy::RejectOverflow,
            missing_byte_policy: MissingBytePolicy::UnconstrainedFree,
        }
    }

    #[must_use]
    pub const fn with_byte_order(mut self, byte_order: MemoryByteOrder) -> Self {
        self.byte_order = byte_order;
        self
    }

    #[must_use]
    pub const fn with_wrap_policy(mut self, wrap_policy: AddressWrapPolicy) -> Self {
        self.wrap_policy = wrap_policy;
        self
    }

    #[must_use]
    pub const fn with_missing_byte_policy(
        mut self,
        missing_byte_policy: MissingBytePolicy,
    ) -> Self {
        self.missing_byte_policy = missing_byte_policy;
        self
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.array_name.trim().is_empty() {
            return Err("memory binding array_name must not be empty".to_string());
        }
        if !(1..=64).contains(&self.index_bits) {
            return Err(format!(
                "memory binding index_bits {} must be in 1..=64",
                self.index_bits
            ));
        }
        if !(1..=64).contains(&self.value_bits) {
            return Err(format!(
                "memory binding value_bits {} must be in 1..=64",
                self.value_bits
            ));
        }
        Ok(())
    }
}

/// One concrete oracle address access observed or found missing during CEGAR refinement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OracleReadRecord {
    pub array_name: String,
    pub space_id: u64,
    pub snapshot_id: Option<MemorySnapshotId>,
    pub address: u64,
    pub bit_width: u32,
    pub byte_order: MemoryByteOrder,
    /// `Some(v)` when concrete bytes existed in the oracle; `None` when missing.
    pub value: Option<u64>,
}

/// Explicit declaration of which oracle identity and concrete addresses a published
/// model was conditioned on (never claiming agreement for unread addresses).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OracleModelScope {
    pub oracle_present: bool,
    pub snapshot_id: Option<MemorySnapshotId>,
    pub bindings: Vec<MemoryBinding>,
    /// Concrete reads verified against backing oracle bytes.
    pub observed_reads: Vec<OracleReadRecord>,
    /// Concrete reads where oracle had no bytes (`MissingBytePolicy::UnconstrainedFree`).
    pub missing_reads: Vec<OracleReadRecord>,
}

impl OracleModelScope {
    #[must_use]
    pub fn symbolic_only() -> Self {
        Self::default()
    }
}

/// Read-only concrete memory oracle consulted during solver CEGAR refinement.
pub trait MemoryOracle {
    /// Read one concrete byte from `space_id` at `addr`, or `None` if unmapped/unread.
    fn read_concrete(&self, space_id: u64, addr: u64) -> Option<u8>;

    /// Immutable snapshot identity for the oracle's backing memory, if declared.
    fn snapshot_id(&self) -> Option<MemorySnapshotId> {
        None
    }

    /// Declared multibyte byte order for `space_id`.
    fn byte_order(&self, _space_id: u64) -> MemoryByteOrder {
        MemoryByteOrder::LittleEndian
    }

    /// Address-width overflow/wrap policy for multibyte reads in `space_id`.
    fn wrap_policy(&self, _space_id: u64) -> AddressWrapPolicy {
        AddressWrapPolicy::RejectOverflow
    }

    /// Behavior when any byte in a multibyte read is missing (`None`).
    fn missing_byte_policy(&self, _space_id: u64) -> MissingBytePolicy {
        MissingBytePolicy::UnconstrainedFree
    }

    /// Explicit symbolic-array-to-memory-space bindings declared by this oracle.
    ///
    /// When empty, queries fall back to the legacy `space_<n>` array name prefix.
    /// When non-empty, only listed arrays consult this oracle.
    fn memory_bindings(&self) -> Vec<MemoryBinding> {
        Vec::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OracleReadOutcome {
    Value(u64),
    MissingByte(u64),
    BoundaryOverflow,
}

#[must_use]
pub(crate) fn read_oracle_with_policy(
    oracle: &dyn MemoryOracle,
    space_id: u64,
    base_addr: u64,
    index_bits: u32,
    bit_width: u32,
    byte_order: MemoryByteOrder,
    wrap_policy: AddressWrapPolicy,
) -> OracleReadOutcome {
    if bit_width == 0 || bit_width > 64 || index_bits == 0 || index_bits > 64 {
        return OracleReadOutcome::BoundaryOverflow;
    }
    let index_mask = if index_bits >= 64 {
        u64::MAX
    } else {
        (1u64 << index_bits) - 1
    };
    let masked_base = base_addr & index_mask;
    let byte_count = u64::from(bit_width.div_ceil(8));
    let mut val = 0u64;

    for byte_idx in 0..byte_count {
        let addr = match wrap_policy {
            AddressWrapPolicy::RejectOverflow => {
                let Some(next) = masked_base.checked_add(byte_idx) else {
                    return OracleReadOutcome::BoundaryOverflow;
                };
                if next > index_mask {
                    return OracleReadOutcome::BoundaryOverflow;
                }
                next
            }
            AddressWrapPolicy::WrapAtIndexWidth => masked_base.wrapping_add(byte_idx) & index_mask,
        };
        let Some(byte) = oracle.read_concrete(space_id, addr) else {
            return OracleReadOutcome::MissingByte(addr);
        };
        match byte_order {
            MemoryByteOrder::LittleEndian => {
                val |= u64::from(byte) << (byte_idx * 8);
            }
            MemoryByteOrder::BigEndian => {
                val = (val << 8) | u64::from(byte);
            }
        }
    }

    let val_mask = if bit_width >= 64 {
        u64::MAX
    } else {
        (1u64 << bit_width) - 1
    };
    OracleReadOutcome::Value(val & val_mask)
}
