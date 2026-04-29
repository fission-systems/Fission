// Auto-generated Fission Native Backend
#[no_mangle]
pub extern "C" fn fission_decode_match(table_ptr: *const i8, bytes: *const u8, bytes_len: usize, ctx_ptr: *const u64) -> i32 {
    let table_name = unsafe { std::ffi::CStr::from_ptr(table_ptr).to_str().unwrap() };
    let bytes = unsafe { std::slice::from_raw_parts(bytes, bytes_len) };
    let ctx = unsafe { *ctx_ptr };
    match table_name {
        "COND" => match_node_COND_0(bytes, ctx),
        "COND2" => match_node_COND2_0(bytes, ctx),
        "abs20" => match_node_abs20_0(bytes, ctx),
        "abs24" => match_node_abs24_0(bytes, ctx),
        "cc" => match_node_cc_0(bytes, ctx),
        "cc2" => match_node_cc2_0(bytes, ctx),
        "cinv1" => match_node_cinv1_0(bytes, ctx),
        "cinv2" => match_node_cinv2_0(bytes, ctx),
        "cinv3" => match_node_cinv3_0(bytes, ctx),
        "cinv4" => match_node_cinv4_0(bytes, ctx),
        "cinv5" => match_node_cinv5_0(bytes, ctx),
        "cinv6" => match_node_cinv6_0(bytes, ctx),
        "cnt3" => match_node_cnt3_0(bytes, ctx),
        "cnt3b" => match_node_cnt3b_0(bytes, ctx),
        "const1" => match_node_const1_0(bytes, ctx),
        "disp14" => match_node_disp14_0(bytes, ctx),
        "disp16" => match_node_disp16_0(bytes, ctx),
        "disp16b" => match_node_disp16b_0(bytes, ctx),
        "disp20" => match_node_disp20_0(bytes, ctx),
        "disp24" => match_node_disp24_0(bytes, ctx),
        "disp24b" => match_node_disp24b_0(bytes, ctx),
        "disp4" => match_node_disp4_0(bytes, ctx),
        "disp4b" => match_node_disp4b_0(bytes, ctx),
        "disp4c" => match_node_disp4c_0(bytes, ctx),
        "disp8" => match_node_disp8_0(bytes, ctx),
        "imm16" => match_node_imm16_0(bytes, ctx),
        "imm20" => match_node_imm20_0(bytes, ctx),
        "imm3" => match_node_imm3_0(bytes, ctx),
        "imm32" => match_node_imm32_0(bytes, ctx),
        "imm3b" => match_node_imm3b_0(bytes, ctx),
        "imm3c" => match_node_imm3c_0(bytes, ctx),
        "imm3d" => match_node_imm3d_0(bytes, ctx),
        "imm4" => match_node_imm4_0(bytes, ctx),
        "imm4a" => match_node_imm4a_0(bytes, ctx),
        "imm4b" => match_node_imm4b_0(bytes, ctx),
        "imm4c" => match_node_imm4c_0(bytes, ctx),
        "imm4d" => match_node_imm4d_0(bytes, ctx),
        "imm5" => match_node_imm5_0(bytes, ctx),
        "instruction" => match_node_instruction_0(bytes, ctx),
        "pop_args" => match_node_pop_args_0(bytes, ctx),
        "pop_ret" => match_node_pop_ret_0(bytes, ctx),
        "prp" => match_node_prp_0(bytes, ctx),
        "prp2" => match_node_prp2_0(bytes, ctx),
        "push_args" => match_node_push_args_0(bytes, ctx),
        "ra" => match_node_ra_0(bytes, ctx),
        "rs" => match_node_rs_0(bytes, ctx),
        _ => -1
    }
}

fn match_node_COND_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_COND2_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_abs20_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_abs24_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_cc_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 15;
    eprintln!("Trace node 0: SlaInstructionBits start=0, size=4, word={:08x}, probe={}", word, probe);
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
    eprintln!("Trace node 15: Terminal matched NOTHING");
    -1
}

fn match_node_cc_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched NOTHING");
    -1
}

fn match_node_cc2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 15;
    eprintln!("Trace node 0: SlaInstructionBits start=0, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_cc2_1(bytes, ctx),
        1 => match_node_cc2_2(bytes, ctx),
        2 => match_node_cc2_3(bytes, ctx),
        3 => match_node_cc2_4(bytes, ctx),
        4 => match_node_cc2_5(bytes, ctx),
        5 => match_node_cc2_6(bytes, ctx),
        6 => match_node_cc2_7(bytes, ctx),
        7 => match_node_cc2_8(bytes, ctx),
        8 => match_node_cc2_9(bytes, ctx),
        9 => match_node_cc2_10(bytes, ctx),
        10 => match_node_cc2_11(bytes, ctx),
        11 => match_node_cc2_12(bytes, ctx),
        12 => match_node_cc2_13(bytes, ctx),
        13 => match_node_cc2_14(bytes, ctx),
        14 => match_node_cc2_15(bytes, ctx),
        15 => match_node_cc2_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_cc2_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_cc2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_cc2_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_cc2_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_cc2_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 4");
    4
}

fn match_node_cc2_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_cc2_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 6");
    6
}

fn match_node_cc2_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 7");
    7
}

fn match_node_cc2_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 8");
    8
}

fn match_node_cc2_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 9");
    9
}

fn match_node_cc2_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 10");
    10
}

fn match_node_cc2_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 11");
    11
}

fn match_node_cc2_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 12");
    12
}

fn match_node_cc2_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 13");
    13
}

fn match_node_cc2_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched NOTHING");
    -1
}

fn match_node_cc2_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched NOTHING");
    -1
}

fn match_node_cinv1_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_cinv2_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_cinv3_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_cinv4_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_cinv5_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_cinv6_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_cnt3_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_cnt3b_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_const1_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_disp14_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_disp16_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_disp16b_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_disp20_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_disp24_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_disp24b_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_disp4_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_disp4b_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_disp4c_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_disp8_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_imm16_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_imm20_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_imm3_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_imm32_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_imm3b_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_imm3c_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_imm3d_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_imm4_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 1");
    1
}

