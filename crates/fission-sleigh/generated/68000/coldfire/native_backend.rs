// Auto-generated Fission Native Backend
#[no_mangle]
pub extern "C" fn fission_decode_match(table_ptr: *const i8, bytes: *const u8, bytes_len: usize, ctx_ptr: *const u64) -> i32 {
    let table_name = unsafe { std::ffi::CStr::from_ptr(table_ptr).to_str().unwrap() };
    let bytes = unsafe { std::slice::from_raw_parts(bytes, bytes_len) };
    let ctx = unsafe { *ctx_ptr };
    match table_name {
        "Tx" => match_node_Tx_0(bytes, ctx),
        "Txb" => match_node_Txb_0(bytes, ctx),
        "Txw" => match_node_Txw_0(bytes, ctx),
        "Ty" => match_node_Ty_0(bytes, ctx),
        "Tyb" => match_node_Tyb_0(bytes, ctx),
        "Tyw" => match_node_Tyw_0(bytes, ctx),
        "accreg" => match_node_accreg_0(bytes, ctx),
        "addr16" => match_node_addr16_0(bytes, ctx),
        "addr32" => match_node_addr32_0(bytes, ctx),
        "addr8" => match_node_addr8_0(bytes, ctx),
        "addrReg" => match_node_addrReg_0(bytes, ctx),
        "addrRegD16" => match_node_addrRegD16_0(bytes, ctx),
        "addrd16" => match_node_addrd16_0(bytes, ctx),
        "addrd32" => match_node_addrd32_0(bytes, ctx),
        "addrextw" => match_node_addrextw_0(bytes, ctx),
        "addrpc16" => match_node_addrpc16_0(bytes, ctx),
        "bfOffWd" => match_node_bfOffWd_0(bytes, ctx),
        "breg" => match_node_breg_0(bytes, ctx),
        "cachetype" => match_node_cachetype_0(bytes, ctx),
        "cc" => match_node_cc_0(bytes, ctx),
        "cntreg" => match_node_cntreg_0(bytes, ctx),
        "const16" => match_node_const16_0(bytes, ctx),
        "const32" => match_node_const32_0(bytes, ctx),
        "const8" => match_node_const8_0(bytes, ctx),
        "ctlreg" => match_node_ctlreg_0(bytes, ctx),
        "d32l" => match_node_d32l_0(bytes, ctx),
        "e2b" => match_node_e2b_0(bytes, ctx),
        "e2d" => match_node_e2d_0(bytes, ctx),
        "e2l" => match_node_e2l_0(bytes, ctx),
        "e2w" => match_node_e2w_0(bytes, ctx),
        "e2x" => match_node_e2x_0(bytes, ctx),
        "ea_index" => match_node_ea_index_0(bytes, ctx),
        "eab" => match_node_eab_0(bytes, ctx),
        "eal" => match_node_eal_0(bytes, ctx),
        "eaptr" => match_node_eaptr_0(bytes, ctx),
        "eaw" => match_node_eaw_0(bytes, ctx),
        "epw" => match_node_epw_0(bytes, ctx),
        "extw" => match_node_extw_0(bytes, ctx),
        "f_mem" => match_node_f_mem_0(bytes, ctx),
        "f_off" => match_node_f_off_0(bytes, ctx),
        "f_wd" => match_node_f_wd_0(bytes, ctx),
        "fabsrnd" => match_node_fabsrnd_0(bytes, ctx),
        "faddrnd" => match_node_faddrnd_0(bytes, ctx),
        "fcc" => match_node_fcc_0(bytes, ctx),
        "fdivrnd" => match_node_fdivrnd_0(bytes, ctx),
        "fl_breg" => match_node_fl_breg_0(bytes, ctx),
        "fmovernd" => match_node_fmovernd_0(bytes, ctx),
        "fmulrnd" => match_node_fmulrnd_0(bytes, ctx),
        "fnegrnd" => match_node_fnegrnd_0(bytes, ctx),
        "fp2mC0" => match_node_fp2mC0_0(bytes, ctx),
        "fp2mC1" => match_node_fp2mC1_0(bytes, ctx),
        "fp2mC2" => match_node_fp2mC2_0(bytes, ctx),
        "fp2mF0" => match_node_fp2mF0_0(bytes, ctx),
        "fp2mF1" => match_node_fp2mF1_0(bytes, ctx),
        "fp2mF2" => match_node_fp2mF2_0(bytes, ctx),
        "fp2mF3" => match_node_fp2mF3_0(bytes, ctx),
        "fp2mF4" => match_node_fp2mF4_0(bytes, ctx),
        "fp2mF5" => match_node_fp2mF5_0(bytes, ctx),
        "fp2mF6" => match_node_fp2mF6_0(bytes, ctx),
        "fp2mF7" => match_node_fp2mF7_0(bytes, ctx),
        "fp2mR0" => match_node_fp2mR0_0(bytes, ctx),
        "fp2mR1" => match_node_fp2mR1_0(bytes, ctx),
        "fp2mR2" => match_node_fp2mR2_0(bytes, ctx),
        "fp2mR3" => match_node_fp2mR3_0(bytes, ctx),
        "fp2mR4" => match_node_fp2mR4_0(bytes, ctx),
        "fp2mR5" => match_node_fp2mR5_0(bytes, ctx),
        "fp2mR6" => match_node_fp2mR6_0(bytes, ctx),
        "fp2mR7" => match_node_fp2mR7_0(bytes, ctx),
        "fprec" => match_node_fprec_0(bytes, ctx),
        "fsqrtrnd" => match_node_fsqrtrnd_0(bytes, ctx),
        "fsubrnd" => match_node_fsubrnd_0(bytes, ctx),
        "instruction" => match_node_instruction_0(bytes, ctx),
        "kfact" => match_node_kfact_0(bytes, ctx),
        "m2fpC0" => match_node_m2fpC0_0(bytes, ctx),
        "m2fpC1" => match_node_m2fpC1_0(bytes, ctx),
        "m2fpC2" => match_node_m2fpC2_0(bytes, ctx),
        "m2fpF0" => match_node_m2fpF0_0(bytes, ctx),
        "m2fpF1" => match_node_m2fpF1_0(bytes, ctx),
        "m2fpF2" => match_node_m2fpF2_0(bytes, ctx),
        "m2fpF3" => match_node_m2fpF3_0(bytes, ctx),
        "m2fpF4" => match_node_m2fpF4_0(bytes, ctx),
        "m2fpF5" => match_node_m2fpF5_0(bytes, ctx),
        "m2fpF6" => match_node_m2fpF6_0(bytes, ctx),
        "m2fpF7" => match_node_m2fpF7_0(bytes, ctx),
        "m2fpR0" => match_node_m2fpR0_0(bytes, ctx),
        "m2fpR1" => match_node_m2fpR1_0(bytes, ctx),
        "m2fpR2" => match_node_m2fpR2_0(bytes, ctx),
        "m2fpR3" => match_node_m2fpR3_0(bytes, ctx),
        "m2fpR4" => match_node_m2fpR4_0(bytes, ctx),
        "m2fpR5" => match_node_m2fpR5_0(bytes, ctx),
        "m2fpR6" => match_node_m2fpR6_0(bytes, ctx),
        "m2fpR7" => match_node_m2fpR7_0(bytes, ctx),
        "m2rfl0" => match_node_m2rfl0_0(bytes, ctx),
        "m2rfl1" => match_node_m2rfl1_0(bytes, ctx),
        "m2rfl2" => match_node_m2rfl2_0(bytes, ctx),
        "m2rfl3" => match_node_m2rfl3_0(bytes, ctx),
        "m2rfl4" => match_node_m2rfl4_0(bytes, ctx),
        "m2rfl5" => match_node_m2rfl5_0(bytes, ctx),
        "m2rfl6" => match_node_m2rfl6_0(bytes, ctx),
        "m2rfl7" => match_node_m2rfl7_0(bytes, ctx),
        "m2rfl8" => match_node_m2rfl8_0(bytes, ctx),
        "m2rfl9" => match_node_m2rfl9_0(bytes, ctx),
        "m2rfla" => match_node_m2rfla_0(bytes, ctx),
        "m2rflb" => match_node_m2rflb_0(bytes, ctx),
        "m2rflc" => match_node_m2rflc_0(bytes, ctx),
        "m2rfld" => match_node_m2rfld_0(bytes, ctx),
        "m2rfle" => match_node_m2rfle_0(bytes, ctx),
        "m2rflf" => match_node_m2rflf_0(bytes, ctx),
        "m2rfw0" => match_node_m2rfw0_0(bytes, ctx),
        "m2rfw1" => match_node_m2rfw1_0(bytes, ctx),
        "m2rfw2" => match_node_m2rfw2_0(bytes, ctx),
        "m2rfw3" => match_node_m2rfw3_0(bytes, ctx),
        "m2rfw4" => match_node_m2rfw4_0(bytes, ctx),
        "m2rfw5" => match_node_m2rfw5_0(bytes, ctx),
        "m2rfw6" => match_node_m2rfw6_0(bytes, ctx),
        "m2rfw7" => match_node_m2rfw7_0(bytes, ctx),
        "m2rfw8" => match_node_m2rfw8_0(bytes, ctx),
        "m2rfw9" => match_node_m2rfw9_0(bytes, ctx),
        "m2rfwa" => match_node_m2rfwa_0(bytes, ctx),
        "m2rfwb" => match_node_m2rfwb_0(bytes, ctx),
        "m2rfwc" => match_node_m2rfwc_0(bytes, ctx),
        "m2rfwd" => match_node_m2rfwd_0(bytes, ctx),
        "m2rfwe" => match_node_m2rfwe_0(bytes, ctx),
        "m2rfwf" => match_node_m2rfwf_0(bytes, ctx),
        "m_eal" => match_node_m_eal_0(bytes, ctx),
        "macregx" => match_node_macregx_0(bytes, ctx),
        "macregxl" => match_node_macregxl_0(bytes, ctx),
        "macregy" => match_node_macregy_0(bytes, ctx),
        "macregy_e" => match_node_macregy_e_0(bytes, ctx),
        "macregyl" => match_node_macregyl_0(bytes, ctx),
        "macrw" => match_node_macrw_0(bytes, ctx),
        "moveaccreg" => match_node_moveaccreg_0(bytes, ctx),
        "moveaccreg2" => match_node_moveaccreg2_0(bytes, ctx),
        "movemOp" => match_node_movemOp_0(bytes, ctx),
        "movemWrt" => match_node_movemWrt_0(bytes, ctx),
        "mulsize" => match_node_mulsize_0(bytes, ctx),
        "r2mbl0" => match_node_r2mbl0_0(bytes, ctx),
        "r2mbl1" => match_node_r2mbl1_0(bytes, ctx),
        "r2mbl2" => match_node_r2mbl2_0(bytes, ctx),
        "r2mbl3" => match_node_r2mbl3_0(bytes, ctx),
        "r2mbl4" => match_node_r2mbl4_0(bytes, ctx),
        "r2mbl5" => match_node_r2mbl5_0(bytes, ctx),
        "r2mbl6" => match_node_r2mbl6_0(bytes, ctx),
        "r2mbl7" => match_node_r2mbl7_0(bytes, ctx),
        "r2mbl8" => match_node_r2mbl8_0(bytes, ctx),
        "r2mbl9" => match_node_r2mbl9_0(bytes, ctx),
        "r2mbla" => match_node_r2mbla_0(bytes, ctx),
        "r2mblb" => match_node_r2mblb_0(bytes, ctx),
        "r2mblc" => match_node_r2mblc_0(bytes, ctx),
        "r2mbld" => match_node_r2mbld_0(bytes, ctx),
        "r2mble" => match_node_r2mble_0(bytes, ctx),
        "r2mblf" => match_node_r2mblf_0(bytes, ctx),
        "r2mbw0" => match_node_r2mbw0_0(bytes, ctx),
        "r2mbw1" => match_node_r2mbw1_0(bytes, ctx),
        "r2mbw2" => match_node_r2mbw2_0(bytes, ctx),
        "r2mbw3" => match_node_r2mbw3_0(bytes, ctx),
        "r2mbw4" => match_node_r2mbw4_0(bytes, ctx),
        "r2mbw5" => match_node_r2mbw5_0(bytes, ctx),
        "r2mbw6" => match_node_r2mbw6_0(bytes, ctx),
        "r2mbw7" => match_node_r2mbw7_0(bytes, ctx),
        "r2mbw8" => match_node_r2mbw8_0(bytes, ctx),
        "r2mbw9" => match_node_r2mbw9_0(bytes, ctx),
        "r2mbwa" => match_node_r2mbwa_0(bytes, ctx),
        "r2mbwb" => match_node_r2mbwb_0(bytes, ctx),
        "r2mbwc" => match_node_r2mbwc_0(bytes, ctx),
        "r2mbwd" => match_node_r2mbwd_0(bytes, ctx),
        "r2mbwe" => match_node_r2mbwe_0(bytes, ctx),
        "r2mbwf" => match_node_r2mbwf_0(bytes, ctx),
        "r2mfl0" => match_node_r2mfl0_0(bytes, ctx),
        "r2mfl1" => match_node_r2mfl1_0(bytes, ctx),
        "r2mfl2" => match_node_r2mfl2_0(bytes, ctx),
        "r2mfl3" => match_node_r2mfl3_0(bytes, ctx),
        "r2mfl4" => match_node_r2mfl4_0(bytes, ctx),
        "r2mfl5" => match_node_r2mfl5_0(bytes, ctx),
        "r2mfl6" => match_node_r2mfl6_0(bytes, ctx),
        "r2mfl7" => match_node_r2mfl7_0(bytes, ctx),
        "r2mfl8" => match_node_r2mfl8_0(bytes, ctx),
        "r2mfl9" => match_node_r2mfl9_0(bytes, ctx),
        "r2mfla" => match_node_r2mfla_0(bytes, ctx),
        "r2mflb" => match_node_r2mflb_0(bytes, ctx),
        "r2mflc" => match_node_r2mflc_0(bytes, ctx),
        "r2mfld" => match_node_r2mfld_0(bytes, ctx),
        "r2mfle" => match_node_r2mfle_0(bytes, ctx),
        "r2mflf" => match_node_r2mflf_0(bytes, ctx),
        "r2mfw0" => match_node_r2mfw0_0(bytes, ctx),
        "r2mfw1" => match_node_r2mfw1_0(bytes, ctx),
        "r2mfw2" => match_node_r2mfw2_0(bytes, ctx),
        "r2mfw3" => match_node_r2mfw3_0(bytes, ctx),
        "r2mfw4" => match_node_r2mfw4_0(bytes, ctx),
        "r2mfw5" => match_node_r2mfw5_0(bytes, ctx),
        "r2mfw6" => match_node_r2mfw6_0(bytes, ctx),
        "r2mfw7" => match_node_r2mfw7_0(bytes, ctx),
        "r2mfw8" => match_node_r2mfw8_0(bytes, ctx),
        "r2mfw9" => match_node_r2mfw9_0(bytes, ctx),
        "r2mfwa" => match_node_r2mfwa_0(bytes, ctx),
        "r2mfwb" => match_node_r2mfwb_0(bytes, ctx),
        "r2mfwc" => match_node_r2mfwc_0(bytes, ctx),
        "r2mfwd" => match_node_r2mfwd_0(bytes, ctx),
        "r2mfwe" => match_node_r2mfwe_0(bytes, ctx),
        "r2mfwf" => match_node_r2mfwf_0(bytes, ctx),
        "reg9Plus" => match_node_reg9Plus_0(bytes, ctx),
        "regParen" => match_node_regParen_0(bytes, ctx),
        "regPlus" => match_node_regPlus_0(bytes, ctx),
        "regxPlus" => match_node_regxPlus_0(bytes, ctx),
        "remyes" => match_node_remyes_0(bytes, ctx),
        "romconst" => match_node_romconst_0(bytes, ctx),
        "rreg" => match_node_rreg_0(bytes, ctx),
        "scalefactor" => match_node_scalefactor_0(bytes, ctx),
        "skip_addr" => match_node_skip_addr_0(bytes, ctx),
        "subdiv" => match_node_subdiv_0(bytes, ctx),
        "submul" => match_node_submul_0(bytes, ctx),
        "with" => match_node_with_0(bytes, ctx),
        _ => -1
    }
}

fn match_node_Tx_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_Tx_1(bytes, ctx),
        1 => match_node_Tx_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Tx_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_Tx_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_Txb_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_Txb_1(bytes, ctx),
        1 => match_node_Txb_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Txb_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_Txb_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_Txw_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_Txw_1(bytes, ctx),
        1 => match_node_Txw_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Txw_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_Txw_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_Ty_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_Ty_1(bytes, ctx),
        1 => match_node_Ty_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Ty_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_Ty_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_Tyb_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_Tyb_1(bytes, ctx),
        1 => match_node_Tyb_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Tyb_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_Tyb_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_Tyw_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_Tyw_1(bytes, ctx),
        1 => match_node_Tyw_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Tyw_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_Tyw_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_accreg_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_accreg_1(bytes, ctx),
        1 => match_node_accreg_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_accreg_1(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (27 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 27) & 1;
    eprintln!("Trace node 1: SlaInstructionBits start=27, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_accreg_2(bytes, ctx),
        1 => match_node_accreg_3(bytes, ctx),
        _ => -1,
    }
}

fn match_node_accreg_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_accreg_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_accreg_4(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (27 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 27) & 1;
    eprintln!("Trace node 4: SlaInstructionBits start=27, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_accreg_5(bytes, ctx),
        1 => match_node_accreg_6(bytes, ctx),
        _ => -1,
    }
}

fn match_node_accreg_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 1");
    1
}

fn match_node_accreg_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 3");
    3
}

fn match_node_addr16_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_addr32_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_addr8_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_addrReg_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_addrRegD16_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_addrd16_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_addrd32_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_addrextw_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_addrpc16_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_bfOffWd_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_breg_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 0: SlaContextBits start=0, size=1, probe={}", probe);
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

fn match_node_cachetype_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 3;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_cachetype_1(bytes, ctx),
        1 => match_node_cachetype_2(bytes, ctx),
        2 => match_node_cachetype_3(bytes, ctx),
        3 => match_node_cachetype_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_cachetype_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_cachetype_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_cachetype_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_cachetype_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
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

fn match_node_cntreg_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_cntreg_1(bytes, ctx),
        1 => match_node_cntreg_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_cntreg_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_cntreg_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_const16_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_const32_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_const8_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_ctlreg_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 15;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ctlreg_1(bytes, ctx),
        1 => match_node_ctlreg_4(bytes, ctx),
        2 => match_node_ctlreg_7(bytes, ctx),
        3 => match_node_ctlreg_10(bytes, ctx),
        4 => match_node_ctlreg_13(bytes, ctx),
        5 => match_node_ctlreg_18(bytes, ctx),
        6 => match_node_ctlreg_23(bytes, ctx),
        7 => match_node_ctlreg_26(bytes, ctx),
        8 => match_node_ctlreg_29(bytes, ctx),
        9 => match_node_ctlreg_32(bytes, ctx),
        10 => match_node_ctlreg_33(bytes, ctx),
        11 => match_node_ctlreg_34(bytes, ctx),
        12 => match_node_ctlreg_35(bytes, ctx),
        13 => match_node_ctlreg_36(bytes, ctx),
        14 => match_node_ctlreg_37(bytes, ctx),
        15 => match_node_ctlreg_38(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ctlreg_1(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 1: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ctlreg_2(bytes, ctx),
        1 => match_node_ctlreg_3(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ctlreg_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_ctlreg_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_ctlreg_4(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 4: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ctlreg_5(bytes, ctx),
        1 => match_node_ctlreg_6(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ctlreg_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 1");
    1
}

fn match_node_ctlreg_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 3");
    3
}

fn match_node_ctlreg_7(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 7: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ctlreg_8(bytes, ctx),
        1 => match_node_ctlreg_9(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ctlreg_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 4");
    4
}

fn match_node_ctlreg_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 5");
    5
}

fn match_node_ctlreg_10(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 10: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ctlreg_11(bytes, ctx),
        1 => match_node_ctlreg_12(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ctlreg_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 8");
    8
}

fn match_node_ctlreg_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 6");
    6
}

fn match_node_ctlreg_13(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 3;
    eprintln!("Trace node 13: SlaInstructionBits start=4, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ctlreg_14(bytes, ctx),
        1 => match_node_ctlreg_15(bytes, ctx),
        2 => match_node_ctlreg_16(bytes, ctx),
        3 => match_node_ctlreg_17(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ctlreg_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 9");
    9
}

fn match_node_ctlreg_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 22");
    22
}

fn match_node_ctlreg_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 7");
    7
}

fn match_node_ctlreg_17(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 17: Terminal matched constructor ID 18");
    18
}

fn match_node_ctlreg_18(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 3;
    eprintln!("Trace node 18: SlaInstructionBits start=4, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ctlreg_19(bytes, ctx),
        1 => match_node_ctlreg_20(bytes, ctx),
        2 => match_node_ctlreg_21(bytes, ctx),
        3 => match_node_ctlreg_22(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ctlreg_19(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 19: Terminal matched constructor ID 10");
    10
}

fn match_node_ctlreg_20(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 20: Terminal matched constructor ID 22");
    22
}

fn match_node_ctlreg_21(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 21: Terminal matched constructor ID 14");
    14
}

fn match_node_ctlreg_22(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 22: Terminal matched constructor ID 19");
    19
}

fn match_node_ctlreg_23(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 23: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ctlreg_24(bytes, ctx),
        1 => match_node_ctlreg_25(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ctlreg_24(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 24: Terminal matched constructor ID 11");
    11
}

fn match_node_ctlreg_25(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 25: Terminal matched constructor ID 15");
    15
}

fn match_node_ctlreg_26(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 26: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ctlreg_27(bytes, ctx),
        1 => match_node_ctlreg_28(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ctlreg_27(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 27: Terminal matched constructor ID 12");
    12
}

fn match_node_ctlreg_28(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 28: Terminal matched constructor ID 16");
    16
}

fn match_node_ctlreg_29(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 29: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ctlreg_30(bytes, ctx),
        1 => match_node_ctlreg_31(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ctlreg_30(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 30: Terminal matched constructor ID 13");
    13
}

fn match_node_ctlreg_31(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 31: Terminal matched constructor ID 17");
    17
}

fn match_node_ctlreg_32(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 32: Terminal matched constructor ID 22");
    22
}

fn match_node_ctlreg_33(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 33: Terminal matched constructor ID 22");
    22
}

fn match_node_ctlreg_34(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 34: Terminal matched constructor ID 22");
    22
}

fn match_node_ctlreg_35(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 35: Terminal matched constructor ID 22");
    22
}

fn match_node_ctlreg_36(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 36: Terminal matched constructor ID 22");
    22
}

fn match_node_ctlreg_37(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 37: Terminal matched constructor ID 20");
    20
}

fn match_node_ctlreg_38(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 38: Terminal matched constructor ID 21");
    21
}

fn match_node_d32l_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_e2b_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 8) & 7;
    eprintln!("Trace node 0: SlaContextBits start=8, size=3, probe={}", probe);
    match probe {
        0 => match_node_e2b_1(bytes, ctx),
        1 => match_node_e2b_2(bytes, ctx),
        2 => match_node_e2b_3(bytes, ctx),
        3 => match_node_e2b_4(bytes, ctx),
        4 => match_node_e2b_5(bytes, ctx),
        5 => match_node_e2b_6(bytes, ctx),
        6 => match_node_e2b_7(bytes, ctx),
        7 => match_node_e2b_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_e2b_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_e2b_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_e2b_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_e2b_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_e2b_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 5");
    5
}

fn match_node_e2b_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 7");
    7
}

fn match_node_e2b_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 8");
    8
}

fn match_node_e2b_8(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 11) & 7;
    eprintln!("Trace node 8: SlaContextBits start=11, size=3, probe={}", probe);
    match probe {
        0 => match_node_e2b_9(bytes, ctx),
        1 => match_node_e2b_10(bytes, ctx),
        2 => match_node_e2b_11(bytes, ctx),
        3 => match_node_e2b_12(bytes, ctx),
        4 => match_node_e2b_13(bytes, ctx),
        5 => match_node_e2b_14(bytes, ctx),
        6 => match_node_e2b_15(bytes, ctx),
        7 => match_node_e2b_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_e2b_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 11");
    11
}

fn match_node_e2b_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 12");
    12
}

fn match_node_e2b_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 9");
    9
}

fn match_node_e2b_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 10");
    10
}

fn match_node_e2b_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 13");
    13
}

fn match_node_e2b_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched NOTHING");
    -1
}

fn match_node_e2b_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched NOTHING");
    -1
}

fn match_node_e2b_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched NOTHING");
    -1
}

fn match_node_e2d_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 8) & 7;
    eprintln!("Trace node 0: SlaContextBits start=8, size=3, probe={}", probe);
    match probe {
        0 => match_node_e2d_1(bytes, ctx),
        1 => match_node_e2d_2(bytes, ctx),
        2 => match_node_e2d_3(bytes, ctx),
        3 => match_node_e2d_4(bytes, ctx),
        4 => match_node_e2d_5(bytes, ctx),
        5 => match_node_e2d_6(bytes, ctx),
        6 => match_node_e2d_7(bytes, ctx),
        7 => match_node_e2d_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_e2d_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched NOTHING");
    -1
}

fn match_node_e2d_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched NOTHING");
    -1
}

fn match_node_e2d_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 0");
    0
}

fn match_node_e2d_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 1");
    1
}

fn match_node_e2d_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 2");
    2
}

fn match_node_e2d_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 3");
    3
}

fn match_node_e2d_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 4");
    4
}

fn match_node_e2d_8(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 11) & 7;
    eprintln!("Trace node 8: SlaContextBits start=11, size=3, probe={}", probe);
    match probe {
        0 => match_node_e2d_9(bytes, ctx),
        1 => match_node_e2d_10(bytes, ctx),
        2 => match_node_e2d_11(bytes, ctx),
        3 => match_node_e2d_12(bytes, ctx),
        4 => match_node_e2d_13(bytes, ctx),
        5 => match_node_e2d_14(bytes, ctx),
        6 => match_node_e2d_15(bytes, ctx),
        7 => match_node_e2d_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_e2d_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 7");
    7
}

fn match_node_e2d_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 8");
    8
}

fn match_node_e2d_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 5");
    5
}

fn match_node_e2d_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 6");
    6
}

fn match_node_e2d_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 9");
    9
}

fn match_node_e2d_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched NOTHING");
    -1
}

fn match_node_e2d_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched NOTHING");
    -1
}

fn match_node_e2d_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched NOTHING");
    -1
}

fn match_node_e2l_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 8) & 7;
    eprintln!("Trace node 0: SlaContextBits start=8, size=3, probe={}", probe);
    match probe {
        0 => match_node_e2l_1(bytes, ctx),
        1 => match_node_e2l_2(bytes, ctx),
        2 => match_node_e2l_3(bytes, ctx),
        3 => match_node_e2l_4(bytes, ctx),
        4 => match_node_e2l_5(bytes, ctx),
        5 => match_node_e2l_6(bytes, ctx),
        6 => match_node_e2l_7(bytes, ctx),
        7 => match_node_e2l_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_e2l_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_e2l_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_e2l_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_e2l_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_e2l_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 4");
    4
}

fn match_node_e2l_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_e2l_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 6");
    6
}

fn match_node_e2l_8(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 11) & 7;
    eprintln!("Trace node 8: SlaContextBits start=11, size=3, probe={}", probe);
    match probe {
        0 => match_node_e2l_9(bytes, ctx),
        1 => match_node_e2l_10(bytes, ctx),
        2 => match_node_e2l_11(bytes, ctx),
        3 => match_node_e2l_12(bytes, ctx),
        4 => match_node_e2l_13(bytes, ctx),
        5 => match_node_e2l_14(bytes, ctx),
        6 => match_node_e2l_15(bytes, ctx),
        7 => match_node_e2l_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_e2l_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 9");
    9
}

fn match_node_e2l_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 10");
    10
}

fn match_node_e2l_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 7");
    7
}

fn match_node_e2l_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 8");
    8
}

fn match_node_e2l_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 11");
    11
}

fn match_node_e2l_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched NOTHING");
    -1
}

fn match_node_e2l_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched NOTHING");
    -1
}

fn match_node_e2l_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched NOTHING");
    -1
}

fn match_node_e2w_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 8) & 7;
    eprintln!("Trace node 0: SlaContextBits start=8, size=3, probe={}", probe);
    match probe {
        0 => match_node_e2w_1(bytes, ctx),
        1 => match_node_e2w_2(bytes, ctx),
        2 => match_node_e2w_3(bytes, ctx),
        3 => match_node_e2w_4(bytes, ctx),
        4 => match_node_e2w_5(bytes, ctx),
        5 => match_node_e2w_6(bytes, ctx),
        6 => match_node_e2w_7(bytes, ctx),
        7 => match_node_e2w_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_e2w_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_e2w_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_e2w_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_e2w_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_e2w_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 4");
    4
}

fn match_node_e2w_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_e2w_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 6");
    6
}

fn match_node_e2w_8(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 11) & 7;
    eprintln!("Trace node 8: SlaContextBits start=11, size=3, probe={}", probe);
    match probe {
        0 => match_node_e2w_9(bytes, ctx),
        1 => match_node_e2w_10(bytes, ctx),
        2 => match_node_e2w_11(bytes, ctx),
        3 => match_node_e2w_12(bytes, ctx),
        4 => match_node_e2w_13(bytes, ctx),
        5 => match_node_e2w_14(bytes, ctx),
        6 => match_node_e2w_15(bytes, ctx),
        7 => match_node_e2w_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_e2w_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 9");
    9
}

fn match_node_e2w_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 10");
    10
}

fn match_node_e2w_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 7");
    7
}

fn match_node_e2w_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 8");
    8
}

fn match_node_e2w_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 11");
    11
}

fn match_node_e2w_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched NOTHING");
    -1
}

fn match_node_e2w_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched NOTHING");
    -1
}

fn match_node_e2w_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched NOTHING");
    -1
}

fn match_node_e2x_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 8) & 7;
    eprintln!("Trace node 0: SlaContextBits start=8, size=3, probe={}", probe);
    match probe {
        0 => match_node_e2x_1(bytes, ctx),
        1 => match_node_e2x_2(bytes, ctx),
        2 => match_node_e2x_3(bytes, ctx),
        3 => match_node_e2x_4(bytes, ctx),
        4 => match_node_e2x_5(bytes, ctx),
        5 => match_node_e2x_6(bytes, ctx),
        6 => match_node_e2x_7(bytes, ctx),
        7 => match_node_e2x_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_e2x_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 9");
    9
}

fn match_node_e2x_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 9");
    9
}

fn match_node_e2x_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 0");
    0
}

fn match_node_e2x_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 1");
    1
}

fn match_node_e2x_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 2");
    2
}

fn match_node_e2x_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 3");
    3
}

fn match_node_e2x_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 4");
    4
}

fn match_node_e2x_8(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 12) & 3;
    eprintln!("Trace node 8: SlaContextBits start=12, size=2, probe={}", probe);
    match probe {
        0 => match_node_e2x_9(bytes, ctx),
        1 => match_node_e2x_10(bytes, ctx),
        2 => match_node_e2x_11(bytes, ctx),
        3 => match_node_e2x_12(bytes, ctx),
        _ => -1,
    }
}

fn match_node_e2x_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 7");
    7
}

fn match_node_e2x_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 8");
    8
}

fn match_node_e2x_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 5");
    5
}

