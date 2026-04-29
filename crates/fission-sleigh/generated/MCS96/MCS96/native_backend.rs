// Auto-generated Fission Native Backend
#[no_mangle]
pub extern "C" fn fission_decode_match(table_ptr: *const i8, bytes: *const u8, bytes_len: usize, ctx_ptr: *const u64) -> i32 {
    let table_name = unsafe { std::ffi::CStr::from_ptr(table_ptr).to_str().unwrap() };
    let bytes = unsafe { std::slice::from_raw_parts(bytes, bytes_len) };
    let ctx = unsafe { *ctx_ptr };
    match table_name {
        "baop8" => match_node_baop8_0(bytes, ctx),
        "breg" => match_node_breg_0(bytes, ctx),
        "cc" => match_node_cc_0(bytes, ctx),
        "immed16" => match_node_immed16_0(bytes, ctx),
        "immed8" => match_node_immed8_0(bytes, ctx),
        "instruction" => match_node_instruction_0(bytes, ctx),
        "iwreg" => match_node_iwreg_0(bytes, ctx),
        "jmpdest" => match_node_jmpdest_0(bytes, ctx),
        "jmpdest11" => match_node_jmpdest11_0(bytes, ctx),
        "jmpdest16" => match_node_jmpdest16_0(bytes, ctx),
        "oper16" => match_node_oper16_0(bytes, ctx),
        "oper8" => match_node_oper8_0(bytes, ctx),
        "simmed8" => match_node_simmed8_0(bytes, ctx),
        "waop16" => match_node_waop16_0(bytes, ctx),
        "wreg" => match_node_wreg_0(bytes, ctx),
        _ => -1
    }
}

fn match_node_baop8_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_baop8_1(bytes, ctx),
        1 => match_node_baop8_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_baop8_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_baop8_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_breg_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_breg_1(bytes, ctx),
        1 => match_node_breg_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_breg_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_breg_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_cc_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 15;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_cc_1(bytes, ctx),
        1 => match_node_cc_2(bytes, ctx),
        2 => match_node_cc_3(bytes, ctx),
        3 => match_node_cc_4(bytes, ctx),
        4 => match_node_cc_5(bytes, ctx),
        5 => match_node_cc_6(bytes, ctx),
        6 => match_node_cc_7(bytes, ctx),
        7 => match_node_cc_8(bytes, ctx),
        8 => match_node_cc_9(bytes, ctx),
        9 => match_node_cc_10(bytes, ctx),
        10 => match_node_cc_11(bytes, ctx),
        11 => match_node_cc_12(bytes, ctx),
        12 => match_node_cc_13(bytes, ctx),
        13 => match_node_cc_14(bytes, ctx),
        14 => match_node_cc_15(bytes, ctx),
        15 => match_node_cc_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_cc_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_cc_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_cc_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_cc_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_cc_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 4");
    4
}

fn match_node_cc_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_cc_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 6");
    6
}

fn match_node_cc_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 7");
    7
}

fn match_node_cc_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 8");
    8
}

fn match_node_cc_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 9");
    9
}

fn match_node_cc_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 10");
    10
}

fn match_node_cc_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 11");
    11
}

fn match_node_cc_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 12");
    12
}

fn match_node_cc_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 13");
    13
}

fn match_node_cc_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 14");
    14
}

fn match_node_cc_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 15");
    15
}

fn match_node_immed16_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_immed8_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 15;
    eprintln!("Trace node 0: SlaInstructionBits start=0, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1(bytes, ctx),
        1 => match_node_instruction_116(bytes, ctx),
        2 => match_node_instruction_183(bytes, ctx),
        3 => match_node_instruction_186(bytes, ctx),
        4 => match_node_instruction_189(bytes, ctx),
        5 => match_node_instruction_194(bytes, ctx),
        6 => match_node_instruction_199(bytes, ctx),
        7 => match_node_instruction_204(bytes, ctx),
        8 => match_node_instruction_209(bytes, ctx),
        9 => match_node_instruction_214(bytes, ctx),
        10 => match_node_instruction_219(bytes, ctx),
        11 => match_node_instruction_224(bytes, ctx),
        12 => match_node_instruction_229(bytes, ctx),
        13 => match_node_instruction_234(bytes, ctx),
        14 => match_node_instruction_235(bytes, ctx),
        15 => match_node_instruction_252(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 15;
    eprintln!("Trace node 1: SlaInstructionBits start=4, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_2(bytes, ctx),
        1 => match_node_instruction_3(bytes, ctx),
        2 => match_node_instruction_4(bytes, ctx),
        3 => match_node_instruction_5(bytes, ctx),
        4 => match_node_instruction_6(bytes, ctx),
        5 => match_node_instruction_7(bytes, ctx),
        6 => match_node_instruction_8(bytes, ctx),
        7 => match_node_instruction_9(bytes, ctx),
        8 => match_node_instruction_10(bytes, ctx),
        9 => match_node_instruction_27(bytes, ctx),
        10 => match_node_instruction_44(bytes, ctx),
        11 => match_node_instruction_61(bytes, ctx),
        12 => match_node_instruction_64(bytes, ctx),
        13 => match_node_instruction_81(bytes, ctx),
        14 => match_node_instruction_98(bytes, ctx),
        15 => match_node_instruction_115(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 91");
    91
}

fn match_node_instruction_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 13");
    13
}

fn match_node_instruction_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 58");
    58
}

fn match_node_instruction_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 54");
    54
}

