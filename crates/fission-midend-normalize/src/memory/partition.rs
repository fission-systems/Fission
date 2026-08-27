use super::super::cleanup::expr_has_side_effects;
use crate::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryAccessKind {
    Load,
    Store,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemoryAccessClass {
    Stack,
    Aggregate,
    HeapLike,
    GlobalLike,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemoryEscapeClass {
    NonEscaping,
    AddressTaken,
    Escaped,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PartitionKey {
    pub base_object: String,
    pub offset_interval: (i64, i64),
    pub stride: Option<i64>,
    pub effect_class: MemoryAccessClass,
    pub escape_class: MemoryEscapeClass,
}

impl PartitionKey {
    pub fn is_promotable_stack_like(&self) -> bool {
        matches!(
            (self.effect_class, self.escape_class),
            (
                MemoryAccessClass::Stack | MemoryAccessClass::Aggregate,
                MemoryEscapeClass::NonEscaping
            )
        )
    }

    /// Heritage promotable across address-space classes Fission models as
    /// fixed non-escaping partitions (stack slots + aggregate locals).
    /// Strided / escaped / heap-like keys stay memory ops (Ghidra space filter).
    pub fn is_heritage_promotable(&self) -> bool {
        self.stride.is_none() && self.is_promotable_stack_like()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionedMemoryAccess {
    pub kind: MemoryAccessKind,
    pub ptr: PreHirExpr,
    pub base: PreHirExpr,
    pub base_repr: String,
    pub const_offset: i64,
    pub stride: Option<i64>,
    pub index: Option<PreHirExpr>,
    pub access_ty: NirType,
}

#[derive(Debug, Default, Clone)]
struct AddressParts {
    base: Option<PreHirExpr>,
    const_offset: i64,
    scaled_index: Option<(PreHirExpr, i64)>,
}

pub fn collect_partitioned_memory_accesses(stmts: &[PreHirStmt]) -> Vec<PartitionedMemoryAccess> {
    let mut accesses = Vec::new();
    collect_accesses_from_stmts(stmts, &mut accesses);
    accesses
}

pub fn partition_key_for_pointer_expr(
    ptr: &PreHirExpr,
    access_ty: &NirType,
) -> Option<PartitionKey> {
    let access = parse_partitioned_access(ptr, access_ty, MemoryAccessKind::Load)?;
    Some(access.partition_key())
}

pub(super) fn type_byte_size(ty: &NirType) -> Option<u32> {
    match ty {
        NirType::Bool => Some(1),
        NirType::Int { bits, .. } => Some(bits / 8),
        NirType::Ptr(_) => Some(8),
        NirType::Aggregate { size, .. } => Some(*size),
        NirType::Float { bits } => Some(bits / 8),
        NirType::Unknown => None,
    }
}

impl PartitionedMemoryAccess {
    pub fn partition_key(&self) -> PartitionKey {
        let width = i64::from(type_byte_size(&self.access_ty).unwrap_or(0));
        let mut effect_class = classify_base_object(&self.base);
        // `classify_base_object` classifies a `local_XX`/`stack_XX`-named
        // base as `Stack` purely from its *name prefix* -- it has no
        // access to the binding's declared type, so it can't tell apart
        // two shapes that produce the exact same `Deref(local_XX + i *
        // stride)` pattern: (a) `local_XX` genuinely *is* a fixed-size
        // local array's own storage, safely indexed at runtime, vs (b)
        // `local_XX` merely *holds* a pointer value that was spilled to a
        // stack slot with that name (e.g. a VLA's dynamically-computed
        // base address) -- dereferencing *through* a pointer value
        // accesses whatever it points to, an entirely different,
        // unbounded region that must never be folded into this
        // function's own stack frame for alias-analysis purposes.
        // Confirmed via a real fixture that this ambiguity is real, not
        // theoretical: `int arr[n]` with a genuinely dynamic `n` spills
        // its runtime-computed base pointer to a `local_XX`-named slot,
        // and `arr[i] = ...` (a `stride`-carrying access through it) was
        // silently misclassified as a safely-removable dead store to
        // "the stack slot `local_XX`" by `apply_dead_store_elimination`
        // -- dropping the whole assignment from the decompiled output.
        // Downgrading to `Unknown` whenever a runtime `stride` is present
        // is conservative -- it forfeits eliminating a genuinely-dead
        // write to a real fixed-size local array (not observed in
        // practice: when this pipeline recognizes a true fixed-size
        // array, the address computation bottoms out at a different,
        // non-`local_XX`-prefixed expression instead, so this rule
        // shouldn't cost real optimization opportunities) in exchange for
        // never risking silently dropping a real store.
        if self.stride.is_some() && matches!(effect_class, MemoryAccessClass::Stack) {
            effect_class = MemoryAccessClass::Unknown;
        }
        let escape_class = if matches!(effect_class, MemoryAccessClass::Unknown) {
            MemoryEscapeClass::Escaped
        } else {
            classify_escape(&self.base)
        };
        PartitionKey {
            base_object: self.base_repr.clone(),
            offset_interval: (self.const_offset, self.const_offset + width),
            stride: self.stride,
            effect_class,
            escape_class,
        }
    }
}

fn collect_accesses_from_stmts(stmts: &[PreHirStmt], accesses: &mut Vec<PartitionedMemoryAccess>) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign { lhs, rhs } => {
                if let PreHirLValue::Deref { ptr, ty } = lhs
                    && let Some(access) = parse_partitioned_access(ptr, ty, MemoryAccessKind::Store)
                {
                    accesses.push(access);
                }
                collect_accesses_from_expr(rhs, accesses);
            }
            PreHirStmt::VaStart { va_list, .. } => {
                collect_accesses_from_expr(va_list, accesses);
            }
            PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
                collect_accesses_from_expr(expr, accesses);
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => collect_accesses_from_stmts(body, accesses),
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                collect_accesses_from_expr(expr, accesses);
                for case in cases {
                    collect_accesses_from_stmts(&case.body, accesses);
                }
                collect_accesses_from_stmts(default, accesses);
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                collect_accesses_from_expr(cond, accesses);
                collect_accesses_from_stmts(then_body, accesses);
                collect_accesses_from_stmts(else_body, accesses);
            }
            PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Return(None)
            | PreHirStmt::Break
            | PreHirStmt::Continue => {}
        }
    }
}

fn collect_accesses_from_expr(expr: &PreHirExpr, accesses: &mut Vec<PartitionedMemoryAccess>) {
    match expr {
        PreHirExpr::Load { ptr, ty } => {
            if let Some(access) = parse_partitioned_access(ptr, ty, MemoryAccessKind::Load) {
                accesses.push(access);
            }
            collect_accesses_from_expr(ptr, accesses);
        }
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => collect_accesses_from_expr(expr, accesses),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            collect_accesses_from_expr(lhs, accesses);
            collect_accesses_from_expr(rhs, accesses);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                collect_accesses_from_expr(arg, accesses);
            }
        }
        PreHirExpr::PtrOffset { base, .. } => collect_accesses_from_expr(base, accesses),
        PreHirExpr::Index { base, index, .. } => {
            collect_accesses_from_expr(base, accesses);
            collect_accesses_from_expr(index, accesses);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_accesses_from_expr(cond, accesses);
            collect_accesses_from_expr(then_expr, accesses);
            collect_accesses_from_expr(else_expr, accesses);
        }
        PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(_, _) => {}
    }
}

