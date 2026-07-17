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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionedMemoryAccess {
    pub kind: MemoryAccessKind,
    pub ptr: HirExpr,
    pub base: HirExpr,
    pub base_repr: String,
    pub const_offset: i64,
    pub stride: Option<i64>,
    pub index: Option<HirExpr>,
    pub access_ty: NirType,
}

#[derive(Debug, Default, Clone)]
struct AddressParts {
    base: Option<HirExpr>,
    const_offset: i64,
    scaled_index: Option<(HirExpr, i64)>,
}

pub fn collect_partitioned_memory_accesses(
    stmts: &[HirStmt],
) -> Vec<PartitionedMemoryAccess> {
    let mut accesses = Vec::new();
    collect_accesses_from_stmts(stmts, &mut accesses);
    accesses
}

pub fn partition_key_for_pointer_expr(
    ptr: &HirExpr,
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
        PartitionKey {
            base_object: self.base_repr.clone(),
            offset_interval: (self.const_offset, self.const_offset + width),
            stride: self.stride,
            effect_class: classify_base_object(&self.base),
            escape_class: classify_escape(&self.base),
        }
    }
}

fn collect_accesses_from_stmts(stmts: &[HirStmt], accesses: &mut Vec<PartitionedMemoryAccess>) {
    for stmt in stmts {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                if let HirLValue::Deref { ptr, ty } = lhs
                    && let Some(access) = parse_partitioned_access(ptr, ty, MemoryAccessKind::Store)
                {
                    accesses.push(access);
                }
                collect_accesses_from_expr(rhs, accesses);
            }
            HirStmt::VaStart { va_list, .. } => {
                collect_accesses_from_expr(va_list, accesses);
            }
            HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
                collect_accesses_from_expr(expr, accesses);
            }
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => collect_accesses_from_stmts(body, accesses),
            HirStmt::Switch {
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
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                collect_accesses_from_expr(cond, accesses);
                collect_accesses_from_stmts(then_body, accesses);
                collect_accesses_from_stmts(else_body, accesses);
            }
            HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(None)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
}

fn collect_accesses_from_expr(expr: &HirExpr, accesses: &mut Vec<PartitionedMemoryAccess>) {
    match expr {
        HirExpr::Load { ptr, ty } => {
            if let Some(access) = parse_partitioned_access(ptr, ty, MemoryAccessKind::Load) {
                accesses.push(access);
            }
            collect_accesses_from_expr(ptr, accesses);
        }
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. }
        | HirExpr::FieldAccess { base: expr, .. } => collect_accesses_from_expr(expr, accesses),
        HirExpr::Binary { lhs, rhs, .. } => {
            collect_accesses_from_expr(lhs, accesses);
            collect_accesses_from_expr(rhs, accesses);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                collect_accesses_from_expr(arg, accesses);
            }
        }
        HirExpr::PtrOffset { base, .. } => collect_accesses_from_expr(base, accesses),
        HirExpr::Index { base, index, .. } => {
            collect_accesses_from_expr(base, accesses);
            collect_accesses_from_expr(index, accesses);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_accesses_from_expr(cond, accesses);
            collect_accesses_from_expr(then_expr, accesses);
            collect_accesses_from_expr(else_expr, accesses);
        }
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
    }
}

fn parse_partitioned_access(
    ptr: &HirExpr,
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

fn classify_base_object(base: &HirExpr) -> MemoryAccessClass {
    match base {
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => {
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
        HirExpr::PtrOffset { base, .. }
        | HirExpr::Cast { expr: base, .. }
        | HirExpr::FieldAccess { base, .. } => classify_base_object(base),
        _ => MemoryAccessClass::HeapLike,
    }
}

fn classify_escape(base: &HirExpr) -> MemoryEscapeClass {
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

fn collect_address_parts(expr: &HirExpr, parts: &mut AddressParts, sign: i64) -> Option<()> {
    match expr {
        HirExpr::Const(value, _) => {
            parts.const_offset += sign * *value;
            Some(())
        }
        HirExpr::Cast { expr, .. } => collect_address_parts(expr, parts, sign),
        HirExpr::PtrOffset { base, offset } => {
            parts.const_offset += sign * *offset;
            collect_address_parts(base, parts, sign)
        }
        HirExpr::FieldAccess { base, offset, .. } => {
            parts.const_offset += sign * i64::from(*offset);
            collect_address_parts(base, parts, sign)
        }
        HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => {
            collect_address_parts(lhs, parts, sign)?;
            collect_address_parts(rhs, parts, sign)
        }
        HirExpr::Binary {
            op: HirBinaryOp::Sub,
            lhs,
            rhs,
            ..
        } => {
            collect_address_parts(lhs, parts, sign)?;
            collect_address_parts(rhs, parts, -sign)
        }
        HirExpr::Binary {
            op: HirBinaryOp::Mul,
            lhs,
            rhs,
            ..
        } => {
            if let HirExpr::Const(value, _) = lhs.as_ref() {
                return add_scaled_index_expr(parts, rhs, sign * *value);
            }
            if let HirExpr::Const(value, _) = rhs.as_ref() {
                return add_scaled_index_expr(parts, lhs, sign * *value);
            }
            add_base_expr(parts, expr.clone(), sign)
        }
        HirExpr::Binary {
            op: HirBinaryOp::Shl,
            lhs,
            rhs,
            ..
        } => {
            let HirExpr::Const(shift, _) = rhs.as_ref() else {
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

fn add_scaled_index_expr(parts: &mut AddressParts, expr: &HirExpr, stride: i64) -> Option<()> {
    if let HirExpr::Const(value, _) = expr {
        parts.const_offset += stride * *value;
        return Some(());
    }
    if let Some((index, bias)) = extract_index_bias(expr) {
        parts.const_offset += stride * bias;
        return add_scaled_index(parts, index, stride);
    }
    add_scaled_index(parts, expr.clone(), stride)
}

fn extract_index_bias(expr: &HirExpr) -> Option<(HirExpr, i64)> {
    match expr {
        HirExpr::Cast { expr, .. } => extract_index_bias(expr),
        HirExpr::Binary {
            op: HirBinaryOp::Add,
            lhs,
            rhs,
            ..
        } => {
            if let HirExpr::Const(value, _) = lhs.as_ref() {
                let (index, bias) = extract_index_bias(rhs)?;
                return Some((index, bias + *value));
            }
            if let HirExpr::Const(value, _) = rhs.as_ref() {
                let (index, bias) = extract_index_bias(lhs)?;
                return Some((index, bias + *value));
            }
            if !expr_has_side_effects(expr) {
                Some((expr.clone(), 0))
            } else {
                None
            }
        }
        HirExpr::Binary {
            op: HirBinaryOp::Sub,
            lhs,
            rhs,
            ..
        } => {
            if let HirExpr::Const(value, _) = rhs.as_ref() {
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

fn add_base_expr(parts: &mut AddressParts, expr: HirExpr, sign: i64) -> Option<()> {
    if sign != 1 || matches!(expr, HirExpr::Const(_, _)) {
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

fn add_scaled_index(parts: &mut AddressParts, expr: HirExpr, stride: i64) -> Option<()> {
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
