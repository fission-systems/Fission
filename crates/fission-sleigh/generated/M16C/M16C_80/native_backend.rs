// Auto-generated Fission Native Backend
#[no_mangle]
pub extern "C" fn fission_decode_match(table_ptr: *const i8, bytes: *const u8, bytes_len: usize, ctx_ptr: *const u64) -> i32 {
    let table_name = unsafe { std::ffi::CStr::from_ptr(table_ptr).to_str().unwrap() };
    let bytes = unsafe { std::slice::from_raw_parts(bytes, bytes_len) };
    let ctx = unsafe { *ctx_ptr };
    match table_name {
        "abs16offset" => match_node_abs16offset_0(bytes, ctx),
        "abs24offset" => match_node_abs24offset_0(bytes, ctx),
        "b1cnd" => match_node_b1cnd_0(bytes, ctx),
        "b2cnd" => match_node_b2cnd_0(bytes, ctx),
        "bit" => match_node_bit_0(bytes, ctx),
        "bitbase" => match_node_bitbase_0(bytes, ctx),
        "bitbaseAbs16" => match_node_bitbaseAbs16_0(bytes, ctx),
        "bitbaseAx" => match_node_bitbaseAx_0(bytes, ctx),
        "bitbaseDsp16" => match_node_bitbaseDsp16_0(bytes, ctx),
        "bitbaseDsp24" => match_node_bitbaseDsp24_0(bytes, ctx),
        "bitbaseDsp8" => match_node_bitbaseDsp8_0(bytes, ctx),
        "cnd" => match_node_cnd_0(bytes, ctx),
        "dsp8spB" => match_node_dsp8spB_0(bytes, ctx),
        "dsp8spW" => match_node_dsp8spW_0(bytes, ctx),
        "dst2B" => match_node_dst2B_0(bytes, ctx),
        "dst2L" => match_node_dst2L_0(bytes, ctx),
        "dst2W" => match_node_dst2W_0(bytes, ctx),
        "dst5A" => match_node_dst5A_0(bytes, ctx),
        "dst5Ax" => match_node_dst5Ax_0(bytes, ctx),
        "dst5B" => match_node_dst5B_0(bytes, ctx),
        "dst5B_afterDsp8" => match_node_dst5B_afterDsp8_0(bytes, ctx),
        "dst5B_afterSrc5" => match_node_dst5B_afterSrc5_0(bytes, ctx),
        "dst5L" => match_node_dst5L_0(bytes, ctx),
        "dst5L_afterSrc5" => match_node_dst5L_afterSrc5_0(bytes, ctx),
        "dst5W" => match_node_dst5W_0(bytes, ctx),
        "dst5W_afterDsp8" => match_node_dst5W_afterDsp8_0(bytes, ctx),
        "dst5W_afterSrc5" => match_node_dst5W_afterSrc5_0(bytes, ctx),
        "dst5dsp16" => match_node_dst5dsp16_0(bytes, ctx),
        "dst5dsp24" => match_node_dst5dsp24_0(bytes, ctx),
        "dst5dsp8" => match_node_dst5dsp8_0(bytes, ctx),
        "dstIndexOffset" => match_node_dstIndexOffset_0(bytes, ctx),
        "flagBit" => match_node_flagBit_0(bytes, ctx),
        "instruction" => match_node_instruction_0(bytes, ctx),
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
        "rel16offset1" => match_node_rel16offset1_0(bytes, ctx),
        "rel3offset2" => match_node_rel3offset2_0(bytes, ctx),
        "rel8offset1" => match_node_rel8offset1_0(bytes, ctx),
        "rel8offset2" => match_node_rel8offset2_0(bytes, ctx),
        "reloffset_dst5Ax" => match_node_reloffset_dst5Ax_0(bytes, ctx),
        "reloffset_dst5L" => match_node_reloffset_dst5L_0(bytes, ctx),
        "reloffset_dst5W" => match_node_reloffset_dst5W_0(bytes, ctx),
        "skipBytesBeforeDst5" => match_node_skipBytesBeforeDst5_0(bytes, ctx),
        "src5B" => match_node_src5B_0(bytes, ctx),
        "src5L" => match_node_src5L_0(bytes, ctx),
        "src5W" => match_node_src5W_0(bytes, ctx),
        "src5dsp16" => match_node_src5dsp16_0(bytes, ctx),
        "src5dsp24" => match_node_src5dsp24_0(bytes, ctx),
        "src5dsp8" => match_node_src5dsp8_0(bytes, ctx),
        "srcImm16" => match_node_srcImm16_0(bytes, ctx),
        "srcImm16a" => match_node_srcImm16a_0(bytes, ctx),
        "srcImm1p" => match_node_srcImm1p_0(bytes, ctx),
        "srcImm24" => match_node_srcImm24_0(bytes, ctx),
        "srcImm3" => match_node_srcImm3_0(bytes, ctx),
        "srcImm32" => match_node_srcImm32_0(bytes, ctx),
        "srcImm3p" => match_node_srcImm3p_0(bytes, ctx),
        "srcImm8" => match_node_srcImm8_0(bytes, ctx),
        "srcImm8a" => match_node_srcImm8a_0(bytes, ctx),
        "srcIndexOffset" => match_node_srcIndexOffset_0(bytes, ctx),
        "srcIntNum" => match_node_srcIntNum_0(bytes, ctx),
        "srcSimm16" => match_node_srcSimm16_0(bytes, ctx),
        "srcSimm16a" => match_node_srcSimm16a_0(bytes, ctx),
        "srcSimm32" => match_node_srcSimm32_0(bytes, ctx),
        "srcSimm4" => match_node_srcSimm4_0(bytes, ctx),
        "srcSimm4Shift" => match_node_srcSimm4Shift_0(bytes, ctx),
        "srcSimm8" => match_node_srcSimm8_0(bytes, ctx),
        "srcSimm8a" => match_node_srcSimm8a_0(bytes, ctx),
        "srcZero16" => match_node_srcZero16_0(bytes, ctx),
        "srcZero8" => match_node_srcZero8_0(bytes, ctx),
        "with" => match_node_with_0(bytes, ctx),
        _ => -1
    }
}

fn match_node_abs16offset_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_abs24offset_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_b1cnd_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 7;
    eprintln!("Trace node 0: SlaInstructionBits start=1, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_b1cnd_1(bytes, ctx),
        1 => match_node_b1cnd_4(bytes, ctx),
        2 => match_node_b1cnd_7(bytes, ctx),
        3 => match_node_b1cnd_10(bytes, ctx),
        4 => match_node_b1cnd_11(bytes, ctx),
        5 => match_node_b1cnd_14(bytes, ctx),
        6 => match_node_b1cnd_17(bytes, ctx),
        7 => match_node_b1cnd_20(bytes, ctx),
        _ => -1,
    }
}

fn match_node_b1cnd_1(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 1: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_b1cnd_2(bytes, ctx),
        1 => match_node_b1cnd_3(bytes, ctx),
        _ => -1,
    }
}

fn match_node_b1cnd_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_b1cnd_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 1");
    1
}

fn match_node_b1cnd_4(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 4: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_b1cnd_5(bytes, ctx),
        1 => match_node_b1cnd_6(bytes, ctx),
        _ => -1,
    }
}

fn match_node_b1cnd_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 2");
    2
}

fn match_node_b1cnd_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 3");
    3
}

fn match_node_b1cnd_7(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 7: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_b1cnd_8(bytes, ctx),
        1 => match_node_b1cnd_9(bytes, ctx),
        _ => -1,
    }
}

fn match_node_b1cnd_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 4");
    4
}

fn match_node_b1cnd_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 5");
    5
}

fn match_node_b1cnd_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 6");
    6
}

fn match_node_b1cnd_11(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 11: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_b1cnd_12(bytes, ctx),
        1 => match_node_b1cnd_13(bytes, ctx),
        _ => -1,
    }
}

fn match_node_b1cnd_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 7");
    7
}

fn match_node_b1cnd_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 8");
    8
}

fn match_node_b1cnd_14(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 14: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_b1cnd_15(bytes, ctx),
        1 => match_node_b1cnd_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_b1cnd_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 9");
    9
}

fn match_node_b1cnd_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 10");
    10
}

fn match_node_b1cnd_17(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 17: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_b1cnd_18(bytes, ctx),
        1 => match_node_b1cnd_19(bytes, ctx),
        _ => -1,
    }
}

fn match_node_b1cnd_18(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 18: Terminal matched constructor ID 11");
    11
}

fn match_node_b1cnd_19(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 19: Terminal matched constructor ID 12");
    12
}

fn match_node_b1cnd_20(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 20: Terminal matched constructor ID 13");
    13
}

fn match_node_b2cnd_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 7;
    eprintln!("Trace node 0: SlaInstructionBits start=5, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_b2cnd_1(bytes, ctx),
        1 => match_node_b2cnd_4(bytes, ctx),
        2 => match_node_b2cnd_7(bytes, ctx),
        3 => match_node_b2cnd_10(bytes, ctx),
        4 => match_node_b2cnd_13(bytes, ctx),
        5 => match_node_b2cnd_16(bytes, ctx),
        6 => match_node_b2cnd_19(bytes, ctx),
        7 => match_node_b2cnd_22(bytes, ctx),
        _ => -1,
    }
}

fn match_node_b2cnd_1(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 1: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_b2cnd_2(bytes, ctx),
        1 => match_node_b2cnd_3(bytes, ctx),
        _ => -1,
    }
}

fn match_node_b2cnd_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_b2cnd_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 7");
    7
}

fn match_node_b2cnd_4(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 4: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_b2cnd_5(bytes, ctx),
        1 => match_node_b2cnd_6(bytes, ctx),
        _ => -1,
    }
}

fn match_node_b2cnd_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 1");
    1
}

fn match_node_b2cnd_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 8");
    8
}

fn match_node_b2cnd_7(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 7: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_b2cnd_8(bytes, ctx),
        1 => match_node_b2cnd_9(bytes, ctx),
        _ => -1,
    }
}

fn match_node_b2cnd_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 2");
    2
}

fn match_node_b2cnd_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 9");
    9
}

fn match_node_b2cnd_10(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 10: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_b2cnd_11(bytes, ctx),
        1 => match_node_b2cnd_12(bytes, ctx),
        _ => -1,
    }
}

fn match_node_b2cnd_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 3");
    3
}

fn match_node_b2cnd_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 10");
    10
}

fn match_node_b2cnd_13(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 13: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_b2cnd_14(bytes, ctx),
        1 => match_node_b2cnd_15(bytes, ctx),
        _ => -1,
    }
}

fn match_node_b2cnd_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 4");
    4
}

fn match_node_b2cnd_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 11");
    11
}

fn match_node_b2cnd_16(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 16: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_b2cnd_17(bytes, ctx),
        1 => match_node_b2cnd_18(bytes, ctx),
        _ => -1,
    }
}

fn match_node_b2cnd_17(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 17: Terminal matched constructor ID 5");
    5
}

fn match_node_b2cnd_18(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 18: Terminal matched constructor ID 12");
    12
}

fn match_node_b2cnd_19(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 19: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_b2cnd_20(bytes, ctx),
        1 => match_node_b2cnd_21(bytes, ctx),
        _ => -1,
    }
}

fn match_node_b2cnd_20(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 20: Terminal matched constructor ID 6");
    6
}

fn match_node_b2cnd_21(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 21: Terminal matched constructor ID 13");
    13
}

fn match_node_b2cnd_22(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 22: Terminal matched NOTHING");
    -1
}

fn match_node_bit_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 0: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_bit_1(bytes, ctx),
        1 => match_node_bit_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_bit_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_bit_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_bitbase_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 7;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_bitbase_1(bytes, ctx),
        1 => match_node_bitbase_6(bytes, ctx),
        2 => match_node_bitbase_9(bytes, ctx),
        3 => match_node_bitbase_12(bytes, ctx),
        4 => match_node_bitbase_23(bytes, ctx),
        5 => match_node_bitbase_24(bytes, ctx),
        6 => match_node_bitbase_25(bytes, ctx),
        7 => match_node_bitbase_26(bytes, ctx),
        _ => -1,
    }
}

fn match_node_bitbase_1(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 1: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_bitbase_2(bytes, ctx),
        1 => match_node_bitbase_5(bytes, ctx),
        _ => -1,
    }
}

fn match_node_bitbase_2(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 2: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_bitbase_3(bytes, ctx),
        1 => match_node_bitbase_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_bitbase_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_bitbase_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 1");
    1
}

fn match_node_bitbase_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 8");
    8
}

fn match_node_bitbase_6(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 6: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_bitbase_7(bytes, ctx),
        1 => match_node_bitbase_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_bitbase_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 3");
    3
}

fn match_node_bitbase_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 9");
    9
}

fn match_node_bitbase_9(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 9: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_bitbase_10(bytes, ctx),
        1 => match_node_bitbase_11(bytes, ctx),
        _ => -1,
    }
}

fn match_node_bitbase_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 4");
    4
}

fn match_node_bitbase_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 10");
    10
}

fn match_node_bitbase_12(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 12: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_bitbase_13(bytes, ctx),
        1 => match_node_bitbase_18(bytes, ctx),
        _ => -1,
    }
}

fn match_node_bitbase_13(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 13: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_bitbase_14(bytes, ctx),
        1 => match_node_bitbase_15(bytes, ctx),
        _ => -1,
    }
}

fn match_node_bitbase_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 5");
    5
}

fn match_node_bitbase_15(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 15: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_bitbase_16(bytes, ctx),
        1 => match_node_bitbase_17(bytes, ctx),
        _ => -1,
    }
}

fn match_node_bitbase_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 7");
    7
}

fn match_node_bitbase_17(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 17: Terminal matched constructor ID 6");
    6
}

fn match_node_bitbase_18(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 18: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_bitbase_19(bytes, ctx),
        1 => match_node_bitbase_20(bytes, ctx),
        _ => -1,
    }
}

fn match_node_bitbase_19(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 19: Terminal matched constructor ID 11");
    11
}

fn match_node_bitbase_20(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 20: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_bitbase_21(bytes, ctx),
        1 => match_node_bitbase_22(bytes, ctx),
        _ => -1,
    }
}

fn match_node_bitbase_21(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 21: Terminal matched constructor ID 13");
    13
}

fn match_node_bitbase_22(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 22: Terminal matched constructor ID 12");
    12
}

fn match_node_bitbase_23(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 23: Terminal matched constructor ID 0");
    0
}

fn match_node_bitbase_24(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 24: Terminal matched NOTHING");
    -1
}

fn match_node_bitbase_25(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 25: Terminal matched NOTHING");
    -1
}

fn match_node_bitbase_26(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 26: Terminal matched NOTHING");
    -1
}

fn match_node_bitbaseAbs16_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_bitbaseAx_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_bitbaseDsp16_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 1");
    1
}

fn match_node_bitbaseDsp24_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 1");
    1
}

fn match_node_bitbaseDsp8_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 1");
    1
}

fn match_node_cnd_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 15;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_cnd_1(bytes, ctx),
        1 => match_node_cnd_2(bytes, ctx),
        2 => match_node_cnd_3(bytes, ctx),
        3 => match_node_cnd_4(bytes, ctx),
        4 => match_node_cnd_5(bytes, ctx),
        5 => match_node_cnd_6(bytes, ctx),
        6 => match_node_cnd_7(bytes, ctx),
        7 => match_node_cnd_8(bytes, ctx),
        8 => match_node_cnd_9(bytes, ctx),
        9 => match_node_cnd_10(bytes, ctx),
        10 => match_node_cnd_11(bytes, ctx),
        11 => match_node_cnd_12(bytes, ctx),
        12 => match_node_cnd_13(bytes, ctx),
        13 => match_node_cnd_14(bytes, ctx),
        14 => match_node_cnd_15(bytes, ctx),
        15 => match_node_cnd_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_cnd_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_cnd_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_cnd_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_cnd_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_cnd_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 4");
    4
}

fn match_node_cnd_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_cnd_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 6");
    6
}

fn match_node_cnd_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched NOTHING");
    -1
}

fn match_node_cnd_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 7");
    7
}

fn match_node_cnd_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 8");
    8
}

fn match_node_cnd_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 9");
    9
}

fn match_node_cnd_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 10");
    10
}

fn match_node_cnd_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 11");
    11
}

fn match_node_cnd_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 12");
    12
}

fn match_node_cnd_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 13");
    13
}

fn match_node_cnd_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched NOTHING");
    -1
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
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 0: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_dst2B_1(bytes, ctx),
        1 => match_node_dst2B_2(bytes, ctx),
        2 => match_node_dst2B_3(bytes, ctx),
        3 => match_node_dst2B_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dst2B_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_dst2B_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 2");
    2
}

fn match_node_dst2B_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 4");
    4
}

fn match_node_dst2B_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 6");
    6
}

fn match_node_dst2L_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 0: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_dst2L_1(bytes, ctx),
        1 => match_node_dst2L_2(bytes, ctx),
        2 => match_node_dst2L_3(bytes, ctx),
        3 => match_node_dst2L_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dst2L_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_dst2L_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 2");
    2
}

fn match_node_dst2L_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 4");
    4
}

fn match_node_dst2L_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 6");
    6
}

fn match_node_dst2W_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 0: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_dst2W_1(bytes, ctx),
        1 => match_node_dst2W_2(bytes, ctx),
        2 => match_node_dst2W_3(bytes, ctx),
        3 => match_node_dst2W_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dst2W_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_dst2W_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 2");
    2
}

fn match_node_dst2W_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 4");
    4
}

fn match_node_dst2W_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 6");
    6
}

fn match_node_dst5A_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 3;
    eprintln!("Trace node 0: SlaInstructionBits start=5, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_dst5A_1(bytes, ctx),
        1 => match_node_dst5A_2(bytes, ctx),
        2 => match_node_dst5A_3(bytes, ctx),
        3 => match_node_dst5A_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dst5A_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched NOTHING");
    -1
}

fn match_node_dst5A_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_dst5A_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 1");
    1
}

fn match_node_dst5A_4(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 4: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_dst5A_5(bytes, ctx),
        1 => match_node_dst5A_6(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dst5A_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 2");
    2
}

fn match_node_dst5A_6(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 6: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_dst5A_7(bytes, ctx),
        1 => match_node_dst5A_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dst5A_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 4");
    4
}

fn match_node_dst5A_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 3");
    3
}

fn match_node_dst5Ax_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_dst5B_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 7;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_dst5B_1(bytes, ctx),
        1 => match_node_dst5B_4(bytes, ctx),
        2 => match_node_dst5B_5(bytes, ctx),
        3 => match_node_dst5B_6(bytes, ctx),
        4 => match_node_dst5B_15(bytes, ctx),
        5 => match_node_dst5B_16(bytes, ctx),
        6 => match_node_dst5B_17(bytes, ctx),
        7 => match_node_dst5B_18(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dst5B_1(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 1: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_dst5B_2(bytes, ctx),
        1 => match_node_dst5B_3(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dst5B_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 4");
    4
}

fn match_node_dst5B_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_dst5B_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 6");
    6
}

fn match_node_dst5B_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 8");
    8
}

fn match_node_dst5B_6(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 6: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_dst5B_7(bytes, ctx),
        1 => match_node_dst5B_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dst5B_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 10");
    10
}

