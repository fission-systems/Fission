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
    pub pointer_bytes: u32,
}

#[derive(Debug, Default, Clone)]
struct AddressParts {
    base: Option<PreHirExpr>,
    const_offset: i64,
    scaled_index: Option<(PreHirExpr, i64)>,
}

pub fn collect_partitioned_memory_accesses(
    stmts: &[PreHirStmt],
    is_64bit: bool,
) -> Vec<PartitionedMemoryAccess> {
    collect_partitioned_accesses(stmts, false, if is_64bit { 8 } else { 4 })
}

/// Collect pointer-based accesses plus accesses already recovered as indexed
/// or field expressions. Memory-slot surfacing intentionally uses the
/// pointer-only collector because it only rewrites raw `Deref`/`Load` forms;
/// typed object facts need structured accesses to preserve byte offsets and
/// scalar access widths.
pub(super) fn collect_partitioned_memory_accesses_with_structured_accesses(
    stmts: &[PreHirStmt],
    is_64bit: bool,
) -> Vec<PartitionedMemoryAccess> {
    collect_partitioned_accesses(stmts, true, if is_64bit { 8 } else { 4 })
}

fn collect_partitioned_accesses(
    stmts: &[PreHirStmt],
    include_structured_accesses: bool,
    pointer_bytes: u32,
) -> Vec<PartitionedMemoryAccess> {
    let mut accesses = Vec::new();
    collect_accesses_from_stmts(
        stmts,
        &mut accesses,
        include_structured_accesses,
        pointer_bytes,
    );
    accesses
}

pub fn partition_key_for_pointer_expr(
    ptr: &PreHirExpr,
    access_ty: &NirType,
) -> Option<PartitionKey> {
    partition_key_for_pointer_expr_with_pointer_bytes(ptr, access_ty, 8)
}

pub fn partition_key_for_pointer_expr_with_pointer_bytes(
    ptr: &PreHirExpr,
    access_ty: &NirType,
    pointer_bytes: u32,
) -> Option<PartitionKey> {
    let access = parse_partitioned_access(ptr, access_ty, MemoryAccessKind::Load, pointer_bytes)?;
    Some(access.partition_key())
}

pub(super) fn type_byte_size(ty: &NirType) -> Option<u32> {
    type_byte_size_with_pointer_bytes(ty, 8)
}

pub(super) fn type_byte_size_with_pointer_bytes(ty: &NirType, pointer_bytes: u32) -> Option<u32> {
    match ty {
        NirType::Bool => Some(1),
        NirType::Int { bits, .. } => Some(bits / 8),
        NirType::Ptr(_) => Some(pointer_bytes),
        NirType::Aggregate { size, .. } => Some(*size),
        NirType::Float { bits } => Some(bits / 8),
        NirType::Unknown => None,
    }
}

