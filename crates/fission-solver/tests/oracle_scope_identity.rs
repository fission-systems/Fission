//! Regression suite scoping memory-oracle refinements to explicit immutable
//! memory identities, domains, byte orders, and wrap/missing policies (#1273).

use fission_solver::{
    AddressWrapPolicy, MemoryBinding, MemoryByteOrder, MemoryOracle, MemorySnapshotId,
    MissingBytePolicy, SatResult, Solver, SymExpr, UnknownReason,
};
use std::collections::HashMap;

struct SnapshotOracle {
    snapshot_id: Option<MemorySnapshotId>,
    byte_order: MemoryByteOrder,
    wrap_policy: AddressWrapPolicy,
    missing_policy: MissingBytePolicy,
    bytes: HashMap<(u64, u64), u8>,
    bindings: Vec<MemoryBinding>,
}

impl SnapshotOracle {
    fn new(snapshot_id: Option<MemorySnapshotId>) -> Self {
        Self {
            snapshot_id,
            byte_order: MemoryByteOrder::LittleEndian,
            wrap_policy: AddressWrapPolicy::RejectOverflow,
            missing_policy: MissingBytePolicy::UnconstrainedFree,
            bytes: HashMap::new(),
            bindings: Vec::new(),
        }
    }

    fn with_byte(mut self, space_id: u64, addr: u64, byte: u8) -> Self {
        self.bytes.insert((space_id, addr), byte);
        self
    }

    fn with_bytes(mut self, space_id: u64, base: u64, slice: &[u8]) -> Self {
        for (i, &b) in slice.iter().enumerate() {
            self.bytes.insert((space_id, base + i as u64), b);
        }
        self
    }
}

impl MemoryOracle for SnapshotOracle {
    fn read_concrete(&self, space_id: u64, addr: u64) -> Option<u8> {
        self.bytes.get(&(space_id, addr)).copied()
    }

    fn snapshot_id(&self) -> Option<MemorySnapshotId> {
        self.snapshot_id
    }

    fn byte_order(&self, _space_id: u64) -> MemoryByteOrder {
        self.byte_order
    }

    fn wrap_policy(&self, _space_id: u64) -> AddressWrapPolicy {
        self.wrap_policy
    }

    fn missing_byte_policy(&self, _space_id: u64) -> MissingBytePolicy {
        self.missing_policy
    }

    fn memory_bindings(&self) -> Vec<MemoryBinding> {
        self.bindings.clone()
    }
}

fn select_space_byte_at(array_name: &str, idx_bits: u32, val_bits: u32, addr: u64) -> SymExpr {
    let mem = SymExpr::new_array_var(array_name, idx_bits, val_bits);
    let idx = SymExpr::new_const(addr, idx_bits);
    SymExpr::ArraySelect {
        array: Box::new(mem),
        index: Box::new(idx),
    }
}

fn eq_const(expr: SymExpr, val: u64, bits: u32) -> SymExpr {
    SymExpr::Eq(Box::new(expr), Box::new(SymExpr::new_const(val, bits)))
}

#[test]
fn oracle_a_then_oracle_b_then_no_oracle_never_leaks_lemmas() {
    let space = SymExpr::new_array_var("space_0", 64, 8);
    let ptr = SymExpr::new_var("ptr", 64);
    let sel = SymExpr::ArraySelect {
        array: Box::new(space),
        index: Box::new(ptr.clone()),
    };

    let oracle_a = SnapshotOracle::new(Some(MemorySnapshotId::new(1))).with_byte(0, 0x1000, 0x42);
    let oracle_b = SnapshotOracle::new(Some(MemorySnapshotId::new(2))).with_byte(0, 0x1000, 0x99);

    let mut reused = Solver::new();
    reused.assert(SymExpr::Eq(
        Box::new(ptr.clone()),
        Box::new(SymExpr::new_const(0x1000, 64)),
    ));
    let initial_assertions_len = reused.assertions().len();
    assert_eq!(initial_assertions_len, 1);

    // Query Oracle A (0x42): unconditional query, then check 0x42 vs 0x99.
    assert_eq!(
        reused.check_sat_with_oracle(Some(&oracle_a), &[]).unwrap(),
        SatResult::Sat
    );
    assert_eq!(reused.assertions().len(), initial_assertions_len);
    assert_eq!(
        reused
            .check_sat_with_oracle(Some(&oracle_a), &[eq_const(sel.clone(), 0x42, 8)])
            .unwrap(),
        SatResult::Sat
    );
    assert_eq!(
        reused
            .check_sat_with_oracle(Some(&oracle_a), &[eq_const(sel.clone(), 0x99, 8)])
            .unwrap(),
        SatResult::Unsat
    );

    // Query Oracle B (0x99) on the SAME solver: must NOT be poisoned by Oracle A's lemma!
    assert_eq!(
        reused
            .check_sat_with_oracle(Some(&oracle_b), &[eq_const(sel.clone(), 0x99, 8)])
            .unwrap(),
        SatResult::Sat
    );
    assert_eq!(
        reused
            .check_sat_with_oracle(Some(&oracle_b), &[eq_const(sel.clone(), 0x42, 8)])
            .unwrap(),
        SatResult::Unsat
    );

    // Query with NO oracle: every 8-bit value must remain satisfiable!
    for val in [0x42u64, 0x99, 0x77, 0x00] {
        assert_eq!(
            reused
                .check_sat_with_oracle(None, &[eq_const(sel.clone(), val, 8)])
                .unwrap(),
            SatResult::Sat
        );
    }
    assert_eq!(reused.assertions().len(), initial_assertions_len);

    // Compare against fresh solver instances.
    let mut fresh_b = Solver::new();
    fresh_b.assert(SymExpr::Eq(
        Box::new(ptr),
        Box::new(SymExpr::new_const(0x1000, 64)),
    ));
    assert_eq!(
        fresh_b
            .check_sat_with_oracle(Some(&oracle_b), &[eq_const(sel, 0x99, 8)])
            .unwrap(),
        SatResult::Sat
    );
}

