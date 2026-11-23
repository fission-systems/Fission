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

pub(super) fn is_sp_or_bp_name(name: &str) -> bool {
    matches!(
        name.to_lowercase().as_str(),
        "rsp" | "rbp" | "esp" | "ebp" | "sp" | "bp"
    )
}

pub(super) fn is_hardware_register_variable(name: &str) -> bool {
    let name_lower = name.to_lowercase();
    let gprs = [
        "eax", "ebx", "ecx", "edx", "esi", "edi", "esp", "ebp", "rax", "rbx", "rcx", "rdx", "rsi",
        "rdi", "rsp", "rbp", "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15", "ax", "bx",
        "cx", "dx", "sp", "bp", "si", "di", "al", "bl", "cl", "dl", "ah", "bh", "ch", "dh", "r8d",
        "r9d", "r10d", "r11d", "r12d", "r13d", "r14d", "r15d", "r8w", "r9w", "r10w", "r11w",
        "r12w", "r13w", "r14w", "r15w", "r8b", "r9b", "r10b", "r11b", "r12b", "r13b", "r14b",
        "r15b",
    ];
    if gprs.contains(&name_lower.as_str()) {
        return true;
    }
    if name_lower == "reg" || name_lower.starts_with("reg_") {
        return true;
    }
    if name_lower.starts_with("reg") && name_lower[3..].chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    if name_lower.starts_with("xmm")
        || name_lower.starts_with("ymm")
        || name_lower.starts_with("zmm")
        || name_lower.starts_with("fvar")
    {
        return true;
    }
    false
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
