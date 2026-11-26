//! Operator precedence and spelling, rotate-shift detection, unary-minus
//! separation and integer-constant formatting.

use super::*;

pub(super) fn binary_precedence(op: HirBinaryOp) -> u8 {
    match op {
        HirBinaryOp::LogicalOr => 10,
        HirBinaryOp::LogicalAnd => 20,
        HirBinaryOp::Or => 30,
        HirBinaryOp::Xor => 40,
        HirBinaryOp::And => 50,
        HirBinaryOp::Eq | HirBinaryOp::Ne => 60,
        HirBinaryOp::Lt
        | HirBinaryOp::Le
        | HirBinaryOp::Gt
        | HirBinaryOp::Ge
        | HirBinaryOp::SLt
        | HirBinaryOp::SLe
        | HirBinaryOp::SGt
        | HirBinaryOp::SGe => 70,
        HirBinaryOp::Shl | HirBinaryOp::Shr | HirBinaryOp::Sar => 80,
        HirBinaryOp::Add | HirBinaryOp::Sub => 90,
        HirBinaryOp::Mul | HirBinaryOp::Div | HirBinaryOp::Mod => 100,
    }
}

/// Return the shift count when an integer right shift uses the machine word
/// width minus that count. This is the canonical rotate lowering emitted by
/// x86 and several other targets: `value >> (width - count)`. At count zero
/// the machine operation shifts by exactly the word width and produces zero,
/// while C makes that shift undefined. The caller renders a short-circuiting
/// conditional so the undefined shift is never evaluated.
pub(super) fn width_minus_shift_count(rhs: &HirExpr) -> Option<&HirExpr> {
    let HirExpr::Binary {
        op: HirBinaryOp::Sub,
        lhs,
        rhs: count,
        ..
    } = rhs
    else {
        return None;
    };
    let HirExpr::Const(width, _) = lhs.as_ref() else {
        return None;
    };
    matches!(*width, 8 | 16 | 32 | 64).then_some(count.as_ref())
}

pub(super) fn binary_rhs_parent_precedence(
    parent_op: HirBinaryOp,
    rhs: &HirExpr,
    fallback: u8,
) -> u8 {
    let HirExpr::Binary { op: rhs_op, .. } = rhs else {
        return fallback;
    };
    if matches!(parent_op, HirBinaryOp::Eq | HirBinaryOp::Ne)
        && matches!(
            rhs_op,
            HirBinaryOp::Lt
                | HirBinaryOp::Le
                | HirBinaryOp::Gt
                | HirBinaryOp::Ge
                | HirBinaryOp::SLt
                | HirBinaryOp::SLe
                | HirBinaryOp::SGt
                | HirBinaryOp::SGe
        )
    {
        return binary_precedence(*rhs_op) + 1;
    }
    fallback
}

pub(super) fn print_binary_op(op: HirBinaryOp) -> &'static str {
    match op {
        HirBinaryOp::Add => "+",
        HirBinaryOp::Sub => "-",
        HirBinaryOp::Mul => "*",
        HirBinaryOp::Div => "/",
        HirBinaryOp::Mod => "%",
        HirBinaryOp::LogicalAnd => "&&",
        HirBinaryOp::LogicalOr => "||",
        HirBinaryOp::And => "&",
        HirBinaryOp::Or => "|",
        HirBinaryOp::Xor => "^",
        HirBinaryOp::Shl => "<<",
        HirBinaryOp::Shr => ">>",
        HirBinaryOp::Sar => ">>", // Handled specially in print_expr_prec with signed cast.
        HirBinaryOp::Eq => "==",
        HirBinaryOp::Ne => "!=",
        HirBinaryOp::Lt | HirBinaryOp::SLt => "<",
        HirBinaryOp::Le | HirBinaryOp::SLe => "<=",
        HirBinaryOp::Gt | HirBinaryOp::SGt => ">",
        HirBinaryOp::Ge | HirBinaryOp::SGe => ">=",
    }
}

/// `-` followed by an operand that itself starts with `-` must not print as
/// `--`, which C reads as the pre-decrement operator (maximal munch): `-(-x)`
/// would become `--x`, a store to `x`, and `-(-5)` would not compile.
pub(super) fn separate_unary_minus(symbol: &str, inner: String) -> String {
    if symbol == "-" && inner.starts_with('-') {
        format!("({inner})")
    } else {
        inner
    }
}

/// Format an integer constant for C pseudocode.
///
/// SLEIGH immediates often arrive as unsigned bit patterns (`0x80000000` →
/// `2147483648` with `signed: false`). Prefer two's-complement decimal when the
/// high bit is set so `INT_MIN` reads as `-2147483648` rather than a value that
/// does not fit in signed `int`.
pub(super) fn print_integer_const(value: i64, ty: &NirType) -> String {
    let NirType::Int { bits, signed } = ty else {
        if matches!(ty, NirType::Ptr(_)) {
            // The caller already tried `ctx.global_names` and fell through to
            // us, so this is an address with no known symbol -- a string
            // pointer whose target text was not captured, a global outside
            // any resolved section, and so on. Every *other* address this
            // tool prints (disasm operands, `xrefs`, `hex --json`) is hex; an
            // agent reading a lone unresolved pointer constant as plain
            // decimal (`5368878324`) has no way to tell it apart from an
            // ordinary integer value it should treat as data, not a location.
            return format!("({})0x{:x}", print_type(ty), value as u64);
        }
        return value.to_string();
    };
    if !(1..=64).contains(bits) {
        return value.to_string();
    }
    let bits = *bits as u32;
    let mask = if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    let raw = (value as u64) & mask;
    let sign_bit = 1u64 << (bits - 1);
    if (raw & sign_bit) != 0 {
        let signed_val = if bits == 64 {
            raw as i64
        } else {
            (raw as i64) - (1i64 << bits)
        };
        // Prefer signed form for true signed types, and for classic INT_MIN
        // immediates (`0x80000000`) even when the NIR type is still unsigned.
        // Do *not* rewrite other high-bit unsigned constants (e.g. carry masks
        // like 0xFFFFFFFC) — those must stay decimal-unsigned for semantics.
        if signed_val < 0 && (*signed || raw == sign_bit) {
            return signed_val.to_string();
        }
    }
    value.to_string()
}