impl PartitionedMemoryAccess {
    pub fn partition_key(&self) -> PartitionKey {
        let width = i64::from(
            type_byte_size_with_pointer_bytes(&self.access_ty, self.pointer_bytes).unwrap_or(0),
        );
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

fn collect_accesses_from_stmts(
    stmts: &[PreHirStmt],
    accesses: &mut Vec<PartitionedMemoryAccess>,
    include_structured_accesses: bool,
    pointer_bytes: u32,
) {
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign { lhs, rhs } => {
                match lhs {
                    PreHirLValue::Deref { ptr, ty } => {
                        if let Some(access) = parse_partitioned_access(
                            ptr,
                            ty,
                            MemoryAccessKind::Store,
                            pointer_bytes,
                        ) {
                            accesses.push(access);
                        }
                    }
                    PreHirLValue::FieldAccess {
                        base, offset, ty, ..
                    } if include_structured_accesses => {
                        if let Some(access) = parse_recovered_field_access(
                            base,
                            *offset,
                            ty,
                            MemoryAccessKind::Store,
                            pointer_bytes,
                        ) {
                            accesses.push(access);
                        }
                        collect_accesses_from_expr(
                            base,
                            accesses,
                            include_structured_accesses,
                            pointer_bytes,
                        );
                    }
                    PreHirLValue::Index {
                        base,
                        index,
                        elem_ty,
                    } if include_structured_accesses => {
                        if let Some(access) = parse_index_access(
                            base,
                            index,
                            elem_ty,
                            MemoryAccessKind::Store,
                            pointer_bytes,
                        ) {
                            accesses.push(access);
                        }
                        collect_accesses_from_expr(
                            base,
                            accesses,
                            include_structured_accesses,
                            pointer_bytes,
                        );
                        collect_accesses_from_expr(
                            index,
                            accesses,
                            include_structured_accesses,
                            pointer_bytes,
                        );
                    }
                    _ => {}
                }
                collect_accesses_from_expr(
                    rhs,
                    accesses,
                    include_structured_accesses,
                    pointer_bytes,
                );
            }
            PreHirStmt::VaStart { va_list, .. } => {
                collect_accesses_from_expr(
                    va_list,
                    accesses,
                    include_structured_accesses,
                    pointer_bytes,
                );
            }
            PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
                collect_accesses_from_expr(
                    expr,
                    accesses,
                    include_structured_accesses,
                    pointer_bytes,
                );
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => collect_accesses_from_stmts(
                body,
                accesses,
                include_structured_accesses,
                pointer_bytes,
            ),
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                collect_accesses_from_expr(
                    expr,
                    accesses,
                    include_structured_accesses,
                    pointer_bytes,
                );
                for case in cases {
                    collect_accesses_from_stmts(
                        &case.body,
                        accesses,
                        include_structured_accesses,
                        pointer_bytes,
                    );
                }
                collect_accesses_from_stmts(
                    default,
                    accesses,
                    include_structured_accesses,
                    pointer_bytes,
                );
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                collect_accesses_from_expr(
                    cond,
                    accesses,
                    include_structured_accesses,
                    pointer_bytes,
                );
                collect_accesses_from_stmts(
                    then_body,
                    accesses,
                    include_structured_accesses,
                    pointer_bytes,
                );
                collect_accesses_from_stmts(
                    else_body,
                    accesses,
                    include_structured_accesses,
                    pointer_bytes,
                );
            }
            PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Return(None)
            | PreHirStmt::Break
            | PreHirStmt::Continue => {}
        }
    }
}

fn collect_accesses_from_expr(
    expr: &PreHirExpr,
    accesses: &mut Vec<PartitionedMemoryAccess>,
    include_structured_accesses: bool,
    pointer_bytes: u32,
) {
    match expr {
        PreHirExpr::Load { ptr, ty } => {
            if let Some(access) =
                parse_partitioned_access(ptr, ty, MemoryAccessKind::Load, pointer_bytes)
            {
                accesses.push(access);
            }
            collect_accesses_from_expr(ptr, accesses, include_structured_accesses, pointer_bytes);
        }
        PreHirExpr::FieldAccess {
            base, offset, ty, ..
        } => {
            if include_structured_accesses
                && let Some(access) = parse_recovered_field_access(
                    base,
                    *offset,
                    ty,
                    MemoryAccessKind::Load,
                    pointer_bytes,
                )
            {
                accesses.push(access);
            }
            collect_accesses_from_expr(base, accesses, include_structured_accesses, pointer_bytes);
        }
        PreHirExpr::Index {
            base,
            index,
            elem_ty,
        } => {
            if include_structured_accesses
                && let Some(access) =
                    parse_index_access(base, index, elem_ty, MemoryAccessKind::Load, pointer_bytes)
            {
                accesses.push(access);
            }
            collect_accesses_from_expr(base, accesses, include_structured_accesses, pointer_bytes);
            collect_accesses_from_expr(index, accesses, include_structured_accesses, pointer_bytes);
        }
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. } => {
            collect_accesses_from_expr(expr, accesses, include_structured_accesses, pointer_bytes)
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            collect_accesses_from_expr(lhs, accesses, include_structured_accesses, pointer_bytes);
            collect_accesses_from_expr(rhs, accesses, include_structured_accesses, pointer_bytes);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                collect_accesses_from_expr(
                    arg,
                    accesses,
                    include_structured_accesses,
                    pointer_bytes,
                );
            }
        }
        PreHirExpr::PtrOffset { base, .. } => {
            collect_accesses_from_expr(base, accesses, include_structured_accesses, pointer_bytes)
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_accesses_from_expr(cond, accesses, include_structured_accesses, pointer_bytes);
            collect_accesses_from_expr(
                then_expr,
                accesses,
                include_structured_accesses,
                pointer_bytes,
            );
            collect_accesses_from_expr(
                else_expr,
                accesses,
                include_structured_accesses,
                pointer_bytes,
            );
        }
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => {}
    }
}

