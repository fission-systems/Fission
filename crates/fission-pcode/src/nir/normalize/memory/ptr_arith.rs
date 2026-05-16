/// Pointer arithmetic HIR recovery pass.
///
/// After use-driven type propagation (`use_type_infer.rs`) has marked pointer
/// variables, many remaining `IntAdd`/`IntSub` expressions of the form
///
/// ```text
/// IntAdd(Var(ptr), Const(k))
/// IntAdd(Var(ptr), Mul(idx, Const(stride)))
/// ```
///
/// still have purely-integer semantics in the HIR even though `ptr` is now
/// known to be a pointer.  This pass converts them to the higher-level
/// pointer-valued HIR expressions, or to `Index` when the arithmetic is the
/// address operand of a load/store. The printer renders these as:
///
/// ```c
/// ptr + k          (PtrOffset with stride 1)
/// ptr + idx        (pointer-valued scaled add)
/// ptr[idx]         (Index load/store)
/// ```
///
/// rather than the verbose `(uint8_t *)(ptr) + k` produced by the generic
/// integer-arithmetic printer.
///
/// ### Algorithm
///
/// For every expression tree:
///
/// 1. `Add(Var(x), Mul(idx, Const(stride)))` where `x.ty == Ptr(T)` and
///    `stride == sizeof(T)` (or T is Unknown/byte) → pointer-valued
///    `Add(Var(x), idx)`. The C printer's pointer arithmetic supplies the
///    element scaling.
///
/// 2. `Add(Var(x), Const(k))` where `x.ty == Ptr(_)` → `PtrOffset { base, offset: k }`
///
/// 3. `Load/Deref(Add(Var(x), Mul(idx, Const(stride))))` where the stride
///    matches the accessed type → `Index { base, index, elem_ty }`.
///
/// 4. `Cast { ty: Ptr(Int8), expr: Add(Var(x), ...) }` where `x` is already
///    known to be a pointer → strip the `Ptr(Int8)` cast (it was added as a
///    `uint8_t *` cast by the builder; now that the base is typed we don't need it).
///
/// 5. `Sub(Var(x), Const(k))` where `x.ty == Ptr(_)` → `PtrOffset { base, offset: -k }`
///
/// All transformations are conservative: we only act when the pointer type is
/// concretely known (`Ptr(_)`), never for `Unknown`.
///
/// Reference: Ghidra `RulePtrArith` (ruleaction.cc:6496),
///            RetDec `DerefToArrayIndexOptimizer`.
use super::super::*;
use std::collections::HashMap;

/// Build a map from variable name → NirType for all locals and params.
fn build_binding_type_map(func: &HirFunction) -> HashMap<String, NirType> {
    func.locals
        .iter()
        .chain(func.params.iter())
        .filter(|b| b.ty != NirType::Unknown)
        .map(|b| (b.name.clone(), b.ty.clone()))
        .collect()
}

/// Return the pointee type if `ty` is `Ptr(pointee)`.
fn pointee_ty(ty: &NirType) -> Option<&NirType> {
    match ty {
        NirType::Ptr(inner) => Some(inner),
        _ => None,
    }
}

/// Return the byte size of a type, if known.
fn type_byte_size(ty: &NirType) -> Option<u64> {
    match ty {
        NirType::Int { bits, .. } | NirType::Float { bits } => Some(u64::from(*bits / 8)),
        NirType::Bool => Some(1),
        NirType::Ptr(_) => Some(8), // assume 64-bit
        _ => None,
    }
}

/// Try to recognise `Mul(idx, Const(stride))` or `Mul(Const(stride), idx)`,
/// returning `(idx_expr, stride)`.
fn try_extract_index_mul(expr: &HirExpr) -> Option<(HirExpr, i64)> {
    let HirExpr::Binary {
        op: HirBinaryOp::Mul,
        lhs,
        rhs,
        ..
    } = expr
    else {
        return None;
    };
    match (lhs.as_ref(), rhs.as_ref()) {
        (_, HirExpr::Const(stride, _)) => Some((*lhs.clone(), *stride)),
        (HirExpr::Const(stride, _), _) => Some((*rhs.clone(), *stride)),
        _ => None,
    }
}

/// Return a typed pointer base expression.  The builder may preserve byte-level
/// address arithmetic as `Cast(Ptr(Int8), p)` even after use-driven inference
/// has proven that `p` is a more specific pointer type.  Keep track of that
/// byte cast so constant byte offsets can be rescaled to element offsets.
fn typed_pointer_base(
    expr: &HirExpr,
    binding_types: &HashMap<String, NirType>,
) -> Option<(HirExpr, NirType, bool)> {
    match expr {
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => {
            binding_types.get(name.as_str()).and_then(|ty| {
                if matches!(ty, NirType::Ptr(_)) {
                    Some((expr.clone(), ty.clone(), false))
                } else {
                    None
                }
            })
        }
        HirExpr::Cast {
            ty: NirType::Ptr(pointee),
            expr: inner,
        } if matches!(pointee.as_ref(), NirType::Int { bits: 8, .. }) => {
            let (base, ty, _) = typed_pointer_base(inner, binding_types)?;
            Some((base, ty, true))
        }
        _ => {
            let ty = expr_type(expr);
            if matches!(ty, NirType::Ptr(_)) {
                Some((expr.clone(), ty, false))
            } else {
                None
            }
        }
    }
}