#[test]
fn same_space_different_snapshot_identity_and_rejected_mismatch() {
    let sel = select_space_byte_at("ram_view", 64, 8, 0x2000);
    let snap1 = MemorySnapshotId::new(101);
    let snap2 = MemorySnapshotId::new(202);

    let binding_1 = MemoryBinding::new("ram_view", 0, snap1, 64, 8);
    let binding_2 = MemoryBinding::new("ram_view", 0, snap2, 64, 8);

    let oracle_1 = SnapshotOracle::new(Some(snap1)).with_byte(0, 0x2000, 0x11);
    let oracle_2 = SnapshotOracle::new(Some(snap2)).with_byte(0, 0x2000, 0x22);

    let mut solver = Solver::new();
    solver.assert(SymExpr::Neq(
        Box::new(sel.clone()),
        Box::new(SymExpr::new_const(0, 8)),
    ));

    // Query snapshot 1 with matching binding.
    assert_eq!(
        solver
            .check_sat_with_memory_bindings(
                Some(&oracle_1),
                std::slice::from_ref(&binding_1),
                &[eq_const(sel.clone(), 0x11, 8)],
            )
            .unwrap(),
        SatResult::Sat
    );
    let published = solver.published_model().expect("published model");
    assert_eq!(published.oracle_scope.snapshot_id, Some(snap1));
    assert_eq!(published.oracle_scope.observed_reads.len(), 1);
    assert_eq!(published.oracle_scope.observed_reads[0].address, 0x2000);
    assert_eq!(published.oracle_scope.observed_reads[0].value, Some(0x11));

    // Mismatched snapshot identity (binding=snap1, oracle=snap2) must be rejected cleanly.
    assert_eq!(
        solver
            .check_sat_with_memory_bindings(
                Some(&oracle_2),
                std::slice::from_ref(&binding_1),
                &[eq_const(sel.clone(), 0x22, 8)],
            )
            .unwrap(),
        SatResult::Unknown
    );
    assert!(matches!(
        solver.last_unknown(),
        Some(UnknownReason::OracleIdentityMismatch(_))
    ));
    assert!(solver.published_model().is_none());

    // Bindings without an active oracle are rejected cleanly.
    assert_eq!(
        solver
            .check_sat_with_memory_bindings(None, std::slice::from_ref(&binding_1), &[])
            .unwrap(),
        SatResult::Unknown
    );
    assert!(matches!(
        solver.last_unknown(),
        Some(UnknownReason::OracleIdentityMismatch(_))
    ));

    // Wrong sort width in binding is rejected cleanly.
    let bad_sort_binding = MemoryBinding::new("ram_view", 0, snap2, 32, 8);
    assert_eq!(
        solver
            .check_sat_with_memory_bindings(Some(&oracle_2), &[bad_sort_binding], &[])
            .unwrap(),
        SatResult::Unknown
    );
    assert!(matches!(
        solver.last_unknown(),
        Some(UnknownReason::OracleIdentityMismatch(_))
    ));

    // Subsequent valid query with snapshot 2 still succeeds!
    assert_eq!(
        solver
            .check_sat_with_memory_bindings(
                Some(&oracle_2),
                &[binding_2],
                &[eq_const(sel, 0x22, 8)],
            )
            .unwrap(),
        SatResult::Sat
    );
    assert_eq!(
        solver.published_model().unwrap().oracle_scope.snapshot_id,
        Some(snap2)
    );
}