fn match_node_imm4a_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_imm4b_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_imm4c_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_imm4d_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_imm5_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 15;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1(bytes, ctx),
        1 => match_node_instruction_256(bytes, ctx),
        2 => match_node_instruction_259(bytes, ctx),
        3 => match_node_instruction_278(bytes, ctx),
        4 => match_node_instruction_295(bytes, ctx),
        5 => match_node_instruction_316(bytes, ctx),
        6 => match_node_instruction_333(bytes, ctx),
        7 => match_node_instruction_362(bytes, ctx),
        8 => match_node_instruction_395(bytes, ctx),
        9 => match_node_instruction_416(bytes, ctx),
        10 => match_node_instruction_419(bytes, ctx),
        11 => match_node_instruction_422(bytes, ctx),
        12 => match_node_instruction_425(bytes, ctx),
        13 => match_node_instruction_446(bytes, ctx),
        14 => match_node_instruction_449(bytes, ctx),
        15 => match_node_instruction_452(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 7;
    eprintln!("Trace node 1: SlaInstructionBits start=12, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_2(bytes, ctx),
        1 => match_node_instruction_223(bytes, ctx),
        2 => match_node_instruction_232(bytes, ctx),
        3 => match_node_instruction_235(bytes, ctx),
        4 => match_node_instruction_238(bytes, ctx),
        5 => match_node_instruction_245(bytes, ctx),
        6 => match_node_instruction_250(bytes, ctx),
        7 => match_node_instruction_253(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_2(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 3;
    eprintln!("Trace node 2: SlaInstructionBits start=1, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_3(bytes, ctx),
        1 => match_node_instruction_160(bytes, ctx),
        2 => match_node_instruction_203(bytes, ctx),
        3 => match_node_instruction_214(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_3(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 1;
    eprintln!("Trace node 3: SlaInstructionBits start=3, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_4(bytes, ctx),
        1 => match_node_instruction_25(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_4(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 4: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_5(bytes, ctx),
        1 => match_node_instruction_24(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_5(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 1;
    eprintln!("Trace node 5: SlaInstructionBits start=0, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_6(bytes, ctx),
        1 => match_node_instruction_23(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_6(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 15;
    eprintln!("Trace node 6: SlaInstructionBits start=4, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_7(bytes, ctx),
        1 => match_node_instruction_8(bytes, ctx),
        2 => match_node_instruction_9(bytes, ctx),
        3 => match_node_instruction_10(bytes, ctx),
        4 => match_node_instruction_11(bytes, ctx),
        5 => match_node_instruction_12(bytes, ctx),
        6 => match_node_instruction_13(bytes, ctx),
        7 => match_node_instruction_14(bytes, ctx),
        8 => match_node_instruction_15(bytes, ctx),
        9 => match_node_instruction_16(bytes, ctx),
        10 => match_node_instruction_17(bytes, ctx),
        11 => match_node_instruction_18(bytes, ctx),
        12 => match_node_instruction_19(bytes, ctx),
        13 => match_node_instruction_20(bytes, ctx),
        14 => match_node_instruction_21(bytes, ctx),
        15 => match_node_instruction_22(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 350");
    350
}

fn match_node_instruction_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 351");
    351
}

fn match_node_instruction_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 352");
    352
}

fn match_node_instruction_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 217");
    217
}

fn match_node_instruction_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 346");
    346
}

fn match_node_instruction_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 347");
    347
}

fn match_node_instruction_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 348");
    348
}

fn match_node_instruction_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 349");
    349
}

fn match_node_instruction_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 353");
    353
}

fn match_node_instruction_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 354");
    354
}

fn match_node_instruction_17(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 17: Terminal matched constructor ID 340");
    340
}

fn match_node_instruction_18(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 18: Terminal matched constructor ID 341");
    341
}

fn match_node_instruction_19(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 19: Terminal matched constructor ID 342");
    342
}

fn match_node_instruction_20(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 20: Terminal matched constructor ID 343");
    343
}

fn match_node_instruction_21(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 21: Terminal matched constructor ID 344");
    344
}

fn match_node_instruction_22(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 22: Terminal matched constructor ID 345");
    345
}

fn match_node_instruction_23(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 23: Terminal matched constructor ID 369");
    369
}

fn match_node_instruction_24(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 24: Terminal matched constructor ID 201");
    201
}

fn match_node_instruction_25(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 25: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_26(bytes, ctx),
        1 => match_node_instruction_159(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_26(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 1;
    eprintln!("Trace node 26: SlaInstructionBits start=0, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_27(bytes, ctx),
        1 => match_node_instruction_158(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_27(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 15;
    eprintln!("Trace node 27: SlaInstructionBits start=4, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_28(bytes, ctx),
        1 => match_node_instruction_47(bytes, ctx),
        2 => match_node_instruction_64(bytes, ctx),
        3 => match_node_instruction_81(bytes, ctx),
        4 => match_node_instruction_98(bytes, ctx),
        5 => match_node_instruction_115(bytes, ctx),
        6 => match_node_instruction_116(bytes, ctx),
        7 => match_node_instruction_117(bytes, ctx),
        8 => match_node_instruction_118(bytes, ctx),
        9 => match_node_instruction_135(bytes, ctx),
        10 => match_node_instruction_152(bytes, ctx),
        11 => match_node_instruction_153(bytes, ctx),
        12 => match_node_instruction_154(bytes, ctx),
        13 => match_node_instruction_155(bytes, ctx),
        14 => match_node_instruction_156(bytes, ctx),
        15 => match_node_instruction_157(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_28(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 15;
    eprintln!("Trace node 28: SlaInstructionBits start=24, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_29(bytes, ctx),
        1 => match_node_instruction_32(bytes, ctx),
        2 => match_node_instruction_33(bytes, ctx),
        3 => match_node_instruction_34(bytes, ctx),
        4 => match_node_instruction_35(bytes, ctx),
        5 => match_node_instruction_36(bytes, ctx),
        6 => match_node_instruction_37(bytes, ctx),
        7 => match_node_instruction_38(bytes, ctx),
        8 => match_node_instruction_39(bytes, ctx),
        9 => match_node_instruction_40(bytes, ctx),
        10 => match_node_instruction_41(bytes, ctx),
        11 => match_node_instruction_42(bytes, ctx),
        12 => match_node_instruction_43(bytes, ctx),
        13 => match_node_instruction_44(bytes, ctx),
        14 => match_node_instruction_45(bytes, ctx),
        15 => match_node_instruction_46(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_29(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (19 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 19) & 1;
    eprintln!("Trace node 29: SlaInstructionBits start=19, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_30(bytes, ctx),
        1 => match_node_instruction_31(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_30(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 30: Terminal matched constructor ID 190");
    190
}

fn match_node_instruction_31(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 31: Terminal matched constructor ID 191");
    191
}

fn match_node_instruction_32(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 32: Terminal matched constructor ID 355");
    355
}

fn match_node_instruction_33(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 33: Terminal matched constructor ID 178");
    178
}

fn match_node_instruction_34(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 34: Terminal matched constructor ID 355");
    355
}

fn match_node_instruction_35(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 35: Terminal matched constructor ID 118");
    118
}

fn match_node_instruction_36(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 36: Terminal matched constructor ID 121");
    121
}

fn match_node_instruction_37(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 37: Terminal matched constructor ID 122");
    122
}

fn match_node_instruction_38(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 38: Terminal matched constructor ID 125");
    125
}

fn match_node_instruction_39(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 39: Terminal matched constructor ID 136");
    136
}

fn match_node_instruction_40(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 40: Terminal matched constructor ID 139");
    139
}

fn match_node_instruction_41(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 41: Terminal matched constructor ID 140");
    140
}

fn match_node_instruction_42(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 42: Terminal matched constructor ID 143");
    143
}

fn match_node_instruction_43(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 43: Terminal matched constructor ID 156");
    156
}

fn match_node_instruction_44(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 44: Terminal matched constructor ID 159");
    159
}

fn match_node_instruction_45(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 45: Terminal matched constructor ID 160");
    160
}

fn match_node_instruction_46(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 46: Terminal matched constructor ID 163");
    163
}

fn match_node_instruction_47(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 15;
    eprintln!("Trace node 47: SlaInstructionBits start=24, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_48(bytes, ctx),
        1 => match_node_instruction_49(bytes, ctx),
        2 => match_node_instruction_50(bytes, ctx),
        3 => match_node_instruction_51(bytes, ctx),
        4 => match_node_instruction_52(bytes, ctx),
        5 => match_node_instruction_53(bytes, ctx),
        6 => match_node_instruction_54(bytes, ctx),
        7 => match_node_instruction_55(bytes, ctx),
        8 => match_node_instruction_56(bytes, ctx),
        9 => match_node_instruction_57(bytes, ctx),
        10 => match_node_instruction_58(bytes, ctx),
        11 => match_node_instruction_59(bytes, ctx),
        12 => match_node_instruction_60(bytes, ctx),
        13 => match_node_instruction_61(bytes, ctx),
        14 => match_node_instruction_62(bytes, ctx),
        15 => match_node_instruction_63(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_48(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 48: Terminal matched constructor ID 356");
    356
}

fn match_node_instruction_49(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 49: Terminal matched constructor ID 356");
    356
}

fn match_node_instruction_50(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 50: Terminal matched constructor ID 356");
    356
}

fn match_node_instruction_51(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 51: Terminal matched constructor ID 356");
    356
}

fn match_node_instruction_52(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 52: Terminal matched constructor ID 127");
    127
}

fn match_node_instruction_53(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 53: Terminal matched constructor ID 130");
    130
}

fn match_node_instruction_54(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 54: Terminal matched constructor ID 131");
    131
}

fn match_node_instruction_55(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 55: Terminal matched constructor ID 134");
    134
}

fn match_node_instruction_56(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 56: Terminal matched constructor ID 145");
    145
}

fn match_node_instruction_57(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 57: Terminal matched constructor ID 148");
    148
}

fn match_node_instruction_58(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 58: Terminal matched constructor ID 149");
    149
}

fn match_node_instruction_59(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 59: Terminal matched constructor ID 152");
    152
}

fn match_node_instruction_60(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 60: Terminal matched constructor ID 165");
    165
}

fn match_node_instruction_61(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 61: Terminal matched constructor ID 168");
    168
}

fn match_node_instruction_62(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 62: Terminal matched constructor ID 169");
    169
}

fn match_node_instruction_63(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 63: Terminal matched constructor ID 172");
    172
}

fn match_node_instruction_64(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 15;
    eprintln!("Trace node 64: SlaInstructionBits start=24, size=4, word={:08x}, probe={}", word, probe);
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
    eprintln!("Trace node 65: Terminal matched constructor ID 283");
    283
}

fn match_node_instruction_66(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 66: Terminal matched constructor ID 286");
    286
}

fn match_node_instruction_67(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 67: Terminal matched constructor ID 287");
    287
}

fn match_node_instruction_68(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 68: Terminal matched constructor ID 290");
    290
}

fn match_node_instruction_69(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 69: Terminal matched constructor ID 220");
    220
}

fn match_node_instruction_70(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 70: Terminal matched constructor ID 223");
    223
}

fn match_node_instruction_71(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 71: Terminal matched constructor ID 226");
    226
}

fn match_node_instruction_72(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 72: Terminal matched constructor ID 229");
    229
}

fn match_node_instruction_73(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 73: Terminal matched constructor ID 232");
    232
}

fn match_node_instruction_74(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 74: Terminal matched constructor ID 235");
    235
}

fn match_node_instruction_75(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 75: Terminal matched constructor ID 238");
    238
}

fn match_node_instruction_76(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 76: Terminal matched constructor ID 241");
    241
}

fn match_node_instruction_77(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 77: Terminal matched constructor ID 260");
    260
}

fn match_node_instruction_78(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 78: Terminal matched constructor ID 263");
    263
}

fn match_node_instruction_79(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 79: Terminal matched constructor ID 266");
    266
}

fn match_node_instruction_80(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 80: Terminal matched constructor ID 269");
    269
}

fn match_node_instruction_81(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 15;
    eprintln!("Trace node 81: SlaInstructionBits start=24, size=4, word={:08x}, probe={}", word, probe);
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
    eprintln!("Trace node 82: Terminal matched constructor ID 332");
    332
}

fn match_node_instruction_83(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 83: Terminal matched constructor ID 335");
    335
}

fn match_node_instruction_84(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 84: Terminal matched constructor ID 336");
    336
}

fn match_node_instruction_85(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 85: Terminal matched constructor ID 339");
    339
}

fn match_node_instruction_86(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 86: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_87(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 87: Terminal matched constructor ID 275");
    275
}

fn match_node_instruction_88(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 88: Terminal matched constructor ID 278");
    278
}

fn match_node_instruction_89(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 89: Terminal matched constructor ID 281");
    281
}

fn match_node_instruction_90(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 90: Terminal matched constructor ID 293");
    293
}

fn match_node_instruction_91(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 91: Terminal matched constructor ID 296");
    296
}

fn match_node_instruction_92(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 92: Terminal matched constructor ID 299");
    299
}

fn match_node_instruction_93(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 93: Terminal matched constructor ID 302");
    302
}

fn match_node_instruction_94(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 94: Terminal matched constructor ID 321");
    321
}

fn match_node_instruction_95(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 95: Terminal matched constructor ID 324");
    324
}

fn match_node_instruction_96(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 96: Terminal matched constructor ID 327");
    327
}

fn match_node_instruction_97(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 97: Terminal matched constructor ID 330");
    330
}

fn match_node_instruction_98(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 15;
    eprintln!("Trace node 98: SlaInstructionBits start=24, size=4, word={:08x}, probe={}", word, probe);
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
    eprintln!("Trace node 99: Terminal matched constructor ID 173");
    173
}

fn match_node_instruction_100(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 100: Terminal matched constructor ID 174");
    174
}

fn match_node_instruction_101(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 101: Terminal matched constructor ID 175");
    175
}

fn match_node_instruction_102(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 102: Terminal matched constructor ID 176");
    176
}

fn match_node_instruction_103(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 103: Terminal matched constructor ID 357");
    357
}

fn match_node_instruction_104(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 104: Terminal matched constructor ID 357");
    357
}

fn match_node_instruction_105(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 105: Terminal matched constructor ID 357");
    357
}

fn match_node_instruction_106(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 106: Terminal matched constructor ID 357");
    357
}

fn match_node_instruction_107(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 107: Terminal matched constructor ID 194");
    194
}

fn match_node_instruction_108(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 108: Terminal matched constructor ID 90");
    90
}

fn match_node_instruction_109(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 109: Terminal matched constructor ID 101");
    101
}

fn match_node_instruction_110(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 110: Terminal matched constructor ID 82");
    82
}

fn match_node_instruction_111(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 111: Terminal matched constructor ID 60");
    60
}

fn match_node_instruction_112(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 112: Terminal matched constructor ID 39");
    39
}

fn match_node_instruction_113(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 113: Terminal matched constructor ID 40");
    40
}

fn match_node_instruction_114(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 114: Terminal matched constructor ID 41");
    41
}

fn match_node_instruction_115(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 115: Terminal matched constructor ID 358");
    358
}

fn match_node_instruction_116(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 116: Terminal matched constructor ID 359");
    359
}

fn match_node_instruction_117(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 117: Terminal matched constructor ID 360");
    360
}

fn match_node_instruction_118(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 15;
    eprintln!("Trace node 118: SlaInstructionBits start=24, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_119(bytes, ctx),
        1 => match_node_instruction_120(bytes, ctx),
        2 => match_node_instruction_121(bytes, ctx),
        3 => match_node_instruction_122(bytes, ctx),
        4 => match_node_instruction_123(bytes, ctx),
        5 => match_node_instruction_124(bytes, ctx),
        6 => match_node_instruction_125(bytes, ctx),
        7 => match_node_instruction_126(bytes, ctx),
        8 => match_node_instruction_127(bytes, ctx),
        9 => match_node_instruction_128(bytes, ctx),
        10 => match_node_instruction_129(bytes, ctx),
        11 => match_node_instruction_130(bytes, ctx),
        12 => match_node_instruction_131(bytes, ctx),
        13 => match_node_instruction_132(bytes, ctx),
        14 => match_node_instruction_133(bytes, ctx),
        15 => match_node_instruction_134(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_119(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 119: Terminal matched constructor ID 361");
    361
}

fn match_node_instruction_120(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 120: Terminal matched constructor ID 361");
    361
}

fn match_node_instruction_121(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 121: Terminal matched constructor ID 361");
    361
}

fn match_node_instruction_122(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 122: Terminal matched constructor ID 361");
    361
}

fn match_node_instruction_123(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 123: Terminal matched constructor ID 221");
    221
}

fn match_node_instruction_124(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 124: Terminal matched constructor ID 224");
    224
}

fn match_node_instruction_125(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 125: Terminal matched constructor ID 361");
    361
}

fn match_node_instruction_126(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 126: Terminal matched constructor ID 361");
    361
}

fn match_node_instruction_127(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 127: Terminal matched constructor ID 233");
    233
}

fn match_node_instruction_128(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 128: Terminal matched constructor ID 236");
    236
}

fn match_node_instruction_129(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 129: Terminal matched constructor ID 361");
    361
}

fn match_node_instruction_130(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 130: Terminal matched constructor ID 361");
    361
}

fn match_node_instruction_131(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 131: Terminal matched constructor ID 261");
    261
}

fn match_node_instruction_132(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 132: Terminal matched constructor ID 264");
    264
}

fn match_node_instruction_133(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 133: Terminal matched constructor ID 361");
    361
}

fn match_node_instruction_134(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 134: Terminal matched constructor ID 361");
    361
}

fn match_node_instruction_135(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 15;
    eprintln!("Trace node 135: SlaInstructionBits start=24, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_136(bytes, ctx),
        1 => match_node_instruction_137(bytes, ctx),
        2 => match_node_instruction_138(bytes, ctx),
        3 => match_node_instruction_139(bytes, ctx),
        4 => match_node_instruction_140(bytes, ctx),
        5 => match_node_instruction_141(bytes, ctx),
        6 => match_node_instruction_142(bytes, ctx),
        7 => match_node_instruction_143(bytes, ctx),
        8 => match_node_instruction_144(bytes, ctx),
        9 => match_node_instruction_145(bytes, ctx),
        10 => match_node_instruction_146(bytes, ctx),
        11 => match_node_instruction_147(bytes, ctx),
        12 => match_node_instruction_148(bytes, ctx),
        13 => match_node_instruction_149(bytes, ctx),
        14 => match_node_instruction_150(bytes, ctx),
        15 => match_node_instruction_151(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_136(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 136: Terminal matched constructor ID 362");
    362
}

fn match_node_instruction_137(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 137: Terminal matched constructor ID 362");
    362
}

fn match_node_instruction_138(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 138: Terminal matched constructor ID 362");
    362
}

fn match_node_instruction_139(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 139: Terminal matched constructor ID 362");
    362
}

fn match_node_instruction_140(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 140: Terminal matched constructor ID 273");
    273
}

fn match_node_instruction_141(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 141: Terminal matched constructor ID 276");
    276
}

fn match_node_instruction_142(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 142: Terminal matched constructor ID 362");
    362
}

fn match_node_instruction_143(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 143: Terminal matched constructor ID 362");
    362
}

fn match_node_instruction_144(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 144: Terminal matched constructor ID 294");
    294
}

fn match_node_instruction_145(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 145: Terminal matched constructor ID 297");
    297
}

fn match_node_instruction_146(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 146: Terminal matched constructor ID 362");
    362
}

fn match_node_instruction_147(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 147: Terminal matched constructor ID 362");
    362
}

fn match_node_instruction_148(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 148: Terminal matched constructor ID 322");
    322
}

fn match_node_instruction_149(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 149: Terminal matched constructor ID 325");
    325
}

fn match_node_instruction_150(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 150: Terminal matched constructor ID 362");
    362
}

fn match_node_instruction_151(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 151: Terminal matched constructor ID 362");
    362
}

fn match_node_instruction_152(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 152: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_153(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 153: Terminal matched constructor ID 364");
    364
}

fn match_node_instruction_154(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 154: Terminal matched constructor ID 365");
    365
}

fn match_node_instruction_155(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 155: Terminal matched constructor ID 366");
    366
}

fn match_node_instruction_156(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 156: Terminal matched constructor ID 367");
    367
}

fn match_node_instruction_157(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 157: Terminal matched constructor ID 368");
    368
}

fn match_node_instruction_158(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 158: Terminal matched constructor ID 68");
    68
}

fn match_node_instruction_159(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 159: Terminal matched constructor ID 202");
    202
}

fn match_node_instruction_160(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 1;
    eprintln!("Trace node 160: SlaInstructionBits start=3, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_161(bytes, ctx),
        1 => match_node_instruction_182(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_161(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 161: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_162(bytes, ctx),
        1 => match_node_instruction_181(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_162(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 1;
    eprintln!("Trace node 162: SlaInstructionBits start=0, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_163(bytes, ctx),
        1 => match_node_instruction_164(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_163(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 163: Terminal matched constructor ID 25");
    25
}

fn match_node_instruction_164(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 15;
    eprintln!("Trace node 164: SlaInstructionBits start=4, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_165(bytes, ctx),
        1 => match_node_instruction_166(bytes, ctx),
        2 => match_node_instruction_167(bytes, ctx),
        3 => match_node_instruction_168(bytes, ctx),
        4 => match_node_instruction_169(bytes, ctx),
        5 => match_node_instruction_170(bytes, ctx),
        6 => match_node_instruction_171(bytes, ctx),
        7 => match_node_instruction_172(bytes, ctx),
        8 => match_node_instruction_173(bytes, ctx),
        9 => match_node_instruction_174(bytes, ctx),
        10 => match_node_instruction_175(bytes, ctx),
        11 => match_node_instruction_176(bytes, ctx),
        12 => match_node_instruction_177(bytes, ctx),
        13 => match_node_instruction_178(bytes, ctx),
        14 => match_node_instruction_179(bytes, ctx),
        15 => match_node_instruction_180(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_165(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 165: Terminal matched constructor ID 242");
    242
}

fn match_node_instruction_166(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 166: Terminal matched constructor ID 243");
    243
}

fn match_node_instruction_167(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 167: Terminal matched constructor ID 244");
    244
}

fn match_node_instruction_168(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 168: Terminal matched constructor ID 245");
    245
}

fn match_node_instruction_169(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 169: Terminal matched constructor ID 246");
    246
}

fn match_node_instruction_170(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 170: Terminal matched constructor ID 247");
    247
}

fn match_node_instruction_171(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 171: Terminal matched constructor ID 248");
    248
}

fn match_node_instruction_172(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 172: Terminal matched constructor ID 249");
    249
}

fn match_node_instruction_173(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 173: Terminal matched constructor ID 250");
    250
}

fn match_node_instruction_174(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 174: Terminal matched constructor ID 251");
    251
}

fn match_node_instruction_175(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 175: Terminal matched constructor ID 252");
    252
}

fn match_node_instruction_176(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 176: Terminal matched constructor ID 253");
    253
}

fn match_node_instruction_177(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 177: Terminal matched constructor ID 254");
    254
}

fn match_node_instruction_178(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 178: Terminal matched constructor ID 255");
    255
}

fn match_node_instruction_179(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 179: Terminal matched constructor ID 256");
    256
}

fn match_node_instruction_180(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 180: Terminal matched constructor ID 257");
    257
}

fn match_node_instruction_181(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 181: Terminal matched constructor ID 203");
    203
}

fn match_node_instruction_182(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 182: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_183(bytes, ctx),
        1 => match_node_instruction_202(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_183(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 1;
    eprintln!("Trace node 183: SlaInstructionBits start=0, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_184(bytes, ctx),
        1 => match_node_instruction_185(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_184(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 184: Terminal matched constructor ID 61");
    61
}

fn match_node_instruction_185(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 15;
    eprintln!("Trace node 185: SlaInstructionBits start=4, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_186(bytes, ctx),
        1 => match_node_instruction_187(bytes, ctx),
        2 => match_node_instruction_188(bytes, ctx),
        3 => match_node_instruction_189(bytes, ctx),
        4 => match_node_instruction_190(bytes, ctx),
        5 => match_node_instruction_191(bytes, ctx),
        6 => match_node_instruction_192(bytes, ctx),
        7 => match_node_instruction_193(bytes, ctx),
        8 => match_node_instruction_194(bytes, ctx),
        9 => match_node_instruction_195(bytes, ctx),
        10 => match_node_instruction_196(bytes, ctx),
        11 => match_node_instruction_197(bytes, ctx),
        12 => match_node_instruction_198(bytes, ctx),
        13 => match_node_instruction_199(bytes, ctx),
        14 => match_node_instruction_200(bytes, ctx),
        15 => match_node_instruction_201(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_186(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 186: Terminal matched constructor ID 303");
    303
}

fn match_node_instruction_187(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 187: Terminal matched constructor ID 304");
    304
}

fn match_node_instruction_188(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 188: Terminal matched constructor ID 305");
    305
}

fn match_node_instruction_189(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 189: Terminal matched constructor ID 306");
    306
}

fn match_node_instruction_190(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 190: Terminal matched constructor ID 307");
    307
}

fn match_node_instruction_191(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 191: Terminal matched constructor ID 308");
    308
}

fn match_node_instruction_192(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 192: Terminal matched constructor ID 309");
    309
}

fn match_node_instruction_193(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 193: Terminal matched constructor ID 310");
    310
}

fn match_node_instruction_194(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 194: Terminal matched constructor ID 311");
    311
}

fn match_node_instruction_195(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 195: Terminal matched constructor ID 312");
    312
}

fn match_node_instruction_196(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 196: Terminal matched constructor ID 313");
    313
}

fn match_node_instruction_197(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 197: Terminal matched constructor ID 314");
    314
}

fn match_node_instruction_198(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 198: Terminal matched constructor ID 315");
    315
}

fn match_node_instruction_199(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 199: Terminal matched constructor ID 316");
    316
}

fn match_node_instruction_200(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 200: Terminal matched constructor ID 317");
    317
}

fn match_node_instruction_201(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 201: Terminal matched constructor ID 318");
    318
}

fn match_node_instruction_202(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 202: Terminal matched constructor ID 204");
    204
}

fn match_node_instruction_203(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 1;
    eprintln!("Trace node 203: SlaInstructionBits start=3, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_204(bytes, ctx),
        1 => match_node_instruction_209(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_204(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 204: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_205(bytes, ctx),
        1 => match_node_instruction_208(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_205(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 1;
    eprintln!("Trace node 205: SlaInstructionBits start=0, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_206(bytes, ctx),
        1 => match_node_instruction_207(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_206(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 206: Terminal matched constructor ID 81");
    81
}

fn match_node_instruction_207(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 207: Terminal matched constructor ID 192");
    192
}

fn match_node_instruction_208(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 208: Terminal matched constructor ID 205");
    205
}

fn match_node_instruction_209(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 209: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_210(bytes, ctx),
        1 => match_node_instruction_213(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_210(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 1;
    eprintln!("Trace node 210: SlaInstructionBits start=0, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_211(bytes, ctx),
        1 => match_node_instruction_212(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_211(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 211: Terminal matched constructor ID 89");
    89
}

fn match_node_instruction_212(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 212: Terminal matched constructor ID 193");
    193
}

fn match_node_instruction_213(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 213: Terminal matched constructor ID 206");
    206
}

fn match_node_instruction_214(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 214: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_215(bytes, ctx),
        1 => match_node_instruction_220(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_215(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 1;
    eprintln!("Trace node 215: SlaInstructionBits start=0, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_216(bytes, ctx),
        1 => match_node_instruction_219(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_216(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 1;
    eprintln!("Trace node 216: SlaInstructionBits start=3, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_217(bytes, ctx),
        1 => match_node_instruction_218(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_217(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 217: Terminal matched constructor ID 100");
    100
}

fn match_node_instruction_218(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 218: Terminal matched constructor ID 4");
    4
}

fn match_node_instruction_219(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 219: Terminal matched constructor ID 370");
    370
}

fn match_node_instruction_220(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 1;
    eprintln!("Trace node 220: SlaInstructionBits start=3, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_221(bytes, ctx),
        1 => match_node_instruction_222(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_221(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 221: Terminal matched constructor ID 207");
    207
}

fn match_node_instruction_222(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 222: Terminal matched constructor ID 208");
    208
}

fn match_node_instruction_223(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 7;
    eprintln!("Trace node 223: SlaInstructionBits start=1, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_224(bytes, ctx),
        1 => match_node_instruction_225(bytes, ctx),
        2 => match_node_instruction_226(bytes, ctx),
        3 => match_node_instruction_227(bytes, ctx),
        4 => match_node_instruction_228(bytes, ctx),
        5 => match_node_instruction_229(bytes, ctx),
        6 => match_node_instruction_230(bytes, ctx),
        7 => match_node_instruction_231(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_224(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 224: Terminal matched constructor ID 209");
    209
}

fn match_node_instruction_225(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 225: Terminal matched constructor ID 210");
    210
}

fn match_node_instruction_226(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 226: Terminal matched constructor ID 211");
    211
}

fn match_node_instruction_227(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 227: Terminal matched constructor ID 212");
    212
}

fn match_node_instruction_228(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 228: Terminal matched constructor ID 213");
    213
}

fn match_node_instruction_229(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 229: Terminal matched constructor ID 214");
    214
}

fn match_node_instruction_230(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 230: Terminal matched constructor ID 215");
    215
}

fn match_node_instruction_231(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 231: Terminal matched constructor ID 216");
    216
}

fn match_node_instruction_232(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 232: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_233(bytes, ctx),
        1 => match_node_instruction_234(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_233(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 233: Terminal matched constructor ID 24");
    24
}

fn match_node_instruction_234(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 234: Terminal matched constructor ID 3");
    3
}

fn match_node_instruction_235(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 235: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_236(bytes, ctx),
        1 => match_node_instruction_237(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_236(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 236: Terminal matched constructor ID 153");
    153
}

fn match_node_instruction_237(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 237: Terminal matched constructor ID 154");
    154
}

fn match_node_instruction_238(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 238: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_239(bytes, ctx),
        1 => match_node_instruction_242(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_239(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 1;
    eprintln!("Trace node 239: SlaInstructionBits start=3, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_240(bytes, ctx),
        1 => match_node_instruction_241(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_240(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 240: Terminal matched constructor ID 92");
    92
}

fn match_node_instruction_241(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 241: Terminal matched constructor ID 93");
    93
}

fn match_node_instruction_242(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 1;
    eprintln!("Trace node 242: SlaInstructionBits start=0, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_243(bytes, ctx),
        1 => match_node_instruction_244(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_243(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 243: Terminal matched constructor ID 371");
    371
}

fn match_node_instruction_244(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 244: Terminal matched constructor ID 111");
    111
}

fn match_node_instruction_245(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 245: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_246(bytes, ctx),
        1 => match_node_instruction_249(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_246(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 1;
    eprintln!("Trace node 246: SlaInstructionBits start=3, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_247(bytes, ctx),
        1 => match_node_instruction_248(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_247(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 247: Terminal matched constructor ID 196");
    196
}

fn match_node_instruction_248(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 248: Terminal matched constructor ID 197");
    197
}

fn match_node_instruction_249(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 249: Terminal matched constructor ID 45");
    45
}

fn match_node_instruction_250(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 250: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_251(bytes, ctx),
        1 => match_node_instruction_252(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_251(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 251: Terminal matched constructor ID 179");
    179
}

fn match_node_instruction_252(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 252: Terminal matched constructor ID 181");
    181
}

fn match_node_instruction_253(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 253: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_254(bytes, ctx),
        1 => match_node_instruction_255(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_254(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 254: Terminal matched constructor ID 180");
    180
}

fn match_node_instruction_255(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 255: Terminal matched constructor ID 182");
    182
}

fn match_node_instruction_256(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 1;
    eprintln!("Trace node 256: SlaInstructionBits start=3, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_257(bytes, ctx),
        1 => match_node_instruction_258(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_257(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 257: Terminal matched constructor ID 187");
    187
}

fn match_node_instruction_258(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 258: Terminal matched constructor ID 188");
    188
}

fn match_node_instruction_259(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 15;
    eprintln!("Trace node 259: SlaInstructionBits start=12, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_260(bytes, ctx),
        1 => match_node_instruction_261(bytes, ctx),
        2 => match_node_instruction_262(bytes, ctx),
        3 => match_node_instruction_263(bytes, ctx),
        4 => match_node_instruction_264(bytes, ctx),
        5 => match_node_instruction_265(bytes, ctx),
        6 => match_node_instruction_266(bytes, ctx),
        7 => match_node_instruction_267(bytes, ctx),
        8 => match_node_instruction_268(bytes, ctx),
        9 => match_node_instruction_269(bytes, ctx),
        10 => match_node_instruction_270(bytes, ctx),
        11 => match_node_instruction_271(bytes, ctx),
        12 => match_node_instruction_272(bytes, ctx),
        13 => match_node_instruction_275(bytes, ctx),
        14 => match_node_instruction_276(bytes, ctx),
        15 => match_node_instruction_277(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_260(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 260: Terminal matched constructor ID 76");
    76
}

fn match_node_instruction_261(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 261: Terminal matched constructor ID 77");
    77
}

fn match_node_instruction_262(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 262: Terminal matched constructor ID 79");
    79
}

fn match_node_instruction_263(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 263: Terminal matched constructor ID 80");
    80
}

fn match_node_instruction_264(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 264: Terminal matched constructor ID 84");
    84
}

fn match_node_instruction_265(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 265: Terminal matched constructor ID 85");
    85
}

fn match_node_instruction_266(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 266: Terminal matched constructor ID 87");
    87
}

fn match_node_instruction_267(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 267: Terminal matched constructor ID 88");
    88
}

fn match_node_instruction_268(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 268: Terminal matched constructor ID 95");
    95
}

fn match_node_instruction_269(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 269: Terminal matched constructor ID 96");
    96
}

fn match_node_instruction_270(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 270: Terminal matched constructor ID 98");
    98
}

fn match_node_instruction_271(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 271: Terminal matched constructor ID 99");
    99
}

fn match_node_instruction_272(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 1;
    eprintln!("Trace node 272: SlaInstructionBits start=0, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_273(bytes, ctx),
        1 => match_node_instruction_274(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_273(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 273: Terminal matched constructor ID 30");
    30
}

fn match_node_instruction_274(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 274: Terminal matched constructor ID 31");
    31
}

fn match_node_instruction_275(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 275: Terminal matched constructor ID 32");
    32
}

fn match_node_instruction_276(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 276: Terminal matched constructor ID 34");
    34
}

fn match_node_instruction_277(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 277: Terminal matched constructor ID 35");
    35
}

fn match_node_instruction_278(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 15;
    eprintln!("Trace node 278: SlaInstructionBits start=12, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_279(bytes, ctx),
        1 => match_node_instruction_280(bytes, ctx),
        2 => match_node_instruction_281(bytes, ctx),
        3 => match_node_instruction_282(bytes, ctx),
        4 => match_node_instruction_283(bytes, ctx),
        5 => match_node_instruction_284(bytes, ctx),
        6 => match_node_instruction_285(bytes, ctx),
        7 => match_node_instruction_286(bytes, ctx),
        8 => match_node_instruction_287(bytes, ctx),
        9 => match_node_instruction_288(bytes, ctx),
        10 => match_node_instruction_289(bytes, ctx),
        11 => match_node_instruction_290(bytes, ctx),
        12 => match_node_instruction_291(bytes, ctx),
        13 => match_node_instruction_292(bytes, ctx),
        14 => match_node_instruction_293(bytes, ctx),
        15 => match_node_instruction_294(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_279(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 279: Terminal matched constructor ID 16");
    16
}

fn match_node_instruction_280(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 280: Terminal matched constructor ID 17");
    17
}

fn match_node_instruction_281(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 281: Terminal matched constructor ID 37");
    37
}

fn match_node_instruction_282(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 282: Terminal matched constructor ID 38");
    38
}

fn match_node_instruction_283(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 283: Terminal matched constructor ID 19");
    19
}

fn match_node_instruction_284(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 284: Terminal matched constructor ID 20");
    20
}

fn match_node_instruction_285(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 285: Terminal matched constructor ID 22");
    22
}

fn match_node_instruction_286(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 286: Terminal matched constructor ID 23");
    23
}

fn match_node_instruction_287(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 287: Terminal matched constructor ID 52");
    52
}

fn match_node_instruction_288(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 288: Terminal matched constructor ID 53");
    53
}

fn match_node_instruction_289(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 289: Terminal matched constructor ID 63");
    63
}

fn match_node_instruction_290(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 290: Terminal matched constructor ID 64");
    64
}

fn match_node_instruction_291(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 291: Terminal matched constructor ID 55");
    55
}

fn match_node_instruction_292(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 292: Terminal matched constructor ID 56");
    56
}

fn match_node_instruction_293(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 293: Terminal matched constructor ID 58");
    58
}

fn match_node_instruction_294(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 294: Terminal matched constructor ID 59");
    59
}

fn match_node_instruction_295(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 7;
    eprintln!("Trace node 295: SlaInstructionBits start=12, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_296(bytes, ctx),
        1 => match_node_instruction_301(bytes, ctx),
        2 => match_node_instruction_304(bytes, ctx),
        3 => match_node_instruction_307(bytes, ctx),
        4 => match_node_instruction_310(bytes, ctx),
        5 => match_node_instruction_313(bytes, ctx),
        6 => match_node_instruction_314(bytes, ctx),
        7 => match_node_instruction_315(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_296(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 296: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_297(bytes, ctx),
        1 => match_node_instruction_300(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_297(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 1;
    eprintln!("Trace node 297: SlaInstructionBits start=0, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_298(bytes, ctx),
        1 => match_node_instruction_299(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_298(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 298: Terminal matched constructor ID 102");
    102
}

fn match_node_instruction_299(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 299: Terminal matched constructor ID 103");
    103
}

fn match_node_instruction_300(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 300: Terminal matched constructor ID 104");
    104
}

fn match_node_instruction_301(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 301: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_302(bytes, ctx),
        1 => match_node_instruction_303(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_302(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 302: Terminal matched constructor ID 108");
    108
}

fn match_node_instruction_303(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 303: Terminal matched constructor ID 109");
    109
}

fn match_node_instruction_304(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 304: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_305(bytes, ctx),
        1 => match_node_instruction_306(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_305(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 305: Terminal matched constructor ID 112");
    112
}

fn match_node_instruction_306(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 306: Terminal matched constructor ID 110");
    110
}

fn match_node_instruction_307(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 307: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_308(bytes, ctx),
        1 => match_node_instruction_309(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_308(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 308: Terminal matched constructor ID 116");
    116
}

fn match_node_instruction_309(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 309: Terminal matched constructor ID 114");
    114
}

fn match_node_instruction_310(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 310: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_311(bytes, ctx),
        1 => match_node_instruction_312(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_311(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 311: Terminal matched constructor ID 107");
    107
}

fn match_node_instruction_312(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 312: Terminal matched constructor ID 115");
    115
}

fn match_node_instruction_313(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 313: Terminal matched constructor ID 113");
    113
}

fn match_node_instruction_314(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 314: Terminal matched constructor ID 105");
    105
}

fn match_node_instruction_315(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 315: Terminal matched constructor ID 106");
    106
}

fn match_node_instruction_316(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 15;
    eprintln!("Trace node 316: SlaInstructionBits start=12, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_317(bytes, ctx),
        1 => match_node_instruction_318(bytes, ctx),
        2 => match_node_instruction_319(bytes, ctx),
        3 => match_node_instruction_320(bytes, ctx),
        4 => match_node_instruction_321(bytes, ctx),
        5 => match_node_instruction_322(bytes, ctx),
        6 => match_node_instruction_323(bytes, ctx),
        7 => match_node_instruction_324(bytes, ctx),
        8 => match_node_instruction_325(bytes, ctx),
        9 => match_node_instruction_326(bytes, ctx),
        10 => match_node_instruction_327(bytes, ctx),
        11 => match_node_instruction_328(bytes, ctx),
        12 => match_node_instruction_329(bytes, ctx),
        13 => match_node_instruction_330(bytes, ctx),
        14 => match_node_instruction_331(bytes, ctx),
        15 => match_node_instruction_332(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_317(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 317: Terminal matched constructor ID 66");
    66
}

fn match_node_instruction_318(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 318: Terminal matched constructor ID 67");
    67
}

fn match_node_instruction_319(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 319: Terminal matched constructor ID 73");
    73
}

fn match_node_instruction_320(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 320: Terminal matched constructor ID 74");
    74
}

fn match_node_instruction_321(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 321: Terminal matched constructor ID 6");
    6
}

fn match_node_instruction_322(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 322: Terminal matched constructor ID 7");
    7
}

fn match_node_instruction_323(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 323: Terminal matched constructor ID 70");
    70
}

fn match_node_instruction_324(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 324: Terminal matched constructor ID 71");
    71
}

fn match_node_instruction_325(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 325: Terminal matched constructor ID 1");
    1
}

fn match_node_instruction_326(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 326: Terminal matched constructor ID 2");
    2
}

fn match_node_instruction_327(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 327: Terminal matched constructor ID 9");
    9
}

fn match_node_instruction_328(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 328: Terminal matched constructor ID 10");
    10
}

fn match_node_instruction_329(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 329: Terminal matched constructor ID 11");
    11
}

fn match_node_instruction_330(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 330: Terminal matched constructor ID 13");
    13
}

fn match_node_instruction_331(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 331: Terminal matched constructor ID 12");
    12
}

fn match_node_instruction_332(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 332: Terminal matched constructor ID 14");
    14
}

fn match_node_instruction_333(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 7;
    eprintln!("Trace node 333: SlaInstructionBits start=12, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_334(bytes, ctx),
        1 => match_node_instruction_337(bytes, ctx),
        2 => match_node_instruction_340(bytes, ctx),
        3 => match_node_instruction_343(bytes, ctx),
        4 => match_node_instruction_346(bytes, ctx),
        5 => match_node_instruction_349(bytes, ctx),
        6 => match_node_instruction_358(bytes, ctx),
        7 => match_node_instruction_359(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_334(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 334: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_335(bytes, ctx),
        1 => match_node_instruction_336(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_335(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 335: Terminal matched constructor ID 27");
    27
}

fn match_node_instruction_336(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 336: Terminal matched constructor ID 28");
    28
}

fn match_node_instruction_337(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 337: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_338(bytes, ctx),
        1 => match_node_instruction_339(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_338(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 338: Terminal matched constructor ID 46");
    46
}

fn match_node_instruction_339(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 339: Terminal matched constructor ID 47");
    47
}

fn match_node_instruction_340(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 340: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_341(bytes, ctx),
        1 => match_node_instruction_342(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_341(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 341: Terminal matched constructor ID 43");
    43
}

fn match_node_instruction_342(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 342: Terminal matched constructor ID 44");
    44
}

fn match_node_instruction_343(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 343: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_344(bytes, ctx),
        1 => match_node_instruction_345(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_344(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 344: Terminal matched constructor ID 49");
    49
}

fn match_node_instruction_345(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 345: Terminal matched constructor ID 50");
    50
}

fn match_node_instruction_346(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 346: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_347(bytes, ctx),
        1 => match_node_instruction_348(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_347(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 347: Terminal matched constructor ID 124");
    124
}

fn match_node_instruction_348(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 348: Terminal matched constructor ID 129");
    129
}

fn match_node_instruction_349(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 1;
    eprintln!("Trace node 349: SlaInstructionBits start=0, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_350(bytes, ctx),
        1 => match_node_instruction_353(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_350(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 350: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_351(bytes, ctx),
        1 => match_node_instruction_352(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_351(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 351: Terminal matched constructor ID 119");
    119
}

fn match_node_instruction_352(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 352: Terminal matched constructor ID 120");
    120
}

fn match_node_instruction_353(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 353: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_354(bytes, ctx),
        1 => match_node_instruction_357(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_354(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 354: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_355(bytes, ctx),
        1 => match_node_instruction_356(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_355(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 355: Terminal matched constructor ID 117");
    117
}

fn match_node_instruction_356(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 356: Terminal matched constructor ID 126");
    126
}

fn match_node_instruction_357(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 357: Terminal matched constructor ID 123");
    123
}

fn match_node_instruction_358(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 358: Terminal matched constructor ID 133");
    133
}

fn match_node_instruction_359(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 359: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_360(bytes, ctx),
        1 => match_node_instruction_361(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_360(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 360: Terminal matched constructor ID 128");
    128
}

fn match_node_instruction_361(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 361: Terminal matched constructor ID 132");
    132
}

fn match_node_instruction_362(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 7;
    eprintln!("Trace node 362: SlaInstructionBits start=12, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_363(bytes, ctx),
        1 => match_node_instruction_366(bytes, ctx),
        2 => match_node_instruction_375(bytes, ctx),
        3 => match_node_instruction_376(bytes, ctx),
        4 => match_node_instruction_379(bytes, ctx),
        5 => match_node_instruction_382(bytes, ctx),
        6 => match_node_instruction_391(bytes, ctx),
        7 => match_node_instruction_392(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_363(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 363: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_364(bytes, ctx),
        1 => match_node_instruction_365(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_364(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 364: Terminal matched constructor ID 142");
    142
}

fn match_node_instruction_365(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 365: Terminal matched constructor ID 147");
    147
}

fn match_node_instruction_366(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 1;
    eprintln!("Trace node 366: SlaInstructionBits start=0, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_367(bytes, ctx),
        1 => match_node_instruction_370(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_367(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 367: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_368(bytes, ctx),
        1 => match_node_instruction_369(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_368(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 368: Terminal matched constructor ID 137");
    137
}

fn match_node_instruction_369(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 369: Terminal matched constructor ID 138");
    138
}

fn match_node_instruction_370(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 370: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_371(bytes, ctx),
        1 => match_node_instruction_374(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_371(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 371: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_372(bytes, ctx),
        1 => match_node_instruction_373(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_372(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 372: Terminal matched constructor ID 135");
    135
}

fn match_node_instruction_373(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 373: Terminal matched constructor ID 144");
    144
}

fn match_node_instruction_374(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 374: Terminal matched constructor ID 141");
    141
}

fn match_node_instruction_375(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 375: Terminal matched constructor ID 151");
    151
}

fn match_node_instruction_376(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 376: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_377(bytes, ctx),
        1 => match_node_instruction_378(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_377(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 377: Terminal matched constructor ID 146");
    146
}

fn match_node_instruction_378(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 378: Terminal matched constructor ID 150");
    150
}

fn match_node_instruction_379(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 379: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_380(bytes, ctx),
        1 => match_node_instruction_381(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_380(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 380: Terminal matched constructor ID 162");
    162
}

fn match_node_instruction_381(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 381: Terminal matched constructor ID 167");
    167
}

fn match_node_instruction_382(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 1;
    eprintln!("Trace node 382: SlaInstructionBits start=0, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_383(bytes, ctx),
        1 => match_node_instruction_386(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_383(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 383: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_384(bytes, ctx),
        1 => match_node_instruction_385(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_384(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 384: Terminal matched constructor ID 157");
    157
}

fn match_node_instruction_385(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 385: Terminal matched constructor ID 158");
    158
}

fn match_node_instruction_386(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 386: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_387(bytes, ctx),
        1 => match_node_instruction_390(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_387(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 387: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_388(bytes, ctx),
        1 => match_node_instruction_389(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_388(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 388: Terminal matched constructor ID 155");
    155
}

fn match_node_instruction_389(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 389: Terminal matched constructor ID 164");
    164
}

fn match_node_instruction_390(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 390: Terminal matched constructor ID 161");
    161
}

fn match_node_instruction_391(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 391: Terminal matched constructor ID 171");
    171
}

fn match_node_instruction_392(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 392: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_393(bytes, ctx),
        1 => match_node_instruction_394(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_393(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 393: Terminal matched constructor ID 166");
    166
}

fn match_node_instruction_394(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 394: Terminal matched constructor ID 170");
    170
}

fn match_node_instruction_395(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 7;
    eprintln!("Trace node 395: SlaInstructionBits start=12, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_396(bytes, ctx),
        1 => match_node_instruction_399(bytes, ctx),
        2 => match_node_instruction_402(bytes, ctx),
        3 => match_node_instruction_403(bytes, ctx),
        4 => match_node_instruction_410(bytes, ctx),
        5 => match_node_instruction_413(bytes, ctx),
        6 => match_node_instruction_414(bytes, ctx),
        7 => match_node_instruction_415(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_396(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 396: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_397(bytes, ctx),
        1 => match_node_instruction_398(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_397(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 397: Terminal matched constructor ID 372");
    372
}

fn match_node_instruction_398(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 398: Terminal matched constructor ID 288");
    288
}

fn match_node_instruction_399(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 399: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_400(bytes, ctx),
        1 => match_node_instruction_401(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_400(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 400: Terminal matched constructor ID 284");
    284
}

fn match_node_instruction_401(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 401: Terminal matched constructor ID 285");
    285
}

fn match_node_instruction_402(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 402: Terminal matched constructor ID 289");
    289
}

fn match_node_instruction_403(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 403: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_404(bytes, ctx),
        1 => match_node_instruction_409(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_404(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 3;
    eprintln!("Trace node 404: SlaInstructionBits start=0, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_405(bytes, ctx),
        1 => match_node_instruction_406(bytes, ctx),
        2 => match_node_instruction_407(bytes, ctx),
        3 => match_node_instruction_408(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_405(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 405: Terminal matched constructor ID 282");
    282
}

fn match_node_instruction_406(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 406: Terminal matched constructor ID 219");
    219
}

fn match_node_instruction_407(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 407: Terminal matched constructor ID 231");
    231
}

fn match_node_instruction_408(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 408: Terminal matched constructor ID 259");
    259
}

fn match_node_instruction_409(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 409: Terminal matched constructor ID 239");
    239
}

fn match_node_instruction_410(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 410: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_411(bytes, ctx),
        1 => match_node_instruction_412(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_411(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 411: Terminal matched constructor ID 227");
    227
}

fn match_node_instruction_412(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 412: Terminal matched constructor ID 267");
    267
}

fn match_node_instruction_413(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 413: Terminal matched constructor ID 228");
    228
}

fn match_node_instruction_414(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 414: Terminal matched constructor ID 240");
    240
}

fn match_node_instruction_415(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 415: Terminal matched constructor ID 268");
    268
}

fn match_node_instruction_416(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 416: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_417(bytes, ctx),
        1 => match_node_instruction_418(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_417(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 417: Terminal matched constructor ID 258");
    258
}

fn match_node_instruction_418(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 418: Terminal matched constructor ID 262");
    262
}

fn match_node_instruction_419(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 419: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_420(bytes, ctx),
        1 => match_node_instruction_421(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_420(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 420: Terminal matched constructor ID 230");
    230
}

fn match_node_instruction_421(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 421: Terminal matched constructor ID 234");
    234
}

fn match_node_instruction_422(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 422: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_423(bytes, ctx),
        1 => match_node_instruction_424(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_423(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 423: Terminal matched constructor ID 218");
    218
}

fn match_node_instruction_424(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 424: Terminal matched constructor ID 222");
    222
}

fn match_node_instruction_425(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 7;
    eprintln!("Trace node 425: SlaInstructionBits start=12, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_426(bytes, ctx),
        1 => match_node_instruction_429(bytes, ctx),
        2 => match_node_instruction_432(bytes, ctx),
        3 => match_node_instruction_433(bytes, ctx),
        4 => match_node_instruction_440(bytes, ctx),
        5 => match_node_instruction_443(bytes, ctx),
        6 => match_node_instruction_444(bytes, ctx),
        7 => match_node_instruction_445(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_426(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 426: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_427(bytes, ctx),
        1 => match_node_instruction_428(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_427(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 427: Terminal matched constructor ID 177");
    177
}

fn match_node_instruction_428(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 428: Terminal matched constructor ID 337");
    337
}

fn match_node_instruction_429(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 429: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_430(bytes, ctx),
        1 => match_node_instruction_431(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_430(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 430: Terminal matched constructor ID 333");
    333
}

fn match_node_instruction_431(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 431: Terminal matched constructor ID 334");
    334
}

fn match_node_instruction_432(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 432: Terminal matched constructor ID 338");
    338
}

fn match_node_instruction_433(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 433: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_434(bytes, ctx),
        1 => match_node_instruction_439(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_434(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 3;
    eprintln!("Trace node 434: SlaInstructionBits start=0, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_435(bytes, ctx),
        1 => match_node_instruction_436(bytes, ctx),
        2 => match_node_instruction_437(bytes, ctx),
        3 => match_node_instruction_438(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_435(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 435: Terminal matched constructor ID 331");
    331
}

fn match_node_instruction_436(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 436: Terminal matched constructor ID 271");
    271
}

fn match_node_instruction_437(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 437: Terminal matched constructor ID 292");
    292
}

fn match_node_instruction_438(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 438: Terminal matched constructor ID 320");
    320
}

fn match_node_instruction_439(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 439: Terminal matched constructor ID 300");
    300
}

fn match_node_instruction_440(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 440: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_441(bytes, ctx),
        1 => match_node_instruction_442(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_441(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 441: Terminal matched constructor ID 279");
    279
}

fn match_node_instruction_442(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 442: Terminal matched constructor ID 328");
    328
}

fn match_node_instruction_443(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 443: Terminal matched constructor ID 280");
    280
}

fn match_node_instruction_444(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 444: Terminal matched constructor ID 301");
    301
}

fn match_node_instruction_445(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 445: Terminal matched constructor ID 329");
    329
}

fn match_node_instruction_446(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 446: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_447(bytes, ctx),
        1 => match_node_instruction_448(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_447(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 447: Terminal matched constructor ID 319");
    319
}

fn match_node_instruction_448(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 448: Terminal matched constructor ID 323");
    323
}

fn match_node_instruction_449(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 449: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_450(bytes, ctx),
        1 => match_node_instruction_451(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_450(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 450: Terminal matched constructor ID 291");
    291
}

fn match_node_instruction_451(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 451: Terminal matched constructor ID 295");
    295
}

fn match_node_instruction_452(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 452: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_453(bytes, ctx),
        1 => match_node_instruction_454(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_453(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 453: Terminal matched constructor ID 270");
    270
}

fn match_node_instruction_454(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 454: Terminal matched constructor ID 373");
    373
}

fn match_node_pop_args_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=0, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_pop_args_1(bytes, ctx),
        1 => match_node_pop_args_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_pop_args_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_pop_args_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_pop_ret_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_pop_ret_1(bytes, ctx),
        1 => match_node_pop_ret_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_pop_ret_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_pop_ret_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_prp_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_prp_1(bytes, ctx),
        1 => match_node_prp_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_prp_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_prp_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_prp2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_prp2_1(bytes, ctx),
        1 => match_node_prp2_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_prp2_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_prp2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_push_args_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=0, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_push_args_1(bytes, ctx),
        1 => match_node_push_args_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_push_args_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_push_args_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_ra_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_rs_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_rs_1(bytes, ctx),
        1 => match_node_rs_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_rs_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_rs_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