fn match_node_dst5B_8(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 8: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_dst5B_9(bytes, ctx),
        1 => match_node_dst5B_12(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dst5B_9(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 5) & 1;
    eprintln!("Trace node 9: SlaContextBits start=5, size=1, probe={}", probe);
    match probe {
        0 => match_node_dst5B_10(bytes, ctx),
        1 => match_node_dst5B_11(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dst5B_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 14");
    14
}

fn match_node_dst5B_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 16");
    16
}

fn match_node_dst5B_12(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 5) & 1;
    eprintln!("Trace node 12: SlaContextBits start=5, size=1, probe={}", probe);
    match probe {
        0 => match_node_dst5B_13(bytes, ctx),
        1 => match_node_dst5B_14(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dst5B_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 11");
    11
}

fn match_node_dst5B_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 13");
    13
}

fn match_node_dst5B_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 0");
    0
}

fn match_node_dst5B_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched NOTHING");
    -1
}

fn match_node_dst5B_17(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 17: Terminal matched NOTHING");
    -1
}

fn match_node_dst5B_18(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 18: Terminal matched NOTHING");
    -1
}

fn match_node_dst5B_afterDsp8_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_dst5B_afterSrc5_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_dst5L_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 7;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_dst5L_1(bytes, ctx),
        1 => match_node_dst5L_4(bytes, ctx),
        2 => match_node_dst5L_5(bytes, ctx),
        3 => match_node_dst5L_6(bytes, ctx),
        4 => match_node_dst5L_15(bytes, ctx),
        5 => match_node_dst5L_16(bytes, ctx),
        6 => match_node_dst5L_17(bytes, ctx),
        7 => match_node_dst5L_18(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dst5L_1(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 1: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_dst5L_2(bytes, ctx),
        1 => match_node_dst5L_3(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dst5L_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 4");
    4
}

fn match_node_dst5L_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_dst5L_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 6");
    6
}

fn match_node_dst5L_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 8");
    8
}

fn match_node_dst5L_6(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 6: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_dst5L_7(bytes, ctx),
        1 => match_node_dst5L_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dst5L_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 10");
    10
}

fn match_node_dst5L_8(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 8: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_dst5L_9(bytes, ctx),
        1 => match_node_dst5L_12(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dst5L_9(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 5) & 1;
    eprintln!("Trace node 9: SlaContextBits start=5, size=1, probe={}", probe);
    match probe {
        0 => match_node_dst5L_10(bytes, ctx),
        1 => match_node_dst5L_11(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dst5L_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 14");
    14
}

fn match_node_dst5L_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 16");
    16
}

fn match_node_dst5L_12(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 5) & 1;
    eprintln!("Trace node 12: SlaContextBits start=5, size=1, probe={}", probe);
    match probe {
        0 => match_node_dst5L_13(bytes, ctx),
        1 => match_node_dst5L_14(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dst5L_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 11");
    11
}

fn match_node_dst5L_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 13");
    13
}

fn match_node_dst5L_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 0");
    0
}

fn match_node_dst5L_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched NOTHING");
    -1
}

fn match_node_dst5L_17(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 17: Terminal matched NOTHING");
    -1
}

fn match_node_dst5L_18(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 18: Terminal matched NOTHING");
    -1
}

fn match_node_dst5L_afterSrc5_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_dst5W_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 7;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_dst5W_1(bytes, ctx),
        1 => match_node_dst5W_4(bytes, ctx),
        2 => match_node_dst5W_5(bytes, ctx),
        3 => match_node_dst5W_6(bytes, ctx),
        4 => match_node_dst5W_15(bytes, ctx),
        5 => match_node_dst5W_16(bytes, ctx),
        6 => match_node_dst5W_17(bytes, ctx),
        7 => match_node_dst5W_18(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dst5W_1(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 1: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_dst5W_2(bytes, ctx),
        1 => match_node_dst5W_3(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dst5W_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 4");
    4
}

fn match_node_dst5W_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_dst5W_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 6");
    6
}

fn match_node_dst5W_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 8");
    8
}

fn match_node_dst5W_6(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 6: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_dst5W_7(bytes, ctx),
        1 => match_node_dst5W_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dst5W_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 10");
    10
}

fn match_node_dst5W_8(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 8: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_dst5W_9(bytes, ctx),
        1 => match_node_dst5W_12(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dst5W_9(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 5) & 1;
    eprintln!("Trace node 9: SlaContextBits start=5, size=1, probe={}", probe);
    match probe {
        0 => match_node_dst5W_10(bytes, ctx),
        1 => match_node_dst5W_11(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dst5W_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 14");
    14
}

fn match_node_dst5W_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 16");
    16
}

fn match_node_dst5W_12(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 5) & 1;
    eprintln!("Trace node 12: SlaContextBits start=5, size=1, probe={}", probe);
    match probe {
        0 => match_node_dst5W_13(bytes, ctx),
        1 => match_node_dst5W_14(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dst5W_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 11");
    11
}

fn match_node_dst5W_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 13");
    13
}

fn match_node_dst5W_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 0");
    0
}

fn match_node_dst5W_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched NOTHING");
    -1
}

fn match_node_dst5W_17(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 17: Terminal matched NOTHING");
    -1
}

fn match_node_dst5W_18(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 18: Terminal matched NOTHING");
    -1
}

fn match_node_dst5W_afterDsp8_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_dst5W_afterSrc5_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_dst5dsp16_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 1");
    1
}

fn match_node_dst5dsp24_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_dst5dsp8_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 1");
    1
}

