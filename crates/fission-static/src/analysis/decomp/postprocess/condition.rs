pub(super) fn negate_condition(cond: &str) -> String {
    let cond = cond.trim();

    if let Some(inner) = cond.strip_prefix('!') {
        let inner = inner.trim();
        if let Some(stripped) = inner.strip_prefix('(') {
            if let Some(stripped) = stripped.strip_suffix(')') {
                return stripped.trim().to_string();
            }
        }
        return inner.to_string();
    }

    let comparisons = [
        (">=", "<"),
        ("<=", ">"),
        ("!=", "=="),
        ("==", "!="),
        (">", "<="),
        ("<", ">="),
    ];
    for (op, negated) in &comparisons {
        if let Some(pos) = cond.find(op) {
            let lhs = &cond[..pos];
            let rhs = &cond[pos + op.len()..];
            return format!("{}{}{}", lhs, negated, rhs);
        }
    }

    format!("!({})", cond)
}