fn parse_partitioned_access(
    ptr: &PreHirExpr,
    access_ty: &NirType,
    kind: MemoryAccessKind,
) -> Option<PartitionedMemoryAccess> {
    let access_size = i64::from(type_byte_size(access_ty)?);
    let mut parts = AddressParts::default();
    collect_address_parts(ptr, &mut parts, 1)?;
    let base = parts.base?;
    if expr_has_side_effects(&base) {
        return None;
    }
    let stride = parts.scaled_index.as_ref().map(|(_, stride)| *stride);
    let index = match parts.scaled_index {
        Some((index, raw_stride)) if raw_stride == access_size => Some(index),
        Some((index, raw_stride)) if raw_stride > access_size && raw_stride % access_size == 0 => {
            Some(index)
        }
        Some(_) => return None,
        None => None,
    };
    Some(PartitionedMemoryAccess {
        kind,
        ptr: ptr.clone(),
        base_repr: format_expr_key(&base),
        base,
        const_offset: parts.const_offset,
        stride,
        index,
        access_ty: access_ty.clone(),
    })
}

fn classify_base_object(base: &PreHirExpr) -> MemoryAccessClass {
    match base {
        PreHirExpr::Var(name) | PreHirExpr::AddressOfGlobal(name) => {
            if name.starts_with("stack_")
                || name.starts_with("local_")
                || name.starts_with("home_")
                || name.starts_with("arg_out_")
                || name.starts_with("ret_scaffold_")
            {
                MemoryAccessClass::Stack
            } else if name.starts_with("param_") {
                MemoryAccessClass::Unknown
            } else if name.starts_with("DAT_") {
                MemoryAccessClass::GlobalLike
            } else {
                MemoryAccessClass::Unknown
            }
        }
        PreHirExpr::PtrOffset { base, .. }
        | PreHirExpr::Cast { expr: base, .. }
        | PreHirExpr::FieldAccess { base, .. } => classify_base_object(base),
        _ => MemoryAccessClass::HeapLike,
    }
}