fn parse_recovered_field_access(
    base: &PreHirExpr,
    offset: u32,
    access_ty: &NirType,
    kind: MemoryAccessKind,
    pointer_bytes: u32,
) -> Option<PartitionedMemoryAccess> {
    let ptr = PreHirExpr::PtrOffset {
        base: Box::new(base.clone()),
        offset: i64::from(offset),
    };
    parse_partitioned_access(&ptr, access_ty, kind, pointer_bytes)
}

fn parse_index_access(
    base: &PreHirExpr,
    index: &PreHirExpr,
    elem_ty: &NirType,
    kind: MemoryAccessKind,
    pointer_bytes: u32,
) -> Option<PartitionedMemoryAccess> {
    let ptr = PreHirExpr::Index {
        base: Box::new(base.clone()),
        index: Box::new(index.clone()),
        elem_ty: elem_ty.clone(),
    };
    parse_partitioned_access(&ptr, elem_ty, kind, pointer_bytes)
}

fn parse_partitioned_access(
    ptr: &PreHirExpr,
    access_ty: &NirType,
    kind: MemoryAccessKind,
    pointer_bytes: u32,
) -> Option<PartitionedMemoryAccess> {
    let access_size = i64::from(type_byte_size_with_pointer_bytes(access_ty, pointer_bytes)?);
    let mut parts = AddressParts::default();
    collect_address_parts(ptr, &mut parts, 1, pointer_bytes)?;
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
        pointer_bytes,
    })
}