#[test]
fn explicit_non_prefix_binding_leaves_ordinary_symbolic_arrays_free() {
    let snap = MemorySnapshotId::new(7);
    let mut oracle = SnapshotOracle::new(Some(snap)).with_byte(7, 0x40, 0xAA);
    oracle.bindings = vec![MemoryBinding::new("bound_mem", 7, snap, 64, 8)];

    let bound_sel = select_space_byte_at("bound_mem", 64, 8, 0x40);
    // Even though this array is named `space_7`, explicit bindings opt out of prefix matching!
    let unbound_space_sel = select_space_byte_at("space_7", 64, 8, 0x40);
    let ordinary_sel = select_space_byte_at("ordinary_arr", 64, 8, 0x40);

    let mut solver = Solver::new();
    // Bound array must match 0xAA.
    assert_eq!(
        solver
            .check_sat_with_oracle(Some(&oracle), &[eq_const(bound_sel.clone(), 0xAA, 8)])
            .unwrap(),
        SatResult::Sat
    );
    assert_eq!(
        solver
            .check_sat_with_oracle(Some(&oracle), &[eq_const(bound_sel, 0xBB, 8)])
            .unwrap(),
        SatResult::Unsat
    );

    // Unbound `space_7` and `ordinary_arr` remain pure symbolic arrays (0xBB is Sat!).
    assert_eq!(
        solver
            .check_sat_with_oracle(Some(&oracle), &[eq_const(unbound_space_sel, 0xBB, 8)])
            .unwrap(),
        SatResult::Sat
    );
    assert_eq!(
        solver
            .check_sat_with_oracle(Some(&oracle), &[eq_const(ordinary_sel, 0xBB, 8)])
            .unwrap(),
        SatResult::Sat
    );
}

#[test]
fn missing_byte_policies_and_unread_byte_honesty() {
    let sel_missing = select_space_byte_at("space_0", 64, 8, 0x3000);
    let sel_present = select_space_byte_at("space_0", 64, 8, 0x1000);

    // UnconstrainedFree: missing address is free, reported in missing_reads (NOT observed_reads).
    let oracle_free =
        SnapshotOracle::new(Some(MemorySnapshotId::new(1))).with_byte(0, 0x1000, 0x55);
    let mut solver = Solver::new();
    assert_eq!(
        solver
            .check_sat_with_oracle(
                Some(&oracle_free),
                &[eq_const(sel_missing.clone(), 0x12, 8)]
            )
            .unwrap(),
        SatResult::Sat
    );
    let scope = &solver.published_model().unwrap().oracle_scope;
    assert!(scope.observed_reads.is_empty());
    assert_eq!(scope.missing_reads.len(), 1);
    assert_eq!(scope.missing_reads[0].address, 0x3000);
    assert_eq!(scope.missing_reads[0].value, None);

    // Another free query on missing address also Sat with different value.
    assert_eq!(
        solver
            .check_sat_with_oracle(
                Some(&oracle_free),
                &[eq_const(sel_missing.clone(), 0x34, 8)]
            )
            .unwrap(),
        SatResult::Sat
    );

    // Concrete address 0x1000 appears in observed_reads; unread addresses never appear.
    assert_eq!(
        solver
            .check_sat_with_oracle(Some(&oracle_free), &[eq_const(sel_present, 0x55, 8)])
            .unwrap(),
        SatResult::Sat
    );
    let scope_present = &solver.published_model().unwrap().oracle_scope;
    assert_eq!(scope_present.observed_reads.len(), 1);
    assert_eq!(scope_present.observed_reads[0].address, 0x1000);
    assert_eq!(scope_present.observed_reads[0].value, Some(0x55));

    // RejectUnknown: missing address returns Unknown(OracleMissingByte).
    let mut oracle_strict = SnapshotOracle::new(Some(MemorySnapshotId::new(1)));
    oracle_strict.missing_policy = MissingBytePolicy::RejectUnknown;
    assert_eq!(
        solver
            .check_sat_with_oracle(Some(&oracle_strict), &[eq_const(sel_missing, 0x12, 8)])
            .unwrap(),
        SatResult::Unknown
    );
    assert_eq!(
        solver.last_unknown(),
        Some(&UnknownReason::OracleMissingByte {
            space_id: 0,
            address: 0x3000
        })
    );
    assert!(solver.published_model().is_none());
}

