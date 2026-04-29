// Auto-generated Fission Native Backend
#[no_mangle]
pub extern "C" fn fission_decode_match(table_ptr: *const i8, bytes: *const u8, bytes_len: usize, ctx_ptr: *const u64) -> i32 {
    let table_name = unsafe { std::ffi::CStr::from_ptr(table_ptr).to_str().unwrap() };
    let bytes = unsafe { std::slice::from_raw_parts(bytes, bytes_len) };
    let ctx = unsafe { *ctx_ptr };
    match table_name {
        "branchdest16" => match_node_branchdest16_0(bytes, ctx),
        "cc" => match_node_cc_0(bytes, ctx),
        "checkbranch" => match_node_checkbranch_0(bytes, ctx),
        "impliedval16" => match_node_impliedval16_0(bytes, ctx),
        "instruction" => match_node_instruction_0(bytes, ctx),
        "jmpdest16" => match_node_jmpdest16_0(bytes, ctx),
        "regval0_2" => match_node_regval0_2_0(bytes, ctx),
        "splitimm16" => match_node_splitimm16_0(bytes, ctx),
        "with" => match_node_with_0(bytes, ctx),
        _ => -1
    }
}

fn match_node_branchdest16_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_branchdest16_1(bytes, ctx),
        1 => match_node_branchdest16_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_branchdest16_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_branchdest16_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_cc_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 15;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=4, word={:08x}, probe={}", word, probe);
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
    eprintln!("Trace node 9: Terminal matched NOTHING");
    -1
}

fn match_node_cc_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 8");
    8
}

fn match_node_cc_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 9");
    9
}

fn match_node_cc_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 10");
    10
}

fn match_node_cc_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 11");
    11
}

fn match_node_cc_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 12");
    12
}

fn match_node_cc_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 13");
    13
}

fn match_node_cc_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 14");
    14
}

fn match_node_checkbranch_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_impliedval16_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 7;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_impliedval16_1(bytes, ctx),
        1 => match_node_impliedval16_4(bytes, ctx),
        2 => match_node_impliedval16_7(bytes, ctx),
        3 => match_node_impliedval16_10(bytes, ctx),
        4 => match_node_impliedval16_13(bytes, ctx),
        5 => match_node_impliedval16_16(bytes, ctx),
        6 => match_node_impliedval16_19(bytes, ctx),
        7 => match_node_impliedval16_22(bytes, ctx),
        _ => -1,
    }
}

fn match_node_impliedval16_1(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 1: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_impliedval16_2(bytes, ctx),
        1 => match_node_impliedval16_3(bytes, ctx),
        _ => -1,
    }
}

fn match_node_impliedval16_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_impliedval16_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 5");
    5
}

fn match_node_impliedval16_4(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 4: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_impliedval16_5(bytes, ctx),
        1 => match_node_impliedval16_6(bytes, ctx),
        _ => -1,
    }
}

fn match_node_impliedval16_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 0");
    0
}

fn match_node_impliedval16_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_impliedval16_7(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 7: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_impliedval16_8(bytes, ctx),
        1 => match_node_impliedval16_9(bytes, ctx),
        _ => -1,
    }
}

fn match_node_impliedval16_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 0");
    0
}

fn match_node_impliedval16_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 5");
    5
}

fn match_node_impliedval16_10(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 10: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_impliedval16_11(bytes, ctx),
        1 => match_node_impliedval16_12(bytes, ctx),
        _ => -1,
    }
}

fn match_node_impliedval16_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 0");
    0
}

fn match_node_impliedval16_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 5");
    5
}

fn match_node_impliedval16_13(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 13: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_impliedval16_14(bytes, ctx),
        1 => match_node_impliedval16_15(bytes, ctx),
        _ => -1,
    }
}

fn match_node_impliedval16_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 1");
    1
}

fn match_node_impliedval16_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 3");
    3
}

