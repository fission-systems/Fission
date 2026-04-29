// Auto-generated Fission Native Backend
#[no_mangle]
pub extern "C" fn fission_decode_match(table_ptr: *const i8, bytes: *const u8, bytes_len: usize, ctx_ptr: *const u64) -> i32 {
    let table_name = unsafe { std::ffi::CStr::from_ptr(table_ptr).to_str().unwrap() };
    let bytes = unsafe { std::slice::from_raw_parts(bytes, bytes_len) };
    let ctx = unsafe { *ctx_ptr };
    match table_name {
        "abs16offset" => match_node_abs16offset_0(bytes, ctx),
        "abs20offset" => match_node_abs20offset_0(bytes, ctx),
        "abs20offsetW" => match_node_abs20offsetW_0(bytes, ctx),
        "b1cnd3" => match_node_b1cnd3_0(bytes, ctx),
        "b2cnd3" => match_node_b2cnd3_0(bytes, ctx),
        "b2cnd4" => match_node_b2cnd4_0(bytes, ctx),
        "cnd8" => match_node_cnd8_0(bytes, ctx),
        "dsp20A0B" => match_node_dsp20A0B_0(bytes, ctx),
        "dsp20A0W" => match_node_dsp20A0W_0(bytes, ctx),
        "dsp8spB" => match_node_dsp8spB_0(bytes, ctx),
        "dsp8spW" => match_node_dsp8spW_0(bytes, ctx),
        "dst2B" => match_node_dst2B_0(bytes, ctx),
        "dst3B" => match_node_dst3B_0(bytes, ctx),
        "dst3B_afterDsp8" => match_node_dst3B_afterDsp8_0(bytes, ctx),
        "dst4A" => match_node_dst4A_0(bytes, ctx),
        "dst4Ax" => match_node_dst4Ax_0(bytes, ctx),
        "dst4B" => match_node_dst4B_0(bytes, ctx),
        "dst4B_afterSrc4" => match_node_dst4B_afterSrc4_0(bytes, ctx),
        "dst4L" => match_node_dst4L_0(bytes, ctx),
        "dst4L_afterSrc4" => match_node_dst4L_afterSrc4_0(bytes, ctx),
        "dst4T" => match_node_dst4T_0(bytes, ctx),
        "dst4W" => match_node_dst4W_0(bytes, ctx),
        "dst4W_afterSrc4" => match_node_dst4W_afterSrc4_0(bytes, ctx),
        "dst4dsp16" => match_node_dst4dsp16_0(bytes, ctx),
        "dst4dsp8" => match_node_dst4dsp8_0(bytes, ctx),
        "flagBit" => match_node_flagBit_0(bytes, ctx),
        "instruction" => match_node_instruction_0(bytes, ctx),
        "ldIntbVal" => match_node_ldIntbVal_0(bytes, ctx),
        "memBase" => match_node_memBase_0(bytes, ctx),
        "memBase11" => match_node_memBase11_0(bytes, ctx),
        "memBaseAx" => match_node_memBaseAx_0(bytes, ctx),
        "memBit" => match_node_memBit_0(bytes, ctx),
        "popRegA0" => match_node_popRegA0_0(bytes, ctx),
        "popRegA1" => match_node_popRegA1_0(bytes, ctx),
        "popRegFB" => match_node_popRegFB_0(bytes, ctx),
        "popRegList" => match_node_popRegList_0(bytes, ctx),
        "popRegR0" => match_node_popRegR0_0(bytes, ctx),
        "popRegR1" => match_node_popRegR1_0(bytes, ctx),
        "popRegR2" => match_node_popRegR2_0(bytes, ctx),
        "popRegR3" => match_node_popRegR3_0(bytes, ctx),
        "popRegSB" => match_node_popRegSB_0(bytes, ctx),
        "pushRegA0" => match_node_pushRegA0_0(bytes, ctx),
        "pushRegA1" => match_node_pushRegA1_0(bytes, ctx),
        "pushRegFB" => match_node_pushRegFB_0(bytes, ctx),
        "pushRegList" => match_node_pushRegList_0(bytes, ctx),
        "pushRegR0" => match_node_pushRegR0_0(bytes, ctx),
        "pushRegR1" => match_node_pushRegR1_0(bytes, ctx),
        "pushRegR2" => match_node_pushRegR2_0(bytes, ctx),
        "pushRegR3" => match_node_pushRegR3_0(bytes, ctx),
        "pushRegSB" => match_node_pushRegSB_0(bytes, ctx),
        "regBase" => match_node_regBase_0(bytes, ctx),
        "regBit" => match_node_regBit_0(bytes, ctx),
        "rel16offset1" => match_node_rel16offset1_0(bytes, ctx),
        "rel3offset2" => match_node_rel3offset2_0(bytes, ctx),
        "rel8offset1" => match_node_rel8offset1_0(bytes, ctx),
        "rel8offset2" => match_node_rel8offset2_0(bytes, ctx),
        "reloffset_dst4L" => match_node_reloffset_dst4L_0(bytes, ctx),
        "reloffset_dst4T" => match_node_reloffset_dst4T_0(bytes, ctx),
        "reloffset_dst4W" => match_node_reloffset_dst4W_0(bytes, ctx),
        "skipBytesBeforeDst2" => match_node_skipBytesBeforeDst2_0(bytes, ctx),
        "skipBytesBeforeDst4" => match_node_skipBytesBeforeDst4_0(bytes, ctx),
        "skipBytesBeforeImm82" => match_node_skipBytesBeforeImm82_0(bytes, ctx),
        "src4B" => match_node_src4B_0(bytes, ctx),
        "src4W" => match_node_src4W_0(bytes, ctx),
        "src4dsp16" => match_node_src4dsp16_0(bytes, ctx),
        "src4dsp8" => match_node_src4dsp8_0(bytes, ctx),
        "srcImm16" => match_node_srcImm16_0(bytes, ctx),
        "srcImm3" => match_node_srcImm3_0(bytes, ctx),
        "srcImm8" => match_node_srcImm8_0(bytes, ctx),
        "srcIntNum" => match_node_srcIntNum_0(bytes, ctx),
        "srcSimm16" => match_node_srcSimm16_0(bytes, ctx),
        "srcSimm4Shift_0003" => match_node_srcSimm4Shift_0003_0(bytes, ctx),
        "srcSimm4Shift_0407" => match_node_srcSimm4Shift_0407_0(bytes, ctx),
        "srcSimm4_0003" => match_node_srcSimm4_0003_0(bytes, ctx),
        "srcSimm4_0407" => match_node_srcSimm4_0407_0(bytes, ctx),
        "srcSimm8" => match_node_srcSimm8_0(bytes, ctx),
        "srcZero8" => match_node_srcZero8_0(bytes, ctx),
        "steA1A0B" => match_node_steA1A0B_0(bytes, ctx),
        "steA1A0W" => match_node_steA1A0W_0(bytes, ctx),
        "stzxImm82" => match_node_stzxImm82_0(bytes, ctx),
        _ => -1
    }
}

fn match_node_abs16offset_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_abs20offset_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_abs20offsetW_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_b1cnd3_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 7;
    eprintln!("Trace node 0: SlaInstructionBits start=5, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_b1cnd3_1(bytes, ctx),
        1 => match_node_b1cnd3_2(bytes, ctx),
        2 => match_node_b1cnd3_3(bytes, ctx),
        3 => match_node_b1cnd3_4(bytes, ctx),
        4 => match_node_b1cnd3_5(bytes, ctx),
        5 => match_node_b1cnd3_6(bytes, ctx),
        6 => match_node_b1cnd3_7(bytes, ctx),
        7 => match_node_b1cnd3_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_b1cnd3_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 4");
    4
}

fn match_node_b1cnd3_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 5");
    5
}

fn match_node_b1cnd3_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 6");
    6
}

fn match_node_b1cnd3_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 7");
    7
}

fn match_node_b1cnd3_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 0");
    0
}

fn match_node_b1cnd3_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 1");
    1
}

fn match_node_b1cnd3_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 2");
    2
}

fn match_node_b1cnd3_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 3");
    3
}

fn match_node_b2cnd3_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 7;
    eprintln!("Trace node 0: SlaInstructionBits start=5, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_b2cnd3_1(bytes, ctx),
        1 => match_node_b2cnd3_2(bytes, ctx),
        2 => match_node_b2cnd3_3(bytes, ctx),
        3 => match_node_b2cnd3_4(bytes, ctx),
        4 => match_node_b2cnd3_5(bytes, ctx),
        5 => match_node_b2cnd3_6(bytes, ctx),
        6 => match_node_b2cnd3_7(bytes, ctx),
        7 => match_node_b2cnd3_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_b2cnd3_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_b2cnd3_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_b2cnd3_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_b2cnd3_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched NOTHING");
    -1
}

fn match_node_b2cnd3_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 3");
    3
}

fn match_node_b2cnd3_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 4");
    4
}

fn match_node_b2cnd3_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 5");
    5
}