fn pointer_const_expr(value: i64, sample: &HirExpr) -> HirExpr {
    match sample {
        HirExpr::Const(_, ty) => HirExpr::Const(value, ty.clone()),
        _ => HirExpr::Const(
            value,
            NirType::Int {
                bits: 64,
                signed: value < 0,
            },
        ),
    }
}

fn pointer_sized_uint_ty() -> NirType {
    NirType::Int {
        bits: 64,
        signed: false,
    }
}

fn cast_pointer_operand_to_uint(
    expr: &HirExpr,
    binding_types: &HashMap<String, NirType>,
) -> Option<HirExpr> {
    typed_pointer_base(expr, binding_types)?;
    Some(HirExpr::Cast {
        ty: pointer_sized_uint_ty(),
        expr: Box::new(expr.clone()),
    })
}

fn cast_pointer_operands_for_integer_arith(
    expr: &HirExpr,
    binding_types: &HashMap<String, NirType>,
) -> Option<HirExpr> {
    let HirExpr::Binary { op, lhs, rhs, ty } = expr else {
        return None;
    };
    if !matches!(ty, NirType::Int { .. }) || !matches!(op, HirBinaryOp::Add | HirBinaryOp::Sub) {
        return None;
    }
    let new_lhs = cast_pointer_operand_to_uint(lhs, binding_types);
    let new_rhs = cast_pointer_operand_to_uint(rhs, binding_types);
    if new_lhs.is_none() && new_rhs.is_none() {
        return None;
    }
    Some(HirExpr::Binary {
        op: *op,
        lhs: Box::new(new_lhs.unwrap_or_else(|| lhs.as_ref().clone())),
        rhs: Box::new(new_rhs.unwrap_or_else(|| rhs.as_ref().clone())),
        ty: ty.clone(),
    })
}

fn recover_const_offset_as_typed_pointer_add(
    ptr_expr: &HirExpr,
    ptr_ty: &NirType,
    elem_ty: &NirType,
    offset: i64,
    rhs_expr: &HirExpr,
) -> Option<HirExpr> {
    if matches!(elem_ty, NirType::Unknown | NirType::Aggregate { .. }) {
        return None;
    }
    let elem_size = type_byte_size(elem_ty)?;
    if elem_size == 0 {
        return None;
    }
    let elem_size = i64::try_from(elem_size).ok()?;
    if offset % elem_size != 0 {
        return None;
    }
    let elem_offset = offset / elem_size;
    if elem_offset == 0 {
        return Some(ptr_expr.clone());
    }
    let op = if elem_offset > 0 {
        HirBinaryOp::Add
    } else {
        HirBinaryOp::Sub
    };
    Some(HirExpr::Binary {
        op,
        lhs: Box::new(ptr_expr.clone()),
        rhs: Box::new(pointer_const_expr(elem_offset.abs(), rhs_expr)),
        ty: ptr_ty.clone(),
    })
}