fn match_node_impliedval16_16(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 16: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_impliedval16_17(bytes, ctx),
        1 => match_node_impliedval16_18(bytes, ctx),
        _ => -1,
    }
}

fn match_node_impliedval16_17(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 17: Terminal matched constructor ID 1");
    1
}

fn match_node_impliedval16_18(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 18: Terminal matched constructor ID 3");
    3
}

fn match_node_impliedval16_19(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 19: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_impliedval16_20(bytes, ctx),
        1 => match_node_impliedval16_21(bytes, ctx),
        _ => -1,
    }
}

fn match_node_impliedval16_20(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 20: Terminal matched constructor ID 2");
    2
}

fn match_node_impliedval16_21(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 21: Terminal matched constructor ID 4");
    4
}

fn match_node_impliedval16_22(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 22: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_impliedval16_23(bytes, ctx),
        1 => match_node_impliedval16_24(bytes, ctx),
        _ => -1,
    }
}

fn match_node_impliedval16_23(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 23: Terminal matched constructor ID 0");
    0
}

fn match_node_impliedval16_24(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 24: Terminal matched constructor ID 5");
    5
}

fn match_node_instruction_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 15;
    eprintln!("Trace node 0: SlaInstructionBits start=6, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1(bytes, ctx),
        1 => match_node_instruction_26(bytes, ctx),
        2 => match_node_instruction_35(bytes, ctx),
        3 => match_node_instruction_44(bytes, ctx),
        4 => match_node_instruction_45(bytes, ctx),
        5 => match_node_instruction_46(bytes, ctx),
        6 => match_node_instruction_47(bytes, ctx),
        7 => match_node_instruction_48(bytes, ctx),
        8 => match_node_instruction_57(bytes, ctx),
        9 => match_node_instruction_60(bytes, ctx),
        10 => match_node_instruction_69(bytes, ctx),
        11 => match_node_instruction_74(bytes, ctx),
        12 => match_node_instruction_77(bytes, ctx),
        13 => match_node_instruction_80(bytes, ctx),
        14 => match_node_instruction_83(bytes, ctx),
        15 => match_node_instruction_86(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 7;
    eprintln!("Trace node 1: SlaInstructionBits start=10, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_2(bytes, ctx),
        1 => match_node_instruction_15(bytes, ctx),
        2 => match_node_instruction_16(bytes, ctx),
        3 => match_node_instruction_17(bytes, ctx),
        4 => match_node_instruction_18(bytes, ctx),
        5 => match_node_instruction_19(bytes, ctx),
        6 => match_node_instruction_20(bytes, ctx),
        7 => match_node_instruction_25(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_2(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 7;
    eprintln!("Trace node 2: SlaInstructionBits start=13, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_3(bytes, ctx),
        1 => match_node_instruction_4(bytes, ctx),
        2 => match_node_instruction_5(bytes, ctx),
        3 => match_node_instruction_6(bytes, ctx),
        4 => match_node_instruction_7(bytes, ctx),
        5 => match_node_instruction_12(bytes, ctx),
        6 => match_node_instruction_13(bytes, ctx),
        7 => match_node_instruction_14(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 19");
    19
}

fn match_node_instruction_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 41");
    41
}

fn match_node_instruction_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 17");
    17
}

fn match_node_instruction_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 16");
    16
}

fn match_node_instruction_7(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (30 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 30) & 3;
    eprintln!("Trace node 7: SlaInstructionBits start=30, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_8(bytes, ctx),
        1 => match_node_instruction_9(bytes, ctx),
        2 => match_node_instruction_10(bytes, ctx),
        3 => match_node_instruction_11(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 21");
    21
}

fn match_node_instruction_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 23");
    23
}

fn match_node_instruction_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 22");
    22
}

fn match_node_instruction_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 47");
    47
}

fn match_node_instruction_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 9");
    9
}

fn match_node_instruction_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 42");
    42
}

fn match_node_instruction_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 20");
    20
}