fn match_node_b2cnd3_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched NOTHING");
    -1
}

fn match_node_b2cnd4_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 15;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_b2cnd4_1(bytes, ctx),
        1 => match_node_b2cnd4_2(bytes, ctx),
        2 => match_node_b2cnd4_3(bytes, ctx),
        3 => match_node_b2cnd4_4(bytes, ctx),
        4 => match_node_b2cnd4_5(bytes, ctx),
        5 => match_node_b2cnd4_6(bytes, ctx),
        6 => match_node_b2cnd4_7(bytes, ctx),
        7 => match_node_b2cnd4_8(bytes, ctx),
        8 => match_node_b2cnd4_9(bytes, ctx),
        9 => match_node_b2cnd4_10(bytes, ctx),
        10 => match_node_b2cnd4_11(bytes, ctx),
        11 => match_node_b2cnd4_12(bytes, ctx),
        12 => match_node_b2cnd4_13(bytes, ctx),
        13 => match_node_b2cnd4_14(bytes, ctx),
        14 => match_node_b2cnd4_15(bytes, ctx),
        15 => match_node_b2cnd4_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_b2cnd4_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_b2cnd4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_b2cnd4_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_b2cnd4_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_b2cnd4_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 4");
    4
}

fn match_node_b2cnd4_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_b2cnd4_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 6");
    6
}

fn match_node_b2cnd4_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 7");
    7
}

fn match_node_b2cnd4_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 8");
    8
}

fn match_node_b2cnd4_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 9");
    9
}

fn match_node_b2cnd4_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 10");
    10
}

fn match_node_b2cnd4_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched NOTHING");
    -1
}

fn match_node_b2cnd4_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 11");
    11
}

fn match_node_b2cnd4_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 12");
    12
}

fn match_node_b2cnd4_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 13");
    13
}

fn match_node_b2cnd4_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched NOTHING");
    -1
}

fn match_node_cnd8_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 15;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_cnd8_1(bytes, ctx),
        1 => match_node_cnd8_2(bytes, ctx),
        2 => match_node_cnd8_3(bytes, ctx),
        3 => match_node_cnd8_4(bytes, ctx),
        4 => match_node_cnd8_5(bytes, ctx),
        5 => match_node_cnd8_6(bytes, ctx),
        6 => match_node_cnd8_7(bytes, ctx),
        7 => match_node_cnd8_8(bytes, ctx),
        8 => match_node_cnd8_9(bytes, ctx),
        9 => match_node_cnd8_10(bytes, ctx),
        10 => match_node_cnd8_11(bytes, ctx),
        11 => match_node_cnd8_12(bytes, ctx),
        12 => match_node_cnd8_13(bytes, ctx),
        13 => match_node_cnd8_14(bytes, ctx),
        14 => match_node_cnd8_15(bytes, ctx),
        15 => match_node_cnd8_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_cnd8_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_cnd8_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_cnd8_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_cnd8_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_cnd8_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 4");
    4
}

fn match_node_cnd8_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_cnd8_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 6");
    6
}

fn match_node_cnd8_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched NOTHING");
    -1
}

fn match_node_cnd8_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 7");
    7
}

fn match_node_cnd8_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 8");
    8
}

fn match_node_cnd8_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 9");
    9
}

fn match_node_cnd8_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 10");
    10
}

fn match_node_cnd8_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 11");
    11
}

fn match_node_cnd8_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 12");
    12
}

fn match_node_cnd8_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 13");
    13
}

fn match_node_cnd8_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched NOTHING");
    -1
}

fn match_node_dsp20A0B_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_dsp20A0W_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_dsp8spB_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_dsp8spW_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_dst2B_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 3;
    eprintln!("Trace node 0: SlaInstructionBits start=6, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_dst2B_1(bytes, ctx),
        1 => match_node_dst2B_4(bytes, ctx),
        2 => match_node_dst2B_5(bytes, ctx),
        3 => match_node_dst2B_6(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dst2B_1(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 1;
    eprintln!("Trace node 1: SlaInstructionBits start=5, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_dst2B_2(bytes, ctx),
        1 => match_node_dst2B_3(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dst2B_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_dst2B_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 0");
    0
}

fn match_node_dst2B_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 2");
    2
}

fn match_node_dst2B_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 3");
    3
}

fn match_node_dst2B_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 4");
    4
}

fn match_node_dst3B_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 7;
    eprintln!("Trace node 0: SlaInstructionBits start=5, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_dst3B_1(bytes, ctx),
        1 => match_node_dst3B_2(bytes, ctx),
        2 => match_node_dst3B_3(bytes, ctx),
        3 => match_node_dst3B_4(bytes, ctx),
        4 => match_node_dst3B_5(bytes, ctx),
        5 => match_node_dst3B_6(bytes, ctx),
        6 => match_node_dst3B_7(bytes, ctx),
        7 => match_node_dst3B_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dst3B_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched NOTHING");
    -1
}

fn match_node_dst3B_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched NOTHING");
    -1
}

fn match_node_dst3B_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched NOTHING");
    -1
}

fn match_node_dst3B_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 1");
    1
}

fn match_node_dst3B_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 0");
    0
}

fn match_node_dst3B_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 2");
    2
}

fn match_node_dst3B_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 3");
    3
}

fn match_node_dst3B_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 4");
    4
}

fn match_node_dst3B_afterDsp8_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_dst4A_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_dst4A_1(bytes, ctx),
        1 => match_node_dst4A_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dst4A_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_dst4A_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 2");
    2
}

fn match_node_dst4Ax_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_dst4B_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 3;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_dst4B_1(bytes, ctx),
        1 => match_node_dst4B_2(bytes, ctx),
        2 => match_node_dst4B_5(bytes, ctx),
        3 => match_node_dst4B_6(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dst4B_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_dst4B_2(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 2: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_dst4B_3(bytes, ctx),
        1 => match_node_dst4B_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dst4B_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 1");
    1
}

fn match_node_dst4B_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 2");
    2
}

fn match_node_dst4B_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 3");
    3
}

fn match_node_dst4B_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_dst4B_afterSrc4_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_dst4L_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 3;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_dst4L_1(bytes, ctx),
        1 => match_node_dst4L_2(bytes, ctx),
        2 => match_node_dst4L_5(bytes, ctx),
        3 => match_node_dst4L_6(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dst4L_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_dst4L_2(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 2: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_dst4L_3(bytes, ctx),
        1 => match_node_dst4L_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dst4L_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 1");
    1
}

fn match_node_dst4L_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 2");
    2
}

fn match_node_dst4L_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 3");
    3
}

fn match_node_dst4L_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_dst4L_afterSrc4_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_dst4T_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 3;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_dst4T_1(bytes, ctx),
        1 => match_node_dst4T_2(bytes, ctx),
        2 => match_node_dst4T_3(bytes, ctx),
        3 => match_node_dst4T_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dst4T_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched NOTHING");
    -1
}

fn match_node_dst4T_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_dst4T_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 1");
    1
}

fn match_node_dst4T_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_dst4W_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 3;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_dst4W_1(bytes, ctx),
        1 => match_node_dst4W_2(bytes, ctx),
        2 => match_node_dst4W_5(bytes, ctx),
        3 => match_node_dst4W_6(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dst4W_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_dst4W_2(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 2: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_dst4W_3(bytes, ctx),
        1 => match_node_dst4W_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dst4W_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 1");
    1
}

fn match_node_dst4W_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 2");
    2
}

fn match_node_dst4W_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 3");
    3
}

fn match_node_dst4W_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_dst4W_afterSrc4_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_dst4dsp16_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_dst4dsp8_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 1");
    1
}

fn match_node_flagBit_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 7;
    eprintln!("Trace node 0: SlaInstructionBits start=1, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_flagBit_1(bytes, ctx),
        1 => match_node_flagBit_2(bytes, ctx),
        2 => match_node_flagBit_3(bytes, ctx),
        3 => match_node_flagBit_4(bytes, ctx),
        4 => match_node_flagBit_5(bytes, ctx),
        5 => match_node_flagBit_6(bytes, ctx),
        6 => match_node_flagBit_7(bytes, ctx),
        7 => match_node_flagBit_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_flagBit_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_flagBit_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_flagBit_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_flagBit_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_flagBit_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 4");
    4
}

fn match_node_flagBit_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_flagBit_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 6");
    6
}

fn match_node_flagBit_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 7");
    7
}