fn match_node_e2x_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 6");
    6
}

fn match_node_ea_index_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=0, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ea_index_1(bytes, ctx),
        1 => match_node_ea_index_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ea_index_1(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 1: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ea_index_2(bytes, ctx),
        1 => match_node_ea_index_3(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ea_index_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 3");
    3
}

fn match_node_ea_index_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_ea_index_4(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 4: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ea_index_5(bytes, ctx),
        1 => match_node_ea_index_6(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ea_index_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 1");
    1
}

fn match_node_ea_index_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 0");
    0
}

fn match_node_eab_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 7;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_eab_1(bytes, ctx),
        1 => match_node_eab_2(bytes, ctx),
        2 => match_node_eab_3(bytes, ctx),
        3 => match_node_eab_4(bytes, ctx),
        4 => match_node_eab_5(bytes, ctx),
        5 => match_node_eab_6(bytes, ctx),
        6 => match_node_eab_7(bytes, ctx),
        7 => match_node_eab_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_eab_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_eab_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_eab_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_eab_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_eab_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 5");
    5
}

fn match_node_eab_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 7");
    7
}

fn match_node_eab_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 8");
    8
}

fn match_node_eab_8(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 7;
    eprintln!("Trace node 8: SlaInstructionBits start=13, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_eab_9(bytes, ctx),
        1 => match_node_eab_10(bytes, ctx),
        2 => match_node_eab_11(bytes, ctx),
        3 => match_node_eab_12(bytes, ctx),
        4 => match_node_eab_13(bytes, ctx),
        5 => match_node_eab_14(bytes, ctx),
        6 => match_node_eab_15(bytes, ctx),
        7 => match_node_eab_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_eab_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 11");
    11
}

fn match_node_eab_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 12");
    12
}

fn match_node_eab_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 9");
    9
}

fn match_node_eab_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 10");
    10
}

fn match_node_eab_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 13");
    13
}

fn match_node_eab_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched NOTHING");
    -1
}

fn match_node_eab_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched NOTHING");
    -1
}

fn match_node_eab_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched NOTHING");
    -1
}

fn match_node_eal_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 7;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_eal_1(bytes, ctx),
        1 => match_node_eal_2(bytes, ctx),
        2 => match_node_eal_3(bytes, ctx),
        3 => match_node_eal_4(bytes, ctx),
        4 => match_node_eal_5(bytes, ctx),
        5 => match_node_eal_6(bytes, ctx),
        6 => match_node_eal_7(bytes, ctx),
        7 => match_node_eal_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_eal_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_eal_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_eal_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_eal_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_eal_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 4");
    4
}

fn match_node_eal_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_eal_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 6");
    6
}

fn match_node_eal_8(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 7;
    eprintln!("Trace node 8: SlaInstructionBits start=13, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_eal_9(bytes, ctx),
        1 => match_node_eal_10(bytes, ctx),
        2 => match_node_eal_11(bytes, ctx),
        3 => match_node_eal_12(bytes, ctx),
        4 => match_node_eal_13(bytes, ctx),
        5 => match_node_eal_14(bytes, ctx),
        6 => match_node_eal_15(bytes, ctx),
        7 => match_node_eal_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_eal_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 9");
    9
}

fn match_node_eal_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 10");
    10
}

fn match_node_eal_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 7");
    7
}

fn match_node_eal_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 8");
    8
}

fn match_node_eal_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 11");
    11
}

fn match_node_eal_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched NOTHING");
    -1
}

fn match_node_eal_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched NOTHING");
    -1
}

fn match_node_eal_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched NOTHING");
    -1
}

fn match_node_eaptr_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 7;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_eaptr_1(bytes, ctx),
        1 => match_node_eaptr_2(bytes, ctx),
        2 => match_node_eaptr_3(bytes, ctx),
        3 => match_node_eaptr_4(bytes, ctx),
        4 => match_node_eaptr_5(bytes, ctx),
        5 => match_node_eaptr_6(bytes, ctx),
        6 => match_node_eaptr_7(bytes, ctx),
        7 => match_node_eaptr_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_eaptr_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched NOTHING");
    -1
}

fn match_node_eaptr_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched NOTHING");
    -1
}

fn match_node_eaptr_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 0");
    0
}

fn match_node_eaptr_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched NOTHING");
    -1
}

fn match_node_eaptr_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched NOTHING");
    -1
}

fn match_node_eaptr_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 1");
    1
}

fn match_node_eaptr_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 2");
    2
}

fn match_node_eaptr_8(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 3;
    eprintln!("Trace node 8: SlaInstructionBits start=14, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_eaptr_9(bytes, ctx),
        1 => match_node_eaptr_10(bytes, ctx),
        2 => match_node_eaptr_11(bytes, ctx),
        3 => match_node_eaptr_12(bytes, ctx),
        _ => -1,
    }
}

fn match_node_eaptr_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 5");
    5
}

fn match_node_eaptr_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 6");
    6
}

fn match_node_eaptr_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 3");
    3
}

fn match_node_eaptr_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 4");
    4
}

fn match_node_eaw_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 7;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_eaw_1(bytes, ctx),
        1 => match_node_eaw_2(bytes, ctx),
        2 => match_node_eaw_3(bytes, ctx),
        3 => match_node_eaw_4(bytes, ctx),
        4 => match_node_eaw_5(bytes, ctx),
        5 => match_node_eaw_6(bytes, ctx),
        6 => match_node_eaw_7(bytes, ctx),
        7 => match_node_eaw_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_eaw_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_eaw_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_eaw_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_eaw_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_eaw_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 4");
    4
}

fn match_node_eaw_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_eaw_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 6");
    6
}

fn match_node_eaw_8(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 7;
    eprintln!("Trace node 8: SlaInstructionBits start=13, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_eaw_9(bytes, ctx),
        1 => match_node_eaw_10(bytes, ctx),
        2 => match_node_eaw_11(bytes, ctx),
        3 => match_node_eaw_12(bytes, ctx),
        4 => match_node_eaw_13(bytes, ctx),
        5 => match_node_eaw_14(bytes, ctx),
        6 => match_node_eaw_15(bytes, ctx),
        7 => match_node_eaw_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_eaw_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 9");
    9
}

fn match_node_eaw_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 10");
    10
}

fn match_node_eaw_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 7");
    7
}

fn match_node_eaw_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 8");
    8
}

fn match_node_eaw_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 11");
    11
}

fn match_node_eaw_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched NOTHING");
    -1
}

fn match_node_eaw_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched NOTHING");
    -1
}

fn match_node_eaw_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched NOTHING");
    -1
}

fn match_node_epw_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_extw_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_extw_1(bytes, ctx),
        1 => match_node_extw_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_extw_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_extw_2(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 7;
    eprintln!("Trace node 2: SlaInstructionBits start=13, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_extw_3(bytes, ctx),
        1 => match_node_extw_12(bytes, ctx),
        2 => match_node_extw_21(bytes, ctx),
        3 => match_node_extw_30(bytes, ctx),
        4 => match_node_extw_39(bytes, ctx),
        5 => match_node_extw_40(bytes, ctx),
        6 => match_node_extw_45(bytes, ctx),
        7 => match_node_extw_50(bytes, ctx),
        _ => -1,
    }
}

fn match_node_extw_3(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 7;
    eprintln!("Trace node 3: SlaInstructionBits start=9, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_extw_4(bytes, ctx),
        1 => match_node_extw_5(bytes, ctx),
        2 => match_node_extw_6(bytes, ctx),
        3 => match_node_extw_7(bytes, ctx),
        4 => match_node_extw_8(bytes, ctx),
        5 => match_node_extw_9(bytes, ctx),
        6 => match_node_extw_10(bytes, ctx),
        7 => match_node_extw_11(bytes, ctx),
        _ => -1,
    }
}

fn match_node_extw_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched NOTHING");
    -1
}

fn match_node_extw_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 2");
    2
}

fn match_node_extw_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 3");
    3
}

fn match_node_extw_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 4");
    4
}

fn match_node_extw_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched NOTHING");
    -1
}

fn match_node_extw_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 23");
    23
}

fn match_node_extw_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 24");
    24
}

fn match_node_extw_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 25");
    25
}

fn match_node_extw_12(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 7;
    eprintln!("Trace node 12: SlaInstructionBits start=9, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_extw_13(bytes, ctx),
        1 => match_node_extw_14(bytes, ctx),
        2 => match_node_extw_15(bytes, ctx),
        3 => match_node_extw_16(bytes, ctx),
        4 => match_node_extw_17(bytes, ctx),
        5 => match_node_extw_18(bytes, ctx),
        6 => match_node_extw_19(bytes, ctx),
        7 => match_node_extw_20(bytes, ctx),
        _ => -1,
    }
}

fn match_node_extw_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched NOTHING");
    -1
}

fn match_node_extw_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 5");
    5
}

fn match_node_extw_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 6");
    6
}

fn match_node_extw_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 7");
    7
}

fn match_node_extw_17(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 17: Terminal matched NOTHING");
    -1
}

fn match_node_extw_18(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 18: Terminal matched constructor ID 26");
    26
}

fn match_node_extw_19(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 19: Terminal matched constructor ID 27");
    27
}

fn match_node_extw_20(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 20: Terminal matched constructor ID 28");
    28
}

fn match_node_extw_21(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 7;
    eprintln!("Trace node 21: SlaInstructionBits start=9, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_extw_22(bytes, ctx),
        1 => match_node_extw_23(bytes, ctx),
        2 => match_node_extw_24(bytes, ctx),
        3 => match_node_extw_25(bytes, ctx),
        4 => match_node_extw_26(bytes, ctx),
        5 => match_node_extw_27(bytes, ctx),
        6 => match_node_extw_28(bytes, ctx),
        7 => match_node_extw_29(bytes, ctx),
        _ => -1,
    }
}

fn match_node_extw_22(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 22: Terminal matched NOTHING");
    -1
}

fn match_node_extw_23(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 23: Terminal matched constructor ID 8");
    8
}

fn match_node_extw_24(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 24: Terminal matched constructor ID 9");
    9
}

fn match_node_extw_25(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 25: Terminal matched constructor ID 10");
    10
}

fn match_node_extw_26(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 26: Terminal matched NOTHING");
    -1
}

fn match_node_extw_27(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 27: Terminal matched constructor ID 29");
    29
}

fn match_node_extw_28(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 28: Terminal matched constructor ID 30");
    30
}

fn match_node_extw_29(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 29: Terminal matched constructor ID 31");
    31
}

fn match_node_extw_30(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 7;
    eprintln!("Trace node 30: SlaInstructionBits start=9, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_extw_31(bytes, ctx),
        1 => match_node_extw_32(bytes, ctx),
        2 => match_node_extw_33(bytes, ctx),
        3 => match_node_extw_34(bytes, ctx),
        4 => match_node_extw_35(bytes, ctx),
        5 => match_node_extw_36(bytes, ctx),
        6 => match_node_extw_37(bytes, ctx),
        7 => match_node_extw_38(bytes, ctx),
        _ => -1,
    }
}

fn match_node_extw_31(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 31: Terminal matched NOTHING");
    -1
}

fn match_node_extw_32(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 32: Terminal matched constructor ID 11");
    11
}

fn match_node_extw_33(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 33: Terminal matched constructor ID 12");
    12
}

fn match_node_extw_34(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 34: Terminal matched constructor ID 13");
    13
}

fn match_node_extw_35(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 35: Terminal matched NOTHING");
    -1
}

fn match_node_extw_36(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 36: Terminal matched constructor ID 32");
    32
}

fn match_node_extw_37(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 37: Terminal matched constructor ID 33");
    33
}

fn match_node_extw_38(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 38: Terminal matched constructor ID 34");
    34
}

fn match_node_extw_39(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 39: Terminal matched NOTHING");
    -1
}

fn match_node_extw_40(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 40: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_extw_41(bytes, ctx),
        1 => match_node_extw_42(bytes, ctx),
        2 => match_node_extw_43(bytes, ctx),
        3 => match_node_extw_44(bytes, ctx),
        _ => -1,
    }
}

fn match_node_extw_41(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 41: Terminal matched NOTHING");
    -1
}

fn match_node_extw_42(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 42: Terminal matched constructor ID 14");
    14
}

fn match_node_extw_43(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 43: Terminal matched constructor ID 15");
    15
}

fn match_node_extw_44(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 44: Terminal matched constructor ID 16");
    16
}

fn match_node_extw_45(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 45: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_extw_46(bytes, ctx),
        1 => match_node_extw_47(bytes, ctx),
        2 => match_node_extw_48(bytes, ctx),
        3 => match_node_extw_49(bytes, ctx),
        _ => -1,
    }
}

fn match_node_extw_46(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 46: Terminal matched NOTHING");
    -1
}

fn match_node_extw_47(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 47: Terminal matched constructor ID 17");
    17
}

fn match_node_extw_48(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 48: Terminal matched constructor ID 18");
    18
}

fn match_node_extw_49(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 49: Terminal matched constructor ID 19");
    19
}

fn match_node_extw_50(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 50: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_extw_51(bytes, ctx),
        1 => match_node_extw_52(bytes, ctx),
        2 => match_node_extw_53(bytes, ctx),
        3 => match_node_extw_54(bytes, ctx),
        _ => -1,
    }
}

fn match_node_extw_51(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 51: Terminal matched NOTHING");
    -1
}

fn match_node_extw_52(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 52: Terminal matched constructor ID 20");
    20
}

fn match_node_extw_53(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 53: Terminal matched constructor ID 21");
    21
}

fn match_node_extw_54(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 54: Terminal matched constructor ID 22");
    22
}

fn match_node_f_mem_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 7;
    eprintln!("Trace node 0: SlaInstructionBits start=3, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_f_mem_1(bytes, ctx),
        1 => match_node_f_mem_2(bytes, ctx),
        2 => match_node_f_mem_3(bytes, ctx),
        3 => match_node_f_mem_4(bytes, ctx),
        4 => match_node_f_mem_5(bytes, ctx),
        5 => match_node_f_mem_6(bytes, ctx),
        6 => match_node_f_mem_7(bytes, ctx),
        7 => match_node_f_mem_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_f_mem_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_f_mem_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 3");
    3
}

fn match_node_f_mem_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 4");
    4
}

fn match_node_f_mem_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 5");
    5
}

fn match_node_f_mem_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 1");
    1
}

fn match_node_f_mem_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 6");
    6
}

fn match_node_f_mem_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 2");
    2
}

fn match_node_f_mem_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched NOTHING");
    -1
}

fn match_node_f_off_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_f_off_1(bytes, ctx),
        1 => match_node_f_off_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_f_off_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_f_off_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_f_wd_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_f_wd_1(bytes, ctx),
        1 => match_node_f_wd_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_f_wd_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_f_wd_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_fabsrnd_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_fabsrnd_1(bytes, ctx),
        1 => match_node_fabsrnd_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_fabsrnd_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_fabsrnd_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_faddrnd_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_faddrnd_1(bytes, ctx),
        1 => match_node_faddrnd_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_faddrnd_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_faddrnd_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_fcc_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 5 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 31;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=5, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_fcc_1(bytes, ctx),
        1 => match_node_fcc_2(bytes, ctx),
        2 => match_node_fcc_3(bytes, ctx),
        3 => match_node_fcc_4(bytes, ctx),
        4 => match_node_fcc_5(bytes, ctx),
        5 => match_node_fcc_6(bytes, ctx),
        6 => match_node_fcc_7(bytes, ctx),
        7 => match_node_fcc_8(bytes, ctx),
        8 => match_node_fcc_9(bytes, ctx),
        9 => match_node_fcc_10(bytes, ctx),
        10 => match_node_fcc_11(bytes, ctx),
        11 => match_node_fcc_12(bytes, ctx),
        12 => match_node_fcc_13(bytes, ctx),
        13 => match_node_fcc_14(bytes, ctx),
        14 => match_node_fcc_15(bytes, ctx),
        15 => match_node_fcc_16(bytes, ctx),
        16 => match_node_fcc_17(bytes, ctx),
        17 => match_node_fcc_18(bytes, ctx),
        18 => match_node_fcc_19(bytes, ctx),
        19 => match_node_fcc_20(bytes, ctx),
        20 => match_node_fcc_21(bytes, ctx),
        21 => match_node_fcc_22(bytes, ctx),
        22 => match_node_fcc_23(bytes, ctx),
        23 => match_node_fcc_24(bytes, ctx),
        24 => match_node_fcc_25(bytes, ctx),
        25 => match_node_fcc_26(bytes, ctx),
        26 => match_node_fcc_27(bytes, ctx),
        27 => match_node_fcc_28(bytes, ctx),
        28 => match_node_fcc_29(bytes, ctx),
        29 => match_node_fcc_30(bytes, ctx),
        30 => match_node_fcc_31(bytes, ctx),
        31 => match_node_fcc_32(bytes, ctx),
        _ => -1,
    }
}

fn match_node_fcc_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 26");
    26
}

fn match_node_fcc_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_fcc_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 14");
    14
}

fn match_node_fcc_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 16");
    16
}

fn match_node_fcc_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 18");
    18
}

fn match_node_fcc_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 20");
    20
}

fn match_node_fcc_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 22");
    22
}

fn match_node_fcc_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 24");
    24
}

fn match_node_fcc_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 25");
    25
}

fn match_node_fcc_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 23");
    23
}

fn match_node_fcc_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 21");
    21
}

fn match_node_fcc_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 19");
    19
}

fn match_node_fcc_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 17");
    17
}

fn match_node_fcc_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 15");
    15
}

fn match_node_fcc_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 1");
    1
}

fn match_node_fcc_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 27");
    27
}

fn match_node_fcc_17(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 17: Terminal matched constructor ID 28");
    28
}

fn match_node_fcc_18(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 18: Terminal matched constructor ID 30");
    30
}

fn match_node_fcc_19(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 19: Terminal matched constructor ID 2");
    2
}

fn match_node_fcc_20(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 20: Terminal matched constructor ID 4");
    4
}

fn match_node_fcc_21(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 21: Terminal matched constructor ID 6");
    6
}

fn match_node_fcc_22(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 22: Terminal matched constructor ID 8");
    8
}

fn match_node_fcc_23(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 23: Terminal matched constructor ID 10");
    10
}

fn match_node_fcc_24(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 24: Terminal matched constructor ID 12");
    12
}

fn match_node_fcc_25(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 25: Terminal matched constructor ID 13");
    13
}

fn match_node_fcc_26(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 26: Terminal matched constructor ID 11");
    11
}

fn match_node_fcc_27(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 27: Terminal matched constructor ID 9");
    9
}

fn match_node_fcc_28(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 28: Terminal matched constructor ID 7");
    7
}

fn match_node_fcc_29(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 29: Terminal matched constructor ID 5");
    5
}

fn match_node_fcc_30(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 30: Terminal matched constructor ID 3");
    3
}

fn match_node_fcc_31(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 31: Terminal matched constructor ID 31");
    31
}

fn match_node_fcc_32(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 32: Terminal matched constructor ID 29");
    29
}

fn match_node_fdivrnd_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_fdivrnd_1(bytes, ctx),
        1 => match_node_fdivrnd_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_fdivrnd_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_fdivrnd_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_fl_breg_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_fl_breg_1(bytes, ctx),
        1 => match_node_fl_breg_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_fl_breg_1(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 1) & 1;
    eprintln!("Trace node 1: SlaContextBits start=1, size=1, probe={}", probe);
    match probe {
        0 => match_node_fl_breg_2(bytes, ctx),
        1 => match_node_fl_breg_3(bytes, ctx),
        _ => -1,
    }
}

fn match_node_fl_breg_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 2");
    2
}

fn match_node_fl_breg_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 3");
    3
}

fn match_node_fl_breg_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 1");
    1
}

fn match_node_fmovernd_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_fmovernd_1(bytes, ctx),
        1 => match_node_fmovernd_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_fmovernd_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_fmovernd_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_fmulrnd_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_fmulrnd_1(bytes, ctx),
        1 => match_node_fmulrnd_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_fmulrnd_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_fmulrnd_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_fnegrnd_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_fnegrnd_1(bytes, ctx),
        1 => match_node_fnegrnd_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_fnegrnd_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_fnegrnd_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_fp2mC0_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=5, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_fp2mC0_1(bytes, ctx),
        1 => match_node_fp2mC0_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_fp2mC0_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_fp2mC0_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_fp2mC1_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_fp2mC1_1(bytes, ctx),
        1 => match_node_fp2mC1_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_fp2mC1_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_fp2mC1_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_fp2mC2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=3, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_fp2mC2_1(bytes, ctx),
        1 => match_node_fp2mC2_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_fp2mC2_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_fp2mC2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_fp2mF0_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_fp2mF0_1(bytes, ctx),
        1 => match_node_fp2mF0_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_fp2mF0_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_fp2mF0_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_fp2mF1_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_fp2mF1_1(bytes, ctx),
        1 => match_node_fp2mF1_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_fp2mF1_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_fp2mF1_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_fp2mF2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_fp2mF2_1(bytes, ctx),
        1 => match_node_fp2mF2_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_fp2mF2_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_fp2mF2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_fp2mF3_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_fp2mF3_1(bytes, ctx),
        1 => match_node_fp2mF3_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_fp2mF3_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_fp2mF3_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_fp2mF4_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_fp2mF4_1(bytes, ctx),
        1 => match_node_fp2mF4_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_fp2mF4_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_fp2mF4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_fp2mF5_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_fp2mF5_1(bytes, ctx),
        1 => match_node_fp2mF5_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_fp2mF5_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_fp2mF5_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_fp2mF6_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_fp2mF6_1(bytes, ctx),
        1 => match_node_fp2mF6_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_fp2mF6_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_fp2mF6_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_fp2mF7_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_fp2mF7_1(bytes, ctx),
        1 => match_node_fp2mF7_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_fp2mF7_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_fp2mF7_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_fp2mR0_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_fp2mR0_1(bytes, ctx),
        1 => match_node_fp2mR0_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_fp2mR0_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_fp2mR0_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_fp2mR1_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_fp2mR1_1(bytes, ctx),
        1 => match_node_fp2mR1_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_fp2mR1_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_fp2mR1_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_fp2mR2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_fp2mR2_1(bytes, ctx),
        1 => match_node_fp2mR2_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_fp2mR2_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_fp2mR2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_fp2mR3_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_fp2mR3_1(bytes, ctx),
        1 => match_node_fp2mR3_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_fp2mR3_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_fp2mR3_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_fp2mR4_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_fp2mR4_1(bytes, ctx),
        1 => match_node_fp2mR4_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_fp2mR4_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_fp2mR4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_fp2mR5_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_fp2mR5_1(bytes, ctx),
        1 => match_node_fp2mR5_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_fp2mR5_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_fp2mR5_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_fp2mR6_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_fp2mR6_1(bytes, ctx),
        1 => match_node_fp2mR6_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_fp2mR6_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_fp2mR6_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_fp2mR7_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_fp2mR7_1(bytes, ctx),
        1 => match_node_fp2mR7_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_fp2mR7_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_fp2mR7_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_fprec_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 7;
    eprintln!("Trace node 0: SlaInstructionBits start=3, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_fprec_1(bytes, ctx),
        1 => match_node_fprec_2(bytes, ctx),
        2 => match_node_fprec_3(bytes, ctx),
        3 => match_node_fprec_4(bytes, ctx),
        4 => match_node_fprec_5(bytes, ctx),
        5 => match_node_fprec_6(bytes, ctx),
        6 => match_node_fprec_7(bytes, ctx),
        7 => match_node_fprec_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_fprec_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_fprec_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_fprec_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_fprec_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_fprec_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 4");
    4
}

fn match_node_fprec_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_fprec_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 6");
    6
}

fn match_node_fprec_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 7");
    7
}

