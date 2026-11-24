use crate::prelude::*;

pub(super) fn is_eligible_for_speculative_merge_by_name(name: &str) -> bool {
    if is_hardware_register_variable(name) {
        let name_lower = name.to_lowercase();
        let is_sp_or_bp = matches!(
            name_lower.as_str(),
            "rsp" | "rbp" | "esp" | "ebp" | "sp" | "bp"
        );
        return !is_sp_or_bp
            && (name.starts_with("xVar")
                || name.starts_with("uVar")
                || name.starts_with("iVar")
                || name.starts_with("bVar")
                || name.starts_with("tmp_"));
    }
    name_priority(name) < 2
}

pub(super) fn name_priority(name: &str) -> usize {
    if name.starts_with("uVar_dp_") {
        return 0; // lowest priority (dp temp variables)
    }
    // Heritage SSA versions (memory heritage) participate in Cover-style merge.
    if name.starts_with("vVar_") {
        return 1;
    }
    if name.starts_with("uVar")
        || name.starts_with("iVar")
        || name.starts_with("xVar")
        || name.starts_with("bVar")
        || name.starts_with("temp_")
        || name.starts_with("temp")
    {
        return 1;
    }
    if name.starts_with("slot_") {
        return 1;
    }
    if name == "result" || name == "retval" {
        return 2;
    }
    3 // highest priority: meaningful recovered symbols
}

// Both predicates below compare ASCII-case-insensitively instead of lowering
// into a fresh `String`. Every token they test is pure ASCII and no non-ASCII
// character lowercases to an ASCII letter these tokens contain, so the result
// is identical to the former `to_lowercase()` comparison.
pub(super) fn is_sp_or_bp_name(name: &str) -> bool {
    ["rsp", "rbp", "esp", "ebp", "sp", "bp"]
        .iter()
        .any(|token| name.eq_ignore_ascii_case(token))
}

pub(super) fn is_hardware_register_variable(name: &str) -> bool {
    const GPRS: &[&str] = &[
        "eax", "ebx", "ecx", "edx", "esi", "edi", "esp", "ebp", "rax", "rbx", "rcx", "rdx", "rsi",
        "rdi", "rsp", "rbp", "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15", "ax", "bx",
        "cx", "dx", "sp", "bp", "si", "di", "al", "bl", "cl", "dl", "ah", "bh", "ch", "dh", "r8d",
        "r9d", "r10d", "r11d", "r12d", "r13d", "r14d", "r15d", "r8w", "r9w", "r10w", "r11w",
        "r12w", "r13w", "r14w", "r15w", "r8b", "r9b", "r10b", "r11b", "r12b", "r13b", "r14b",
        "r15b",
    ];
    let bytes = name.as_bytes();
    let starts_with_ignore_case = |prefix: &str| {
        bytes.len() >= prefix.len() && bytes[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
    };
    if GPRS.iter().any(|gpr| name.eq_ignore_ascii_case(gpr)) {
        return true;
    }
    if name.eq_ignore_ascii_case("reg") || starts_with_ignore_case("reg_") {
        return true;
    }
    if starts_with_ignore_case("reg") && bytes[3..].iter().all(u8::is_ascii_digit) {
        return true;
    }
    starts_with_ignore_case("xmm")
        || starts_with_ignore_case("ymm")
        || starts_with_ignore_case("zmm")
        || starts_with_ignore_case("fvar")
}

pub(super) fn is_eligible_for_speculative_merge(binding: &PreHirBinding) -> bool {
    if is_hardware_register_variable(&binding.name) {
        // A hardware name carries architectural storage identity even when its
        // binding origin is TempPreserved. Reusing it for a disjoint synthetic
        // range recreates the register-role collapse that materialization's
        // reaching-definition proofs intentionally prevented.
        return false;
    }
    // Symbolic Priority Preservation: Exclude variables with priority >= 2
    // (e.g. result, retval, or meaningful recovered symbols).
    if name_priority(&binding.name) >= 2 {
        return false;
    }
    true
}
