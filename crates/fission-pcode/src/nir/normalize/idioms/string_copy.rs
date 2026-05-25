use super::super::*;
use std::collections::HashSet;

pub(crate) fn apply_string_copy_pass(func: &mut HirFunction) -> bool {
    let mut changed = false;
    if process_statement_list(&mut func.body, func.is_64bit) {
        changed = true;
    }
    changed
}

fn process_statement_list(stmts: &mut Vec<HirStmt>, is_64bit: bool) -> bool {
    let mut changed = false;

    // Recurse into nested blocks first
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. } => {
                changed |= process_statement_list(body, is_64bit);
            }
            HirStmt::For { init, update, body, .. } => {
                if let Some(init_stmt) = init {
                    if let HirStmt::Block(init_body) = init_stmt.as_mut() {
                        changed |= process_statement_list(init_body, is_64bit);
                    }
                }
                if let Some(update_stmt) = update {
                    if let HirStmt::Block(update_body) = update_stmt.as_mut() {
                        changed |= process_statement_list(update_body, is_64bit);
                    }
                }
                changed |= process_statement_list(body, is_64bit);
            }
            HirStmt::If { then_body, else_body, .. } => {
                changed |= process_statement_list(then_body, is_64bit);
                changed |= process_statement_list(else_body, is_64bit);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= process_statement_list(&mut case.body, is_64bit);
                }
                changed |= process_statement_list(default, is_64bit);
            }
            _ => {}
        }
    }

    // Now optimize at this block level
    let mut i = 0;
    while i < stmts.len() {
        if let Some((base, offset, val, size)) = match_char_store(&stmts[i]) {
            let mut collected = Vec::new();
            collected.push((offset, val, size, i));

            let mut j = i + 1;
            let mut base_vars = get_expr_vars(&base);
            
            while j < stmts.len() {
                if let Some((next_base, next_offset, next_val, next_size)) = match_char_store(&stmts[j]) {
                    if base == next_base {
                        collected.push((next_offset, next_val, next_size, j));
                        j += 1;
                        continue;
                    }
                }

                // Check interference
                if stmt_interferes(&stmts[j], &base, &base_vars) {
                    break;
                }
                j += 1;
            }

            // Sort collected stores by offset
            collected.sort_by_key(|item| item.0);

            // Check contiguity, size and validity
            if let Some(bytes) = check_contiguous_and_extract_bytes(&collected) {
                if collected.len() >= 2 && bytes.len() >= 4 && is_valid_string_bytes(&bytes) {
                    let dest_ptr = if collected[0].0 == 0 {
                        base.clone()
                    } else {
                        HirExpr::PtrOffset {
                            base: Box::new(base.clone()),
                            offset: collected[0].0,
                        }
                    };

                    let escaped = escape_string(&bytes);
                    let src_ptr = HirExpr::AddressOfGlobal(format!("*\"{}\"", escaped));

                    let count_val = bytes.len() as i64;
                    let count_ptr = HirExpr::Const(
                        count_val,
                        NirType::Int {
                            bits: if is_64bit { 64 } else { 32 },
                            signed: false,
                        },
                    );

                    let memcpy_call = HirStmt::Expr(HirExpr::Call {
                        target: "memcpy".to_string(),
                        args: vec![dest_ptr, src_ptr, count_ptr],
                        ty: NirType::Unknown,
                    });

                    // Rebuild stmts:
                    let remove_indices: HashSet<usize> = collected
                        .iter()
                        .skip(1)
                        .map(|item| item.3)
                        .collect();

                    let mut new_stmts = Vec::new();
                    for (idx, stmt) in stmts.drain(..).enumerate() {
                        if idx == i {
                            new_stmts.push(memcpy_call.clone());
                        } else if remove_indices.contains(&idx) {
                            // omit
                        } else {
                            new_stmts.push(stmt);
                        }
                    }
                    *stmts = new_stmts;
                    changed = true;
                    // Reset index to start re-scanning this block
                    i = 0;
                    continue;
                }
            }
        }
        i += 1;
    }

    changed
}