fn match_node_fsqrtrnd_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_fsqrtrnd_1(bytes, ctx),
        1 => match_node_fsqrtrnd_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_fsqrtrnd_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_fsqrtrnd_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_fsubrnd_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_fsubrnd_1(bytes, ctx),
        1 => match_node_fsubrnd_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_fsubrnd_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_fsubrnd_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_instruction_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 14) & 1;
    eprintln!("Trace node 0: SlaContextBits start=14, size=1, probe={}", probe);
    match probe {
        0 => match_node_instruction_1(bytes, ctx),
        1 => match_node_instruction_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_2(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 15;
    eprintln!("Trace node 2: SlaInstructionBits start=0, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_3(bytes, ctx),
        1 => match_node_instruction_222(bytes, ctx),
        2 => match_node_instruction_227(bytes, ctx),
        3 => match_node_instruction_234(bytes, ctx),
        4 => match_node_instruction_241(bytes, ctx),
        5 => match_node_instruction_432(bytes, ctx),
        6 => match_node_instruction_453(bytes, ctx),
        7 => match_node_instruction_460(bytes, ctx),
        8 => match_node_instruction_467(bytes, ctx),
        9 => match_node_instruction_504(bytes, ctx),
        10 => match_node_instruction_525(bytes, ctx),
        11 => match_node_instruction_584(bytes, ctx),
        12 => match_node_instruction_611(bytes, ctx),
        13 => match_node_instruction_646(bytes, ctx),
        14 => match_node_instruction_667(bytes, ctx),
        15 => match_node_instruction_780(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_3(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 7;
    eprintln!("Trace node 3: SlaInstructionBits start=7, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_4(bytes, ctx),
        1 => match_node_instruction_45(bytes, ctx),
        2 => match_node_instruction_86(bytes, ctx),
        3 => match_node_instruction_125(bytes, ctx),
        4 => match_node_instruction_194(bytes, ctx),
        5 => match_node_instruction_201(bytes, ctx),
        6 => match_node_instruction_208(bytes, ctx),
        7 => match_node_instruction_215(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_4(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 7;
    eprintln!("Trace node 4: SlaInstructionBits start=4, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_5(bytes, ctx),
        1 => match_node_instruction_10(bytes, ctx),
        2 => match_node_instruction_15(bytes, ctx),
        3 => match_node_instruction_20(bytes, ctx),
        4 => match_node_instruction_25(bytes, ctx),
        5 => match_node_instruction_30(bytes, ctx),
        6 => match_node_instruction_35(bytes, ctx),
        7 => match_node_instruction_40(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_5(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 5: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_6(bytes, ctx),
        1 => match_node_instruction_9(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_6(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 6: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_7(bytes, ctx),
        1 => match_node_instruction_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 222");
    222
}

fn match_node_instruction_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 222");
    222
}

fn match_node_instruction_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 225");
    225
}

fn match_node_instruction_10(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 10: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_11(bytes, ctx),
        1 => match_node_instruction_14(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_11(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 11: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_12(bytes, ctx),
        1 => match_node_instruction_13(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 27");
    27
}

fn match_node_instruction_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 27");
    27
}

fn match_node_instruction_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 30");
    30
}

fn match_node_instruction_15(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 15: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_16(bytes, ctx),
        1 => match_node_instruction_19(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_16(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 16: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_17(bytes, ctx),
        1 => match_node_instruction_18(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_17(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 17: Terminal matched constructor ID 270");
    270
}

fn match_node_instruction_18(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 18: Terminal matched constructor ID 270");
    270
}

fn match_node_instruction_19(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 19: Terminal matched constructor ID 270");
    270
}

fn match_node_instruction_20(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 20: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_21(bytes, ctx),
        1 => match_node_instruction_24(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_21(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 21: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_22(bytes, ctx),
        1 => match_node_instruction_23(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_22(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 22: Terminal matched constructor ID 10");
    10
}

fn match_node_instruction_23(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 23: Terminal matched constructor ID 10");
    10
}

fn match_node_instruction_24(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 24: Terminal matched constructor ID 10");
    10
}

fn match_node_instruction_25(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 25: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_26(bytes, ctx),
        1 => match_node_instruction_29(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_26(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 26: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_27(bytes, ctx),
        1 => match_node_instruction_28(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_27(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 27: Terminal matched constructor ID 74");
    74
}

fn match_node_instruction_28(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 28: Terminal matched constructor ID 72");
    72
}

fn match_node_instruction_29(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 29: Terminal matched constructor ID 72");
    72
}

fn match_node_instruction_30(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 30: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_31(bytes, ctx),
        1 => match_node_instruction_34(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_31(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 31: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_32(bytes, ctx),
        1 => match_node_instruction_33(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_32(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 32: Terminal matched constructor ID 118");
    118
}

fn match_node_instruction_33(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 33: Terminal matched constructor ID 118");
    118
}

fn match_node_instruction_34(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 34: Terminal matched constructor ID 121");
    121
}

fn match_node_instruction_35(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 35: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_36(bytes, ctx),
        1 => match_node_instruction_39(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_36(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 36: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_37(bytes, ctx),
        1 => match_node_instruction_38(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_37(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 37: Terminal matched constructor ID 105");
    105
}

fn match_node_instruction_38(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 38: Terminal matched constructor ID 105");
    105
}

fn match_node_instruction_39(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 39: Terminal matched constructor ID 105");
    105
}

fn match_node_instruction_40(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 40: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_41(bytes, ctx),
        1 => match_node_instruction_44(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_41(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (16 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 16) & 1;
    eprintln!("Trace node 41: SlaInstructionBits start=16, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_42(bytes, ctx),
        1 => match_node_instruction_43(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_42(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 42: Terminal matched constructor ID 197");
    197
}

fn match_node_instruction_43(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 43: Terminal matched constructor ID 199");
    199
}

fn match_node_instruction_44(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 44: Terminal matched constructor ID 194");
    194
}

fn match_node_instruction_45(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 7;
    eprintln!("Trace node 45: SlaInstructionBits start=4, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_46(bytes, ctx),
        1 => match_node_instruction_51(bytes, ctx),
        2 => match_node_instruction_56(bytes, ctx),
        3 => match_node_instruction_61(bytes, ctx),
        4 => match_node_instruction_66(bytes, ctx),
        5 => match_node_instruction_71(bytes, ctx),
        6 => match_node_instruction_76(bytes, ctx),
        7 => match_node_instruction_81(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_46(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 46: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_47(bytes, ctx),
        1 => match_node_instruction_50(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_47(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 47: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_48(bytes, ctx),
        1 => match_node_instruction_49(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_48(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 48: Terminal matched constructor ID 223");
    223
}

fn match_node_instruction_49(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 49: Terminal matched constructor ID 223");
    223
}

fn match_node_instruction_50(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 50: Terminal matched constructor ID 226");
    226
}

fn match_node_instruction_51(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 51: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_52(bytes, ctx),
        1 => match_node_instruction_55(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_52(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 52: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_53(bytes, ctx),
        1 => match_node_instruction_54(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_53(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 53: Terminal matched constructor ID 28");
    28
}

fn match_node_instruction_54(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 54: Terminal matched constructor ID 28");
    28
}

fn match_node_instruction_55(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 55: Terminal matched constructor ID 31");
    31
}

fn match_node_instruction_56(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 56: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_57(bytes, ctx),
        1 => match_node_instruction_60(bytes, ctx),
        _ => -1,
    }
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
    eprintln!("Trace node 58: Terminal matched constructor ID 271");
    271
}

fn match_node_instruction_59(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 59: Terminal matched constructor ID 271");
    271
}

fn match_node_instruction_60(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 60: Terminal matched constructor ID 271");
    271
}

fn match_node_instruction_61(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 61: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_62(bytes, ctx),
        1 => match_node_instruction_65(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_62(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 62: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_63(bytes, ctx),
        1 => match_node_instruction_64(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_63(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 63: Terminal matched constructor ID 11");
    11
}

fn match_node_instruction_64(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 64: Terminal matched constructor ID 11");
    11
}

fn match_node_instruction_65(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 65: Terminal matched constructor ID 11");
    11
}

fn match_node_instruction_66(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 66: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_67(bytes, ctx),
        1 => match_node_instruction_70(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_67(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 67: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_68(bytes, ctx),
        1 => match_node_instruction_69(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_68(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 68: Terminal matched constructor ID 46");
    46
}

fn match_node_instruction_69(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 69: Terminal matched constructor ID 44");
    44
}

fn match_node_instruction_70(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 70: Terminal matched constructor ID 44");
    44
}

fn match_node_instruction_71(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 71: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_72(bytes, ctx),
        1 => match_node_instruction_75(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_72(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 72: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_73(bytes, ctx),
        1 => match_node_instruction_74(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_73(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 73: Terminal matched constructor ID 119");
    119
}

fn match_node_instruction_74(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 74: Terminal matched constructor ID 119");
    119
}

fn match_node_instruction_75(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 75: Terminal matched constructor ID 122");
    122
}

fn match_node_instruction_76(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 76: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_77(bytes, ctx),
        1 => match_node_instruction_80(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_77(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 77: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_78(bytes, ctx),
        1 => match_node_instruction_79(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_78(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 78: Terminal matched constructor ID 106");
    106
}

fn match_node_instruction_79(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 79: Terminal matched constructor ID 106");
    106
}

fn match_node_instruction_80(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 80: Terminal matched constructor ID 106");
    106
}

fn match_node_instruction_81(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 81: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_82(bytes, ctx),
        1 => match_node_instruction_85(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_82(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (16 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 16) & 1;
    eprintln!("Trace node 82: SlaInstructionBits start=16, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_83(bytes, ctx),
        1 => match_node_instruction_84(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_83(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 83: Terminal matched constructor ID 198");
    198
}

fn match_node_instruction_84(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 84: Terminal matched constructor ID 200");
    200
}

fn match_node_instruction_85(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 85: Terminal matched constructor ID 195");
    195
}

fn match_node_instruction_86(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 7;
    eprintln!("Trace node 86: SlaInstructionBits start=4, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_87(bytes, ctx),
        1 => match_node_instruction_92(bytes, ctx),
        2 => match_node_instruction_97(bytes, ctx),
        3 => match_node_instruction_102(bytes, ctx),
        4 => match_node_instruction_107(bytes, ctx),
        5 => match_node_instruction_112(bytes, ctx),
        6 => match_node_instruction_117(bytes, ctx),
        7 => match_node_instruction_122(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_87(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 87: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_88(bytes, ctx),
        1 => match_node_instruction_91(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_88(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 88: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_89(bytes, ctx),
        1 => match_node_instruction_90(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_89(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 89: Terminal matched constructor ID 224");
    224
}

fn match_node_instruction_90(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 90: Terminal matched constructor ID 224");
    224
}

fn match_node_instruction_91(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 91: Terminal matched constructor ID 224");
    224
}

fn match_node_instruction_92(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 92: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_93(bytes, ctx),
        1 => match_node_instruction_96(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_93(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 93: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_94(bytes, ctx),
        1 => match_node_instruction_95(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_94(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 94: Terminal matched constructor ID 29");
    29
}

fn match_node_instruction_95(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 95: Terminal matched constructor ID 29");
    29
}

fn match_node_instruction_96(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 96: Terminal matched constructor ID 29");
    29
}

fn match_node_instruction_97(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 97: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_98(bytes, ctx),
        1 => match_node_instruction_101(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_98(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 98: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_99(bytes, ctx),
        1 => match_node_instruction_100(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_99(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 99: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_100(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 100: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_101(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 101: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_102(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 102: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_103(bytes, ctx),
        1 => match_node_instruction_106(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_103(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 103: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_104(bytes, ctx),
        1 => match_node_instruction_105(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_104(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 104: Terminal matched constructor ID 12");
    12
}

fn match_node_instruction_105(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 105: Terminal matched constructor ID 12");
    12
}

fn match_node_instruction_106(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 106: Terminal matched constructor ID 12");
    12
}

fn match_node_instruction_107(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 107: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_108(bytes, ctx),
        1 => match_node_instruction_111(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_108(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 108: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_109(bytes, ctx),
        1 => match_node_instruction_110(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_109(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 109: Terminal matched constructor ID 50");
    50
}

fn match_node_instruction_110(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 110: Terminal matched constructor ID 48");
    48
}

fn match_node_instruction_111(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 111: Terminal matched constructor ID 48");
    48
}

fn match_node_instruction_112(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 112: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_113(bytes, ctx),
        1 => match_node_instruction_116(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_113(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 113: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_114(bytes, ctx),
        1 => match_node_instruction_115(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_114(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 114: Terminal matched constructor ID 120");
    120
}

fn match_node_instruction_115(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 115: Terminal matched constructor ID 120");
    120
}

fn match_node_instruction_116(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 116: Terminal matched constructor ID 120");
    120
}

fn match_node_instruction_117(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 117: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_118(bytes, ctx),
        1 => match_node_instruction_121(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_118(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 118: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_119(bytes, ctx),
        1 => match_node_instruction_120(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_119(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 119: Terminal matched constructor ID 107");
    107
}

fn match_node_instruction_120(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 120: Terminal matched constructor ID 107");
    107
}

fn match_node_instruction_121(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 121: Terminal matched constructor ID 107");
    107
}

fn match_node_instruction_122(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 122: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_123(bytes, ctx),
        1 => match_node_instruction_124(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_123(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 123: Terminal matched constructor ID 201");
    201
}

fn match_node_instruction_124(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 124: Terminal matched constructor ID 196");
    196
}

fn match_node_instruction_125(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 7;
    eprintln!("Trace node 125: SlaInstructionBits start=4, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_126(bytes, ctx),
        1 => match_node_instruction_143(bytes, ctx),
        2 => match_node_instruction_160(bytes, ctx),
        3 => match_node_instruction_177(bytes, ctx),
        4 => match_node_instruction_186(bytes, ctx),
        5 => match_node_instruction_191(bytes, ctx),
        6 => match_node_instruction_192(bytes, ctx),
        7 => match_node_instruction_193(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_126(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 7;
    eprintln!("Trace node 126: SlaInstructionBits start=10, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_127(bytes, ctx),
        1 => match_node_instruction_128(bytes, ctx),
        2 => match_node_instruction_129(bytes, ctx),
        3 => match_node_instruction_132(bytes, ctx),
        4 => match_node_instruction_133(bytes, ctx),
        5 => match_node_instruction_134(bytes, ctx),
        6 => match_node_instruction_137(bytes, ctx),
        7 => match_node_instruction_140(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_127(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 127: Terminal matched constructor ID 75");
    75
}

fn match_node_instruction_128(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 128: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_129(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 129: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_130(bytes, ctx),
        1 => match_node_instruction_131(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_130(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 130: Terminal matched constructor ID 88");
    88
}

fn match_node_instruction_131(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 131: Terminal matched constructor ID 85");
    85
}

fn match_node_instruction_132(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 132: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_133(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 133: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_134(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 134: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_135(bytes, ctx),
        1 => match_node_instruction_136(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_135(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 135: Terminal matched constructor ID 88");
    88
}

fn match_node_instruction_136(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 136: Terminal matched constructor ID 85");
    85
}

fn match_node_instruction_137(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 137: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_138(bytes, ctx),
        1 => match_node_instruction_139(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_138(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 138: Terminal matched constructor ID 88");
    88
}

fn match_node_instruction_139(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 139: Terminal matched constructor ID 85");
    85
}

fn match_node_instruction_140(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 140: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_141(bytes, ctx),
        1 => match_node_instruction_142(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_141(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 141: Terminal matched constructor ID 88");
    88
}

fn match_node_instruction_142(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 142: Terminal matched constructor ID 85");
    85
}

fn match_node_instruction_143(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 7;
    eprintln!("Trace node 143: SlaInstructionBits start=10, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_144(bytes, ctx),
        1 => match_node_instruction_145(bytes, ctx),
        2 => match_node_instruction_146(bytes, ctx),
        3 => match_node_instruction_149(bytes, ctx),
        4 => match_node_instruction_150(bytes, ctx),
        5 => match_node_instruction_151(bytes, ctx),
        6 => match_node_instruction_154(bytes, ctx),
        7 => match_node_instruction_157(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_144(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 144: Terminal matched constructor ID 76");
    76
}

fn match_node_instruction_145(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 145: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_146(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 146: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_147(bytes, ctx),
        1 => match_node_instruction_148(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_147(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 147: Terminal matched constructor ID 89");
    89
}

fn match_node_instruction_148(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 148: Terminal matched constructor ID 86");
    86
}

fn match_node_instruction_149(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 149: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_150(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 150: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_151(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 151: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_152(bytes, ctx),
        1 => match_node_instruction_153(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_152(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 152: Terminal matched constructor ID 89");
    89
}

fn match_node_instruction_153(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 153: Terminal matched constructor ID 86");
    86
}

fn match_node_instruction_154(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 154: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_155(bytes, ctx),
        1 => match_node_instruction_156(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_155(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 155: Terminal matched constructor ID 89");
    89
}

fn match_node_instruction_156(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 156: Terminal matched constructor ID 86");
    86
}

fn match_node_instruction_157(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 157: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_158(bytes, ctx),
        1 => match_node_instruction_159(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_158(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 158: Terminal matched constructor ID 89");
    89
}

fn match_node_instruction_159(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 159: Terminal matched constructor ID 86");
    86
}

fn match_node_instruction_160(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 7;
    eprintln!("Trace node 160: SlaInstructionBits start=10, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_161(bytes, ctx),
        1 => match_node_instruction_162(bytes, ctx),
        2 => match_node_instruction_163(bytes, ctx),
        3 => match_node_instruction_166(bytes, ctx),
        4 => match_node_instruction_167(bytes, ctx),
        5 => match_node_instruction_168(bytes, ctx),
        6 => match_node_instruction_171(bytes, ctx),
        7 => match_node_instruction_174(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_161(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 161: Terminal matched constructor ID 423");
    423
}

fn match_node_instruction_162(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 162: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_163(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 163: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_164(bytes, ctx),
        1 => match_node_instruction_165(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_164(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 164: Terminal matched constructor ID 90");
    90
}

fn match_node_instruction_165(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 165: Terminal matched constructor ID 87");
    87
}

fn match_node_instruction_166(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 166: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_167(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 167: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_168(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 168: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_169(bytes, ctx),
        1 => match_node_instruction_170(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_169(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 169: Terminal matched constructor ID 90");
    90
}

fn match_node_instruction_170(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 170: Terminal matched constructor ID 87");
    87
}

fn match_node_instruction_171(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 171: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_172(bytes, ctx),
        1 => match_node_instruction_173(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_172(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 172: Terminal matched constructor ID 90");
    90
}

fn match_node_instruction_173(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 173: Terminal matched constructor ID 87");
    87
}

fn match_node_instruction_174(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 174: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_175(bytes, ctx),
        1 => match_node_instruction_176(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_175(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 175: Terminal matched constructor ID 90");
    90
}

fn match_node_instruction_176(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 176: Terminal matched constructor ID 87");
    87
}

fn match_node_instruction_177(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 7;
    eprintln!("Trace node 177: SlaInstructionBits start=10, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_178(bytes, ctx),
        1 => match_node_instruction_179(bytes, ctx),
        2 => match_node_instruction_180(bytes, ctx),
        3 => match_node_instruction_181(bytes, ctx),
        4 => match_node_instruction_182(bytes, ctx),
        5 => match_node_instruction_183(bytes, ctx),
        6 => match_node_instruction_184(bytes, ctx),
        7 => match_node_instruction_185(bytes, ctx),
        _ => -1,
    }
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
    eprintln!("Trace node 180: Terminal matched constructor ID 77");
    77
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
    eprintln!("Trace node 183: Terminal matched constructor ID 77");
    77
}

fn match_node_instruction_184(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 184: Terminal matched constructor ID 77");
    77
}

fn match_node_instruction_185(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 185: Terminal matched constructor ID 77");
    77
}

fn match_node_instruction_186(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 186: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_187(bytes, ctx),
        1 => match_node_instruction_190(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_187(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 187: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_188(bytes, ctx),
        1 => match_node_instruction_189(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_188(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 188: Terminal matched constructor ID 67");
    67
}

fn match_node_instruction_189(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 189: Terminal matched constructor ID 65");
    65
}

fn match_node_instruction_190(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 190: Terminal matched constructor ID 65");
    65
}

fn match_node_instruction_191(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 191: Terminal matched constructor ID 80");
    80
}

fn match_node_instruction_192(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 192: Terminal matched constructor ID 78");
    78
}

fn match_node_instruction_193(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 193: Terminal matched constructor ID 79");
    79
}

fn match_node_instruction_194(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 194: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_195(bytes, ctx),
        1 => match_node_instruction_200(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_195(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 195: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_196(bytes, ctx),
        1 => match_node_instruction_199(bytes, ctx),
        _ => -1,
    }
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
    eprintln!("Trace node 197: Terminal matched constructor ID 73");
    73
}

fn match_node_instruction_198(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 198: Terminal matched constructor ID 189");
    189
}

fn match_node_instruction_199(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 199: Terminal matched constructor ID 71");
    71
}

fn match_node_instruction_200(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 200: Terminal matched constructor ID 71");
    71
}

fn match_node_instruction_201(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 201: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_202(bytes, ctx),
        1 => match_node_instruction_207(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_202(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 202: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_203(bytes, ctx),
        1 => match_node_instruction_206(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_203(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 203: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_204(bytes, ctx),
        1 => match_node_instruction_205(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_204(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 204: Terminal matched constructor ID 45");
    45
}

fn match_node_instruction_205(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 205: Terminal matched constructor ID 190");
    190
}

fn match_node_instruction_206(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 206: Terminal matched constructor ID 43");
    43
}

fn match_node_instruction_207(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 207: Terminal matched constructor ID 43");
    43
}

fn match_node_instruction_208(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 208: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_209(bytes, ctx),
        1 => match_node_instruction_214(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_209(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 209: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_210(bytes, ctx),
        1 => match_node_instruction_213(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_210(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 210: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_211(bytes, ctx),
        1 => match_node_instruction_212(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_211(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 211: Terminal matched constructor ID 49");
    49
}

fn match_node_instruction_212(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 212: Terminal matched constructor ID 191");
    191
}

fn match_node_instruction_213(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 213: Terminal matched constructor ID 47");
    47
}

fn match_node_instruction_214(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 214: Terminal matched constructor ID 47");
    47
}

fn match_node_instruction_215(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 215: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_216(bytes, ctx),
        1 => match_node_instruction_221(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_216(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 216: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_217(bytes, ctx),
        1 => match_node_instruction_220(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_217(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 217: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_218(bytes, ctx),
        1 => match_node_instruction_219(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_218(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 218: Terminal matched constructor ID 66");
    66
}

fn match_node_instruction_219(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 219: Terminal matched constructor ID 192");
    192
}

fn match_node_instruction_220(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 220: Terminal matched constructor ID 64");
    64
}

fn match_node_instruction_221(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 221: Terminal matched constructor ID 64");
    64
}

fn match_node_instruction_222(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 222: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_223(bytes, ctx),
        1 => match_node_instruction_226(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_223(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 223: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_224(bytes, ctx),
        1 => match_node_instruction_225(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_224(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 224: Terminal matched constructor ID 156");
    156
}

fn match_node_instruction_225(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 225: Terminal matched constructor ID 156");
    156
}

fn match_node_instruction_226(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 226: Terminal matched constructor ID 156");
    156
}

fn match_node_instruction_227(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 227: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_228(bytes, ctx),
        1 => match_node_instruction_233(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_228(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 228: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_229(bytes, ctx),
        1 => match_node_instruction_232(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_229(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 229: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_230(bytes, ctx),
        1 => match_node_instruction_231(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_230(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 230: Terminal matched constructor ID 158");
    158
}

fn match_node_instruction_231(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 231: Terminal matched constructor ID 166");
    166
}

fn match_node_instruction_232(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 232: Terminal matched constructor ID 158");
    158
}

fn match_node_instruction_233(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 233: Terminal matched constructor ID 158");
    158
}

fn match_node_instruction_234(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 234: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_235(bytes, ctx),
        1 => match_node_instruction_240(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_235(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 235: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_236(bytes, ctx),
        1 => match_node_instruction_239(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_236(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 236: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_237(bytes, ctx),
        1 => match_node_instruction_238(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_237(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 237: Terminal matched constructor ID 157");
    157
}

fn match_node_instruction_238(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 238: Terminal matched constructor ID 165");
    165
}

fn match_node_instruction_239(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 239: Terminal matched constructor ID 157");
    157
}

fn match_node_instruction_240(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 240: Terminal matched constructor ID 157");
    157
}

fn match_node_instruction_241(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 7;
    eprintln!("Trace node 241: SlaInstructionBits start=7, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_242(bytes, ctx),
        1 => match_node_instruction_277(bytes, ctx),
        2 => match_node_instruction_338(bytes, ctx),
        3 => match_node_instruction_377(bytes, ctx),
        4 => match_node_instruction_420(bytes, ctx),
        5 => match_node_instruction_425(bytes, ctx),
        6 => match_node_instruction_426(bytes, ctx),
        7 => match_node_instruction_431(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_242(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 7;
    eprintln!("Trace node 242: SlaInstructionBits start=4, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_243(bytes, ctx),
        1 => match_node_instruction_248(bytes, ctx),
        2 => match_node_instruction_253(bytes, ctx),
        3 => match_node_instruction_258(bytes, ctx),
        4 => match_node_instruction_263(bytes, ctx),
        5 => match_node_instruction_270(bytes, ctx),
        6 => match_node_instruction_271(bytes, ctx),
        7 => match_node_instruction_276(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_243(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 243: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_244(bytes, ctx),
        1 => match_node_instruction_247(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_244(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 244: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_245(bytes, ctx),
        1 => match_node_instruction_246(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_245(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 245: Terminal matched constructor ID 209");
    209
}

fn match_node_instruction_246(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 246: Terminal matched constructor ID 209");
    209
}

fn match_node_instruction_247(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 247: Terminal matched constructor ID 209");
    209
}

fn match_node_instruction_248(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 248: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_249(bytes, ctx),
        1 => match_node_instruction_252(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_249(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 249: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_250(bytes, ctx),
        1 => match_node_instruction_251(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_250(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 250: Terminal matched constructor ID 97");
    97
}

fn match_node_instruction_251(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 251: Terminal matched constructor ID 97");
    97
}

fn match_node_instruction_252(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 252: Terminal matched constructor ID 97");
    97
}

fn match_node_instruction_253(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 253: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_254(bytes, ctx),
        1 => match_node_instruction_257(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_254(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 254: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_255(bytes, ctx),
        1 => match_node_instruction_256(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_255(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 255: Terminal matched constructor ID 206");
    206
}

fn match_node_instruction_256(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 256: Terminal matched constructor ID 206");
    206
}

fn match_node_instruction_257(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 257: Terminal matched constructor ID 206");
    206
}

fn match_node_instruction_258(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 258: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_259(bytes, ctx),
        1 => match_node_instruction_262(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_259(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 259: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_260(bytes, ctx),
        1 => match_node_instruction_261(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_260(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 260: Terminal matched constructor ID 213");
    213
}

fn match_node_instruction_261(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 261: Terminal matched constructor ID 213");
    213
}

fn match_node_instruction_262(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 262: Terminal matched constructor ID 213");
    213
}

fn match_node_instruction_263(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 263: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_264(bytes, ctx),
        1 => match_node_instruction_269(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_264(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 264: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_265(bytes, ctx),
        1 => match_node_instruction_268(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_265(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 265: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_266(bytes, ctx),
        1 => match_node_instruction_267(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_266(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 266: Terminal matched constructor ID 205");
    205
}

fn match_node_instruction_267(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 267: Terminal matched constructor ID 147");
    147
}

fn match_node_instruction_268(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 268: Terminal matched constructor ID 205");
    205
}

fn match_node_instruction_269(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 269: Terminal matched constructor ID 205");
    205
}

fn match_node_instruction_270(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 270: Terminal matched constructor ID 287");
    287
}

fn match_node_instruction_271(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 271: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_272(bytes, ctx),
        1 => match_node_instruction_275(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_272(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 272: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_273(bytes, ctx),
        1 => match_node_instruction_274(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_273(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 273: Terminal matched constructor ID 204");
    204
}

fn match_node_instruction_274(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 274: Terminal matched constructor ID 204");
    204
}

fn match_node_instruction_275(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 275: Terminal matched constructor ID 204");
    204
}

fn match_node_instruction_276(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 276: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_277(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 7;
    eprintln!("Trace node 277: SlaInstructionBits start=4, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_278(bytes, ctx),
        1 => match_node_instruction_283(bytes, ctx),
        2 => match_node_instruction_288(bytes, ctx),
        3 => match_node_instruction_293(bytes, ctx),
        4 => match_node_instruction_298(bytes, ctx),
        5 => match_node_instruction_307(bytes, ctx),
        6 => match_node_instruction_308(bytes, ctx),
        7 => match_node_instruction_313(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_278(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 278: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_279(bytes, ctx),
        1 => match_node_instruction_282(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_279(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 279: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_280(bytes, ctx),
        1 => match_node_instruction_281(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_280(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 280: Terminal matched constructor ID 210");
    210
}

fn match_node_instruction_281(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 281: Terminal matched constructor ID 210");
    210
}

fn match_node_instruction_282(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 282: Terminal matched constructor ID 210");
    210
}

fn match_node_instruction_283(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 283: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_284(bytes, ctx),
        1 => match_node_instruction_287(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_284(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 284: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_285(bytes, ctx),
        1 => match_node_instruction_286(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_285(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 285: Terminal matched constructor ID 98");
    98
}

fn match_node_instruction_286(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 286: Terminal matched constructor ID 98");
    98
}

fn match_node_instruction_287(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 287: Terminal matched constructor ID 98");
    98
}

fn match_node_instruction_288(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 288: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_289(bytes, ctx),
        1 => match_node_instruction_292(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_289(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 289: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_290(bytes, ctx),
        1 => match_node_instruction_291(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_290(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 290: Terminal matched constructor ID 207");
    207
}

fn match_node_instruction_291(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 291: Terminal matched constructor ID 207");
    207
}

fn match_node_instruction_292(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 292: Terminal matched constructor ID 207");
    207
}

fn match_node_instruction_293(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 293: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_294(bytes, ctx),
        1 => match_node_instruction_297(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_294(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 294: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_295(bytes, ctx),
        1 => match_node_instruction_296(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_295(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 295: Terminal matched constructor ID 214");
    214
}

fn match_node_instruction_296(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 296: Terminal matched constructor ID 214");
    214
}

fn match_node_instruction_297(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 297: Terminal matched constructor ID 214");
    214
}

fn match_node_instruction_298(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 7;
    eprintln!("Trace node 298: SlaInstructionBits start=10, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_299(bytes, ctx),
        1 => match_node_instruction_300(bytes, ctx),
        2 => match_node_instruction_301(bytes, ctx),
        3 => match_node_instruction_302(bytes, ctx),
        4 => match_node_instruction_303(bytes, ctx),
        5 => match_node_instruction_304(bytes, ctx),
        6 => match_node_instruction_305(bytes, ctx),
        7 => match_node_instruction_306(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_299(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 299: Terminal matched constructor ID 281");
    281
}

fn match_node_instruction_300(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 300: Terminal matched constructor ID 60");
    60
}

fn match_node_instruction_301(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 301: Terminal matched constructor ID 229");
    229
}

fn match_node_instruction_302(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 302: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_303(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 303: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_304(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 304: Terminal matched constructor ID 229");
    229
}

fn match_node_instruction_305(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 305: Terminal matched constructor ID 229");
    229
}

fn match_node_instruction_306(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 306: Terminal matched constructor ID 229");
    229
}

fn match_node_instruction_307(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 307: Terminal matched constructor ID 288");
    288
}

fn match_node_instruction_308(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 308: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_309(bytes, ctx),
        1 => match_node_instruction_312(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_309(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 309: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_310(bytes, ctx),
        1 => match_node_instruction_311(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_310(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 310: Terminal matched constructor ID 114");
    114
}

fn match_node_instruction_311(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 311: Terminal matched constructor ID 114");
    114
}

fn match_node_instruction_312(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 312: Terminal matched constructor ID 114");
    114
}

fn match_node_instruction_313(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 313: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_314(bytes, ctx),
        1 => match_node_instruction_315(bytes, ctx),
        2 => match_node_instruction_318(bytes, ctx),
        3 => match_node_instruction_321(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_314(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 314: Terminal matched constructor ID 282");
    282
}

fn match_node_instruction_315(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 315: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_316(bytes, ctx),
        1 => match_node_instruction_317(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_316(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 316: Terminal matched constructor ID 146");
    146
}

fn match_node_instruction_317(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 317: Terminal matched constructor ID 291");
    291
}

fn match_node_instruction_318(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 318: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_319(bytes, ctx),
        1 => match_node_instruction_320(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_319(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 319: Terminal matched constructor ID 164");
    164
}

fn match_node_instruction_320(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 320: Terminal matched constructor ID 163");
    163
}

fn match_node_instruction_321(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 15;
    eprintln!("Trace node 321: SlaInstructionBits start=12, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_322(bytes, ctx),
        1 => match_node_instruction_323(bytes, ctx),
        2 => match_node_instruction_324(bytes, ctx),
        3 => match_node_instruction_325(bytes, ctx),
        4 => match_node_instruction_326(bytes, ctx),
        5 => match_node_instruction_327(bytes, ctx),
        6 => match_node_instruction_328(bytes, ctx),
        7 => match_node_instruction_329(bytes, ctx),
        8 => match_node_instruction_330(bytes, ctx),
        9 => match_node_instruction_331(bytes, ctx),
        10 => match_node_instruction_332(bytes, ctx),
        11 => match_node_instruction_333(bytes, ctx),
        12 => match_node_instruction_334(bytes, ctx),
        13 => match_node_instruction_335(bytes, ctx),
        14 => match_node_instruction_336(bytes, ctx),
        15 => match_node_instruction_337(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_322(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 322: Terminal matched constructor ID 236");
    236
}

fn match_node_instruction_323(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 323: Terminal matched constructor ID 212");
    212
}

fn match_node_instruction_324(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 324: Terminal matched constructor ID 261");
    261
}

fn match_node_instruction_325(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 325: Terminal matched constructor ID 254");
    254
}

fn match_node_instruction_326(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 326: Terminal matched constructor ID 253");
    253
}

fn match_node_instruction_327(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 327: Terminal matched constructor ID 258");
    258
}

fn match_node_instruction_328(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 328: Terminal matched constructor ID 286");
    286
}

fn match_node_instruction_329(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 329: Terminal matched constructor ID 257");
    257
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
    eprintln!("Trace node 332: Terminal matched constructor ID 167");
    167
}

fn match_node_instruction_333(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 333: Terminal matched constructor ID 168");
    168
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
    let byte_cnt = (4 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 7;
    eprintln!("Trace node 338: SlaInstructionBits start=4, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_339(bytes, ctx),
        1 => match_node_instruction_344(bytes, ctx),
        2 => match_node_instruction_349(bytes, ctx),
        3 => match_node_instruction_354(bytes, ctx),
        4 => match_node_instruction_359(bytes, ctx),
        5 => match_node_instruction_362(bytes, ctx),
        6 => match_node_instruction_363(bytes, ctx),
        7 => match_node_instruction_364(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_339(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 339: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_340(bytes, ctx),
        1 => match_node_instruction_343(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_340(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 340: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_341(bytes, ctx),
        1 => match_node_instruction_342(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_341(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 341: Terminal matched constructor ID 211");
    211
}

fn match_node_instruction_342(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 342: Terminal matched constructor ID 211");
    211
}

fn match_node_instruction_343(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 343: Terminal matched constructor ID 211");
    211
}

fn match_node_instruction_344(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 344: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_345(bytes, ctx),
        1 => match_node_instruction_348(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_345(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 345: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_346(bytes, ctx),
        1 => match_node_instruction_347(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_346(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 346: Terminal matched constructor ID 99");
    99
}

fn match_node_instruction_347(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 347: Terminal matched constructor ID 99");
    99
}

fn match_node_instruction_348(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 348: Terminal matched constructor ID 99");
    99
}

fn match_node_instruction_349(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 349: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_350(bytes, ctx),
        1 => match_node_instruction_353(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_350(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 350: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_351(bytes, ctx),
        1 => match_node_instruction_352(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_351(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 351: Terminal matched constructor ID 208");
    208
}

fn match_node_instruction_352(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 352: Terminal matched constructor ID 208");
    208
}

fn match_node_instruction_353(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 353: Terminal matched constructor ID 208");
    208
}

fn match_node_instruction_354(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 354: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_355(bytes, ctx),
        1 => match_node_instruction_358(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_355(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 355: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_356(bytes, ctx),
        1 => match_node_instruction_357(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_356(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 356: Terminal matched constructor ID 215");
    215
}

fn match_node_instruction_357(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 357: Terminal matched constructor ID 215");
    215
}

fn match_node_instruction_358(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 358: Terminal matched constructor ID 215");
    215
}

fn match_node_instruction_359(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 359: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_360(bytes, ctx),
        1 => match_node_instruction_361(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_360(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 360: Terminal matched constructor ID 126");
    126
}

fn match_node_instruction_361(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 361: Terminal matched constructor ID 184");
    184
}

fn match_node_instruction_362(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 362: Terminal matched constructor ID 289");
    289
}

fn match_node_instruction_363(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 363: Terminal matched constructor ID 179");
    179
}

fn match_node_instruction_364(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 7;
    eprintln!("Trace node 364: SlaInstructionBits start=10, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_365(bytes, ctx),
        1 => match_node_instruction_366(bytes, ctx),
        2 => match_node_instruction_367(bytes, ctx),
        3 => match_node_instruction_368(bytes, ctx),
        4 => match_node_instruction_369(bytes, ctx),
        5 => match_node_instruction_370(bytes, ctx),
        6 => match_node_instruction_371(bytes, ctx),
        7 => match_node_instruction_372(bytes, ctx),
        _ => -1,
    }
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
    eprintln!("Trace node 367: Terminal matched constructor ID 138");
    138
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
    eprintln!("Trace node 370: Terminal matched constructor ID 139");
    139
}

fn match_node_instruction_371(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 371: Terminal matched constructor ID 140");
    140
}

fn match_node_instruction_372(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 3;
    eprintln!("Trace node 372: SlaInstructionBits start=14, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_373(bytes, ctx),
        1 => match_node_instruction_374(bytes, ctx),
        2 => match_node_instruction_375(bytes, ctx),
        3 => match_node_instruction_376(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_373(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 373: Terminal matched constructor ID 143");
    143
}

fn match_node_instruction_374(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 374: Terminal matched constructor ID 144");
    144
}

fn match_node_instruction_375(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 375: Terminal matched constructor ID 141");
    141
}

fn match_node_instruction_376(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 376: Terminal matched constructor ID 142");
    142
}

fn match_node_instruction_377(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 7;
    eprintln!("Trace node 377: SlaInstructionBits start=4, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_378(bytes, ctx),
        1 => match_node_instruction_383(bytes, ctx),
        2 => match_node_instruction_388(bytes, ctx),
        3 => match_node_instruction_393(bytes, ctx),
        4 => match_node_instruction_398(bytes, ctx),
        5 => match_node_instruction_401(bytes, ctx),
        6 => match_node_instruction_406(bytes, ctx),
        7 => match_node_instruction_407(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_378(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 378: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_379(bytes, ctx),
        1 => match_node_instruction_382(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_379(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 379: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_380(bytes, ctx),
        1 => match_node_instruction_381(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_380(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 380: Terminal matched constructor ID 161");
    161
}

fn match_node_instruction_381(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 381: Terminal matched constructor ID 161");
    161
}

fn match_node_instruction_382(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 382: Terminal matched constructor ID 161");
    161
}

fn match_node_instruction_383(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 383: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_384(bytes, ctx),
        1 => match_node_instruction_387(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_384(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 384: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_385(bytes, ctx),
        1 => match_node_instruction_386(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_385(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 385: Terminal matched constructor ID 159");
    159
}

fn match_node_instruction_386(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 386: Terminal matched constructor ID 159");
    159
}

fn match_node_instruction_387(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 387: Terminal matched constructor ID 159");
    159
}

fn match_node_instruction_388(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 388: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_389(bytes, ctx),
        1 => match_node_instruction_392(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_389(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 389: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_390(bytes, ctx),
        1 => match_node_instruction_391(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_390(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 390: Terminal matched constructor ID 160");
    160
}

fn match_node_instruction_391(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 391: Terminal matched constructor ID 160");
    160
}

fn match_node_instruction_392(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 392: Terminal matched constructor ID 160");
    160
}

fn match_node_instruction_393(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 393: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_394(bytes, ctx),
        1 => match_node_instruction_397(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_394(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 394: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_395(bytes, ctx),
        1 => match_node_instruction_396(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_395(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 395: Terminal matched constructor ID 162");
    162
}

fn match_node_instruction_396(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 396: Terminal matched constructor ID 162");
    162
}

fn match_node_instruction_397(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 397: Terminal matched constructor ID 162");
    162
}

fn match_node_instruction_398(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 398: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_399(bytes, ctx),
        1 => match_node_instruction_400(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_399(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 399: Terminal matched constructor ID 127");
    127
}

fn match_node_instruction_400(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 400: Terminal matched constructor ID 186");
    186
}

fn match_node_instruction_401(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 401: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_402(bytes, ctx),
        1 => match_node_instruction_405(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_402(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 402: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_403(bytes, ctx),
        1 => match_node_instruction_404(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_403(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 403: Terminal matched constructor ID 129");
    129
}

fn match_node_instruction_404(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 404: Terminal matched constructor ID 290");
    290
}

fn match_node_instruction_405(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 405: Terminal matched constructor ID 130");
    130
}

fn match_node_instruction_406(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 406: Terminal matched constructor ID 188");
    188
}

fn match_node_instruction_407(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 7;
    eprintln!("Trace node 407: SlaInstructionBits start=10, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_408(bytes, ctx),
        1 => match_node_instruction_409(bytes, ctx),
        2 => match_node_instruction_410(bytes, ctx),
        3 => match_node_instruction_411(bytes, ctx),
        4 => match_node_instruction_412(bytes, ctx),
        5 => match_node_instruction_413(bytes, ctx),
        6 => match_node_instruction_414(bytes, ctx),
        7 => match_node_instruction_415(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_408(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 408: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_409(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 409: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_410(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 410: Terminal matched constructor ID 131");
    131
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
    eprintln!("Trace node 413: Terminal matched constructor ID 132");
    132
}

fn match_node_instruction_414(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 414: Terminal matched constructor ID 133");
    133
}

fn match_node_instruction_415(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 3;
    eprintln!("Trace node 415: SlaInstructionBits start=14, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_416(bytes, ctx),
        1 => match_node_instruction_417(bytes, ctx),
        2 => match_node_instruction_418(bytes, ctx),
        3 => match_node_instruction_419(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_416(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 416: Terminal matched constructor ID 136");
    136
}

fn match_node_instruction_417(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 417: Terminal matched constructor ID 137");
    137
}

fn match_node_instruction_418(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 418: Terminal matched constructor ID 134");
    134
}

fn match_node_instruction_419(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 419: Terminal matched constructor ID 135");
    135
}

fn match_node_instruction_420(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 420: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_421(bytes, ctx),
        1 => match_node_instruction_424(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_421(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 421: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_422(bytes, ctx),
        1 => match_node_instruction_423(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_422(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 422: Terminal matched constructor ID 84");
    84
}

fn match_node_instruction_423(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 423: Terminal matched constructor ID 84");
    84
}

fn match_node_instruction_424(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 424: Terminal matched constructor ID 84");
    84
}

fn match_node_instruction_425(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 425: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_426(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 426: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_427(bytes, ctx),
        1 => match_node_instruction_430(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_427(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 427: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_428(bytes, ctx),
        1 => match_node_instruction_429(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_428(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 428: Terminal matched constructor ID 83");
    83
}

fn match_node_instruction_429(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 429: Terminal matched constructor ID 83");
    83
}

fn match_node_instruction_430(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 430: Terminal matched constructor ID 83");
    83
}

fn match_node_instruction_431(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 431: Terminal matched constructor ID 128");
    128
}

fn match_node_instruction_432(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 3;
    eprintln!("Trace node 432: SlaInstructionBits start=8, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_433(bytes, ctx),
        1 => match_node_instruction_436(bytes, ctx),
        2 => match_node_instruction_439(bytes, ctx),
        3 => match_node_instruction_442(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_433(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 433: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_434(bytes, ctx),
        1 => match_node_instruction_435(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_434(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 434: Terminal matched constructor ID 13");
    13
}

fn match_node_instruction_435(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 435: Terminal matched constructor ID 273");
    273
}

fn match_node_instruction_436(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 436: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_437(bytes, ctx),
        1 => match_node_instruction_438(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_437(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 437: Terminal matched constructor ID 16");
    16
}

fn match_node_instruction_438(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 438: Terminal matched constructor ID 276");
    276
}

fn match_node_instruction_439(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 439: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_440(bytes, ctx),
        1 => match_node_instruction_441(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_440(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 440: Terminal matched constructor ID 17");
    17
}

fn match_node_instruction_441(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 441: Terminal matched constructor ID 277");
    277
}

fn match_node_instruction_442(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 442: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_443(bytes, ctx),
        1 => match_node_instruction_448(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_443(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 443: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_444(bytes, ctx),
        1 => match_node_instruction_447(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_444(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 444: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_445(bytes, ctx),
        1 => match_node_instruction_446(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_445(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 445: Terminal matched constructor ID 260");
    260
}

fn match_node_instruction_446(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 446: Terminal matched constructor ID 111");
    111
}

fn match_node_instruction_447(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 447: Terminal matched constructor ID 260");
    260
}

fn match_node_instruction_448(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 3;
    eprintln!("Trace node 448: SlaInstructionBits start=14, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_449(bytes, ctx),
        1 => match_node_instruction_450(bytes, ctx),
        2 => match_node_instruction_451(bytes, ctx),
        3 => match_node_instruction_452(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_449(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 449: Terminal matched constructor ID 180");
    180
}

fn match_node_instruction_450(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 450: Terminal matched constructor ID 260");
    260
}

fn match_node_instruction_451(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 451: Terminal matched constructor ID 181");
    181
}

fn match_node_instruction_452(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 452: Terminal matched constructor ID 182");
    182
}

fn match_node_instruction_453(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 453: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_454(bytes, ctx),
        1 => match_node_instruction_457(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_454(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 454: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_455(bytes, ctx),
        1 => match_node_instruction_456(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_455(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 455: Terminal matched constructor ID 62");
    62
}

fn match_node_instruction_456(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 456: Terminal matched constructor ID 63");
    63
}

fn match_node_instruction_457(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 457: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_458(bytes, ctx),
        1 => match_node_instruction_459(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_458(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 458: Terminal matched constructor ID 69");
    69
}

fn match_node_instruction_459(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 459: Terminal matched constructor ID 70");
    70
}

fn match_node_instruction_460(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 460: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_461(bytes, ctx),
        1 => match_node_instruction_462(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_461(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 461: Terminal matched constructor ID 193");
    193
}

fn match_node_instruction_462(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 3;
    eprintln!("Trace node 462: SlaInstructionBits start=8, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_463(bytes, ctx),
        1 => match_node_instruction_464(bytes, ctx),
        2 => match_node_instruction_465(bytes, ctx),
        3 => match_node_instruction_466(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_463(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 463: Terminal matched constructor ID 174");
    174
}

fn match_node_instruction_464(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 464: Terminal matched constructor ID 175");
    175
}

fn match_node_instruction_465(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 465: Terminal matched constructor ID 176");
    176
}

fn match_node_instruction_466(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 466: Terminal matched constructor ID 177");
    177
}

fn match_node_instruction_467(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 7;
    eprintln!("Trace node 467: SlaInstructionBits start=7, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_468(bytes, ctx),
        1 => match_node_instruction_473(bytes, ctx),
        2 => match_node_instruction_478(bytes, ctx),
        3 => match_node_instruction_483(bytes, ctx),
        4 => match_node_instruction_484(bytes, ctx),
        5 => match_node_instruction_489(bytes, ctx),
        6 => match_node_instruction_496(bytes, ctx),
        7 => match_node_instruction_503(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_468(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 468: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_469(bytes, ctx),
        1 => match_node_instruction_472(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_469(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 469: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_470(bytes, ctx),
        1 => match_node_instruction_471(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_470(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 470: Terminal matched constructor ID 216");
    216
}

fn match_node_instruction_471(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 471: Terminal matched constructor ID 216");
    216
}

fn match_node_instruction_472(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 472: Terminal matched constructor ID 216");
    216
}

fn match_node_instruction_473(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 473: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_474(bytes, ctx),
        1 => match_node_instruction_477(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_474(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 474: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_475(bytes, ctx),
        1 => match_node_instruction_476(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_475(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 475: Terminal matched constructor ID 217");
    217
}

fn match_node_instruction_476(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 476: Terminal matched constructor ID 217");
    217
}

fn match_node_instruction_477(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 477: Terminal matched constructor ID 217");
    217
}

fn match_node_instruction_478(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 478: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_479(bytes, ctx),
        1 => match_node_instruction_482(bytes, ctx),
        _ => -1,
    }
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
    eprintln!("Trace node 480: Terminal matched constructor ID 218");
    218
}

fn match_node_instruction_481(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 481: Terminal matched constructor ID 218");
    218
}

fn match_node_instruction_482(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 482: Terminal matched constructor ID 218");
    218
}

fn match_node_instruction_483(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 483: Terminal matched constructor ID 113");
    113
}

fn match_node_instruction_484(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 484: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_485(bytes, ctx),
        1 => match_node_instruction_488(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_485(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 485: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_486(bytes, ctx),
        1 => match_node_instruction_487(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_486(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 486: Terminal matched constructor ID 259");
    259
}

fn match_node_instruction_487(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 487: Terminal matched constructor ID 219");
    219
}

fn match_node_instruction_488(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 488: Terminal matched constructor ID 219");
    219
}

fn match_node_instruction_489(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 489: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_490(bytes, ctx),
        1 => match_node_instruction_495(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_490(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 490: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_491(bytes, ctx),
        1 => match_node_instruction_494(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_491(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 491: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_492(bytes, ctx),
        1 => match_node_instruction_493(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_492(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 492: Terminal matched constructor ID 227");
    227
}

fn match_node_instruction_493(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 493: Terminal matched constructor ID 228");
    228
}

fn match_node_instruction_494(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 494: Terminal matched constructor ID 220");
    220
}

fn match_node_instruction_495(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 495: Terminal matched constructor ID 220");
    220
}

fn match_node_instruction_496(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 496: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_497(bytes, ctx),
        1 => match_node_instruction_502(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_497(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 497: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_498(bytes, ctx),
        1 => match_node_instruction_501(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_498(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 498: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_499(bytes, ctx),
        1 => match_node_instruction_500(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_499(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 499: Terminal matched constructor ID 292");
    292
}

fn match_node_instruction_500(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 500: Terminal matched constructor ID 293");
    293
}

fn match_node_instruction_501(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 501: Terminal matched constructor ID 221");
    221
}

fn match_node_instruction_502(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 502: Terminal matched constructor ID 221");
    221
}

fn match_node_instruction_503(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 503: Terminal matched constructor ID 112");
    112
}

fn match_node_instruction_504(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 7;
    eprintln!("Trace node 504: SlaInstructionBits start=7, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_505(bytes, ctx),
        1 => match_node_instruction_506(bytes, ctx),
        2 => match_node_instruction_507(bytes, ctx),
        3 => match_node_instruction_508(bytes, ctx),
        4 => match_node_instruction_509(bytes, ctx),
        5 => match_node_instruction_514(bytes, ctx),
        6 => match_node_instruction_519(bytes, ctx),
        7 => match_node_instruction_524(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_505(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 505: Terminal matched constructor ID 262");
    262
}

fn match_node_instruction_506(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 506: Terminal matched constructor ID 263");
    263
}

fn match_node_instruction_507(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 507: Terminal matched constructor ID 264");
    264
}

fn match_node_instruction_508(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 508: Terminal matched constructor ID 268");
    268
}

fn match_node_instruction_509(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 509: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_510(bytes, ctx),
        1 => match_node_instruction_513(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_510(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 510: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_511(bytes, ctx),
        1 => match_node_instruction_512(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_511(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 511: Terminal matched constructor ID 278");
    278
}

fn match_node_instruction_512(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 512: Terminal matched constructor ID 265");
    265
}

fn match_node_instruction_513(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 513: Terminal matched constructor ID 265");
    265
}

fn match_node_instruction_514(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 514: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_515(bytes, ctx),
        1 => match_node_instruction_518(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_515(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 515: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_516(bytes, ctx),
        1 => match_node_instruction_517(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_516(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 516: Terminal matched constructor ID 279");
    279
}

fn match_node_instruction_517(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 517: Terminal matched constructor ID 266");
    266
}

fn match_node_instruction_518(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 518: Terminal matched constructor ID 266");
    266
}

fn match_node_instruction_519(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 519: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_520(bytes, ctx),
        1 => match_node_instruction_523(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_520(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 520: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_521(bytes, ctx),
        1 => match_node_instruction_522(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_521(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 521: Terminal matched constructor ID 280");
    280
}

fn match_node_instruction_522(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 522: Terminal matched constructor ID 267");
    267
}

fn match_node_instruction_523(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 523: Terminal matched constructor ID 267");
    267
}

fn match_node_instruction_524(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 524: Terminal matched constructor ID 269");
    269
}

fn match_node_instruction_525(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 525: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_526(bytes, ctx),
        1 => match_node_instruction_549(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_526(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (23 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 23) & 1;
    eprintln!("Trace node 526: SlaInstructionBits start=23, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_527(bytes, ctx),
        1 => match_node_instruction_538(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_527(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 527: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_528(bytes, ctx),
        1 => match_node_instruction_531(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_528(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (30 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 30) & 1;
    eprintln!("Trace node 528: SlaInstructionBits start=30, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_529(bytes, ctx),
        1 => match_node_instruction_530(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_529(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 529: Terminal matched constructor ID 426");
    426
}

fn match_node_instruction_530(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 530: Terminal matched constructor ID 434");
    434
}

fn match_node_instruction_531(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 531: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_532(bytes, ctx),
        1 => match_node_instruction_535(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_532(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (30 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 30) & 1;
    eprintln!("Trace node 532: SlaInstructionBits start=30, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_533(bytes, ctx),
        1 => match_node_instruction_534(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_533(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 533: Terminal matched constructor ID 424");
    424
}

fn match_node_instruction_534(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 534: Terminal matched constructor ID 432");
    432
}

fn match_node_instruction_535(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (30 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 30) & 1;
    eprintln!("Trace node 535: SlaInstructionBits start=30, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_536(bytes, ctx),
        1 => match_node_instruction_537(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_536(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 536: Terminal matched constructor ID 425");
    425
}

fn match_node_instruction_537(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 537: Terminal matched constructor ID 433");
    433
}

fn match_node_instruction_538(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 538: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_539(bytes, ctx),
        1 => match_node_instruction_542(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_539(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (30 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 30) & 1;
    eprintln!("Trace node 539: SlaInstructionBits start=30, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_540(bytes, ctx),
        1 => match_node_instruction_541(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_540(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 540: Terminal matched constructor ID 449");
    449
}

fn match_node_instruction_541(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 541: Terminal matched constructor ID 452");
    452
}

fn match_node_instruction_542(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 542: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_543(bytes, ctx),
        1 => match_node_instruction_546(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_543(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (30 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 30) & 1;
    eprintln!("Trace node 543: SlaInstructionBits start=30, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_544(bytes, ctx),
        1 => match_node_instruction_545(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_544(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 544: Terminal matched constructor ID 447");
    447
}

fn match_node_instruction_545(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 545: Terminal matched constructor ID 455");
    455
}

fn match_node_instruction_546(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (30 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 30) & 1;
    eprintln!("Trace node 546: SlaInstructionBits start=30, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_547(bytes, ctx),
        1 => match_node_instruction_548(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_547(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 547: Terminal matched constructor ID 448");
    448
}

fn match_node_instruction_548(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 548: Terminal matched constructor ID 456");
    456
}

fn match_node_instruction_549(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 3;
    eprintln!("Trace node 549: SlaInstructionBits start=8, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_550(bytes, ctx),
        1 => match_node_instruction_575(bytes, ctx),
        2 => match_node_instruction_576(bytes, ctx),
        3 => match_node_instruction_583(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_550(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 550: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_551(bytes, ctx),
        1 => match_node_instruction_566(bytes, ctx),
        2 => match_node_instruction_567(bytes, ctx),
        3 => match_node_instruction_568(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_551(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 551: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_552(bytes, ctx),
        1 => match_node_instruction_559(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_552(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 552: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_553(bytes, ctx),
        1 => match_node_instruction_554(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_553(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 553: Terminal matched constructor ID 438");
    438
}

fn match_node_instruction_554(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 3;
    eprintln!("Trace node 554: SlaInstructionBits start=5, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_555(bytes, ctx),
        1 => match_node_instruction_556(bytes, ctx),
        2 => match_node_instruction_557(bytes, ctx),
        3 => match_node_instruction_558(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_555(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 555: Terminal matched constructor ID 445");
    445
}

fn match_node_instruction_556(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 556: Terminal matched constructor ID 436");
    436
}

fn match_node_instruction_557(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 557: Terminal matched constructor ID 446");
    446
}

fn match_node_instruction_558(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 558: Terminal matched constructor ID 437");
    437
}

fn match_node_instruction_559(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 559: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_560(bytes, ctx),
        1 => match_node_instruction_561(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_560(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 560: Terminal matched constructor ID 438");
    438
}

fn match_node_instruction_561(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 3;
    eprintln!("Trace node 561: SlaInstructionBits start=5, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_562(bytes, ctx),
        1 => match_node_instruction_563(bytes, ctx),
        2 => match_node_instruction_564(bytes, ctx),
        3 => match_node_instruction_565(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_562(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 562: Terminal matched constructor ID 445");
    445
}

fn match_node_instruction_563(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 563: Terminal matched constructor ID 436");
    436
}

fn match_node_instruction_564(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 564: Terminal matched constructor ID 446");
    446
}

fn match_node_instruction_565(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 565: Terminal matched constructor ID 437");
    437
}

fn match_node_instruction_566(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 566: Terminal matched constructor ID 439");
    439
}

fn match_node_instruction_567(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 567: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_568(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 568: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_569(bytes, ctx),
        1 => match_node_instruction_570(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_569(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 569: Terminal matched constructor ID 438");
    438
}

fn match_node_instruction_570(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 3;
    eprintln!("Trace node 570: SlaInstructionBits start=5, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_571(bytes, ctx),
        1 => match_node_instruction_572(bytes, ctx),
        2 => match_node_instruction_573(bytes, ctx),
        3 => match_node_instruction_574(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_571(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 571: Terminal matched constructor ID 445");
    445
}

fn match_node_instruction_572(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 572: Terminal matched constructor ID 436");
    436
}

fn match_node_instruction_573(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 573: Terminal matched constructor ID 446");
    446
}

fn match_node_instruction_574(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 574: Terminal matched constructor ID 437");
    437
}

fn match_node_instruction_575(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 575: Terminal matched constructor ID 178");
    178
}

fn match_node_instruction_576(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 576: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_577(bytes, ctx),
        1 => match_node_instruction_578(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_577(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 577: Terminal matched constructor ID 435");
    435
}

fn match_node_instruction_578(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 3;
    eprintln!("Trace node 578: SlaInstructionBits start=5, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_579(bytes, ctx),
        1 => match_node_instruction_580(bytes, ctx),
        2 => match_node_instruction_581(bytes, ctx),
        3 => match_node_instruction_582(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_579(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 579: Terminal matched constructor ID 440");
    440
}

fn match_node_instruction_580(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 580: Terminal matched constructor ID 441");
    441
}

fn match_node_instruction_581(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 581: Terminal matched constructor ID 443");
    443
}

fn match_node_instruction_582(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 582: Terminal matched constructor ID 442");
    442
}

fn match_node_instruction_583(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 583: Terminal matched constructor ID 444");
    444
}

fn match_node_instruction_584(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 7;
    eprintln!("Trace node 584: SlaInstructionBits start=7, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_585(bytes, ctx),
        1 => match_node_instruction_586(bytes, ctx),
        2 => match_node_instruction_587(bytes, ctx),
        3 => match_node_instruction_588(bytes, ctx),
        4 => match_node_instruction_589(bytes, ctx),
        5 => match_node_instruction_596(bytes, ctx),
        6 => match_node_instruction_603(bytes, ctx),
        7 => match_node_instruction_610(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_585(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 585: Terminal matched constructor ID 100");
    100
}

fn match_node_instruction_586(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 586: Terminal matched constructor ID 101");
    101
}

fn match_node_instruction_587(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 587: Terminal matched constructor ID 102");
    102
}

fn match_node_instruction_588(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 588: Terminal matched constructor ID 103");
    103
}

fn match_node_instruction_589(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 589: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_590(bytes, ctx),
        1 => match_node_instruction_595(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_590(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 590: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_591(bytes, ctx),
        1 => match_node_instruction_594(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_591(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 591: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_592(bytes, ctx),
        1 => match_node_instruction_593(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_592(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 592: Terminal matched constructor ID 115");
    115
}

fn match_node_instruction_593(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 593: Terminal matched constructor ID 108");
    108
}

fn match_node_instruction_594(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 594: Terminal matched constructor ID 115");
    115
}

fn match_node_instruction_595(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 595: Terminal matched constructor ID 115");
    115
}

fn match_node_instruction_596(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 596: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_597(bytes, ctx),
        1 => match_node_instruction_602(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_597(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 597: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_598(bytes, ctx),
        1 => match_node_instruction_601(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_598(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 598: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_599(bytes, ctx),
        1 => match_node_instruction_600(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_599(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 599: Terminal matched constructor ID 116");
    116
}

fn match_node_instruction_600(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 600: Terminal matched constructor ID 109");
    109
}

fn match_node_instruction_601(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 601: Terminal matched constructor ID 116");
    116
}

fn match_node_instruction_602(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 602: Terminal matched constructor ID 116");
    116
}

fn match_node_instruction_603(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 603: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_604(bytes, ctx),
        1 => match_node_instruction_609(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_604(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 604: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_605(bytes, ctx),
        1 => match_node_instruction_608(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_605(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 605: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_606(bytes, ctx),
        1 => match_node_instruction_607(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_606(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 606: Terminal matched constructor ID 117");
    117
}

fn match_node_instruction_607(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 607: Terminal matched constructor ID 110");
    110
}

fn match_node_instruction_608(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 608: Terminal matched constructor ID 117");
    117
}

fn match_node_instruction_609(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 609: Terminal matched constructor ID 117");
    117
}

fn match_node_instruction_610(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 610: Terminal matched constructor ID 104");
    104
}

fn match_node_instruction_611(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 7;
    eprintln!("Trace node 611: SlaInstructionBits start=7, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_612(bytes, ctx),
        1 => match_node_instruction_617(bytes, ctx),
        2 => match_node_instruction_622(bytes, ctx),
        3 => match_node_instruction_627(bytes, ctx),
        4 => match_node_instruction_628(bytes, ctx),
        5 => match_node_instruction_633(bytes, ctx),
        6 => match_node_instruction_640(bytes, ctx),
        7 => match_node_instruction_645(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_612(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 612: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_613(bytes, ctx),
        1 => match_node_instruction_616(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_613(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 613: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_614(bytes, ctx),
        1 => match_node_instruction_615(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_614(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 614: Terminal matched constructor ID 21");
    21
}

fn match_node_instruction_615(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 615: Terminal matched constructor ID 21");
    21
}

fn match_node_instruction_616(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 616: Terminal matched constructor ID 21");
    21
}

fn match_node_instruction_617(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 617: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_618(bytes, ctx),
        1 => match_node_instruction_621(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_618(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 618: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_619(bytes, ctx),
        1 => match_node_instruction_620(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_619(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 619: Terminal matched constructor ID 22");
    22
}

fn match_node_instruction_620(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 620: Terminal matched constructor ID 22");
    22
}

fn match_node_instruction_621(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 621: Terminal matched constructor ID 22");
    22
}

fn match_node_instruction_622(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 622: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_623(bytes, ctx),
        1 => match_node_instruction_626(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_623(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 623: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_624(bytes, ctx),
        1 => match_node_instruction_625(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_624(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 624: Terminal matched constructor ID 23");
    23
}

fn match_node_instruction_625(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 625: Terminal matched constructor ID 23");
    23
}

fn match_node_instruction_626(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 626: Terminal matched constructor ID 23");
    23
}

fn match_node_instruction_627(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 627: Terminal matched constructor ID 203");
    203
}

fn match_node_instruction_628(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 628: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_629(bytes, ctx),
        1 => match_node_instruction_632(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_629(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 629: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_630(bytes, ctx),
        1 => match_node_instruction_631(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_630(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 630: Terminal matched constructor ID 1");
    1
}

fn match_node_instruction_631(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 631: Terminal matched constructor ID 24");
    24
}

fn match_node_instruction_632(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 632: Terminal matched constructor ID 24");
    24
}

fn match_node_instruction_633(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 633: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_634(bytes, ctx),
        1 => match_node_instruction_639(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_634(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 634: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_635(bytes, ctx),
        1 => match_node_instruction_638(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_635(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 635: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_636(bytes, ctx),
        1 => match_node_instruction_637(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_636(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 636: Terminal matched constructor ID 123");
    123
}

fn match_node_instruction_637(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 637: Terminal matched constructor ID 124");
    124
}

fn match_node_instruction_638(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 638: Terminal matched constructor ID 25");
    25
}

fn match_node_instruction_639(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 639: Terminal matched constructor ID 25");
    25
}

fn match_node_instruction_640(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 640: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_641(bytes, ctx),
        1 => match_node_instruction_644(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_641(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 641: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_642(bytes, ctx),
        1 => match_node_instruction_643(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_642(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 642: Terminal matched constructor ID 125");
    125
}

fn match_node_instruction_643(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 643: Terminal matched constructor ID 26");
    26
}

fn match_node_instruction_644(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 644: Terminal matched constructor ID 26");
    26
}

fn match_node_instruction_645(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 645: Terminal matched constructor ID 202");
    202
}

fn match_node_instruction_646(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 7;
    eprintln!("Trace node 646: SlaInstructionBits start=7, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_647(bytes, ctx),
        1 => match_node_instruction_648(bytes, ctx),
        2 => match_node_instruction_649(bytes, ctx),
        3 => match_node_instruction_650(bytes, ctx),
        4 => match_node_instruction_651(bytes, ctx),
        5 => match_node_instruction_656(bytes, ctx),
        6 => match_node_instruction_661(bytes, ctx),
        7 => match_node_instruction_666(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_647(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 647: Terminal matched constructor ID 2");
    2
}

fn match_node_instruction_648(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 648: Terminal matched constructor ID 3");
    3
}

fn match_node_instruction_649(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 649: Terminal matched constructor ID 4");
    4
}

fn match_node_instruction_650(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 650: Terminal matched constructor ID 8");
    8
}

fn match_node_instruction_651(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 651: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_652(bytes, ctx),
        1 => match_node_instruction_655(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_652(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 652: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_653(bytes, ctx),
        1 => match_node_instruction_654(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_653(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 653: Terminal matched constructor ID 18");
    18
}

fn match_node_instruction_654(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 654: Terminal matched constructor ID 5");
    5
}

fn match_node_instruction_655(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 655: Terminal matched constructor ID 5");
    5
}

fn match_node_instruction_656(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 656: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_657(bytes, ctx),
        1 => match_node_instruction_660(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_657(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 657: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_658(bytes, ctx),
        1 => match_node_instruction_659(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_658(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 658: Terminal matched constructor ID 19");
    19
}

fn match_node_instruction_659(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 659: Terminal matched constructor ID 6");
    6
}

fn match_node_instruction_660(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 660: Terminal matched constructor ID 6");
    6
}

fn match_node_instruction_661(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 661: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_662(bytes, ctx),
        1 => match_node_instruction_665(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_662(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 662: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_663(bytes, ctx),
        1 => match_node_instruction_664(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_663(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 663: Terminal matched constructor ID 20");
    20
}

fn match_node_instruction_664(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 664: Terminal matched constructor ID 7");
    7
}

fn match_node_instruction_665(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 665: Terminal matched constructor ID 7");
    7
}

fn match_node_instruction_666(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 666: Terminal matched constructor ID 9");
    9
}

fn match_node_instruction_667(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 7;
    eprintln!("Trace node 667: SlaInstructionBits start=7, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_668(bytes, ctx),
        1 => match_node_instruction_673(bytes, ctx),
        2 => match_node_instruction_678(bytes, ctx),
        3 => match_node_instruction_683(bytes, ctx),
        4 => match_node_instruction_724(bytes, ctx),
        5 => match_node_instruction_729(bytes, ctx),
        6 => match_node_instruction_734(bytes, ctx),
        7 => match_node_instruction_739(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_668(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 3;
    eprintln!("Trace node 668: SlaInstructionBits start=11, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_669(bytes, ctx),
        1 => match_node_instruction_670(bytes, ctx),
        2 => match_node_instruction_671(bytes, ctx),
        3 => match_node_instruction_672(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_669(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 669: Terminal matched constructor ID 36");
    36
}

fn match_node_instruction_670(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 670: Terminal matched constructor ID 152");
    152
}

fn match_node_instruction_671(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 671: Terminal matched constructor ID 249");
    249
}

fn match_node_instruction_672(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 672: Terminal matched constructor ID 241");
    241
}

fn match_node_instruction_673(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 3;
    eprintln!("Trace node 673: SlaInstructionBits start=11, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_674(bytes, ctx),
        1 => match_node_instruction_675(bytes, ctx),
        2 => match_node_instruction_676(bytes, ctx),
        3 => match_node_instruction_677(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_674(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 674: Terminal matched constructor ID 37");
    37
}

fn match_node_instruction_675(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 675: Terminal matched constructor ID 153");
    153
}

fn match_node_instruction_676(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 676: Terminal matched constructor ID 250");
    250
}

fn match_node_instruction_677(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 677: Terminal matched constructor ID 242");
    242
}

fn match_node_instruction_678(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 3;
    eprintln!("Trace node 678: SlaInstructionBits start=11, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_679(bytes, ctx),
        1 => match_node_instruction_680(bytes, ctx),
        2 => match_node_instruction_681(bytes, ctx),
        3 => match_node_instruction_682(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_679(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 679: Terminal matched constructor ID 38");
    38
}

fn match_node_instruction_680(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 680: Terminal matched constructor ID 154");
    154
}

fn match_node_instruction_681(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 681: Terminal matched constructor ID 251");
    251
}

fn match_node_instruction_682(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 682: Terminal matched constructor ID 243");
    243
}

fn match_node_instruction_683(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 7;
    eprintln!("Trace node 683: SlaInstructionBits start=4, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_684(bytes, ctx),
        1 => match_node_instruction_685(bytes, ctx),
        2 => match_node_instruction_686(bytes, ctx),
        3 => match_node_instruction_687(bytes, ctx),
        4 => match_node_instruction_688(bytes, ctx),
        5 => match_node_instruction_697(bytes, ctx),
        6 => match_node_instruction_706(bytes, ctx),
        7 => match_node_instruction_715(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_684(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 684: Terminal matched constructor ID 39");
    39
}

fn match_node_instruction_685(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 685: Terminal matched constructor ID 155");
    155
}

fn match_node_instruction_686(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 686: Terminal matched constructor ID 252");
    252
}

fn match_node_instruction_687(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 687: Terminal matched constructor ID 244");
    244
}

fn match_node_instruction_688(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 7;
    eprintln!("Trace node 688: SlaInstructionBits start=10, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_689(bytes, ctx),
        1 => match_node_instruction_690(bytes, ctx),
        2 => match_node_instruction_691(bytes, ctx),
        3 => match_node_instruction_692(bytes, ctx),
        4 => match_node_instruction_693(bytes, ctx),
        5 => match_node_instruction_694(bytes, ctx),
        6 => match_node_instruction_695(bytes, ctx),
        7 => match_node_instruction_696(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_689(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 689: Terminal matched constructor ID 59");
    59
}

fn match_node_instruction_690(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 690: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_691(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 691: Terminal matched constructor ID 59");
    59
}

fn match_node_instruction_692(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 692: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_693(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 693: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_694(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 694: Terminal matched constructor ID 59");
    59
}

fn match_node_instruction_695(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 695: Terminal matched constructor ID 59");
    59
}

fn match_node_instruction_696(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 696: Terminal matched constructor ID 59");
    59
}

fn match_node_instruction_697(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 7;
    eprintln!("Trace node 697: SlaInstructionBits start=10, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_698(bytes, ctx),
        1 => match_node_instruction_699(bytes, ctx),
        2 => match_node_instruction_700(bytes, ctx),
        3 => match_node_instruction_701(bytes, ctx),
        4 => match_node_instruction_702(bytes, ctx),
        5 => match_node_instruction_703(bytes, ctx),
        6 => match_node_instruction_704(bytes, ctx),
        7 => match_node_instruction_705(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_698(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 698: Terminal matched constructor ID 51");
    51
}

fn match_node_instruction_699(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 699: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_700(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 700: Terminal matched constructor ID 51");
    51
}

fn match_node_instruction_701(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 701: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_702(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 702: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_703(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 703: Terminal matched constructor ID 51");
    51
}

fn match_node_instruction_704(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 704: Terminal matched constructor ID 51");
    51
}

fn match_node_instruction_705(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 705: Terminal matched constructor ID 51");
    51
}

fn match_node_instruction_706(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 7;
    eprintln!("Trace node 706: SlaInstructionBits start=10, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_707(bytes, ctx),
        1 => match_node_instruction_708(bytes, ctx),
        2 => match_node_instruction_709(bytes, ctx),
        3 => match_node_instruction_710(bytes, ctx),
        4 => match_node_instruction_711(bytes, ctx),
        5 => match_node_instruction_712(bytes, ctx),
        6 => match_node_instruction_713(bytes, ctx),
        7 => match_node_instruction_714(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_707(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 707: Terminal matched constructor ID 52");
    52
}

fn match_node_instruction_708(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 708: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_709(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 709: Terminal matched constructor ID 52");
    52
}

fn match_node_instruction_710(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 710: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_711(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 711: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_712(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 712: Terminal matched constructor ID 52");
    52
}

fn match_node_instruction_713(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 713: Terminal matched constructor ID 52");
    52
}

fn match_node_instruction_714(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 714: Terminal matched constructor ID 52");
    52
}

fn match_node_instruction_715(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 7;
    eprintln!("Trace node 715: SlaInstructionBits start=10, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_716(bytes, ctx),
        1 => match_node_instruction_717(bytes, ctx),
        2 => match_node_instruction_718(bytes, ctx),
        3 => match_node_instruction_719(bytes, ctx),
        4 => match_node_instruction_720(bytes, ctx),
        5 => match_node_instruction_721(bytes, ctx),
        6 => match_node_instruction_722(bytes, ctx),
        7 => match_node_instruction_723(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_716(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 716: Terminal matched constructor ID 58");
    58
}

fn match_node_instruction_717(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 717: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_718(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 718: Terminal matched constructor ID 58");
    58
}

fn match_node_instruction_719(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 719: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_720(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 720: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_721(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 721: Terminal matched constructor ID 58");
    58
}

fn match_node_instruction_722(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 722: Terminal matched constructor ID 58");
    58
}

fn match_node_instruction_723(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 723: Terminal matched constructor ID 58");
    58
}

fn match_node_instruction_724(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 3;
    eprintln!("Trace node 724: SlaInstructionBits start=11, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_725(bytes, ctx),
        1 => match_node_instruction_726(bytes, ctx),
        2 => match_node_instruction_727(bytes, ctx),
        3 => match_node_instruction_728(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_725(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 725: Terminal matched constructor ID 32");
    32
}

fn match_node_instruction_726(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 726: Terminal matched constructor ID 148");
    148
}

fn match_node_instruction_727(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 727: Terminal matched constructor ID 245");
    245
}

fn match_node_instruction_728(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 728: Terminal matched constructor ID 237");
    237
}

fn match_node_instruction_729(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 3;
    eprintln!("Trace node 729: SlaInstructionBits start=11, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_730(bytes, ctx),
        1 => match_node_instruction_731(bytes, ctx),
        2 => match_node_instruction_732(bytes, ctx),
        3 => match_node_instruction_733(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_730(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 730: Terminal matched constructor ID 33");
    33
}

fn match_node_instruction_731(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 731: Terminal matched constructor ID 149");
    149
}

fn match_node_instruction_732(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 732: Terminal matched constructor ID 246");
    246
}

fn match_node_instruction_733(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 733: Terminal matched constructor ID 238");
    238
}

fn match_node_instruction_734(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 3;
    eprintln!("Trace node 734: SlaInstructionBits start=11, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_735(bytes, ctx),
        1 => match_node_instruction_736(bytes, ctx),
        2 => match_node_instruction_737(bytes, ctx),
        3 => match_node_instruction_738(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_735(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 735: Terminal matched constructor ID 34");
    34
}

fn match_node_instruction_736(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 736: Terminal matched constructor ID 150");
    150
}

fn match_node_instruction_737(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 737: Terminal matched constructor ID 247");
    247
}

fn match_node_instruction_738(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 738: Terminal matched constructor ID 239");
    239
}

fn match_node_instruction_739(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 7;
    eprintln!("Trace node 739: SlaInstructionBits start=4, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_740(bytes, ctx),
        1 => match_node_instruction_741(bytes, ctx),
        2 => match_node_instruction_742(bytes, ctx),
        3 => match_node_instruction_743(bytes, ctx),
        4 => match_node_instruction_744(bytes, ctx),
        5 => match_node_instruction_753(bytes, ctx),
        6 => match_node_instruction_762(bytes, ctx),
        7 => match_node_instruction_771(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_740(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 740: Terminal matched constructor ID 35");
    35
}

fn match_node_instruction_741(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 741: Terminal matched constructor ID 151");
    151
}

fn match_node_instruction_742(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 742: Terminal matched constructor ID 248");
    248
}

fn match_node_instruction_743(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 743: Terminal matched constructor ID 240");
    240
}

fn match_node_instruction_744(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 7;
    eprintln!("Trace node 744: SlaInstructionBits start=10, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_745(bytes, ctx),
        1 => match_node_instruction_746(bytes, ctx),
        2 => match_node_instruction_747(bytes, ctx),
        3 => match_node_instruction_748(bytes, ctx),
        4 => match_node_instruction_749(bytes, ctx),
        5 => match_node_instruction_750(bytes, ctx),
        6 => match_node_instruction_751(bytes, ctx),
        7 => match_node_instruction_752(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_745(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 745: Terminal matched constructor ID 54");
    54
}

fn match_node_instruction_746(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 746: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_747(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 747: Terminal matched constructor ID 54");
    54
}

fn match_node_instruction_748(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 748: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_749(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 749: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_750(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 750: Terminal matched constructor ID 54");
    54
}

fn match_node_instruction_751(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 751: Terminal matched constructor ID 54");
    54
}

fn match_node_instruction_752(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 752: Terminal matched constructor ID 54");
    54
}

fn match_node_instruction_753(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 7;
    eprintln!("Trace node 753: SlaInstructionBits start=10, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_754(bytes, ctx),
        1 => match_node_instruction_755(bytes, ctx),
        2 => match_node_instruction_756(bytes, ctx),
        3 => match_node_instruction_757(bytes, ctx),
        4 => match_node_instruction_758(bytes, ctx),
        5 => match_node_instruction_759(bytes, ctx),
        6 => match_node_instruction_760(bytes, ctx),
        7 => match_node_instruction_761(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_754(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 754: Terminal matched constructor ID 53");
    53
}

fn match_node_instruction_755(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 755: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_756(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 756: Terminal matched constructor ID 53");
    53
}

fn match_node_instruction_757(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 757: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_758(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 758: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_759(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 759: Terminal matched constructor ID 53");
    53
}

fn match_node_instruction_760(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 760: Terminal matched constructor ID 53");
    53
}

fn match_node_instruction_761(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 761: Terminal matched constructor ID 53");
    53
}

fn match_node_instruction_762(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 7;
    eprintln!("Trace node 762: SlaInstructionBits start=10, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_763(bytes, ctx),
        1 => match_node_instruction_764(bytes, ctx),
        2 => match_node_instruction_765(bytes, ctx),
        3 => match_node_instruction_766(bytes, ctx),
        4 => match_node_instruction_767(bytes, ctx),
        5 => match_node_instruction_768(bytes, ctx),
        6 => match_node_instruction_769(bytes, ctx),
        7 => match_node_instruction_770(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_763(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 763: Terminal matched constructor ID 55");
    55
}

fn match_node_instruction_764(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 764: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_765(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 765: Terminal matched constructor ID 55");
    55
}

fn match_node_instruction_766(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 766: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_767(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 767: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_768(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 768: Terminal matched constructor ID 55");
    55
}

fn match_node_instruction_769(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 769: Terminal matched constructor ID 55");
    55
}

fn match_node_instruction_770(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 770: Terminal matched constructor ID 55");
    55
}

fn match_node_instruction_771(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 7;
    eprintln!("Trace node 771: SlaInstructionBits start=10, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_772(bytes, ctx),
        1 => match_node_instruction_773(bytes, ctx),
        2 => match_node_instruction_774(bytes, ctx),
        3 => match_node_instruction_775(bytes, ctx),
        4 => match_node_instruction_776(bytes, ctx),
        5 => match_node_instruction_777(bytes, ctx),
        6 => match_node_instruction_778(bytes, ctx),
        7 => match_node_instruction_779(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_772(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 772: Terminal matched constructor ID 57");
    57
}

fn match_node_instruction_773(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 773: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_774(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 774: Terminal matched constructor ID 57");
    57
}

fn match_node_instruction_775(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 775: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_776(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 776: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_777(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 777: Terminal matched constructor ID 57");
    57
}

fn match_node_instruction_778(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 778: Terminal matched constructor ID 57");
    57
}

fn match_node_instruction_779(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 779: Terminal matched constructor ID 57");
    57
}

fn match_node_instruction_780(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 7;
    eprintln!("Trace node 780: SlaInstructionBits start=5, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_781(bytes, ctx),
        1 => match_node_instruction_782(bytes, ctx),
        2 => match_node_instruction_783(bytes, ctx),
        3 => match_node_instruction_1286(bytes, ctx),
        4 => match_node_instruction_1303(bytes, ctx),
        5 => match_node_instruction_1312(bytes, ctx),
        6 => match_node_instruction_1329(bytes, ctx),
        7 => match_node_instruction_1338(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_781(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 781: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_782(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 782: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_783(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 3;
    eprintln!("Trace node 783: SlaInstructionBits start=8, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_784(bytes, ctx),
        1 => match_node_instruction_1273(bytes, ctx),
        2 => match_node_instruction_1284(bytes, ctx),
        3 => match_node_instruction_1285(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_784(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (16 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 16) & 7;
    eprintln!("Trace node 784: SlaInstructionBits start=16, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_785(bytes, ctx),
        1 => match_node_instruction_866(bytes, ctx),
        2 => match_node_instruction_867(bytes, ctx),
        3 => match_node_instruction_1164(bytes, ctx),
        4 => match_node_instruction_1197(bytes, ctx),
        5 => match_node_instruction_1210(bytes, ctx),
        6 => match_node_instruction_1223(bytes, ctx),
        7 => match_node_instruction_1244(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_785(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 15;
    eprintln!("Trace node 785: SlaInstructionBits start=25, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_786(bytes, ctx),
        1 => match_node_instruction_795(bytes, ctx),
        2 => match_node_instruction_804(bytes, ctx),
        3 => match_node_instruction_813(bytes, ctx),
        4 => match_node_instruction_822(bytes, ctx),
        5 => match_node_instruction_831(bytes, ctx),
        6 => match_node_instruction_832(bytes, ctx),
        7 => match_node_instruction_833(bytes, ctx),
        8 => match_node_instruction_836(bytes, ctx),
        9 => match_node_instruction_845(bytes, ctx),
        10 => match_node_instruction_846(bytes, ctx),
        11 => match_node_instruction_847(bytes, ctx),
        12 => match_node_instruction_852(bytes, ctx),
        13 => match_node_instruction_861(bytes, ctx),
        14 => match_node_instruction_864(bytes, ctx),
        15 => match_node_instruction_865(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_786(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (29 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 29) & 7;
    eprintln!("Trace node 786: SlaInstructionBits start=29, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_787(bytes, ctx),
        1 => match_node_instruction_788(bytes, ctx),
        2 => match_node_instruction_789(bytes, ctx),
        3 => match_node_instruction_790(bytes, ctx),
        4 => match_node_instruction_791(bytes, ctx),
        5 => match_node_instruction_792(bytes, ctx),
        6 => match_node_instruction_793(bytes, ctx),
        7 => match_node_instruction_794(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_787(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 787: Terminal matched constructor ID 346");
    346
}

fn match_node_instruction_788(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 788: Terminal matched constructor ID 332");
    332
}

fn match_node_instruction_789(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 789: Terminal matched constructor ID 401");
    401
}

fn match_node_instruction_790(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 790: Terminal matched constructor ID 334");
    334
}

fn match_node_instruction_791(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 791: Terminal matched constructor ID 403");
    403
}

fn match_node_instruction_792(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 792: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_793(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 793: Terminal matched constructor ID 342");
    342
}

fn match_node_instruction_794(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 794: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_795(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (29 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 29) & 7;
    eprintln!("Trace node 795: SlaInstructionBits start=29, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_796(bytes, ctx),
        1 => match_node_instruction_797(bytes, ctx),
        2 => match_node_instruction_798(bytes, ctx),
        3 => match_node_instruction_799(bytes, ctx),
        4 => match_node_instruction_800(bytes, ctx),
        5 => match_node_instruction_801(bytes, ctx),
        6 => match_node_instruction_802(bytes, ctx),
        7 => match_node_instruction_803(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_796(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 796: Terminal matched constructor ID 326");
    326
}

fn match_node_instruction_797(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 797: Terminal matched constructor ID 413");
    413
}

fn match_node_instruction_798(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 798: Terminal matched constructor ID 307");
    307
}

fn match_node_instruction_799(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 799: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_800(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 800: Terminal matched constructor ID 305");
    305
}

fn match_node_instruction_801(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 801: Terminal matched constructor ID 309");
    309
}

fn match_node_instruction_802(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 802: Terminal matched constructor ID 397");
    397
}

fn match_node_instruction_803(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 803: Terminal matched constructor ID 411");
    411
}

fn match_node_instruction_804(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (29 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 29) & 7;
    eprintln!("Trace node 804: SlaInstructionBits start=29, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_805(bytes, ctx),
        1 => match_node_instruction_806(bytes, ctx),
        2 => match_node_instruction_807(bytes, ctx),
        3 => match_node_instruction_808(bytes, ctx),
        4 => match_node_instruction_809(bytes, ctx),
        5 => match_node_instruction_810(bytes, ctx),
        6 => match_node_instruction_811(bytes, ctx),
        7 => match_node_instruction_812(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_805(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 805: Terminal matched constructor ID 324");
    324
}

fn match_node_instruction_806(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 806: Terminal matched constructor ID 422");
    422
}

fn match_node_instruction_807(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 807: Terminal matched constructor ID 415");
    415
}

fn match_node_instruction_808(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 808: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_809(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 809: Terminal matched constructor ID 340");
    340
}

fn match_node_instruction_810(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 810: Terminal matched constructor ID 336");
    336
}

fn match_node_instruction_811(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 811: Terminal matched constructor ID 338");
    338
}

fn match_node_instruction_812(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 812: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_813(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (29 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 29) & 7;
    eprintln!("Trace node 813: SlaInstructionBits start=29, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_814(bytes, ctx),
        1 => match_node_instruction_815(bytes, ctx),
        2 => match_node_instruction_816(bytes, ctx),
        3 => match_node_instruction_817(bytes, ctx),
        4 => match_node_instruction_818(bytes, ctx),
        5 => match_node_instruction_819(bytes, ctx),
        6 => match_node_instruction_820(bytes, ctx),
        7 => match_node_instruction_821(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_814(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 814: Terminal matched constructor ID 295");
    295
}

fn match_node_instruction_815(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 815: Terminal matched constructor ID 317");
    317
}

fn match_node_instruction_816(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 816: Terminal matched constructor ID 381");
    381
}

fn match_node_instruction_817(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 817: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_818(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 818: Terminal matched constructor ID 299");
    299
}

fn match_node_instruction_819(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 819: Terminal matched constructor ID 315");
    315
}

fn match_node_instruction_820(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 820: Terminal matched constructor ID 328");
    328
}

fn match_node_instruction_821(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 821: Terminal matched constructor ID 330");
    330
}

fn match_node_instruction_822(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (29 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 29) & 7;
    eprintln!("Trace node 822: SlaInstructionBits start=29, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_823(bytes, ctx),
        1 => match_node_instruction_824(bytes, ctx),
        2 => match_node_instruction_825(bytes, ctx),
        3 => match_node_instruction_826(bytes, ctx),
        4 => match_node_instruction_827(bytes, ctx),
        5 => match_node_instruction_828(bytes, ctx),
        6 => match_node_instruction_829(bytes, ctx),
        7 => match_node_instruction_830(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_823(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 823: Terminal matched constructor ID 320");
    320
}

fn match_node_instruction_824(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 824: Terminal matched constructor ID 344");
    344
}

fn match_node_instruction_825(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 825: Terminal matched constructor ID 301");
    301
}

fn match_node_instruction_826(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 826: Terminal matched constructor ID 377");
    377
}

fn match_node_instruction_827(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 827: Terminal matched constructor ID 393");
    393
}

fn match_node_instruction_828(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 828: Terminal matched constructor ID 386");
    386
}

fn match_node_instruction_829(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 829: Terminal matched constructor ID 390");
    390
}

fn match_node_instruction_830(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 830: Terminal matched constructor ID 395");
    395
}

fn match_node_instruction_831(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 831: Terminal matched constructor ID 407");
    407
}

fn match_node_instruction_832(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 832: Terminal matched constructor ID 399");
    399
}

fn match_node_instruction_833(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (30 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 30) & 1;
    eprintln!("Trace node 833: SlaInstructionBits start=30, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_834(bytes, ctx),
        1 => match_node_instruction_835(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_834(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 834: Terminal matched constructor ID 313");
    313
}

fn match_node_instruction_835(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 835: Terminal matched constructor ID 420");
    420
}

fn match_node_instruction_836(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (29 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 29) & 7;
    eprintln!("Trace node 836: SlaInstructionBits start=29, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_837(bytes, ctx),
        1 => match_node_instruction_838(bytes, ctx),
        2 => match_node_instruction_839(bytes, ctx),
        3 => match_node_instruction_840(bytes, ctx),
        4 => match_node_instruction_841(bytes, ctx),
        5 => match_node_instruction_842(bytes, ctx),
        6 => match_node_instruction_843(bytes, ctx),
        7 => match_node_instruction_844(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_837(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 837: Terminal matched constructor ID 348");
    348
}

fn match_node_instruction_838(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 838: Terminal matched constructor ID 405");
    405
}

fn match_node_instruction_839(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 839: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_840(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 840: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_841(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 841: Terminal matched constructor ID 348");
    348
}

fn match_node_instruction_842(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 842: Terminal matched constructor ID 405");
    405
}

fn match_node_instruction_843(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 843: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_844(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 844: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_845(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 845: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_846(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 846: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_847(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (29 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 29) & 3;
    eprintln!("Trace node 847: SlaInstructionBits start=29, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_848(bytes, ctx),
        1 => match_node_instruction_849(bytes, ctx),
        2 => match_node_instruction_850(bytes, ctx),
        3 => match_node_instruction_851(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_848(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 848: Terminal matched constructor ID 297");
    297
}

fn match_node_instruction_849(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 849: Terminal matched constructor ID 383");
    383
}

fn match_node_instruction_850(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 850: Terminal matched constructor ID 297");
    297
}

fn match_node_instruction_851(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 851: Terminal matched constructor ID 383");
    383
}

fn match_node_instruction_852(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (29 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 29) & 7;
    eprintln!("Trace node 852: SlaInstructionBits start=29, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_853(bytes, ctx),
        1 => match_node_instruction_854(bytes, ctx),
        2 => match_node_instruction_855(bytes, ctx),
        3 => match_node_instruction_856(bytes, ctx),
        4 => match_node_instruction_857(bytes, ctx),
        5 => match_node_instruction_858(bytes, ctx),
        6 => match_node_instruction_859(bytes, ctx),
        7 => match_node_instruction_860(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_853(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 853: Terminal matched constructor ID 322");
    322
}

fn match_node_instruction_854(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 854: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_855(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 855: Terminal matched constructor ID 303");
    303
}

fn match_node_instruction_856(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 856: Terminal matched constructor ID 379");
    379
}

fn match_node_instruction_857(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 857: Terminal matched constructor ID 322");
    322
}

fn match_node_instruction_858(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 858: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_859(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 859: Terminal matched constructor ID 303");
    303
}

fn match_node_instruction_860(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 860: Terminal matched constructor ID 379");
    379
}

fn match_node_instruction_861(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (29 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 29) & 1;
    eprintln!("Trace node 861: SlaInstructionBits start=29, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_862(bytes, ctx),
        1 => match_node_instruction_863(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_862(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 862: Terminal matched constructor ID 409");
    409
}

fn match_node_instruction_863(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 863: Terminal matched constructor ID 409");
    409
}

fn match_node_instruction_864(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 864: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_865(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 865: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_866(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 866: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_867(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 15;
    eprintln!("Trace node 867: SlaInstructionBits start=25, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_868(bytes, ctx),
        1 => match_node_instruction_901(bytes, ctx),
        2 => match_node_instruction_938(bytes, ctx),
        3 => match_node_instruction_971(bytes, ctx),
        4 => match_node_instruction_1008(bytes, ctx),
        5 => match_node_instruction_1049(bytes, ctx),
        6 => match_node_instruction_1054(bytes, ctx),
        7 => match_node_instruction_1059(bytes, ctx),
        8 => match_node_instruction_1070(bytes, ctx),
        9 => match_node_instruction_1095(bytes, ctx),
        10 => match_node_instruction_1096(bytes, ctx),
        11 => match_node_instruction_1097(bytes, ctx),
        12 => match_node_instruction_1118(bytes, ctx),
        13 => match_node_instruction_1151(bytes, ctx),
        14 => match_node_instruction_1162(bytes, ctx),
        15 => match_node_instruction_1163(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_868(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (29 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 29) & 7;
    eprintln!("Trace node 868: SlaInstructionBits start=29, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_869(bytes, ctx),
        1 => match_node_instruction_874(bytes, ctx),
        2 => match_node_instruction_879(bytes, ctx),
        3 => match_node_instruction_884(bytes, ctx),
        4 => match_node_instruction_889(bytes, ctx),
        5 => match_node_instruction_894(bytes, ctx),
        6 => match_node_instruction_895(bytes, ctx),
        7 => match_node_instruction_900(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_869(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 869: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_870(bytes, ctx),
        1 => match_node_instruction_873(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_870(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 870: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_871(bytes, ctx),
        1 => match_node_instruction_872(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_871(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 871: Terminal matched constructor ID 345");
    345
}

fn match_node_instruction_872(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 872: Terminal matched constructor ID 345");
    345
}

fn match_node_instruction_873(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 873: Terminal matched constructor ID 345");
    345
}

fn match_node_instruction_874(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 874: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_875(bytes, ctx),
        1 => match_node_instruction_878(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_875(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 875: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_876(bytes, ctx),
        1 => match_node_instruction_877(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_876(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 876: Terminal matched constructor ID 331");
    331
}

fn match_node_instruction_877(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 877: Terminal matched constructor ID 331");
    331
}

fn match_node_instruction_878(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 878: Terminal matched constructor ID 331");
    331
}

fn match_node_instruction_879(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 879: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_880(bytes, ctx),
        1 => match_node_instruction_883(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_880(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 880: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_881(bytes, ctx),
        1 => match_node_instruction_882(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_881(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 881: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_882(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 882: Terminal matched constructor ID 400");
    400
}

fn match_node_instruction_883(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 883: Terminal matched constructor ID 400");
    400
}

fn match_node_instruction_884(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 884: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_885(bytes, ctx),
        1 => match_node_instruction_888(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_885(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 885: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_886(bytes, ctx),
        1 => match_node_instruction_887(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_886(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 886: Terminal matched constructor ID 333");
    333
}

fn match_node_instruction_887(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 887: Terminal matched constructor ID 333");
    333
}

fn match_node_instruction_888(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 888: Terminal matched constructor ID 333");
    333
}

fn match_node_instruction_889(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 889: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_890(bytes, ctx),
        1 => match_node_instruction_893(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_890(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 890: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_891(bytes, ctx),
        1 => match_node_instruction_892(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_891(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 891: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_892(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 892: Terminal matched constructor ID 402");
    402
}

fn match_node_instruction_893(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 893: Terminal matched constructor ID 402");
    402
}

fn match_node_instruction_894(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 894: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_895(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 895: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_896(bytes, ctx),
        1 => match_node_instruction_899(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_896(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 896: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_897(bytes, ctx),
        1 => match_node_instruction_898(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_897(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 897: Terminal matched constructor ID 341");
    341
}

fn match_node_instruction_898(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 898: Terminal matched constructor ID 341");
    341
}

fn match_node_instruction_899(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 899: Terminal matched constructor ID 341");
    341
}

fn match_node_instruction_900(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 900: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_901(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (29 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 29) & 7;
    eprintln!("Trace node 901: SlaInstructionBits start=29, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_902(bytes, ctx),
        1 => match_node_instruction_907(bytes, ctx),
        2 => match_node_instruction_912(bytes, ctx),
        3 => match_node_instruction_917(bytes, ctx),
        4 => match_node_instruction_918(bytes, ctx),
        5 => match_node_instruction_923(bytes, ctx),
        6 => match_node_instruction_928(bytes, ctx),
        7 => match_node_instruction_933(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_902(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 902: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_903(bytes, ctx),
        1 => match_node_instruction_906(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_903(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 903: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_904(bytes, ctx),
        1 => match_node_instruction_905(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_904(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 904: Terminal matched constructor ID 325");
    325
}

fn match_node_instruction_905(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 905: Terminal matched constructor ID 325");
    325
}

fn match_node_instruction_906(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 906: Terminal matched constructor ID 325");
    325
}

fn match_node_instruction_907(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 907: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_908(bytes, ctx),
        1 => match_node_instruction_911(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_908(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 908: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_909(bytes, ctx),
        1 => match_node_instruction_910(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_909(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 909: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_910(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 910: Terminal matched constructor ID 412");
    412
}

fn match_node_instruction_911(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 911: Terminal matched constructor ID 412");
    412
}

fn match_node_instruction_912(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 912: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_913(bytes, ctx),
        1 => match_node_instruction_916(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_913(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 913: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_914(bytes, ctx),
        1 => match_node_instruction_915(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_914(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 914: Terminal matched constructor ID 306");
    306
}

fn match_node_instruction_915(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 915: Terminal matched constructor ID 306");
    306
}

fn match_node_instruction_916(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 916: Terminal matched constructor ID 306");
    306
}

fn match_node_instruction_917(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 917: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_918(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 918: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_919(bytes, ctx),
        1 => match_node_instruction_922(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_919(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 919: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_920(bytes, ctx),
        1 => match_node_instruction_921(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_920(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 920: Terminal matched constructor ID 304");
    304
}

fn match_node_instruction_921(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 921: Terminal matched constructor ID 304");
    304
}

fn match_node_instruction_922(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 922: Terminal matched constructor ID 304");
    304
}

fn match_node_instruction_923(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 923: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_924(bytes, ctx),
        1 => match_node_instruction_927(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_924(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 924: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_925(bytes, ctx),
        1 => match_node_instruction_926(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_925(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 925: Terminal matched constructor ID 308");
    308
}

fn match_node_instruction_926(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 926: Terminal matched constructor ID 308");
    308
}

fn match_node_instruction_927(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 927: Terminal matched constructor ID 308");
    308
}

fn match_node_instruction_928(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 928: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_929(bytes, ctx),
        1 => match_node_instruction_932(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_929(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 929: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_930(bytes, ctx),
        1 => match_node_instruction_931(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_930(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 930: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_931(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 931: Terminal matched constructor ID 396");
    396
}

fn match_node_instruction_932(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 932: Terminal matched constructor ID 396");
    396
}

fn match_node_instruction_933(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 933: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_934(bytes, ctx),
        1 => match_node_instruction_937(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_934(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 934: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_935(bytes, ctx),
        1 => match_node_instruction_936(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_935(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 935: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_936(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 936: Terminal matched constructor ID 410");
    410
}

fn match_node_instruction_937(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 937: Terminal matched constructor ID 410");
    410
}

fn match_node_instruction_938(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (29 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 29) & 7;
    eprintln!("Trace node 938: SlaInstructionBits start=29, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_939(bytes, ctx),
        1 => match_node_instruction_944(bytes, ctx),
        2 => match_node_instruction_949(bytes, ctx),
        3 => match_node_instruction_954(bytes, ctx),
        4 => match_node_instruction_955(bytes, ctx),
        5 => match_node_instruction_960(bytes, ctx),
        6 => match_node_instruction_965(bytes, ctx),
        7 => match_node_instruction_970(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_939(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 939: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_940(bytes, ctx),
        1 => match_node_instruction_943(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_940(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 940: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_941(bytes, ctx),
        1 => match_node_instruction_942(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_941(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 941: Terminal matched constructor ID 323");
    323
}

fn match_node_instruction_942(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 942: Terminal matched constructor ID 323");
    323
}

fn match_node_instruction_943(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 943: Terminal matched constructor ID 323");
    323
}

fn match_node_instruction_944(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 944: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_945(bytes, ctx),
        1 => match_node_instruction_948(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_945(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 945: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_946(bytes, ctx),
        1 => match_node_instruction_947(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_946(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 946: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_947(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 947: Terminal matched constructor ID 421");
    421
}

fn match_node_instruction_948(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 948: Terminal matched constructor ID 421");
    421
}

fn match_node_instruction_949(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 949: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_950(bytes, ctx),
        1 => match_node_instruction_953(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_950(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 950: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_951(bytes, ctx),
        1 => match_node_instruction_952(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_951(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 951: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_952(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 952: Terminal matched constructor ID 414");
    414
}

fn match_node_instruction_953(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 953: Terminal matched constructor ID 414");
    414
}

fn match_node_instruction_954(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 954: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_955(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 955: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_956(bytes, ctx),
        1 => match_node_instruction_959(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_956(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 956: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_957(bytes, ctx),
        1 => match_node_instruction_958(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_957(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 957: Terminal matched constructor ID 339");
    339
}

fn match_node_instruction_958(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 958: Terminal matched constructor ID 339");
    339
}

fn match_node_instruction_959(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 959: Terminal matched constructor ID 339");
    339
}

fn match_node_instruction_960(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 960: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_961(bytes, ctx),
        1 => match_node_instruction_964(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_961(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 961: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_962(bytes, ctx),
        1 => match_node_instruction_963(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_962(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 962: Terminal matched constructor ID 335");
    335
}

fn match_node_instruction_963(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 963: Terminal matched constructor ID 335");
    335
}

fn match_node_instruction_964(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 964: Terminal matched constructor ID 335");
    335
}

fn match_node_instruction_965(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 965: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_966(bytes, ctx),
        1 => match_node_instruction_969(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_966(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 966: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_967(bytes, ctx),
        1 => match_node_instruction_968(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_967(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 967: Terminal matched constructor ID 337");
    337
}

fn match_node_instruction_968(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 968: Terminal matched constructor ID 337");
    337
}

fn match_node_instruction_969(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 969: Terminal matched constructor ID 337");
    337
}

fn match_node_instruction_970(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 970: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_971(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (29 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 29) & 7;
    eprintln!("Trace node 971: SlaInstructionBits start=29, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_972(bytes, ctx),
        1 => match_node_instruction_977(bytes, ctx),
        2 => match_node_instruction_982(bytes, ctx),
        3 => match_node_instruction_987(bytes, ctx),
        4 => match_node_instruction_988(bytes, ctx),
        5 => match_node_instruction_993(bytes, ctx),
        6 => match_node_instruction_998(bytes, ctx),
        7 => match_node_instruction_1003(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_972(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 972: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_973(bytes, ctx),
        1 => match_node_instruction_976(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_973(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 973: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_974(bytes, ctx),
        1 => match_node_instruction_975(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_974(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 974: Terminal matched constructor ID 294");
    294
}

fn match_node_instruction_975(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 975: Terminal matched constructor ID 294");
    294
}

fn match_node_instruction_976(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 976: Terminal matched constructor ID 294");
    294
}

fn match_node_instruction_977(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 977: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_978(bytes, ctx),
        1 => match_node_instruction_981(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_978(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 978: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_979(bytes, ctx),
        1 => match_node_instruction_980(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_979(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 979: Terminal matched constructor ID 316");
    316
}

fn match_node_instruction_980(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 980: Terminal matched constructor ID 316");
    316
}

fn match_node_instruction_981(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 981: Terminal matched constructor ID 316");
    316
}

fn match_node_instruction_982(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 982: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_983(bytes, ctx),
        1 => match_node_instruction_986(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_983(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 983: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_984(bytes, ctx),
        1 => match_node_instruction_985(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_984(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 984: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_985(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 985: Terminal matched constructor ID 380");
    380
}

fn match_node_instruction_986(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 986: Terminal matched constructor ID 380");
    380
}

fn match_node_instruction_987(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 987: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_988(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 988: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_989(bytes, ctx),
        1 => match_node_instruction_992(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_989(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 989: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_990(bytes, ctx),
        1 => match_node_instruction_991(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_990(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 990: Terminal matched constructor ID 298");
    298
}

fn match_node_instruction_991(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 991: Terminal matched constructor ID 298");
    298
}

fn match_node_instruction_992(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 992: Terminal matched constructor ID 298");
    298
}

fn match_node_instruction_993(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 993: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_994(bytes, ctx),
        1 => match_node_instruction_997(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_994(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 994: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_995(bytes, ctx),
        1 => match_node_instruction_996(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_995(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 995: Terminal matched constructor ID 314");
    314
}

fn match_node_instruction_996(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 996: Terminal matched constructor ID 314");
    314
}

fn match_node_instruction_997(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 997: Terminal matched constructor ID 314");
    314
}

fn match_node_instruction_998(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 998: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_999(bytes, ctx),
        1 => match_node_instruction_1002(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_999(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 999: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1000(bytes, ctx),
        1 => match_node_instruction_1001(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1000(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1000: Terminal matched constructor ID 327");
    327
}

fn match_node_instruction_1001(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1001: Terminal matched constructor ID 327");
    327
}

fn match_node_instruction_1002(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1002: Terminal matched constructor ID 327");
    327
}

fn match_node_instruction_1003(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1003: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1004(bytes, ctx),
        1 => match_node_instruction_1007(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1004(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1004: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1005(bytes, ctx),
        1 => match_node_instruction_1006(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1005(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1005: Terminal matched constructor ID 329");
    329
}

fn match_node_instruction_1006(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1006: Terminal matched constructor ID 329");
    329
}

fn match_node_instruction_1007(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1007: Terminal matched constructor ID 329");
    329
}

fn match_node_instruction_1008(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (29 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 29) & 7;
    eprintln!("Trace node 1008: SlaInstructionBits start=29, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1009(bytes, ctx),
        1 => match_node_instruction_1014(bytes, ctx),
        2 => match_node_instruction_1019(bytes, ctx),
        3 => match_node_instruction_1024(bytes, ctx),
        4 => match_node_instruction_1029(bytes, ctx),
        5 => match_node_instruction_1034(bytes, ctx),
        6 => match_node_instruction_1039(bytes, ctx),
        7 => match_node_instruction_1044(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1009(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1009: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1010(bytes, ctx),
        1 => match_node_instruction_1013(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1010(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1010: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1011(bytes, ctx),
        1 => match_node_instruction_1012(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1011(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1011: Terminal matched constructor ID 319");
    319
}

fn match_node_instruction_1012(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1012: Terminal matched constructor ID 319");
    319
}

fn match_node_instruction_1013(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1013: Terminal matched constructor ID 319");
    319
}

fn match_node_instruction_1014(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1014: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1015(bytes, ctx),
        1 => match_node_instruction_1018(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1015(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1015: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1016(bytes, ctx),
        1 => match_node_instruction_1017(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1016(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1016: Terminal matched constructor ID 343");
    343
}

fn match_node_instruction_1017(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1017: Terminal matched constructor ID 343");
    343
}

fn match_node_instruction_1018(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1018: Terminal matched constructor ID 343");
    343
}

fn match_node_instruction_1019(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1019: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1020(bytes, ctx),
        1 => match_node_instruction_1023(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1020(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1020: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1021(bytes, ctx),
        1 => match_node_instruction_1022(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1021(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1021: Terminal matched constructor ID 300");
    300
}

fn match_node_instruction_1022(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1022: Terminal matched constructor ID 300");
    300
}

fn match_node_instruction_1023(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1023: Terminal matched constructor ID 300");
    300
}

fn match_node_instruction_1024(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1024: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1025(bytes, ctx),
        1 => match_node_instruction_1028(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1025(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1025: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1026(bytes, ctx),
        1 => match_node_instruction_1027(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1026(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1026: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_1027(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1027: Terminal matched constructor ID 376");
    376
}

fn match_node_instruction_1028(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1028: Terminal matched constructor ID 376");
    376
}

fn match_node_instruction_1029(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1029: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1030(bytes, ctx),
        1 => match_node_instruction_1033(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1030(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1030: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1031(bytes, ctx),
        1 => match_node_instruction_1032(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1031(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1031: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_1032(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1032: Terminal matched constructor ID 392");
    392
}

fn match_node_instruction_1033(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1033: Terminal matched constructor ID 392");
    392
}

fn match_node_instruction_1034(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1034: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1035(bytes, ctx),
        1 => match_node_instruction_1038(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1035(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1035: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1036(bytes, ctx),
        1 => match_node_instruction_1037(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1036(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1036: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_1037(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1037: Terminal matched constructor ID 385");
    385
}

fn match_node_instruction_1038(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1038: Terminal matched constructor ID 385");
    385
}

fn match_node_instruction_1039(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1039: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1040(bytes, ctx),
        1 => match_node_instruction_1043(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1040(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1040: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1041(bytes, ctx),
        1 => match_node_instruction_1042(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1041(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1041: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_1042(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1042: Terminal matched constructor ID 389");
    389
}

fn match_node_instruction_1043(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1043: Terminal matched constructor ID 389");
    389
}

fn match_node_instruction_1044(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1044: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1045(bytes, ctx),
        1 => match_node_instruction_1048(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1045(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1045: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1046(bytes, ctx),
        1 => match_node_instruction_1047(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1046(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1046: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_1047(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1047: Terminal matched constructor ID 394");
    394
}

fn match_node_instruction_1048(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1048: Terminal matched constructor ID 394");
    394
}

fn match_node_instruction_1049(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1049: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1050(bytes, ctx),
        1 => match_node_instruction_1053(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1050(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1050: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1051(bytes, ctx),
        1 => match_node_instruction_1052(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1051(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1051: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_1052(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1052: Terminal matched constructor ID 406");
    406
}

fn match_node_instruction_1053(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1053: Terminal matched constructor ID 406");
    406
}

fn match_node_instruction_1054(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1054: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1055(bytes, ctx),
        1 => match_node_instruction_1058(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1055(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1055: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1056(bytes, ctx),
        1 => match_node_instruction_1057(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1056(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1056: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_1057(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1057: Terminal matched constructor ID 398");
    398
}

fn match_node_instruction_1058(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1058: Terminal matched constructor ID 398");
    398
}

fn match_node_instruction_1059(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (30 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 30) & 1;
    eprintln!("Trace node 1059: SlaInstructionBits start=30, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1060(bytes, ctx),
        1 => match_node_instruction_1065(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1060(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1060: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1061(bytes, ctx),
        1 => match_node_instruction_1064(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1061(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1061: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1062(bytes, ctx),
        1 => match_node_instruction_1063(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1062(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1062: Terminal matched constructor ID 312");
    312
}

fn match_node_instruction_1063(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1063: Terminal matched constructor ID 312");
    312
}

fn match_node_instruction_1064(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1064: Terminal matched constructor ID 312");
    312
}

fn match_node_instruction_1065(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1065: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1066(bytes, ctx),
        1 => match_node_instruction_1069(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1066(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1066: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1067(bytes, ctx),
        1 => match_node_instruction_1068(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1067(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1067: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_1068(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1068: Terminal matched constructor ID 419");
    419
}

fn match_node_instruction_1069(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1069: Terminal matched constructor ID 419");
    419
}

fn match_node_instruction_1070(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (29 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 29) & 7;
    eprintln!("Trace node 1070: SlaInstructionBits start=29, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1071(bytes, ctx),
        1 => match_node_instruction_1076(bytes, ctx),
        2 => match_node_instruction_1081(bytes, ctx),
        3 => match_node_instruction_1082(bytes, ctx),
        4 => match_node_instruction_1083(bytes, ctx),
        5 => match_node_instruction_1088(bytes, ctx),
        6 => match_node_instruction_1093(bytes, ctx),
        7 => match_node_instruction_1094(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1071(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1071: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1072(bytes, ctx),
        1 => match_node_instruction_1075(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1072(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1072: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1073(bytes, ctx),
        1 => match_node_instruction_1074(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1073(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1073: Terminal matched constructor ID 347");
    347
}

fn match_node_instruction_1074(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1074: Terminal matched constructor ID 347");
    347
}

fn match_node_instruction_1075(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1075: Terminal matched constructor ID 347");
    347
}

fn match_node_instruction_1076(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1076: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1077(bytes, ctx),
        1 => match_node_instruction_1080(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1077(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1077: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1078(bytes, ctx),
        1 => match_node_instruction_1079(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1078(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1078: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_1079(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1079: Terminal matched constructor ID 404");
    404
}

fn match_node_instruction_1080(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1080: Terminal matched constructor ID 404");
    404
}

fn match_node_instruction_1081(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1081: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_1082(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1082: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_1083(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1083: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1084(bytes, ctx),
        1 => match_node_instruction_1087(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1084(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1084: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1085(bytes, ctx),
        1 => match_node_instruction_1086(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1085(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1085: Terminal matched constructor ID 347");
    347
}

fn match_node_instruction_1086(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1086: Terminal matched constructor ID 347");
    347
}

fn match_node_instruction_1087(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1087: Terminal matched constructor ID 347");
    347
}

fn match_node_instruction_1088(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1088: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1089(bytes, ctx),
        1 => match_node_instruction_1092(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1089(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1089: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1090(bytes, ctx),
        1 => match_node_instruction_1091(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1090(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1090: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_1091(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1091: Terminal matched constructor ID 404");
    404
}

fn match_node_instruction_1092(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1092: Terminal matched constructor ID 404");
    404
}

fn match_node_instruction_1093(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1093: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_1094(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1094: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_1095(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1095: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_1096(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1096: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_1097(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (29 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 29) & 3;
    eprintln!("Trace node 1097: SlaInstructionBits start=29, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1098(bytes, ctx),
        1 => match_node_instruction_1103(bytes, ctx),
        2 => match_node_instruction_1108(bytes, ctx),
        3 => match_node_instruction_1113(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1098(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1098: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1099(bytes, ctx),
        1 => match_node_instruction_1102(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1099(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1099: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1100(bytes, ctx),
        1 => match_node_instruction_1101(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1100(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1100: Terminal matched constructor ID 296");
    296
}

fn match_node_instruction_1101(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1101: Terminal matched constructor ID 296");
    296
}

fn match_node_instruction_1102(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1102: Terminal matched constructor ID 296");
    296
}

fn match_node_instruction_1103(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1103: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1104(bytes, ctx),
        1 => match_node_instruction_1107(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1104(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1104: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1105(bytes, ctx),
        1 => match_node_instruction_1106(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1105(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1105: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_1106(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1106: Terminal matched constructor ID 382");
    382
}

fn match_node_instruction_1107(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1107: Terminal matched constructor ID 382");
    382
}

fn match_node_instruction_1108(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1108: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1109(bytes, ctx),
        1 => match_node_instruction_1112(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1109(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1109: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1110(bytes, ctx),
        1 => match_node_instruction_1111(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1110(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1110: Terminal matched constructor ID 296");
    296
}

fn match_node_instruction_1111(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1111: Terminal matched constructor ID 296");
    296
}

fn match_node_instruction_1112(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1112: Terminal matched constructor ID 296");
    296
}

fn match_node_instruction_1113(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1113: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1114(bytes, ctx),
        1 => match_node_instruction_1117(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1114(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1114: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1115(bytes, ctx),
        1 => match_node_instruction_1116(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1115(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1115: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_1116(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1116: Terminal matched constructor ID 382");
    382
}

fn match_node_instruction_1117(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1117: Terminal matched constructor ID 382");
    382
}

fn match_node_instruction_1118(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (29 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 29) & 7;
    eprintln!("Trace node 1118: SlaInstructionBits start=29, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1119(bytes, ctx),
        1 => match_node_instruction_1124(bytes, ctx),
        2 => match_node_instruction_1125(bytes, ctx),
        3 => match_node_instruction_1130(bytes, ctx),
        4 => match_node_instruction_1135(bytes, ctx),
        5 => match_node_instruction_1140(bytes, ctx),
        6 => match_node_instruction_1141(bytes, ctx),
        7 => match_node_instruction_1146(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1119(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1119: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1120(bytes, ctx),
        1 => match_node_instruction_1123(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1120(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1120: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1121(bytes, ctx),
        1 => match_node_instruction_1122(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1121(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1121: Terminal matched constructor ID 321");
    321
}

fn match_node_instruction_1122(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1122: Terminal matched constructor ID 321");
    321
}

fn match_node_instruction_1123(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1123: Terminal matched constructor ID 321");
    321
}

fn match_node_instruction_1124(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1124: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_1125(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1125: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1126(bytes, ctx),
        1 => match_node_instruction_1129(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1126(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1126: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1127(bytes, ctx),
        1 => match_node_instruction_1128(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1127(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1127: Terminal matched constructor ID 302");
    302
}

fn match_node_instruction_1128(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1128: Terminal matched constructor ID 302");
    302
}

fn match_node_instruction_1129(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1129: Terminal matched constructor ID 302");
    302
}

fn match_node_instruction_1130(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1130: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1131(bytes, ctx),
        1 => match_node_instruction_1134(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1131(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1131: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1132(bytes, ctx),
        1 => match_node_instruction_1133(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1132(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1132: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_1133(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1133: Terminal matched constructor ID 378");
    378
}

fn match_node_instruction_1134(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1134: Terminal matched constructor ID 378");
    378
}

fn match_node_instruction_1135(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1135: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1136(bytes, ctx),
        1 => match_node_instruction_1139(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1136(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1136: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1137(bytes, ctx),
        1 => match_node_instruction_1138(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1137(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1137: Terminal matched constructor ID 321");
    321
}

fn match_node_instruction_1138(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1138: Terminal matched constructor ID 321");
    321
}

fn match_node_instruction_1139(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1139: Terminal matched constructor ID 321");
    321
}

fn match_node_instruction_1140(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1140: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_1141(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1141: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1142(bytes, ctx),
        1 => match_node_instruction_1145(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1142(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1142: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1143(bytes, ctx),
        1 => match_node_instruction_1144(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1143(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1143: Terminal matched constructor ID 302");
    302
}

fn match_node_instruction_1144(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1144: Terminal matched constructor ID 302");
    302
}

fn match_node_instruction_1145(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1145: Terminal matched constructor ID 302");
    302
}

fn match_node_instruction_1146(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1146: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1147(bytes, ctx),
        1 => match_node_instruction_1150(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1147(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1147: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1148(bytes, ctx),
        1 => match_node_instruction_1149(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1148(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1148: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_1149(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1149: Terminal matched constructor ID 378");
    378
}

fn match_node_instruction_1150(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1150: Terminal matched constructor ID 378");
    378
}

fn match_node_instruction_1151(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (29 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 29) & 1;
    eprintln!("Trace node 1151: SlaInstructionBits start=29, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1152(bytes, ctx),
        1 => match_node_instruction_1157(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1152(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1152: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1153(bytes, ctx),
        1 => match_node_instruction_1156(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1153(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1153: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1154(bytes, ctx),
        1 => match_node_instruction_1155(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1154(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1154: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_1155(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1155: Terminal matched constructor ID 408");
    408
}

fn match_node_instruction_1156(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1156: Terminal matched constructor ID 408");
    408
}

fn match_node_instruction_1157(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1157: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1158(bytes, ctx),
        1 => match_node_instruction_1161(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1158(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1158: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1159(bytes, ctx),
        1 => match_node_instruction_1160(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1159(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1159: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_1160(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1160: Terminal matched constructor ID 408");
    408
}

fn match_node_instruction_1161(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1161: Terminal matched constructor ID 408");
    408
}

fn match_node_instruction_1162(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1162: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_1163(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1163: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_1164(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (19 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 19) & 7;
    eprintln!("Trace node 1164: SlaInstructionBits start=19, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1165(bytes, ctx),
        1 => match_node_instruction_1170(bytes, ctx),
        2 => match_node_instruction_1175(bytes, ctx),
        3 => match_node_instruction_1176(bytes, ctx),
        4 => match_node_instruction_1181(bytes, ctx),
        5 => match_node_instruction_1186(bytes, ctx),
        6 => match_node_instruction_1187(bytes, ctx),
        7 => match_node_instruction_1192(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1165(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1165: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1166(bytes, ctx),
        1 => match_node_instruction_1169(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1166(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1166: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1167(bytes, ctx),
        1 => match_node_instruction_1168(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1167(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1167: Terminal matched constructor ID 351");
    351
}

fn match_node_instruction_1168(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1168: Terminal matched constructor ID 351");
    351
}

fn match_node_instruction_1169(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1169: Terminal matched constructor ID 351");
    351
}

fn match_node_instruction_1170(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1170: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1171(bytes, ctx),
        1 => match_node_instruction_1174(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1171(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1171: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1172(bytes, ctx),
        1 => match_node_instruction_1173(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1172(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1172: Terminal matched constructor ID 352");
    352
}

fn match_node_instruction_1173(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1173: Terminal matched constructor ID 352");
    352
}

fn match_node_instruction_1174(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1174: Terminal matched constructor ID 352");
    352
}

fn match_node_instruction_1175(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1175: Terminal matched constructor ID 353");
    353
}

fn match_node_instruction_1176(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1176: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1177(bytes, ctx),
        1 => match_node_instruction_1180(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1177(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1177: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1178(bytes, ctx),
        1 => match_node_instruction_1179(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1178(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1178: Terminal matched constructor ID 355");
    355
}

fn match_node_instruction_1179(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1179: Terminal matched constructor ID 355");
    355
}

fn match_node_instruction_1180(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1180: Terminal matched constructor ID 355");
    355
}

fn match_node_instruction_1181(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1181: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1182(bytes, ctx),
        1 => match_node_instruction_1185(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1182(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1182: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1183(bytes, ctx),
        1 => match_node_instruction_1184(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1183(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1183: Terminal matched constructor ID 350");
    350
}

fn match_node_instruction_1184(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1184: Terminal matched constructor ID 350");
    350
}

fn match_node_instruction_1185(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1185: Terminal matched constructor ID 350");
    350
}

fn match_node_instruction_1186(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1186: Terminal matched constructor ID 354");
    354
}

fn match_node_instruction_1187(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1187: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1188(bytes, ctx),
        1 => match_node_instruction_1191(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1188(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1188: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1189(bytes, ctx),
        1 => match_node_instruction_1190(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1189(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1189: Terminal matched constructor ID 349");
    349
}

fn match_node_instruction_1190(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1190: Terminal matched constructor ID 349");
    349
}

fn match_node_instruction_1191(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1191: Terminal matched constructor ID 349");
    349
}

fn match_node_instruction_1192(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1192: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1193(bytes, ctx),
        1 => match_node_instruction_1196(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1193(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1193: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1194(bytes, ctx),
        1 => match_node_instruction_1195(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1194(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1194: Terminal matched constructor ID 356");
    356
}

fn match_node_instruction_1195(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1195: Terminal matched constructor ID 356");
    356
}

fn match_node_instruction_1196(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1196: Terminal matched constructor ID 356");
    356
}

fn match_node_instruction_1197(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (19 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 19) & 3;
    eprintln!("Trace node 1197: SlaInstructionBits start=19, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1198(bytes, ctx),
        1 => match_node_instruction_1199(bytes, ctx),
        2 => match_node_instruction_1204(bytes, ctx),
        3 => match_node_instruction_1209(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1198(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1198: Terminal matched constructor ID 361");
    361
}

fn match_node_instruction_1199(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1199: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1200(bytes, ctx),
        1 => match_node_instruction_1203(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1200(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1200: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1201(bytes, ctx),
        1 => match_node_instruction_1202(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1201(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1201: Terminal matched constructor ID 359");
    359
}

fn match_node_instruction_1202(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1202: Terminal matched constructor ID 359");
    359
}

fn match_node_instruction_1203(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1203: Terminal matched constructor ID 359");
    359
}

fn match_node_instruction_1204(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1204: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1205(bytes, ctx),
        1 => match_node_instruction_1208(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1205(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1205: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1206(bytes, ctx),
        1 => match_node_instruction_1207(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1206(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1206: Terminal matched constructor ID 357");
    357
}

fn match_node_instruction_1207(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1207: Terminal matched constructor ID 357");
    357
}

fn match_node_instruction_1208(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1208: Terminal matched constructor ID 357");
    357
}

fn match_node_instruction_1209(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1209: Terminal matched constructor ID 375");
    375
}

fn match_node_instruction_1210(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (19 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 19) & 3;
    eprintln!("Trace node 1210: SlaInstructionBits start=19, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1211(bytes, ctx),
        1 => match_node_instruction_1212(bytes, ctx),
        2 => match_node_instruction_1217(bytes, ctx),
        3 => match_node_instruction_1222(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1211(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1211: Terminal matched constructor ID 362");
    362
}

fn match_node_instruction_1212(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1212: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1213(bytes, ctx),
        1 => match_node_instruction_1216(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1213(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1213: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1214(bytes, ctx),
        1 => match_node_instruction_1215(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1214(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1214: Terminal matched constructor ID 360");
    360
}

fn match_node_instruction_1215(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1215: Terminal matched constructor ID 360");
    360
}

fn match_node_instruction_1216(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1216: Terminal matched constructor ID 360");
    360
}

fn match_node_instruction_1217(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1217: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1218(bytes, ctx),
        1 => match_node_instruction_1221(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1218(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1218: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1219(bytes, ctx),
        1 => match_node_instruction_1220(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1219(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1219: Terminal matched constructor ID 358");
    358
}

fn match_node_instruction_1220(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1220: Terminal matched constructor ID 358");
    358
}

fn match_node_instruction_1221(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1221: Terminal matched constructor ID 358");
    358
}

fn match_node_instruction_1222(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1222: Terminal matched constructor ID 374");
    374
}

fn match_node_instruction_1223(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 7;
    eprintln!("Trace node 1223: SlaInstructionBits start=10, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1224(bytes, ctx),
        1 => match_node_instruction_1225(bytes, ctx),
        2 => match_node_instruction_1226(bytes, ctx),
        3 => match_node_instruction_1229(bytes, ctx),
        4 => match_node_instruction_1232(bytes, ctx),
        5 => match_node_instruction_1235(bytes, ctx),
        6 => match_node_instruction_1238(bytes, ctx),
        7 => match_node_instruction_1241(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1224(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1224: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1225(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1225: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1226(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 1226: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1227(bytes, ctx),
        1 => match_node_instruction_1228(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1227(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1227: Terminal matched constructor ID 367");
    367
}

fn match_node_instruction_1228(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1228: Terminal matched constructor ID 372");
    372
}

fn match_node_instruction_1229(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 1229: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1230(bytes, ctx),
        1 => match_node_instruction_1231(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1230(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1230: Terminal matched constructor ID 368");
    368
}

fn match_node_instruction_1231(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1231: Terminal matched constructor ID 372");
    372
}

fn match_node_instruction_1232(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 1232: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1233(bytes, ctx),
        1 => match_node_instruction_1234(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1233(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1233: Terminal matched constructor ID 369");
    369
}

fn match_node_instruction_1234(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1234: Terminal matched constructor ID 373");
    373
}

fn match_node_instruction_1235(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 1235: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1236(bytes, ctx),
        1 => match_node_instruction_1237(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1236(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1236: Terminal matched constructor ID 367");
    367
}

fn match_node_instruction_1237(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1237: Terminal matched constructor ID 372");
    372
}

fn match_node_instruction_1238(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 1238: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1239(bytes, ctx),
        1 => match_node_instruction_1240(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1239(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1239: Terminal matched constructor ID 367");
    367
}

fn match_node_instruction_1240(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1240: Terminal matched constructor ID 372");
    372
}

fn match_node_instruction_1241(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 1241: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1242(bytes, ctx),
        1 => match_node_instruction_1243(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1242(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1242: Terminal matched constructor ID 367");
    367
}

fn match_node_instruction_1243(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1243: Terminal matched constructor ID 372");
    372
}

fn match_node_instruction_1244(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 7;
    eprintln!("Trace node 1244: SlaInstructionBits start=10, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1245(bytes, ctx),
        1 => match_node_instruction_1246(bytes, ctx),
        2 => match_node_instruction_1247(bytes, ctx),
        3 => match_node_instruction_1252(bytes, ctx),
        4 => match_node_instruction_1255(bytes, ctx),
        5 => match_node_instruction_1258(bytes, ctx),
        6 => match_node_instruction_1263(bytes, ctx),
        7 => match_node_instruction_1268(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1245(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1245: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1246(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1246: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1247(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (19 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 19) & 3;
    eprintln!("Trace node 1247: SlaInstructionBits start=19, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1248(bytes, ctx),
        1 => match_node_instruction_1249(bytes, ctx),
        2 => match_node_instruction_1250(bytes, ctx),
        3 => match_node_instruction_1251(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1248(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1248: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1249(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1249: Terminal matched constructor ID 371");
    371
}

fn match_node_instruction_1250(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1250: Terminal matched constructor ID 364");
    364
}

fn match_node_instruction_1251(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1251: Terminal matched constructor ID 370");
    370
}

fn match_node_instruction_1252(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 1252: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1253(bytes, ctx),
        1 => match_node_instruction_1254(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1253(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1253: Terminal matched constructor ID 365");
    365
}

fn match_node_instruction_1254(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1254: Terminal matched constructor ID 370");
    370
}

fn match_node_instruction_1255(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 1255: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1256(bytes, ctx),
        1 => match_node_instruction_1257(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1256(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1256: Terminal matched constructor ID 366");
    366
}

fn match_node_instruction_1257(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1257: Terminal matched constructor ID 371");
    371
}

fn match_node_instruction_1258(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (19 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 19) & 3;
    eprintln!("Trace node 1258: SlaInstructionBits start=19, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1259(bytes, ctx),
        1 => match_node_instruction_1260(bytes, ctx),
        2 => match_node_instruction_1261(bytes, ctx),
        3 => match_node_instruction_1262(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1259(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1259: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1260(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1260: Terminal matched constructor ID 371");
    371
}

fn match_node_instruction_1261(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1261: Terminal matched constructor ID 364");
    364
}

fn match_node_instruction_1262(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1262: Terminal matched constructor ID 370");
    370
}

fn match_node_instruction_1263(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (19 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 19) & 3;
    eprintln!("Trace node 1263: SlaInstructionBits start=19, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1264(bytes, ctx),
        1 => match_node_instruction_1265(bytes, ctx),
        2 => match_node_instruction_1266(bytes, ctx),
        3 => match_node_instruction_1267(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1264(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1264: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1265(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1265: Terminal matched constructor ID 371");
    371
}

fn match_node_instruction_1266(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1266: Terminal matched constructor ID 364");
    364
}

fn match_node_instruction_1267(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1267: Terminal matched constructor ID 370");
    370
}

fn match_node_instruction_1268(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (19 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 19) & 3;
    eprintln!("Trace node 1268: SlaInstructionBits start=19, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1269(bytes, ctx),
        1 => match_node_instruction_1270(bytes, ctx),
        2 => match_node_instruction_1271(bytes, ctx),
        3 => match_node_instruction_1272(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1269(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1269: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1270(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1270: Terminal matched constructor ID 371");
    371
}

fn match_node_instruction_1271(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1271: Terminal matched constructor ID 364");
    364
}

fn match_node_instruction_1272(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1272: Terminal matched constructor ID 370");
    370
}

fn match_node_instruction_1273(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1273: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1274(bytes, ctx),
        1 => match_node_instruction_1279(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1274(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1274: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1275(bytes, ctx),
        1 => match_node_instruction_1278(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1275(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 1275: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1276(bytes, ctx),
        1 => match_node_instruction_1277(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1276(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1276: Terminal matched constructor ID 391");
    391
}

fn match_node_instruction_1277(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1277: Terminal matched constructor ID 318");
    318
}

fn match_node_instruction_1278(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1278: Terminal matched constructor ID 391");
    391
}

fn match_node_instruction_1279(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 3;
    eprintln!("Trace node 1279: SlaInstructionBits start=14, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1280(bytes, ctx),
        1 => match_node_instruction_1281(bytes, ctx),
        2 => match_node_instruction_1282(bytes, ctx),
        3 => match_node_instruction_1283(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1280(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1280: Terminal matched constructor ID 418");
    418
}

fn match_node_instruction_1281(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1281: Terminal matched constructor ID 391");
    391
}

fn match_node_instruction_1282(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1282: Terminal matched constructor ID 416");
    416
}

fn match_node_instruction_1283(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1283: Terminal matched constructor ID 417");
    417
}

fn match_node_instruction_1284(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1284: Terminal matched constructor ID 384");
    384
}

fn match_node_instruction_1285(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1285: Terminal matched constructor ID 311");
    311
}

fn match_node_instruction_1286(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 15;
    eprintln!("Trace node 1286: SlaInstructionBits start=9, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1287(bytes, ctx),
        1 => match_node_instruction_1288(bytes, ctx),
        2 => match_node_instruction_1289(bytes, ctx),
        3 => match_node_instruction_1290(bytes, ctx),
        4 => match_node_instruction_1291(bytes, ctx),
        5 => match_node_instruction_1292(bytes, ctx),
        6 => match_node_instruction_1293(bytes, ctx),
        7 => match_node_instruction_1294(bytes, ctx),
        8 => match_node_instruction_1295(bytes, ctx),
        9 => match_node_instruction_1296(bytes, ctx),
        10 => match_node_instruction_1297(bytes, ctx),
        11 => match_node_instruction_1298(bytes, ctx),
        12 => match_node_instruction_1299(bytes, ctx),
        13 => match_node_instruction_1300(bytes, ctx),
        14 => match_node_instruction_1301(bytes, ctx),
        15 => match_node_instruction_1302(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1287(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1287: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1288(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1288: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1289(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1289: Terminal matched constructor ID 388");
    388
}

fn match_node_instruction_1290(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1290: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1291(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1291: Terminal matched constructor ID 388");
    388
}

fn match_node_instruction_1292(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1292: Terminal matched constructor ID 388");
    388
}

fn match_node_instruction_1293(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1293: Terminal matched constructor ID 388");
    388
}

fn match_node_instruction_1294(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1294: Terminal matched constructor ID 388");
    388
}

fn match_node_instruction_1295(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1295: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1296(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1296: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1297(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1297: Terminal matched constructor ID 387");
    387
}

fn match_node_instruction_1298(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1298: Terminal matched constructor ID 387");
    387
}

fn match_node_instruction_1299(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1299: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1300(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1300: Terminal matched constructor ID 387");
    387
}

fn match_node_instruction_1301(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1301: Terminal matched constructor ID 387");
    387
}

fn match_node_instruction_1302(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1302: Terminal matched constructor ID 387");
    387
}

fn match_node_instruction_1303(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 7;
    eprintln!("Trace node 1303: SlaInstructionBits start=10, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1304(bytes, ctx),
        1 => match_node_instruction_1305(bytes, ctx),
        2 => match_node_instruction_1306(bytes, ctx),
        3 => match_node_instruction_1307(bytes, ctx),
        4 => match_node_instruction_1308(bytes, ctx),
        5 => match_node_instruction_1309(bytes, ctx),
        6 => match_node_instruction_1310(bytes, ctx),
        7 => match_node_instruction_1311(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1304(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1304: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1305(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1305: Terminal matched constructor ID 91");
    91
}

fn match_node_instruction_1306(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1306: Terminal matched constructor ID 92");
    92
}

fn match_node_instruction_1307(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1307: Terminal matched constructor ID 93");
    93
}

fn match_node_instruction_1308(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1308: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1309(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1309: Terminal matched constructor ID 94");
    94
}

fn match_node_instruction_1310(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1310: Terminal matched constructor ID 95");
    95
}

fn match_node_instruction_1311(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1311: Terminal matched constructor ID 96");
    96
}

fn match_node_instruction_1312(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 15;
    eprintln!("Trace node 1312: SlaInstructionBits start=9, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1313(bytes, ctx),
        1 => match_node_instruction_1314(bytes, ctx),
        2 => match_node_instruction_1315(bytes, ctx),
        3 => match_node_instruction_1316(bytes, ctx),
        4 => match_node_instruction_1317(bytes, ctx),
        5 => match_node_instruction_1318(bytes, ctx),
        6 => match_node_instruction_1319(bytes, ctx),
        7 => match_node_instruction_1320(bytes, ctx),
        8 => match_node_instruction_1321(bytes, ctx),
        9 => match_node_instruction_1322(bytes, ctx),
        10 => match_node_instruction_1323(bytes, ctx),
        11 => match_node_instruction_1324(bytes, ctx),
        12 => match_node_instruction_1325(bytes, ctx),
        13 => match_node_instruction_1326(bytes, ctx),
        14 => match_node_instruction_1327(bytes, ctx),
        15 => match_node_instruction_1328(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1313(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1313: Terminal matched constructor ID 230");
    230
}

fn match_node_instruction_1314(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1314: Terminal matched constructor ID 231");
    231
}

fn match_node_instruction_1315(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1315: Terminal matched constructor ID 232");
    232
}

fn match_node_instruction_1316(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1316: Terminal matched constructor ID 233");
    233
}

fn match_node_instruction_1317(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1317: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1318(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1318: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1319(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1319: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1320(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1320: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1321(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1321: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1322(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1322: Terminal matched constructor ID 235");
    235
}

fn match_node_instruction_1323(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1323: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1324(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1324: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1325(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1325: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1326(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1326: Terminal matched constructor ID 234");
    234
}

fn match_node_instruction_1327(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1327: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1328(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1328: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1329(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 7;
    eprintln!("Trace node 1329: SlaInstructionBits start=10, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1330(bytes, ctx),
        1 => match_node_instruction_1331(bytes, ctx),
        2 => match_node_instruction_1332(bytes, ctx),
        3 => match_node_instruction_1333(bytes, ctx),
        4 => match_node_instruction_1334(bytes, ctx),
        5 => match_node_instruction_1335(bytes, ctx),
        6 => match_node_instruction_1336(bytes, ctx),
        7 => match_node_instruction_1337(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1330(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1330: Terminal matched constructor ID 170");
    170
}

fn match_node_instruction_1331(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1331: Terminal matched constructor ID 171");
    171
}

fn match_node_instruction_1332(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1332: Terminal matched constructor ID 172");
    172
}

fn match_node_instruction_1333(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1333: Terminal matched constructor ID 173");
    173
}

fn match_node_instruction_1334(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1334: Terminal matched constructor ID 169");
    169
}

fn match_node_instruction_1335(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1335: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1336(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1336: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1337(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1337: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_1338(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1338: Terminal matched NOTHING");
    -1
}

fn match_node_kfact_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=3, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_kfact_1(bytes, ctx),
        1 => match_node_kfact_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_kfact_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_kfact_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_m2fpC0_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=5, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2fpC0_1(bytes, ctx),
        1 => match_node_m2fpC0_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2fpC0_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2fpC0_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2fpC1_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2fpC1_1(bytes, ctx),
        1 => match_node_m2fpC1_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2fpC1_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2fpC1_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2fpC2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=3, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2fpC2_1(bytes, ctx),
        1 => match_node_m2fpC2_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2fpC2_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2fpC2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2fpF0_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2fpF0_1(bytes, ctx),
        1 => match_node_m2fpF0_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2fpF0_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2fpF0_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2fpF1_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2fpF1_1(bytes, ctx),
        1 => match_node_m2fpF1_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2fpF1_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2fpF1_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2fpF2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2fpF2_1(bytes, ctx),
        1 => match_node_m2fpF2_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2fpF2_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2fpF2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2fpF3_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2fpF3_1(bytes, ctx),
        1 => match_node_m2fpF3_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2fpF3_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2fpF3_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2fpF4_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2fpF4_1(bytes, ctx),
        1 => match_node_m2fpF4_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2fpF4_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2fpF4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2fpF5_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2fpF5_1(bytes, ctx),
        1 => match_node_m2fpF5_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2fpF5_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2fpF5_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2fpF6_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2fpF6_1(bytes, ctx),
        1 => match_node_m2fpF6_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2fpF6_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2fpF6_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2fpF7_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2fpF7_1(bytes, ctx),
        1 => match_node_m2fpF7_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2fpF7_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2fpF7_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2fpR0_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2fpR0_1(bytes, ctx),
        1 => match_node_m2fpR0_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2fpR0_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2fpR0_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2fpR1_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2fpR1_1(bytes, ctx),
        1 => match_node_m2fpR1_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2fpR1_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2fpR1_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2fpR2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2fpR2_1(bytes, ctx),
        1 => match_node_m2fpR2_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2fpR2_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2fpR2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2fpR3_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2fpR3_1(bytes, ctx),
        1 => match_node_m2fpR3_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2fpR3_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2fpR3_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2fpR4_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2fpR4_1(bytes, ctx),
        1 => match_node_m2fpR4_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2fpR4_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2fpR4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2fpR5_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2fpR5_1(bytes, ctx),
        1 => match_node_m2fpR5_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2fpR5_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2fpR5_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2fpR6_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2fpR6_1(bytes, ctx),
        1 => match_node_m2fpR6_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2fpR6_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2fpR6_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2fpR7_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2fpR7_1(bytes, ctx),
        1 => match_node_m2fpR7_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2fpR7_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2fpR7_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2rfl0_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=0, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2rfl0_1(bytes, ctx),
        1 => match_node_m2rfl0_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2rfl0_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2rfl0_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2rfl1_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2rfl1_1(bytes, ctx),
        1 => match_node_m2rfl1_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2rfl1_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2rfl1_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2rfl2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=2, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2rfl2_1(bytes, ctx),
        1 => match_node_m2rfl2_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2rfl2_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2rfl2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2rfl3_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=3, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2rfl3_1(bytes, ctx),
        1 => match_node_m2rfl3_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2rfl3_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2rfl3_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2rfl4_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2rfl4_1(bytes, ctx),
        1 => match_node_m2rfl4_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2rfl4_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2rfl4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2rfl5_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=5, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2rfl5_1(bytes, ctx),
        1 => match_node_m2rfl5_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2rfl5_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2rfl5_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2rfl6_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=6, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2rfl6_1(bytes, ctx),
        1 => match_node_m2rfl6_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2rfl6_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2rfl6_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2rfl7_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2rfl7_1(bytes, ctx),
        1 => match_node_m2rfl7_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2rfl7_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2rfl7_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2rfl8_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2rfl8_1(bytes, ctx),
        1 => match_node_m2rfl8_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2rfl8_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2rfl8_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2rfl9_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2rfl9_1(bytes, ctx),
        1 => match_node_m2rfl9_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2rfl9_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2rfl9_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2rfla_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2rfla_1(bytes, ctx),
        1 => match_node_m2rfla_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2rfla_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2rfla_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2rflb_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2rflb_1(bytes, ctx),
        1 => match_node_m2rflb_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2rflb_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2rflb_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2rflc_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2rflc_1(bytes, ctx),
        1 => match_node_m2rflc_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2rflc_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2rflc_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2rfld_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2rfld_1(bytes, ctx),
        1 => match_node_m2rfld_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2rfld_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2rfld_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2rfle_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2rfle_1(bytes, ctx),
        1 => match_node_m2rfle_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2rfle_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2rfle_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2rflf_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2rflf_1(bytes, ctx),
        1 => match_node_m2rflf_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2rflf_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2rflf_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2rfw0_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=0, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2rfw0_1(bytes, ctx),
        1 => match_node_m2rfw0_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2rfw0_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2rfw0_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2rfw1_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2rfw1_1(bytes, ctx),
        1 => match_node_m2rfw1_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2rfw1_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2rfw1_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2rfw2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=2, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2rfw2_1(bytes, ctx),
        1 => match_node_m2rfw2_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2rfw2_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2rfw2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2rfw3_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=3, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2rfw3_1(bytes, ctx),
        1 => match_node_m2rfw3_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2rfw3_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2rfw3_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2rfw4_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2rfw4_1(bytes, ctx),
        1 => match_node_m2rfw4_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2rfw4_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2rfw4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2rfw5_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=5, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2rfw5_1(bytes, ctx),
        1 => match_node_m2rfw5_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2rfw5_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2rfw5_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2rfw6_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=6, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2rfw6_1(bytes, ctx),
        1 => match_node_m2rfw6_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2rfw6_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2rfw6_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2rfw7_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2rfw7_1(bytes, ctx),
        1 => match_node_m2rfw7_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2rfw7_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2rfw7_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2rfw8_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2rfw8_1(bytes, ctx),
        1 => match_node_m2rfw8_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2rfw8_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2rfw8_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2rfw9_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2rfw9_1(bytes, ctx),
        1 => match_node_m2rfw9_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2rfw9_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2rfw9_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2rfwa_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2rfwa_1(bytes, ctx),
        1 => match_node_m2rfwa_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2rfwa_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2rfwa_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2rfwb_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2rfwb_1(bytes, ctx),
        1 => match_node_m2rfwb_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2rfwb_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2rfwb_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2rfwc_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2rfwc_1(bytes, ctx),
        1 => match_node_m2rfwc_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2rfwc_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2rfwc_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2rfwd_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2rfwd_1(bytes, ctx),
        1 => match_node_m2rfwd_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2rfwd_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2rfwd_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2rfwe_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2rfwe_1(bytes, ctx),
        1 => match_node_m2rfwe_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2rfwe_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2rfwe_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m2rfwf_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m2rfwf_1(bytes, ctx),
        1 => match_node_m2rfwf_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m2rfwf_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_m2rfwf_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_m_eal_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 3;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_m_eal_1(bytes, ctx),
        1 => match_node_m_eal_2(bytes, ctx),
        2 => match_node_m_eal_3(bytes, ctx),
        3 => match_node_m_eal_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_m_eal_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_m_eal_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 3");
    3
}

fn match_node_m_eal_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 0");
    0
}

fn match_node_m_eal_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 1");
    1
}

fn match_node_macregx_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_macregx_1(bytes, ctx),
        1 => match_node_macregx_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_macregx_1(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 1;
    eprintln!("Trace node 1: SlaInstructionBits start=24, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_macregx_2(bytes, ctx),
        1 => match_node_macregx_3(bytes, ctx),
        _ => -1,
    }
}

fn match_node_macregx_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_macregx_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 1");
    1
}

fn match_node_macregx_4(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 1;
    eprintln!("Trace node 4: SlaInstructionBits start=24, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_macregx_5(bytes, ctx),
        1 => match_node_macregx_6(bytes, ctx),
        _ => -1,
    }
}

fn match_node_macregx_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 2");
    2
}

fn match_node_macregx_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 3");
    3
}

fn match_node_macregxl_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_macregxl_1(bytes, ctx),
        1 => match_node_macregxl_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_macregxl_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_macregxl_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_macregy_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=25, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_macregy_1(bytes, ctx),
        1 => match_node_macregy_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_macregy_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_macregy_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_macregy_e_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_macregyl_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_macregyl_1(bytes, ctx),
        1 => match_node_macregyl_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_macregyl_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_macregyl_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_macrw_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_macrw_1(bytes, ctx),
        1 => match_node_macrw_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_macrw_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_macrw_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_moveaccreg_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 3;
    eprintln!("Trace node 0: SlaInstructionBits start=5, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_moveaccreg_1(bytes, ctx),
        1 => match_node_moveaccreg_2(bytes, ctx),
        2 => match_node_moveaccreg_3(bytes, ctx),
        3 => match_node_moveaccreg_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_moveaccreg_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_moveaccreg_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_moveaccreg_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_moveaccreg_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_moveaccreg2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 3;
    eprintln!("Trace node 0: SlaInstructionBits start=14, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_moveaccreg2_1(bytes, ctx),
        1 => match_node_moveaccreg2_2(bytes, ctx),
        2 => match_node_moveaccreg2_3(bytes, ctx),
        3 => match_node_moveaccreg2_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_moveaccreg2_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_moveaccreg2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_moveaccreg2_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_moveaccreg2_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_movemOp_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 7;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_movemOp_1(bytes, ctx),
        1 => match_node_movemOp_2(bytes, ctx),
        2 => match_node_movemOp_3(bytes, ctx),
        3 => match_node_movemOp_4(bytes, ctx),
        4 => match_node_movemOp_5(bytes, ctx),
        5 => match_node_movemOp_6(bytes, ctx),
        6 => match_node_movemOp_7(bytes, ctx),
        7 => match_node_movemOp_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_movemOp_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched NOTHING");
    -1
}

fn match_node_movemOp_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched NOTHING");
    -1
}

fn match_node_movemOp_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 0");
    0
}

fn match_node_movemOp_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 1");
    1
}

fn match_node_movemOp_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 2");
    2
}

fn match_node_movemOp_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 3");
    3
}

fn match_node_movemOp_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 4");
    4
}

fn match_node_movemOp_8(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 3;
    eprintln!("Trace node 8: SlaInstructionBits start=14, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_movemOp_9(bytes, ctx),
        1 => match_node_movemOp_10(bytes, ctx),
        2 => match_node_movemOp_11(bytes, ctx),
        3 => match_node_movemOp_12(bytes, ctx),
        _ => -1,
    }
}

fn match_node_movemOp_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 5");
    5
}

fn match_node_movemOp_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 6");
    6
}

fn match_node_movemOp_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 7");
    7
}

fn match_node_movemOp_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 8");
    8
}

fn match_node_movemWrt_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_movemWrt_1(bytes, ctx),
        1 => match_node_movemWrt_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_movemWrt_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_movemWrt_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_mulsize_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_mulsize_1(bytes, ctx),
        1 => match_node_mulsize_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_mulsize_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_mulsize_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mbl0_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=0, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mbl0_1(bytes, ctx),
        1 => match_node_r2mbl0_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mbl0_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mbl0_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mbl1_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mbl1_1(bytes, ctx),
        1 => match_node_r2mbl1_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mbl1_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mbl1_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mbl2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=2, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mbl2_1(bytes, ctx),
        1 => match_node_r2mbl2_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mbl2_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mbl2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mbl3_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=3, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mbl3_1(bytes, ctx),
        1 => match_node_r2mbl3_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mbl3_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mbl3_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mbl4_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mbl4_1(bytes, ctx),
        1 => match_node_r2mbl4_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mbl4_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mbl4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mbl5_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=5, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mbl5_1(bytes, ctx),
        1 => match_node_r2mbl5_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mbl5_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mbl5_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mbl6_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=6, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mbl6_1(bytes, ctx),
        1 => match_node_r2mbl6_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mbl6_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mbl6_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mbl7_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mbl7_1(bytes, ctx),
        1 => match_node_r2mbl7_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mbl7_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mbl7_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mbl8_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mbl8_1(bytes, ctx),
        1 => match_node_r2mbl8_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mbl8_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mbl8_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mbl9_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mbl9_1(bytes, ctx),
        1 => match_node_r2mbl9_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mbl9_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mbl9_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mbla_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mbla_1(bytes, ctx),
        1 => match_node_r2mbla_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mbla_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mbla_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mblb_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mblb_1(bytes, ctx),
        1 => match_node_r2mblb_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mblb_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mblb_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mblc_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mblc_1(bytes, ctx),
        1 => match_node_r2mblc_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mblc_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mblc_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mbld_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mbld_1(bytes, ctx),
        1 => match_node_r2mbld_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mbld_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mbld_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mble_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mble_1(bytes, ctx),
        1 => match_node_r2mble_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mble_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mble_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mblf_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mblf_1(bytes, ctx),
        1 => match_node_r2mblf_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mblf_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mblf_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mbw0_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=0, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mbw0_1(bytes, ctx),
        1 => match_node_r2mbw0_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mbw0_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mbw0_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mbw1_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mbw1_1(bytes, ctx),
        1 => match_node_r2mbw1_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mbw1_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mbw1_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mbw2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=2, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mbw2_1(bytes, ctx),
        1 => match_node_r2mbw2_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mbw2_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mbw2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mbw3_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=3, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mbw3_1(bytes, ctx),
        1 => match_node_r2mbw3_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mbw3_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mbw3_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mbw4_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mbw4_1(bytes, ctx),
        1 => match_node_r2mbw4_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mbw4_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mbw4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mbw5_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=5, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mbw5_1(bytes, ctx),
        1 => match_node_r2mbw5_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mbw5_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mbw5_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mbw6_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=6, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mbw6_1(bytes, ctx),
        1 => match_node_r2mbw6_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mbw6_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mbw6_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mbw7_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mbw7_1(bytes, ctx),
        1 => match_node_r2mbw7_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mbw7_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mbw7_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mbw8_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mbw8_1(bytes, ctx),
        1 => match_node_r2mbw8_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mbw8_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mbw8_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mbw9_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mbw9_1(bytes, ctx),
        1 => match_node_r2mbw9_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mbw9_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mbw9_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mbwa_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mbwa_1(bytes, ctx),
        1 => match_node_r2mbwa_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mbwa_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mbwa_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mbwb_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mbwb_1(bytes, ctx),
        1 => match_node_r2mbwb_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mbwb_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mbwb_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mbwc_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mbwc_1(bytes, ctx),
        1 => match_node_r2mbwc_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mbwc_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mbwc_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mbwd_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mbwd_1(bytes, ctx),
        1 => match_node_r2mbwd_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mbwd_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mbwd_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mbwe_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mbwe_1(bytes, ctx),
        1 => match_node_r2mbwe_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mbwe_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mbwe_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mbwf_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mbwf_1(bytes, ctx),
        1 => match_node_r2mbwf_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mbwf_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mbwf_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mfl0_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=0, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mfl0_1(bytes, ctx),
        1 => match_node_r2mfl0_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mfl0_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mfl0_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mfl1_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mfl1_1(bytes, ctx),
        1 => match_node_r2mfl1_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mfl1_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mfl1_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mfl2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=2, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mfl2_1(bytes, ctx),
        1 => match_node_r2mfl2_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mfl2_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mfl2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mfl3_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=3, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mfl3_1(bytes, ctx),
        1 => match_node_r2mfl3_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mfl3_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mfl3_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mfl4_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mfl4_1(bytes, ctx),
        1 => match_node_r2mfl4_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mfl4_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mfl4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mfl5_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=5, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mfl5_1(bytes, ctx),
        1 => match_node_r2mfl5_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mfl5_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mfl5_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mfl6_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=6, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mfl6_1(bytes, ctx),
        1 => match_node_r2mfl6_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mfl6_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mfl6_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mfl7_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mfl7_1(bytes, ctx),
        1 => match_node_r2mfl7_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mfl7_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mfl7_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mfl8_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mfl8_1(bytes, ctx),
        1 => match_node_r2mfl8_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mfl8_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mfl8_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mfl9_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mfl9_1(bytes, ctx),
        1 => match_node_r2mfl9_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mfl9_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mfl9_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mfla_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mfla_1(bytes, ctx),
        1 => match_node_r2mfla_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mfla_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mfla_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mflb_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mflb_1(bytes, ctx),
        1 => match_node_r2mflb_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mflb_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mflb_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mflc_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mflc_1(bytes, ctx),
        1 => match_node_r2mflc_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mflc_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mflc_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mfld_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mfld_1(bytes, ctx),
        1 => match_node_r2mfld_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mfld_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mfld_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mfle_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mfle_1(bytes, ctx),
        1 => match_node_r2mfle_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mfle_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mfle_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mflf_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mflf_1(bytes, ctx),
        1 => match_node_r2mflf_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mflf_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mflf_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mfw0_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=0, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mfw0_1(bytes, ctx),
        1 => match_node_r2mfw0_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mfw0_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mfw0_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mfw1_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mfw1_1(bytes, ctx),
        1 => match_node_r2mfw1_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mfw1_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mfw1_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mfw2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=2, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mfw2_1(bytes, ctx),
        1 => match_node_r2mfw2_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mfw2_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mfw2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mfw3_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=3, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mfw3_1(bytes, ctx),
        1 => match_node_r2mfw3_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mfw3_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mfw3_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mfw4_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mfw4_1(bytes, ctx),
        1 => match_node_r2mfw4_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mfw4_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mfw4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mfw5_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=5, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mfw5_1(bytes, ctx),
        1 => match_node_r2mfw5_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mfw5_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mfw5_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mfw6_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=6, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mfw6_1(bytes, ctx),
        1 => match_node_r2mfw6_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mfw6_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mfw6_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mfw7_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mfw7_1(bytes, ctx),
        1 => match_node_r2mfw7_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mfw7_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mfw7_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mfw8_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mfw8_1(bytes, ctx),
        1 => match_node_r2mfw8_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mfw8_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mfw8_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mfw9_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mfw9_1(bytes, ctx),
        1 => match_node_r2mfw9_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mfw9_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mfw9_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mfwa_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mfwa_1(bytes, ctx),
        1 => match_node_r2mfwa_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mfwa_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mfwa_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mfwb_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mfwb_1(bytes, ctx),
        1 => match_node_r2mfwb_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mfwb_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mfwb_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mfwc_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mfwc_1(bytes, ctx),
        1 => match_node_r2mfwc_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mfwc_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mfwc_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mfwd_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mfwd_1(bytes, ctx),
        1 => match_node_r2mfwd_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mfwd_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mfwd_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mfwe_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mfwe_1(bytes, ctx),
        1 => match_node_r2mfwe_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mfwe_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mfwe_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_r2mfwf_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_r2mfwf_1(bytes, ctx),
        1 => match_node_r2mfwf_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_r2mfwf_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_r2mfwf_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_reg9Plus_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_regParen_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_regPlus_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_regxPlus_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_remyes_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_remyes_1(bytes, ctx),
        1 => match_node_remyes_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_remyes_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_remyes_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_romconst_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 7;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_romconst_1(bytes, ctx),
        1 => match_node_romconst_2(bytes, ctx),
        2 => match_node_romconst_3(bytes, ctx),
        3 => match_node_romconst_4(bytes, ctx),
        4 => match_node_romconst_5(bytes, ctx),
        5 => match_node_romconst_6(bytes, ctx),
        6 => match_node_romconst_7(bytes, ctx),
        7 => match_node_romconst_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_romconst_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_romconst_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 7");
    7
}

fn match_node_romconst_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_romconst_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_romconst_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 4");
    4
}

fn match_node_romconst_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_romconst_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 6");
    6
}

fn match_node_romconst_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 1");
    1
}

fn match_node_rreg_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=0, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_rreg_1(bytes, ctx),
        1 => match_node_rreg_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_rreg_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_rreg_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_scalefactor_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 3;
    eprintln!("Trace node 0: SlaInstructionBits start=5, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_scalefactor_1(bytes, ctx),
        1 => match_node_scalefactor_2(bytes, ctx),
        2 => match_node_scalefactor_3(bytes, ctx),
        3 => match_node_scalefactor_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_scalefactor_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_scalefactor_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_scalefactor_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched NOTHING");
    -1
}

fn match_node_scalefactor_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 2");
    2
}

fn match_node_skip_addr_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_skip_addr_1(bytes, ctx),
        1 => match_node_skip_addr_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_skip_addr_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_skip_addr_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_subdiv_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 3;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_subdiv_1(bytes, ctx),
        1 => match_node_subdiv_2(bytes, ctx),
        2 => match_node_subdiv_3(bytes, ctx),
        3 => match_node_subdiv_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_subdiv_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_subdiv_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_subdiv_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_subdiv_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_submul_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 3;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_submul_1(bytes, ctx),
        1 => match_node_submul_2(bytes, ctx),
        2 => match_node_submul_3(bytes, ctx),
        3 => match_node_submul_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_submul_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_submul_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 3");
    3
}

fn match_node_submul_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 0");
    0
}

fn match_node_submul_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 1");
    1
}

fn match_node_with_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 0: InstructionBitSlice offset=0, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_1(bytes, ctx),
        1 => match_node_with_266(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_1(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 2) >> 1;
    eprintln!("Trace node 1: InstructionBitSlice offset=0, mask=2, probe={}", probe);
    match probe {
        0 => match_node_with_2(bytes, ctx),
        1 => match_node_with_237(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_2(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 4) >> 2;
    eprintln!("Trace node 2: InstructionBitSlice offset=0, mask=4, probe={}", probe);
    match probe {
        0 => match_node_with_3(bytes, ctx),
        1 => match_node_with_214(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_3(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 3: InstructionBitSlice offset=0, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_4(bytes, ctx),
        1 => match_node_with_141(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_4(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 16) >> 4;
    eprintln!("Trace node 4: InstructionBitSlice offset=0, mask=16, probe={}", probe);
    match probe {
        0 => match_node_with_5(bytes, ctx),
        1 => match_node_with_98(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_5(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 32) >> 5;
    eprintln!("Trace node 5: InstructionBitSlice offset=0, mask=32, probe={}", probe);
    match probe {
        0 => match_node_with_6(bytes, ctx),
        1 => match_node_with_69(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_6(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 64) >> 6;
    eprintln!("Trace node 6: InstructionBitSlice offset=0, mask=64, probe={}", probe);
    match probe {
        0 => match_node_with_7(bytes, ctx),
        1 => match_node_with_48(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_7(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 128) >> 7;
    eprintln!("Trace node 7: InstructionBitSlice offset=0, mask=128, probe={}", probe);
    match probe {
        0 => match_node_with_8(bytes, ctx),
        1 => match_node_with_33(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_8(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 8: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_9(bytes, ctx),
        1 => match_node_with_24(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_9(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 4) >> 2;
    eprintln!("Trace node 9: InstructionBitSlice offset=1, mask=4, probe={}", probe);
    match probe {
        0 => match_node_with_10(bytes, ctx),
        1 => match_node_with_23(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_10(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 10: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_11(bytes, ctx),
        1 => match_node_with_22(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_11(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 16) >> 4;
    eprintln!("Trace node 11: InstructionBitSlice offset=1, mask=16, probe={}", probe);
    match probe {
        0 => match_node_with_12(bytes, ctx),
        1 => match_node_with_21(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_12(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 32) >> 5;
    eprintln!("Trace node 12: InstructionBitSlice offset=1, mask=32, probe={}", probe);
    match probe {
        0 => match_node_with_13(bytes, ctx),
        1 => match_node_with_18(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_13(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 64) >> 6;
    eprintln!("Trace node 13: InstructionBitSlice offset=1, mask=64, probe={}", probe);
    match probe {
        0 => match_node_with_14(bytes, ctx),
        1 => match_node_with_17(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_14(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 128) >> 7;
    eprintln!("Trace node 14: InstructionBitSlice offset=1, mask=128, probe={}", probe);
    match probe {
        0 => match_node_with_15(bytes, ctx),
        1 => match_node_with_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 10");
    10
}

fn match_node_with_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 12");
    12
}

fn match_node_with_17(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 17: Terminal matched constructor ID 97");
    97
}

fn match_node_with_18(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 64) >> 6;
    eprintln!("Trace node 18: InstructionBitSlice offset=1, mask=64, probe={}", probe);
    match probe {
        0 => match_node_with_19(bytes, ctx),
        1 => match_node_with_20(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_19(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 19: Terminal matched constructor ID 5");
    5
}

fn match_node_with_20(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 20: Terminal matched constructor ID 97");
    97
}

fn match_node_with_21(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 21: Terminal matched constructor ID 25");
    25
}

fn match_node_with_22(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 22: Terminal matched constructor ID 81");
    81
}

fn match_node_with_23(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 23: Terminal matched constructor ID 1");
    1
}

fn match_node_with_24(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 2) >> 1;
    eprintln!("Trace node 24: InstructionBitSlice offset=1, mask=2, probe={}", probe);
    match probe {
        0 => match_node_with_25(bytes, ctx),
        1 => match_node_with_30(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_25(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 4) >> 2;
    eprintln!("Trace node 25: InstructionBitSlice offset=1, mask=4, probe={}", probe);
    match probe {
        0 => match_node_with_26(bytes, ctx),
        1 => match_node_with_29(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_26(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 26: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_27(bytes, ctx),
        1 => match_node_with_28(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_27(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 27: Terminal matched constructor ID 95");
    95
}

fn match_node_with_28(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 28: Terminal matched constructor ID 89");
    89
}

fn match_node_with_29(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 29: Terminal matched constructor ID 97");
    97
}

fn match_node_with_30(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 4) >> 2;
    eprintln!("Trace node 30: InstructionBitSlice offset=1, mask=4, probe={}", probe);
    match probe {
        0 => match_node_with_31(bytes, ctx),
        1 => match_node_with_32(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_31(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 31: Terminal matched constructor ID 97");
    97
}

fn match_node_with_32(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 32: Terminal matched constructor ID 97");
    97
}

fn match_node_with_33(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 33: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_34(bytes, ctx),
        1 => match_node_with_37(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_34(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 34: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_35(bytes, ctx),
        1 => match_node_with_36(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_35(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 35: Terminal matched constructor ID 0");
    0
}

fn match_node_with_36(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 36: Terminal matched constructor ID 127");
    127
}

fn match_node_with_37(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 2) >> 1;
    eprintln!("Trace node 37: InstructionBitSlice offset=1, mask=2, probe={}", probe);
    match probe {
        0 => match_node_with_38(bytes, ctx),
        1 => match_node_with_45(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_38(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 4) >> 2;
    eprintln!("Trace node 38: InstructionBitSlice offset=1, mask=4, probe={}", probe);
    match probe {
        0 => match_node_with_39(bytes, ctx),
        1 => match_node_with_44(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_39(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 39: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_40(bytes, ctx),
        1 => match_node_with_43(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_40(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 32) >> 5;
    eprintln!("Trace node 40: InstructionBitSlice offset=1, mask=32, probe={}", probe);
    match probe {
        0 => match_node_with_41(bytes, ctx),
        1 => match_node_with_42(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_41(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 41: Terminal matched constructor ID 116");
    116
}

fn match_node_with_42(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 42: Terminal matched constructor ID 85");
    85
}

fn match_node_with_43(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 43: Terminal matched constructor ID 124");
    124
}

fn match_node_with_44(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 44: Terminal matched constructor ID 125");
    125
}

fn match_node_with_45(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 4) >> 2;
    eprintln!("Trace node 45: InstructionBitSlice offset=1, mask=4, probe={}", probe);
    match probe {
        0 => match_node_with_46(bytes, ctx),
        1 => match_node_with_47(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_46(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 46: Terminal matched constructor ID 87");
    87
}

fn match_node_with_47(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 47: Terminal matched constructor ID 88");
    88
}

fn match_node_with_48(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 128) >> 7;
    eprintln!("Trace node 48: InstructionBitSlice offset=0, mask=128, probe={}", probe);
    match probe {
        0 => match_node_with_49(bytes, ctx),
        1 => match_node_with_58(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_49(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 49: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_50(bytes, ctx),
        1 => match_node_with_55(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_50(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 50: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_51(bytes, ctx),
        1 => match_node_with_54(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_51(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 128) >> 7;
    eprintln!("Trace node 51: InstructionBitSlice offset=1, mask=128, probe={}", probe);
    match probe {
        0 => match_node_with_52(bytes, ctx),
        1 => match_node_with_53(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_52(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 52: Terminal matched constructor ID 75");
    75
}

fn match_node_with_53(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 53: Terminal matched constructor ID 126");
    126
}

fn match_node_with_54(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 54: Terminal matched constructor ID 82");
    82
}

fn match_node_with_55(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 55: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_56(bytes, ctx),
        1 => match_node_with_57(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_56(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 56: Terminal matched constructor ID 110");
    110
}

fn match_node_with_57(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 57: Terminal matched constructor ID 90");
    90
}

fn match_node_with_58(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 58: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_59(bytes, ctx),
        1 => match_node_with_64(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_59(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 2) >> 1;
    eprintln!("Trace node 59: InstructionBitSlice offset=1, mask=2, probe={}", probe);
    match probe {
        0 => match_node_with_60(bytes, ctx),
        1 => match_node_with_63(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_60(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 60: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_61(bytes, ctx),
        1 => match_node_with_62(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_61(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 61: Terminal matched constructor ID 21");
    21
}

fn match_node_with_62(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 62: Terminal matched constructor ID 128");
    128
}

fn match_node_with_63(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 63: Terminal matched constructor ID 73");
    73
}

fn match_node_with_64(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 4) >> 2;
    eprintln!("Trace node 64: InstructionBitSlice offset=1, mask=4, probe={}", probe);
    match probe {
        0 => match_node_with_65(bytes, ctx),
        1 => match_node_with_68(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_65(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 65: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_66(bytes, ctx),
        1 => match_node_with_67(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_66(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 66: Terminal matched constructor ID 128");
    128
}

fn match_node_with_67(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 67: Terminal matched constructor ID 128");
    128
}

fn match_node_with_68(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 68: Terminal matched constructor ID 78");
    78
}

fn match_node_with_69(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 64) >> 6;
    eprintln!("Trace node 69: InstructionBitSlice offset=0, mask=64, probe={}", probe);
    match probe {
        0 => match_node_with_70(bytes, ctx),
        1 => match_node_with_87(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_70(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 128) >> 7;
    eprintln!("Trace node 70: InstructionBitSlice offset=0, mask=128, probe={}", probe);
    match probe {
        0 => match_node_with_71(bytes, ctx),
        1 => match_node_with_82(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_71(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 71: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_72(bytes, ctx),
        1 => match_node_with_79(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_72(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 16) >> 4;
    eprintln!("Trace node 72: InstructionBitSlice offset=1, mask=16, probe={}", probe);
    match probe {
        0 => match_node_with_73(bytes, ctx),
        1 => match_node_with_78(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_73(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 64) >> 6;
    eprintln!("Trace node 73: InstructionBitSlice offset=1, mask=64, probe={}", probe);
    match probe {
        0 => match_node_with_74(bytes, ctx),
        1 => match_node_with_77(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_74(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 128) >> 7;
    eprintln!("Trace node 74: InstructionBitSlice offset=1, mask=128, probe={}", probe);
    match probe {
        0 => match_node_with_75(bytes, ctx),
        1 => match_node_with_76(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_75(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 75: Terminal matched constructor ID 141");
    141
}

fn match_node_with_76(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 76: Terminal matched constructor ID 158");
    158
}

fn match_node_with_77(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 77: Terminal matched constructor ID 74");
    74
}

fn match_node_with_78(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 78: Terminal matched constructor ID 8");
    8
}

fn match_node_with_79(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 79: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_80(bytes, ctx),
        1 => match_node_with_81(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_80(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 80: Terminal matched constructor ID 142");
    142
}

fn match_node_with_81(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 81: Terminal matched constructor ID 143");
    143
}

fn match_node_with_82(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 82: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_83(bytes, ctx),
        1 => match_node_with_84(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_83(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 83: Terminal matched constructor ID 133");
    133
}

fn match_node_with_84(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 84: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_85(bytes, ctx),
        1 => match_node_with_86(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_85(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 85: Terminal matched constructor ID 133");
    133
}

fn match_node_with_86(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 86: Terminal matched constructor ID 133");
    133
}

fn match_node_with_87(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 128) >> 7;
    eprintln!("Trace node 87: InstructionBitSlice offset=0, mask=128, probe={}", probe);
    match probe {
        0 => match_node_with_88(bytes, ctx),
        1 => match_node_with_93(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_88(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 88: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_89(bytes, ctx),
        1 => match_node_with_90(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_89(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 89: Terminal matched constructor ID 141");
    141
}

fn match_node_with_90(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 90: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_91(bytes, ctx),
        1 => match_node_with_92(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_91(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 91: Terminal matched constructor ID 142");
    142
}

fn match_node_with_92(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 92: Terminal matched constructor ID 143");
    143
}

fn match_node_with_93(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 93: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_94(bytes, ctx),
        1 => match_node_with_95(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_94(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 94: Terminal matched constructor ID 134");
    134
}

fn match_node_with_95(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 95: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_96(bytes, ctx),
        1 => match_node_with_97(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_96(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 96: Terminal matched constructor ID 134");
    134
}

fn match_node_with_97(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 97: Terminal matched constructor ID 134");
    134
}

fn match_node_with_98(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 32) >> 5;
    eprintln!("Trace node 98: InstructionBitSlice offset=0, mask=32, probe={}", probe);
    match probe {
        0 => match_node_with_99(bytes, ctx),
        1 => match_node_with_124(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_99(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 64) >> 6;
    eprintln!("Trace node 99: InstructionBitSlice offset=0, mask=64, probe={}", probe);
    match probe {
        0 => match_node_with_100(bytes, ctx),
        1 => match_node_with_113(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_100(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 128) >> 7;
    eprintln!("Trace node 100: InstructionBitSlice offset=0, mask=128, probe={}", probe);
    match probe {
        0 => match_node_with_101(bytes, ctx),
        1 => match_node_with_108(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_101(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 101: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_102(bytes, ctx),
        1 => match_node_with_103(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_102(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 102: Terminal matched constructor ID 141");
    141
}

fn match_node_with_103(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 103: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_104(bytes, ctx),
        1 => match_node_with_107(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_104(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 32) >> 5;
    eprintln!("Trace node 104: InstructionBitSlice offset=1, mask=32, probe={}", probe);
    match probe {
        0 => match_node_with_105(bytes, ctx),
        1 => match_node_with_106(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_105(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 105: Terminal matched constructor ID 142");
    142
}

fn match_node_with_106(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 106: Terminal matched constructor ID 86");
    86
}

fn match_node_with_107(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 107: Terminal matched constructor ID 143");
    143
}

fn match_node_with_108(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 108: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_109(bytes, ctx),
        1 => match_node_with_110(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_109(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 109: Terminal matched constructor ID 141");
    141
}

fn match_node_with_110(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 110: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_111(bytes, ctx),
        1 => match_node_with_112(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_111(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 111: Terminal matched constructor ID 142");
    142
}

fn match_node_with_112(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 112: Terminal matched constructor ID 143");
    143
}

fn match_node_with_113(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 128) >> 7;
    eprintln!("Trace node 113: InstructionBitSlice offset=0, mask=128, probe={}", probe);
    match probe {
        0 => match_node_with_114(bytes, ctx),
        1 => match_node_with_119(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_114(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 114: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_115(bytes, ctx),
        1 => match_node_with_116(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_115(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 115: Terminal matched constructor ID 141");
    141
}

fn match_node_with_116(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 116: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_117(bytes, ctx),
        1 => match_node_with_118(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_117(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 117: Terminal matched constructor ID 142");
    142
}

fn match_node_with_118(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 118: Terminal matched constructor ID 143");
    143
}

fn match_node_with_119(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 119: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_120(bytes, ctx),
        1 => match_node_with_121(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_120(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 120: Terminal matched constructor ID 141");
    141
}

fn match_node_with_121(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 121: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_122(bytes, ctx),
        1 => match_node_with_123(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_122(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 122: Terminal matched constructor ID 142");
    142
}

fn match_node_with_123(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 123: Terminal matched constructor ID 143");
    143
}

fn match_node_with_124(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 64) >> 6;
    eprintln!("Trace node 124: InstructionBitSlice offset=0, mask=64, probe={}", probe);
    match probe {
        0 => match_node_with_125(bytes, ctx),
        1 => match_node_with_136(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_125(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 128) >> 7;
    eprintln!("Trace node 125: InstructionBitSlice offset=0, mask=128, probe={}", probe);
    match probe {
        0 => match_node_with_126(bytes, ctx),
        1 => match_node_with_131(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_126(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 126: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_127(bytes, ctx),
        1 => match_node_with_128(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_127(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 127: Terminal matched constructor ID 6");
    6
}

fn match_node_with_128(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 128: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_129(bytes, ctx),
        1 => match_node_with_130(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_129(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 129: Terminal matched constructor ID 142");
    142
}

fn match_node_with_130(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 130: Terminal matched constructor ID 143");
    143
}

fn match_node_with_131(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 131: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_132(bytes, ctx),
        1 => match_node_with_133(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_132(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 132: Terminal matched constructor ID 141");
    141
}

fn match_node_with_133(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 133: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_134(bytes, ctx),
        1 => match_node_with_135(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_134(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 134: Terminal matched constructor ID 142");
    142
}

fn match_node_with_135(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 135: Terminal matched constructor ID 143");
    143
}

fn match_node_with_136(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 136: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_137(bytes, ctx),
        1 => match_node_with_138(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_137(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 137: Terminal matched constructor ID 141");
    141
}

fn match_node_with_138(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 138: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_139(bytes, ctx),
        1 => match_node_with_140(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_139(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 139: Terminal matched constructor ID 142");
    142
}

fn match_node_with_140(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 140: Terminal matched constructor ID 143");
    143
}

fn match_node_with_141(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 16) >> 4;
    eprintln!("Trace node 141: InstructionBitSlice offset=0, mask=16, probe={}", probe);
    match probe {
        0 => match_node_with_142(bytes, ctx),
        1 => match_node_with_177(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_142(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 32) >> 5;
    eprintln!("Trace node 142: InstructionBitSlice offset=0, mask=32, probe={}", probe);
    match probe {
        0 => match_node_with_143(bytes, ctx),
        1 => match_node_with_166(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_143(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 64) >> 6;
    eprintln!("Trace node 143: InstructionBitSlice offset=0, mask=64, probe={}", probe);
    match probe {
        0 => match_node_with_144(bytes, ctx),
        1 => match_node_with_155(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_144(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 128) >> 7;
    eprintln!("Trace node 144: InstructionBitSlice offset=0, mask=128, probe={}", probe);
    match probe {
        0 => match_node_with_145(bytes, ctx),
        1 => match_node_with_150(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_145(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 145: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_146(bytes, ctx),
        1 => match_node_with_147(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_146(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 146: Terminal matched constructor ID 141");
    141
}

fn match_node_with_147(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 147: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_148(bytes, ctx),
        1 => match_node_with_149(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_148(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 148: Terminal matched constructor ID 142");
    142
}

fn match_node_with_149(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 149: Terminal matched constructor ID 143");
    143
}

fn match_node_with_150(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 150: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_151(bytes, ctx),
        1 => match_node_with_152(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_151(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 151: Terminal matched constructor ID 141");
    141
}

fn match_node_with_152(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 152: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_153(bytes, ctx),
        1 => match_node_with_154(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_153(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 153: Terminal matched constructor ID 117");
    117
}

fn match_node_with_154(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 154: Terminal matched constructor ID 143");
    143
}

fn match_node_with_155(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 128) >> 7;
    eprintln!("Trace node 155: InstructionBitSlice offset=0, mask=128, probe={}", probe);
    match probe {
        0 => match_node_with_156(bytes, ctx),
        1 => match_node_with_161(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_156(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 156: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_157(bytes, ctx),
        1 => match_node_with_158(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_157(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 157: Terminal matched constructor ID 11");
    11
}

fn match_node_with_158(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 158: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_159(bytes, ctx),
        1 => match_node_with_160(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_159(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 159: Terminal matched constructor ID 111");
    111
}

fn match_node_with_160(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 160: Terminal matched constructor ID 143");
    143
}

fn match_node_with_161(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 161: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_162(bytes, ctx),
        1 => match_node_with_163(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_162(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 162: Terminal matched constructor ID 141");
    141
}

fn match_node_with_163(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 163: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_164(bytes, ctx),
        1 => match_node_with_165(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_164(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 164: Terminal matched constructor ID 142");
    142
}

fn match_node_with_165(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 165: Terminal matched constructor ID 143");
    143
}

fn match_node_with_166(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 64) >> 6;
    eprintln!("Trace node 166: InstructionBitSlice offset=0, mask=64, probe={}", probe);
    match probe {
        0 => match_node_with_167(bytes, ctx),
        1 => match_node_with_172(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_167(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 167: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_168(bytes, ctx),
        1 => match_node_with_169(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_168(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 168: Terminal matched constructor ID 141");
    141
}

fn match_node_with_169(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 169: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_170(bytes, ctx),
        1 => match_node_with_171(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_170(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 170: Terminal matched constructor ID 142");
    142
}

fn match_node_with_171(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 171: Terminal matched constructor ID 143");
    143
}

fn match_node_with_172(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 172: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_173(bytes, ctx),
        1 => match_node_with_174(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_173(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 173: Terminal matched constructor ID 141");
    141
}

fn match_node_with_174(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 174: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_175(bytes, ctx),
        1 => match_node_with_176(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_175(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 175: Terminal matched constructor ID 142");
    142
}

fn match_node_with_176(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 176: Terminal matched constructor ID 143");
    143
}

fn match_node_with_177(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 32) >> 5;
    eprintln!("Trace node 177: InstructionBitSlice offset=0, mask=32, probe={}", probe);
    match probe {
        0 => match_node_with_178(bytes, ctx),
        1 => match_node_with_197(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_178(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 64) >> 6;
    eprintln!("Trace node 178: InstructionBitSlice offset=0, mask=64, probe={}", probe);
    match probe {
        0 => match_node_with_179(bytes, ctx),
        1 => match_node_with_192(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_179(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 128) >> 7;
    eprintln!("Trace node 179: InstructionBitSlice offset=0, mask=128, probe={}", probe);
    match probe {
        0 => match_node_with_180(bytes, ctx),
        1 => match_node_with_187(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_180(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 180: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_181(bytes, ctx),
        1 => match_node_with_184(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_181(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 16) >> 4;
    eprintln!("Trace node 181: InstructionBitSlice offset=1, mask=16, probe={}", probe);
    match probe {
        0 => match_node_with_182(bytes, ctx),
        1 => match_node_with_183(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_182(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 182: Terminal matched constructor ID 141");
    141
}

fn match_node_with_183(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 183: Terminal matched constructor ID 7");
    7
}

fn match_node_with_184(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 184: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_185(bytes, ctx),
        1 => match_node_with_186(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_185(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 185: Terminal matched constructor ID 142");
    142
}

fn match_node_with_186(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 186: Terminal matched constructor ID 143");
    143
}

fn match_node_with_187(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 187: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_188(bytes, ctx),
        1 => match_node_with_189(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_188(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 188: Terminal matched constructor ID 141");
    141
}

fn match_node_with_189(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 189: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_190(bytes, ctx),
        1 => match_node_with_191(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_190(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 190: Terminal matched constructor ID 142");
    142
}

fn match_node_with_191(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 191: Terminal matched constructor ID 143");
    143
}

fn match_node_with_192(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 192: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_193(bytes, ctx),
        1 => match_node_with_194(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_193(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 193: Terminal matched constructor ID 141");
    141
}

fn match_node_with_194(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 194: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_195(bytes, ctx),
        1 => match_node_with_196(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_195(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 195: Terminal matched constructor ID 142");
    142
}

fn match_node_with_196(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 196: Terminal matched constructor ID 143");
    143
}

fn match_node_with_197(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 64) >> 6;
    eprintln!("Trace node 197: InstructionBitSlice offset=0, mask=64, probe={}", probe);
    match probe {
        0 => match_node_with_198(bytes, ctx),
        1 => match_node_with_209(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_198(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 128) >> 7;
    eprintln!("Trace node 198: InstructionBitSlice offset=0, mask=128, probe={}", probe);
    match probe {
        0 => match_node_with_199(bytes, ctx),
        1 => match_node_with_204(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_199(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 199: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_200(bytes, ctx),
        1 => match_node_with_201(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_200(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 200: Terminal matched constructor ID 141");
    141
}

fn match_node_with_201(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 201: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_202(bytes, ctx),
        1 => match_node_with_203(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_202(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 202: Terminal matched constructor ID 142");
    142
}

fn match_node_with_203(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 203: Terminal matched constructor ID 143");
    143
}

fn match_node_with_204(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 204: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_205(bytes, ctx),
        1 => match_node_with_206(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_205(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 205: Terminal matched constructor ID 107");
    107
}

fn match_node_with_206(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 206: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_207(bytes, ctx),
        1 => match_node_with_208(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_207(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 207: Terminal matched constructor ID 107");
    107
}

fn match_node_with_208(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 208: Terminal matched constructor ID 107");
    107
}

fn match_node_with_209(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 209: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_210(bytes, ctx),
        1 => match_node_with_211(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_210(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 210: Terminal matched constructor ID 104");
    104
}

fn match_node_with_211(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 211: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_212(bytes, ctx),
        1 => match_node_with_213(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_212(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 212: Terminal matched constructor ID 104");
    104
}

fn match_node_with_213(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 213: Terminal matched constructor ID 104");
    104
}

fn match_node_with_214(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 214: InstructionBitSlice offset=0, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_215(bytes, ctx),
        1 => match_node_with_220(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_215(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 215: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_216(bytes, ctx),
        1 => match_node_with_217(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_216(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 216: Terminal matched constructor ID 112");
    112
}

fn match_node_with_217(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 217: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_218(bytes, ctx),
        1 => match_node_with_219(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_218(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 218: Terminal matched constructor ID 112");
    112
}

fn match_node_with_219(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 219: Terminal matched constructor ID 112");
    112
}

fn match_node_with_220(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 64) >> 6;
    eprintln!("Trace node 220: InstructionBitSlice offset=0, mask=64, probe={}", probe);
    match probe {
        0 => match_node_with_221(bytes, ctx),
        1 => match_node_with_226(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_221(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 221: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_222(bytes, ctx),
        1 => match_node_with_223(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_222(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 222: Terminal matched constructor ID 109");
    109
}

fn match_node_with_223(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 223: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_224(bytes, ctx),
        1 => match_node_with_225(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_224(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 224: Terminal matched constructor ID 109");
    109
}

fn match_node_with_225(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 225: Terminal matched constructor ID 109");
    109
}

fn match_node_with_226(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 128) >> 7;
    eprintln!("Trace node 226: InstructionBitSlice offset=0, mask=128, probe={}", probe);
    match probe {
        0 => match_node_with_227(bytes, ctx),
        1 => match_node_with_232(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_227(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 227: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_228(bytes, ctx),
        1 => match_node_with_229(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_228(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 228: Terminal matched constructor ID 4");
    4
}

fn match_node_with_229(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 229: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_230(bytes, ctx),
        1 => match_node_with_231(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_230(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 230: Terminal matched constructor ID 77");
    77
}

fn match_node_with_231(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 231: Terminal matched constructor ID 77");
    77
}

fn match_node_with_232(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 232: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_233(bytes, ctx),
        1 => match_node_with_234(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_233(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 233: Terminal matched constructor ID 141");
    141
}

fn match_node_with_234(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 234: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_235(bytes, ctx),
        1 => match_node_with_236(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_235(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 235: Terminal matched constructor ID 142");
    142
}

fn match_node_with_236(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 236: Terminal matched constructor ID 143");
    143
}

fn match_node_with_237(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 4) >> 2;
    eprintln!("Trace node 237: InstructionBitSlice offset=0, mask=4, probe={}", probe);
    match probe {
        0 => match_node_with_238(bytes, ctx),
        1 => match_node_with_261(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_238(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 238: InstructionBitSlice offset=0, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_239(bytes, ctx),
        1 => match_node_with_244(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_239(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 239: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_240(bytes, ctx),
        1 => match_node_with_241(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_240(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 240: Terminal matched constructor ID 113");
    113
}

fn match_node_with_241(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 241: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_242(bytes, ctx),
        1 => match_node_with_243(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_242(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 242: Terminal matched constructor ID 113");
    113
}

fn match_node_with_243(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 243: Terminal matched constructor ID 113");
    113
}

fn match_node_with_244(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 64) >> 6;
    eprintln!("Trace node 244: InstructionBitSlice offset=0, mask=64, probe={}", probe);
    match probe {
        0 => match_node_with_245(bytes, ctx),
        1 => match_node_with_250(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_245(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 245: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_246(bytes, ctx),
        1 => match_node_with_247(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_246(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 246: Terminal matched constructor ID 106");
    106
}

fn match_node_with_247(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 247: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_248(bytes, ctx),
        1 => match_node_with_249(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_248(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 248: Terminal matched constructor ID 106");
    106
}

fn match_node_with_249(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 249: Terminal matched constructor ID 106");
    106
}

fn match_node_with_250(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 128) >> 7;
    eprintln!("Trace node 250: InstructionBitSlice offset=0, mask=128, probe={}", probe);
    match probe {
        0 => match_node_with_251(bytes, ctx),
        1 => match_node_with_256(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_251(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 251: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_252(bytes, ctx),
        1 => match_node_with_253(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_252(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 252: Terminal matched constructor ID 2");
    2
}

fn match_node_with_253(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 253: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_254(bytes, ctx),
        1 => match_node_with_255(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_254(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 254: Terminal matched constructor ID 142");
    142
}

fn match_node_with_255(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 255: Terminal matched constructor ID 143");
    143
}

fn match_node_with_256(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 256: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_257(bytes, ctx),
        1 => match_node_with_258(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_257(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 257: Terminal matched constructor ID 103");
    103
}

fn match_node_with_258(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 258: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_259(bytes, ctx),
        1 => match_node_with_260(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_259(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 259: Terminal matched constructor ID 103");
    103
}

fn match_node_with_260(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 260: Terminal matched constructor ID 103");
    103
}

fn match_node_with_261(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 261: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_262(bytes, ctx),
        1 => match_node_with_263(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_262(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 262: Terminal matched constructor ID 141");
    141
}

fn match_node_with_263(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 263: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_264(bytes, ctx),
        1 => match_node_with_265(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_264(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 264: Terminal matched constructor ID 142");
    142
}

fn match_node_with_265(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 265: Terminal matched constructor ID 143");
    143
}

fn match_node_with_266(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 2) >> 1;
    eprintln!("Trace node 266: InstructionBitSlice offset=0, mask=2, probe={}", probe);
    match probe {
        0 => match_node_with_267(bytes, ctx),
        1 => match_node_with_290(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_267(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 4) >> 2;
    eprintln!("Trace node 267: InstructionBitSlice offset=0, mask=4, probe={}", probe);
    match probe {
        0 => match_node_with_268(bytes, ctx),
        1 => match_node_with_285(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_268(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 268: InstructionBitSlice offset=0, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_269(bytes, ctx),
        1 => match_node_with_274(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_269(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 269: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_270(bytes, ctx),
        1 => match_node_with_271(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_270(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 270: Terminal matched constructor ID 141");
    141
}

fn match_node_with_271(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 271: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_272(bytes, ctx),
        1 => match_node_with_273(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_272(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 272: Terminal matched constructor ID 142");
    142
}

fn match_node_with_273(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 273: Terminal matched constructor ID 143");
    143
}

fn match_node_with_274(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 64) >> 6;
    eprintln!("Trace node 274: InstructionBitSlice offset=0, mask=64, probe={}", probe);
    match probe {
        0 => match_node_with_275(bytes, ctx),
        1 => match_node_with_280(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_275(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 275: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_276(bytes, ctx),
        1 => match_node_with_277(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_276(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 276: Terminal matched constructor ID 108");
    108
}

fn match_node_with_277(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 277: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_278(bytes, ctx),
        1 => match_node_with_279(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_278(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 278: Terminal matched constructor ID 108");
    108
}

fn match_node_with_279(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 279: Terminal matched constructor ID 108");
    108
}

fn match_node_with_280(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 280: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_281(bytes, ctx),
        1 => match_node_with_282(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_281(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 281: Terminal matched constructor ID 105");
    105
}

fn match_node_with_282(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 282: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_283(bytes, ctx),
        1 => match_node_with_284(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_283(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 283: Terminal matched constructor ID 105");
    105
}

fn match_node_with_284(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 284: Terminal matched constructor ID 105");
    105
}

fn match_node_with_285(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 285: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_286(bytes, ctx),
        1 => match_node_with_287(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_286(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 286: Terminal matched constructor ID 141");
    141
}

fn match_node_with_287(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 287: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_288(bytes, ctx),
        1 => match_node_with_289(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_288(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 288: Terminal matched constructor ID 142");
    142
}

fn match_node_with_289(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 289: Terminal matched constructor ID 143");
    143
}

fn match_node_with_290(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 4) >> 2;
    eprintln!("Trace node 290: InstructionBitSlice offset=0, mask=4, probe={}", probe);
    match probe {
        0 => match_node_with_291(bytes, ctx),
        1 => match_node_with_308(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_291(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 64) >> 6;
    eprintln!("Trace node 291: InstructionBitSlice offset=0, mask=64, probe={}", probe);
    match probe {
        0 => match_node_with_292(bytes, ctx),
        1 => match_node_with_297(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_292(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 292: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_293(bytes, ctx),
        1 => match_node_with_294(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_293(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 293: Terminal matched constructor ID 141");
    141
}

fn match_node_with_294(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 294: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_295(bytes, ctx),
        1 => match_node_with_296(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_295(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 295: Terminal matched constructor ID 142");
    142
}

fn match_node_with_296(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 296: Terminal matched constructor ID 143");
    143
}

fn match_node_with_297(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 128) >> 7;
    eprintln!("Trace node 297: InstructionBitSlice offset=0, mask=128, probe={}", probe);
    match probe {
        0 => match_node_with_298(bytes, ctx),
        1 => match_node_with_303(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_298(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 298: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_299(bytes, ctx),
        1 => match_node_with_300(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_299(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 299: Terminal matched constructor ID 3");
    3
}

fn match_node_with_300(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 300: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_301(bytes, ctx),
        1 => match_node_with_302(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_301(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 301: Terminal matched constructor ID 142");
    142
}

fn match_node_with_302(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 302: Terminal matched constructor ID 143");
    143
}

fn match_node_with_303(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 303: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_304(bytes, ctx),
        1 => match_node_with_305(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_304(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 304: Terminal matched constructor ID 115");
    115
}

fn match_node_with_305(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 305: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_306(bytes, ctx),
        1 => match_node_with_307(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_306(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 306: Terminal matched constructor ID 115");
    115
}

fn match_node_with_307(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 307: Terminal matched constructor ID 115");
    115
}

fn match_node_with_308(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(0).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 308: InstructionBitSlice offset=0, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_309(bytes, ctx),
        1 => match_node_with_314(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_309(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 309: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_310(bytes, ctx),
        1 => match_node_with_311(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_310(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 310: Terminal matched constructor ID 141");
    141
}

fn match_node_with_311(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 311: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_312(bytes, ctx),
        1 => match_node_with_313(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_312(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 312: Terminal matched constructor ID 142");
    142
}

fn match_node_with_313(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 313: Terminal matched constructor ID 143");
    143
}

fn match_node_with_314(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 1) >> 0;
    eprintln!("Trace node 314: InstructionBitSlice offset=1, mask=1, probe={}", probe);
    match probe {
        0 => match_node_with_315(bytes, ctx),
        1 => match_node_with_316(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_315(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 315: Terminal matched constructor ID 98");
    98
}

fn match_node_with_316(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (bytes.get(1).copied().unwrap_or(0) & 8) >> 3;
    eprintln!("Trace node 316: InstructionBitSlice offset=1, mask=8, probe={}", probe);
    match probe {
        0 => match_node_with_317(bytes, ctx),
        1 => match_node_with_318(bytes, ctx),
        _ => -1,
    }
}

fn match_node_with_317(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 317: Terminal matched constructor ID 98");
    98
}

fn match_node_with_318(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 318: Terminal matched constructor ID 98");
    98
}