/// Attempt to convert a pointer-arithmetic expression to a PtrOffset or Index
/// node.  Returns `Some(new_expr)` on success; `None` means leave unchanged.
fn try_recover_ptr_arith(
    expr: &HirExpr,
    binding_types: &HashMap<String, NirType>,
) -> Option<HirExpr> {
    let HirExpr::Binary { op, lhs, rhs, ty } = expr else {
        return None;
    };

    let (ptr_expr, rhs_expr, neg) = match op {
        HirBinaryOp::Add => (lhs.as_ref(), rhs.as_ref(), false),
        HirBinaryOp::Sub => (lhs.as_ref(), rhs.as_ref(), true),
        _ => return None,
    };

    let (typed_ptr_expr, ptr_ty, from_byte_cast) = typed_pointer_base(ptr_expr, binding_types)?;
    let elem_ty = pointee_ty(&ptr_ty).cloned().unwrap_or(NirType::Unknown);
    let pointer_typed_const_byte_offset = matches!(ty, NirType::Ptr(_))
        && !from_byte_cast
        && matches!(elem_ty, NirType::Unknown | NirType::Aggregate { .. })
        && matches!(rhs_expr, HirExpr::Const(_, _));
    if matches!(ty, NirType::Ptr(_)) && !from_byte_cast && !pointer_typed_const_byte_offset {
        return None;
    }

    // Pattern 1: Add(ptr, Mul(idx, Const(stride))) → pointer add when stride matches.
    if !neg {
        if let Some((idx_expr, stride)) = try_extract_index_mul(rhs_expr) {
            let stride_matches = match type_byte_size(&elem_ty) {
                Some(sz) => stride as u64 == sz,
                None => stride == 1, // unknown elem_ty → only allow stride-1
            };
            if stride_matches && stride > 0 {
                return Some(HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(typed_ptr_expr.clone()),
                    rhs: Box::new(idx_expr),
                    ty: ptr_ty.clone(),
                });
            }
            // stride == 1 with byte pointer is also a valid index
            if stride == 1 && matches!(elem_ty, NirType::Int { bits: 8, .. } | NirType::Unknown) {
                return Some(HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(typed_ptr_expr.clone()),
                    rhs: Box::new(idx_expr),
                    ty: ptr_ty.clone(),
                });
            }
        }
    }

    // Pattern 2: Add(ptr, Const(k)) / Sub(ptr, Const(k)) → PtrOffset.
    if let HirExpr::Const(k, _) = rhs_expr {
        let offset = if neg { -k } else { *k };
        if let Some(recovered) = recover_const_offset_as_typed_pointer_add(
            &typed_ptr_expr,
            &ptr_ty,
            &elem_ty,
            offset,
            rhs_expr,
        ) {
            return Some(recovered);
        }
        if from_byte_cast && offset == 0 {
            return Some(typed_ptr_expr.clone());
        }
        // Only emit PtrOffset when offset != 0 (offset 0 is a no-op).
        if offset != 0 {
            return Some(HirExpr::PtrOffset {
                base: Box::new(typed_ptr_expr.clone()),
                offset,
            });
        }
        // offset == 0: the expression equals ptr — return bare Var.
        if offset == 0 {
            return Some(typed_ptr_expr.clone());
        }
    }

    // Pattern 3: Add(ptr, non-const-index) → pointer add with stride 1 for byte pointers.
    if !neg && matches!(elem_ty, NirType::Int { bits: 8, .. } | NirType::Unknown) {
        return Some(HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs: Box::new(typed_ptr_expr.clone()),
            rhs: Box::new(rhs_expr.clone()),
            ty: ptr_ty.clone(),
        });
    }

    None
}

fn try_recover_index_access(
    ptr: &HirExpr,
    access_ty: &NirType,
    binding_types: &HashMap<String, NirType>,
) -> Option<HirExpr> {
    let HirExpr::Binary {
        op: HirBinaryOp::Add,
        lhs,
        rhs,
        ..
    } = ptr
    else {
        return None;
    };
    let ptr_ty = match lhs.as_ref() {
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => {
            binding_types.get(name.as_str()).and_then(|t| {
                if matches!(t, NirType::Ptr(_)) {
                    Some(t)
                } else {
                    None
                }
            })?
        }
        _ => return None,
    };
    let elem_ty = pointee_ty(ptr_ty)
        .cloned()
        .unwrap_or_else(|| access_ty.clone());
    let (idx_expr, stride) = try_extract_index_mul(rhs.as_ref())?;
    let access_size = type_byte_size(access_ty).or_else(|| type_byte_size(&elem_ty))?;
    if stride > 0 && stride as u64 == access_size {
        Some(HirExpr::Index {
            base: lhs.clone(),
            index: Box::new(idx_expr),
            elem_ty: access_ty.clone(),
        })
    } else {
        None
    }
}