fn match_char_store(stmt: &HirStmt) -> Option<(HirExpr, i64, i64, usize)> {
    if let HirStmt::Assign { lhs, rhs } = stmt {
        if let HirExpr::Const(val, ty) = rhs {
            let val_size = match ty {
                NirType::Int { bits, .. } => (*bits as usize) / 8,
                NirType::Float { bits } => (*bits as usize) / 8,
                _ => 1,
            };
            match lhs {
                HirLValue::Index { base, index, elem_ty } => {
                    if let HirExpr::Const(idx, _) = &**index {
                        let elem_size = match elem_ty {
                            NirType::Int { bits, .. } => (*bits as usize) / 8,
                            NirType::Float { bits } => (*bits as usize) / 8,
                            _ => 1,
                        };
                        return Some((base.as_ref().clone(), *idx * (elem_size as i64), *val, val_size));
                    }
                }
                HirLValue::Deref { ptr, ty } => {
                    let deref_size = match ty {
                        NirType::Int { bits, .. } => (*bits as usize) / 8,
                        NirType::Float { bits } => (*bits as usize) / 8,
                        _ => 1,
                    };
                    let _ = deref_size; // compiler warning mitigation
                    match &**ptr {
                        HirExpr::PtrOffset { base, offset } => {
                            return Some((base.as_ref().clone(), *offset, *val, val_size));
                        }
                        other => {
                            return Some((other.clone(), 0, *val, val_size));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    None
}

fn get_expr_vars(expr: &HirExpr) -> HashSet<String> {
    let mut vars = HashSet::new();
    collect_vars(expr, &mut vars);
    vars
}

fn collect_vars(expr: &HirExpr, vars: &mut HashSet<String>) {
    match expr {
        HirExpr::Var(name) => {
            vars.insert(name.clone());
        }
        HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => {
            collect_vars(expr, vars);
        }
        HirExpr::Binary { lhs, rhs, .. }
        | HirExpr::Index { base: lhs, index: rhs, .. } => {
            collect_vars(lhs, vars);
            collect_vars(rhs, vars);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                collect_vars(arg, vars);
            }
        }
        HirExpr::Select { cond, then_expr, else_expr, .. } => {
            collect_vars(cond, vars);
            collect_vars(then_expr, vars);
            collect_vars(else_expr, vars);
        }
    }
}

fn lvalue_contains_var(lval: &HirLValue, var_name: &str) -> bool {
    match lval {
        HirLValue::Var(name) => name == var_name,
        HirLValue::Deref { ptr, .. } => expr_contains_var(ptr, var_name),
        HirLValue::Index { base, index, .. } => {
            expr_contains_var(base, var_name) || expr_contains_var(index, var_name)
        }
    }
}

fn expr_contains_var(expr: &HirExpr, var_name: &str) -> bool {
    match expr {
        HirExpr::Var(name) => name == var_name,
        HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => false,
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => expr_contains_var(expr, var_name),
        HirExpr::Binary { lhs, rhs, .. } => {
            expr_contains_var(lhs, var_name) || expr_contains_var(rhs, var_name)
        }
        HirExpr::Call { args, .. } => args.iter().any(|arg| expr_contains_var(arg, var_name)),
        HirExpr::Index { base, index, .. } => {
            expr_contains_var(base, var_name) || expr_contains_var(index, var_name)
        }
        HirExpr::Select { cond, then_expr, else_expr, .. } => {
            expr_contains_var(cond, var_name)
                || expr_contains_var(then_expr, var_name)
                || expr_contains_var(else_expr, var_name)
        }
    }
}

fn stmt_interferes(stmt: &HirStmt, base: &HirExpr, base_vars: &HashSet<String>) -> bool {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            for var in base_vars {
                if lvalue_contains_var(lhs, var) {
                    return true;
                }
            }
            expr_contains_call(rhs) || expr_reads_memory_of(rhs, base)
        }
        HirStmt::Expr(expr) => {
            expr_contains_call(expr) || expr_reads_memory_of(expr, base)
        }
        HirStmt::VaStart { va_list, .. } => {
            expr_contains_call(va_list) || expr_reads_memory_of(va_list, base)
        }
        _ => true,
    }
}

fn expr_contains_call(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Call { .. } => true,
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => expr_contains_call(expr),
        HirExpr::Binary { lhs, rhs, .. }
        | HirExpr::Index { base: lhs, index: rhs, .. } => {
            expr_contains_call(lhs) || expr_contains_call(rhs)
        }
        HirExpr::Select { cond, then_expr, else_expr, .. } => {
            expr_contains_call(cond) || expr_contains_call(then_expr) || expr_contains_call(else_expr)
        }
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => false,
    }
}

fn expr_reads_memory_of(expr: &HirExpr, base: &HirExpr) -> bool {
    match expr {
        HirExpr::Load { ptr, .. } => {
            ptr.as_ref() == base || is_offset_of(ptr, base)
        }
        HirExpr::Index { base: idx_base, .. } => {
            idx_base.as_ref() == base
        }
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => expr_reads_memory_of(expr, base),
        HirExpr::Binary { lhs, rhs, .. } => {
            expr_reads_memory_of(lhs, base) || expr_reads_memory_of(rhs, base)
        }
        HirExpr::Call { args, .. } => {
            args.iter().any(|arg| expr_reads_memory_of(arg, base))
        }
        HirExpr::Select { cond, then_expr, else_expr, .. } => {
            expr_reads_memory_of(cond, base)
                || expr_reads_memory_of(then_expr, base)
                || expr_reads_memory_of(else_expr, base)
        }
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => false,
    }
}

fn is_offset_of(ptr: &HirExpr, base: &HirExpr) -> bool {
    if let HirExpr::PtrOffset { base: inner_base, .. } = ptr {
        inner_base.as_ref() == base
    } else {
        false
    }
}

fn check_contiguous_and_extract_bytes(
    collected: &[(i64, i64, usize, usize)],
) -> Option<Vec<u8>> {
    if collected.is_empty() {
        return None;
    }
    let mut bytes = Vec::new();
    let mut expected_offset = collected[0].0;
    for &(offset, val, size, _) in collected {
        if offset != expected_offset {
            return None;
        }
        let mut temp = val;
        for _ in 0..size {
            bytes.push((temp & 0xff) as u8);
            temp >>= 8;
        }
        expected_offset += size as i64;
    }
    Some(bytes)
}

fn is_valid_string_bytes(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }
    for &b in bytes {
        if b == 0 || b == b'\t' || b == b'\n' || b == b'\r' || (b >= 32 && b <= 126) {
            continue;
        }
        return false;
    }
    true
}

fn escape_string(bytes: &[u8]) -> String {
    let mut escaped = String::new();
    for &b in bytes {
        match b {
            b'\\' => escaped.push_str("\\\\"),
            b'"' => escaped.push_str("\\\""),
            b'\n' => escaped.push_str("\\n"),
            b'\r' => escaped.push_str("\\r"),
            b'\t' => escaped.push_str("\\t"),
            0 => escaped.push_str("\\0"),
            32..=126 => escaped.push(b as char),
            _ => {
                escaped.push_str(&format!("\\x{:02x}", b));
            }
        }
    }
    escaped
}