fn match_node_instruction_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 103");
    103
}

fn match_node_instruction_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 20");
    20
}

fn match_node_instruction_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 32");
    32
}

fn match_node_instruction_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 34");
    34
}

fn match_node_instruction_10(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 15;
    eprintln!("Trace node 10: SlaInstructionBits start=8, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_11(bytes, ctx),
        1 => match_node_instruction_12(bytes, ctx),
        2 => match_node_instruction_13(bytes, ctx),
        3 => match_node_instruction_14(bytes, ctx),
        4 => match_node_instruction_15(bytes, ctx),
        5 => match_node_instruction_16(bytes, ctx),
        6 => match_node_instruction_17(bytes, ctx),
        7 => match_node_instruction_18(bytes, ctx),
        8 => match_node_instruction_19(bytes, ctx),
        9 => match_node_instruction_20(bytes, ctx),
        10 => match_node_instruction_21(bytes, ctx),
        11 => match_node_instruction_22(bytes, ctx),
        12 => match_node_instruction_23(bytes, ctx),
        13 => match_node_instruction_24(bytes, ctx),
        14 => match_node_instruction_25(bytes, ctx),
        15 => match_node_instruction_26(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 78");
    78
}

fn match_node_instruction_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 79");
    79
}

fn match_node_instruction_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 79");
    79
}

fn match_node_instruction_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 79");
    79
}

fn match_node_instruction_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 79");
    79
}

fn match_node_instruction_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 79");
    79
}

fn match_node_instruction_17(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 17: Terminal matched constructor ID 79");
    79
}

fn match_node_instruction_18(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 18: Terminal matched constructor ID 79");
    79
}

fn match_node_instruction_19(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 19: Terminal matched constructor ID 79");
    79
}

fn match_node_instruction_20(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 20: Terminal matched constructor ID 79");
    79
}

fn match_node_instruction_21(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 21: Terminal matched constructor ID 79");
    79
}

fn match_node_instruction_22(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 22: Terminal matched constructor ID 79");
    79
}

fn match_node_instruction_23(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 23: Terminal matched constructor ID 79");
    79
}

fn match_node_instruction_24(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 24: Terminal matched constructor ID 79");
    79
}

fn match_node_instruction_25(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 25: Terminal matched constructor ID 79");
    79
}

fn match_node_instruction_26(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 26: Terminal matched constructor ID 79");
    79
}