/// Recursively rewrite all pointer-arithmetic sub-expressions in `expr`.
fn recover_in_expr(expr: &mut HirExpr, binding_types: &HashMap<String, NirType>) -> bool {
    // Try the top-level pattern first.
    if let Some(new_expr) = try_recover_ptr_arith(expr, binding_types) {
        *expr = new_expr;
        return true;
    }
    if let Some(new_expr) = cast_pointer_operands_for_integer_arith(expr, binding_types) {
        *expr = new_expr;
        return true;
    }
    // Recurse into children.
    let mut changed = false;
    match expr {
        HirExpr::Binary { lhs, rhs, .. } => {
            changed |= recover_in_expr(lhs, binding_types);
            changed |= recover_in_expr(rhs, binding_types);
        }
        HirExpr::Unary { expr: inner, .. } => {
            changed |= recover_in_expr(inner, binding_types);
        }
        HirExpr::Cast { expr: inner, .. } => {
            changed |= recover_in_expr(inner, binding_types);
            // After recursing: if we now have Cast(Ptr(Int8), PtrOffset { base, .. })
            // and base is already pointer-typed, strip the cast.
            if let HirExpr::Cast {
                ty: NirType::Ptr(pointee),
                expr: inner2,
            } = expr
            {
                if matches!(pointee.as_ref(), NirType::Int { bits: 8, .. }) {
                    if matches!(
                        inner2.as_ref(),
                        HirExpr::PtrOffset { .. } | HirExpr::Index { .. }
                    ) {
                        let new_inner = *inner2.clone();
                        *expr = new_inner;
                        changed = true;
                    }
                }
            }
        }
        HirExpr::Load { ptr, ty } => {
            if let Some(index_expr) = try_recover_index_access(ptr, ty, binding_types) {
                *expr = index_expr;
                return true;
            }
            changed |= recover_in_expr(ptr, binding_types);
        }
        HirExpr::Call { args, .. } => {
            for arg in args.iter_mut() {
                changed |= recover_in_expr(arg, binding_types);
            }
        }
        HirExpr::PtrOffset { base, .. } => {
            changed |= recover_in_expr(base, binding_types);
        }
        HirExpr::Index {
            base,
            index,
            ..
        } => {
            changed |= recover_in_expr(base, binding_types);
            changed |= recover_in_expr(index, binding_types);
        }
        HirExpr::AggregateCopy { src, .. } => {
            changed |= recover_in_expr(src, binding_types);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= recover_in_expr(cond, binding_types);
            changed |= recover_in_expr(then_expr, binding_types);
            changed |= recover_in_expr(else_expr, binding_types);
        }
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
    }
    changed
}

fn recover_in_lvalue(lhs: &mut HirLValue, binding_types: &HashMap<String, NirType>) -> bool {
    match lhs {
        HirLValue::Deref { ptr, ty } => {
            if let Some(HirExpr::Index {
                base,
                index,
                elem_ty,
            }) = try_recover_index_access(ptr, ty, binding_types)
            {
                *lhs = HirLValue::Index {
                    base,
                    index,
                    elem_ty,
                };
                true
            } else {
                recover_in_expr(ptr, binding_types)
            }
        }
        HirLValue::Index {
            base,
            index,
            ..
        } => {
            let a = recover_in_expr(base, binding_types);
            let b = recover_in_expr(index, binding_types);
            a || b
        }
        HirLValue::Var(_) => false,
    }
}

fn is_zero_index(index: &HirExpr) -> bool {
    matches!(index, HirExpr::Const(0, _))
}

fn collect_pointer_assignment_types(stmts: &[HirStmt], out: &mut HashMap<String, Option<NirType>>) {
    for stmt in stmts {
        collect_pointer_assignment_types_stmt(stmt, out);
    }
}

fn record_pointer_assignment(out: &mut HashMap<String, Option<NirType>>, name: &str, ty: NirType) {
    if !matches!(ty, NirType::Ptr(_)) {
        if let Some(slot) = out.get_mut(name) {
            *slot = None;
        }
        return;
    }
    match out.get_mut(name) {
        Some(slot) => {
            if slot.as_ref().is_some_and(|existing| existing != &ty) {
                *slot = None;
            }
        }
        None => {
            out.insert(name.to_string(), Some(ty));
        }
    }
}

fn collect_pointer_assignment_types_stmt(
    stmt: &HirStmt,
    out: &mut HashMap<String, Option<NirType>>,
) {
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs,
        } => record_pointer_assignment(out, name, expr_type(rhs)),
        HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            collect_pointer_assignment_types(body, out);
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_pointer_assignment_types(then_body, out);
            collect_pointer_assignment_types(else_body, out);
        }
        HirStmt::For {
            init, update, body, ..
        } => {
            if let Some(init) = init {
                collect_pointer_assignment_types_stmt(init, out);
            }
            if let Some(update) = update {
                collect_pointer_assignment_types_stmt(update, out);
            }
            collect_pointer_assignment_types(body, out);
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                collect_pointer_assignment_types(&case.body, out);
            }
            collect_pointer_assignment_types(default, out);
        }
        _ => {}
    }
}

fn propagate_pointer_assignment_types(func: &mut HirFunction) -> bool {
    let mut inferred = HashMap::new();
    collect_pointer_assignment_types(&func.body, &mut inferred);
    if inferred.is_empty() {
        return false;
    }
    let mut changed = false;
    for binding in func.locals.iter_mut().chain(func.params.iter_mut()) {
        let Some(Some(ty)) = inferred.get(binding.name.as_str()) else {
            continue;
        };
        if !matches!(binding.ty, NirType::Ptr(_)) {
            binding.ty = ty.clone();
            changed = true;
        }
    }
    changed
}

fn recover_in_stmts(stmts: &mut Vec<HirStmt>, binding_types: &HashMap<String, NirType>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        changed |= recover_in_stmt(stmt, binding_types);
    }
    changed
}