fn match_node_instruction_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 15");
    15
}

fn match_node_instruction_17(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 17: Terminal matched constructor ID 14");
    14
}

fn match_node_instruction_18(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 18: Terminal matched constructor ID 35");
    35
}

fn match_node_instruction_19(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 19: Terminal matched constructor ID 2");
    2
}

fn match_node_instruction_20(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 20: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_21(bytes, ctx),
        1 => match_node_instruction_22(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_21(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 21: Terminal matched constructor ID 18");
    18
}

fn match_node_instruction_22(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 22: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_23(bytes, ctx),
        1 => match_node_instruction_24(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_23(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 23: Terminal matched constructor ID 36");
    36
}

fn match_node_instruction_24(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 24: Terminal matched constructor ID 43");
    43
}

fn match_node_instruction_25(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 25: Terminal matched constructor ID 40");
    40
}

fn match_node_instruction_26(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 7;
    eprintln!("Trace node 26: SlaInstructionBits start=10, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_27(bytes, ctx),
        1 => match_node_instruction_28(bytes, ctx),
        2 => match_node_instruction_29(bytes, ctx),
        3 => match_node_instruction_30(bytes, ctx),
        4 => match_node_instruction_31(bytes, ctx),
        5 => match_node_instruction_32(bytes, ctx),
        6 => match_node_instruction_33(bytes, ctx),
        7 => match_node_instruction_34(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_27(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 27: Terminal matched constructor ID 67");
    67
}

fn match_node_instruction_28(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 28: Terminal matched constructor ID 64");
    64
}

fn match_node_instruction_29(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 29: Terminal matched constructor ID 60");
    60
}

fn match_node_instruction_30(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 30: Terminal matched constructor ID 65");
    65
}

fn match_node_instruction_31(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 31: Terminal matched constructor ID 66");
    66
}

fn match_node_instruction_32(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 32: Terminal matched constructor ID 62");
    62
}

fn match_node_instruction_33(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 33: Terminal matched constructor ID 61");
    61
}

fn match_node_instruction_34(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 34: Terminal matched constructor ID 63");
    63
}

fn match_node_instruction_35(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 7;
    eprintln!("Trace node 35: SlaInstructionBits start=13, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_36(bytes, ctx),
        1 => match_node_instruction_37(bytes, ctx),
        2 => match_node_instruction_38(bytes, ctx),
        3 => match_node_instruction_39(bytes, ctx),
        4 => match_node_instruction_40(bytes, ctx),
        5 => match_node_instruction_41(bytes, ctx),
        6 => match_node_instruction_42(bytes, ctx),
        7 => match_node_instruction_43(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_36(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 36: Terminal matched constructor ID 48");
    48
}

fn match_node_instruction_37(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 37: Terminal matched constructor ID 48");
    48
}

fn match_node_instruction_38(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 38: Terminal matched constructor ID 48");
    48
}

fn match_node_instruction_39(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 39: Terminal matched constructor ID 48");
    48
}

fn match_node_instruction_40(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 40: Terminal matched constructor ID 48");
    48
}

fn match_node_instruction_41(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 41: Terminal matched constructor ID 48");
    48
}

fn match_node_instruction_42(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 42: Terminal matched constructor ID 48");
    48
}

fn match_node_instruction_43(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 43: Terminal matched constructor ID 48");
    48
}

fn match_node_instruction_44(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 44: Terminal matched constructor ID 3");
    3
}

fn match_node_instruction_45(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 45: Terminal matched constructor ID 46");
    46
}

fn match_node_instruction_46(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 46: Terminal matched constructor ID 13");
    13
}

fn match_node_instruction_47(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 47: Terminal matched constructor ID 6");
    6
}

fn match_node_instruction_48(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 7;
    eprintln!("Trace node 48: SlaInstructionBits start=10, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_49(bytes, ctx),
        1 => match_node_instruction_50(bytes, ctx),
        2 => match_node_instruction_51(bytes, ctx),
        3 => match_node_instruction_52(bytes, ctx),
        4 => match_node_instruction_53(bytes, ctx),
        5 => match_node_instruction_54(bytes, ctx),
        6 => match_node_instruction_55(bytes, ctx),
        7 => match_node_instruction_56(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_49(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 49: Terminal matched constructor ID 10");
    10
}

fn match_node_instruction_50(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 50: Terminal matched constructor ID 10");
    10
}

fn match_node_instruction_51(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 51: Terminal matched constructor ID 10");
    10
}

fn match_node_instruction_52(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 52: Terminal matched constructor ID 10");
    10
}

fn match_node_instruction_53(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 53: Terminal matched constructor ID 10");
    10
}

fn match_node_instruction_54(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 54: Terminal matched constructor ID 10");
    10
}

fn match_node_instruction_55(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 55: Terminal matched constructor ID 10");
    10
}

fn match_node_instruction_56(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 56: Terminal matched constructor ID 10");
    10
}

fn match_node_instruction_57(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 57: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_58(bytes, ctx),
        1 => match_node_instruction_59(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_58(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 58: Terminal matched constructor ID 37");
    37
}

fn match_node_instruction_59(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 59: Terminal matched constructor ID 8");
    8
}

fn match_node_instruction_60(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 7;
    eprintln!("Trace node 60: SlaInstructionBits start=10, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_61(bytes, ctx),
        1 => match_node_instruction_62(bytes, ctx),
        2 => match_node_instruction_63(bytes, ctx),
        3 => match_node_instruction_64(bytes, ctx),
        4 => match_node_instruction_65(bytes, ctx),
        5 => match_node_instruction_66(bytes, ctx),
        6 => match_node_instruction_67(bytes, ctx),
        7 => match_node_instruction_68(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_61(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 61: Terminal matched constructor ID 31");
    31
}

fn match_node_instruction_62(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 62: Terminal matched constructor ID 33");
    33
}

fn match_node_instruction_63(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 63: Terminal matched constructor ID 33");
    33
}

fn match_node_instruction_64(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 64: Terminal matched constructor ID 33");
    33
}

fn match_node_instruction_65(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 65: Terminal matched constructor ID 32");
    32
}

fn match_node_instruction_66(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 66: Terminal matched constructor ID 32");
    32
}

fn match_node_instruction_67(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 67: Terminal matched constructor ID 38");
    38
}

fn match_node_instruction_68(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 68: Terminal matched constructor ID 34");
    34
}

fn match_node_instruction_69(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 3;
    eprintln!("Trace node 69: SlaInstructionBits start=11, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_70(bytes, ctx),
        1 => match_node_instruction_71(bytes, ctx),
        2 => match_node_instruction_72(bytes, ctx),
        3 => match_node_instruction_73(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_70(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 70: Terminal matched constructor ID 29");
    29
}

fn match_node_instruction_71(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 71: Terminal matched constructor ID 30");
    30
}

fn match_node_instruction_72(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 72: Terminal matched constructor ID 39");
    39
}

fn match_node_instruction_73(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 73: Terminal matched constructor ID 77");
    77
}

fn match_node_instruction_74(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 74: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_75(bytes, ctx),
        1 => match_node_instruction_76(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_75(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 75: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_76(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 76: Terminal matched constructor ID 74");
    74
}

fn match_node_instruction_77(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 77: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_78(bytes, ctx),
        1 => match_node_instruction_79(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_78(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 78: Terminal matched constructor ID 44");
    44
}

fn match_node_instruction_79(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 79: Terminal matched constructor ID 78");
    78
}

fn match_node_instruction_80(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 80: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_81(bytes, ctx),
        1 => match_node_instruction_82(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_81(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 81: Terminal matched constructor ID 11");
    11
}

fn match_node_instruction_82(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 82: Terminal matched constructor ID 76");
    76
}

fn match_node_instruction_83(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 83: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_84(bytes, ctx),
        1 => match_node_instruction_85(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_84(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 84: Terminal matched constructor ID 4");
    4
}

fn match_node_instruction_85(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 85: Terminal matched constructor ID 75");
    75
}

fn match_node_instruction_86(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 86: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_87(bytes, ctx),
        1 => match_node_instruction_88(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_87(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 87: Terminal matched constructor ID 49");
    49
}

fn match_node_instruction_88(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 88: Terminal matched constructor ID 79");
    79
}

fn match_node_jmpdest16_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_regval0_2_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_splitimm16_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_with_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 0: InstructionBitSlice offset=0, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_1(bytes, ctx),
        1 => match_node_with_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_1(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 16) >> 4;
    eprintln!("Trace node 1: InstructionBitSlice offset=0, mask=16, probe={}", probe);
    match probe {
        0 => match_node_with_2(bytes, ctx),
        1 => match_node_with_5(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_2(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 32) >> 5;
    eprintln!("Trace node 2: InstructionBitSlice offset=0, mask=32, probe={}", probe);
    match probe {
        0 => match_node_with_3(bytes, ctx),
        1 => match_node_with_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 7");
    7
}

fn match_node_with_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 6");
    6
}

fn match_node_with_5(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 32) >> 5;
    eprintln!("Trace node 5: InstructionBitSlice offset=0, mask=32, probe={}", probe);
    match probe {
        0 => match_node_with_6(bytes, ctx),
        1 => match_node_with_7(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 0");
    0
}

fn match_node_with_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 1");
    1
}

fn match_node_with_8(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 16) >> 4;
    eprintln!("Trace node 8: InstructionBitSlice offset=0, mask=16, probe={}", probe);
    match probe {
        0 => match_node_with_9(bytes, ctx),
        1 => match_node_with_12(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_9(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 32) >> 5;
    eprintln!("Trace node 9: InstructionBitSlice offset=0, mask=32, probe={}", probe);
    match probe {
        0 => match_node_with_10(bytes, ctx),
        1 => match_node_with_11(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 4");
    4
}

fn match_node_with_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 2");
    2
}

fn match_node_with_12(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 32) >> 5;
    eprintln!("Trace node 12: InstructionBitSlice offset=0, mask=32, probe={}", probe);
    match probe {
        0 => match_node_with_13(bytes, ctx),
        1 => match_node_with_14(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 5");
    5
}

fn match_node_with_14(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 64) >> 6;
    eprintln!("Trace node 14: InstructionBitSlice offset=0, mask=64, probe={}", probe);
    match probe {
        0 => match_node_with_15(bytes, ctx),
        1 => match_node_with_20(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_15(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 128) >> 7;
    eprintln!("Trace node 15: InstructionBitSlice offset=0, mask=128, probe={}", probe);
    match probe {
        0 => match_node_with_16(bytes, ctx),
        1 => match_node_with_17(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 12");
    12
}

fn match_node_with_17(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 17: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_18(bytes, ctx),
        1 => match_node_with_19(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_18(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 18: Terminal matched constructor ID 11");
    11
}

fn match_node_with_19(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 19: Terminal matched constructor ID 9");
    9
}

fn match_node_with_20(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 128) >> 7;
    eprintln!("Trace node 20: InstructionBitSlice offset=0, mask=128, probe={}", probe);
    match probe {
        0 => match_node_with_21(bytes, ctx),
        1 => match_node_with_24(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_21(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 21: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_22(bytes, ctx),
        1 => match_node_with_23(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_22(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 22: Terminal matched constructor ID 3");
    3
}

fn match_node_with_23(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 23: Terminal matched constructor ID 10");
    10
}

fn match_node_with_24(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 24: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_25(bytes, ctx),
        1 => match_node_with_26(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_25(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 25: Terminal matched constructor ID 8");
    8
}

fn match_node_with_26(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 26: Terminal matched constructor ID 13");
    13
}