fn classify_escape(base: &PreHirExpr) -> MemoryEscapeClass {
    match classify_base_object(base) {
        MemoryAccessClass::Stack | MemoryAccessClass::Aggregate | MemoryAccessClass::GlobalLike => {
            if expr_has_side_effects(base) {
                MemoryEscapeClass::AddressTaken
            } else {
                MemoryEscapeClass::NonEscaping
            }
        }
        MemoryAccessClass::HeapLike | MemoryAccessClass::Unknown => MemoryEscapeClass::Escaped,
    }
}

fn collect_address_parts(expr: &PreHirExpr, parts: &mut AddressParts, sign: i64) -> Option<()> {
    match expr {
        PreHirExpr::Const(value, _) => {
            parts.const_offset += sign * *value;
            Some(())
        }
        PreHirExpr::Cast { expr, .. } => collect_address_parts(expr, parts, sign),
        PreHirExpr::PtrOffset { base, offset } => {
            parts.const_offset += sign * *offset;
            collect_address_parts(base, parts, sign)
        }
        PreHirExpr::FieldAccess { base, offset, .. } => {
            parts.const_offset += sign * i64::from(*offset);
            collect_address_parts(base, parts, sign)
        }
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => {
            collect_address_parts(lhs, parts, sign)?;
            collect_address_parts(rhs, parts, sign)
        }
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Sub,
            lhs,
            rhs,
            ..
        } => {
            collect_address_parts(lhs, parts, sign)?;
            collect_address_parts(rhs, parts, -sign)
        }
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Mul,
            lhs,
            rhs,
            ..
        } => {
            if let PreHirExpr::Const(value, _) = lhs.as_ref() {
                return add_scaled_index_expr(parts, rhs, sign * *value);
            }
            if let PreHirExpr::Const(value, _) = rhs.as_ref() {
                return add_scaled_index_expr(parts, lhs, sign * *value);
            }
            add_base_expr(parts, expr.clone(), sign)
        }
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Shl,
            lhs,
            rhs,
            ..
        } => {
            let PreHirExpr::Const(shift, _) = rhs.as_ref() else {
                return add_base_expr(parts, expr.clone(), sign);
            };
            if *shift < 0 || *shift > 30 {
                return add_base_expr(parts, expr.clone(), sign);
            }
            add_scaled_index_expr(parts, lhs, sign * (1_i64 << shift))
        }
        _ => add_base_expr(parts, expr.clone(), sign),
    }
}

fn add_scaled_index_expr(parts: &mut AddressParts, expr: &PreHirExpr, stride: i64) -> Option<()> {
    if let PreHirExpr::Const(value, _) = expr {
        parts.const_offset += stride * *value;
        return Some(());
    }
    if let Some((index, bias)) = extract_index_bias(expr) {
        parts.const_offset += stride * bias;
        return add_scaled_index(parts, index, stride);
    }
    add_scaled_index(parts, expr.clone(), stride)
}

fn extract_index_bias(expr: &PreHirExpr) -> Option<(PreHirExpr, i64)> {
    match expr {
        PreHirExpr::Cast { expr, .. } => extract_index_bias(expr),
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => {
            if let PreHirExpr::Const(value, _) = lhs.as_ref() {
                let (index, bias) = extract_index_bias(rhs)?;
                return Some((index, bias + *value));
            }
            if let PreHirExpr::Const(value, _) = rhs.as_ref() {
                let (index, bias) = extract_index_bias(lhs)?;
                return Some((index, bias + *value));
            }
            if !expr_has_side_effects(expr) {
                Some((expr.clone(), 0))
            } else {
                None
            }
        }
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Sub,
            lhs,
            rhs,
            ..
        } => {
            if let PreHirExpr::Const(value, _) = rhs.as_ref() {
                let (index, bias) = extract_index_bias(lhs)?;
                return Some((index, bias - *value));
            }
            if !expr_has_side_effects(expr) {
                Some((expr.clone(), 0))
            } else {
                None
            }
        }
        _ if !expr_has_side_effects(expr) => Some((expr.clone(), 0)),
        _ => None,
    }
}

fn add_base_expr(parts: &mut AddressParts, expr: PreHirExpr, sign: i64) -> Option<()> {
    if sign != 1 || matches!(expr, PreHirExpr::Const(_, _)) {
        return None;
    }
    match &parts.base {
        Some(existing) if existing != &expr => None,
        Some(_) => Some(()),
        None => {
            parts.base = Some(expr);
            Some(())
        }
    }
}

fn add_scaled_index(parts: &mut AddressParts, expr: PreHirExpr, stride: i64) -> Option<()> {
    if stride <= 0 || expr_has_side_effects(&expr) {
        return None;
    }
    match &parts.scaled_index {
        Some((existing, existing_stride)) if existing != &expr || *existing_stride != stride => {
            None
        }
        Some(_) => Some(()),
        None => {
            parts.scaled_index = Some((expr, stride));
            Some(())
        }
    }
}