fn match_node_instruction_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 15;
    eprintln!("Trace node 0: SlaInstructionBits start=0, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1(bytes, ctx),
        1 => match_node_instruction_6(bytes, ctx),
        2 => match_node_instruction_9(bytes, ctx),
        3 => match_node_instruction_12(bytes, ctx),
        4 => match_node_instruction_15(bytes, ctx),
        5 => match_node_instruction_18(bytes, ctx),
        6 => match_node_instruction_21(bytes, ctx),
        7 => match_node_instruction_24(bytes, ctx),
        8 => match_node_instruction_219(bytes, ctx),
        9 => match_node_instruction_234(bytes, ctx),
        10 => match_node_instruction_249(bytes, ctx),
        11 => match_node_instruction_264(bytes, ctx),
        12 => match_node_instruction_279(bytes, ctx),
        13 => match_node_instruction_294(bytes, ctx),
        14 => match_node_instruction_309(bytes, ctx),
        15 => match_node_instruction_410(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 1: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_2(bytes, ctx),
        1 => match_node_instruction_5(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_2(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 1;
    eprintln!("Trace node 2: SlaInstructionBits start=5, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_3(bytes, ctx),
        1 => match_node_instruction_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 67");
    67
}

fn match_node_instruction_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 207");
    207
}

fn match_node_instruction_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 182");
    182
}

fn match_node_instruction_6(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 6: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_7(bytes, ctx),
        1 => match_node_instruction_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 36");
    36
}

fn match_node_instruction_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 218");
    218
}

fn match_node_instruction_9(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 9: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_10(bytes, ctx),
        1 => match_node_instruction_11(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 22");
    22
}

fn match_node_instruction_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 310");
    310
}

fn match_node_instruction_12(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 12: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_13(bytes, ctx),
        1 => match_node_instruction_14(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 180");
    180
}

fn match_node_instruction_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 95");
    95
}

fn match_node_instruction_15(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 15: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_16(bytes, ctx),
        1 => match_node_instruction_17(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 43");
    43
}

fn match_node_instruction_17(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 17: Terminal matched constructor ID 71");
    71
}

fn match_node_instruction_18(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 18: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_19(bytes, ctx),
        1 => match_node_instruction_20(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_19(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 19: Terminal matched constructor ID 57");
    57
}

fn match_node_instruction_20(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 20: Terminal matched constructor ID 75");
    75
}

fn match_node_instruction_21(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 21: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_22(bytes, ctx),
        1 => match_node_instruction_23(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_22(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 22: Terminal matched constructor ID 139");
    139
}

fn match_node_instruction_23(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 23: Terminal matched constructor ID 137");
    137
}

fn match_node_instruction_24(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 15;
    eprintln!("Trace node 24: SlaInstructionBits start=4, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_25(bytes, ctx),
        1 => match_node_instruction_26(bytes, ctx),
        2 => match_node_instruction_27(bytes, ctx),
        3 => match_node_instruction_28(bytes, ctx),
        4 => match_node_instruction_29(bytes, ctx),
        5 => match_node_instruction_46(bytes, ctx),
        6 => match_node_instruction_63(bytes, ctx),
        7 => match_node_instruction_80(bytes, ctx),
        8 => match_node_instruction_97(bytes, ctx),
        9 => match_node_instruction_98(bytes, ctx),
        10 => match_node_instruction_99(bytes, ctx),
        11 => match_node_instruction_102(bytes, ctx),
        12 => match_node_instruction_105(bytes, ctx),
        13 => match_node_instruction_144(bytes, ctx),
        14 => match_node_instruction_185(bytes, ctx),
        15 => match_node_instruction_218(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_25(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 25: Terminal matched constructor ID 203");
    203
}

fn match_node_instruction_26(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 26: Terminal matched constructor ID 204");
    204
}

fn match_node_instruction_27(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 27: Terminal matched constructor ID 178");
    178
}

fn match_node_instruction_28(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 28: Terminal matched constructor ID 179");
    179
}

fn match_node_instruction_29(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 15;
    eprintln!("Trace node 29: SlaInstructionBits start=8, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_30(bytes, ctx),
        1 => match_node_instruction_31(bytes, ctx),
        2 => match_node_instruction_32(bytes, ctx),
        3 => match_node_instruction_33(bytes, ctx),
        4 => match_node_instruction_34(bytes, ctx),
        5 => match_node_instruction_35(bytes, ctx),
        6 => match_node_instruction_36(bytes, ctx),
        7 => match_node_instruction_37(bytes, ctx),
        8 => match_node_instruction_38(bytes, ctx),
        9 => match_node_instruction_39(bytes, ctx),
        10 => match_node_instruction_40(bytes, ctx),
        11 => match_node_instruction_41(bytes, ctx),
        12 => match_node_instruction_42(bytes, ctx),
        13 => match_node_instruction_43(bytes, ctx),
        14 => match_node_instruction_44(bytes, ctx),
        15 => match_node_instruction_45(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_30(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 30: Terminal matched constructor ID 294");
    294
}

fn match_node_instruction_31(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 31: Terminal matched constructor ID 296");
    296
}

fn match_node_instruction_32(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 32: Terminal matched constructor ID 298");
    298
}

fn match_node_instruction_33(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 33: Terminal matched constructor ID 186");
    186
}

fn match_node_instruction_34(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 34: Terminal matched constructor ID 228");
    228
}

fn match_node_instruction_35(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 35: Terminal matched constructor ID 205");
    205
}

fn match_node_instruction_36(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 36: Terminal matched constructor ID 251");
    251
}

fn match_node_instruction_37(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 37: Terminal matched constructor ID 208");
    208
}

fn match_node_instruction_38(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 38: Terminal matched constructor ID 157");
    157
}

fn match_node_instruction_39(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 39: Terminal matched constructor ID 160");
    160
}

fn match_node_instruction_40(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 40: Terminal matched constructor ID 163");
    163
}

fn match_node_instruction_41(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 41: Terminal matched constructor ID 184");
    184
}

fn match_node_instruction_42(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 42: Terminal matched constructor ID 168");
    168
}

fn match_node_instruction_43(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 43: Terminal matched constructor ID 220");
    220
}

fn match_node_instruction_44(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 44: Terminal matched constructor ID 279");
    279
}

fn match_node_instruction_45(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 45: Terminal matched constructor ID 267");
    267
}

fn match_node_instruction_46(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 15;
    eprintln!("Trace node 46: SlaInstructionBits start=8, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_47(bytes, ctx),
        1 => match_node_instruction_48(bytes, ctx),
        2 => match_node_instruction_49(bytes, ctx),
        3 => match_node_instruction_50(bytes, ctx),
        4 => match_node_instruction_51(bytes, ctx),
        5 => match_node_instruction_52(bytes, ctx),
        6 => match_node_instruction_53(bytes, ctx),
        7 => match_node_instruction_54(bytes, ctx),
        8 => match_node_instruction_55(bytes, ctx),
        9 => match_node_instruction_56(bytes, ctx),
        10 => match_node_instruction_57(bytes, ctx),
        11 => match_node_instruction_58(bytes, ctx),
        12 => match_node_instruction_59(bytes, ctx),
        13 => match_node_instruction_60(bytes, ctx),
        14 => match_node_instruction_61(bytes, ctx),
        15 => match_node_instruction_62(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_47(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 47: Terminal matched constructor ID 295");
    295
}

fn match_node_instruction_48(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 48: Terminal matched constructor ID 297");
    297
}

fn match_node_instruction_49(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 49: Terminal matched constructor ID 299");
    299
}

fn match_node_instruction_50(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 50: Terminal matched constructor ID 187");
    187
}

fn match_node_instruction_51(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 51: Terminal matched constructor ID 229");
    229
}

fn match_node_instruction_52(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 52: Terminal matched constructor ID 206");
    206
}

fn match_node_instruction_53(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 53: Terminal matched constructor ID 252");
    252
}

fn match_node_instruction_54(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 54: Terminal matched constructor ID 209");
    209
}

fn match_node_instruction_55(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 55: Terminal matched constructor ID 158");
    158
}

fn match_node_instruction_56(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 56: Terminal matched constructor ID 161");
    161
}

fn match_node_instruction_57(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 57: Terminal matched constructor ID 164");
    164
}

fn match_node_instruction_58(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 58: Terminal matched constructor ID 185");
    185
}

fn match_node_instruction_59(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 59: Terminal matched constructor ID 169");
    169
}

fn match_node_instruction_60(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 60: Terminal matched constructor ID 221");
    221
}

fn match_node_instruction_61(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 61: Terminal matched constructor ID 280");
    280
}

fn match_node_instruction_62(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 62: Terminal matched constructor ID 268");
    268
}

fn match_node_instruction_63(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 15;
    eprintln!("Trace node 63: SlaInstructionBits start=8, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_64(bytes, ctx),
        1 => match_node_instruction_65(bytes, ctx),
        2 => match_node_instruction_66(bytes, ctx),
        3 => match_node_instruction_67(bytes, ctx),
        4 => match_node_instruction_68(bytes, ctx),
        5 => match_node_instruction_69(bytes, ctx),
        6 => match_node_instruction_70(bytes, ctx),
        7 => match_node_instruction_71(bytes, ctx),
        8 => match_node_instruction_72(bytes, ctx),
        9 => match_node_instruction_73(bytes, ctx),
        10 => match_node_instruction_74(bytes, ctx),
        11 => match_node_instruction_75(bytes, ctx),
        12 => match_node_instruction_76(bytes, ctx),
        13 => match_node_instruction_77(bytes, ctx),
        14 => match_node_instruction_78(bytes, ctx),
        15 => match_node_instruction_79(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_64(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 64: Terminal matched constructor ID 311");
    311
}

fn match_node_instruction_65(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 65: Terminal matched constructor ID 320");
    320
}

fn match_node_instruction_66(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 66: Terminal matched constructor ID 30");
    30
}

fn match_node_instruction_67(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 67: Terminal matched constructor ID 212");
    212
}

fn match_node_instruction_68(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 68: Terminal matched constructor ID 13");
    13
}

fn match_node_instruction_69(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 69: Terminal matched constructor ID 304");
    304
}

fn match_node_instruction_70(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 70: Terminal matched constructor ID 4");
    4
}

fn match_node_instruction_71(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 71: Terminal matched constructor ID 255");
    255
}

fn match_node_instruction_72(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 72: Terminal matched constructor ID 86");
    86
}

fn match_node_instruction_73(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 73: Terminal matched constructor ID 116");
    116
}

fn match_node_instruction_74(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 74: Terminal matched constructor ID 239");
    239
}

fn match_node_instruction_75(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 75: Terminal matched constructor ID 242");
    242
}

fn match_node_instruction_76(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 76: Terminal matched constructor ID 112");
    112
}

fn match_node_instruction_77(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 77: Terminal matched constructor ID 108");
    108
}

fn match_node_instruction_78(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 78: Terminal matched constructor ID 10");
    10
}

fn match_node_instruction_79(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 79: Terminal matched constructor ID 1");
    1
}

fn match_node_instruction_80(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 15;
    eprintln!("Trace node 80: SlaInstructionBits start=8, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_81(bytes, ctx),
        1 => match_node_instruction_82(bytes, ctx),
        2 => match_node_instruction_83(bytes, ctx),
        3 => match_node_instruction_84(bytes, ctx),
        4 => match_node_instruction_85(bytes, ctx),
        5 => match_node_instruction_86(bytes, ctx),
        6 => match_node_instruction_87(bytes, ctx),
        7 => match_node_instruction_88(bytes, ctx),
        8 => match_node_instruction_89(bytes, ctx),
        9 => match_node_instruction_90(bytes, ctx),
        10 => match_node_instruction_91(bytes, ctx),
        11 => match_node_instruction_92(bytes, ctx),
        12 => match_node_instruction_93(bytes, ctx),
        13 => match_node_instruction_94(bytes, ctx),
        14 => match_node_instruction_95(bytes, ctx),
        15 => match_node_instruction_96(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_81(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 81: Terminal matched constructor ID 312");
    312
}

fn match_node_instruction_82(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 82: Terminal matched constructor ID 321");
    321
}

fn match_node_instruction_83(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 83: Terminal matched constructor ID 31");
    31
}

fn match_node_instruction_84(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 84: Terminal matched constructor ID 213");
    213
}

fn match_node_instruction_85(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 85: Terminal matched constructor ID 14");
    14
}

fn match_node_instruction_86(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 86: Terminal matched constructor ID 305");
    305
}

fn match_node_instruction_87(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 87: Terminal matched constructor ID 5");
    5
}

fn match_node_instruction_88(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 88: Terminal matched constructor ID 256");
    256
}

fn match_node_instruction_89(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 89: Terminal matched constructor ID 87");
    87
}

fn match_node_instruction_90(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 90: Terminal matched constructor ID 117");
    117
}

fn match_node_instruction_91(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 91: Terminal matched constructor ID 240");
    240
}

fn match_node_instruction_92(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 92: Terminal matched constructor ID 243");
    243
}

fn match_node_instruction_93(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 93: Terminal matched constructor ID 113");
    113
}

fn match_node_instruction_94(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 94: Terminal matched constructor ID 109");
    109
}

fn match_node_instruction_95(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 95: Terminal matched constructor ID 11");
    11
}

fn match_node_instruction_96(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 96: Terminal matched constructor ID 2");
    2
}

fn match_node_instruction_97(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 97: Terminal matched constructor ID 199");
    199
}

fn match_node_instruction_98(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 98: Terminal matched constructor ID 200");
    200
}

fn match_node_instruction_99(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 99: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_100(bytes, ctx),
        1 => match_node_instruction_101(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_100(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 100: Terminal matched constructor ID 317");
    317
}

fn match_node_instruction_101(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 101: Terminal matched constructor ID 154");
    154
}

fn match_node_instruction_102(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 102: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_103(bytes, ctx),
        1 => match_node_instruction_104(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_103(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 103: Terminal matched constructor ID 318");
    318
}

fn match_node_instruction_104(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 104: Terminal matched constructor ID 290");
    290
}

fn match_node_instruction_105(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 15;
    eprintln!("Trace node 105: SlaInstructionBits start=8, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_106(bytes, ctx),
        1 => match_node_instruction_107(bytes, ctx),
        2 => match_node_instruction_108(bytes, ctx),
        3 => match_node_instruction_109(bytes, ctx),
        4 => match_node_instruction_110(bytes, ctx),
        5 => match_node_instruction_111(bytes, ctx),
        6 => match_node_instruction_112(bytes, ctx),
        7 => match_node_instruction_113(bytes, ctx),
        8 => match_node_instruction_114(bytes, ctx),
        9 => match_node_instruction_115(bytes, ctx),
        10 => match_node_instruction_116(bytes, ctx),
        11 => match_node_instruction_117(bytes, ctx),
        12 => match_node_instruction_118(bytes, ctx),
        13 => match_node_instruction_121(bytes, ctx),
        14 => match_node_instruction_122(bytes, ctx),
        15 => match_node_instruction_139(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_106(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 106: Terminal matched constructor ID 193");
    193
}

fn match_node_instruction_107(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 107: Terminal matched constructor ID 194");
    194
}

fn match_node_instruction_108(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 108: Terminal matched constructor ID 195");
    195
}

fn match_node_instruction_109(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 109: Terminal matched constructor ID 196");
    196
}

fn match_node_instruction_110(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 110: Terminal matched constructor ID 201");
    201
}

fn match_node_instruction_111(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 111: Terminal matched constructor ID 197");
    197
}

fn match_node_instruction_112(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 112: Terminal matched constructor ID 129");
    129
}

fn match_node_instruction_113(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 113: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_114(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 114: Terminal matched constructor ID 189");
    189
}

fn match_node_instruction_115(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 115: Terminal matched constructor ID 190");
    190
}

fn match_node_instruction_116(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 116: Terminal matched constructor ID 191");
    191
}

fn match_node_instruction_117(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 117: Terminal matched constructor ID 192");
    192
}

fn match_node_instruction_118(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 118: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_119(bytes, ctx),
        1 => match_node_instruction_120(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_119(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 119: Terminal matched constructor ID 292");
    292
}

fn match_node_instruction_120(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 120: Terminal matched constructor ID 292");
    292
}

fn match_node_instruction_121(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 121: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_122(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 15;
    eprintln!("Trace node 122: SlaInstructionBits start=12, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_123(bytes, ctx),
        1 => match_node_instruction_124(bytes, ctx),
        2 => match_node_instruction_125(bytes, ctx),
        3 => match_node_instruction_126(bytes, ctx),
        4 => match_node_instruction_127(bytes, ctx),
        5 => match_node_instruction_128(bytes, ctx),
        6 => match_node_instruction_129(bytes, ctx),
        7 => match_node_instruction_130(bytes, ctx),
        8 => match_node_instruction_131(bytes, ctx),
        9 => match_node_instruction_132(bytes, ctx),
        10 => match_node_instruction_133(bytes, ctx),
        11 => match_node_instruction_134(bytes, ctx),
        12 => match_node_instruction_135(bytes, ctx),
        13 => match_node_instruction_136(bytes, ctx),
        14 => match_node_instruction_137(bytes, ctx),
        15 => match_node_instruction_138(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_123(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 123: Terminal matched constructor ID 110");
    110
}

fn match_node_instruction_124(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 124: Terminal matched constructor ID 106");
    106
}

fn match_node_instruction_125(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 125: Terminal matched constructor ID 226");
    226
}

fn match_node_instruction_126(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 126: Terminal matched constructor ID 114");
    114
}

fn match_node_instruction_127(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 127: Terminal matched constructor ID 102");
    102
}

fn match_node_instruction_128(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 128: Terminal matched constructor ID 124");
    124
}

fn match_node_instruction_129(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 129: Terminal matched constructor ID 98");
    98
}

fn match_node_instruction_130(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 130: Terminal matched constructor ID 120");
    120
}

fn match_node_instruction_131(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 131: Terminal matched constructor ID 286");
    286
}

fn match_node_instruction_132(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 132: Terminal matched constructor ID 284");
    284
}

fn match_node_instruction_133(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 133: Terminal matched constructor ID 288");
    288
}

fn match_node_instruction_134(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 134: Terminal matched constructor ID 23");
    23
}

fn match_node_instruction_135(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 135: Terminal matched constructor ID 100");
    100
}

fn match_node_instruction_136(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 136: Terminal matched constructor ID 122");
    122
}

fn match_node_instruction_137(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 137: Terminal matched constructor ID 96");
    96
}

fn match_node_instruction_138(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 138: Terminal matched constructor ID 118");
    118
}

fn match_node_instruction_139(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 3;
    eprintln!("Trace node 139: SlaInstructionBits start=14, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_140(bytes, ctx),
        1 => match_node_instruction_141(bytes, ctx),
        2 => match_node_instruction_142(bytes, ctx),
        3 => match_node_instruction_143(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_140(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 140: Terminal matched constructor ID 155");
    155
}

fn match_node_instruction_141(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 141: Terminal matched constructor ID 236");
    236
}

fn match_node_instruction_142(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 142: Terminal matched constructor ID 126");
    126
}

fn match_node_instruction_143(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 143: Terminal matched constructor ID 130");
    130
}

fn match_node_instruction_144(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 15;
    eprintln!("Trace node 144: SlaInstructionBits start=8, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_145(bytes, ctx),
        1 => match_node_instruction_148(bytes, ctx),
        2 => match_node_instruction_151(bytes, ctx),
        3 => match_node_instruction_152(bytes, ctx),
        4 => match_node_instruction_153(bytes, ctx),
        5 => match_node_instruction_154(bytes, ctx),
        6 => match_node_instruction_155(bytes, ctx),
        7 => match_node_instruction_156(bytes, ctx),
        8 => match_node_instruction_157(bytes, ctx),
        9 => match_node_instruction_158(bytes, ctx),
        10 => match_node_instruction_159(bytes, ctx),
        11 => match_node_instruction_160(bytes, ctx),
        12 => match_node_instruction_161(bytes, ctx),
        13 => match_node_instruction_162(bytes, ctx),
        14 => match_node_instruction_163(bytes, ctx),
        15 => match_node_instruction_180(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_145(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 145: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_146(bytes, ctx),
        1 => match_node_instruction_147(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_146(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 146: Terminal matched constructor ID 145");
    145
}

fn match_node_instruction_147(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 147: Terminal matched constructor ID 145");
    145
}

fn match_node_instruction_148(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 148: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_149(bytes, ctx),
        1 => match_node_instruction_150(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_149(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 149: Terminal matched constructor ID 151");
    151
}

fn match_node_instruction_150(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 150: Terminal matched constructor ID 151");
    151
}

fn match_node_instruction_151(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 151: Terminal matched constructor ID 143");
    143
}

fn match_node_instruction_152(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 152: Terminal matched constructor ID 149");
    149
}

fn match_node_instruction_153(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 153: Terminal matched constructor ID 202");
    202
}

fn match_node_instruction_154(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 154: Terminal matched constructor ID 198");
    198
}

fn match_node_instruction_155(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 155: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_156(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 156: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_157(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 157: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_158(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 158: Terminal matched constructor ID 232");
    232
}

fn match_node_instruction_159(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 159: Terminal matched constructor ID 166");
    166
}

fn match_node_instruction_160(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 160: Terminal matched constructor ID 25");
    25
}

fn match_node_instruction_161(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 161: Terminal matched constructor ID 138");
    138
}

fn match_node_instruction_162(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 162: Terminal matched constructor ID 47");
    47
}

fn match_node_instruction_163(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 15;
    eprintln!("Trace node 163: SlaInstructionBits start=12, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_164(bytes, ctx),
        1 => match_node_instruction_165(bytes, ctx),
        2 => match_node_instruction_166(bytes, ctx),
        3 => match_node_instruction_167(bytes, ctx),
        4 => match_node_instruction_168(bytes, ctx),
        5 => match_node_instruction_169(bytes, ctx),
        6 => match_node_instruction_170(bytes, ctx),
        7 => match_node_instruction_171(bytes, ctx),
        8 => match_node_instruction_172(bytes, ctx),
        9 => match_node_instruction_173(bytes, ctx),
        10 => match_node_instruction_174(bytes, ctx),
        11 => match_node_instruction_175(bytes, ctx),
        12 => match_node_instruction_176(bytes, ctx),
        13 => match_node_instruction_177(bytes, ctx),
        14 => match_node_instruction_178(bytes, ctx),
        15 => match_node_instruction_179(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_164(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 164: Terminal matched constructor ID 111");
    111
}

fn match_node_instruction_165(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 165: Terminal matched constructor ID 107");
    107
}

fn match_node_instruction_166(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 166: Terminal matched constructor ID 227");
    227
}

fn match_node_instruction_167(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 167: Terminal matched constructor ID 115");
    115
}

fn match_node_instruction_168(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 168: Terminal matched constructor ID 103");
    103
}

fn match_node_instruction_169(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 169: Terminal matched constructor ID 125");
    125
}

fn match_node_instruction_170(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 170: Terminal matched constructor ID 99");
    99
}

fn match_node_instruction_171(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 171: Terminal matched constructor ID 121");
    121
}

fn match_node_instruction_172(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 172: Terminal matched constructor ID 287");
    287
}

fn match_node_instruction_173(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 173: Terminal matched constructor ID 285");
    285
}

fn match_node_instruction_174(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 174: Terminal matched constructor ID 289");
    289
}

fn match_node_instruction_175(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 175: Terminal matched constructor ID 24");
    24
}

fn match_node_instruction_176(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 176: Terminal matched constructor ID 101");
    101
}

fn match_node_instruction_177(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 177: Terminal matched constructor ID 123");
    123
}

fn match_node_instruction_178(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 178: Terminal matched constructor ID 97");
    97
}

fn match_node_instruction_179(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 179: Terminal matched constructor ID 119");
    119
}

fn match_node_instruction_180(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 3;
    eprintln!("Trace node 180: SlaInstructionBits start=14, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_181(bytes, ctx),
        1 => match_node_instruction_182(bytes, ctx),
        2 => match_node_instruction_183(bytes, ctx),
        3 => match_node_instruction_184(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_181(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 181: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_182(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 182: Terminal matched constructor ID 237");
    237
}

fn match_node_instruction_183(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 183: Terminal matched constructor ID 127");
    127
}

fn match_node_instruction_184(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 184: Terminal matched constructor ID 315");
    315
}

fn match_node_instruction_185(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 5 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 31;
    eprintln!("Trace node 185: SlaInstructionBits start=8, size=5, word={:08x}, probe={}", word, probe);
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
        16 => match_node_instruction_202(bytes, ctx),
        17 => match_node_instruction_203(bytes, ctx),
        18 => match_node_instruction_204(bytes, ctx),
        19 => match_node_instruction_205(bytes, ctx),
        20 => match_node_instruction_206(bytes, ctx),
        21 => match_node_instruction_207(bytes, ctx),
        22 => match_node_instruction_208(bytes, ctx),
        23 => match_node_instruction_209(bytes, ctx),
        24 => match_node_instruction_210(bytes, ctx),
        25 => match_node_instruction_211(bytes, ctx),
        26 => match_node_instruction_212(bytes, ctx),
        27 => match_node_instruction_213(bytes, ctx),
        28 => match_node_instruction_214(bytes, ctx),
        29 => match_node_instruction_215(bytes, ctx),
        30 => match_node_instruction_216(bytes, ctx),
        31 => match_node_instruction_217(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_186(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 186: Terminal matched constructor ID 77");
    77
}

fn match_node_instruction_187(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 187: Terminal matched constructor ID 78");
    78
}

fn match_node_instruction_188(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 188: Terminal matched constructor ID 80");
    80
}

fn match_node_instruction_189(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 189: Terminal matched constructor ID 81");
    81
}

fn match_node_instruction_190(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 190: Terminal matched constructor ID 45");
    45
}

fn match_node_instruction_191(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 191: Terminal matched constructor ID 46");
    46
}

fn match_node_instruction_192(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 192: Terminal matched constructor ID 59");
    59
}

fn match_node_instruction_193(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 193: Terminal matched constructor ID 60");
    60
}

fn match_node_instruction_194(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 194: Terminal matched constructor ID 38");
    38
}

fn match_node_instruction_195(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 195: Terminal matched constructor ID 39");
    39
}

fn match_node_instruction_196(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 196: Terminal matched constructor ID 49");
    49
}

fn match_node_instruction_197(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 197: Terminal matched constructor ID 50");
    50
}

fn match_node_instruction_198(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 198: Terminal matched constructor ID 65");
    65
}

fn match_node_instruction_199(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 199: Terminal matched constructor ID 66");
    66
}

fn match_node_instruction_200(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 200: Terminal matched constructor ID 52");
    52
}

fn match_node_instruction_201(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 201: Terminal matched constructor ID 53");
    53
}

fn match_node_instruction_202(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 202: Terminal matched constructor ID 41");
    41
}

fn match_node_instruction_203(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 203: Terminal matched constructor ID 42");
    42
}

fn match_node_instruction_204(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 204: Terminal matched constructor ID 69");
    69
}

fn match_node_instruction_205(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 205: Terminal matched constructor ID 70");
    70
}

fn match_node_instruction_206(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 206: Terminal matched constructor ID 55");
    55
}

fn match_node_instruction_207(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 207: Terminal matched constructor ID 56");
    56
}

fn match_node_instruction_208(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 208: Terminal matched constructor ID 73");
    73
}

fn match_node_instruction_209(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 209: Terminal matched constructor ID 74");
    74
}

fn match_node_instruction_210(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 210: Terminal matched constructor ID 83");
    83
}

fn match_node_instruction_211(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 211: Terminal matched constructor ID 84");
    84
}

fn match_node_instruction_212(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 212: Terminal matched constructor ID 62");
    62
}

fn match_node_instruction_213(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 213: Terminal matched constructor ID 63");
    63
}

fn match_node_instruction_214(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 214: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_215(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 215: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_216(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 216: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_217(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 217: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_218(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 218: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_219(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 1;
    eprintln!("Trace node 219: SlaInstructionBits start=5, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_220(bytes, ctx),
        1 => match_node_instruction_231(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_220(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 3;
    eprintln!("Trace node 220: SlaInstructionBits start=6, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_221(bytes, ctx),
        1 => match_node_instruction_224(bytes, ctx),
        2 => match_node_instruction_227(bytes, ctx),
        3 => match_node_instruction_228(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_221(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 221: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_222(bytes, ctx),
        1 => match_node_instruction_223(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_222(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 222: Terminal matched constructor ID 313");
    313
}

fn match_node_instruction_223(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 223: Terminal matched constructor ID 323");
    323
}

fn match_node_instruction_224(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 224: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_225(bytes, ctx),
        1 => match_node_instruction_226(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_225(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 225: Terminal matched constructor ID 314");
    314
}

fn match_node_instruction_226(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 226: Terminal matched constructor ID 324");
    324
}

fn match_node_instruction_227(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 227: Terminal matched constructor ID 230");
    230
}

fn match_node_instruction_228(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 228: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_229(bytes, ctx),
        1 => match_node_instruction_230(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_229(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 229: Terminal matched constructor ID 18");
    18
}

fn match_node_instruction_230(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 230: Terminal matched constructor ID 306");
    306
}

fn match_node_instruction_231(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 231: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_232(bytes, ctx),
        1 => match_node_instruction_233(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_232(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 232: Terminal matched constructor ID 18");
    18
}

fn match_node_instruction_233(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 233: Terminal matched constructor ID 306");
    306
}

fn match_node_instruction_234(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 1;
    eprintln!("Trace node 234: SlaInstructionBits start=5, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_235(bytes, ctx),
        1 => match_node_instruction_246(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_235(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 3;
    eprintln!("Trace node 235: SlaInstructionBits start=6, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_236(bytes, ctx),
        1 => match_node_instruction_239(bytes, ctx),
        2 => match_node_instruction_242(bytes, ctx),
        3 => match_node_instruction_243(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_236(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 236: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_237(bytes, ctx),
        1 => match_node_instruction_238(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_237(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 237: Terminal matched constructor ID 34");
    34
}

fn match_node_instruction_238(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 238: Terminal matched constructor ID 216");
    216
}

fn match_node_instruction_239(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 239: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_240(bytes, ctx),
        1 => match_node_instruction_241(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_240(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 240: Terminal matched constructor ID 35");
    35
}

fn match_node_instruction_241(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 241: Terminal matched constructor ID 217");
    217
}

fn match_node_instruction_242(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 242: Terminal matched constructor ID 222");
    222
}

fn match_node_instruction_243(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 243: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_244(bytes, ctx),
        1 => match_node_instruction_245(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_244(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 244: Terminal matched constructor ID 32");
    32
}

fn match_node_instruction_245(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 245: Terminal matched constructor ID 214");
    214
}

fn match_node_instruction_246(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 246: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_247(bytes, ctx),
        1 => match_node_instruction_248(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_247(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 247: Terminal matched constructor ID 32");
    32
}

fn match_node_instruction_248(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 248: Terminal matched constructor ID 214");
    214
}

fn match_node_instruction_249(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 1;
    eprintln!("Trace node 249: SlaInstructionBits start=5, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_250(bytes, ctx),
        1 => match_node_instruction_261(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_250(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 3;
    eprintln!("Trace node 250: SlaInstructionBits start=6, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_251(bytes, ctx),
        1 => match_node_instruction_254(bytes, ctx),
        2 => match_node_instruction_257(bytes, ctx),
        3 => match_node_instruction_258(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_251(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 251: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_252(bytes, ctx),
        1 => match_node_instruction_253(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_252(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 252: Terminal matched constructor ID 20");
    20
}

fn match_node_instruction_253(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 253: Terminal matched constructor ID 308");
    308
}

fn match_node_instruction_254(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 254: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_255(bytes, ctx),
        1 => match_node_instruction_256(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_255(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 255: Terminal matched constructor ID 21");
    21
}

fn match_node_instruction_256(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 256: Terminal matched constructor ID 309");
    309
}

fn match_node_instruction_257(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 257: Terminal matched constructor ID 175");
    175
}

fn match_node_instruction_258(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 258: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_259(bytes, ctx),
        1 => match_node_instruction_260(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_259(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 259: Terminal matched constructor ID 133");
    133
}

fn match_node_instruction_260(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 260: Terminal matched constructor ID 104");
    104
}

fn match_node_instruction_261(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 261: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_262(bytes, ctx),
        1 => match_node_instruction_263(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_262(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 262: Terminal matched constructor ID 133");
    133
}

fn match_node_instruction_263(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 263: Terminal matched constructor ID 104");
    104
}

fn match_node_instruction_264(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 1;
    eprintln!("Trace node 264: SlaInstructionBits start=5, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_265(bytes, ctx),
        1 => match_node_instruction_276(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_265(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 3;
    eprintln!("Trace node 265: SlaInstructionBits start=6, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_266(bytes, ctx),
        1 => match_node_instruction_269(bytes, ctx),
        2 => match_node_instruction_272(bytes, ctx),
        3 => match_node_instruction_273(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_266(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 266: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_267(bytes, ctx),
        1 => match_node_instruction_268(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_267(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 267: Terminal matched constructor ID 7");
    7
}

fn match_node_instruction_268(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 268: Terminal matched constructor ID 258");
    258
}

fn match_node_instruction_269(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 269: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_270(bytes, ctx),
        1 => match_node_instruction_271(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_270(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 270: Terminal matched constructor ID 8");
    8
}

fn match_node_instruction_271(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 271: Terminal matched constructor ID 259");
    259
}

fn match_node_instruction_272(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 272: Terminal matched constructor ID 134");
    134
}

fn match_node_instruction_273(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 273: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_274(bytes, ctx),
        1 => match_node_instruction_275(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_274(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 274: Terminal matched constructor ID 176");
    176
}

fn match_node_instruction_275(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 275: Terminal matched constructor ID 210");
    210
}

fn match_node_instruction_276(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 276: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_277(bytes, ctx),
        1 => match_node_instruction_278(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_277(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 277: Terminal matched constructor ID 293");
    293
}

fn match_node_instruction_278(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 278: Terminal matched constructor ID 210");
    210
}

fn match_node_instruction_279(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 1;
    eprintln!("Trace node 279: SlaInstructionBits start=5, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_280(bytes, ctx),
        1 => match_node_instruction_291(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_280(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 3;
    eprintln!("Trace node 280: SlaInstructionBits start=6, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_281(bytes, ctx),
        1 => match_node_instruction_284(bytes, ctx),
        2 => match_node_instruction_287(bytes, ctx),
        3 => match_node_instruction_288(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_281(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 281: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_282(bytes, ctx),
        1 => match_node_instruction_283(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_282(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 282: Terminal matched constructor ID 93");
    93
}

fn match_node_instruction_283(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 283: Terminal matched constructor ID 16");
    16
}

fn match_node_instruction_284(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 284: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_285(bytes, ctx),
        1 => match_node_instruction_286(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_285(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 285: Terminal matched constructor ID 94");
    94
}

fn match_node_instruction_286(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 286: Terminal matched constructor ID 17");
    17
}

fn match_node_instruction_287(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 287: Terminal matched constructor ID 231");
    231
}

fn match_node_instruction_288(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 288: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_289(bytes, ctx),
        1 => match_node_instruction_290(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_289(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 289: Terminal matched constructor ID 173");
    173
}

fn match_node_instruction_290(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 290: Terminal matched constructor ID 301");
    301
}

fn match_node_instruction_291(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 291: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_292(bytes, ctx),
        1 => match_node_instruction_293(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_292(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 292: Terminal matched constructor ID 173");
    173
}

fn match_node_instruction_293(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 293: Terminal matched constructor ID 301");
    301
}

fn match_node_instruction_294(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 1;
    eprintln!("Trace node 294: SlaInstructionBits start=5, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_295(bytes, ctx),
        1 => match_node_instruction_306(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_295(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 3;
    eprintln!("Trace node 295: SlaInstructionBits start=6, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_296(bytes, ctx),
        1 => match_node_instruction_299(bytes, ctx),
        2 => match_node_instruction_302(bytes, ctx),
        3 => match_node_instruction_303(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_296(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 296: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_297(bytes, ctx),
        1 => match_node_instruction_298(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_297(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 297: Terminal matched constructor ID 89");
    89
}

fn match_node_instruction_298(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 298: Terminal matched constructor ID 171");
    171
}

fn match_node_instruction_299(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 299: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_300(bytes, ctx),
        1 => match_node_instruction_301(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_300(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 300: Terminal matched constructor ID 90");
    90
}

fn match_node_instruction_301(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 301: Terminal matched constructor ID 172");
    172
}

fn match_node_instruction_302(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 302: Terminal matched constructor ID 223");
    223
}

fn match_node_instruction_303(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 303: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_304(bytes, ctx),
        1 => match_node_instruction_305(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_304(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 304: Terminal matched constructor ID 300");
    300
}

fn match_node_instruction_305(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 305: Terminal matched constructor ID 302");
    302
}

fn match_node_instruction_306(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 306: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_307(bytes, ctx),
        1 => match_node_instruction_308(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_307(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 307: Terminal matched constructor ID 300");
    300
}

fn match_node_instruction_308(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 308: Terminal matched constructor ID 302");
    302
}

fn match_node_instruction_309(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 1;
    eprintln!("Trace node 309: SlaInstructionBits start=5, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_310(bytes, ctx),
        1 => match_node_instruction_403(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_310(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 3;
    eprintln!("Trace node 310: SlaInstructionBits start=6, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_311(bytes, ctx),
        1 => match_node_instruction_344(bytes, ctx),
        2 => match_node_instruction_377(bytes, ctx),
        3 => match_node_instruction_378(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_311(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 5 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 31;
    eprintln!("Trace node 311: SlaInstructionBits start=4, size=5, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_312(bytes, ctx),
        1 => match_node_instruction_313(bytes, ctx),
        2 => match_node_instruction_314(bytes, ctx),
        3 => match_node_instruction_315(bytes, ctx),
        4 => match_node_instruction_316(bytes, ctx),
        5 => match_node_instruction_317(bytes, ctx),
        6 => match_node_instruction_318(bytes, ctx),
        7 => match_node_instruction_319(bytes, ctx),
        8 => match_node_instruction_320(bytes, ctx),
        9 => match_node_instruction_321(bytes, ctx),
        10 => match_node_instruction_322(bytes, ctx),
        11 => match_node_instruction_323(bytes, ctx),
        12 => match_node_instruction_324(bytes, ctx),
        13 => match_node_instruction_325(bytes, ctx),
        14 => match_node_instruction_326(bytes, ctx),
        15 => match_node_instruction_327(bytes, ctx),
        16 => match_node_instruction_328(bytes, ctx),
        17 => match_node_instruction_329(bytes, ctx),
        18 => match_node_instruction_330(bytes, ctx),
        19 => match_node_instruction_331(bytes, ctx),
        20 => match_node_instruction_332(bytes, ctx),
        21 => match_node_instruction_333(bytes, ctx),
        22 => match_node_instruction_334(bytes, ctx),
        23 => match_node_instruction_335(bytes, ctx),
        24 => match_node_instruction_336(bytes, ctx),
        25 => match_node_instruction_337(bytes, ctx),
        26 => match_node_instruction_338(bytes, ctx),
        27 => match_node_instruction_339(bytes, ctx),
        28 => match_node_instruction_340(bytes, ctx),
        29 => match_node_instruction_341(bytes, ctx),
        30 => match_node_instruction_342(bytes, ctx),
        31 => match_node_instruction_343(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_312(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 312: Terminal matched constructor ID 248");
    248
}

fn match_node_instruction_313(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 313: Terminal matched constructor ID 245");
    245
}

fn match_node_instruction_314(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 314: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_315(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 315: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_316(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 316: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_317(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 317: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_318(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 318: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_319(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 319: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_320(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 320: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_321(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 321: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_322(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 322: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_323(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 323: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_324(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 324: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_325(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 325: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_326(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 326: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_327(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 327: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_328(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 328: Terminal matched constructor ID 276");
    276
}

fn match_node_instruction_329(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 329: Terminal matched constructor ID 273");
    273
}

fn match_node_instruction_330(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 330: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_331(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 331: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_332(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 332: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_333(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 333: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_334(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 334: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_335(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 335: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_336(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 336: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_337(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 337: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_338(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 338: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_339(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 339: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_340(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 340: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_341(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 341: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_342(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 342: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_343(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 343: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_344(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 5 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 31;
    eprintln!("Trace node 344: SlaInstructionBits start=4, size=5, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_345(bytes, ctx),
        1 => match_node_instruction_346(bytes, ctx),
        2 => match_node_instruction_347(bytes, ctx),
        3 => match_node_instruction_348(bytes, ctx),
        4 => match_node_instruction_349(bytes, ctx),
        5 => match_node_instruction_350(bytes, ctx),
        6 => match_node_instruction_351(bytes, ctx),
        7 => match_node_instruction_352(bytes, ctx),
        8 => match_node_instruction_353(bytes, ctx),
        9 => match_node_instruction_354(bytes, ctx),
        10 => match_node_instruction_355(bytes, ctx),
        11 => match_node_instruction_356(bytes, ctx),
        12 => match_node_instruction_357(bytes, ctx),
        13 => match_node_instruction_358(bytes, ctx),
        14 => match_node_instruction_359(bytes, ctx),
        15 => match_node_instruction_360(bytes, ctx),
        16 => match_node_instruction_361(bytes, ctx),
        17 => match_node_instruction_362(bytes, ctx),
        18 => match_node_instruction_363(bytes, ctx),
        19 => match_node_instruction_364(bytes, ctx),
        20 => match_node_instruction_365(bytes, ctx),
        21 => match_node_instruction_366(bytes, ctx),
        22 => match_node_instruction_367(bytes, ctx),
        23 => match_node_instruction_368(bytes, ctx),
        24 => match_node_instruction_369(bytes, ctx),
        25 => match_node_instruction_370(bytes, ctx),
        26 => match_node_instruction_371(bytes, ctx),
        27 => match_node_instruction_372(bytes, ctx),
        28 => match_node_instruction_373(bytes, ctx),
        29 => match_node_instruction_374(bytes, ctx),
        30 => match_node_instruction_375(bytes, ctx),
        31 => match_node_instruction_376(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_345(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 345: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_346(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 346: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_347(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 347: Terminal matched constructor ID 249");
    249
}

fn match_node_instruction_348(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 348: Terminal matched constructor ID 246");
    246
}

fn match_node_instruction_349(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 349: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_350(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 350: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_351(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 351: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_352(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 352: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_353(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 353: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_354(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 354: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_355(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 355: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_356(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 356: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_357(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 357: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_358(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 358: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_359(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 359: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_360(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 360: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_361(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 361: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_362(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 362: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_363(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 363: Terminal matched constructor ID 277");
    277
}

fn match_node_instruction_364(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 364: Terminal matched constructor ID 274");
    274
}

fn match_node_instruction_365(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 365: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_366(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 366: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_367(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 367: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_368(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 368: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_369(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 369: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_370(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 370: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_371(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 371: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_372(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 372: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_373(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 373: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_374(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 374: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_375(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 375: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_376(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 376: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_377(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 377: Terminal matched constructor ID 174");
    174
}

fn match_node_instruction_378(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 378: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_379(bytes, ctx),
        1 => match_node_instruction_380(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_379(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 379: Terminal matched constructor ID 91");
    91
}

fn match_node_instruction_380(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 380: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_381(bytes, ctx),
        1 => match_node_instruction_394(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_381(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 381: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_382(bytes, ctx),
        1 => match_node_instruction_393(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_382(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 7;
    eprintln!("Trace node 382: SlaInstructionBits start=13, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_383(bytes, ctx),
        1 => match_node_instruction_384(bytes, ctx),
        2 => match_node_instruction_387(bytes, ctx),
        3 => match_node_instruction_388(bytes, ctx),
        4 => match_node_instruction_389(bytes, ctx),
        5 => match_node_instruction_390(bytes, ctx),
        6 => match_node_instruction_391(bytes, ctx),
        7 => match_node_instruction_392(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_383(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 383: Terminal matched constructor ID 165");
    165
}

fn match_node_instruction_384(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 384: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_385(bytes, ctx),
        1 => match_node_instruction_386(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_385(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 385: Terminal matched constructor ID 283");
    283
}

fn match_node_instruction_386(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 386: Terminal matched constructor ID 271");
    271
}

fn match_node_instruction_387(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 387: Terminal matched constructor ID 233");
    233
}

fn match_node_instruction_388(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 388: Terminal matched constructor ID 224");
    224
}

fn match_node_instruction_389(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 389: Terminal matched constructor ID 132");
    132
}

fn match_node_instruction_390(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 390: Terminal matched constructor ID 131");
    131
}

fn match_node_instruction_391(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 391: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_392(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 392: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_393(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 393: Terminal matched constructor ID 188");
    188
}

fn match_node_instruction_394(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 394: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_395(bytes, ctx),
        1 => match_node_instruction_402(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_395(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 395: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_396(bytes, ctx),
        1 => match_node_instruction_399(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_396(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 396: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_397(bytes, ctx),
        1 => match_node_instruction_398(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_397(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 397: Terminal matched constructor ID 282");
    282
}

fn match_node_instruction_398(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 398: Terminal matched constructor ID 281");
    281
}

fn match_node_instruction_399(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 399: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_400(bytes, ctx),
        1 => match_node_instruction_401(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_400(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 400: Terminal matched constructor ID 270");
    270
}

fn match_node_instruction_401(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 401: Terminal matched constructor ID 269");
    269
}

fn match_node_instruction_402(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 402: Terminal matched constructor ID 135");
    135
}

fn match_node_instruction_403(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 403: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_404(bytes, ctx),
        1 => match_node_instruction_405(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_404(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 404: Terminal matched constructor ID 91");
    91
}

fn match_node_instruction_405(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 3;
    eprintln!("Trace node 405: SlaInstructionBits start=6, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_406(bytes, ctx),
        1 => match_node_instruction_407(bytes, ctx),
        2 => match_node_instruction_408(bytes, ctx),
        3 => match_node_instruction_409(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_406(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 406: Terminal matched constructor ID 234");
    234
}

fn match_node_instruction_407(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 407: Terminal matched constructor ID 225");
    225
}

fn match_node_instruction_408(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 408: Terminal matched constructor ID 146");
    146
}

fn match_node_instruction_409(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 409: Terminal matched constructor ID 152");
    152
}

fn match_node_instruction_410(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 7;
    eprintln!("Trace node 410: SlaInstructionBits start=5, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_411(bytes, ctx),
        1 => match_node_instruction_416(bytes, ctx),
        2 => match_node_instruction_421(bytes, ctx),
        3 => match_node_instruction_422(bytes, ctx),
        4 => match_node_instruction_425(bytes, ctx),
        5 => match_node_instruction_428(bytes, ctx),
        6 => match_node_instruction_431(bytes, ctx),
        7 => match_node_instruction_434(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_411(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 411: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_412(bytes, ctx),
        1 => match_node_instruction_415(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_412(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 412: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_413(bytes, ctx),
        1 => match_node_instruction_414(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_413(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 413: Terminal matched constructor ID 264");
    264
}

fn match_node_instruction_414(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 414: Terminal matched constructor ID 261");
    261
}

fn match_node_instruction_415(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 415: Terminal matched constructor ID 27");
    27
}

fn match_node_instruction_416(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 416: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_417(bytes, ctx),
        1 => match_node_instruction_420(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_417(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 417: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_418(bytes, ctx),
        1 => match_node_instruction_419(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_418(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 418: Terminal matched constructor ID 265");
    265
}

fn match_node_instruction_419(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 419: Terminal matched constructor ID 262");
    262
}

fn match_node_instruction_420(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 420: Terminal matched constructor ID 28");
    28
}

fn match_node_instruction_421(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 421: Terminal matched constructor ID 105");
    105
}

fn match_node_instruction_422(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 422: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_423(bytes, ctx),
        1 => match_node_instruction_424(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_423(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 423: Terminal matched constructor ID 253");
    253
}

fn match_node_instruction_424(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 424: Terminal matched constructor ID 235");
    235
}

fn match_node_instruction_425(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 425: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_426(bytes, ctx),
        1 => match_node_instruction_427(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_426(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 426: Terminal matched constructor ID 141");
    141
}

fn match_node_instruction_427(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 427: Terminal matched constructor ID 142");
    142
}

fn match_node_instruction_428(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 428: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_429(bytes, ctx),
        1 => match_node_instruction_430(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_429(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 429: Terminal matched constructor ID 147");
    147
}

fn match_node_instruction_430(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 430: Terminal matched constructor ID 148");
    148
}

fn match_node_instruction_431(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 431: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_432(bytes, ctx),
        1 => match_node_instruction_433(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_432(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 432: Terminal matched constructor ID 136");
    136
}

fn match_node_instruction_433(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 433: Terminal matched constructor ID 140");
    140
}

fn match_node_instruction_434(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 434: Terminal matched NOTHING");
    -1
}

fn match_node_ldIntbVal_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_memBase_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_memBase_1(bytes, ctx),
        1 => match_node_memBase_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_memBase_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_memBase_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 3");
    3
}

fn match_node_memBase11_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_memBaseAx_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_memBit_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 1");
    1
}

fn match_node_popRegA0_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_popRegA1_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_popRegFB_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=0, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_popRegFB_1(bytes, ctx),
        1 => match_node_popRegFB_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_popRegFB_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_popRegFB_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_popRegList_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_popRegR0_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_popRegR1_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_popRegR2_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_popRegR3_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_popRegSB_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_pushRegA0_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_pushRegA1_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_pushRegFB_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_pushRegList_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_pushRegR0_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=0, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_pushRegR0_1(bytes, ctx),
        1 => match_node_pushRegR0_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_pushRegR0_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_pushRegR0_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_pushRegR1_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_pushRegR2_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_pushRegR3_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_pushRegSB_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_regBase_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_regBase_1(bytes, ctx),
        1 => match_node_regBase_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_regBase_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_regBase_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_regBit_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_rel16offset1_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_rel3offset2_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_rel8offset1_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_rel8offset2_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_reloffset_dst4L_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_reloffset_dst4T_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_reloffset_dst4T_1(bytes, ctx),
        1 => match_node_reloffset_dst4T_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_reloffset_dst4T_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_reloffset_dst4T_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_reloffset_dst4W_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_skipBytesBeforeDst2_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 1");
    1
}

fn match_node_skipBytesBeforeDst4_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 0: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_skipBytesBeforeDst4_1(bytes, ctx),
        1 => match_node_skipBytesBeforeDst4_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_skipBytesBeforeDst4_1(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 1: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_skipBytesBeforeDst4_2(bytes, ctx),
        1 => match_node_skipBytesBeforeDst4_3(bytes, ctx),
        _ => -1,
    }
}

fn match_node_skipBytesBeforeDst4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_skipBytesBeforeDst4_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_skipBytesBeforeDst4_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_skipBytesBeforeImm82_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 3;
    eprintln!("Trace node 0: SlaInstructionBits start=6, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_skipBytesBeforeImm82_1(bytes, ctx),
        1 => match_node_skipBytesBeforeImm82_2(bytes, ctx),
        2 => match_node_skipBytesBeforeImm82_3(bytes, ctx),
        3 => match_node_skipBytesBeforeImm82_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_skipBytesBeforeImm82_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_skipBytesBeforeImm82_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_skipBytesBeforeImm82_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_skipBytesBeforeImm82_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_src4B_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 3;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_src4B_1(bytes, ctx),
        1 => match_node_src4B_2(bytes, ctx),
        2 => match_node_src4B_5(bytes, ctx),
        3 => match_node_src4B_6(bytes, ctx),
        _ => -1,
    }
}

fn match_node_src4B_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_src4B_2(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 2: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_src4B_3(bytes, ctx),
        1 => match_node_src4B_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_src4B_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 1");
    1
}

fn match_node_src4B_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 2");
    2
}

fn match_node_src4B_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 3");
    3
}

fn match_node_src4B_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_src4W_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 3;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_src4W_1(bytes, ctx),
        1 => match_node_src4W_2(bytes, ctx),
        2 => match_node_src4W_5(bytes, ctx),
        3 => match_node_src4W_6(bytes, ctx),
        _ => -1,
    }
}

fn match_node_src4W_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_src4W_2(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 2: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_src4W_3(bytes, ctx),
        1 => match_node_src4W_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_src4W_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 1");
    1
}

fn match_node_src4W_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 2");
    2
}

fn match_node_src4W_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 3");
    3
}

fn match_node_src4W_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_src4dsp16_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_src4dsp8_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 1");
    1
}

fn match_node_srcImm16_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_srcImm3_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_srcImm8_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_srcIntNum_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_srcSimm16_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_srcSimm4Shift_0003_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_srcSimm4Shift_0003_1(bytes, ctx),
        1 => match_node_srcSimm4Shift_0003_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_srcSimm4Shift_0003_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_srcSimm4Shift_0003_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_srcSimm4Shift_0407_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=0, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_srcSimm4Shift_0407_1(bytes, ctx),
        1 => match_node_srcSimm4Shift_0407_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_srcSimm4Shift_0407_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_srcSimm4Shift_0407_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_srcSimm4_0003_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_srcSimm4_0407_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_srcSimm8_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_srcZero8_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_steA1A0B_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_steA1A0W_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_stzxImm82_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