fn match_node_dstIndexOffset_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 2) & 1;
    eprintln!("Trace node 0: SlaContextBits start=2, size=1, probe={}", probe);
    match probe {
        0 => match_node_dstIndexOffset_1(bytes, ctx),
        1 => match_node_dstIndexOffset_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_dstIndexOffset_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_dstIndexOffset_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_flagBit_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 7;
    eprintln!("Trace node 0: SlaInstructionBits start=5, size=3, word={:08x}, probe={}", word, probe);
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
    let probe = (ctx >> 4) & 1;
    eprintln!("Trace node 0: SlaContextBits start=4, size=1, probe={}", probe);
    match probe {
        0 => match_node_instruction_1(bytes, ctx),
        1 => match_node_instruction_18(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 15;
    eprintln!("Trace node 1: SlaInstructionBits start=1, size=4, word={:08x}, probe={}", word, probe);
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
        9 => match_node_instruction_11(bytes, ctx),
        10 => match_node_instruction_12(bytes, ctx),
        11 => match_node_instruction_13(bytes, ctx),
        12 => match_node_instruction_14(bytes, ctx),
        13 => match_node_instruction_15(bytes, ctx),
        14 => match_node_instruction_16(bytes, ctx),
        15 => match_node_instruction_17(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 1");
    1
}

fn match_node_instruction_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 2");
    2
}

fn match_node_instruction_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 3");
    3
}

fn match_node_instruction_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_17(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 17: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_18(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 1;
    eprintln!("Trace node 18: SlaInstructionBits start=0, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_19(bytes, ctx),
        1 => match_node_instruction_312(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_19(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 7;
    eprintln!("Trace node 19: SlaInstructionBits start=4, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_20(bytes, ctx),
        1 => match_node_instruction_271(bytes, ctx),
        2 => match_node_instruction_276(bytes, ctx),
        3 => match_node_instruction_283(bytes, ctx),
        4 => match_node_instruction_290(bytes, ctx),
        5 => match_node_instruction_295(bytes, ctx),
        6 => match_node_instruction_298(bytes, ctx),
        7 => match_node_instruction_305(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_20(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 20: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_21(bytes, ctx),
        1 => match_node_instruction_24(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_21(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 21: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_22(bytes, ctx),
        1 => match_node_instruction_23(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_22(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 22: Terminal matched constructor ID 82");
    82
}

fn match_node_instruction_23(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 23: Terminal matched constructor ID 117");
    117
}

fn match_node_instruction_24(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 24: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_25(bytes, ctx),
        1 => match_node_instruction_270(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_25(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 25: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_26(bytes, ctx),
        1 => match_node_instruction_165(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_26(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 26: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_27(bytes, ctx),
        1 => match_node_instruction_112(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_27(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 15;
    eprintln!("Trace node 27: SlaInstructionBits start=20, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_28(bytes, ctx),
        1 => match_node_instruction_31(bytes, ctx),
        2 => match_node_instruction_32(bytes, ctx),
        3 => match_node_instruction_35(bytes, ctx),
        4 => match_node_instruction_36(bytes, ctx),
        5 => match_node_instruction_39(bytes, ctx),
        6 => match_node_instruction_40(bytes, ctx),
        7 => match_node_instruction_43(bytes, ctx),
        8 => match_node_instruction_46(bytes, ctx),
        9 => match_node_instruction_49(bytes, ctx),
        10 => match_node_instruction_52(bytes, ctx),
        11 => match_node_instruction_55(bytes, ctx),
        12 => match_node_instruction_58(bytes, ctx),
        13 => match_node_instruction_61(bytes, ctx),
        14 => match_node_instruction_64(bytes, ctx),
        15 => match_node_instruction_105(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_28(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 28: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_29(bytes, ctx),
        1 => match_node_instruction_30(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_29(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 29: Terminal matched constructor ID 134");
    134
}

fn match_node_instruction_30(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 30: Terminal matched constructor ID 133");
    133
}

fn match_node_instruction_31(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 31: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_32(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 32: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_33(bytes, ctx),
        1 => match_node_instruction_34(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_33(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 33: Terminal matched constructor ID 166");
    166
}

fn match_node_instruction_34(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 34: Terminal matched constructor ID 165");
    165
}

fn match_node_instruction_35(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 35: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_36(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 36: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_37(bytes, ctx),
        1 => match_node_instruction_38(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_37(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 37: Terminal matched constructor ID 13");
    13
}

fn match_node_instruction_38(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 38: Terminal matched constructor ID 12");
    12
}

fn match_node_instruction_39(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 39: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_40(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 40: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_41(bytes, ctx),
        1 => match_node_instruction_42(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_41(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 41: Terminal matched constructor ID 374");
    374
}

fn match_node_instruction_42(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 42: Terminal matched constructor ID 373");
    373
}

fn match_node_instruction_43(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 43: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_44(bytes, ctx),
        1 => match_node_instruction_45(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_44(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 44: Terminal matched constructor ID 176");
    176
}

fn match_node_instruction_45(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 45: Terminal matched constructor ID 175");
    175
}

fn match_node_instruction_46(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 46: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_47(bytes, ctx),
        1 => match_node_instruction_48(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_47(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 47: Terminal matched constructor ID 126");
    126
}

fn match_node_instruction_48(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 48: Terminal matched constructor ID 125");
    125
}

fn match_node_instruction_49(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 49: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_50(bytes, ctx),
        1 => match_node_instruction_51(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_50(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 50: Terminal matched constructor ID 466");
    466
}

fn match_node_instruction_51(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 51: Terminal matched constructor ID 466");
    466
}

fn match_node_instruction_52(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 52: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_53(bytes, ctx),
        1 => match_node_instruction_54(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_53(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 53: Terminal matched constructor ID 158");
    158
}

fn match_node_instruction_54(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 54: Terminal matched constructor ID 157");
    157
}

fn match_node_instruction_55(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 55: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_56(bytes, ctx),
        1 => match_node_instruction_57(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_56(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 56: Terminal matched constructor ID 178");
    178
}

fn match_node_instruction_57(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 57: Terminal matched constructor ID 177");
    177
}

fn match_node_instruction_58(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 58: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_59(bytes, ctx),
        1 => match_node_instruction_60(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_59(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 59: Terminal matched constructor ID 242");
    242
}

fn match_node_instruction_60(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 60: Terminal matched constructor ID 241");
    241
}

fn match_node_instruction_61(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 61: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_62(bytes, ctx),
        1 => match_node_instruction_63(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_62(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 62: Terminal matched constructor ID 234");
    234
}

fn match_node_instruction_63(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 63: Terminal matched constructor ID 233");
    233
}

fn match_node_instruction_64(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 7;
    eprintln!("Trace node 64: SlaInstructionBits start=10, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_65(bytes, ctx),
        1 => match_node_instruction_70(bytes, ctx),
        2 => match_node_instruction_75(bytes, ctx),
        3 => match_node_instruction_80(bytes, ctx),
        4 => match_node_instruction_85(bytes, ctx),
        5 => match_node_instruction_90(bytes, ctx),
        6 => match_node_instruction_95(bytes, ctx),
        7 => match_node_instruction_100(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_65(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (18 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 18) & 3;
    eprintln!("Trace node 65: SlaInstructionBits start=18, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_66(bytes, ctx),
        1 => match_node_instruction_67(bytes, ctx),
        2 => match_node_instruction_68(bytes, ctx),
        3 => match_node_instruction_69(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_66(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 66: Terminal matched constructor ID 122");
    122
}

fn match_node_instruction_67(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 67: Terminal matched constructor ID 130");
    130
}

fn match_node_instruction_68(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 68: Terminal matched constructor ID 9");
    9
}

fn match_node_instruction_69(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 69: Terminal matched constructor ID 96");
    96
}

fn match_node_instruction_70(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (18 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 18) & 3;
    eprintln!("Trace node 70: SlaInstructionBits start=18, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_71(bytes, ctx),
        1 => match_node_instruction_72(bytes, ctx),
        2 => match_node_instruction_73(bytes, ctx),
        3 => match_node_instruction_74(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_71(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 71: Terminal matched constructor ID 121");
    121
}

fn match_node_instruction_72(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 72: Terminal matched constructor ID 129");
    129
}

fn match_node_instruction_73(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 73: Terminal matched constructor ID 8");
    8
}

fn match_node_instruction_74(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 74: Terminal matched constructor ID 95");
    95
}

fn match_node_instruction_75(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (18 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 18) & 3;
    eprintln!("Trace node 75: SlaInstructionBits start=18, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_76(bytes, ctx),
        1 => match_node_instruction_77(bytes, ctx),
        2 => match_node_instruction_78(bytes, ctx),
        3 => match_node_instruction_79(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_76(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 76: Terminal matched constructor ID 154");
    154
}

fn match_node_instruction_77(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 77: Terminal matched constructor ID 162");
    162
}

fn match_node_instruction_78(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 78: Terminal matched constructor ID 370");
    370
}

fn match_node_instruction_79(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 79: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_80(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (18 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 18) & 3;
    eprintln!("Trace node 80: SlaInstructionBits start=18, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_81(bytes, ctx),
        1 => match_node_instruction_82(bytes, ctx),
        2 => match_node_instruction_83(bytes, ctx),
        3 => match_node_instruction_84(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_81(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 81: Terminal matched constructor ID 153");
    153
}

fn match_node_instruction_82(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 82: Terminal matched constructor ID 161");
    161
}

fn match_node_instruction_83(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 83: Terminal matched constructor ID 369");
    369
}

fn match_node_instruction_84(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 84: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_85(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (18 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 18) & 3;
    eprintln!("Trace node 85: SlaInstructionBits start=18, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_86(bytes, ctx),
        1 => match_node_instruction_87(bytes, ctx),
        2 => match_node_instruction_88(bytes, ctx),
        3 => match_node_instruction_89(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_86(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 86: Terminal matched constructor ID 286");
    286
}

fn match_node_instruction_87(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 87: Terminal matched constructor ID 287");
    287
}

fn match_node_instruction_88(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 88: Terminal matched constructor ID 288");
    288
}

fn match_node_instruction_89(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 89: Terminal matched constructor ID 289");
    289
}

fn match_node_instruction_90(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (18 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 18) & 3;
    eprintln!("Trace node 90: SlaInstructionBits start=18, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_91(bytes, ctx),
        1 => match_node_instruction_92(bytes, ctx),
        2 => match_node_instruction_93(bytes, ctx),
        3 => match_node_instruction_94(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_91(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 91: Terminal matched constructor ID 286");
    286
}

fn match_node_instruction_92(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 92: Terminal matched constructor ID 287");
    287
}

fn match_node_instruction_93(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 93: Terminal matched constructor ID 288");
    288
}

fn match_node_instruction_94(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 94: Terminal matched constructor ID 289");
    289
}

fn match_node_instruction_95(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (18 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 18) & 3;
    eprintln!("Trace node 95: SlaInstructionBits start=18, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_96(bytes, ctx),
        1 => match_node_instruction_97(bytes, ctx),
        2 => match_node_instruction_98(bytes, ctx),
        3 => match_node_instruction_99(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_96(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 96: Terminal matched constructor ID 282");
    282
}

fn match_node_instruction_97(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 97: Terminal matched constructor ID 283");
    283
}

fn match_node_instruction_98(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 98: Terminal matched constructor ID 284");
    284
}

fn match_node_instruction_99(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 99: Terminal matched constructor ID 285");
    285
}

fn match_node_instruction_100(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (18 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 18) & 3;
    eprintln!("Trace node 100: SlaInstructionBits start=18, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_101(bytes, ctx),
        1 => match_node_instruction_102(bytes, ctx),
        2 => match_node_instruction_103(bytes, ctx),
        3 => match_node_instruction_104(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_101(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 101: Terminal matched constructor ID 282");
    282
}

fn match_node_instruction_102(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 102: Terminal matched constructor ID 283");
    283
}

fn match_node_instruction_103(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 103: Terminal matched constructor ID 284");
    284
}

fn match_node_instruction_104(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 104: Terminal matched constructor ID 285");
    285
}

fn match_node_instruction_105(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (19 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 19) & 1;
    eprintln!("Trace node 105: SlaInstructionBits start=19, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_106(bytes, ctx),
        1 => match_node_instruction_109(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_106(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 106: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_107(bytes, ctx),
        1 => match_node_instruction_108(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_107(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 107: Terminal matched constructor ID 238");
    238
}

fn match_node_instruction_108(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 108: Terminal matched constructor ID 237");
    237
}

fn match_node_instruction_109(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 109: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_110(bytes, ctx),
        1 => match_node_instruction_111(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_110(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 110: Terminal matched constructor ID 230");
    230
}

fn match_node_instruction_111(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 111: Terminal matched constructor ID 229");
    229
}

fn match_node_instruction_112(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 3;
    eprintln!("Trace node 112: SlaInstructionBits start=11, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_113(bytes, ctx),
        1 => match_node_instruction_130(bytes, ctx),
        2 => match_node_instruction_147(bytes, ctx),
        3 => match_node_instruction_156(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_113(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 15;
    eprintln!("Trace node 113: SlaInstructionBits start=20, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_114(bytes, ctx),
        1 => match_node_instruction_115(bytes, ctx),
        2 => match_node_instruction_116(bytes, ctx),
        3 => match_node_instruction_117(bytes, ctx),
        4 => match_node_instruction_118(bytes, ctx),
        5 => match_node_instruction_119(bytes, ctx),
        6 => match_node_instruction_120(bytes, ctx),
        7 => match_node_instruction_121(bytes, ctx),
        8 => match_node_instruction_122(bytes, ctx),
        9 => match_node_instruction_123(bytes, ctx),
        10 => match_node_instruction_124(bytes, ctx),
        11 => match_node_instruction_125(bytes, ctx),
        12 => match_node_instruction_126(bytes, ctx),
        13 => match_node_instruction_127(bytes, ctx),
        14 => match_node_instruction_128(bytes, ctx),
        15 => match_node_instruction_129(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_114(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 114: Terminal matched constructor ID 134");
    134
}

fn match_node_instruction_115(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 115: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_116(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 116: Terminal matched constructor ID 166");
    166
}

fn match_node_instruction_117(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 117: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_118(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 118: Terminal matched constructor ID 13");
    13
}

fn match_node_instruction_119(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 119: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_120(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 120: Terminal matched constructor ID 374");
    374
}

fn match_node_instruction_121(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 121: Terminal matched constructor ID 176");
    176
}

fn match_node_instruction_122(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 122: Terminal matched constructor ID 126");
    126
}

fn match_node_instruction_123(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 123: Terminal matched constructor ID 466");
    466
}

fn match_node_instruction_124(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 124: Terminal matched constructor ID 158");
    158
}

fn match_node_instruction_125(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 125: Terminal matched constructor ID 178");
    178
}

fn match_node_instruction_126(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 126: Terminal matched constructor ID 242");
    242
}

fn match_node_instruction_127(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 127: Terminal matched constructor ID 234");
    234
}

fn match_node_instruction_128(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 128: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_129(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 129: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_130(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 15;
    eprintln!("Trace node 130: SlaInstructionBits start=20, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_131(bytes, ctx),
        1 => match_node_instruction_132(bytes, ctx),
        2 => match_node_instruction_133(bytes, ctx),
        3 => match_node_instruction_134(bytes, ctx),
        4 => match_node_instruction_135(bytes, ctx),
        5 => match_node_instruction_136(bytes, ctx),
        6 => match_node_instruction_137(bytes, ctx),
        7 => match_node_instruction_138(bytes, ctx),
        8 => match_node_instruction_139(bytes, ctx),
        9 => match_node_instruction_140(bytes, ctx),
        10 => match_node_instruction_141(bytes, ctx),
        11 => match_node_instruction_142(bytes, ctx),
        12 => match_node_instruction_143(bytes, ctx),
        13 => match_node_instruction_144(bytes, ctx),
        14 => match_node_instruction_145(bytes, ctx),
        15 => match_node_instruction_146(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_131(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 131: Terminal matched constructor ID 133");
    133
}

fn match_node_instruction_132(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 132: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_133(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 133: Terminal matched constructor ID 165");
    165
}

fn match_node_instruction_134(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 134: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_135(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 135: Terminal matched constructor ID 12");
    12
}

fn match_node_instruction_136(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 136: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_137(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 137: Terminal matched constructor ID 373");
    373
}

fn match_node_instruction_138(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 138: Terminal matched constructor ID 175");
    175
}

fn match_node_instruction_139(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 139: Terminal matched constructor ID 125");
    125
}

fn match_node_instruction_140(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 140: Terminal matched constructor ID 466");
    466
}

fn match_node_instruction_141(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 141: Terminal matched constructor ID 157");
    157
}

fn match_node_instruction_142(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 142: Terminal matched constructor ID 177");
    177
}

fn match_node_instruction_143(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 143: Terminal matched constructor ID 241");
    241
}

fn match_node_instruction_144(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 144: Terminal matched constructor ID 233");
    233
}

fn match_node_instruction_145(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 145: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_146(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 146: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_147(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (18 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 18) & 7;
    eprintln!("Trace node 147: SlaInstructionBits start=18, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_148(bytes, ctx),
        1 => match_node_instruction_149(bytes, ctx),
        2 => match_node_instruction_150(bytes, ctx),
        3 => match_node_instruction_151(bytes, ctx),
        4 => match_node_instruction_152(bytes, ctx),
        5 => match_node_instruction_153(bytes, ctx),
        6 => match_node_instruction_154(bytes, ctx),
        7 => match_node_instruction_155(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_148(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 148: Terminal matched constructor ID 77");
    77
}

fn match_node_instruction_149(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 149: Terminal matched constructor ID 62");
    62
}

fn match_node_instruction_150(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 150: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_151(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 151: Terminal matched constructor ID 71");
    71
}

fn match_node_instruction_152(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 152: Terminal matched constructor ID 81");
    81
}

fn match_node_instruction_153(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 153: Terminal matched constructor ID 94");
    94
}

fn match_node_instruction_154(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 154: Terminal matched constructor ID 73");
    73
}

fn match_node_instruction_155(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 155: Terminal matched constructor ID 79");
    79
}

fn match_node_instruction_156(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (18 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 18) & 7;
    eprintln!("Trace node 156: SlaInstructionBits start=18, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_157(bytes, ctx),
        1 => match_node_instruction_158(bytes, ctx),
        2 => match_node_instruction_159(bytes, ctx),
        3 => match_node_instruction_160(bytes, ctx),
        4 => match_node_instruction_161(bytes, ctx),
        5 => match_node_instruction_162(bytes, ctx),
        6 => match_node_instruction_163(bytes, ctx),
        7 => match_node_instruction_164(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_157(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 157: Terminal matched constructor ID 76");
    76
}

fn match_node_instruction_158(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 158: Terminal matched constructor ID 62");
    62
}

fn match_node_instruction_159(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 159: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_160(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 160: Terminal matched constructor ID 70");
    70
}

fn match_node_instruction_161(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 161: Terminal matched constructor ID 80");
    80
}

fn match_node_instruction_162(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 162: Terminal matched constructor ID 93");
    93
}

fn match_node_instruction_163(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 163: Terminal matched constructor ID 72");
    72
}

fn match_node_instruction_164(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 164: Terminal matched constructor ID 78");
    78
}

fn match_node_instruction_165(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 165: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_166(bytes, ctx),
        1 => match_node_instruction_225(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_166(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 15;
    eprintln!("Trace node 166: SlaInstructionBits start=20, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_167(bytes, ctx),
        1 => match_node_instruction_170(bytes, ctx),
        2 => match_node_instruction_171(bytes, ctx),
        3 => match_node_instruction_174(bytes, ctx),
        4 => match_node_instruction_175(bytes, ctx),
        5 => match_node_instruction_178(bytes, ctx),
        6 => match_node_instruction_179(bytes, ctx),
        7 => match_node_instruction_182(bytes, ctx),
        8 => match_node_instruction_183(bytes, ctx),
        9 => match_node_instruction_186(bytes, ctx),
        10 => match_node_instruction_189(bytes, ctx),
        11 => match_node_instruction_192(bytes, ctx),
        12 => match_node_instruction_193(bytes, ctx),
        13 => match_node_instruction_196(bytes, ctx),
        14 => match_node_instruction_199(bytes, ctx),
        15 => match_node_instruction_218(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_167(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 167: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_168(bytes, ctx),
        1 => match_node_instruction_169(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_168(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 168: Terminal matched constructor ID 136");
    136
}

fn match_node_instruction_169(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 169: Terminal matched constructor ID 135");
    135
}

fn match_node_instruction_170(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 170: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_171(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 171: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_172(bytes, ctx),
        1 => match_node_instruction_173(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_172(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 172: Terminal matched constructor ID 168");
    168
}

fn match_node_instruction_173(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 173: Terminal matched constructor ID 167");
    167
}

fn match_node_instruction_174(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 174: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_175(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 175: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_176(bytes, ctx),
        1 => match_node_instruction_177(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_176(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 176: Terminal matched constructor ID 15");
    15
}

fn match_node_instruction_177(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 177: Terminal matched constructor ID 14");
    14
}

fn match_node_instruction_178(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 178: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_179(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 179: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_180(bytes, ctx),
        1 => match_node_instruction_181(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_180(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 180: Terminal matched constructor ID 376");
    376
}

fn match_node_instruction_181(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 181: Terminal matched constructor ID 375");
    375
}

fn match_node_instruction_182(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 182: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_183(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 183: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_184(bytes, ctx),
        1 => match_node_instruction_185(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_184(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 184: Terminal matched constructor ID 128");
    128
}

fn match_node_instruction_185(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 185: Terminal matched constructor ID 127");
    127
}

fn match_node_instruction_186(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 186: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_187(bytes, ctx),
        1 => match_node_instruction_188(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_187(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 187: Terminal matched constructor ID 467");
    467
}

fn match_node_instruction_188(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 188: Terminal matched constructor ID 467");
    467
}

fn match_node_instruction_189(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 189: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_190(bytes, ctx),
        1 => match_node_instruction_191(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_190(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 190: Terminal matched constructor ID 160");
    160
}

fn match_node_instruction_191(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 191: Terminal matched constructor ID 159");
    159
}

fn match_node_instruction_192(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 192: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_193(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 193: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_194(bytes, ctx),
        1 => match_node_instruction_195(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_194(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 194: Terminal matched constructor ID 244");
    244
}

fn match_node_instruction_195(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 195: Terminal matched constructor ID 243");
    243
}

fn match_node_instruction_196(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 196: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_197(bytes, ctx),
        1 => match_node_instruction_198(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_197(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 197: Terminal matched constructor ID 236");
    236
}

fn match_node_instruction_198(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 198: Terminal matched constructor ID 235");
    235
}

fn match_node_instruction_199(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (18 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 18) & 3;
    eprintln!("Trace node 199: SlaInstructionBits start=18, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_200(bytes, ctx),
        1 => match_node_instruction_205(bytes, ctx),
        2 => match_node_instruction_210(bytes, ctx),
        3 => match_node_instruction_215(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_200(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 3;
    eprintln!("Trace node 200: SlaInstructionBits start=11, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_201(bytes, ctx),
        1 => match_node_instruction_202(bytes, ctx),
        2 => match_node_instruction_203(bytes, ctx),
        3 => match_node_instruction_204(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_201(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 201: Terminal matched constructor ID 124");
    124
}

fn match_node_instruction_202(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 202: Terminal matched constructor ID 123");
    123
}

fn match_node_instruction_203(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 203: Terminal matched constructor ID 156");
    156
}

fn match_node_instruction_204(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 204: Terminal matched constructor ID 155");
    155
}

fn match_node_instruction_205(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 3;
    eprintln!("Trace node 205: SlaInstructionBits start=11, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_206(bytes, ctx),
        1 => match_node_instruction_207(bytes, ctx),
        2 => match_node_instruction_208(bytes, ctx),
        3 => match_node_instruction_209(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_206(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 206: Terminal matched constructor ID 132");
    132
}

fn match_node_instruction_207(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 207: Terminal matched constructor ID 131");
    131
}

fn match_node_instruction_208(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 208: Terminal matched constructor ID 164");
    164
}

fn match_node_instruction_209(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 209: Terminal matched constructor ID 163");
    163
}

fn match_node_instruction_210(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 3;
    eprintln!("Trace node 210: SlaInstructionBits start=11, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_211(bytes, ctx),
        1 => match_node_instruction_212(bytes, ctx),
        2 => match_node_instruction_213(bytes, ctx),
        3 => match_node_instruction_214(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_211(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 211: Terminal matched constructor ID 11");
    11
}

fn match_node_instruction_212(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 212: Terminal matched constructor ID 10");
    10
}

fn match_node_instruction_213(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 213: Terminal matched constructor ID 372");
    372
}

fn match_node_instruction_214(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 214: Terminal matched constructor ID 371");
    371
}

fn match_node_instruction_215(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 215: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_216(bytes, ctx),
        1 => match_node_instruction_217(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_216(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 216: Terminal matched constructor ID 98");
    98
}

fn match_node_instruction_217(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 217: Terminal matched constructor ID 97");
    97
}

fn match_node_instruction_218(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (19 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 19) & 1;
    eprintln!("Trace node 218: SlaInstructionBits start=19, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_219(bytes, ctx),
        1 => match_node_instruction_222(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_219(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 219: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_220(bytes, ctx),
        1 => match_node_instruction_221(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_220(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 220: Terminal matched constructor ID 240");
    240
}

fn match_node_instruction_221(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 221: Terminal matched constructor ID 239");
    239
}

fn match_node_instruction_222(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 222: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_223(bytes, ctx),
        1 => match_node_instruction_224(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_223(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 223: Terminal matched constructor ID 232");
    232
}

fn match_node_instruction_224(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 224: Terminal matched constructor ID 231");
    231
}

fn match_node_instruction_225(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 3;
    eprintln!("Trace node 225: SlaInstructionBits start=11, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_226(bytes, ctx),
        1 => match_node_instruction_243(bytes, ctx),
        2 => match_node_instruction_260(bytes, ctx),
        3 => match_node_instruction_265(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_226(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 15;
    eprintln!("Trace node 226: SlaInstructionBits start=20, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_227(bytes, ctx),
        1 => match_node_instruction_228(bytes, ctx),
        2 => match_node_instruction_229(bytes, ctx),
        3 => match_node_instruction_230(bytes, ctx),
        4 => match_node_instruction_231(bytes, ctx),
        5 => match_node_instruction_232(bytes, ctx),
        6 => match_node_instruction_233(bytes, ctx),
        7 => match_node_instruction_234(bytes, ctx),
        8 => match_node_instruction_235(bytes, ctx),
        9 => match_node_instruction_236(bytes, ctx),
        10 => match_node_instruction_237(bytes, ctx),
        11 => match_node_instruction_238(bytes, ctx),
        12 => match_node_instruction_239(bytes, ctx),
        13 => match_node_instruction_240(bytes, ctx),
        14 => match_node_instruction_241(bytes, ctx),
        15 => match_node_instruction_242(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_227(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 227: Terminal matched constructor ID 136");
    136
}

fn match_node_instruction_228(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 228: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_229(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 229: Terminal matched constructor ID 168");
    168
}

fn match_node_instruction_230(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 230: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_231(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 231: Terminal matched constructor ID 15");
    15
}

fn match_node_instruction_232(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 232: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_233(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 233: Terminal matched constructor ID 376");
    376
}

fn match_node_instruction_234(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 234: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_235(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 235: Terminal matched constructor ID 128");
    128
}

fn match_node_instruction_236(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 236: Terminal matched constructor ID 467");
    467
}

fn match_node_instruction_237(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 237: Terminal matched constructor ID 160");
    160
}

fn match_node_instruction_238(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 238: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_239(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 239: Terminal matched constructor ID 244");
    244
}

fn match_node_instruction_240(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 240: Terminal matched constructor ID 236");
    236
}

fn match_node_instruction_241(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 241: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_242(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 242: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_243(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 15;
    eprintln!("Trace node 243: SlaInstructionBits start=20, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_244(bytes, ctx),
        1 => match_node_instruction_245(bytes, ctx),
        2 => match_node_instruction_246(bytes, ctx),
        3 => match_node_instruction_247(bytes, ctx),
        4 => match_node_instruction_248(bytes, ctx),
        5 => match_node_instruction_249(bytes, ctx),
        6 => match_node_instruction_250(bytes, ctx),
        7 => match_node_instruction_251(bytes, ctx),
        8 => match_node_instruction_252(bytes, ctx),
        9 => match_node_instruction_253(bytes, ctx),
        10 => match_node_instruction_254(bytes, ctx),
        11 => match_node_instruction_255(bytes, ctx),
        12 => match_node_instruction_256(bytes, ctx),
        13 => match_node_instruction_257(bytes, ctx),
        14 => match_node_instruction_258(bytes, ctx),
        15 => match_node_instruction_259(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_244(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 244: Terminal matched constructor ID 135");
    135
}

fn match_node_instruction_245(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 245: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_246(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 246: Terminal matched constructor ID 167");
    167
}

fn match_node_instruction_247(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 247: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_248(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 248: Terminal matched constructor ID 14");
    14
}

fn match_node_instruction_249(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 249: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_250(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 250: Terminal matched constructor ID 375");
    375
}

fn match_node_instruction_251(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 251: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_252(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 252: Terminal matched constructor ID 127");
    127
}

fn match_node_instruction_253(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 253: Terminal matched constructor ID 467");
    467
}

fn match_node_instruction_254(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 254: Terminal matched constructor ID 159");
    159
}

fn match_node_instruction_255(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 255: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_256(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 256: Terminal matched constructor ID 243");
    243
}

fn match_node_instruction_257(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 257: Terminal matched constructor ID 235");
    235
}

fn match_node_instruction_258(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 258: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_259(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 259: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_260(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (19 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 19) & 3;
    eprintln!("Trace node 260: SlaInstructionBits start=19, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_261(bytes, ctx),
        1 => match_node_instruction_262(bytes, ctx),
        2 => match_node_instruction_263(bytes, ctx),
        3 => match_node_instruction_264(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_261(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 261: Terminal matched constructor ID 226");
    226
}

fn match_node_instruction_262(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 262: Terminal matched constructor ID 224");
    224
}

fn match_node_instruction_263(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 263: Terminal matched constructor ID 426");
    426
}

fn match_node_instruction_264(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 264: Terminal matched constructor ID 428");
    428
}

fn match_node_instruction_265(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (19 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 19) & 3;
    eprintln!("Trace node 265: SlaInstructionBits start=19, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_266(bytes, ctx),
        1 => match_node_instruction_267(bytes, ctx),
        2 => match_node_instruction_268(bytes, ctx),
        3 => match_node_instruction_269(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_266(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 266: Terminal matched constructor ID 226");
    226
}

fn match_node_instruction_267(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 267: Terminal matched constructor ID 224");
    224
}

fn match_node_instruction_268(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 268: Terminal matched constructor ID 425");
    425
}

fn match_node_instruction_269(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 269: Terminal matched constructor ID 427");
    427
}

fn match_node_instruction_270(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 270: Terminal matched constructor ID 118");
    118
}

fn match_node_instruction_271(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 271: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_272(bytes, ctx),
        1 => match_node_instruction_275(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_272(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 272: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_273(bytes, ctx),
        1 => match_node_instruction_274(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_273(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 273: Terminal matched constructor ID 259");
    259
}

fn match_node_instruction_274(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 274: Terminal matched constructor ID 260");
    260
}

fn match_node_instruction_275(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 275: Terminal matched constructor ID 42");
    42
}

fn match_node_instruction_276(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 276: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_277(bytes, ctx),
        1 => match_node_instruction_280(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_277(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 277: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_278(bytes, ctx),
        1 => match_node_instruction_279(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_278(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 278: Terminal matched constructor ID 255");
    255
}

fn match_node_instruction_279(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 279: Terminal matched constructor ID 256");
    256
}

fn match_node_instruction_280(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 280: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_281(bytes, ctx),
        1 => match_node_instruction_282(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_281(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 281: Terminal matched constructor ID 322");
    322
}

fn match_node_instruction_282(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 282: Terminal matched constructor ID 323");
    323
}

fn match_node_instruction_283(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 283: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_284(bytes, ctx),
        1 => match_node_instruction_287(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_284(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 284: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_285(bytes, ctx),
        1 => match_node_instruction_286(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_285(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 285: Terminal matched constructor ID 32");
    32
}

fn match_node_instruction_286(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 286: Terminal matched constructor ID 33");
    33
}

fn match_node_instruction_287(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 287: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_288(bytes, ctx),
        1 => match_node_instruction_289(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_288(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 288: Terminal matched constructor ID 109");
    109
}

fn match_node_instruction_289(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 289: Terminal matched constructor ID 110");
    110
}

fn match_node_instruction_290(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 290: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_291(bytes, ctx),
        1 => match_node_instruction_294(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_291(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 291: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_292(bytes, ctx),
        1 => match_node_instruction_293(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_292(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 292: Terminal matched constructor ID 83");
    83
}

fn match_node_instruction_293(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 293: Terminal matched constructor ID 268");
    268
}

fn match_node_instruction_294(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 294: Terminal matched constructor ID 273");
    273
}

fn match_node_instruction_295(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 295: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_296(bytes, ctx),
        1 => match_node_instruction_297(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_296(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 296: Terminal matched constructor ID 88");
    88
}

fn match_node_instruction_297(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 297: Terminal matched constructor ID 207");
    207
}

fn match_node_instruction_298(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 298: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_299(bytes, ctx),
        1 => match_node_instruction_302(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_299(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 299: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_300(bytes, ctx),
        1 => match_node_instruction_301(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_300(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 300: Terminal matched constructor ID 464");
    464
}

fn match_node_instruction_301(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 301: Terminal matched constructor ID 465");
    465
}

fn match_node_instruction_302(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 302: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_303(bytes, ctx),
        1 => match_node_instruction_304(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_303(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 303: Terminal matched constructor ID 56");
    56
}

fn match_node_instruction_304(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 304: Terminal matched constructor ID 57");
    57
}

fn match_node_instruction_305(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 305: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_306(bytes, ctx),
        1 => match_node_instruction_309(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_306(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 306: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_307(bytes, ctx),
        1 => match_node_instruction_308(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_307(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 307: Terminal matched constructor ID 450");
    450
}

fn match_node_instruction_308(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 308: Terminal matched constructor ID 451");
    451
}

fn match_node_instruction_309(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 309: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_310(bytes, ctx),
        1 => match_node_instruction_311(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_310(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 310: Terminal matched constructor ID 269");
    269
}

fn match_node_instruction_311(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 311: Terminal matched constructor ID 270");
    270
}

fn match_node_instruction_312(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 312: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_313(bytes, ctx),
        1 => match_node_instruction_674(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_313(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 313: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_314(bytes, ctx),
        1 => match_node_instruction_531(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_314(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 314: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_315(bytes, ctx),
        1 => match_node_instruction_476(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_315(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 15;
    eprintln!("Trace node 315: SlaInstructionBits start=12, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_316(bytes, ctx),
        1 => match_node_instruction_317(bytes, ctx),
        2 => match_node_instruction_338(bytes, ctx),
        3 => match_node_instruction_339(bytes, ctx),
        4 => match_node_instruction_424(bytes, ctx),
        5 => match_node_instruction_425(bytes, ctx),
        6 => match_node_instruction_426(bytes, ctx),
        7 => match_node_instruction_427(bytes, ctx),
        8 => match_node_instruction_428(bytes, ctx),
        9 => match_node_instruction_429(bytes, ctx),
        10 => match_node_instruction_430(bytes, ctx),
        11 => match_node_instruction_431(bytes, ctx),
        12 => match_node_instruction_432(bytes, ctx),
        13 => match_node_instruction_433(bytes, ctx),
        14 => match_node_instruction_434(bytes, ctx),
        15 => match_node_instruction_455(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_316(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 316: Terminal matched constructor ID 461");
    461
}

fn match_node_instruction_317(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 317: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_318(bytes, ctx),
        1 => match_node_instruction_323(bytes, ctx),
        2 => match_node_instruction_328(bytes, ctx),
        3 => match_node_instruction_333(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_318(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 318: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_319(bytes, ctx),
        1 => match_node_instruction_320(bytes, ctx),
        2 => match_node_instruction_321(bytes, ctx),
        3 => match_node_instruction_322(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_319(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 319: Terminal matched constructor ID 213");
    213
}

fn match_node_instruction_320(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 320: Terminal matched constructor ID 45");
    45
}

fn match_node_instruction_321(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 321: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_322(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 322: Terminal matched constructor ID 25");
    25
}

fn match_node_instruction_323(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 323: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_324(bytes, ctx),
        1 => match_node_instruction_325(bytes, ctx),
        2 => match_node_instruction_326(bytes, ctx),
        3 => match_node_instruction_327(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_324(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 324: Terminal matched constructor ID 219");
    219
}

fn match_node_instruction_325(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 325: Terminal matched constructor ID 459");
    459
}

fn match_node_instruction_326(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 326: Terminal matched constructor ID 406");
    406
}

fn match_node_instruction_327(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 327: Terminal matched constructor ID 449");
    449
}

fn match_node_instruction_328(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 328: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_329(bytes, ctx),
        1 => match_node_instruction_330(bytes, ctx),
        2 => match_node_instruction_331(bytes, ctx),
        3 => match_node_instruction_332(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_329(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 329: Terminal matched constructor ID 340");
    340
}

fn match_node_instruction_330(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 330: Terminal matched constructor ID 120");
    120
}

fn match_node_instruction_331(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 331: Terminal matched constructor ID 390");
    390
}

fn match_node_instruction_332(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 332: Terminal matched constructor ID 104");
    104
}

fn match_node_instruction_333(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 333: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_334(bytes, ctx),
        1 => match_node_instruction_335(bytes, ctx),
        2 => match_node_instruction_336(bytes, ctx),
        3 => match_node_instruction_337(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_334(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 334: Terminal matched constructor ID 341");
    341
}

fn match_node_instruction_335(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 335: Terminal matched constructor ID 291");
    291
}

fn match_node_instruction_336(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 336: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_337(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 337: Terminal matched constructor ID 250");
    250
}

fn match_node_instruction_338(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 338: Terminal matched constructor ID 47");
    47
}

fn match_node_instruction_339(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 339: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_340(bytes, ctx),
        1 => match_node_instruction_377(bytes, ctx),
        2 => match_node_instruction_414(bytes, ctx),
        3 => match_node_instruction_419(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_340(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 340: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_341(bytes, ctx),
        1 => match_node_instruction_342(bytes, ctx),
        2 => match_node_instruction_343(bytes, ctx),
        3 => match_node_instruction_344(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_341(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 341: Terminal matched constructor ID 186");
    186
}

fn match_node_instruction_342(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 342: Terminal matched constructor ID 196");
    196
}

fn match_node_instruction_343(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 343: Terminal matched constructor ID 188");
    188
}

fn match_node_instruction_344(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 5 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 31;
    eprintln!("Trace node 344: SlaInstructionBits start=5, size=5, word={:08x}, probe={}", word, probe);
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
    eprintln!("Trace node 345: Terminal matched constructor ID 145");
    145
}

fn match_node_instruction_346(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 346: Terminal matched constructor ID 141");
    141
}

fn match_node_instruction_347(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 347: Terminal matched constructor ID 417");
    417
}

fn match_node_instruction_348(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 348: Terminal matched NOTHING");
    -1
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
    eprintln!("Trace node 353: Terminal matched constructor ID 468");
    468
}

fn match_node_instruction_354(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 354: Terminal matched constructor ID 149");
    149
}

fn match_node_instruction_355(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 355: Terminal matched constructor ID 413");
    413
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
    eprintln!("Trace node 363: Terminal matched constructor ID 421");
    421
}

fn match_node_instruction_364(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 364: Terminal matched NOTHING");
    -1
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
    eprintln!("Trace node 369: Terminal matched constructor ID 43");
    43
}

fn match_node_instruction_370(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 370: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_371(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 371: Terminal matched constructor ID 415");
    415
}

fn match_node_instruction_372(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 372: Terminal matched constructor ID 227");
    227
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
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 377: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_378(bytes, ctx),
        1 => match_node_instruction_379(bytes, ctx),
        2 => match_node_instruction_380(bytes, ctx),
        3 => match_node_instruction_381(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_378(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 378: Terminal matched constructor ID 187");
    187
}

fn match_node_instruction_379(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 379: Terminal matched constructor ID 197");
    197
}

fn match_node_instruction_380(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 380: Terminal matched constructor ID 189");
    189
}

fn match_node_instruction_381(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 5 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 31;
    eprintln!("Trace node 381: SlaInstructionBits start=5, size=5, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_382(bytes, ctx),
        1 => match_node_instruction_383(bytes, ctx),
        2 => match_node_instruction_384(bytes, ctx),
        3 => match_node_instruction_385(bytes, ctx),
        4 => match_node_instruction_386(bytes, ctx),
        5 => match_node_instruction_387(bytes, ctx),
        6 => match_node_instruction_388(bytes, ctx),
        7 => match_node_instruction_389(bytes, ctx),
        8 => match_node_instruction_390(bytes, ctx),
        9 => match_node_instruction_391(bytes, ctx),
        10 => match_node_instruction_392(bytes, ctx),
        11 => match_node_instruction_393(bytes, ctx),
        12 => match_node_instruction_394(bytes, ctx),
        13 => match_node_instruction_395(bytes, ctx),
        14 => match_node_instruction_396(bytes, ctx),
        15 => match_node_instruction_397(bytes, ctx),
        16 => match_node_instruction_398(bytes, ctx),
        17 => match_node_instruction_399(bytes, ctx),
        18 => match_node_instruction_400(bytes, ctx),
        19 => match_node_instruction_401(bytes, ctx),
        20 => match_node_instruction_402(bytes, ctx),
        21 => match_node_instruction_403(bytes, ctx),
        22 => match_node_instruction_404(bytes, ctx),
        23 => match_node_instruction_405(bytes, ctx),
        24 => match_node_instruction_406(bytes, ctx),
        25 => match_node_instruction_407(bytes, ctx),
        26 => match_node_instruction_408(bytes, ctx),
        27 => match_node_instruction_409(bytes, ctx),
        28 => match_node_instruction_410(bytes, ctx),
        29 => match_node_instruction_411(bytes, ctx),
        30 => match_node_instruction_412(bytes, ctx),
        31 => match_node_instruction_413(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_382(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 382: Terminal matched constructor ID 146");
    146
}

fn match_node_instruction_383(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 383: Terminal matched constructor ID 142");
    142
}

fn match_node_instruction_384(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 384: Terminal matched constructor ID 418");
    418
}

fn match_node_instruction_385(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 385: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_386(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 386: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_387(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 387: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_388(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 388: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_389(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 389: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_390(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 390: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_391(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 391: Terminal matched constructor ID 150");
    150
}

fn match_node_instruction_392(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 392: Terminal matched constructor ID 414");
    414
}

fn match_node_instruction_393(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 393: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_394(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 394: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_395(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 395: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_396(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 396: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_397(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 397: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_398(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 398: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_399(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 399: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_400(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 400: Terminal matched constructor ID 422");
    422
}

fn match_node_instruction_401(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 401: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_402(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 402: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_403(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 403: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_404(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 404: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_405(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 405: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_406(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 406: Terminal matched constructor ID 41");
    41
}

fn match_node_instruction_407(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 407: Terminal matched constructor ID 339");
    339
}

fn match_node_instruction_408(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 408: Terminal matched constructor ID 416");
    416
}

fn match_node_instruction_409(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 409: Terminal matched constructor ID 431");
    431
}

fn match_node_instruction_410(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 410: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_411(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 411: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_412(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 412: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_413(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 413: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_414(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 414: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_415(bytes, ctx),
        1 => match_node_instruction_416(bytes, ctx),
        2 => match_node_instruction_417(bytes, ctx),
        3 => match_node_instruction_418(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_415(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 415: Terminal matched constructor ID 198");
    198
}

fn match_node_instruction_416(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 416: Terminal matched constructor ID 192");
    192
}

fn match_node_instruction_417(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 417: Terminal matched constructor ID 200");
    200
}

fn match_node_instruction_418(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 418: Terminal matched constructor ID 194");
    194
}

fn match_node_instruction_419(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 419: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_420(bytes, ctx),
        1 => match_node_instruction_421(bytes, ctx),
        2 => match_node_instruction_422(bytes, ctx),
        3 => match_node_instruction_423(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_420(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 420: Terminal matched constructor ID 199");
    199
}

fn match_node_instruction_421(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 421: Terminal matched constructor ID 193");
    193
}

fn match_node_instruction_422(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 422: Terminal matched constructor ID 201");
    201
}

fn match_node_instruction_423(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 423: Terminal matched constructor ID 195");
    195
}

fn match_node_instruction_424(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 424: Terminal matched constructor ID 306");
    306
}

fn match_node_instruction_425(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 425: Terminal matched constructor ID 325");
    325
}

fn match_node_instruction_426(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 426: Terminal matched constructor ID 112");
    112
}

fn match_node_instruction_427(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 427: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_428(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 428: Terminal matched constructor ID 36");
    36
}

fn match_node_instruction_429(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 429: Terminal matched constructor ID 482");
    482
}

fn match_node_instruction_430(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 430: Terminal matched constructor ID 453");
    453
}

fn match_node_instruction_431(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 431: Terminal matched constructor ID 262");
    262
}

fn match_node_instruction_432(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 432: Terminal matched constructor ID 297");
    297
}

fn match_node_instruction_433(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 433: Terminal matched constructor ID 59");
    59
}

fn match_node_instruction_434(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 434: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_435(bytes, ctx),
        1 => match_node_instruction_440(bytes, ctx),
        2 => match_node_instruction_445(bytes, ctx),
        3 => match_node_instruction_450(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_435(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 435: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_436(bytes, ctx),
        1 => match_node_instruction_437(bytes, ctx),
        2 => match_node_instruction_438(bytes, ctx),
        3 => match_node_instruction_439(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_436(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 436: Terminal matched constructor ID 147");
    147
}

fn match_node_instruction_437(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 437: Terminal matched constructor ID 478");
    478
}

fn match_node_instruction_438(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 438: Terminal matched constructor ID 183");
    183
}

fn match_node_instruction_439(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 439: Terminal matched constructor ID 138");
    138
}

fn match_node_instruction_440(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 440: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_441(bytes, ctx),
        1 => match_node_instruction_442(bytes, ctx),
        2 => match_node_instruction_443(bytes, ctx),
        3 => match_node_instruction_444(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_441(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 441: Terminal matched constructor ID 143");
    143
}

fn match_node_instruction_442(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 442: Terminal matched constructor ID 151");
    151
}

fn match_node_instruction_443(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 443: Terminal matched constructor ID 315");
    315
}

fn match_node_instruction_444(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 444: Terminal matched constructor ID 17");
    17
}

fn match_node_instruction_445(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 445: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_446(bytes, ctx),
        1 => match_node_instruction_447(bytes, ctx),
        2 => match_node_instruction_448(bytes, ctx),
        3 => match_node_instruction_449(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_446(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 446: Terminal matched constructor ID 21");
    21
}

fn match_node_instruction_447(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 447: Terminal matched constructor ID 100");
    100
}

fn match_node_instruction_448(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 448: Terminal matched constructor ID 353");
    353
}

fn match_node_instruction_449(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 449: Terminal matched constructor ID 349");
    349
}

fn match_node_instruction_450(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 450: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_451(bytes, ctx),
        1 => match_node_instruction_452(bytes, ctx),
        2 => match_node_instruction_453(bytes, ctx),
        3 => match_node_instruction_454(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_451(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 451: Terminal matched constructor ID 445");
    445
}

fn match_node_instruction_452(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 452: Terminal matched constructor ID 462");
    462
}

fn match_node_instruction_453(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 453: Terminal matched constructor ID 408");
    408
}

fn match_node_instruction_454(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 454: Terminal matched constructor ID 392");
    392
}

fn match_node_instruction_455(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 455: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_456(bytes, ctx),
        1 => match_node_instruction_461(bytes, ctx),
        2 => match_node_instruction_466(bytes, ctx),
        3 => match_node_instruction_471(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_456(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 456: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_457(bytes, ctx),
        1 => match_node_instruction_458(bytes, ctx),
        2 => match_node_instruction_459(bytes, ctx),
        3 => match_node_instruction_460(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_457(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 457: Terminal matched constructor ID 302");
    302
}

fn match_node_instruction_458(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 458: Terminal matched constructor ID 437");
    437
}

fn match_node_instruction_459(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 459: Terminal matched constructor ID 278");
    278
}

fn match_node_instruction_460(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 460: Terminal matched constructor ID 275");
    275
}

fn match_node_instruction_461(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 461: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_462(bytes, ctx),
        1 => match_node_instruction_463(bytes, ctx),
        2 => match_node_instruction_464(bytes, ctx),
        3 => match_node_instruction_465(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_462(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 462: Terminal matched constructor ID 293");
    293
}

fn match_node_instruction_463(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 463: Terminal matched constructor ID 433");
    433
}

fn match_node_instruction_464(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 464: Terminal matched constructor ID 5");
    5
}

fn match_node_instruction_465(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 465: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_466(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 466: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_467(bytes, ctx),
        1 => match_node_instruction_468(bytes, ctx),
        2 => match_node_instruction_469(bytes, ctx),
        3 => match_node_instruction_470(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_467(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 467: Terminal matched constructor ID 319");
    319
}

fn match_node_instruction_468(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 468: Terminal matched constructor ID 246");
    246
}

fn match_node_instruction_469(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 469: Terminal matched constructor ID 310");
    310
}

fn match_node_instruction_470(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 470: Terminal matched constructor ID 329");
    329
}

fn match_node_instruction_471(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 471: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_472(bytes, ctx),
        1 => match_node_instruction_473(bytes, ctx),
        2 => match_node_instruction_474(bytes, ctx),
        3 => match_node_instruction_475(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_472(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 472: Terminal matched constructor ID 53");
    53
}

fn match_node_instruction_473(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 473: Terminal matched constructor ID 441");
    441
}

fn match_node_instruction_474(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 474: Terminal matched constructor ID 365");
    365
}

fn match_node_instruction_475(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 475: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_476(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 476: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_477(bytes, ctx),
        1 => match_node_instruction_504(bytes, ctx),
        2 => match_node_instruction_515(bytes, ctx),
        3 => match_node_instruction_524(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_477(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 15;
    eprintln!("Trace node 477: SlaInstructionBits start=12, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_478(bytes, ctx),
        1 => match_node_instruction_479(bytes, ctx),
        2 => match_node_instruction_482(bytes, ctx),
        3 => match_node_instruction_483(bytes, ctx),
        4 => match_node_instruction_488(bytes, ctx),
        5 => match_node_instruction_489(bytes, ctx),
        6 => match_node_instruction_490(bytes, ctx),
        7 => match_node_instruction_491(bytes, ctx),
        8 => match_node_instruction_492(bytes, ctx),
        9 => match_node_instruction_493(bytes, ctx),
        10 => match_node_instruction_494(bytes, ctx),
        11 => match_node_instruction_495(bytes, ctx),
        12 => match_node_instruction_496(bytes, ctx),
        13 => match_node_instruction_497(bytes, ctx),
        14 => match_node_instruction_498(bytes, ctx),
        15 => match_node_instruction_503(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_478(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 478: Terminal matched constructor ID 461");
    461
}

fn match_node_instruction_479(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 479: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_480(bytes, ctx),
        1 => match_node_instruction_481(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_480(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 480: Terminal matched constructor ID 412");
    412
}

fn match_node_instruction_481(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 481: Terminal matched constructor ID 396");
    396
}

fn match_node_instruction_482(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 482: Terminal matched constructor ID 47");
    47
}

fn match_node_instruction_483(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 483: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_484(bytes, ctx),
        1 => match_node_instruction_485(bytes, ctx),
        2 => match_node_instruction_486(bytes, ctx),
        3 => match_node_instruction_487(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_484(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 484: Terminal matched constructor ID 190");
    190
}

fn match_node_instruction_485(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 485: Terminal matched constructor ID 191");
    191
}

fn match_node_instruction_486(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 486: Terminal matched constructor ID 202");
    202
}

fn match_node_instruction_487(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 487: Terminal matched constructor ID 203");
    203
}

fn match_node_instruction_488(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 488: Terminal matched constructor ID 306");
    306
}

fn match_node_instruction_489(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 489: Terminal matched constructor ID 325");
    325
}

fn match_node_instruction_490(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 490: Terminal matched constructor ID 112");
    112
}

fn match_node_instruction_491(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 491: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_492(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 492: Terminal matched constructor ID 36");
    36
}

fn match_node_instruction_493(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 493: Terminal matched constructor ID 482");
    482
}

fn match_node_instruction_494(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 494: Terminal matched constructor ID 453");
    453
}

fn match_node_instruction_495(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 495: Terminal matched constructor ID 262");
    262
}

fn match_node_instruction_496(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 496: Terminal matched constructor ID 297");
    297
}

fn match_node_instruction_497(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 497: Terminal matched constructor ID 59");
    59
}

fn match_node_instruction_498(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 498: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_499(bytes, ctx),
        1 => match_node_instruction_500(bytes, ctx),
        2 => match_node_instruction_501(bytes, ctx),
        3 => match_node_instruction_502(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_499(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 499: Terminal matched constructor ID 337");
    337
}

fn match_node_instruction_500(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 500: Terminal matched constructor ID 172");
    172
}

fn match_node_instruction_501(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 501: Terminal matched constructor ID 65");
    65
}

fn match_node_instruction_502(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 502: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_503(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 503: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_504(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 7;
    eprintln!("Trace node 504: SlaInstructionBits start=10, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_505(bytes, ctx),
        1 => match_node_instruction_506(bytes, ctx),
        2 => match_node_instruction_509(bytes, ctx),
        3 => match_node_instruction_510(bytes, ctx),
        4 => match_node_instruction_511(bytes, ctx),
        5 => match_node_instruction_512(bytes, ctx),
        6 => match_node_instruction_513(bytes, ctx),
        7 => match_node_instruction_514(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_505(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 505: Terminal matched constructor ID 87");
    87
}

fn match_node_instruction_506(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 506: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_507(bytes, ctx),
        1 => match_node_instruction_508(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_507(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 507: Terminal matched constructor ID 471");
    471
}

fn match_node_instruction_508(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 508: Terminal matched constructor ID 472");
    472
}

fn match_node_instruction_509(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 509: Terminal matched constructor ID 68");
    68
}

fn match_node_instruction_510(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 510: Terminal matched constructor ID 75");
    75
}

fn match_node_instruction_511(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 511: Terminal matched constructor ID 90");
    90
}

fn match_node_instruction_512(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 512: Terminal matched constructor ID 92");
    92
}

fn match_node_instruction_513(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 513: Terminal matched constructor ID 64");
    64
}

fn match_node_instruction_514(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 514: Terminal matched constructor ID 85");
    85
}

fn match_node_instruction_515(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 515: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_516(bytes, ctx),
        1 => match_node_instruction_519(bytes, ctx),
        2 => match_node_instruction_520(bytes, ctx),
        3 => match_node_instruction_523(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_516(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 516: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_517(bytes, ctx),
        1 => match_node_instruction_518(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_517(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 517: Terminal matched constructor ID 402");
    402
}

fn match_node_instruction_518(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 518: Terminal matched constructor ID 398");
    398
}

fn match_node_instruction_519(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 519: Terminal matched constructor ID 106");
    106
}

fn match_node_instruction_520(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 520: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_521(bytes, ctx),
        1 => match_node_instruction_522(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_521(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 521: Terminal matched constructor ID 361");
    361
}

fn match_node_instruction_522(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 522: Terminal matched constructor ID 357");
    357
}

fn match_node_instruction_523(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 523: Terminal matched constructor ID 27");
    27
}

fn match_node_instruction_524(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 524: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_525(bytes, ctx),
        1 => match_node_instruction_528(bytes, ctx),
        2 => match_node_instruction_529(bytes, ctx),
        3 => match_node_instruction_530(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_525(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 525: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_526(bytes, ctx),
        1 => match_node_instruction_527(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_526(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 526: Terminal matched constructor ID 386");
    386
}

fn match_node_instruction_527(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 527: Terminal matched constructor ID 382");
    382
}

fn match_node_instruction_528(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 528: Terminal matched constructor ID 49");
    49
}

fn match_node_instruction_529(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 529: Terminal matched constructor ID 252");
    252
}

fn match_node_instruction_530(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 530: Terminal matched constructor ID 31");
    31
}

fn match_node_instruction_531(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 531: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_532(bytes, ctx),
        1 => match_node_instruction_589(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_532(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 15;
    eprintln!("Trace node 532: SlaInstructionBits start=12, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_533(bytes, ctx),
        1 => match_node_instruction_534(bytes, ctx),
        2 => match_node_instruction_535(bytes, ctx),
        3 => match_node_instruction_536(bytes, ctx),
        4 => match_node_instruction_537(bytes, ctx),
        5 => match_node_instruction_538(bytes, ctx),
        6 => match_node_instruction_539(bytes, ctx),
        7 => match_node_instruction_540(bytes, ctx),
        8 => match_node_instruction_541(bytes, ctx),
        9 => match_node_instruction_542(bytes, ctx),
        10 => match_node_instruction_543(bytes, ctx),
        11 => match_node_instruction_544(bytes, ctx),
        12 => match_node_instruction_545(bytes, ctx),
        13 => match_node_instruction_546(bytes, ctx),
        14 => match_node_instruction_547(bytes, ctx),
        15 => match_node_instruction_568(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_533(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 533: Terminal matched constructor ID 457");
    457
}

fn match_node_instruction_534(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 534: Terminal matched constructor ID 116");
    116
}

fn match_node_instruction_535(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 535: Terminal matched constructor ID 40");
    40
}

fn match_node_instruction_536(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 536: Terminal matched constructor ID 266");
    266
}

fn match_node_instruction_537(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 537: Terminal matched constructor ID 308");
    308
}

fn match_node_instruction_538(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 538: Terminal matched constructor ID 327");
    327
}

fn match_node_instruction_539(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 539: Terminal matched constructor ID 114");
    114
}

fn match_node_instruction_540(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 540: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_541(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 541: Terminal matched constructor ID 38");
    38
}

fn match_node_instruction_542(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 542: Terminal matched constructor ID 484");
    484
}

fn match_node_instruction_543(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 543: Terminal matched constructor ID 455");
    455
}

fn match_node_instruction_544(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 544: Terminal matched constructor ID 264");
    264
}

fn match_node_instruction_545(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 545: Terminal matched constructor ID 299");
    299
}

fn match_node_instruction_546(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 546: Terminal matched constructor ID 61");
    61
}

fn match_node_instruction_547(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 547: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_548(bytes, ctx),
        1 => match_node_instruction_553(bytes, ctx),
        2 => match_node_instruction_558(bytes, ctx),
        3 => match_node_instruction_563(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_548(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 548: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_549(bytes, ctx),
        1 => match_node_instruction_550(bytes, ctx),
        2 => match_node_instruction_551(bytes, ctx),
        3 => match_node_instruction_552(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_549(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 549: Terminal matched constructor ID 148");
    148
}

fn match_node_instruction_550(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 550: Terminal matched constructor ID 480");
    480
}

fn match_node_instruction_551(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 551: Terminal matched constructor ID 185");
    185
}

fn match_node_instruction_552(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 552: Terminal matched constructor ID 140");
    140
}

fn match_node_instruction_553(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 553: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_554(bytes, ctx),
        1 => match_node_instruction_555(bytes, ctx),
        2 => match_node_instruction_556(bytes, ctx),
        3 => match_node_instruction_557(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_554(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 554: Terminal matched constructor ID 144");
    144
}

fn match_node_instruction_555(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 555: Terminal matched constructor ID 152");
    152
}

fn match_node_instruction_556(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 556: Terminal matched constructor ID 317");
    317
}

fn match_node_instruction_557(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 557: Terminal matched constructor ID 19");
    19
}

fn match_node_instruction_558(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 558: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_559(bytes, ctx),
        1 => match_node_instruction_560(bytes, ctx),
        2 => match_node_instruction_561(bytes, ctx),
        3 => match_node_instruction_562(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_559(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 559: Terminal matched constructor ID 23");
    23
}

fn match_node_instruction_560(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 560: Terminal matched constructor ID 102");
    102
}

fn match_node_instruction_561(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 561: Terminal matched constructor ID 355");
    355
}

fn match_node_instruction_562(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 562: Terminal matched constructor ID 351");
    351
}

fn match_node_instruction_563(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 563: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_564(bytes, ctx),
        1 => match_node_instruction_565(bytes, ctx),
        2 => match_node_instruction_566(bytes, ctx),
        3 => match_node_instruction_567(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_564(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 564: Terminal matched constructor ID 447");
    447
}

fn match_node_instruction_565(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 565: Terminal matched constructor ID 463");
    463
}

fn match_node_instruction_566(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 566: Terminal matched constructor ID 410");
    410
}

fn match_node_instruction_567(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 567: Terminal matched constructor ID 394");
    394
}

fn match_node_instruction_568(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 568: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_569(bytes, ctx),
        1 => match_node_instruction_574(bytes, ctx),
        2 => match_node_instruction_579(bytes, ctx),
        3 => match_node_instruction_584(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_569(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 569: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_570(bytes, ctx),
        1 => match_node_instruction_571(bytes, ctx),
        2 => match_node_instruction_572(bytes, ctx),
        3 => match_node_instruction_573(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_570(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 570: Terminal matched constructor ID 304");
    304
}

fn match_node_instruction_571(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 571: Terminal matched constructor ID 439");
    439
}

fn match_node_instruction_572(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 572: Terminal matched constructor ID 279");
    279
}

fn match_node_instruction_573(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 573: Terminal matched constructor ID 277");
    277
}

fn match_node_instruction_574(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 574: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_575(bytes, ctx),
        1 => match_node_instruction_576(bytes, ctx),
        2 => match_node_instruction_577(bytes, ctx),
        3 => match_node_instruction_578(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_575(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 575: Terminal matched constructor ID 295");
    295
}

fn match_node_instruction_576(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 576: Terminal matched constructor ID 435");
    435
}

fn match_node_instruction_577(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 577: Terminal matched constructor ID 7");
    7
}

fn match_node_instruction_578(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 578: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_579(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 579: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_580(bytes, ctx),
        1 => match_node_instruction_581(bytes, ctx),
        2 => match_node_instruction_582(bytes, ctx),
        3 => match_node_instruction_583(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_580(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 580: Terminal matched constructor ID 321");
    321
}

fn match_node_instruction_581(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 581: Terminal matched constructor ID 248");
    248
}

fn match_node_instruction_582(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 582: Terminal matched constructor ID 312");
    312
}

fn match_node_instruction_583(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 583: Terminal matched constructor ID 331");
    331
}

fn match_node_instruction_584(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 584: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_585(bytes, ctx),
        1 => match_node_instruction_586(bytes, ctx),
        2 => match_node_instruction_587(bytes, ctx),
        3 => match_node_instruction_588(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_585(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 585: Terminal matched constructor ID 55");
    55
}

fn match_node_instruction_586(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 586: Terminal matched constructor ID 443");
    443
}

fn match_node_instruction_587(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 587: Terminal matched constructor ID 367");
    367
}

fn match_node_instruction_588(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 588: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_589(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 589: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_590(bytes, ctx),
        1 => match_node_instruction_613(bytes, ctx),
        2 => match_node_instruction_658(bytes, ctx),
        3 => match_node_instruction_667(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_590(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 15;
    eprintln!("Trace node 590: SlaInstructionBits start=12, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_591(bytes, ctx),
        1 => match_node_instruction_592(bytes, ctx),
        2 => match_node_instruction_593(bytes, ctx),
        3 => match_node_instruction_594(bytes, ctx),
        4 => match_node_instruction_595(bytes, ctx),
        5 => match_node_instruction_596(bytes, ctx),
        6 => match_node_instruction_597(bytes, ctx),
        7 => match_node_instruction_598(bytes, ctx),
        8 => match_node_instruction_599(bytes, ctx),
        9 => match_node_instruction_600(bytes, ctx),
        10 => match_node_instruction_601(bytes, ctx),
        11 => match_node_instruction_602(bytes, ctx),
        12 => match_node_instruction_603(bytes, ctx),
        13 => match_node_instruction_604(bytes, ctx),
        14 => match_node_instruction_605(bytes, ctx),
        15 => match_node_instruction_610(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_591(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 591: Terminal matched constructor ID 457");
    457
}

fn match_node_instruction_592(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 592: Terminal matched constructor ID 116");
    116
}

fn match_node_instruction_593(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 593: Terminal matched constructor ID 40");
    40
}

fn match_node_instruction_594(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 594: Terminal matched constructor ID 266");
    266
}

fn match_node_instruction_595(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 595: Terminal matched constructor ID 308");
    308
}

fn match_node_instruction_596(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 596: Terminal matched constructor ID 327");
    327
}

fn match_node_instruction_597(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 597: Terminal matched constructor ID 114");
    114
}

fn match_node_instruction_598(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 598: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_599(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 599: Terminal matched constructor ID 38");
    38
}

fn match_node_instruction_600(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 600: Terminal matched constructor ID 484");
    484
}

fn match_node_instruction_601(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 601: Terminal matched constructor ID 455");
    455
}

fn match_node_instruction_602(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 602: Terminal matched constructor ID 264");
    264
}

fn match_node_instruction_603(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 603: Terminal matched constructor ID 299");
    299
}

fn match_node_instruction_604(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 604: Terminal matched constructor ID 61");
    61
}

fn match_node_instruction_605(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 605: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_606(bytes, ctx),
        1 => match_node_instruction_607(bytes, ctx),
        2 => match_node_instruction_608(bytes, ctx),
        3 => match_node_instruction_609(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_606(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 606: Terminal matched constructor ID 338");
    338
}

fn match_node_instruction_607(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 607: Terminal matched constructor ID 174");
    174
}

fn match_node_instruction_608(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 608: Terminal matched constructor ID 66");
    66
}

fn match_node_instruction_609(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 609: Terminal matched constructor ID 300");
    300
}

fn match_node_instruction_610(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 610: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_611(bytes, ctx),
        1 => match_node_instruction_612(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_611(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 611: Terminal matched constructor ID 211");
    211
}

fn match_node_instruction_612(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 612: Terminal matched constructor ID 217");
    217
}

fn match_node_instruction_613(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 613: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_614(bytes, ctx),
        1 => match_node_instruction_619(bytes, ctx),
        2 => match_node_instruction_624(bytes, ctx),
        3 => match_node_instruction_657(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_614(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 614: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_615(bytes, ctx),
        1 => match_node_instruction_616(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_615(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 615: Terminal matched constructor ID 225");
    225
}

fn match_node_instruction_616(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 616: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_617(bytes, ctx),
        1 => match_node_instruction_618(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_617(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 617: Terminal matched constructor ID 475");
    475
}

fn match_node_instruction_618(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 618: Terminal matched constructor ID 476");
    476
}

fn match_node_instruction_619(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 619: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_620(bytes, ctx),
        1 => match_node_instruction_621(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_620(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 620: Terminal matched constructor ID 430");
    430
}

fn match_node_instruction_621(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 621: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_622(bytes, ctx),
        1 => match_node_instruction_623(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_622(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 622: Terminal matched constructor ID 280");
    280
}

fn match_node_instruction_623(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 623: Terminal matched constructor ID 281");
    281
}

fn match_node_instruction_624(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 5 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 31;
    eprintln!("Trace node 624: SlaInstructionBits start=5, size=5, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_625(bytes, ctx),
        1 => match_node_instruction_626(bytes, ctx),
        2 => match_node_instruction_627(bytes, ctx),
        3 => match_node_instruction_628(bytes, ctx),
        4 => match_node_instruction_629(bytes, ctx),
        5 => match_node_instruction_630(bytes, ctx),
        6 => match_node_instruction_631(bytes, ctx),
        7 => match_node_instruction_632(bytes, ctx),
        8 => match_node_instruction_633(bytes, ctx),
        9 => match_node_instruction_634(bytes, ctx),
        10 => match_node_instruction_635(bytes, ctx),
        11 => match_node_instruction_636(bytes, ctx),
        12 => match_node_instruction_637(bytes, ctx),
        13 => match_node_instruction_638(bytes, ctx),
        14 => match_node_instruction_639(bytes, ctx),
        15 => match_node_instruction_640(bytes, ctx),
        16 => match_node_instruction_641(bytes, ctx),
        17 => match_node_instruction_642(bytes, ctx),
        18 => match_node_instruction_643(bytes, ctx),
        19 => match_node_instruction_644(bytes, ctx),
        20 => match_node_instruction_645(bytes, ctx),
        21 => match_node_instruction_646(bytes, ctx),
        22 => match_node_instruction_647(bytes, ctx),
        23 => match_node_instruction_648(bytes, ctx),
        24 => match_node_instruction_649(bytes, ctx),
        25 => match_node_instruction_650(bytes, ctx),
        26 => match_node_instruction_651(bytes, ctx),
        27 => match_node_instruction_652(bytes, ctx),
        28 => match_node_instruction_653(bytes, ctx),
        29 => match_node_instruction_654(bytes, ctx),
        30 => match_node_instruction_655(bytes, ctx),
        31 => match_node_instruction_656(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_625(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 625: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_626(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 626: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_627(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 627: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_628(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 628: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_629(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 629: Terminal matched constructor ID 343");
    343
}

fn match_node_instruction_630(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 630: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_631(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 631: Terminal matched constructor ID 342");
    342
}

fn match_node_instruction_632(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 632: Terminal matched constructor ID 181");
    181
}

fn match_node_instruction_633(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 633: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_634(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 634: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_635(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 635: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_636(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 636: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_637(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 637: Terminal matched constructor ID 333");
    333
}

fn match_node_instruction_638(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 638: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_639(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 639: Terminal matched constructor ID 332");
    332
}

fn match_node_instruction_640(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 640: Terminal matched constructor ID 179");
    179
}

fn match_node_instruction_641(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 641: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_642(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 642: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_643(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 643: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_644(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 644: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_645(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 645: Terminal matched constructor ID 222");
    222
}

fn match_node_instruction_646(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 646: Terminal matched constructor ID 223");
    223
}

fn match_node_instruction_647(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 647: Terminal matched constructor ID 221");
    221
}

fn match_node_instruction_648(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 648: Terminal matched constructor ID 228");
    228
}

fn match_node_instruction_649(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 649: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_650(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 650: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_651(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 651: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_652(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 652: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_653(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 653: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_654(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 654: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_655(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 655: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_656(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 656: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_657(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 657: Terminal matched constructor ID 378");
    378
}

fn match_node_instruction_658(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 658: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_659(bytes, ctx),
        1 => match_node_instruction_662(bytes, ctx),
        2 => match_node_instruction_663(bytes, ctx),
        3 => match_node_instruction_666(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_659(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 659: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_660(bytes, ctx),
        1 => match_node_instruction_661(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_660(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 660: Terminal matched constructor ID 404");
    404
}

fn match_node_instruction_661(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 661: Terminal matched constructor ID 400");
    400
}

fn match_node_instruction_662(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 662: Terminal matched constructor ID 108");
    108
}

fn match_node_instruction_663(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 663: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_664(bytes, ctx),
        1 => match_node_instruction_665(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_664(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 664: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_665(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 665: Terminal matched constructor ID 359");
    359
}

fn match_node_instruction_666(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 666: Terminal matched constructor ID 29");
    29
}

fn match_node_instruction_667(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 667: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_668(bytes, ctx),
        1 => match_node_instruction_671(bytes, ctx),
        2 => match_node_instruction_672(bytes, ctx),
        3 => match_node_instruction_673(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_668(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 668: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_669(bytes, ctx),
        1 => match_node_instruction_670(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_669(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 669: Terminal matched constructor ID 388");
    388
}

fn match_node_instruction_670(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 670: Terminal matched constructor ID 384");
    384
}

fn match_node_instruction_671(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 671: Terminal matched constructor ID 51");
    51
}

fn match_node_instruction_672(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 672: Terminal matched constructor ID 254");
    254
}

fn match_node_instruction_673(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 673: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_674(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 3;
    eprintln!("Trace node 674: SlaInstructionBits start=5, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_675(bytes, ctx),
        1 => match_node_instruction_942(bytes, ctx),
        2 => match_node_instruction_943(bytes, ctx),
        3 => match_node_instruction_1018(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_675(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 675: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_676(bytes, ctx),
        1 => match_node_instruction_835(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_676(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 676: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_677(bytes, ctx),
        1 => match_node_instruction_780(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_677(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 15;
    eprintln!("Trace node 677: SlaInstructionBits start=12, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_678(bytes, ctx),
        1 => match_node_instruction_679(bytes, ctx),
        2 => match_node_instruction_698(bytes, ctx),
        3 => match_node_instruction_699(bytes, ctx),
        4 => match_node_instruction_728(bytes, ctx),
        5 => match_node_instruction_729(bytes, ctx),
        6 => match_node_instruction_730(bytes, ctx),
        7 => match_node_instruction_731(bytes, ctx),
        8 => match_node_instruction_732(bytes, ctx),
        9 => match_node_instruction_733(bytes, ctx),
        10 => match_node_instruction_734(bytes, ctx),
        11 => match_node_instruction_735(bytes, ctx),
        12 => match_node_instruction_736(bytes, ctx),
        13 => match_node_instruction_737(bytes, ctx),
        14 => match_node_instruction_738(bytes, ctx),
        15 => match_node_instruction_759(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_678(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 678: Terminal matched constructor ID 460");
    460
}

fn match_node_instruction_679(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 679: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_680(bytes, ctx),
        1 => match_node_instruction_685(bytes, ctx),
        2 => match_node_instruction_690(bytes, ctx),
        3 => match_node_instruction_695(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_680(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 680: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_681(bytes, ctx),
        1 => match_node_instruction_682(bytes, ctx),
        2 => match_node_instruction_683(bytes, ctx),
        3 => match_node_instruction_684(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_681(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 681: Terminal matched constructor ID 212");
    212
}

fn match_node_instruction_682(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 682: Terminal matched constructor ID 44");
    44
}

fn match_node_instruction_683(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 683: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_684(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 684: Terminal matched constructor ID 24");
    24
}

fn match_node_instruction_685(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 685: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_686(bytes, ctx),
        1 => match_node_instruction_687(bytes, ctx),
        2 => match_node_instruction_688(bytes, ctx),
        3 => match_node_instruction_689(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_686(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 686: Terminal matched constructor ID 218");
    218
}

fn match_node_instruction_687(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 687: Terminal matched constructor ID 458");
    458
}

fn match_node_instruction_688(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 688: Terminal matched constructor ID 405");
    405
}

fn match_node_instruction_689(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 689: Terminal matched constructor ID 448");
    448
}

fn match_node_instruction_690(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 690: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_691(bytes, ctx),
        1 => match_node_instruction_692(bytes, ctx),
        2 => match_node_instruction_693(bytes, ctx),
        3 => match_node_instruction_694(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_691(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 691: Terminal matched constructor ID 340");
    340
}

fn match_node_instruction_692(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 692: Terminal matched constructor ID 119");
    119
}

fn match_node_instruction_693(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 693: Terminal matched constructor ID 389");
    389
}

fn match_node_instruction_694(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 694: Terminal matched constructor ID 103");
    103
}

fn match_node_instruction_695(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 695: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_696(bytes, ctx),
        1 => match_node_instruction_697(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_696(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 696: Terminal matched constructor ID 290");
    290
}

fn match_node_instruction_697(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 697: Terminal matched constructor ID 249");
    249
}

fn match_node_instruction_698(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 698: Terminal matched constructor ID 46");
    46
}

fn match_node_instruction_699(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 699: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_700(bytes, ctx),
        1 => match_node_instruction_709(bytes, ctx),
        2 => match_node_instruction_718(bytes, ctx),
        3 => match_node_instruction_723(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_700(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 700: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_701(bytes, ctx),
        1 => match_node_instruction_702(bytes, ctx),
        2 => match_node_instruction_703(bytes, ctx),
        3 => match_node_instruction_704(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_701(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 701: Terminal matched constructor ID 186");
    186
}

fn match_node_instruction_702(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 702: Terminal matched constructor ID 196");
    196
}

fn match_node_instruction_703(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 703: Terminal matched constructor ID 188");
    188
}

fn match_node_instruction_704(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 3;
    eprintln!("Trace node 704: SlaInstructionBits start=8, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_705(bytes, ctx),
        1 => match_node_instruction_706(bytes, ctx),
        2 => match_node_instruction_707(bytes, ctx),
        3 => match_node_instruction_708(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_705(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 705: Terminal matched constructor ID 423");
    423
}

fn match_node_instruction_706(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 706: Terminal matched constructor ID 346");
    346
}

fn match_node_instruction_707(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 707: Terminal matched constructor ID 419");
    419
}

fn match_node_instruction_708(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 708: Terminal matched constructor ID 379");
    379
}

fn match_node_instruction_709(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 709: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_710(bytes, ctx),
        1 => match_node_instruction_711(bytes, ctx),
        2 => match_node_instruction_712(bytes, ctx),
        3 => match_node_instruction_713(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_710(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 710: Terminal matched constructor ID 187");
    187
}

fn match_node_instruction_711(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 711: Terminal matched constructor ID 197");
    197
}

fn match_node_instruction_712(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 712: Terminal matched constructor ID 189");
    189
}

fn match_node_instruction_713(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 3;
    eprintln!("Trace node 713: SlaInstructionBits start=8, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_714(bytes, ctx),
        1 => match_node_instruction_715(bytes, ctx),
        2 => match_node_instruction_716(bytes, ctx),
        3 => match_node_instruction_717(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_714(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 714: Terminal matched constructor ID 424");
    424
}

fn match_node_instruction_715(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 715: Terminal matched constructor ID 347");
    347
}

fn match_node_instruction_716(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 716: Terminal matched constructor ID 420");
    420
}

fn match_node_instruction_717(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 717: Terminal matched constructor ID 380");
    380
}

fn match_node_instruction_718(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 718: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_719(bytes, ctx),
        1 => match_node_instruction_720(bytes, ctx),
        2 => match_node_instruction_721(bytes, ctx),
        3 => match_node_instruction_722(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_719(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 719: Terminal matched constructor ID 198");
    198
}

fn match_node_instruction_720(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 720: Terminal matched constructor ID 192");
    192
}

fn match_node_instruction_721(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 721: Terminal matched constructor ID 200");
    200
}

fn match_node_instruction_722(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 722: Terminal matched constructor ID 194");
    194
}

fn match_node_instruction_723(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 723: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_724(bytes, ctx),
        1 => match_node_instruction_725(bytes, ctx),
        2 => match_node_instruction_726(bytes, ctx),
        3 => match_node_instruction_727(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_724(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 724: Terminal matched constructor ID 199");
    199
}

fn match_node_instruction_725(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 725: Terminal matched constructor ID 193");
    193
}

fn match_node_instruction_726(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 726: Terminal matched constructor ID 201");
    201
}

fn match_node_instruction_727(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 727: Terminal matched constructor ID 195");
    195
}

fn match_node_instruction_728(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 728: Terminal matched constructor ID 305");
    305
}

fn match_node_instruction_729(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 729: Terminal matched constructor ID 324");
    324
}

fn match_node_instruction_730(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 730: Terminal matched constructor ID 111");
    111
}

fn match_node_instruction_731(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 731: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_732(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 732: Terminal matched constructor ID 35");
    35
}

fn match_node_instruction_733(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 733: Terminal matched constructor ID 481");
    481
}

fn match_node_instruction_734(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 734: Terminal matched constructor ID 452");
    452
}

fn match_node_instruction_735(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 735: Terminal matched constructor ID 261");
    261
}

fn match_node_instruction_736(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 736: Terminal matched constructor ID 296");
    296
}

fn match_node_instruction_737(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 737: Terminal matched constructor ID 58");
    58
}

fn match_node_instruction_738(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 738: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_739(bytes, ctx),
        1 => match_node_instruction_744(bytes, ctx),
        2 => match_node_instruction_749(bytes, ctx),
        3 => match_node_instruction_754(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_739(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 739: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_740(bytes, ctx),
        1 => match_node_instruction_741(bytes, ctx),
        2 => match_node_instruction_742(bytes, ctx),
        3 => match_node_instruction_743(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_740(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 740: Terminal matched constructor ID 147");
    147
}

fn match_node_instruction_741(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 741: Terminal matched constructor ID 143");
    143
}

fn match_node_instruction_742(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 742: Terminal matched constructor ID 20");
    20
}

fn match_node_instruction_743(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 743: Terminal matched constructor ID 444");
    444
}

fn match_node_instruction_744(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 744: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_745(bytes, ctx),
        1 => match_node_instruction_746(bytes, ctx),
        2 => match_node_instruction_747(bytes, ctx),
        3 => match_node_instruction_748(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_745(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 745: Terminal matched constructor ID 477");
    477
}

fn match_node_instruction_746(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 746: Terminal matched constructor ID 151");
    151
}

fn match_node_instruction_747(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 747: Terminal matched constructor ID 99");
    99
}

fn match_node_instruction_748(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 748: Terminal matched constructor ID 462");
    462
}

fn match_node_instruction_749(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 749: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_750(bytes, ctx),
        1 => match_node_instruction_751(bytes, ctx),
        2 => match_node_instruction_752(bytes, ctx),
        3 => match_node_instruction_753(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_750(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 750: Terminal matched constructor ID 182");
    182
}

fn match_node_instruction_751(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 751: Terminal matched constructor ID 314");
    314
}

fn match_node_instruction_752(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 752: Terminal matched constructor ID 352");
    352
}

fn match_node_instruction_753(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 753: Terminal matched constructor ID 407");
    407
}

fn match_node_instruction_754(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 754: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_755(bytes, ctx),
        1 => match_node_instruction_756(bytes, ctx),
        2 => match_node_instruction_757(bytes, ctx),
        3 => match_node_instruction_758(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_755(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 755: Terminal matched constructor ID 137");
    137
}

fn match_node_instruction_756(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 756: Terminal matched constructor ID 16");
    16
}

fn match_node_instruction_757(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 757: Terminal matched constructor ID 348");
    348
}

fn match_node_instruction_758(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 758: Terminal matched constructor ID 391");
    391
}

fn match_node_instruction_759(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 759: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_760(bytes, ctx),
        1 => match_node_instruction_765(bytes, ctx),
        2 => match_node_instruction_770(bytes, ctx),
        3 => match_node_instruction_775(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_760(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 760: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_761(bytes, ctx),
        1 => match_node_instruction_762(bytes, ctx),
        2 => match_node_instruction_763(bytes, ctx),
        3 => match_node_instruction_764(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_761(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 761: Terminal matched constructor ID 301");
    301
}

fn match_node_instruction_762(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 762: Terminal matched constructor ID 436");
    436
}

fn match_node_instruction_763(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 763: Terminal matched constructor ID 278");
    278
}

fn match_node_instruction_764(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 764: Terminal matched constructor ID 274");
    274
}

fn match_node_instruction_765(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 765: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_766(bytes, ctx),
        1 => match_node_instruction_767(bytes, ctx),
        2 => match_node_instruction_768(bytes, ctx),
        3 => match_node_instruction_769(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_766(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 766: Terminal matched constructor ID 292");
    292
}

fn match_node_instruction_767(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 767: Terminal matched constructor ID 432");
    432
}

fn match_node_instruction_768(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 768: Terminal matched constructor ID 4");
    4
}

fn match_node_instruction_769(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 769: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_770(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 770: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_771(bytes, ctx),
        1 => match_node_instruction_772(bytes, ctx),
        2 => match_node_instruction_773(bytes, ctx),
        3 => match_node_instruction_774(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_771(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 771: Terminal matched constructor ID 318");
    318
}

fn match_node_instruction_772(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 772: Terminal matched constructor ID 245");
    245
}

fn match_node_instruction_773(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 773: Terminal matched constructor ID 309");
    309
}

fn match_node_instruction_774(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 774: Terminal matched constructor ID 328");
    328
}

fn match_node_instruction_775(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 775: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_776(bytes, ctx),
        1 => match_node_instruction_777(bytes, ctx),
        2 => match_node_instruction_778(bytes, ctx),
        3 => match_node_instruction_779(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_776(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 776: Terminal matched constructor ID 52");
    52
}

fn match_node_instruction_777(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 777: Terminal matched constructor ID 440");
    440
}

fn match_node_instruction_778(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 778: Terminal matched constructor ID 364");
    364
}

fn match_node_instruction_779(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 779: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_780(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 780: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_781(bytes, ctx),
        1 => match_node_instruction_808(bytes, ctx),
        2 => match_node_instruction_819(bytes, ctx),
        3 => match_node_instruction_828(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_781(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 15;
    eprintln!("Trace node 781: SlaInstructionBits start=12, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_782(bytes, ctx),
        1 => match_node_instruction_783(bytes, ctx),
        2 => match_node_instruction_786(bytes, ctx),
        3 => match_node_instruction_787(bytes, ctx),
        4 => match_node_instruction_792(bytes, ctx),
        5 => match_node_instruction_793(bytes, ctx),
        6 => match_node_instruction_794(bytes, ctx),
        7 => match_node_instruction_795(bytes, ctx),
        8 => match_node_instruction_796(bytes, ctx),
        9 => match_node_instruction_797(bytes, ctx),
        10 => match_node_instruction_798(bytes, ctx),
        11 => match_node_instruction_799(bytes, ctx),
        12 => match_node_instruction_800(bytes, ctx),
        13 => match_node_instruction_801(bytes, ctx),
        14 => match_node_instruction_802(bytes, ctx),
        15 => match_node_instruction_807(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_782(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 782: Terminal matched constructor ID 460");
    460
}

fn match_node_instruction_783(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 783: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_784(bytes, ctx),
        1 => match_node_instruction_785(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_784(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 784: Terminal matched constructor ID 411");
    411
}

fn match_node_instruction_785(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 785: Terminal matched constructor ID 395");
    395
}

fn match_node_instruction_786(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 786: Terminal matched constructor ID 46");
    46
}

fn match_node_instruction_787(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 787: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_788(bytes, ctx),
        1 => match_node_instruction_789(bytes, ctx),
        2 => match_node_instruction_790(bytes, ctx),
        3 => match_node_instruction_791(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_788(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 788: Terminal matched constructor ID 190");
    190
}

fn match_node_instruction_789(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 789: Terminal matched constructor ID 191");
    191
}

fn match_node_instruction_790(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 790: Terminal matched constructor ID 202");
    202
}

fn match_node_instruction_791(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 791: Terminal matched constructor ID 203");
    203
}

fn match_node_instruction_792(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 792: Terminal matched constructor ID 305");
    305
}

fn match_node_instruction_793(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 793: Terminal matched constructor ID 324");
    324
}

fn match_node_instruction_794(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 794: Terminal matched constructor ID 111");
    111
}

fn match_node_instruction_795(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 795: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_796(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 796: Terminal matched constructor ID 35");
    35
}

fn match_node_instruction_797(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 797: Terminal matched constructor ID 481");
    481
}

fn match_node_instruction_798(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 798: Terminal matched constructor ID 452");
    452
}

fn match_node_instruction_799(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 799: Terminal matched constructor ID 261");
    261
}

fn match_node_instruction_800(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 800: Terminal matched constructor ID 296");
    296
}

fn match_node_instruction_801(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 801: Terminal matched constructor ID 58");
    58
}

fn match_node_instruction_802(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 802: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_803(bytes, ctx),
        1 => match_node_instruction_804(bytes, ctx),
        2 => match_node_instruction_805(bytes, ctx),
        3 => match_node_instruction_806(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_803(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 803: Terminal matched constructor ID 337");
    337
}

fn match_node_instruction_804(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 804: Terminal matched constructor ID 171");
    171
}

fn match_node_instruction_805(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 805: Terminal matched constructor ID 65");
    65
}

fn match_node_instruction_806(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 806: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_807(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 807: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_808(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 7;
    eprintln!("Trace node 808: SlaInstructionBits start=10, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_809(bytes, ctx),
        1 => match_node_instruction_810(bytes, ctx),
        2 => match_node_instruction_813(bytes, ctx),
        3 => match_node_instruction_814(bytes, ctx),
        4 => match_node_instruction_815(bytes, ctx),
        5 => match_node_instruction_816(bytes, ctx),
        6 => match_node_instruction_817(bytes, ctx),
        7 => match_node_instruction_818(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_809(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 809: Terminal matched constructor ID 86");
    86
}

fn match_node_instruction_810(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 810: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_811(bytes, ctx),
        1 => match_node_instruction_812(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_811(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 811: Terminal matched constructor ID 469");
    469
}

fn match_node_instruction_812(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 812: Terminal matched constructor ID 470");
    470
}

fn match_node_instruction_813(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 813: Terminal matched constructor ID 67");
    67
}

fn match_node_instruction_814(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 814: Terminal matched constructor ID 74");
    74
}

fn match_node_instruction_815(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 815: Terminal matched constructor ID 89");
    89
}

fn match_node_instruction_816(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 816: Terminal matched constructor ID 91");
    91
}

fn match_node_instruction_817(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 817: Terminal matched constructor ID 63");
    63
}

fn match_node_instruction_818(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 818: Terminal matched constructor ID 84");
    84
}

fn match_node_instruction_819(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 819: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_820(bytes, ctx),
        1 => match_node_instruction_823(bytes, ctx),
        2 => match_node_instruction_824(bytes, ctx),
        3 => match_node_instruction_827(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_820(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 820: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_821(bytes, ctx),
        1 => match_node_instruction_822(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_821(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 821: Terminal matched constructor ID 401");
    401
}

fn match_node_instruction_822(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 822: Terminal matched constructor ID 397");
    397
}

fn match_node_instruction_823(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 823: Terminal matched constructor ID 105");
    105
}

fn match_node_instruction_824(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 824: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_825(bytes, ctx),
        1 => match_node_instruction_826(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_825(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 825: Terminal matched constructor ID 360");
    360
}

fn match_node_instruction_826(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 826: Terminal matched constructor ID 356");
    356
}

fn match_node_instruction_827(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 827: Terminal matched constructor ID 26");
    26
}

fn match_node_instruction_828(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 828: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_829(bytes, ctx),
        1 => match_node_instruction_832(bytes, ctx),
        2 => match_node_instruction_833(bytes, ctx),
        3 => match_node_instruction_834(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_829(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 829: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_830(bytes, ctx),
        1 => match_node_instruction_831(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_830(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 830: Terminal matched constructor ID 385");
    385
}

fn match_node_instruction_831(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 831: Terminal matched constructor ID 381");
    381
}

fn match_node_instruction_832(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 832: Terminal matched constructor ID 48");
    48
}

fn match_node_instruction_833(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 833: Terminal matched constructor ID 251");
    251
}

fn match_node_instruction_834(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 834: Terminal matched constructor ID 30");
    30
}

fn match_node_instruction_835(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 835: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_836(bytes, ctx),
        1 => match_node_instruction_893(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_836(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 15;
    eprintln!("Trace node 836: SlaInstructionBits start=12, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_837(bytes, ctx),
        1 => match_node_instruction_838(bytes, ctx),
        2 => match_node_instruction_839(bytes, ctx),
        3 => match_node_instruction_840(bytes, ctx),
        4 => match_node_instruction_841(bytes, ctx),
        5 => match_node_instruction_842(bytes, ctx),
        6 => match_node_instruction_843(bytes, ctx),
        7 => match_node_instruction_844(bytes, ctx),
        8 => match_node_instruction_845(bytes, ctx),
        9 => match_node_instruction_846(bytes, ctx),
        10 => match_node_instruction_847(bytes, ctx),
        11 => match_node_instruction_848(bytes, ctx),
        12 => match_node_instruction_849(bytes, ctx),
        13 => match_node_instruction_850(bytes, ctx),
        14 => match_node_instruction_851(bytes, ctx),
        15 => match_node_instruction_872(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_837(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 837: Terminal matched constructor ID 456");
    456
}

fn match_node_instruction_838(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 838: Terminal matched constructor ID 115");
    115
}

fn match_node_instruction_839(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 839: Terminal matched constructor ID 39");
    39
}

fn match_node_instruction_840(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 840: Terminal matched constructor ID 265");
    265
}

fn match_node_instruction_841(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 841: Terminal matched constructor ID 307");
    307
}

fn match_node_instruction_842(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 842: Terminal matched constructor ID 326");
    326
}

fn match_node_instruction_843(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 843: Terminal matched constructor ID 113");
    113
}

fn match_node_instruction_844(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 844: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_845(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 845: Terminal matched constructor ID 37");
    37
}

fn match_node_instruction_846(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 846: Terminal matched constructor ID 483");
    483
}

fn match_node_instruction_847(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 847: Terminal matched constructor ID 454");
    454
}

fn match_node_instruction_848(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 848: Terminal matched constructor ID 263");
    263
}

fn match_node_instruction_849(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 849: Terminal matched constructor ID 298");
    298
}

fn match_node_instruction_850(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 850: Terminal matched constructor ID 60");
    60
}

fn match_node_instruction_851(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 851: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_852(bytes, ctx),
        1 => match_node_instruction_857(bytes, ctx),
        2 => match_node_instruction_862(bytes, ctx),
        3 => match_node_instruction_867(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_852(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 852: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_853(bytes, ctx),
        1 => match_node_instruction_854(bytes, ctx),
        2 => match_node_instruction_855(bytes, ctx),
        3 => match_node_instruction_856(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_853(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 853: Terminal matched constructor ID 148");
    148
}

fn match_node_instruction_854(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 854: Terminal matched constructor ID 144");
    144
}

fn match_node_instruction_855(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 855: Terminal matched constructor ID 22");
    22
}

fn match_node_instruction_856(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 856: Terminal matched constructor ID 446");
    446
}

fn match_node_instruction_857(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 857: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_858(bytes, ctx),
        1 => match_node_instruction_859(bytes, ctx),
        2 => match_node_instruction_860(bytes, ctx),
        3 => match_node_instruction_861(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_858(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 858: Terminal matched constructor ID 479");
    479
}

fn match_node_instruction_859(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 859: Terminal matched constructor ID 152");
    152
}

fn match_node_instruction_860(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 860: Terminal matched constructor ID 101");
    101
}

fn match_node_instruction_861(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 861: Terminal matched constructor ID 463");
    463
}

fn match_node_instruction_862(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 862: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_863(bytes, ctx),
        1 => match_node_instruction_864(bytes, ctx),
        2 => match_node_instruction_865(bytes, ctx),
        3 => match_node_instruction_866(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_863(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 863: Terminal matched constructor ID 184");
    184
}

fn match_node_instruction_864(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 864: Terminal matched constructor ID 316");
    316
}

fn match_node_instruction_865(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 865: Terminal matched constructor ID 354");
    354
}

fn match_node_instruction_866(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 866: Terminal matched constructor ID 409");
    409
}

fn match_node_instruction_867(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 867: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_868(bytes, ctx),
        1 => match_node_instruction_869(bytes, ctx),
        2 => match_node_instruction_870(bytes, ctx),
        3 => match_node_instruction_871(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_868(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 868: Terminal matched constructor ID 139");
    139
}

fn match_node_instruction_869(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 869: Terminal matched constructor ID 18");
    18
}

fn match_node_instruction_870(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 870: Terminal matched constructor ID 350");
    350
}

fn match_node_instruction_871(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 871: Terminal matched constructor ID 393");
    393
}

fn match_node_instruction_872(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 872: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_873(bytes, ctx),
        1 => match_node_instruction_878(bytes, ctx),
        2 => match_node_instruction_883(bytes, ctx),
        3 => match_node_instruction_888(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_873(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 873: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_874(bytes, ctx),
        1 => match_node_instruction_875(bytes, ctx),
        2 => match_node_instruction_876(bytes, ctx),
        3 => match_node_instruction_877(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_874(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 874: Terminal matched constructor ID 303");
    303
}

fn match_node_instruction_875(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 875: Terminal matched constructor ID 438");
    438
}

fn match_node_instruction_876(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 876: Terminal matched constructor ID 279");
    279
}

fn match_node_instruction_877(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 877: Terminal matched constructor ID 276");
    276
}

fn match_node_instruction_878(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 878: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_879(bytes, ctx),
        1 => match_node_instruction_880(bytes, ctx),
        2 => match_node_instruction_881(bytes, ctx),
        3 => match_node_instruction_882(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_879(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 879: Terminal matched constructor ID 294");
    294
}

fn match_node_instruction_880(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 880: Terminal matched constructor ID 434");
    434
}

fn match_node_instruction_881(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 881: Terminal matched constructor ID 6");
    6
}

fn match_node_instruction_882(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 882: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_883(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 883: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_884(bytes, ctx),
        1 => match_node_instruction_885(bytes, ctx),
        2 => match_node_instruction_886(bytes, ctx),
        3 => match_node_instruction_887(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_884(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 884: Terminal matched constructor ID 320");
    320
}

fn match_node_instruction_885(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 885: Terminal matched constructor ID 247");
    247
}

fn match_node_instruction_886(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 886: Terminal matched constructor ID 311");
    311
}

fn match_node_instruction_887(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 887: Terminal matched constructor ID 330");
    330
}

fn match_node_instruction_888(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 888: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_889(bytes, ctx),
        1 => match_node_instruction_890(bytes, ctx),
        2 => match_node_instruction_891(bytes, ctx),
        3 => match_node_instruction_892(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_889(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 889: Terminal matched constructor ID 54");
    54
}

fn match_node_instruction_890(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 890: Terminal matched constructor ID 442");
    442
}

fn match_node_instruction_891(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 891: Terminal matched constructor ID 366");
    366
}

fn match_node_instruction_892(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 892: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_893(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 893: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_894(bytes, ctx),
        1 => match_node_instruction_917(bytes, ctx),
        2 => match_node_instruction_926(bytes, ctx),
        3 => match_node_instruction_935(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_894(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 15;
    eprintln!("Trace node 894: SlaInstructionBits start=12, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_895(bytes, ctx),
        1 => match_node_instruction_896(bytes, ctx),
        2 => match_node_instruction_897(bytes, ctx),
        3 => match_node_instruction_898(bytes, ctx),
        4 => match_node_instruction_899(bytes, ctx),
        5 => match_node_instruction_900(bytes, ctx),
        6 => match_node_instruction_901(bytes, ctx),
        7 => match_node_instruction_902(bytes, ctx),
        8 => match_node_instruction_903(bytes, ctx),
        9 => match_node_instruction_904(bytes, ctx),
        10 => match_node_instruction_905(bytes, ctx),
        11 => match_node_instruction_906(bytes, ctx),
        12 => match_node_instruction_907(bytes, ctx),
        13 => match_node_instruction_908(bytes, ctx),
        14 => match_node_instruction_909(bytes, ctx),
        15 => match_node_instruction_914(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_895(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 895: Terminal matched constructor ID 456");
    456
}

fn match_node_instruction_896(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 896: Terminal matched constructor ID 115");
    115
}

fn match_node_instruction_897(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 897: Terminal matched constructor ID 39");
    39
}

fn match_node_instruction_898(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 898: Terminal matched constructor ID 265");
    265
}

fn match_node_instruction_899(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 899: Terminal matched constructor ID 307");
    307
}

fn match_node_instruction_900(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 900: Terminal matched constructor ID 326");
    326
}

fn match_node_instruction_901(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 901: Terminal matched constructor ID 113");
    113
}

fn match_node_instruction_902(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 902: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_903(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 903: Terminal matched constructor ID 37");
    37
}

fn match_node_instruction_904(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 904: Terminal matched constructor ID 483");
    483
}

fn match_node_instruction_905(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 905: Terminal matched constructor ID 454");
    454
}

fn match_node_instruction_906(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 906: Terminal matched constructor ID 263");
    263
}

fn match_node_instruction_907(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 907: Terminal matched constructor ID 298");
    298
}

fn match_node_instruction_908(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 908: Terminal matched constructor ID 60");
    60
}

fn match_node_instruction_909(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 909: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_910(bytes, ctx),
        1 => match_node_instruction_911(bytes, ctx),
        2 => match_node_instruction_912(bytes, ctx),
        3 => match_node_instruction_913(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_910(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 910: Terminal matched constructor ID 338");
    338
}

fn match_node_instruction_911(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 911: Terminal matched constructor ID 173");
    173
}

fn match_node_instruction_912(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 912: Terminal matched constructor ID 66");
    66
}

fn match_node_instruction_913(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 913: Terminal matched constructor ID 300");
    300
}

fn match_node_instruction_914(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 914: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_915(bytes, ctx),
        1 => match_node_instruction_916(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_915(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 915: Terminal matched constructor ID 211");
    211
}

fn match_node_instruction_916(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 916: Terminal matched constructor ID 217");
    217
}

fn match_node_instruction_917(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 917: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_918(bytes, ctx),
        1 => match_node_instruction_923(bytes, ctx),
        2 => match_node_instruction_924(bytes, ctx),
        3 => match_node_instruction_925(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_918(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 918: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_919(bytes, ctx),
        1 => match_node_instruction_920(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_919(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 919: Terminal matched constructor ID 225");
    225
}

fn match_node_instruction_920(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 920: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_921(bytes, ctx),
        1 => match_node_instruction_922(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_921(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 921: Terminal matched constructor ID 473");
    473
}

fn match_node_instruction_922(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 922: Terminal matched constructor ID 474");
    474
}

fn match_node_instruction_923(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 923: Terminal matched constructor ID 429");
    429
}

fn match_node_instruction_924(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 924: Terminal matched constructor ID 69");
    69
}

fn match_node_instruction_925(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 925: Terminal matched constructor ID 377");
    377
}

fn match_node_instruction_926(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 926: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_927(bytes, ctx),
        1 => match_node_instruction_930(bytes, ctx),
        2 => match_node_instruction_931(bytes, ctx),
        3 => match_node_instruction_934(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_927(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 927: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_928(bytes, ctx),
        1 => match_node_instruction_929(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_928(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 928: Terminal matched constructor ID 403");
    403
}

fn match_node_instruction_929(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 929: Terminal matched constructor ID 399");
    399
}

fn match_node_instruction_930(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 930: Terminal matched constructor ID 107");
    107
}

fn match_node_instruction_931(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 931: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_932(bytes, ctx),
        1 => match_node_instruction_933(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_932(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 932: Terminal matched constructor ID 362");
    362
}

fn match_node_instruction_933(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 933: Terminal matched constructor ID 358");
    358
}

fn match_node_instruction_934(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 934: Terminal matched constructor ID 28");
    28
}

fn match_node_instruction_935(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 935: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_936(bytes, ctx),
        1 => match_node_instruction_939(bytes, ctx),
        2 => match_node_instruction_940(bytes, ctx),
        3 => match_node_instruction_941(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_936(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 936: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_937(bytes, ctx),
        1 => match_node_instruction_938(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_937(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 937: Terminal matched constructor ID 387");
    387
}

fn match_node_instruction_938(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 938: Terminal matched constructor ID 383");
    383
}

fn match_node_instruction_939(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 939: Terminal matched constructor ID 50");
    50
}

fn match_node_instruction_940(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 940: Terminal matched constructor ID 253");
    253
}

fn match_node_instruction_941(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 941: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_942(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 942: Terminal matched constructor ID 208");
    208
}

fn match_node_instruction_943(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 1;
    eprintln!("Trace node 943: SlaInstructionBits start=3, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_944(bytes, ctx),
        1 => match_node_instruction_1011(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_944(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 944: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_945(bytes, ctx),
        1 => match_node_instruction_946(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_945(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 945: Terminal matched constructor ID 34");
    34
}

fn match_node_instruction_946(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 6 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 63;
    eprintln!("Trace node 946: SlaInstructionBits start=2, size=6, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_947(bytes, ctx),
        1 => match_node_instruction_948(bytes, ctx),
        2 => match_node_instruction_949(bytes, ctx),
        3 => match_node_instruction_950(bytes, ctx),
        4 => match_node_instruction_951(bytes, ctx),
        5 => match_node_instruction_952(bytes, ctx),
        6 => match_node_instruction_953(bytes, ctx),
        7 => match_node_instruction_954(bytes, ctx),
        8 => match_node_instruction_955(bytes, ctx),
        9 => match_node_instruction_956(bytes, ctx),
        10 => match_node_instruction_957(bytes, ctx),
        11 => match_node_instruction_958(bytes, ctx),
        12 => match_node_instruction_959(bytes, ctx),
        13 => match_node_instruction_960(bytes, ctx),
        14 => match_node_instruction_961(bytes, ctx),
        15 => match_node_instruction_962(bytes, ctx),
        16 => match_node_instruction_963(bytes, ctx),
        17 => match_node_instruction_964(bytes, ctx),
        18 => match_node_instruction_965(bytes, ctx),
        19 => match_node_instruction_966(bytes, ctx),
        20 => match_node_instruction_967(bytes, ctx),
        21 => match_node_instruction_968(bytes, ctx),
        22 => match_node_instruction_969(bytes, ctx),
        23 => match_node_instruction_970(bytes, ctx),
        24 => match_node_instruction_971(bytes, ctx),
        25 => match_node_instruction_972(bytes, ctx),
        26 => match_node_instruction_973(bytes, ctx),
        27 => match_node_instruction_974(bytes, ctx),
        28 => match_node_instruction_975(bytes, ctx),
        29 => match_node_instruction_976(bytes, ctx),
        30 => match_node_instruction_977(bytes, ctx),
        31 => match_node_instruction_978(bytes, ctx),
        32 => match_node_instruction_979(bytes, ctx),
        33 => match_node_instruction_980(bytes, ctx),
        34 => match_node_instruction_981(bytes, ctx),
        35 => match_node_instruction_982(bytes, ctx),
        36 => match_node_instruction_983(bytes, ctx),
        37 => match_node_instruction_984(bytes, ctx),
        38 => match_node_instruction_985(bytes, ctx),
        39 => match_node_instruction_986(bytes, ctx),
        40 => match_node_instruction_987(bytes, ctx),
        41 => match_node_instruction_988(bytes, ctx),
        42 => match_node_instruction_989(bytes, ctx),
        43 => match_node_instruction_990(bytes, ctx),
        44 => match_node_instruction_991(bytes, ctx),
        45 => match_node_instruction_992(bytes, ctx),
        46 => match_node_instruction_993(bytes, ctx),
        47 => match_node_instruction_994(bytes, ctx),
        48 => match_node_instruction_995(bytes, ctx),
        49 => match_node_instruction_996(bytes, ctx),
        50 => match_node_instruction_997(bytes, ctx),
        51 => match_node_instruction_998(bytes, ctx),
        52 => match_node_instruction_999(bytes, ctx),
        53 => match_node_instruction_1000(bytes, ctx),
        54 => match_node_instruction_1001(bytes, ctx),
        55 => match_node_instruction_1002(bytes, ctx),
        56 => match_node_instruction_1003(bytes, ctx),
        57 => match_node_instruction_1004(bytes, ctx),
        58 => match_node_instruction_1005(bytes, ctx),
        59 => match_node_instruction_1006(bytes, ctx),
        60 => match_node_instruction_1007(bytes, ctx),
        61 => match_node_instruction_1008(bytes, ctx),
        62 => match_node_instruction_1009(bytes, ctx),
        63 => match_node_instruction_1010(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_947(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 947: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_948(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 948: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_949(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 949: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_950(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 950: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_951(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 951: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_952(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 952: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_953(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 953: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_954(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 954: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_955(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 955: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_956(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 956: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_957(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 957: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_958(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 958: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_959(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 959: Terminal matched constructor ID 210");
    210
}

fn match_node_instruction_960(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 960: Terminal matched constructor ID 216");
    216
}

fn match_node_instruction_961(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 961: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_962(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 962: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_963(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 963: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_964(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 964: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_965(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 965: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_966(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 966: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_967(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 967: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_968(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 968: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_969(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 969: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_970(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 970: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_971(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 971: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_972(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 972: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_973(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 973: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_974(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 974: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_975(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 975: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_976(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 976: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_977(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 977: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_978(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 978: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_979(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 979: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_980(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 980: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_981(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 981: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_982(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 982: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_983(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 983: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_984(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 984: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_985(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 985: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_986(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 986: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_987(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 987: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_988(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 988: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_989(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 989: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_990(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 990: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_991(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 991: Terminal matched constructor ID 169");
    169
}

fn match_node_instruction_992(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 992: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_993(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 993: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_994(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 994: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_995(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 995: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_996(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 996: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_997(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 997: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_998(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 998: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_999(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 999: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1000(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1000: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1001(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1001: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1002(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1002: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1003(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1003: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1004(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1004: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1005(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1005: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1006(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1006: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1007(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1007: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1008(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1008: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1009(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1009: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1010(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1010: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1011(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 3;
    eprintln!("Trace node 1011: SlaInstructionBits start=1, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1012(bytes, ctx),
        1 => match_node_instruction_1013(bytes, ctx),
        2 => match_node_instruction_1014(bytes, ctx),
        3 => match_node_instruction_1017(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1012(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1012: Terminal matched constructor ID 257");
    257
}

fn match_node_instruction_1013(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1013: Terminal matched constructor ID 258");
    258
}

fn match_node_instruction_1014(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 1014: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1015(bytes, ctx),
        1 => match_node_instruction_1016(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1015(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1015: Terminal matched constructor ID 214");
    214
}

fn match_node_instruction_1016(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1016: Terminal matched constructor ID 220");
    220
}

fn match_node_instruction_1017(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1017: Terminal matched constructor ID 170");
    170
}

fn match_node_instruction_1018(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 7 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 127;
    eprintln!("Trace node 1018: SlaInstructionBits start=1, size=7, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1019(bytes, ctx),
        1 => match_node_instruction_1020(bytes, ctx),
        2 => match_node_instruction_1021(bytes, ctx),
        3 => match_node_instruction_1022(bytes, ctx),
        4 => match_node_instruction_1023(bytes, ctx),
        5 => match_node_instruction_1024(bytes, ctx),
        6 => match_node_instruction_1025(bytes, ctx),
        7 => match_node_instruction_1026(bytes, ctx),
        8 => match_node_instruction_1027(bytes, ctx),
        9 => match_node_instruction_1028(bytes, ctx),
        10 => match_node_instruction_1029(bytes, ctx),
        11 => match_node_instruction_1030(bytes, ctx),
        12 => match_node_instruction_1031(bytes, ctx),
        13 => match_node_instruction_1032(bytes, ctx),
        14 => match_node_instruction_1033(bytes, ctx),
        15 => match_node_instruction_1034(bytes, ctx),
        16 => match_node_instruction_1035(bytes, ctx),
        17 => match_node_instruction_1036(bytes, ctx),
        18 => match_node_instruction_1037(bytes, ctx),
        19 => match_node_instruction_1038(bytes, ctx),
        20 => match_node_instruction_1039(bytes, ctx),
        21 => match_node_instruction_1040(bytes, ctx),
        22 => match_node_instruction_1041(bytes, ctx),
        23 => match_node_instruction_1042(bytes, ctx),
        24 => match_node_instruction_1043(bytes, ctx),
        25 => match_node_instruction_1044(bytes, ctx),
        26 => match_node_instruction_1045(bytes, ctx),
        27 => match_node_instruction_1046(bytes, ctx),
        28 => match_node_instruction_1047(bytes, ctx),
        29 => match_node_instruction_1048(bytes, ctx),
        30 => match_node_instruction_1049(bytes, ctx),
        31 => match_node_instruction_1050(bytes, ctx),
        32 => match_node_instruction_1051(bytes, ctx),
        33 => match_node_instruction_1052(bytes, ctx),
        34 => match_node_instruction_1053(bytes, ctx),
        35 => match_node_instruction_1054(bytes, ctx),
        36 => match_node_instruction_1055(bytes, ctx),
        37 => match_node_instruction_1056(bytes, ctx),
        38 => match_node_instruction_1057(bytes, ctx),
        39 => match_node_instruction_1058(bytes, ctx),
        40 => match_node_instruction_1059(bytes, ctx),
        41 => match_node_instruction_1060(bytes, ctx),
        42 => match_node_instruction_1061(bytes, ctx),
        43 => match_node_instruction_1062(bytes, ctx),
        44 => match_node_instruction_1063(bytes, ctx),
        45 => match_node_instruction_1064(bytes, ctx),
        46 => match_node_instruction_1065(bytes, ctx),
        47 => match_node_instruction_1066(bytes, ctx),
        48 => match_node_instruction_1067(bytes, ctx),
        49 => match_node_instruction_1068(bytes, ctx),
        50 => match_node_instruction_1069(bytes, ctx),
        51 => match_node_instruction_1070(bytes, ctx),
        52 => match_node_instruction_1071(bytes, ctx),
        53 => match_node_instruction_1072(bytes, ctx),
        54 => match_node_instruction_1073(bytes, ctx),
        55 => match_node_instruction_1074(bytes, ctx),
        56 => match_node_instruction_1075(bytes, ctx),
        57 => match_node_instruction_1076(bytes, ctx),
        58 => match_node_instruction_1077(bytes, ctx),
        59 => match_node_instruction_1078(bytes, ctx),
        60 => match_node_instruction_1079(bytes, ctx),
        61 => match_node_instruction_1080(bytes, ctx),
        62 => match_node_instruction_1081(bytes, ctx),
        63 => match_node_instruction_1082(bytes, ctx),
        64 => match_node_instruction_1083(bytes, ctx),
        65 => match_node_instruction_1084(bytes, ctx),
        66 => match_node_instruction_1085(bytes, ctx),
        67 => match_node_instruction_1086(bytes, ctx),
        68 => match_node_instruction_1087(bytes, ctx),
        69 => match_node_instruction_1088(bytes, ctx),
        70 => match_node_instruction_1089(bytes, ctx),
        71 => match_node_instruction_1090(bytes, ctx),
        72 => match_node_instruction_1091(bytes, ctx),
        73 => match_node_instruction_1092(bytes, ctx),
        74 => match_node_instruction_1093(bytes, ctx),
        75 => match_node_instruction_1094(bytes, ctx),
        76 => match_node_instruction_1095(bytes, ctx),
        77 => match_node_instruction_1096(bytes, ctx),
        78 => match_node_instruction_1097(bytes, ctx),
        79 => match_node_instruction_1098(bytes, ctx),
        80 => match_node_instruction_1099(bytes, ctx),
        81 => match_node_instruction_1100(bytes, ctx),
        82 => match_node_instruction_1101(bytes, ctx),
        83 => match_node_instruction_1102(bytes, ctx),
        84 => match_node_instruction_1103(bytes, ctx),
        85 => match_node_instruction_1104(bytes, ctx),
        86 => match_node_instruction_1105(bytes, ctx),
        87 => match_node_instruction_1106(bytes, ctx),
        88 => match_node_instruction_1107(bytes, ctx),
        89 => match_node_instruction_1108(bytes, ctx),
        90 => match_node_instruction_1109(bytes, ctx),
        91 => match_node_instruction_1110(bytes, ctx),
        92 => match_node_instruction_1111(bytes, ctx),
        93 => match_node_instruction_1112(bytes, ctx),
        94 => match_node_instruction_1113(bytes, ctx),
        95 => match_node_instruction_1114(bytes, ctx),
        96 => match_node_instruction_1115(bytes, ctx),
        97 => match_node_instruction_1116(bytes, ctx),
        98 => match_node_instruction_1117(bytes, ctx),
        99 => match_node_instruction_1118(bytes, ctx),
        100 => match_node_instruction_1119(bytes, ctx),
        101 => match_node_instruction_1120(bytes, ctx),
        102 => match_node_instruction_1121(bytes, ctx),
        103 => match_node_instruction_1122(bytes, ctx),
        104 => match_node_instruction_1123(bytes, ctx),
        105 => match_node_instruction_1124(bytes, ctx),
        106 => match_node_instruction_1125(bytes, ctx),
        107 => match_node_instruction_1126(bytes, ctx),
        108 => match_node_instruction_1127(bytes, ctx),
        109 => match_node_instruction_1128(bytes, ctx),
        110 => match_node_instruction_1129(bytes, ctx),
        111 => match_node_instruction_1130(bytes, ctx),
        112 => match_node_instruction_1131(bytes, ctx),
        113 => match_node_instruction_1132(bytes, ctx),
        114 => match_node_instruction_1133(bytes, ctx),
        115 => match_node_instruction_1134(bytes, ctx),
        116 => match_node_instruction_1135(bytes, ctx),
        117 => match_node_instruction_1136(bytes, ctx),
        118 => match_node_instruction_1137(bytes, ctx),
        119 => match_node_instruction_1138(bytes, ctx),
        120 => match_node_instruction_1139(bytes, ctx),
        121 => match_node_instruction_1140(bytes, ctx),
        122 => match_node_instruction_1141(bytes, ctx),
        123 => match_node_instruction_1142(bytes, ctx),
        124 => match_node_instruction_1143(bytes, ctx),
        125 => match_node_instruction_1144(bytes, ctx),
        126 => match_node_instruction_1145(bytes, ctx),
        127 => match_node_instruction_1146(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1019(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1019: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1020(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1020: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1021(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1021: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1022(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1022: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1023(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1023: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1024(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1024: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1025(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1025: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1026(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1026: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1027(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1027: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1028(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1028: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1029(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1029: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1030(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1030: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1031(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1031: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1032(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1032: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1033(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1033: Terminal matched constructor ID 334");
    334
}

fn match_node_instruction_1034(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1034: Terminal matched constructor ID 344");
    344
}

fn match_node_instruction_1035(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1035: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1036(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1036: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1037(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1037: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1038(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1038: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1039(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1039: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1040(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1040: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1041(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1041: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1042(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1042: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1043(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1043: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1044(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1044: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1045(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1045: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1046(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1046: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1047(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1047: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1048(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1048: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1049(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1049: Terminal matched constructor ID 345");
    345
}

fn match_node_instruction_1050(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1050: Terminal matched constructor ID 180");
    180
}

fn match_node_instruction_1051(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1051: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1052(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1052: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1053(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1053: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1054(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1054: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1055(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1055: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1056(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1056: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1057(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1057: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1058(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1058: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1059(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1059: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1060(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1060: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1061(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1061: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1062(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1062: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1063(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1063: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1064(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1064: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1065(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1065: Terminal matched constructor ID 335");
    335
}

fn match_node_instruction_1066(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1066: Terminal matched constructor ID 336");
    336
}

fn match_node_instruction_1067(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1067: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1068(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1068: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1069(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1069: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1070(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1070: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1071(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1071: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1072(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1072: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1073(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1073: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1074(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1074: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1075(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1075: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1076(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1076: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1077(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1077: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1078(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1078: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1079(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1079: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1080(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1080: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1081(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1081: Terminal matched constructor ID 204");
    204
}

fn match_node_instruction_1082(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1082: Terminal matched constructor ID 205");
    205
}

fn match_node_instruction_1083(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1083: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1084(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1084: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1085(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1085: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1086(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1086: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1087(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1087: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1088(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1088: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1089(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1089: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1090(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1090: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1091(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1091: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1092(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1092: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1093(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1093: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1094(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1094: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1095(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1095: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1096(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1096: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1097(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1097: Terminal matched constructor ID 209");
    209
}

fn match_node_instruction_1098(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1098: Terminal matched constructor ID 215");
    215
}

fn match_node_instruction_1099(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1099: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1100(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1100: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1101(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1101: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1102(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1102: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1103(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1103: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1104(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1104: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1105(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1105: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1106(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1106: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1107(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1107: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1108(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1108: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1109(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1109: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1110(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1110: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1111(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1111: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1112(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1112: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1113(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1113: Terminal matched constructor ID 313");
    313
}

fn match_node_instruction_1114(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1114: Terminal matched constructor ID 368");
    368
}

fn match_node_instruction_1115(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1115: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1116(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1116: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1117(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1117: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1118(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1118: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1119(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1119: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1120(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1120: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1121(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1121: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1122(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1122: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1123(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1123: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1124(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1124: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1125(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1125: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1126(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1126: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1127(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1127: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1128(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1128: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1129(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1129: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1130(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1130: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1131(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1131: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1132(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1132: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1133(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1133: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1134(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1134: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1135(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1135: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1136(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1136: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1137(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1137: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1138(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1138: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1139(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1139: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1140(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1140: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1141(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1141: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1142(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1142: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1143(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1143: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1144(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1144: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1145(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1145: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1146(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1146: Terminal matched NOTHING");
    -1
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

fn match_node_reloffset_dst5Ax_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_reloffset_dst5L_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_reloffset_dst5L_1(bytes, ctx),
        1 => match_node_reloffset_dst5L_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_reloffset_dst5L_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_reloffset_dst5L_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_reloffset_dst5W_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_reloffset_dst5W_1(bytes, ctx),
        1 => match_node_reloffset_dst5W_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_reloffset_dst5W_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_reloffset_dst5W_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_skipBytesBeforeDst5_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 7) & 1;
    eprintln!("Trace node 0: SlaContextBits start=7, size=1, probe={}", probe);
    match probe {
        0 => match_node_skipBytesBeforeDst5_1(bytes, ctx),
        1 => match_node_skipBytesBeforeDst5_6(bytes, ctx),
        _ => -1,
    }
}

fn match_node_skipBytesBeforeDst5_1(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 1: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_skipBytesBeforeDst5_2(bytes, ctx),
        1 => match_node_skipBytesBeforeDst5_3(bytes, ctx),
        2 => match_node_skipBytesBeforeDst5_4(bytes, ctx),
        3 => match_node_skipBytesBeforeDst5_5(bytes, ctx),
        _ => -1,
    }
}

fn match_node_skipBytesBeforeDst5_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_skipBytesBeforeDst5_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 1");
    1
}

fn match_node_skipBytesBeforeDst5_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 2");
    2
}

fn match_node_skipBytesBeforeDst5_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 4");
    4
}

fn match_node_skipBytesBeforeDst5_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_src5B_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 7;
    eprintln!("Trace node 0: SlaInstructionBits start=1, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_src5B_1(bytes, ctx),
        1 => match_node_src5B_4(bytes, ctx),
        2 => match_node_src5B_5(bytes, ctx),
        3 => match_node_src5B_6(bytes, ctx),
        4 => match_node_src5B_15(bytes, ctx),
        5 => match_node_src5B_16(bytes, ctx),
        6 => match_node_src5B_17(bytes, ctx),
        7 => match_node_src5B_18(bytes, ctx),
        _ => -1,
    }
}

fn match_node_src5B_1(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_src5B_2(bytes, ctx),
        1 => match_node_src5B_3(bytes, ctx),
        _ => -1,
    }
}

fn match_node_src5B_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 4");
    4
}

fn match_node_src5B_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_src5B_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 6");
    6
}

fn match_node_src5B_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 8");
    8
}

fn match_node_src5B_6(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 6: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_src5B_7(bytes, ctx),
        1 => match_node_src5B_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_src5B_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 10");
    10
}

fn match_node_src5B_8(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 8: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_src5B_9(bytes, ctx),
        1 => match_node_src5B_12(bytes, ctx),
        _ => -1,
    }
}

fn match_node_src5B_9(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 6) & 1;
    eprintln!("Trace node 9: SlaContextBits start=6, size=1, probe={}", probe);
    match probe {
        0 => match_node_src5B_10(bytes, ctx),
        1 => match_node_src5B_11(bytes, ctx),
        _ => -1,
    }
}

fn match_node_src5B_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 14");
    14
}

fn match_node_src5B_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 16");
    16
}

fn match_node_src5B_12(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 6) & 1;
    eprintln!("Trace node 12: SlaContextBits start=6, size=1, probe={}", probe);
    match probe {
        0 => match_node_src5B_13(bytes, ctx),
        1 => match_node_src5B_14(bytes, ctx),
        _ => -1,
    }
}

fn match_node_src5B_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 11");
    11
}

fn match_node_src5B_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 13");
    13
}

fn match_node_src5B_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 0");
    0
}

fn match_node_src5B_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched NOTHING");
    -1
}

fn match_node_src5B_17(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 17: Terminal matched NOTHING");
    -1
}

fn match_node_src5B_18(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 18: Terminal matched NOTHING");
    -1
}

fn match_node_src5L_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 7;
    eprintln!("Trace node 0: SlaInstructionBits start=1, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_src5L_1(bytes, ctx),
        1 => match_node_src5L_4(bytes, ctx),
        2 => match_node_src5L_5(bytes, ctx),
        3 => match_node_src5L_6(bytes, ctx),
        4 => match_node_src5L_15(bytes, ctx),
        5 => match_node_src5L_16(bytes, ctx),
        6 => match_node_src5L_17(bytes, ctx),
        7 => match_node_src5L_18(bytes, ctx),
        _ => -1,
    }
}

fn match_node_src5L_1(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_src5L_2(bytes, ctx),
        1 => match_node_src5L_3(bytes, ctx),
        _ => -1,
    }
}

fn match_node_src5L_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 4");
    4
}

fn match_node_src5L_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_src5L_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 6");
    6
}

fn match_node_src5L_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 8");
    8
}

fn match_node_src5L_6(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 6: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_src5L_7(bytes, ctx),
        1 => match_node_src5L_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_src5L_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 10");
    10
}

fn match_node_src5L_8(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 8: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_src5L_9(bytes, ctx),
        1 => match_node_src5L_12(bytes, ctx),
        _ => -1,
    }
}

fn match_node_src5L_9(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 6) & 1;
    eprintln!("Trace node 9: SlaContextBits start=6, size=1, probe={}", probe);
    match probe {
        0 => match_node_src5L_10(bytes, ctx),
        1 => match_node_src5L_11(bytes, ctx),
        _ => -1,
    }
}

fn match_node_src5L_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 14");
    14
}

fn match_node_src5L_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 16");
    16
}

fn match_node_src5L_12(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 6) & 1;
    eprintln!("Trace node 12: SlaContextBits start=6, size=1, probe={}", probe);
    match probe {
        0 => match_node_src5L_13(bytes, ctx),
        1 => match_node_src5L_14(bytes, ctx),
        _ => -1,
    }
}

fn match_node_src5L_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 11");
    11
}

fn match_node_src5L_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 13");
    13
}

fn match_node_src5L_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 0");
    0
}

fn match_node_src5L_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched NOTHING");
    -1
}

fn match_node_src5L_17(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 17: Terminal matched NOTHING");
    -1
}

fn match_node_src5L_18(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 18: Terminal matched NOTHING");
    -1
}

fn match_node_src5W_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 7;
    eprintln!("Trace node 0: SlaInstructionBits start=1, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_src5W_1(bytes, ctx),
        1 => match_node_src5W_4(bytes, ctx),
        2 => match_node_src5W_5(bytes, ctx),
        3 => match_node_src5W_6(bytes, ctx),
        4 => match_node_src5W_15(bytes, ctx),
        5 => match_node_src5W_16(bytes, ctx),
        6 => match_node_src5W_17(bytes, ctx),
        7 => match_node_src5W_18(bytes, ctx),
        _ => -1,
    }
}

fn match_node_src5W_1(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_src5W_2(bytes, ctx),
        1 => match_node_src5W_3(bytes, ctx),
        _ => -1,
    }
}

fn match_node_src5W_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 4");
    4
}

fn match_node_src5W_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_src5W_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 6");
    6
}

fn match_node_src5W_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 8");
    8
}

fn match_node_src5W_6(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 6: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_src5W_7(bytes, ctx),
        1 => match_node_src5W_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_src5W_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 10");
    10
}

fn match_node_src5W_8(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 8: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_src5W_9(bytes, ctx),
        1 => match_node_src5W_12(bytes, ctx),
        _ => -1,
    }
}

fn match_node_src5W_9(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 6) & 1;
    eprintln!("Trace node 9: SlaContextBits start=6, size=1, probe={}", probe);
    match probe {
        0 => match_node_src5W_10(bytes, ctx),
        1 => match_node_src5W_11(bytes, ctx),
        _ => -1,
    }
}

fn match_node_src5W_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 14");
    14
}

fn match_node_src5W_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 16");
    16
}

fn match_node_src5W_12(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 6) & 1;
    eprintln!("Trace node 12: SlaContextBits start=6, size=1, probe={}", probe);
    match probe {
        0 => match_node_src5W_13(bytes, ctx),
        1 => match_node_src5W_14(bytes, ctx),
        _ => -1,
    }
}

fn match_node_src5W_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 11");
    11
}

fn match_node_src5W_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 13");
    13
}

fn match_node_src5W_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 0");
    0
}

fn match_node_src5W_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched NOTHING");
    -1
}

fn match_node_src5W_17(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 17: Terminal matched NOTHING");
    -1
}

fn match_node_src5W_18(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 18: Terminal matched NOTHING");
    -1
}

fn match_node_src5dsp16_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 1");
    1
}

fn match_node_src5dsp24_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 1");
    1
}

fn match_node_src5dsp8_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 1");
    1
}

fn match_node_srcImm16_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_srcImm16a_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_srcImm1p_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_srcImm24_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_srcImm3_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_srcImm32_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_srcImm3p_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_srcImm8_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_srcImm8a_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_srcIndexOffset_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 1) & 1;
    eprintln!("Trace node 0: SlaContextBits start=1, size=1, probe={}", probe);
    match probe {
        0 => match_node_srcIndexOffset_1(bytes, ctx),
        1 => match_node_srcIndexOffset_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_srcIndexOffset_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_srcIndexOffset_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_srcIntNum_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_srcSimm16_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_srcSimm16a_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_srcSimm32_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_srcSimm4_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_srcSimm4Shift_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_srcSimm4Shift_1(bytes, ctx),
        1 => match_node_srcSimm4Shift_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_srcSimm4Shift_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_srcSimm4Shift_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_srcSimm8_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_srcSimm8a_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_srcZero16_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_srcZero8_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_with_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

