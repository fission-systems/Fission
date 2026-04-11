/// Switch-discriminant recovery from jump-table load patterns.
///
/// When a compiler lowers a `switch` statement to a jump table it typically
/// generates a sequence like:
///
/// ```text
/// // bounds check: if (sel > N) goto default;
/// // optional offset: temp = sel - min_val;
/// // jump: goto table[temp * 8];
/// ```
///
/// The P-code `BranchInd` receives the *loaded address* from the table, not
/// the original selector.  By pattern-matching the HIR of the `switch_expr`
/// (= the lowered load) we can recover:
/// - the actual discriminant expression (`sel` or `sel - min_val` adjusted)
/// - the `min_val` base offset for ordinal case values
///
/// This is entirely algorithmic: no binary-specific heuristics, no hard-coded
/// table sizes.  We accept only structurally clear patterns and fall back to
/// ordinal indices when the match fails.
///
/// Ghidra reference: `ActionNormalizeSetup` + jump-table analysis in
/// `jumptable.cc`.  Our approach is simpler (HIR-level only) but covers the
/// common compiler output.
use super::super::types::{DispatcherProofKind, HirBinaryOp, HirExpr, NirRenderOptions, NirType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RecoveredSwitchSelector {
    pub discriminant: HirExpr,
    pub min_val: i64,
    pub proof_kind: DispatcherProofKind,
}

/// Try to recover `(discriminant_expr, min_val)` from a jump-table load.
///
/// - `switch_expr` — the HIR expression lowered from the `BranchInd` input.
///   This is typically `Load { ptr: table_base + selector * scale }`.
/// - `options`    — render options, used to validate that `table_base` is a
///   mapped global (read-only section).
///
/// Returns `None` when the pattern does not match; the caller then keeps the
/// original `switch_expr` unchanged with `min_val = 0`.
pub(super) fn recover_switch_discriminant(
    switch_expr: &HirExpr,
    options: &NirRenderOptions,
) -> Option<RecoveredSwitchSelector> {
    // The switch expression must be a LOAD whose address is the jump-table entry.
    let HirExpr::Load { ptr: addr_expr, .. } = switch_expr else {
        return None;
    };

    // Extract (table_base_addr, selector_expr, _scale) from the address.
    let (table_base, selector_expr, scaled_by_mul) = extract_table_base_and_selector(addr_expr)?;

    // Validate: table_base must be a mapped section address (jump table lives
    // in .rdata / .text, not on the stack).
    if !options.is_mapped_global(table_base) {
        return None;
    }

    // Peel an outer zero-extension or narrowing cast from the selector — the
    // compiler often zero-extends the selector to pointer width.
    let selector_inner = peel_cast(selector_expr);

    // Detect `selector = orig - min_val` pattern.
    let (discriminant, min_val) =
        extract_min_val_sub(selector_inner).unwrap_or_else(|| (selector_inner.clone(), 0));

    Some(RecoveredSwitchSelector {
        discriminant,
        min_val,
        proof_kind: if scaled_by_mul {
            DispatcherProofKind::JumpTable
        } else {
            DispatcherProofKind::ConstantStrideIndex
        },
    })
}

/// Try to decompose `addr` into `(table_base_addr, selector_expr)`.
///
/// Accepted patterns (after normalization the Add operands may be in either
/// order, and the scale multiply may have been folded into a shift or left as
/// an explicit Mul):
///
/// ```text
/// Const(base) + selector * Const(scale)
/// Const(base) + selector << Const(log2_scale)
/// Const(base) + selector                        (scale == 1)
/// selector * Const(scale) + Const(base)         (commuted)
/// ```
fn extract_table_base_and_selector(addr: &HirExpr) -> Option<(u64, &HirExpr, bool)> {
    let HirExpr::Binary {
        op: HirBinaryOp::Add,
        lhs,
        rhs,
        ..
    } = addr
    else {
        return None;
    };

    // Try Const(base) + scaled_selector.
    if let HirExpr::Const(base, _) = lhs.as_ref() {
        let (selector, scaled_by_mul) = extract_unscaled_selector(rhs)?;
        return Some((*base as u64, selector, scaled_by_mul));
    }

    // Try scaled_selector + Const(base).
    if let HirExpr::Const(base, _) = rhs.as_ref() {
        let (selector, scaled_by_mul) = extract_unscaled_selector(lhs)?;
        return Some((*base as u64, selector, scaled_by_mul));
    }

    None
}

/// Strip the scale multiply/shift from a "scaled selector" expression,
/// returning the underlying selector.
///
/// Accepted inner patterns:
/// - `selector * Const(_scale)`
/// - `Const(_scale) * selector`
/// - `selector << Const(_log2)`
/// - `selector` (scale = 1; no extra operation)
fn extract_unscaled_selector(expr: &HirExpr) -> Option<(&HirExpr, bool)> {
    match expr {
        // selector * scale  or  scale * selector
        HirExpr::Binary {
            op: HirBinaryOp::Mul,
            lhs,
            rhs,
            ..
        } => {
            if matches!(rhs.as_ref(), HirExpr::Const(..)) {
                Some((lhs, true))
            } else if matches!(lhs.as_ref(), HirExpr::Const(..)) {
                Some((rhs, true))
            } else {
                None
            }
        }
        // selector << log2(scale)
        HirExpr::Binary {
            op: HirBinaryOp::Shl,
            lhs,
            rhs,
            ..
        } if matches!(rhs.as_ref(), HirExpr::Const(..)) => Some((lhs, false)),
        // scale == 1: selector directly (any non-constant expression)
        other if !matches!(other, HirExpr::Const(..)) => Some((other, false)),
        _ => None,
    }
}

pub(super) fn proves_single_target_dispatcher_surface(
    switch_expr: &HirExpr,
    targets: &[u64],
    current_block: u64,
    options: &NirRenderOptions,
) -> bool {
    if targets.len() != 1 || targets[0] != current_block {
        return false;
    }
    recover_switch_discriminant(switch_expr, options).is_some()
        || is_mapped_global_load_source(switch_expr, options)
}

fn is_mapped_global_load_source(expr: &HirExpr, options: &NirRenderOptions) -> bool {
    match expr {
        HirExpr::Load { ptr, .. } => extract_mapped_global_address(ptr, options).is_some(),
        HirExpr::Cast { expr: inner, .. } => is_mapped_global_load_source(inner, options),
        _ => false,
    }
}

fn extract_mapped_global_address(expr: &HirExpr, options: &NirRenderOptions) -> Option<u64> {
    match expr {
        HirExpr::Const(addr, _) if *addr >= 0 => {
            let addr = *addr as u64;
            options.is_mapped_global(addr).then_some(addr)
        }
        HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => extract_mapped_global_address(lhs, options)
            .or_else(|| extract_mapped_global_address(rhs, options)),
        HirExpr::Cast { expr: inner, .. } => extract_mapped_global_address(inner, options),
        _ => None,
    }
}

/// Detect `expr = orig - min_val` where `min_val` is a non-zero constant.
///
/// Returns `(orig_expr, min_val)` when matched, `None` otherwise.
fn extract_min_val_sub(expr: &HirExpr) -> Option<(HirExpr, i64)> {
    let HirExpr::Binary {
        op: HirBinaryOp::Sub,
        lhs,
        rhs,
        ..
    } = expr
    else {
        return None;
    };
    let HirExpr::Const(min_val, _) = rhs.as_ref() else {
        return None;
    };
    // min_val == 0 means no adjustment; treat as unrecovered.
    if *min_val == 0 {
        return None;
    }
    Some((*lhs.clone(), *min_val))
}

/// Peel one layer of zero-extension or truncation cast from `expr`.
///
/// The compiler commonly inserts a cast like `(ulong)selector` to widen the
/// selector to pointer width for the table address computation.  Removing it
/// gives us the original switch variable.
fn peel_cast(expr: &HirExpr) -> &HirExpr {
    match expr {
        HirExpr::Cast {
            ty: NirType::Int { .. } | NirType::Bool,
            expr: inner,
        } => inner.as_ref(),
        _ => expr,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nir::types::NirRenderOptions;

    fn options_with_section(start: u64, end: u64) -> NirRenderOptions {
        NirRenderOptions {
            pe_x64_only: false,
            is_64bit: true,
            pointer_size: 8,
            format: "PE64".to_owned(),
            image_base: 0x400000,
            sections: vec![(start, end)],
            region_linearize_structuring: false,
            force_linear_structuring: false,
            conservative_irreducible_fallback: false,
            global_names: Default::default(),
            calling_convention: Default::default(),
        }
    }

    fn uint64() -> NirType {
        NirType::Int {
            bits: 64,
            signed: false,
        }
    }
    fn uint32() -> NirType {
        NirType::Int {
            bits: 32,
            signed: false,
        }
    }

    fn load(ptr: HirExpr) -> HirExpr {
        HirExpr::Load {
            ptr: Box::new(ptr),
            ty: uint64(),
        }
    }

    fn add(lhs: HirExpr, rhs: HirExpr) -> HirExpr {
        HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            ty: uint64(),
        }
    }

    fn mul(lhs: HirExpr, rhs: HirExpr) -> HirExpr {
        HirExpr::Binary {
            op: HirBinaryOp::Mul,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            ty: uint64(),
        }
    }

    fn sub(lhs: HirExpr, rhs: HirExpr) -> HirExpr {
        HirExpr::Binary {
            op: HirBinaryOp::Sub,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            ty: uint32(),
        }
    }

    fn shl(lhs: HirExpr, rhs: HirExpr) -> HirExpr {
        HirExpr::Binary {
            op: HirBinaryOp::Shl,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            ty: uint64(),
        }
    }

    fn cst(v: i64) -> HirExpr {
        HirExpr::Const(v, uint64())
    }

    fn var(name: &str) -> HirExpr {
        HirExpr::Var(name.to_owned())
    }

    fn cast_u64(inner: HirExpr) -> HirExpr {
        HirExpr::Cast {
            ty: uint64(),
            expr: Box::new(inner),
        }
    }

    // switch_expr = Load(0x40b000 + sel * 8)
    // expected: discriminant = sel, min_val = 0
    #[test]
    fn recovers_basic_mul8_pattern() {
        let opts = options_with_section(0x40b000, 0x40c000);
        let sel = var("sel");
        let expr = load(add(cst(0x40b000), mul(sel.clone(), cst(8))));
        let result = recover_switch_discriminant(&expr, &opts);
        assert!(result.is_some());
        let recovered = result.unwrap();
        assert_eq!(recovered.discriminant, sel);
        assert_eq!(recovered.min_val, 0);
        assert_eq!(recovered.proof_kind, DispatcherProofKind::JumpTable);
    }

    // switch_expr = Load(0x40b000 + (sel - 5) * 8)
    // expected: discriminant = sel, min_val = 5
    #[test]
    fn recovers_min_val_sub_pattern() {
        let opts = options_with_section(0x40b000, 0x40c000);
        let orig = var("orig");
        let adj_sel = sub(orig.clone(), cst(5));
        let expr = load(add(cst(0x40b000), mul(adj_sel, cst(8))));
        let result = recover_switch_discriminant(&expr, &opts);
        assert!(result.is_some());
        let recovered = result.unwrap();
        assert_eq!(recovered.discriminant, orig);
        assert_eq!(recovered.min_val, 5);
        assert_eq!(recovered.proof_kind, DispatcherProofKind::JumpTable);
    }

    // switch_expr = Load(0x40b000 + (ulonglong)sel * 8)
    // expected: discriminant = sel (cast peeled), min_val = 0
    #[test]
    fn peels_cast_from_selector() {
        let opts = options_with_section(0x40b000, 0x40c000);
        let inner_sel = var("sel");
        let expr = load(add(cst(0x40b000), mul(cast_u64(inner_sel.clone()), cst(8))));
        let result = recover_switch_discriminant(&expr, &opts);
        assert!(result.is_some());
        let recovered = result.unwrap();
        assert_eq!(recovered.discriminant, inner_sel);
        assert_eq!(recovered.min_val, 0);
    }

    // switch_expr = Load(0x40b000 + sel << 3)
    // expected: discriminant = sel, min_val = 0
    #[test]
    fn recovers_shl3_pattern() {
        let opts = options_with_section(0x40b000, 0x40c000);
        let sel = var("sel");
        let expr = load(add(cst(0x40b000), shl(sel.clone(), cst(3))));
        let result = recover_switch_discriminant(&expr, &opts);
        assert!(result.is_some());
        let recovered = result.unwrap();
        assert_eq!(recovered.discriminant, sel);
        assert_eq!(recovered.min_val, 0);
        assert_eq!(
            recovered.proof_kind,
            DispatcherProofKind::ConstantStrideIndex
        );
    }

    // Base address NOT in a section → should return None
    #[test]
    fn rejects_unmapped_base() {
        let opts = options_with_section(0x40b000, 0x40c000);
        let sel = var("sel");
        // table_base = 0x200000 which is not in the section
        let expr = load(add(cst(0x200000_i64), mul(sel, cst(8))));
        let result = recover_switch_discriminant(&expr, &opts);
        assert!(result.is_none());
    }

    // Commuted: (sel * 8) + table_base
    #[test]
    fn recovers_commuted_add_pattern() {
        let opts = options_with_section(0x40b000, 0x40c000);
        let sel = var("sel");
        let expr = load(add(mul(sel.clone(), cst(8)), cst(0x40b000)));
        let result = recover_switch_discriminant(&expr, &opts);
        assert!(result.is_some());
        let recovered = result.unwrap();
        assert_eq!(recovered.discriminant, sel);
        assert_eq!(recovered.min_val, 0);
        assert_eq!(recovered.proof_kind, DispatcherProofKind::JumpTable);
    }

    // Not a Load expression → should return None
    #[test]
    fn rejects_non_load_expr() {
        let opts = options_with_section(0x40b000, 0x40c000);
        let expr = var("x");
        assert!(recover_switch_discriminant(&expr, &opts).is_none());
    }

    #[test]
    fn proves_single_target_self_loop_dispatcher_from_global_load() {
        let opts = options_with_section(0x40b000, 0x40c000);
        let expr = load(cst(0x40b120));
        assert!(proves_single_target_dispatcher_surface(
            &expr,
            &[0x5000],
            0x5000,
            &opts
        ));
        assert!(!proves_single_target_dispatcher_surface(
            &expr,
            &[0x5010],
            0x5000,
            &opts
        ));
    }
}