fn recover_in_stmt(stmt: &mut HirStmt, binding_types: &HashMap<String, NirType>) -> bool {
    let mut changed = false;
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            changed |= recover_in_lvalue(lhs, binding_types);
            changed |= recover_in_expr(rhs, binding_types);
        }
        HirStmt::Expr(expr) => {
            changed |= recover_in_expr(expr, binding_types);
        }
        HirStmt::Block(body) => {
            changed |= recover_in_stmts(body, binding_types);
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            changed |= recover_in_expr(cond, binding_types);
            changed |= recover_in_stmts(then_body, binding_types);
            changed |= recover_in_stmts(else_body, binding_types);
        }
        HirStmt::While { cond, body } => {
            changed |= recover_in_expr(cond, binding_types);
            changed |= recover_in_stmts(body, binding_types);
        }
        HirStmt::DoWhile { body, cond } => {
            changed |= recover_in_stmts(body, binding_types);
            changed |= recover_in_expr(cond, binding_types);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                changed |= recover_in_stmt(i, binding_types);
            }
            if let Some(c) = cond {
                changed |= recover_in_expr(c, binding_types);
            }
            if let Some(u) = update {
                changed |= recover_in_stmt(u, binding_types);
            }
            changed |= recover_in_stmts(body, binding_types);
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            changed |= recover_in_expr(expr, binding_types);
            for case in cases.iter_mut() {
                changed |= recover_in_stmts(&mut case.body, binding_types);
            }
            changed |= recover_in_stmts(default, binding_types);
        }
        HirStmt::Return(Some(expr)) => {
            changed |= recover_in_expr(expr, binding_types);
        }
        _ => {}
    }
    changed
}

/// Apply the pointer arithmetic recovery pass to a function.
///
/// Returns `true` if any expression was rewritten.
pub(crate) fn apply_ptr_arith_recovery_pass(func: &mut HirFunction) -> bool {
    let mut changed = false;
    for _ in 0..4 {
        let propagated = propagate_pointer_assignment_types(func);
        changed |= propagated;
        let binding_types = build_binding_type_map(func);
        if binding_types.is_empty() {
            break;
        }
        let recovered = recover_in_stmts(&mut func.body, &binding_types);
        changed |= recovered;
        if !propagated && !recovered {
            break;
        }
    }
    changed
}

pub(crate) fn apply_zero_index_deref_pass(func: &mut HirFunction) -> bool {
    let mut changed = false;
    for stmt in &mut func.body {
        changed |= normalize_zero_index_stmt(stmt);
    }
    changed
}

fn normalize_zero_index_stmt(stmt: &mut HirStmt) -> bool {
    let mut changed = false;
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            changed |= normalize_zero_index_lvalue(lhs);
            changed |= normalize_zero_index_expr(rhs);
        }
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
            changed |= normalize_zero_index_expr(expr);
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            changed |= normalize_zero_index_expr(cond);
            for stmt in then_body {
                changed |= normalize_zero_index_stmt(stmt);
            }
            for stmt in else_body {
                changed |= normalize_zero_index_stmt(stmt);
            }
        }
        HirStmt::While { cond, body } => {
            changed |= normalize_zero_index_expr(cond);
            for stmt in body {
                changed |= normalize_zero_index_stmt(stmt);
            }
        }
        HirStmt::DoWhile { body, cond } => {
            for stmt in body {
                changed |= normalize_zero_index_stmt(stmt);
            }
            changed |= normalize_zero_index_expr(cond);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(init) = init {
                changed |= normalize_zero_index_stmt(init);
            }
            if let Some(cond) = cond {
                changed |= normalize_zero_index_expr(cond);
            }
            if let Some(update) = update {
                changed |= normalize_zero_index_stmt(update);
            }
            for stmt in body {
                changed |= normalize_zero_index_stmt(stmt);
            }
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            changed |= normalize_zero_index_expr(expr);
            for case in cases {
                for stmt in &mut case.body {
                    changed |= normalize_zero_index_stmt(stmt);
                }
            }
            for stmt in default {
                changed |= normalize_zero_index_stmt(stmt);
            }
        }
        HirStmt::Block(body) => {
            for stmt in body {
                changed |= normalize_zero_index_stmt(stmt);
            }
        }
        HirStmt::VaStart { va_list, .. } => {
            changed |= normalize_zero_index_expr(va_list);
        }
        HirStmt::Return(None)
        | HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Break
        | HirStmt::Continue => {}
    }
    changed
}

fn normalize_zero_index_lvalue(lhs: &mut HirLValue) -> bool {
    match lhs {
        HirLValue::Deref { ptr, .. } => normalize_zero_index_expr(ptr),
        HirLValue::Index {
            base,
            index,
            elem_ty,
        } => {
            let mut changed = normalize_zero_index_expr(base) | normalize_zero_index_expr(index);
            if is_zero_index(index) {
                let ptr = std::mem::replace(base, Box::new(HirExpr::Const(0, NirType::Unknown)));
                let ty = elem_ty.clone();
                *lhs = HirLValue::Deref { ptr, ty };
                changed = true;
            }
            changed
        }
        HirLValue::Var(_) => false,
    }
}

