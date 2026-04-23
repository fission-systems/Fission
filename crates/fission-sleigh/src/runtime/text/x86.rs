pub(crate) fn format_memory_operand(
    base: Option<u8>,
    index: Option<u8>,
    scale: u8,
    displacement: i64,
    rip_relative: bool,
) -> String {
    let mut terms = Vec::new();
    if rip_relative {
        terms.push("rip".to_string());
    } else if let Some(base) = base {
        terms.push(register_name(base, 8).to_string());
    }
    if let Some(index) = index {
        let reg = register_name(index, 8);
        if scale > 1 {
            terms.push(format!("{reg}*{scale}"));
        } else {
            terms.push(reg.to_string());
        }
    }
    let mut expr = if terms.is_empty() {
        String::new()
    } else {
        terms.join("+")
    };
    if displacement != 0 || expr.is_empty() {
        if expr.is_empty() {
            if displacement < 0 {
                expr.push_str(&format!("-0x{:x}", displacement.unsigned_abs()));
            } else {
                expr.push_str(&format!("0x{:x}", displacement as u64));
            }
        } else if displacement < 0 {
            expr.push_str(&format!("-0x{:x}", displacement.unsigned_abs()));
        } else {
            expr.push_str(&format!("+0x{:x}", displacement as u64));
        }
    }
    format!("[{expr}]")
}

pub(crate) fn register_name(index: u8, size: u32) -> &'static str {
    const REG8: [&str; 16] = [
        "al", "cl", "dl", "bl", "spl", "bpl", "sil", "dil", "r8b", "r9b", "r10b", "r11b", "r12b",
        "r13b", "r14b", "r15b",
    ];
    const REG16: [&str; 16] = [
        "ax", "cx", "dx", "bx", "sp", "bp", "si", "di", "r8w", "r9w", "r10w", "r11w", "r12w",
        "r13w", "r14w", "r15w",
    ];
    const REG32: [&str; 16] = [
        "eax", "ecx", "edx", "ebx", "esp", "ebp", "esi", "edi", "r8d", "r9d", "r10d", "r11d",
        "r12d", "r13d", "r14d", "r15d",
    ];
    const REG64: [&str; 16] = [
        "rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi", "r8", "r9", "r10", "r11", "r12",
        "r13", "r14", "r15",
    ];
    let index = usize::from(index.min(15));
    match size {
        1 => REG8[index],
        2 => REG16[index],
        4 => REG32[index],
        _ => REG64[index],
    }
}

pub(crate) fn jcc_suffix(condition_code: u8) -> &'static str {
    match condition_code {
        0x0 => "o",
        0x1 => "no",
        0x2 => "b",
        0x3 => "ae",
        0x4 => "e",
        0x5 => "ne",
        0x6 => "be",
        0x7 => "a",
        0x8 => "s",
        0x9 => "ns",
        0xA => "p",
        0xB => "np",
        0xC => "l",
        0xD => "ge",
        0xE => "le",
        0xF => "g",
        _ => "cc",
    }
}