fn match_node_instruction_27(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 15;
    eprintln!("Trace node 27: SlaInstructionBits start=8, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_28(bytes, ctx),
        1 => match_node_instruction_29(bytes, ctx),
        2 => match_node_instruction_30(bytes, ctx),
        3 => match_node_instruction_31(bytes, ctx),
        4 => match_node_instruction_32(bytes, ctx),
        5 => match_node_instruction_33(bytes, ctx),
        6 => match_node_instruction_34(bytes, ctx),
        7 => match_node_instruction_35(bytes, ctx),
        8 => match_node_instruction_36(bytes, ctx),
        9 => match_node_instruction_37(bytes, ctx),
        10 => match_node_instruction_38(bytes, ctx),
        11 => match_node_instruction_39(bytes, ctx),
        12 => match_node_instruction_40(bytes, ctx),
        13 => match_node_instruction_41(bytes, ctx),
        14 => match_node_instruction_42(bytes, ctx),
        15 => match_node_instruction_43(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_28(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 28: Terminal matched constructor ID 72");
    72
}

fn match_node_instruction_29(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 29: Terminal matched constructor ID 73");
    73
}

fn match_node_instruction_30(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 30: Terminal matched constructor ID 73");
    73
}

fn match_node_instruction_31(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 31: Terminal matched constructor ID 73");
    73
}

fn match_node_instruction_32(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 32: Terminal matched constructor ID 73");
    73
}

fn match_node_instruction_33(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 33: Terminal matched constructor ID 73");
    73
}

fn match_node_instruction_34(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 34: Terminal matched constructor ID 73");
    73
}

fn match_node_instruction_35(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 35: Terminal matched constructor ID 73");
    73
}

fn match_node_instruction_36(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 36: Terminal matched constructor ID 73");
    73
}

fn match_node_instruction_37(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 37: Terminal matched constructor ID 73");
    73
}

fn match_node_instruction_38(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 38: Terminal matched constructor ID 73");
    73
}

fn match_node_instruction_39(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 39: Terminal matched constructor ID 73");
    73
}

fn match_node_instruction_40(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 40: Terminal matched constructor ID 73");
    73
}

fn match_node_instruction_41(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 41: Terminal matched constructor ID 73");
    73
}

fn match_node_instruction_42(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 42: Terminal matched constructor ID 73");
    73
}

fn match_node_instruction_43(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 43: Terminal matched constructor ID 73");
    73
}

fn match_node_instruction_44(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 15;
    eprintln!("Trace node 44: SlaInstructionBits start=8, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_45(bytes, ctx),
        1 => match_node_instruction_46(bytes, ctx),
        2 => match_node_instruction_47(bytes, ctx),
        3 => match_node_instruction_48(bytes, ctx),
        4 => match_node_instruction_49(bytes, ctx),
        5 => match_node_instruction_50(bytes, ctx),
        6 => match_node_instruction_51(bytes, ctx),
        7 => match_node_instruction_52(bytes, ctx),
        8 => match_node_instruction_53(bytes, ctx),
        9 => match_node_instruction_54(bytes, ctx),
        10 => match_node_instruction_55(bytes, ctx),
        11 => match_node_instruction_56(bytes, ctx),
        12 => match_node_instruction_57(bytes, ctx),
        13 => match_node_instruction_58(bytes, ctx),
        14 => match_node_instruction_59(bytes, ctx),
        15 => match_node_instruction_60(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_45(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 45: Terminal matched constructor ID 80");
    80
}

fn match_node_instruction_46(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 46: Terminal matched constructor ID 81");
    81
}

fn match_node_instruction_47(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 47: Terminal matched constructor ID 81");
    81
}

fn match_node_instruction_48(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 48: Terminal matched constructor ID 81");
    81
}

fn match_node_instruction_49(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 49: Terminal matched constructor ID 81");
    81
}

fn match_node_instruction_50(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 50: Terminal matched constructor ID 81");
    81
}

fn match_node_instruction_51(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 51: Terminal matched constructor ID 81");
    81
}

fn match_node_instruction_52(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 52: Terminal matched constructor ID 81");
    81
}

fn match_node_instruction_53(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 53: Terminal matched constructor ID 81");
    81
}

fn match_node_instruction_54(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 54: Terminal matched constructor ID 81");
    81
}

fn match_node_instruction_55(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 55: Terminal matched constructor ID 81");
    81
}

fn match_node_instruction_56(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 56: Terminal matched constructor ID 81");
    81
}

fn match_node_instruction_57(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 57: Terminal matched constructor ID 81");
    81
}

fn match_node_instruction_58(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 58: Terminal matched constructor ID 81");
    81
}

fn match_node_instruction_59(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 59: Terminal matched constructor ID 81");
    81
}

fn match_node_instruction_60(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 60: Terminal matched constructor ID 81");
    81
}

fn match_node_instruction_61(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 61: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_62(bytes, ctx),
        1 => match_node_instruction_63(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_62(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 62: Terminal matched constructor ID 104");
    104
}

fn match_node_instruction_63(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 63: Terminal matched constructor ID 105");
    105
}

fn match_node_instruction_64(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 15;
    eprintln!("Trace node 64: SlaInstructionBits start=8, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_65(bytes, ctx),
        1 => match_node_instruction_66(bytes, ctx),
        2 => match_node_instruction_67(bytes, ctx),
        3 => match_node_instruction_68(bytes, ctx),
        4 => match_node_instruction_69(bytes, ctx),
        5 => match_node_instruction_70(bytes, ctx),
        6 => match_node_instruction_71(bytes, ctx),
        7 => match_node_instruction_72(bytes, ctx),
        8 => match_node_instruction_73(bytes, ctx),
        9 => match_node_instruction_74(bytes, ctx),
        10 => match_node_instruction_75(bytes, ctx),
        11 => match_node_instruction_76(bytes, ctx),
        12 => match_node_instruction_77(bytes, ctx),
        13 => match_node_instruction_78(bytes, ctx),
        14 => match_node_instruction_79(bytes, ctx),
        15 => match_node_instruction_80(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_65(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 65: Terminal matched constructor ID 88");
    88
}

fn match_node_instruction_66(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 66: Terminal matched constructor ID 89");
    89
}

fn match_node_instruction_67(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 67: Terminal matched constructor ID 89");
    89
}

fn match_node_instruction_68(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 68: Terminal matched constructor ID 89");
    89
}

fn match_node_instruction_69(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 69: Terminal matched constructor ID 89");
    89
}

fn match_node_instruction_70(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 70: Terminal matched constructor ID 89");
    89
}

fn match_node_instruction_71(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 71: Terminal matched constructor ID 89");
    89
}

fn match_node_instruction_72(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 72: Terminal matched constructor ID 89");
    89
}

fn match_node_instruction_73(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 73: Terminal matched constructor ID 89");
    89
}

fn match_node_instruction_74(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 74: Terminal matched constructor ID 89");
    89
}

fn match_node_instruction_75(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 75: Terminal matched constructor ID 89");
    89
}

fn match_node_instruction_76(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 76: Terminal matched constructor ID 89");
    89
}

fn match_node_instruction_77(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 77: Terminal matched constructor ID 89");
    89
}

fn match_node_instruction_78(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 78: Terminal matched constructor ID 89");
    89
}

fn match_node_instruction_79(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 79: Terminal matched constructor ID 89");
    89
}

fn match_node_instruction_80(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 80: Terminal matched constructor ID 89");
    89
}

fn match_node_instruction_81(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 15;
    eprintln!("Trace node 81: SlaInstructionBits start=8, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_82(bytes, ctx),
        1 => match_node_instruction_83(bytes, ctx),
        2 => match_node_instruction_84(bytes, ctx),
        3 => match_node_instruction_85(bytes, ctx),
        4 => match_node_instruction_86(bytes, ctx),
        5 => match_node_instruction_87(bytes, ctx),
        6 => match_node_instruction_88(bytes, ctx),
        7 => match_node_instruction_89(bytes, ctx),
        8 => match_node_instruction_90(bytes, ctx),
        9 => match_node_instruction_91(bytes, ctx),
        10 => match_node_instruction_92(bytes, ctx),
        11 => match_node_instruction_93(bytes, ctx),
        12 => match_node_instruction_94(bytes, ctx),
        13 => match_node_instruction_95(bytes, ctx),
        14 => match_node_instruction_96(bytes, ctx),
        15 => match_node_instruction_97(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_82(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 82: Terminal matched constructor ID 76");
    76
}

fn match_node_instruction_83(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 83: Terminal matched constructor ID 77");
    77
}

fn match_node_instruction_84(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 84: Terminal matched constructor ID 77");
    77
}

fn match_node_instruction_85(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 85: Terminal matched constructor ID 77");
    77
}

fn match_node_instruction_86(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 86: Terminal matched constructor ID 77");
    77
}

fn match_node_instruction_87(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 87: Terminal matched constructor ID 77");
    77
}

fn match_node_instruction_88(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 88: Terminal matched constructor ID 77");
    77
}

fn match_node_instruction_89(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 89: Terminal matched constructor ID 77");
    77
}

fn match_node_instruction_90(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 90: Terminal matched constructor ID 77");
    77
}

fn match_node_instruction_91(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 91: Terminal matched constructor ID 77");
    77
}

fn match_node_instruction_92(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 92: Terminal matched constructor ID 77");
    77
}

fn match_node_instruction_93(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 93: Terminal matched constructor ID 77");
    77
}

fn match_node_instruction_94(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 94: Terminal matched constructor ID 77");
    77
}

fn match_node_instruction_95(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 95: Terminal matched constructor ID 77");
    77
}

fn match_node_instruction_96(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 96: Terminal matched constructor ID 77");
    77
}

fn match_node_instruction_97(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 97: Terminal matched constructor ID 77");
    77
}

fn match_node_instruction_98(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 15;
    eprintln!("Trace node 98: SlaInstructionBits start=8, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_99(bytes, ctx),
        1 => match_node_instruction_100(bytes, ctx),
        2 => match_node_instruction_101(bytes, ctx),
        3 => match_node_instruction_102(bytes, ctx),
        4 => match_node_instruction_103(bytes, ctx),
        5 => match_node_instruction_104(bytes, ctx),
        6 => match_node_instruction_105(bytes, ctx),
        7 => match_node_instruction_106(bytes, ctx),
        8 => match_node_instruction_107(bytes, ctx),
        9 => match_node_instruction_108(bytes, ctx),
        10 => match_node_instruction_109(bytes, ctx),
        11 => match_node_instruction_110(bytes, ctx),
        12 => match_node_instruction_111(bytes, ctx),
        13 => match_node_instruction_112(bytes, ctx),
        14 => match_node_instruction_113(bytes, ctx),
        15 => match_node_instruction_114(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_99(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 99: Terminal matched constructor ID 84");
    84
}

fn match_node_instruction_100(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 100: Terminal matched constructor ID 85");
    85
}

fn match_node_instruction_101(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 101: Terminal matched constructor ID 85");
    85
}

fn match_node_instruction_102(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 102: Terminal matched constructor ID 85");
    85
}

fn match_node_instruction_103(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 103: Terminal matched constructor ID 85");
    85
}

fn match_node_instruction_104(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 104: Terminal matched constructor ID 85");
    85
}

fn match_node_instruction_105(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 105: Terminal matched constructor ID 85");
    85
}

fn match_node_instruction_106(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 106: Terminal matched constructor ID 85");
    85
}

fn match_node_instruction_107(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 107: Terminal matched constructor ID 85");
    85
}

fn match_node_instruction_108(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 108: Terminal matched constructor ID 85");
    85
}

fn match_node_instruction_109(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 109: Terminal matched constructor ID 85");
    85
}

fn match_node_instruction_110(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 110: Terminal matched constructor ID 85");
    85
}

fn match_node_instruction_111(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 111: Terminal matched constructor ID 85");
    85
}

fn match_node_instruction_112(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 112: Terminal matched constructor ID 85");
    85
}

fn match_node_instruction_113(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 113: Terminal matched constructor ID 85");
    85
}

fn match_node_instruction_114(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 114: Terminal matched constructor ID 85");
    85
}

fn match_node_instruction_115(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 115: Terminal matched constructor ID 57");
    57
}

fn match_node_instruction_116(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 15;
    eprintln!("Trace node 116: SlaInstructionBits start=4, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_117(bytes, ctx),
        1 => match_node_instruction_118(bytes, ctx),
        2 => match_node_instruction_119(bytes, ctx),
        3 => match_node_instruction_120(bytes, ctx),
        4 => match_node_instruction_121(bytes, ctx),
        5 => match_node_instruction_122(bytes, ctx),
        6 => match_node_instruction_123(bytes, ctx),
        7 => match_node_instruction_124(bytes, ctx),
        8 => match_node_instruction_125(bytes, ctx),
        9 => match_node_instruction_142(bytes, ctx),
        10 => match_node_instruction_159(bytes, ctx),
        11 => match_node_instruction_176(bytes, ctx),
        12 => match_node_instruction_179(bytes, ctx),
        13 => match_node_instruction_180(bytes, ctx),
        14 => match_node_instruction_181(bytes, ctx),
        15 => match_node_instruction_182(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_117(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 117: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_118(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 118: Terminal matched constructor ID 14");
    14
}

fn match_node_instruction_119(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 119: Terminal matched constructor ID 59");
    59
}

fn match_node_instruction_120(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 120: Terminal matched constructor ID 55");
    55
}

fn match_node_instruction_121(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 121: Terminal matched constructor ID 106");
    106
}

fn match_node_instruction_122(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 122: Terminal matched constructor ID 21");
    21
}

fn match_node_instruction_123(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 123: Terminal matched constructor ID 33");
    33
}

fn match_node_instruction_124(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 124: Terminal matched constructor ID 35");
    35
}

fn match_node_instruction_125(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 15;
    eprintln!("Trace node 125: SlaInstructionBits start=8, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_126(bytes, ctx),
        1 => match_node_instruction_127(bytes, ctx),
        2 => match_node_instruction_128(bytes, ctx),
        3 => match_node_instruction_129(bytes, ctx),
        4 => match_node_instruction_130(bytes, ctx),
        5 => match_node_instruction_131(bytes, ctx),
        6 => match_node_instruction_132(bytes, ctx),
        7 => match_node_instruction_133(bytes, ctx),
        8 => match_node_instruction_134(bytes, ctx),
        9 => match_node_instruction_135(bytes, ctx),
        10 => match_node_instruction_136(bytes, ctx),
        11 => match_node_instruction_137(bytes, ctx),
        12 => match_node_instruction_138(bytes, ctx),
        13 => match_node_instruction_139(bytes, ctx),
        14 => match_node_instruction_140(bytes, ctx),
        15 => match_node_instruction_141(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_126(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 126: Terminal matched constructor ID 86");
    86
}

fn match_node_instruction_127(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 127: Terminal matched constructor ID 87");
    87
}

fn match_node_instruction_128(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 128: Terminal matched constructor ID 87");
    87
}

fn match_node_instruction_129(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 129: Terminal matched constructor ID 87");
    87
}

fn match_node_instruction_130(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 130: Terminal matched constructor ID 87");
    87
}

fn match_node_instruction_131(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 131: Terminal matched constructor ID 87");
    87
}

fn match_node_instruction_132(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 132: Terminal matched constructor ID 87");
    87
}

fn match_node_instruction_133(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 133: Terminal matched constructor ID 87");
    87
}

fn match_node_instruction_134(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 134: Terminal matched constructor ID 87");
    87
}

fn match_node_instruction_135(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 135: Terminal matched constructor ID 87");
    87
}

fn match_node_instruction_136(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 136: Terminal matched constructor ID 87");
    87
}

fn match_node_instruction_137(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 137: Terminal matched constructor ID 87");
    87
}

fn match_node_instruction_138(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 138: Terminal matched constructor ID 87");
    87
}

fn match_node_instruction_139(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 139: Terminal matched constructor ID 87");
    87
}

fn match_node_instruction_140(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 140: Terminal matched constructor ID 87");
    87
}

fn match_node_instruction_141(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 141: Terminal matched constructor ID 87");
    87
}

fn match_node_instruction_142(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 15;
    eprintln!("Trace node 142: SlaInstructionBits start=8, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_143(bytes, ctx),
        1 => match_node_instruction_144(bytes, ctx),
        2 => match_node_instruction_145(bytes, ctx),
        3 => match_node_instruction_146(bytes, ctx),
        4 => match_node_instruction_147(bytes, ctx),
        5 => match_node_instruction_148(bytes, ctx),
        6 => match_node_instruction_149(bytes, ctx),
        7 => match_node_instruction_150(bytes, ctx),
        8 => match_node_instruction_151(bytes, ctx),
        9 => match_node_instruction_152(bytes, ctx),
        10 => match_node_instruction_153(bytes, ctx),
        11 => match_node_instruction_154(bytes, ctx),
        12 => match_node_instruction_155(bytes, ctx),
        13 => match_node_instruction_156(bytes, ctx),
        14 => match_node_instruction_157(bytes, ctx),
        15 => match_node_instruction_158(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_143(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 143: Terminal matched constructor ID 74");
    74
}

fn match_node_instruction_144(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 144: Terminal matched constructor ID 75");
    75
}

fn match_node_instruction_145(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 145: Terminal matched constructor ID 75");
    75
}

fn match_node_instruction_146(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 146: Terminal matched constructor ID 75");
    75
}

fn match_node_instruction_147(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 147: Terminal matched constructor ID 75");
    75
}

fn match_node_instruction_148(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 148: Terminal matched constructor ID 75");
    75
}

fn match_node_instruction_149(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 149: Terminal matched constructor ID 75");
    75
}

fn match_node_instruction_150(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 150: Terminal matched constructor ID 75");
    75
}

fn match_node_instruction_151(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 151: Terminal matched constructor ID 75");
    75
}

fn match_node_instruction_152(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 152: Terminal matched constructor ID 75");
    75
}

fn match_node_instruction_153(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 153: Terminal matched constructor ID 75");
    75
}

fn match_node_instruction_154(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 154: Terminal matched constructor ID 75");
    75
}

fn match_node_instruction_155(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 155: Terminal matched constructor ID 75");
    75
}

fn match_node_instruction_156(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 156: Terminal matched constructor ID 75");
    75
}

fn match_node_instruction_157(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 157: Terminal matched constructor ID 75");
    75
}

fn match_node_instruction_158(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 158: Terminal matched constructor ID 75");
    75
}

fn match_node_instruction_159(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 15;
    eprintln!("Trace node 159: SlaInstructionBits start=8, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_160(bytes, ctx),
        1 => match_node_instruction_161(bytes, ctx),
        2 => match_node_instruction_162(bytes, ctx),
        3 => match_node_instruction_163(bytes, ctx),
        4 => match_node_instruction_164(bytes, ctx),
        5 => match_node_instruction_165(bytes, ctx),
        6 => match_node_instruction_166(bytes, ctx),
        7 => match_node_instruction_167(bytes, ctx),
        8 => match_node_instruction_168(bytes, ctx),
        9 => match_node_instruction_169(bytes, ctx),
        10 => match_node_instruction_170(bytes, ctx),
        11 => match_node_instruction_171(bytes, ctx),
        12 => match_node_instruction_172(bytes, ctx),
        13 => match_node_instruction_173(bytes, ctx),
        14 => match_node_instruction_174(bytes, ctx),
        15 => match_node_instruction_175(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_160(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 160: Terminal matched constructor ID 82");
    82
}

fn match_node_instruction_161(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 161: Terminal matched constructor ID 83");
    83
}

fn match_node_instruction_162(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 162: Terminal matched constructor ID 83");
    83
}

fn match_node_instruction_163(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 163: Terminal matched constructor ID 83");
    83
}

fn match_node_instruction_164(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 164: Terminal matched constructor ID 83");
    83
}

fn match_node_instruction_165(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 165: Terminal matched constructor ID 83");
    83
}

fn match_node_instruction_166(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 166: Terminal matched constructor ID 83");
    83
}

fn match_node_instruction_167(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 167: Terminal matched constructor ID 83");
    83
}

fn match_node_instruction_168(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 168: Terminal matched constructor ID 83");
    83
}

fn match_node_instruction_169(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 169: Terminal matched constructor ID 83");
    83
}

fn match_node_instruction_170(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 170: Terminal matched constructor ID 83");
    83
}

fn match_node_instruction_171(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 171: Terminal matched constructor ID 83");
    83
}

fn match_node_instruction_172(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 172: Terminal matched constructor ID 83");
    83
}

fn match_node_instruction_173(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 173: Terminal matched constructor ID 83");
    83
}

fn match_node_instruction_174(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 174: Terminal matched constructor ID 83");
    83
}

fn match_node_instruction_175(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 175: Terminal matched constructor ID 83");
    83
}

fn match_node_instruction_176(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 176: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_177(bytes, ctx),
        1 => match_node_instruction_178(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_177(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 177: Terminal matched constructor ID 107");
    107
}

fn match_node_instruction_178(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 178: Terminal matched constructor ID 108");
    108
}

fn match_node_instruction_179(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 179: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_180(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 180: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_181(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 181: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_182(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 182: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_183(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 183: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_184(bytes, ctx),
        1 => match_node_instruction_185(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_184(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 184: Terminal matched constructor ID 90");
    90
}

fn match_node_instruction_185(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 185: Terminal matched constructor ID 70");
    70
}

fn match_node_instruction_186(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 186: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_187(bytes, ctx),
        1 => match_node_instruction_188(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_187(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 187: Terminal matched constructor ID 37");
    37
}

fn match_node_instruction_188(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 188: Terminal matched constructor ID 38");
    38
}

fn match_node_instruction_189(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 3;
    eprintln!("Trace node 189: SlaInstructionBits start=4, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_190(bytes, ctx),
        1 => match_node_instruction_191(bytes, ctx),
        2 => match_node_instruction_192(bytes, ctx),
        3 => match_node_instruction_193(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_190(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 190: Terminal matched constructor ID 7");
    7
}

fn match_node_instruction_191(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 191: Terminal matched constructor ID 1");
    1
}

fn match_node_instruction_192(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 192: Terminal matched constructor ID 95");
    95
}

fn match_node_instruction_193(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 193: Terminal matched constructor ID 51");
    51
}

fn match_node_instruction_194(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 3;
    eprintln!("Trace node 194: SlaInstructionBits start=4, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_195(bytes, ctx),
        1 => match_node_instruction_196(bytes, ctx),
        2 => match_node_instruction_197(bytes, ctx),
        3 => match_node_instruction_198(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_195(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 195: Terminal matched constructor ID 9");
    9
}

fn match_node_instruction_196(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 196: Terminal matched constructor ID 3");
    3
}

fn match_node_instruction_197(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 197: Terminal matched constructor ID 97");
    97
}

fn match_node_instruction_198(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 198: Terminal matched constructor ID 53");
    53
}

fn match_node_instruction_199(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 3;
    eprintln!("Trace node 199: SlaInstructionBits start=4, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_200(bytes, ctx),
        1 => match_node_instruction_201(bytes, ctx),
        2 => match_node_instruction_202(bytes, ctx),
        3 => match_node_instruction_203(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_200(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 200: Terminal matched constructor ID 6");
    6
}

fn match_node_instruction_201(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 201: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_202(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 202: Terminal matched constructor ID 94");
    94
}

fn match_node_instruction_203(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 203: Terminal matched constructor ID 50");
    50
}

fn match_node_instruction_204(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 3;
    eprintln!("Trace node 204: SlaInstructionBits start=4, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_205(bytes, ctx),
        1 => match_node_instruction_206(bytes, ctx),
        2 => match_node_instruction_207(bytes, ctx),
        3 => match_node_instruction_208(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_205(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 205: Terminal matched constructor ID 8");
    8
}

fn match_node_instruction_206(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 206: Terminal matched constructor ID 2");
    2
}

fn match_node_instruction_207(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 207: Terminal matched constructor ID 96");
    96
}

fn match_node_instruction_208(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 208: Terminal matched constructor ID 52");
    52
}

fn match_node_instruction_209(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 3;
    eprintln!("Trace node 209: SlaInstructionBits start=4, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_210(bytes, ctx),
        1 => match_node_instruction_211(bytes, ctx),
        2 => match_node_instruction_212(bytes, ctx),
        3 => match_node_instruction_213(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_210(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 210: Terminal matched constructor ID 60");
    60
}

fn match_node_instruction_211(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 211: Terminal matched constructor ID 102");
    102
}

fn match_node_instruction_212(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 212: Terminal matched constructor ID 17");
    17
}

fn match_node_instruction_213(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 213: Terminal matched constructor ID 25");
    25
}

fn match_node_instruction_214(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 3;
    eprintln!("Trace node 214: SlaInstructionBits start=4, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_215(bytes, ctx),
        1 => match_node_instruction_216(bytes, ctx),
        2 => match_node_instruction_217(bytes, ctx),
        3 => match_node_instruction_218(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_215(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 215: Terminal matched constructor ID 61");
    61
}

fn match_node_instruction_216(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 216: Terminal matched constructor ID 109");
    109
}

fn match_node_instruction_217(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 217: Terminal matched constructor ID 18");
    18
}

fn match_node_instruction_218(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 218: Terminal matched constructor ID 26");
    26
}

fn match_node_instruction_219(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 3;
    eprintln!("Trace node 219: SlaInstructionBits start=4, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_220(bytes, ctx),
        1 => match_node_instruction_221(bytes, ctx),
        2 => match_node_instruction_222(bytes, ctx),
        3 => match_node_instruction_223(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_220(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 220: Terminal matched constructor ID 41");
    41
}

fn match_node_instruction_221(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 221: Terminal matched constructor ID 4");
    4
}

fn match_node_instruction_222(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 222: Terminal matched constructor ID 98");
    98
}

fn match_node_instruction_223(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 223: Terminal matched constructor ID 44");
    44
}

fn match_node_instruction_224(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 3;
    eprintln!("Trace node 224: SlaInstructionBits start=4, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_225(bytes, ctx),
        1 => match_node_instruction_226(bytes, ctx),
        2 => match_node_instruction_227(bytes, ctx),
        3 => match_node_instruction_228(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_225(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 225: Terminal matched constructor ID 42");
    42
}

fn match_node_instruction_226(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 226: Terminal matched constructor ID 5");
    5
}

fn match_node_instruction_227(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 227: Terminal matched constructor ID 99");
    99
}

fn match_node_instruction_228(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 228: Terminal matched constructor ID 43");
    43
}

fn match_node_instruction_229(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 3;
    eprintln!("Trace node 229: SlaInstructionBits start=4, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_230(bytes, ctx),
        1 => match_node_instruction_231(bytes, ctx),
        2 => match_node_instruction_232(bytes, ctx),
        3 => match_node_instruction_233(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_230(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 230: Terminal matched constructor ID 10");
    10
}

fn match_node_instruction_231(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 231: Terminal matched constructor ID 19");
    19
}

fn match_node_instruction_232(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 232: Terminal matched constructor ID 65");
    65
}

fn match_node_instruction_233(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 233: Terminal matched constructor ID 11");
    11
}

fn match_node_instruction_234(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 234: Terminal matched constructor ID 39");
    39
}

fn match_node_instruction_235(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 15;
    eprintln!("Trace node 235: SlaInstructionBits start=4, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_236(bytes, ctx),
        1 => match_node_instruction_237(bytes, ctx),
        2 => match_node_instruction_238(bytes, ctx),
        3 => match_node_instruction_239(bytes, ctx),
        4 => match_node_instruction_240(bytes, ctx),
        5 => match_node_instruction_241(bytes, ctx),
        6 => match_node_instruction_242(bytes, ctx),
        7 => match_node_instruction_243(bytes, ctx),
        8 => match_node_instruction_244(bytes, ctx),
        9 => match_node_instruction_245(bytes, ctx),
        10 => match_node_instruction_246(bytes, ctx),
        11 => match_node_instruction_247(bytes, ctx),
        12 => match_node_instruction_248(bytes, ctx),
        13 => match_node_instruction_249(bytes, ctx),
        14 => match_node_instruction_250(bytes, ctx),
        15 => match_node_instruction_251(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_236(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 236: Terminal matched constructor ID 27");
    27
}

fn match_node_instruction_237(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 237: Terminal matched constructor ID 28");
    28
}

fn match_node_instruction_238(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 238: Terminal matched constructor ID 100");
    100
}

fn match_node_instruction_239(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 239: Terminal matched constructor ID 12");
    12
}

fn match_node_instruction_240(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 240: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_241(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 241: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_242(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 242: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_243(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 243: Terminal matched constructor ID 45");
    45
}

fn match_node_instruction_244(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 244: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_245(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 245: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_246(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 246: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_247(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 247: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_248(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 248: Terminal matched constructor ID 29");
    29
}

fn match_node_instruction_249(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 249: Terminal matched constructor ID 31");
    31
}

fn match_node_instruction_250(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 250: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_251(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 251: Terminal matched constructor ID 40");
    40
}

fn match_node_instruction_252(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 15;
    eprintln!("Trace node 252: SlaInstructionBits start=4, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_253(bytes, ctx),
        1 => match_node_instruction_254(bytes, ctx),
        2 => match_node_instruction_255(bytes, ctx),
        3 => match_node_instruction_256(bytes, ctx),
        4 => match_node_instruction_257(bytes, ctx),
        5 => match_node_instruction_258(bytes, ctx),
        6 => match_node_instruction_259(bytes, ctx),
        7 => match_node_instruction_260(bytes, ctx),
        8 => match_node_instruction_261(bytes, ctx),
        9 => match_node_instruction_262(bytes, ctx),
        10 => match_node_instruction_263(bytes, ctx),
        11 => match_node_instruction_264(bytes, ctx),
        12 => match_node_instruction_265(bytes, ctx),
        13 => match_node_instruction_266(bytes, ctx),
        14 => match_node_instruction_267(bytes, ctx),
        15 => match_node_instruction_276(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_253(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 253: Terminal matched constructor ID 68");
    68
}

fn match_node_instruction_254(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 254: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_255(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 255: Terminal matched constructor ID 67");
    67
}

fn match_node_instruction_256(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 256: Terminal matched constructor ID 64");
    64
}

fn match_node_instruction_257(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 257: Terminal matched constructor ID 66");
    66
}

fn match_node_instruction_258(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 258: Terminal matched constructor ID 63");
    63
}

fn match_node_instruction_259(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 259: Terminal matched constructor ID 36");
    36
}

fn match_node_instruction_260(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 260: Terminal matched constructor ID 101");
    101
}

fn match_node_instruction_261(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 261: Terminal matched constructor ID 15");
    15
}

fn match_node_instruction_262(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 262: Terminal matched constructor ID 71");
    71
}

fn match_node_instruction_263(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 263: Terminal matched constructor ID 22");
    22
}

fn match_node_instruction_264(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 264: Terminal matched constructor ID 30");
    30
}

fn match_node_instruction_265(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 265: Terminal matched constructor ID 16");
    16
}

fn match_node_instruction_266(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 266: Terminal matched constructor ID 56");
    56
}

fn match_node_instruction_267(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 7;
    eprintln!("Trace node 267: SlaInstructionBits start=9, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_268(bytes, ctx),
        1 => match_node_instruction_269(bytes, ctx),
        2 => match_node_instruction_270(bytes, ctx),
        3 => match_node_instruction_271(bytes, ctx),
        4 => match_node_instruction_272(bytes, ctx),
        5 => match_node_instruction_273(bytes, ctx),
        6 => match_node_instruction_274(bytes, ctx),
        7 => match_node_instruction_275(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_268(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 268: Terminal matched constructor ID 23");
    23
}

fn match_node_instruction_269(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 269: Terminal matched constructor ID 24");
    24
}

fn match_node_instruction_270(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 270: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_271(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 271: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_272(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 272: Terminal matched constructor ID 47");
    47
}

fn match_node_instruction_273(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 273: Terminal matched constructor ID 49");
    49
}

fn match_node_instruction_274(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 274: Terminal matched constructor ID 46");
    46
}

fn match_node_instruction_275(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 275: Terminal matched constructor ID 48");
    48
}

fn match_node_instruction_276(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 276: Terminal matched constructor ID 69");
    69
}

fn match_node_iwreg_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_jmpdest_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_jmpdest11_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_jmpdest16_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_oper16_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 3;
    eprintln!("Trace node 0: SlaInstructionBits start=6, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_oper16_1(bytes, ctx),
        1 => match_node_oper16_2(bytes, ctx),
        2 => match_node_oper16_3(bytes, ctx),
        3 => match_node_oper16_6(bytes, ctx),
        _ => -1,
    }
}

fn match_node_oper16_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_oper16_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_oper16_3(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 3: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_oper16_4(bytes, ctx),
        1 => match_node_oper16_5(bytes, ctx),
        _ => -1,
    }
}

fn match_node_oper16_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 2");
    2
}

fn match_node_oper16_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 3");
    3
}

fn match_node_oper16_6(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 6: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_oper16_7(bytes, ctx),
        1 => match_node_oper16_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_oper16_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 4");
    4
}

fn match_node_oper16_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 5");
    5
}

fn match_node_oper8_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 3;
    eprintln!("Trace node 0: SlaInstructionBits start=6, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_oper8_1(bytes, ctx),
        1 => match_node_oper8_2(bytes, ctx),
        2 => match_node_oper8_3(bytes, ctx),
        3 => match_node_oper8_6(bytes, ctx),
        _ => -1,
    }
}

fn match_node_oper8_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_oper8_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_oper8_3(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 3: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_oper8_4(bytes, ctx),
        1 => match_node_oper8_5(bytes, ctx),
        _ => -1,
    }
}

fn match_node_oper8_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 2");
    2
}

fn match_node_oper8_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 3");
    3
}

fn match_node_oper8_6(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 6: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_oper8_7(bytes, ctx),
        1 => match_node_oper8_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_oper8_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 4");
    4
}

fn match_node_oper8_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 5");
    5
}

fn match_node_simmed8_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_waop16_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_wreg_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