fn normalize_zero_index_expr(expr: &mut HirExpr) -> bool {
    let mut changed = false;
    match expr {
        HirExpr::Binary { lhs, rhs, .. } => {
            changed |= normalize_zero_index_expr(lhs);
            changed |= normalize_zero_index_expr(rhs);
        }
        HirExpr::Unary { expr, .. }
        | HirExpr::Cast { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => {
            changed |= normalize_zero_index_expr(expr);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                changed |= normalize_zero_index_expr(arg);
            }
        }
        HirExpr::Index {
            base,
            index,
            elem_ty,
        } => {
            changed |= normalize_zero_index_expr(base);
            changed |= normalize_zero_index_expr(index);
            if is_zero_index(index) {
                let ptr = std::mem::replace(base, Box::new(HirExpr::Const(0, NirType::Unknown)));
                let ty = elem_ty.clone();
                *expr = HirExpr::Load { ptr, ty };
                changed = true;
            }
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= normalize_zero_index_expr(cond);
            changed |= normalize_zero_index_expr(then_expr);
            changed |= normalize_zero_index_expr(else_expr);
        }
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::super::super::*;

    fn make_binding_with_ty(name: &str, ty: NirType) -> NirBinding {
        NirBinding {
            name: name.to_owned(),
            ty,
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        }
    }

    fn make_func(locals: Vec<NirBinding>, body: Vec<HirStmt>) -> HirFunction {
        HirFunction {
            name: "test".to_owned(),
            params: vec![],
            locals,
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body,
            ..Default::default()
        }
    }

    #[test]
    fn rewrites_zero_index_access_to_deref() {
        let elem_ty = NirType::Int {
            bits: 32,
            signed: true,
        };
        let ptr_ty = NirType::Ptr(Box::new(elem_ty.clone()));
        let body = vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("value".to_owned()),
                rhs: HirExpr::Index {
                    base: Box::new(HirExpr::Var("p".to_owned())),
                    index: Box::new(HirExpr::Const(
                        0,
                        NirType::Int {
                            bits: 64,
                            signed: false,
                        },
                    )),
                    elem_ty: elem_ty.clone(),
                },
            },
            HirStmt::Assign {
                lhs: HirLValue::Index {
                    base: Box::new(HirExpr::Var("p".to_owned())),
                    index: Box::new(HirExpr::Const(
                        0,
                        NirType::Int {
                            bits: 64,
                            signed: false,
                        },
                    )),
                    elem_ty,
                },
                rhs: HirExpr::Var("value".to_owned()),
            },
        ];
        let mut func = make_func(vec![make_binding_with_ty("p", ptr_ty)], body);

        assert!(super::apply_zero_index_deref_pass(&mut func));
        assert!(matches!(
            &func.body[0],
            HirStmt::Assign {
                rhs: HirExpr::Load { .. },
                ..
            }
        ));
        assert!(matches!(
            &func.body[1],
            HirStmt::Assign {
                lhs: HirLValue::Deref { .. },
                ..
            }
        ));
    }

    /// Add(Var("p"), Const(8)) where p: Ptr(Aggregate) preserves byte-offset form
    /// for later field recovery.
    #[test]
    fn preserves_aggregate_add_const_as_ptr_offset() {
        let elem_ty = NirType::Aggregate {
            size: 16,
            fields: vec![],
        };
        let p_ty = NirType::Ptr(Box::new(elem_ty.clone()));
        let body = vec![HirStmt::Assign {
            lhs: HirLValue::Var("result".to_owned()),
            rhs: HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(HirExpr::Var("p".to_owned())),
                rhs: Box::new(HirExpr::Const(
                    8,
                    NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                )),
                ty: NirType::Int {
                    bits: 64,
                    signed: false,
                },
            },
        }];
        let mut func = make_func(vec![make_binding_with_ty("p", p_ty)], body);
        let changed = super::apply_ptr_arith_recovery_pass(&mut func);
        assert!(changed);
        if let HirStmt::Assign { rhs, .. } = &func.body[0] {
            assert!(matches!(rhs, HirExpr::PtrOffset { offset: 8, .. }));
        } else {
            panic!("expected assign");
        }
    }

    /// Pointer-typed Add(Var("p"), Const(8)) where p: Ptr(Aggregate) still
    /// preserves byte-offset form.  The raw p-code builder may type the address
    /// expression as a pointer before aggregate fields have been recovered.
    #[test]
    fn preserves_pointer_typed_aggregate_add_const_as_ptr_offset() {
        let elem_ty = NirType::Aggregate {
            size: 16,
            fields: vec![],
        };
        let p_ty = NirType::Ptr(Box::new(elem_ty.clone()));
        let body = vec![HirStmt::Assign {
            lhs: HirLValue::Var("result".to_owned()),
            rhs: HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(HirExpr::Var("p".to_owned())),
                rhs: Box::new(HirExpr::Const(
                    8,
                    NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                )),
                ty: NirType::Ptr(Box::new(elem_ty)),
            },
        }];
        let mut func = make_func(vec![make_binding_with_ty("p", p_ty)], body);
        let changed = super::apply_ptr_arith_recovery_pass(&mut func);
        assert!(changed);
        if let HirStmt::Assign { rhs, .. } = &func.body[0] {
            assert!(matches!(rhs, HirExpr::PtrOffset { offset: 8, .. }));
        } else {
            panic!("expected assign");
        }
    }

    /// Add(Var("p"), Const(4)) where p: Ptr(uint32) → Add(Var("p"), Const(1)).
    #[test]
    fn rescales_scalar_add_const_to_typed_pointer_add() {
        let elem_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let p_ty = NirType::Ptr(Box::new(elem_ty));
        let body = vec![HirStmt::Assign {
            lhs: HirLValue::Var("result".to_owned()),
            rhs: HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(HirExpr::Var("p".to_owned())),
                rhs: Box::new(HirExpr::Const(
                    4,
                    NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                )),
                ty: NirType::Int {
                    bits: 64,
                    signed: false,
                },
            },
        }];
        let mut func = make_func(vec![make_binding_with_ty("p", p_ty)], body);
        let changed = super::apply_ptr_arith_recovery_pass(&mut func);
        assert!(changed);
        if let HirStmt::Assign { rhs, .. } = &func.body[0] {
            assert!(matches!(
                rhs,
                HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs,
                    rhs,
                    ty: NirType::Ptr(_),
                } if matches!(lhs.as_ref(), HirExpr::Var(name) if name == "p")
                    && matches!(rhs.as_ref(), HirExpr::Const(1, _))
            ));
        } else {
            panic!("expected assign");
        }
    }

    /// Pointer-valued Add(Var("p"), Mul(Var("i"), Const(4))) where p: Ptr(uint32)
    /// keeps address semantics and becomes Add(Var("p"), Var("i")).
    #[test]
    fn converts_add_stride_to_pointer_add() {
        let elem_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let p_ty = NirType::Ptr(Box::new(elem_ty.clone()));
        let body = vec![HirStmt::Assign {
            lhs: HirLValue::Var("result".to_owned()),
            rhs: HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(HirExpr::Var("p".to_owned())),
                rhs: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::Mul,
                    lhs: Box::new(HirExpr::Var("i".to_owned())),
                    rhs: Box::new(HirExpr::Const(
                        4,
                        NirType::Int {
                            bits: 64,
                            signed: false,
                        },
                    )),
                    ty: NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                }),
                ty: NirType::Int {
                    bits: 64,
                    signed: false,
                },
            },
        }];
        let mut func = make_func(vec![make_binding_with_ty("p", p_ty)], body);
        let changed = super::apply_ptr_arith_recovery_pass(&mut func);
        assert!(changed);
        if let HirStmt::Assign { rhs, .. } = &func.body[0] {
            assert!(matches!(
                rhs,
                HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs,
                    rhs,
                    ty: NirType::Ptr(_),
                } if matches!(lhs.as_ref(), HirExpr::Var(name) if name == "p")
                    && matches!(rhs.as_ref(), HirExpr::Var(name) if name == "i")
            ));
        } else {
            panic!("expected assign");
        }
    }

    #[test]
    fn rescales_byte_cast_const_offset_to_typed_pointer_add() {
        let elem_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let p_ty = NirType::Ptr(Box::new(elem_ty));
        let byte_ptr_ty = NirType::Ptr(Box::new(NirType::Int {
            bits: 8,
            signed: false,
        }));
        let body = vec![HirStmt::Assign {
            lhs: HirLValue::Var("result".to_owned()),
            rhs: HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs: Box::new(HirExpr::Cast {
                    ty: byte_ptr_ty,
                    expr: Box::new(HirExpr::Var("p".to_owned())),
                }),
                rhs: Box::new(HirExpr::Const(
                    4,
                    NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                )),
                ty: NirType::Ptr(Box::new(NirType::Int {
                    bits: 8,
                    signed: false,
                })),
            },
        }];
        let mut func = make_func(vec![make_binding_with_ty("p", p_ty)], body);
        let changed = super::apply_ptr_arith_recovery_pass(&mut func);
        assert!(changed);
        if let HirStmt::Assign { rhs, .. } = &func.body[0] {
            assert!(matches!(
                rhs,
                HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs,
                    rhs,
                    ty: NirType::Ptr(_),
                } if matches!(lhs.as_ref(), HirExpr::Var(name) if name == "p")
                    && matches!(rhs.as_ref(), HirExpr::Const(1, _))
            ));
        } else {
            panic!("expected assign");
        }
    }

    /// Load(Add(Var("p"), Mul(Var("i"), Const(4)))) where p: Ptr(uint32) → p[i].
    #[test]
    fn converts_load_stride_to_index_value() {
        let elem_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let p_ty = NirType::Ptr(Box::new(elem_ty.clone()));
        let body = vec![HirStmt::Assign {
            lhs: HirLValue::Var("result".to_owned()),
            rhs: HirExpr::Load {
                ptr: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(HirExpr::Var("p".to_owned())),
                    rhs: Box::new(HirExpr::Binary {
                        op: HirBinaryOp::Mul,
                        lhs: Box::new(HirExpr::Var("i".to_owned())),
                        rhs: Box::new(HirExpr::Const(
                            4,
                            NirType::Int {
                                bits: 64,
                                signed: false,
                            },
                        )),
                        ty: NirType::Int {
                            bits: 64,
                            signed: false,
                        },
                    }),
                    ty: NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                }),
                ty: elem_ty.clone(),
            },
        }];
        let mut func = make_func(vec![make_binding_with_ty("p", p_ty)], body);
        let changed = super::apply_ptr_arith_recovery_pass(&mut func);
        assert!(changed);
        if let HirStmt::Assign { rhs, .. } = &func.body[0] {
            assert!(matches!(rhs, HirExpr::Index { .. }));
        } else {
            panic!("expected assign");
        }
    }

    #[test]
    fn casts_pointer_rhs_in_integer_subtraction() {
        let elem_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let p_ty = NirType::Ptr(Box::new(elem_ty));
        let body = vec![HirStmt::Assign {
            lhs: HirLValue::Var("diff".to_owned()),
            rhs: HirExpr::Binary {
                op: HirBinaryOp::Sub,
                lhs: Box::new(HirExpr::Var("addr".to_owned())),
                rhs: Box::new(HirExpr::Var("p".to_owned())),
                ty: NirType::Int {
                    bits: 64,
                    signed: false,
                },
            },
        }];
        let mut func = make_func(
            vec![
                make_binding_with_ty(
                    "addr",
                    NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                ),
                make_binding_with_ty("p", p_ty),
            ],
            body,
        );
        let changed = super::apply_ptr_arith_recovery_pass(&mut func);
        assert!(changed);
        if let HirStmt::Assign { rhs, .. } = &func.body[0] {
            assert!(matches!(
                rhs,
                HirExpr::Binary {
                    op: HirBinaryOp::Sub,
                    lhs,
                    rhs,
                    ty: NirType::Int { bits: 64, signed: false },
                } if matches!(lhs.as_ref(), HirExpr::Var(name) if name == "addr")
                    && matches!(rhs.as_ref(), HirExpr::Cast {
                        ty: NirType::Int { bits: 64, signed: false },
                        expr,
                    } if matches!(expr.as_ref(), HirExpr::Var(name) if name == "p"))
            ));
        } else {
            panic!("expected assign");
        }
    }

    #[test]
    fn casts_both_pointer_operands_in_integer_subtraction() {
        let elem_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let p_ty = NirType::Ptr(Box::new(elem_ty));
        let body = vec![HirStmt::Assign {
            lhs: HirLValue::Var("diff".to_owned()),
            rhs: HirExpr::Binary {
                op: HirBinaryOp::Sub,
                lhs: Box::new(HirExpr::Var("lhs".to_owned())),
                rhs: Box::new(HirExpr::Var("rhs".to_owned())),
                ty: NirType::Int {
                    bits: 64,
                    signed: false,
                },
            },
        }];
        let mut func = make_func(
            vec![
                make_binding_with_ty("lhs", p_ty.clone()),
                make_binding_with_ty("rhs", p_ty),
            ],
            body,
        );
        let changed = super::apply_ptr_arith_recovery_pass(&mut func);
        assert!(changed);
        if let HirStmt::Assign { rhs, .. } = &func.body[0] {
            assert!(matches!(
                rhs,
                HirExpr::Binary {
                    op: HirBinaryOp::Sub,
                    lhs,
                    rhs,
                    ty: NirType::Int { bits: 64, signed: false },
                } if matches!(lhs.as_ref(), HirExpr::Cast { expr, .. }
                        if matches!(expr.as_ref(), HirExpr::Var(name) if name == "lhs"))
                    && matches!(rhs.as_ref(), HirExpr::Cast { expr, .. }
                        if matches!(expr.as_ref(), HirExpr::Var(name) if name == "rhs"))
            ));
        } else {
            panic!("expected assign");
        }
    }
}