fn classify_base_object(base: &PreHirExpr) -> MemoryAccessClass {
    match base {
        PreHirExpr::AddressOfGlobal(_) => MemoryAccessClass::GlobalLike,
        PreHirExpr::AddressOfLocal(_) => MemoryAccessClass::Stack,
        PreHirExpr::Var(name) => {
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

fn collect_address_parts(
    expr: &PreHirExpr,
    parts: &mut AddressParts,
    sign: i64,
    pointer_bytes: u32,
) -> Option<()> {
    match expr {
        PreHirExpr::Const(value, _) => {
            parts.const_offset += sign * *value;
            Some(())
        }
        PreHirExpr::Cast { expr, .. } => collect_address_parts(expr, parts, sign, pointer_bytes),
        PreHirExpr::PtrOffset { base, offset } => {
            parts.const_offset += sign * *offset;
            collect_address_parts(base, parts, sign, pointer_bytes)
        }
        PreHirExpr::FieldAccess { base, offset, .. } => {
            parts.const_offset += sign * i64::from(*offset);
            collect_address_parts(base, parts, sign, pointer_bytes)
        }
        PreHirExpr::Index {
            base,
            index,
            elem_ty,
        } => {
            let stride = i64::from(type_byte_size_with_pointer_bytes(elem_ty, pointer_bytes)?);
            if stride <= 0 {
                return None;
            }
            collect_address_parts(base, parts, sign, pointer_bytes)?;
            if let PreHirExpr::Const(index, _) = index.as_ref() {
                let offset = index.checked_mul(stride)?.checked_mul(sign)?;
                parts.const_offset = parts.const_offset.checked_add(offset)?;
                Some(())
            } else {
                add_scaled_index(parts, index.as_ref().clone(), sign.checked_mul(stride)?)
            }
        }
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs,
            rhs,
            ty,
        } => {
            if let NirType::Ptr(elem_ty) = ty {
                if let Some(elem_sz) = type_byte_size_with_pointer_bytes(elem_ty, pointer_bytes).filter(|&sz| sz > 1) {
                    let elem_sz_i = elem_sz as i64;
                    if let PreHirExpr::Const(val, _) = rhs.as_ref() {
                        collect_address_parts(lhs, parts, sign, pointer_bytes)?;
                        let offset = val.checked_mul(elem_sz_i)?.checked_mul(sign)?;
                        parts.const_offset = parts.const_offset.checked_add(offset)?;
                        return Some(());
                    }
                    if let PreHirExpr::Const(val, _) = lhs.as_ref() {
                        collect_address_parts(rhs, parts, sign, pointer_bytes)?;
                        let offset = val.checked_mul(elem_sz_i)?.checked_mul(sign)?;
                        parts.const_offset = parts.const_offset.checked_add(offset)?;
                        return Some(());
                    }
                }
            }
            collect_address_parts(lhs, parts, sign, pointer_bytes)?;
            collect_address_parts(rhs, parts, sign, pointer_bytes)
        }
        PreHirExpr::Binary {
            op: PreHirBinaryOp::Sub,
            lhs,
            rhs,
            ty,
        } => {
            if let NirType::Ptr(elem_ty) = ty {
                if let Some(elem_sz) = type_byte_size_with_pointer_bytes(elem_ty, pointer_bytes).filter(|&sz| sz > 1) {
                    let elem_sz_i = elem_sz as i64;
                    if let PreHirExpr::Const(val, _) = rhs.as_ref() {
                        collect_address_parts(lhs, parts, sign, pointer_bytes)?;
                        let offset = val.checked_mul(elem_sz_i)?.checked_mul(-sign)?;
                        parts.const_offset = parts.const_offset.checked_add(offset)?;
                        return Some(());
                    }
                }
            }
            collect_address_parts(lhs, parts, sign, pointer_bytes)?;
            collect_address_parts(rhs, parts, -sign, pointer_bytes)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_fact_collector_includes_constant_index_and_field_accesses() {
        let u8_ty = NirType::Int {
            bits: 8,
            signed: false,
        };
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let body = vec![
            PreHirStmt::Expr(PreHirExpr::Load {
                ptr: Box::new(PreHirExpr::Var("node".to_string())),
                ty: u8_ty.clone(),
            }),
            PreHirStmt::Assign {
                lhs: PreHirLValue::Index {
                    base: Box::new(PreHirExpr::Var("node".to_string())),
                    index: Box::new(PreHirExpr::Const(
                        1,
                        NirType::Int {
                            bits: 32,
                            signed: true,
                        },
                    )),
                    elem_ty: u32_ty.clone(),
                },
                rhs: PreHirExpr::Const(0x1234_5678, u32_ty.clone()),
            },
            PreHirStmt::Expr(PreHirExpr::FieldAccess {
                base: Box::new(PreHirExpr::Var("node".to_string())),
                field_name: "tail".to_string(),
                offset: 8,
                ty: u32_ty.clone(),
            }),
        ];

        let pointer_only = collect_partitioned_memory_accesses(&body, true);
        assert_eq!(pointer_only.len(), 1);
        assert_eq!(pointer_only[0].kind, MemoryAccessKind::Load);
        assert_eq!(pointer_only[0].const_offset, 0);

        let accesses = collect_partitioned_memory_accesses_with_structured_accesses(&body, true);
        assert_eq!(accesses.len(), 3);
        let load = accesses
            .iter()
            .find(|access| access.kind == MemoryAccessKind::Load)
            .expect("raw or recovered field read should be collected");
        assert_eq!(load.const_offset, 0);
        assert_eq!(load.access_ty, u8_ty);
        assert!(matches!(&load.base, PreHirExpr::Var(name) if name == "node"));

        let indexed_store = accesses
            .iter()
            .find(|access| access.kind == MemoryAccessKind::Store)
            .expect("constant-index write should be collected");
        assert_eq!(indexed_store.const_offset, 4);
        assert_eq!(indexed_store.access_ty, u32_ty);
        assert!(indexed_store.stride.is_none());
        assert!(matches!(&indexed_store.base, PreHirExpr::Var(name) if name == "node"));

        let field_load = accesses
            .iter()
            .find(|access| access.kind == MemoryAccessKind::Load && access.const_offset == 8)
            .expect("recovered field read should be collected");
        assert_eq!(field_load.access_ty, u32_ty);
        assert!(matches!(&field_load.base, PreHirExpr::Var(name) if name == "node"));

        let dynamic_index = PreHirExpr::Index {
            base: Box::new(PreHirExpr::Var("node".to_string())),
            index: Box::new(PreHirExpr::Var("i".to_string())),
            elem_ty: u32_ty.clone(),
        };
        let dynamic_access =
            parse_partitioned_access(&dynamic_index, &u32_ty, MemoryAccessKind::Load, 8)
                .expect("dynamic index should retain its strided access shape");
        assert_eq!(dynamic_access.const_offset, 0);
        assert_eq!(dynamic_access.stride, Some(4));
    }

    #[test]
    fn parse_partitioned_access_scales_typed_pointer_binary_add_offset() {
        let u32_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        let ptr_u32_ty = NirType::Ptr(Box::new(u32_ty.clone()));
        let expr = PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            lhs: Box::new(PreHirExpr::Var("p".to_string())),
            rhs: Box::new(PreHirExpr::Const(3, u32_ty.clone())),
            ty: ptr_u32_ty.clone(),
        };
        let access = parse_partitioned_access(&expr, &u32_ty, MemoryAccessKind::Load, 8)
            .expect("should parse typed pointer add");
        // 3 elements * 4 bytes/element = 12 bytes
        assert_eq!(access.const_offset, 12);
    }

    #[test]
    fn global_and_local_address_classification_and_32bit_pointer_width() {
        let stderr_expr = PreHirExpr::AddressOfGlobal("stderr".to_string());
        assert_eq!(
            classify_base_object(&stderr_expr),
            MemoryAccessClass::GlobalLike
        );
        assert_eq!(
            classify_escape(&stderr_expr),
            MemoryEscapeClass::NonEscaping
        );

        let local_buf = PreHirExpr::AddressOfLocal("buffer".to_string());
        assert_eq!(classify_base_object(&local_buf), MemoryAccessClass::Stack);
        assert_eq!(classify_escape(&local_buf), MemoryEscapeClass::NonEscaping);

        // 32-bit pointer width check
        let ptr_ty = NirType::Ptr(Box::new(NirType::Int {
            bits: 32,
            signed: false,
        }));
        let load_32 = parse_partitioned_access(
            &PreHirExpr::Var("stack_0".to_string()),
            &ptr_ty,
            MemoryAccessKind::Load,
            4,
        )
        .expect("partitioned access");
        assert_eq!(load_32.pointer_bytes, 4);
        let key_32 = load_32.partition_key();
        assert_eq!(key_32.offset_interval, (0, 4));

        // 64-bit pointer width check
        let load_64 = parse_partitioned_access(
            &PreHirExpr::Var("stack_0".to_string()),
            &ptr_ty,
            MemoryAccessKind::Load,
            8,
        )
        .expect("partitioned access");
        assert_eq!(load_64.pointer_bytes, 8);
        let key_64 = load_64.partition_key();
        assert_eq!(key_64.offset_interval, (0, 8));
    }
}