#[test]
fn multibyte_byte_order_little_endian_vs_big_endian() {
    let sel = select_space_byte_at("space_0", 64, 32, 0x1000);
    let mut oracle_le = SnapshotOracle::new(Some(MemorySnapshotId::new(1))).with_bytes(
        0,
        0x1000,
        &[0x11, 0x22, 0x33, 0x44],
    );
    oracle_le.byte_order = MemoryByteOrder::LittleEndian;

    let mut oracle_be = oracle_le;
    oracle_be.byte_order = MemoryByteOrder::BigEndian;

    let oracle_le = SnapshotOracle::new(Some(MemorySnapshotId::new(1))).with_bytes(
        0,
        0x1000,
        &[0x11, 0x22, 0x33, 0x44],
    );

    let mut solver = Solver::new();
    assert_eq!(
        solver
            .check_sat_with_oracle(Some(&oracle_le), &[eq_const(sel.clone(), 0x4433_2211, 32)])
            .unwrap(),
        SatResult::Sat
    );
    assert_eq!(
        solver
            .check_sat_with_oracle(Some(&oracle_le), &[eq_const(sel.clone(), 0x1122_3344, 32)])
            .unwrap(),
        SatResult::Unsat
    );

    assert_eq!(
        solver
            .check_sat_with_oracle(Some(&oracle_be), &[eq_const(sel.clone(), 0x1122_3344, 32)])
            .unwrap(),
        SatResult::Sat
    );
    assert_eq!(
        solver
            .check_sat_with_oracle(Some(&oracle_be), &[eq_const(sel, 0x4433_2211, 32)])
            .unwrap(),
        SatResult::Unsat
    );
}

#[test]
fn address_boundary_reject_overflow_vs_wrap_at_index_width() {
    let sel_8bit_wrap = select_space_byte_at("space_0", 8, 16, 0xFF);
    let sel_64bit_wrap = select_space_byte_at("space_1", 64, 16, u64::MAX);

    let oracle_reject = SnapshotOracle::new(Some(MemorySnapshotId::new(1)))
        .with_byte(0, 0xFF, 0xAA)
        .with_byte(0, 0x00, 0xBB)
        .with_byte(1, u64::MAX, 0xAA)
        .with_byte(1, 0x00, 0xBB);

    let mut oracle_wrap_le = SnapshotOracle::new(Some(MemorySnapshotId::new(2)))
        .with_byte(0, 0xFF, 0xAA)
        .with_byte(0, 0x00, 0xBB)
        .with_byte(1, u64::MAX, 0xAA)
        .with_byte(1, 0x00, 0xBB);
    oracle_wrap_le.wrap_policy = AddressWrapPolicy::WrapAtIndexWidth;

    let mut oracle_wrap_be = SnapshotOracle::new(Some(MemorySnapshotId::new(3)))
        .with_byte(0, 0xFF, 0xAA)
        .with_byte(0, 0x00, 0xBB);
    oracle_wrap_be.wrap_policy = AddressWrapPolicy::WrapAtIndexWidth;
    oracle_wrap_be.byte_order = MemoryByteOrder::BigEndian;

    let mut solver = Solver::new();
    // 8-bit boundary overflow is rejected under RejectOverflow.
    assert_eq!(
        solver
            .check_sat_with_oracle(
                Some(&oracle_reject),
                &[eq_const(sel_8bit_wrap.clone(), 0xBBAA, 16)],
            )
            .unwrap(),
        SatResult::Unknown
    );
    assert_eq!(
        solver.last_unknown(),
        Some(&UnknownReason::OracleBoundaryOverflow {
            space_id: 0,
            address: 0xFF,
            bit_width: 16
        })
    );

    // WrapAtIndexWidth wraps 0xFF + 1 -> 0x00 in 8-bit space (LE=0xBBAA, BE=0xAABB).
    assert_eq!(
        solver
            .check_sat_with_oracle(
                Some(&oracle_wrap_le),
                &[eq_const(sel_8bit_wrap.clone(), 0xBBAA, 16)],
            )
            .unwrap(),
        SatResult::Sat
    );
    assert_eq!(
        solver
            .check_sat_with_oracle(
                Some(&oracle_wrap_be),
                &[eq_const(sel_8bit_wrap, 0xAABB, 16)],
            )
            .unwrap(),
        SatResult::Sat
    );

    // 64-bit u64::MAX + 1 wraps to 0x00 under WrapAtIndexWidth vs rejected under RejectOverflow.
    assert_eq!(
        solver
            .check_sat_with_oracle(
                Some(&oracle_reject),
                &[eq_const(sel_64bit_wrap.clone(), 0xBBAA, 16)],
            )
            .unwrap(),
        SatResult::Unknown
    );
    assert!(matches!(
        solver.last_unknown(),
        Some(UnknownReason::OracleBoundaryOverflow {
            address: u64::MAX,
            ..
        })
    ));
    assert_eq!(
        solver
            .check_sat_with_oracle(
                Some(&oracle_wrap_le),
                &[eq_const(sel_64bit_wrap, 0xBBAA, 16)]
            )
            .unwrap(),
        SatResult::Sat
    );
}
