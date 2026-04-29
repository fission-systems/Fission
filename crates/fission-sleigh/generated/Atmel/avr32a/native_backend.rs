// Auto-generated Fission Native Backend
#[no_mangle]
pub extern "C" fn fission_decode_match(table_ptr: *const i8, bytes: *const u8, bytes_len: usize, ctx_ptr: *const u64) -> i32 {
    let table_name = unsafe { std::ffi::CStr::from_ptr(table_ptr).to_str().unwrap() };
    let bytes = unsafe { std::slice::from_raw_parts(bytes, bytes_len) };
    let ctx = unsafe { *ctx_ptr };
    match table_name {
        "ACALLdisp" => match_node_ACALLdisp_0(bytes, ctx),
        "COM10" => match_node_COM10_0(bytes, ctx),
        "COM11" => match_node_COM11_0(bytes, ctx),
        "COM5" => match_node_COM5_0(bytes, ctx),
        "COM6" => match_node_COM6_0(bytes, ctx),
        "COM7" => match_node_COM7_0(bytes, ctx),
        "COM8" => match_node_COM8_0(bytes, ctx),
        "COM9" => match_node_COM9_0(bytes, ctx),
        "COND_3" => match_node_COND_3_0(bytes, ctx),
        "COND_4_0" => match_node_COND_4_0_0(bytes, ctx),
        "COND_4_4" => match_node_COND_4_4_0(bytes, ctx),
        "COND_e12" => match_node_COND_e12_0(bytes, ctx),
        "COPD" => match_node_COPD_0(bytes, ctx),
        "CPLoadAddress" => match_node_CPLoadAddress_0(bytes, ctx),
        "CRD" => match_node_CRD_0(bytes, ctx),
        "CRX" => match_node_CRX_0(bytes, ctx),
        "CRY" => match_node_CRY_0(bytes, ctx),
        "CSBRH" => match_node_CSBRH_0(bytes, ctx),
        "ECOND_4_4" => match_node_ECOND_4_4_0(bytes, ctx),
        "ECOND_8_4" => match_node_ECOND_8_4_0(bytes, ctx),
        "F" => match_node_F_0(bytes, ctx),
        "IndirectPlaceHolder" => match_node_IndirectPlaceHolder_0(bytes, ctx),
        "JV3" => match_node_JV3_0(bytes, ctx),
        "LDBP" => match_node_LDBP_0(bytes, ctx),
        "LDCMDcommon" => match_node_LDCMDcommon_0(bytes, ctx),
        "LDCMDinc0" => match_node_LDCMDinc0_0(bytes, ctx),
        "LDCMDinc1" => match_node_LDCMDinc1_0(bytes, ctx),
        "LDCMDinc2" => match_node_LDCMDinc2_0(bytes, ctx),
        "LDCMDinc3" => match_node_LDCMDinc3_0(bytes, ctx),
        "LDCMDinc4" => match_node_LDCMDinc4_0(bytes, ctx),
        "LDCMDinc5" => match_node_LDCMDinc5_0(bytes, ctx),
        "LDCMDinc6" => match_node_LDCMDinc6_0(bytes, ctx),
        "LDCMDinc7" => match_node_LDCMDinc7_0(bytes, ctx),
        "LDCMWHcommon" => match_node_LDCMWHcommon_0(bytes, ctx),
        "LDCMWLcommon" => match_node_LDCMWLcommon_0(bytes, ctx),
        "LDCMWinc0" => match_node_LDCMWinc0_0(bytes, ctx),
        "LDCMWinc1" => match_node_LDCMWinc1_0(bytes, ctx),
        "LDCMWinc10" => match_node_LDCMWinc10_0(bytes, ctx),
        "LDCMWinc11" => match_node_LDCMWinc11_0(bytes, ctx),
        "LDCMWinc12" => match_node_LDCMWinc12_0(bytes, ctx),
        "LDCMWinc13" => match_node_LDCMWinc13_0(bytes, ctx),
        "LDCMWinc14" => match_node_LDCMWinc14_0(bytes, ctx),
        "LDCMWinc15" => match_node_LDCMWinc15_0(bytes, ctx),
        "LDCMWinc2" => match_node_LDCMWinc2_0(bytes, ctx),
        "LDCMWinc3" => match_node_LDCMWinc3_0(bytes, ctx),
        "LDCMWinc4" => match_node_LDCMWinc4_0(bytes, ctx),
        "LDCMWinc5" => match_node_LDCMWinc5_0(bytes, ctx),
        "LDCMWinc6" => match_node_LDCMWinc6_0(bytes, ctx),
        "LDCMWinc7" => match_node_LDCMWinc7_0(bytes, ctx),
        "LDCMWinc8" => match_node_LDCMWinc8_0(bytes, ctx),
        "LDCMWinc9" => match_node_LDCMWinc9_0(bytes, ctx),
        "LDCMpp" => match_node_LDCMpp_0(bytes, ctx),
        "LDDPCdisp" => match_node_LDDPCdisp_0(bytes, ctx),
        "LDHP" => match_node_LDHP_0(bytes, ctx),
        "LDMcommon" => match_node_LDMcommon_0(bytes, ctx),
        "LDMinc0" => match_node_LDMinc0_0(bytes, ctx),
        "LDMinc1" => match_node_LDMinc1_0(bytes, ctx),
        "LDMinc10" => match_node_LDMinc10_0(bytes, ctx),
        "LDMinc11" => match_node_LDMinc11_0(bytes, ctx),
        "LDMinc12ab" => match_node_LDMinc12ab_0(bytes, ctx),
        "LDMinc12c" => match_node_LDMinc12c_0(bytes, ctx),
        "LDMinc12s" => match_node_LDMinc12s_0(bytes, ctx),
        "LDMinc13ab" => match_node_LDMinc13ab_0(bytes, ctx),
        "LDMinc14ab" => match_node_LDMinc14ab_0(bytes, ctx),
        "LDMinc15" => match_node_LDMinc15_0(bytes, ctx),
        "LDMinc2" => match_node_LDMinc2_0(bytes, ctx),
        "LDMinc3" => match_node_LDMinc3_0(bytes, ctx),
        "LDMinc4" => match_node_LDMinc4_0(bytes, ctx),
        "LDMinc5" => match_node_LDMinc5_0(bytes, ctx),
        "LDMinc6" => match_node_LDMinc6_0(bytes, ctx),
        "LDMinc7" => match_node_LDMinc7_0(bytes, ctx),
        "LDMinc8" => match_node_LDMinc8_0(bytes, ctx),
        "LDMinc9" => match_node_LDMinc9_0(bytes, ctx),
        "LDMlista" => match_node_LDMlista_0(bytes, ctx),
        "LDMlistb" => match_node_LDMlistb_0(bytes, ctx),
        "LDMlistc" => match_node_LDMlistc_0(bytes, ctx),
        "LDMlistd" => match_node_LDMlistd_0(bytes, ctx),
        "LDMpp" => match_node_LDMpp_0(bytes, ctx),
        "LDMret" => match_node_LDMret_0(bytes, ctx),
        "LDSHIFT12" => match_node_LDSHIFT12_0(bytes, ctx),
        "LDSTSWPH" => match_node_LDSTSWPH_0(bytes, ctx),
        "LDSTSWPW" => match_node_LDSTSWPW_0(bytes, ctx),
        "LoadAddress" => match_node_LoadAddress_0(bytes, ctx),
        "LoadAddressTS" => match_node_LoadAddressTS_0(bytes, ctx),
        "MEMSH" => match_node_MEMSH_0(bytes, ctx),
        "PCDisp16" => match_node_PCDisp16_0(bytes, ctx),
        "POPMchunk" => match_node_POPMchunk_0(bytes, ctx),
        "POPMdispa" => match_node_POPMdispa_0(bytes, ctx),
        "POPMdispb" => match_node_POPMdispb_0(bytes, ctx),
        "POPMinc10a" => match_node_POPMinc10a_0(bytes, ctx),
        "POPMinc10b" => match_node_POPMinc10b_0(bytes, ctx),
        "POPMinc11" => match_node_POPMinc11_0(bytes, ctx),
        "POPMinc4" => match_node_POPMinc4_0(bytes, ctx),
        "POPMinc5" => match_node_POPMinc5_0(bytes, ctx),
        "POPMinc6" => match_node_POPMinc6_0(bytes, ctx),
        "POPMinc7" => match_node_POPMinc7_0(bytes, ctx),
        "POPMinc8" => match_node_POPMinc8_0(bytes, ctx),
        "POPMinc9a" => match_node_POPMinc9a_0(bytes, ctx),
        "POPMinc9b" => match_node_POPMinc9b_0(bytes, ctx),
        "POPMinc9s" => match_node_POPMinc9s_0(bytes, ctx),
        "POPMjump" => match_node_POPMjump_0(bytes, ctx),
        "PUPART" => match_node_PUPART_0(bytes, ctx),
        "PUSHMdec10" => match_node_PUSHMdec10_0(bytes, ctx),
        "PUSHMdec11" => match_node_PUSHMdec11_0(bytes, ctx),
        "PUSHMdec4" => match_node_PUSHMdec4_0(bytes, ctx),
        "PUSHMdec5" => match_node_PUSHMdec5_0(bytes, ctx),
        "PUSHMdec6" => match_node_PUSHMdec6_0(bytes, ctx),
        "PUSHMdec7" => match_node_PUSHMdec7_0(bytes, ctx),
        "PUSHMdec8" => match_node_PUSHMdec8_0(bytes, ctx),
        "PUSHMdec9" => match_node_PUSHMdec9_0(bytes, ctx),
        "PUSHMdisp" => match_node_PUSHMdisp_0(bytes, ctx),
        "PXPART" => match_node_PXPART_0(bytes, ctx),
        "PYPART" => match_node_PYPART_0(bytes, ctx),
        "RB9Shift" => match_node_RB9Shift_0(bytes, ctx),
        "RBSelector" => match_node_RBSelector_0(bytes, ctx),
        "RBShift0" => match_node_RBShift0_0(bytes, ctx),
        "RD0A" => match_node_RD0A_0(bytes, ctx),
        "RJMPdisp" => match_node_RJMPdisp_0(bytes, ctx),
        "RP0Disp16" => match_node_RP0Disp16_0(bytes, ctx),
        "RP0Disp16_2" => match_node_RP0Disp16_2_0(bytes, ctx),
        "RP9bDec" => match_node_RP9bDec_0(bytes, ctx),
        "RP9bInc" => match_node_RP9bInc_0(bytes, ctx),
        "RPDisp16" => match_node_RPDisp16_0(bytes, ctx),
        "RPbDisp3" => match_node_RPbDisp3_0(bytes, ctx),
        "RPbDisp9" => match_node_RPbDisp9_0(bytes, ctx),
        "RPdDec" => match_node_RPdDec_0(bytes, ctx),
        "RPdDec0" => match_node_RPdDec0_0(bytes, ctx),
        "RPdInc" => match_node_RPdInc_0(bytes, ctx),
        "RPhDec" => match_node_RPhDec_0(bytes, ctx),
        "RPhDisp3" => match_node_RPhDisp3_0(bytes, ctx),
        "RPhDisp9" => match_node_RPhDisp9_0(bytes, ctx),
        "RPhInc" => match_node_RPhInc_0(bytes, ctx),
        "RPwDec" => match_node_RPwDec_0(bytes, ctx),
        "RPwDec0" => match_node_RPwDec0_0(bytes, ctx),
        "RPwDisp12" => match_node_RPwDisp12_0(bytes, ctx),
        "RPwDisp4" => match_node_RPwDisp4_0(bytes, ctx),
        "RPwDisp5" => match_node_RPwDisp5_0(bytes, ctx),
        "RPwDisp8" => match_node_RPwDisp8_0(bytes, ctx),
        "RPwDisp9" => match_node_RPwDisp9_0(bytes, ctx),
        "RPwInc" => match_node_RPwInc_0(bytes, ctx),
        "RS0A" => match_node_RS0A_0(bytes, ctx),
        "RS9A" => match_node_RS9A_0(bytes, ctx),
        "RX9A" => match_node_RX9A_0(bytes, ctx),
        "RY0A" => match_node_RY0A_0(bytes, ctx),
        "RelDisp10" => match_node_RelDisp10_0(bytes, ctx),
        "RelDisp21" => match_node_RelDisp21_0(bytes, ctx),
        "SATM" => match_node_SATM_0(bytes, ctx),
        "SLGM" => match_node_SLGM_0(bytes, ctx),
        "STCMDcommon" => match_node_STCMDcommon_0(bytes, ctx),
        "STCMDdec0" => match_node_STCMDdec0_0(bytes, ctx),
        "STCMDdec1" => match_node_STCMDdec1_0(bytes, ctx),
        "STCMDdec2" => match_node_STCMDdec2_0(bytes, ctx),
        "STCMDdec3" => match_node_STCMDdec3_0(bytes, ctx),
        "STCMDdec4" => match_node_STCMDdec4_0(bytes, ctx),
        "STCMDdec5" => match_node_STCMDdec5_0(bytes, ctx),
        "STCMDdec6" => match_node_STCMDdec6_0(bytes, ctx),
        "STCMDdec7" => match_node_STCMDdec7_0(bytes, ctx),
        "STCMWHcommon" => match_node_STCMWHcommon_0(bytes, ctx),
        "STCMWLcommon" => match_node_STCMWLcommon_0(bytes, ctx),
        "STCMWdec0" => match_node_STCMWdec0_0(bytes, ctx),
        "STCMWdec1" => match_node_STCMWdec1_0(bytes, ctx),
        "STCMWdec10" => match_node_STCMWdec10_0(bytes, ctx),
        "STCMWdec11" => match_node_STCMWdec11_0(bytes, ctx),
        "STCMWdec12" => match_node_STCMWdec12_0(bytes, ctx),
        "STCMWdec13" => match_node_STCMWdec13_0(bytes, ctx),
        "STCMWdec14" => match_node_STCMWdec14_0(bytes, ctx),
        "STCMWdec15" => match_node_STCMWdec15_0(bytes, ctx),
        "STCMWdec2" => match_node_STCMWdec2_0(bytes, ctx),
        "STCMWdec3" => match_node_STCMWdec3_0(bytes, ctx),
        "STCMWdec4" => match_node_STCMWdec4_0(bytes, ctx),
        "STCMWdec5" => match_node_STCMWdec5_0(bytes, ctx),
        "STCMWdec6" => match_node_STCMWdec6_0(bytes, ctx),
        "STCMWdec7" => match_node_STCMWdec7_0(bytes, ctx),
        "STCMWdec8" => match_node_STCMWdec8_0(bytes, ctx),
        "STCMWdec9" => match_node_STCMWdec9_0(bytes, ctx),
        "STCMmm" => match_node_STCMmm_0(bytes, ctx),
        "STHHD" => match_node_STHHD_0(bytes, ctx),
        "STMdec0" => match_node_STMdec0_0(bytes, ctx),
        "STMdec1" => match_node_STMdec1_0(bytes, ctx),
        "STMdec10" => match_node_STMdec10_0(bytes, ctx),
        "STMdec11" => match_node_STMdec11_0(bytes, ctx),
        "STMdec12" => match_node_STMdec12_0(bytes, ctx),
        "STMdec13" => match_node_STMdec13_0(bytes, ctx),
        "STMdec14" => match_node_STMdec14_0(bytes, ctx),
        "STMdec15" => match_node_STMdec15_0(bytes, ctx),
        "STMdec2" => match_node_STMdec2_0(bytes, ctx),
        "STMdec3" => match_node_STMdec3_0(bytes, ctx),
        "STMdec4" => match_node_STMdec4_0(bytes, ctx),
        "STMdec5" => match_node_STMdec5_0(bytes, ctx),
        "STMdec6" => match_node_STMdec6_0(bytes, ctx),
        "STMdec7" => match_node_STMdec7_0(bytes, ctx),
        "STMdec8" => match_node_STMdec8_0(bytes, ctx),
        "STMdec9" => match_node_STMdec9_0(bytes, ctx),
        "STMdecdisp" => match_node_STMdecdisp_0(bytes, ctx),
        "STMinc0" => match_node_STMinc0_0(bytes, ctx),
        "STMinc1" => match_node_STMinc1_0(bytes, ctx),
        "STMinc10" => match_node_STMinc10_0(bytes, ctx),
        "STMinc11" => match_node_STMinc11_0(bytes, ctx),
        "STMinc12" => match_node_STMinc12_0(bytes, ctx),
        "STMinc13" => match_node_STMinc13_0(bytes, ctx),
        "STMinc14" => match_node_STMinc14_0(bytes, ctx),
        "STMinc15" => match_node_STMinc15_0(bytes, ctx),
        "STMinc2" => match_node_STMinc2_0(bytes, ctx),
        "STMinc3" => match_node_STMinc3_0(bytes, ctx),
        "STMinc4" => match_node_STMinc4_0(bytes, ctx),
        "STMinc5" => match_node_STMinc5_0(bytes, ctx),
        "STMinc6" => match_node_STMinc6_0(bytes, ctx),
        "STMinc7" => match_node_STMinc7_0(bytes, ctx),
        "STMinc8" => match_node_STMinc8_0(bytes, ctx),
        "STMinc9" => match_node_STMinc9_0(bytes, ctx),
        "STMincdisp" => match_node_STMincdisp_0(bytes, ctx),
        "STXP" => match_node_STXP_0(bytes, ctx),
        "STYP" => match_node_STYP_0(bytes, ctx),
        "StoreAddress" => match_node_StoreAddress_0(bytes, ctx),
        "XPART" => match_node_XPART_0(bytes, ctx),
        "YPART" => match_node_YPART_0(bytes, ctx),
        "cc3_0" => match_node_cc3_0_0(bytes, ctx),
        "cc4_0" => match_node_cc4_0_0(bytes, ctx),
        "cc4_4" => match_node_cc4_4_0(bytes, ctx),
        "cc4_e12" => match_node_cc4_e12_0(bytes, ctx),
        "ecc4_4" => match_node_ecc4_4_0(bytes, ctx),
        "ecc8_4" => match_node_ecc8_4_0(bytes, ctx),
        "instruction" => match_node_instruction_0(bytes, ctx),
        "retCond4Sub" => match_node_retCond4Sub_0(bytes, ctx),
        "sDisp21" => match_node_sDisp21_0(bytes, ctx),
        "sDisp8" => match_node_sDisp8_0(bytes, ctx),
        _ => -1
    }
}

fn match_node_ACALLdisp_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_COM10_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_COM11_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=6, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_COM11_1(bytes, ctx),
        1 => match_node_COM11_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_COM11_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_COM11_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_COM5_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_COM6_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_COM7_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_COM8_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_COM9_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_COND_3_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_COND_4_0_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 1");
    1
}

fn match_node_COND_4_4_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 1");
    1
}

fn match_node_COND_e12_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 1");
    1
}

fn match_node_COPD_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 7;
    eprintln!("Trace node 0: SlaInstructionBits start=0, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_COPD_1(bytes, ctx),
        1 => match_node_COPD_2(bytes, ctx),
        2 => match_node_COPD_3(bytes, ctx),
        3 => match_node_COPD_4(bytes, ctx),
        4 => match_node_COPD_5(bytes, ctx),
        5 => match_node_COPD_6(bytes, ctx),
        6 => match_node_COPD_7(bytes, ctx),
        7 => match_node_COPD_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_COPD_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_COPD_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_COPD_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_COPD_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_COPD_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 4");
    4
}

fn match_node_COPD_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_COPD_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 6");
    6
}

fn match_node_COPD_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 7");
    7
}

fn match_node_CPLoadAddress_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_CRD_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 15;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_CRD_1(bytes, ctx),
        1 => match_node_CRD_2(bytes, ctx),
        2 => match_node_CRD_3(bytes, ctx),
        3 => match_node_CRD_4(bytes, ctx),
        4 => match_node_CRD_5(bytes, ctx),
        5 => match_node_CRD_6(bytes, ctx),
        6 => match_node_CRD_7(bytes, ctx),
        7 => match_node_CRD_8(bytes, ctx),
        8 => match_node_CRD_9(bytes, ctx),
        9 => match_node_CRD_10(bytes, ctx),
        10 => match_node_CRD_11(bytes, ctx),
        11 => match_node_CRD_12(bytes, ctx),
        12 => match_node_CRD_13(bytes, ctx),
        13 => match_node_CRD_14(bytes, ctx),
        14 => match_node_CRD_15(bytes, ctx),
        15 => match_node_CRD_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_CRD_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_CRD_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_CRD_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_CRD_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_CRD_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 4");
    4
}

fn match_node_CRD_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_CRD_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 6");
    6
}

fn match_node_CRD_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 7");
    7
}

fn match_node_CRD_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 8");
    8
}

fn match_node_CRD_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 9");
    9
}

fn match_node_CRD_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 10");
    10
}

fn match_node_CRD_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 11");
    11
}

fn match_node_CRD_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 12");
    12
}

fn match_node_CRD_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 13");
    13
}

fn match_node_CRD_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 14");
    14
}

fn match_node_CRD_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 15");
    15
}

fn match_node_CRX_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 15;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_CRX_1(bytes, ctx),
        1 => match_node_CRX_2(bytes, ctx),
        2 => match_node_CRX_3(bytes, ctx),
        3 => match_node_CRX_4(bytes, ctx),
        4 => match_node_CRX_5(bytes, ctx),
        5 => match_node_CRX_6(bytes, ctx),
        6 => match_node_CRX_7(bytes, ctx),
        7 => match_node_CRX_8(bytes, ctx),
        8 => match_node_CRX_9(bytes, ctx),
        9 => match_node_CRX_10(bytes, ctx),
        10 => match_node_CRX_11(bytes, ctx),
        11 => match_node_CRX_12(bytes, ctx),
        12 => match_node_CRX_13(bytes, ctx),
        13 => match_node_CRX_14(bytes, ctx),
        14 => match_node_CRX_15(bytes, ctx),
        15 => match_node_CRX_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_CRX_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_CRX_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_CRX_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_CRX_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_CRX_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 4");
    4
}

fn match_node_CRX_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_CRX_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 6");
    6
}

fn match_node_CRX_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 7");
    7
}

fn match_node_CRX_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 8");
    8
}

fn match_node_CRX_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 9");
    9
}

fn match_node_CRX_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 10");
    10
}

fn match_node_CRX_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 11");
    11
}

fn match_node_CRX_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 12");
    12
}

fn match_node_CRX_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 13");
    13
}

fn match_node_CRX_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 14");
    14
}

fn match_node_CRX_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 15");
    15
}

fn match_node_CRY_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 15;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_CRY_1(bytes, ctx),
        1 => match_node_CRY_2(bytes, ctx),
        2 => match_node_CRY_3(bytes, ctx),
        3 => match_node_CRY_4(bytes, ctx),
        4 => match_node_CRY_5(bytes, ctx),
        5 => match_node_CRY_6(bytes, ctx),
        6 => match_node_CRY_7(bytes, ctx),
        7 => match_node_CRY_8(bytes, ctx),
        8 => match_node_CRY_9(bytes, ctx),
        9 => match_node_CRY_10(bytes, ctx),
        10 => match_node_CRY_11(bytes, ctx),
        11 => match_node_CRY_12(bytes, ctx),
        12 => match_node_CRY_13(bytes, ctx),
        13 => match_node_CRY_14(bytes, ctx),
        14 => match_node_CRY_15(bytes, ctx),
        15 => match_node_CRY_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_CRY_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_CRY_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_CRY_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_CRY_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_CRY_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 4");
    4
}

fn match_node_CRY_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_CRY_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 6");
    6
}

fn match_node_CRY_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 7");
    7
}

fn match_node_CRY_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 8");
    8
}

fn match_node_CRY_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 9");
    9
}

fn match_node_CRY_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 10");
    10
}

fn match_node_CRY_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 11");
    11
}

fn match_node_CRY_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 12");
    12
}

fn match_node_CRY_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 13");
    13
}

fn match_node_CRY_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 14");
    14
}

fn match_node_CRY_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 15");
    15
}

fn match_node_CSBRH_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_ECOND_4_4_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 1");
    1
}

fn match_node_ECOND_8_4_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 1");
    1
}

fn match_node_F_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=6, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_F_1(bytes, ctx),
        1 => match_node_F_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_F_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_F_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_IndirectPlaceHolder_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_JV3_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_JV3_1(bytes, ctx),
        1 => match_node_JV3_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_JV3_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_JV3_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDBP_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 3;
    eprintln!("Trace node 0: SlaInstructionBits start=2, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDBP_1(bytes, ctx),
        1 => match_node_LDBP_2(bytes, ctx),
        2 => match_node_LDBP_3(bytes, ctx),
        3 => match_node_LDBP_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDBP_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_LDBP_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_LDBP_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_LDBP_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_LDCMDcommon_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_LDCMDinc0_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDCMDinc0_1(bytes, ctx),
        1 => match_node_LDCMDinc0_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDCMDinc0_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDCMDinc0_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDCMDinc1_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDCMDinc1_1(bytes, ctx),
        1 => match_node_LDCMDinc1_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDCMDinc1_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDCMDinc1_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDCMDinc2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDCMDinc2_1(bytes, ctx),
        1 => match_node_LDCMDinc2_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDCMDinc2_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDCMDinc2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDCMDinc3_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDCMDinc3_1(bytes, ctx),
        1 => match_node_LDCMDinc3_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDCMDinc3_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDCMDinc3_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDCMDinc4_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDCMDinc4_1(bytes, ctx),
        1 => match_node_LDCMDinc4_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDCMDinc4_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDCMDinc4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDCMDinc5_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDCMDinc5_1(bytes, ctx),
        1 => match_node_LDCMDinc5_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDCMDinc5_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDCMDinc5_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDCMDinc6_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDCMDinc6_1(bytes, ctx),
        1 => match_node_LDCMDinc6_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDCMDinc6_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDCMDinc6_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDCMDinc7_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDCMDinc7_1(bytes, ctx),
        1 => match_node_LDCMDinc7_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDCMDinc7_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDCMDinc7_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDCMWHcommon_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_LDCMWLcommon_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_LDCMWinc0_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDCMWinc0_1(bytes, ctx),
        1 => match_node_LDCMWinc0_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDCMWinc0_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDCMWinc0_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDCMWinc1_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDCMWinc1_1(bytes, ctx),
        1 => match_node_LDCMWinc1_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDCMWinc1_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDCMWinc1_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDCMWinc10_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDCMWinc10_1(bytes, ctx),
        1 => match_node_LDCMWinc10_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDCMWinc10_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDCMWinc10_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDCMWinc11_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDCMWinc11_1(bytes, ctx),
        1 => match_node_LDCMWinc11_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDCMWinc11_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDCMWinc11_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDCMWinc12_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDCMWinc12_1(bytes, ctx),
        1 => match_node_LDCMWinc12_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDCMWinc12_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDCMWinc12_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDCMWinc13_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDCMWinc13_1(bytes, ctx),
        1 => match_node_LDCMWinc13_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDCMWinc13_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDCMWinc13_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDCMWinc14_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDCMWinc14_1(bytes, ctx),
        1 => match_node_LDCMWinc14_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDCMWinc14_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDCMWinc14_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDCMWinc15_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDCMWinc15_1(bytes, ctx),
        1 => match_node_LDCMWinc15_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDCMWinc15_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDCMWinc15_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDCMWinc2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDCMWinc2_1(bytes, ctx),
        1 => match_node_LDCMWinc2_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDCMWinc2_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDCMWinc2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDCMWinc3_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDCMWinc3_1(bytes, ctx),
        1 => match_node_LDCMWinc3_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDCMWinc3_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDCMWinc3_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDCMWinc4_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDCMWinc4_1(bytes, ctx),
        1 => match_node_LDCMWinc4_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDCMWinc4_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDCMWinc4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDCMWinc5_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDCMWinc5_1(bytes, ctx),
        1 => match_node_LDCMWinc5_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDCMWinc5_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDCMWinc5_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDCMWinc6_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDCMWinc6_1(bytes, ctx),
        1 => match_node_LDCMWinc6_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDCMWinc6_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDCMWinc6_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDCMWinc7_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDCMWinc7_1(bytes, ctx),
        1 => match_node_LDCMWinc7_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDCMWinc7_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDCMWinc7_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDCMWinc8_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDCMWinc8_1(bytes, ctx),
        1 => match_node_LDCMWinc8_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDCMWinc8_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDCMWinc8_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDCMWinc9_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDCMWinc9_1(bytes, ctx),
        1 => match_node_LDCMWinc9_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDCMWinc9_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDCMWinc9_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDCMpp_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (19 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 19) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=19, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDCMpp_1(bytes, ctx),
        1 => match_node_LDCMpp_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDCMpp_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_LDCMpp_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_LDDPCdisp_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_LDHP_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=3, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDHP_1(bytes, ctx),
        1 => match_node_LDHP_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDHP_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_LDHP_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_LDMcommon_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_LDMinc0_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDMinc0_1(bytes, ctx),
        1 => match_node_LDMinc0_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDMinc0_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDMinc0_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDMinc1_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDMinc1_1(bytes, ctx),
        1 => match_node_LDMinc1_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDMinc1_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDMinc1_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDMinc10_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=5, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDMinc10_1(bytes, ctx),
        1 => match_node_LDMinc10_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDMinc10_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDMinc10_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDMinc11_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDMinc11_1(bytes, ctx),
        1 => match_node_LDMinc11_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDMinc11_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDMinc11_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDMinc12ab_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=3, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDMinc12ab_1(bytes, ctx),
        1 => match_node_LDMinc12ab_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDMinc12ab_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDMinc12ab_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDMinc12c_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDMinc12c_1(bytes, ctx),
        1 => match_node_LDMinc12c_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDMinc12c_1(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 1;
    eprintln!("Trace node 1: SlaInstructionBits start=3, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDMinc12c_2(bytes, ctx),
        1 => match_node_LDMinc12c_3(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDMinc12c_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDMinc12c_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 1");
    1
}

fn match_node_LDMinc12c_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 2");
    2
}

fn match_node_LDMinc12s_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=0, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDMinc12s_1(bytes, ctx),
        1 => match_node_LDMinc12s_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDMinc12s_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDMinc12s_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDMinc13ab_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=2, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDMinc13ab_1(bytes, ctx),
        1 => match_node_LDMinc13ab_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDMinc13ab_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDMinc13ab_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDMinc14ab_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDMinc14ab_1(bytes, ctx),
        1 => match_node_LDMinc14ab_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDMinc14ab_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDMinc14ab_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDMinc15_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=0, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDMinc15_1(bytes, ctx),
        1 => match_node_LDMinc15_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDMinc15_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDMinc15_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDMinc2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDMinc2_1(bytes, ctx),
        1 => match_node_LDMinc2_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDMinc2_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDMinc2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDMinc3_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDMinc3_1(bytes, ctx),
        1 => match_node_LDMinc3_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDMinc3_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDMinc3_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDMinc4_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDMinc4_1(bytes, ctx),
        1 => match_node_LDMinc4_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDMinc4_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDMinc4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDMinc5_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDMinc5_1(bytes, ctx),
        1 => match_node_LDMinc5_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDMinc5_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDMinc5_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDMinc6_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDMinc6_1(bytes, ctx),
        1 => match_node_LDMinc6_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDMinc6_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDMinc6_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDMinc7_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDMinc7_1(bytes, ctx),
        1 => match_node_LDMinc7_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDMinc7_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDMinc7_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDMinc8_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDMinc8_1(bytes, ctx),
        1 => match_node_LDMinc8_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDMinc8_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDMinc8_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDMinc9_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=6, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDMinc9_1(bytes, ctx),
        1 => match_node_LDMinc9_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDMinc9_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDMinc9_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDMlista_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_LDMlistb_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_LDMlistc_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_LDMlistd_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_LDMpp_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=6, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDMpp_1(bytes, ctx),
        1 => match_node_LDMpp_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDMpp_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_LDMpp_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_LDMret_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=0, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LDMret_1(bytes, ctx),
        1 => match_node_LDMret_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LDMret_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_LDMret_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LDSHIFT12_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_LDSTSWPH_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_LDSTSWPW_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_LoadAddress_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_LoadAddressTS_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_MEMSH_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_PCDisp16_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_POPMchunk_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_POPMdispa_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_POPMdispb_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_POPMinc10a_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=5, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_POPMinc10a_1(bytes, ctx),
        1 => match_node_POPMinc10a_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_POPMinc10a_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_POPMinc10a_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_POPMinc10b_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_POPMinc11_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_POPMinc11_1(bytes, ctx),
        1 => match_node_POPMinc11_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_POPMinc11_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_POPMinc11_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_POPMinc4_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_POPMinc4_1(bytes, ctx),
        1 => match_node_POPMinc4_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_POPMinc4_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_POPMinc4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_POPMinc5_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_POPMinc5_1(bytes, ctx),
        1 => match_node_POPMinc5_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_POPMinc5_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_POPMinc5_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_POPMinc6_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_POPMinc6_1(bytes, ctx),
        1 => match_node_POPMinc6_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_POPMinc6_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_POPMinc6_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_POPMinc7_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_POPMinc7_1(bytes, ctx),
        1 => match_node_POPMinc7_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_POPMinc7_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_POPMinc7_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_POPMinc8_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_POPMinc8_1(bytes, ctx),
        1 => match_node_POPMinc8_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_POPMinc8_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_POPMinc8_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_POPMinc9a_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=6, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_POPMinc9a_1(bytes, ctx),
        1 => match_node_POPMinc9a_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_POPMinc9a_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_POPMinc9a_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_POPMinc9b_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_POPMinc9b_1(bytes, ctx),
        1 => match_node_POPMinc9b_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_POPMinc9b_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 3");
    3
}

fn match_node_POPMinc9b_2(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 1;
    eprintln!("Trace node 2: SlaInstructionBits start=5, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_POPMinc9b_3(bytes, ctx),
        1 => match_node_POPMinc9b_6(bytes, ctx),
        _ => -1,
    }
}

fn match_node_POPMinc9b_3(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 1;
    eprintln!("Trace node 3: SlaInstructionBits start=6, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_POPMinc9b_4(bytes, ctx),
        1 => match_node_POPMinc9b_5(bytes, ctx),
        _ => -1,
    }
}

fn match_node_POPMinc9b_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 0");
    0
}

fn match_node_POPMinc9b_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 1");
    1
}

fn match_node_POPMinc9b_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 2");
    2
}

fn match_node_POPMinc9s_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_POPMinc9s_1(bytes, ctx),
        1 => match_node_POPMinc9s_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_POPMinc9s_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_POPMinc9s_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_POPMjump_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_POPMjump_1(bytes, ctx),
        1 => match_node_POPMjump_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_POPMjump_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_POPMjump_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_PUPART_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_PUPART_1(bytes, ctx),
        1 => match_node_PUPART_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_PUPART_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_PUPART_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_PUSHMdec10_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=5, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_PUSHMdec10_1(bytes, ctx),
        1 => match_node_PUSHMdec10_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_PUSHMdec10_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_PUSHMdec10_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_PUSHMdec11_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_PUSHMdec11_1(bytes, ctx),
        1 => match_node_PUSHMdec11_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_PUSHMdec11_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_PUSHMdec11_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_PUSHMdec4_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_PUSHMdec4_1(bytes, ctx),
        1 => match_node_PUSHMdec4_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_PUSHMdec4_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_PUSHMdec4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_PUSHMdec5_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_PUSHMdec5_1(bytes, ctx),
        1 => match_node_PUSHMdec5_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_PUSHMdec5_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_PUSHMdec5_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_PUSHMdec6_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_PUSHMdec6_1(bytes, ctx),
        1 => match_node_PUSHMdec6_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_PUSHMdec6_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_PUSHMdec6_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_PUSHMdec7_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_PUSHMdec7_1(bytes, ctx),
        1 => match_node_PUSHMdec7_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_PUSHMdec7_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_PUSHMdec7_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_PUSHMdec8_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_PUSHMdec8_1(bytes, ctx),
        1 => match_node_PUSHMdec8_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_PUSHMdec8_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_PUSHMdec8_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_PUSHMdec9_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=6, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_PUSHMdec9_1(bytes, ctx),
        1 => match_node_PUSHMdec9_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_PUSHMdec9_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_PUSHMdec9_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_PUSHMdisp_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_PXPART_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_PXPART_1(bytes, ctx),
        1 => match_node_PXPART_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_PXPART_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_PXPART_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_PYPART_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_PYPART_1(bytes, ctx),
        1 => match_node_PYPART_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_PYPART_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_PYPART_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_RB9Shift_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_RBSelector_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (26 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 26) & 3;
    eprintln!("Trace node 0: SlaInstructionBits start=26, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_RBSelector_1(bytes, ctx),
        1 => match_node_RBSelector_2(bytes, ctx),
        2 => match_node_RBSelector_3(bytes, ctx),
        3 => match_node_RBSelector_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_RBSelector_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_RBSelector_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_RBSelector_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_RBSelector_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_RBShift0_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_RD0A_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 1");
    1
}

fn match_node_RJMPdisp_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_RP0Disp16_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_RP0Disp16_2_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_RP9bDec_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_RP9bInc_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_RPDisp16_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 1");
    1
}

fn match_node_RPbDisp3_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_RPbDisp9_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_RPdDec_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_RPdDec0_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_RPdInc_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_RPhDec_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_RPhDisp3_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_RPhDisp9_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_RPhInc_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_RPwDec_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_RPwDec0_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_RPwDisp12_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_RPwDisp4_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_RPwDisp5_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_RPwDisp8_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_RPwDisp9_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_RPwInc_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_RS0A_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 1");
    1
}

fn match_node_RS9A_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 1");
    1
}

fn match_node_RX9A_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 1");
    1
}

fn match_node_RY0A_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 1");
    1
}

fn match_node_RelDisp10_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_RelDisp21_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_SATM_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 5 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 31;
    eprintln!("Trace node 0: SlaInstructionBits start=6, size=5, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_SATM_1(bytes, ctx),
        1 => match_node_SATM_2(bytes, ctx),
        2 => match_node_SATM_3(bytes, ctx),
        3 => match_node_SATM_4(bytes, ctx),
        4 => match_node_SATM_5(bytes, ctx),
        5 => match_node_SATM_6(bytes, ctx),
        6 => match_node_SATM_7(bytes, ctx),
        7 => match_node_SATM_8(bytes, ctx),
        8 => match_node_SATM_9(bytes, ctx),
        9 => match_node_SATM_10(bytes, ctx),
        10 => match_node_SATM_11(bytes, ctx),
        11 => match_node_SATM_12(bytes, ctx),
        12 => match_node_SATM_13(bytes, ctx),
        13 => match_node_SATM_14(bytes, ctx),
        14 => match_node_SATM_15(bytes, ctx),
        15 => match_node_SATM_16(bytes, ctx),
        16 => match_node_SATM_17(bytes, ctx),
        17 => match_node_SATM_18(bytes, ctx),
        18 => match_node_SATM_19(bytes, ctx),
        19 => match_node_SATM_20(bytes, ctx),
        20 => match_node_SATM_21(bytes, ctx),
        21 => match_node_SATM_22(bytes, ctx),
        22 => match_node_SATM_23(bytes, ctx),
        23 => match_node_SATM_24(bytes, ctx),
        24 => match_node_SATM_25(bytes, ctx),
        25 => match_node_SATM_26(bytes, ctx),
        26 => match_node_SATM_27(bytes, ctx),
        27 => match_node_SATM_28(bytes, ctx),
        28 => match_node_SATM_29(bytes, ctx),
        29 => match_node_SATM_30(bytes, ctx),
        30 => match_node_SATM_31(bytes, ctx),
        31 => match_node_SATM_32(bytes, ctx),
        _ => -1,
    }
}

fn match_node_SATM_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_SATM_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_SATM_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_SATM_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_SATM_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 4");
    4
}

fn match_node_SATM_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_SATM_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 6");
    6
}

fn match_node_SATM_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 7");
    7
}

fn match_node_SATM_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 8");
    8
}

fn match_node_SATM_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 9");
    9
}

fn match_node_SATM_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 10");
    10
}

fn match_node_SATM_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 11");
    11
}

fn match_node_SATM_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 12");
    12
}

fn match_node_SATM_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 13");
    13
}

fn match_node_SATM_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 14");
    14
}

fn match_node_SATM_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 15");
    15
}

fn match_node_SATM_17(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 17: Terminal matched constructor ID 16");
    16
}

fn match_node_SATM_18(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 18: Terminal matched constructor ID 17");
    17
}

fn match_node_SATM_19(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 19: Terminal matched constructor ID 18");
    18
}

fn match_node_SATM_20(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 20: Terminal matched constructor ID 19");
    19
}

fn match_node_SATM_21(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 21: Terminal matched constructor ID 20");
    20
}

fn match_node_SATM_22(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 22: Terminal matched constructor ID 21");
    21
}

fn match_node_SATM_23(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 23: Terminal matched constructor ID 22");
    22
}

fn match_node_SATM_24(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 24: Terminal matched constructor ID 23");
    23
}

fn match_node_SATM_25(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 25: Terminal matched constructor ID 24");
    24
}

fn match_node_SATM_26(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 26: Terminal matched constructor ID 25");
    25
}

fn match_node_SATM_27(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 27: Terminal matched constructor ID 26");
    26
}

fn match_node_SATM_28(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 28: Terminal matched constructor ID 27");
    27
}

fn match_node_SATM_29(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 29: Terminal matched constructor ID 28");
    28
}

fn match_node_SATM_30(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 30: Terminal matched constructor ID 29");
    29
}

fn match_node_SATM_31(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 31: Terminal matched constructor ID 30");
    30
}

fn match_node_SATM_32(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 32: Terminal matched constructor ID 31");
    31
}

fn match_node_SLGM_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 1");
    1
}

fn match_node_STCMDcommon_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_STCMDdec0_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STCMDdec0_1(bytes, ctx),
        1 => match_node_STCMDdec0_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STCMDdec0_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STCMDdec0_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STCMDdec1_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STCMDdec1_1(bytes, ctx),
        1 => match_node_STCMDdec1_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STCMDdec1_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STCMDdec1_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STCMDdec2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STCMDdec2_1(bytes, ctx),
        1 => match_node_STCMDdec2_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STCMDdec2_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STCMDdec2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STCMDdec3_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STCMDdec3_1(bytes, ctx),
        1 => match_node_STCMDdec3_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STCMDdec3_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STCMDdec3_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STCMDdec4_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STCMDdec4_1(bytes, ctx),
        1 => match_node_STCMDdec4_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STCMDdec4_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STCMDdec4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STCMDdec5_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STCMDdec5_1(bytes, ctx),
        1 => match_node_STCMDdec5_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STCMDdec5_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STCMDdec5_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STCMDdec6_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STCMDdec6_1(bytes, ctx),
        1 => match_node_STCMDdec6_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STCMDdec6_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STCMDdec6_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STCMDdec7_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STCMDdec7_1(bytes, ctx),
        1 => match_node_STCMDdec7_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STCMDdec7_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STCMDdec7_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STCMWHcommon_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_STCMWLcommon_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_STCMWdec0_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STCMWdec0_1(bytes, ctx),
        1 => match_node_STCMWdec0_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STCMWdec0_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STCMWdec0_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STCMWdec1_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STCMWdec1_1(bytes, ctx),
        1 => match_node_STCMWdec1_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STCMWdec1_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STCMWdec1_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STCMWdec10_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STCMWdec10_1(bytes, ctx),
        1 => match_node_STCMWdec10_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STCMWdec10_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STCMWdec10_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STCMWdec11_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STCMWdec11_1(bytes, ctx),
        1 => match_node_STCMWdec11_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STCMWdec11_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STCMWdec11_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STCMWdec12_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STCMWdec12_1(bytes, ctx),
        1 => match_node_STCMWdec12_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STCMWdec12_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STCMWdec12_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STCMWdec13_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STCMWdec13_1(bytes, ctx),
        1 => match_node_STCMWdec13_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STCMWdec13_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STCMWdec13_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STCMWdec14_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STCMWdec14_1(bytes, ctx),
        1 => match_node_STCMWdec14_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STCMWdec14_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STCMWdec14_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STCMWdec15_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STCMWdec15_1(bytes, ctx),
        1 => match_node_STCMWdec15_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STCMWdec15_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STCMWdec15_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STCMWdec2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STCMWdec2_1(bytes, ctx),
        1 => match_node_STCMWdec2_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STCMWdec2_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STCMWdec2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STCMWdec3_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STCMWdec3_1(bytes, ctx),
        1 => match_node_STCMWdec3_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STCMWdec3_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STCMWdec3_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STCMWdec4_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STCMWdec4_1(bytes, ctx),
        1 => match_node_STCMWdec4_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STCMWdec4_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STCMWdec4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STCMWdec5_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STCMWdec5_1(bytes, ctx),
        1 => match_node_STCMWdec5_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STCMWdec5_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STCMWdec5_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STCMWdec6_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STCMWdec6_1(bytes, ctx),
        1 => match_node_STCMWdec6_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STCMWdec6_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STCMWdec6_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STCMWdec7_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STCMWdec7_1(bytes, ctx),
        1 => match_node_STCMWdec7_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STCMWdec7_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STCMWdec7_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STCMWdec8_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STCMWdec8_1(bytes, ctx),
        1 => match_node_STCMWdec8_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STCMWdec8_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STCMWdec8_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STCMWdec9_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STCMWdec9_1(bytes, ctx),
        1 => match_node_STCMWdec9_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STCMWdec9_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STCMWdec9_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STCMmm_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (19 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 19) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=19, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STCMmm_1(bytes, ctx),
        1 => match_node_STCMmm_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STCMmm_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_STCMmm_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_STHHD_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_STMdec0_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (31 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 31) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=31, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STMdec0_1(bytes, ctx),
        1 => match_node_STMdec0_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STMdec0_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STMdec0_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STMdec1_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (30 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 30) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=30, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STMdec1_1(bytes, ctx),
        1 => match_node_STMdec1_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STMdec1_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STMdec1_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STMdec10_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (21 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 21) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=21, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STMdec10_1(bytes, ctx),
        1 => match_node_STMdec10_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STMdec10_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STMdec10_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STMdec11_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STMdec11_1(bytes, ctx),
        1 => match_node_STMdec11_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STMdec11_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STMdec11_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STMdec12_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (19 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 19) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=19, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STMdec12_1(bytes, ctx),
        1 => match_node_STMdec12_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STMdec12_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STMdec12_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STMdec13_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (18 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 18) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=18, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STMdec13_1(bytes, ctx),
        1 => match_node_STMdec13_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STMdec13_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STMdec13_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STMdec14_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (17 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 17) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=17, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STMdec14_1(bytes, ctx),
        1 => match_node_STMdec14_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STMdec14_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STMdec14_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STMdec15_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (16 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 16) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=16, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STMdec15_1(bytes, ctx),
        1 => match_node_STMdec15_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STMdec15_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STMdec15_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STMdec2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (29 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 29) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=29, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STMdec2_1(bytes, ctx),
        1 => match_node_STMdec2_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STMdec2_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STMdec2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STMdec3_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (28 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 28) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=28, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STMdec3_1(bytes, ctx),
        1 => match_node_STMdec3_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STMdec3_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STMdec3_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STMdec4_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (27 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 27) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=27, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STMdec4_1(bytes, ctx),
        1 => match_node_STMdec4_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STMdec4_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STMdec4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STMdec5_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (26 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 26) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=26, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STMdec5_1(bytes, ctx),
        1 => match_node_STMdec5_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STMdec5_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STMdec5_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STMdec6_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=25, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STMdec6_1(bytes, ctx),
        1 => match_node_STMdec6_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STMdec6_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STMdec6_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STMdec7_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=24, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STMdec7_1(bytes, ctx),
        1 => match_node_STMdec7_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STMdec7_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STMdec7_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STMdec8_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (23 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 23) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=23, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STMdec8_1(bytes, ctx),
        1 => match_node_STMdec8_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STMdec8_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STMdec8_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STMdec9_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (22 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 22) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=22, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STMdec9_1(bytes, ctx),
        1 => match_node_STMdec9_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STMdec9_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STMdec9_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STMdecdisp_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_STMinc0_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (31 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 31) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=31, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STMinc0_1(bytes, ctx),
        1 => match_node_STMinc0_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STMinc0_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STMinc0_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STMinc1_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (30 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 30) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=30, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STMinc1_1(bytes, ctx),
        1 => match_node_STMinc1_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STMinc1_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STMinc1_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STMinc10_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (21 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 21) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=21, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STMinc10_1(bytes, ctx),
        1 => match_node_STMinc10_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STMinc10_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STMinc10_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STMinc11_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STMinc11_1(bytes, ctx),
        1 => match_node_STMinc11_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STMinc11_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STMinc11_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STMinc12_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (19 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 19) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=19, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STMinc12_1(bytes, ctx),
        1 => match_node_STMinc12_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STMinc12_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STMinc12_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STMinc13_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (18 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 18) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=18, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STMinc13_1(bytes, ctx),
        1 => match_node_STMinc13_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STMinc13_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STMinc13_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STMinc14_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (17 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 17) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=17, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STMinc14_1(bytes, ctx),
        1 => match_node_STMinc14_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STMinc14_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STMinc14_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STMinc15_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (16 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 16) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=16, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STMinc15_1(bytes, ctx),
        1 => match_node_STMinc15_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STMinc15_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STMinc15_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STMinc2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (29 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 29) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=29, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STMinc2_1(bytes, ctx),
        1 => match_node_STMinc2_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STMinc2_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STMinc2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STMinc3_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (28 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 28) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=28, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STMinc3_1(bytes, ctx),
        1 => match_node_STMinc3_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STMinc3_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STMinc3_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STMinc4_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (27 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 27) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=27, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STMinc4_1(bytes, ctx),
        1 => match_node_STMinc4_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STMinc4_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STMinc4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STMinc5_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (26 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 26) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=26, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STMinc5_1(bytes, ctx),
        1 => match_node_STMinc5_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STMinc5_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STMinc5_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STMinc6_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=25, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STMinc6_1(bytes, ctx),
        1 => match_node_STMinc6_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STMinc6_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STMinc6_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STMinc7_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=24, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STMinc7_1(bytes, ctx),
        1 => match_node_STMinc7_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STMinc7_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STMinc7_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STMinc8_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (23 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 23) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=23, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STMinc8_1(bytes, ctx),
        1 => match_node_STMinc8_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STMinc8_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STMinc8_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STMinc9_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (22 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 22) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=22, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STMinc9_1(bytes, ctx),
        1 => match_node_STMinc9_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STMinc9_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STMinc9_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STMincdisp_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_STXP_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=2, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STXP_1(bytes, ctx),
        1 => match_node_STXP_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STXP_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STXP_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_STYP_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=3, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_STYP_1(bytes, ctx),
        1 => match_node_STYP_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_STYP_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_STYP_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_StoreAddress_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_XPART_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_XPART_1(bytes, ctx),
        1 => match_node_XPART_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_XPART_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_XPART_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_YPART_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_YPART_1(bytes, ctx),
        1 => match_node_YPART_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_YPART_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_YPART_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_cc3_0_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 7;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_cc3_0_1(bytes, ctx),
        1 => match_node_cc3_0_2(bytes, ctx),
        2 => match_node_cc3_0_3(bytes, ctx),
        3 => match_node_cc3_0_4(bytes, ctx),
        4 => match_node_cc3_0_5(bytes, ctx),
        5 => match_node_cc3_0_6(bytes, ctx),
        6 => match_node_cc3_0_7(bytes, ctx),
        7 => match_node_cc3_0_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_cc3_0_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_cc3_0_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_cc3_0_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_cc3_0_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_cc3_0_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 4");
    4
}

fn match_node_cc3_0_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_cc3_0_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 6");
    6
}

fn match_node_cc3_0_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 7");
    7
}

fn match_node_cc4_0_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 15;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_cc4_0_1(bytes, ctx),
        1 => match_node_cc4_0_2(bytes, ctx),
        2 => match_node_cc4_0_3(bytes, ctx),
        3 => match_node_cc4_0_4(bytes, ctx),
        4 => match_node_cc4_0_5(bytes, ctx),
        5 => match_node_cc4_0_6(bytes, ctx),
        6 => match_node_cc4_0_7(bytes, ctx),
        7 => match_node_cc4_0_8(bytes, ctx),
        8 => match_node_cc4_0_9(bytes, ctx),
        9 => match_node_cc4_0_10(bytes, ctx),
        10 => match_node_cc4_0_11(bytes, ctx),
        11 => match_node_cc4_0_12(bytes, ctx),
        12 => match_node_cc4_0_13(bytes, ctx),
        13 => match_node_cc4_0_14(bytes, ctx),
        14 => match_node_cc4_0_15(bytes, ctx),
        15 => match_node_cc4_0_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_cc4_0_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_cc4_0_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_cc4_0_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_cc4_0_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_cc4_0_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 4");
    4
}

fn match_node_cc4_0_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_cc4_0_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 6");
    6
}

fn match_node_cc4_0_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 7");
    7
}

fn match_node_cc4_0_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 8");
    8
}

fn match_node_cc4_0_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 9");
    9
}

fn match_node_cc4_0_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 10");
    10
}

fn match_node_cc4_0_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 11");
    11
}

fn match_node_cc4_0_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 12");
    12
}

fn match_node_cc4_0_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 13");
    13
}

fn match_node_cc4_0_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 14");
    14
}

fn match_node_cc4_0_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 15");
    15
}

fn match_node_cc4_4_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 15;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_cc4_4_1(bytes, ctx),
        1 => match_node_cc4_4_2(bytes, ctx),
        2 => match_node_cc4_4_3(bytes, ctx),
        3 => match_node_cc4_4_4(bytes, ctx),
        4 => match_node_cc4_4_5(bytes, ctx),
        5 => match_node_cc4_4_6(bytes, ctx),
        6 => match_node_cc4_4_7(bytes, ctx),
        7 => match_node_cc4_4_8(bytes, ctx),
        8 => match_node_cc4_4_9(bytes, ctx),
        9 => match_node_cc4_4_10(bytes, ctx),
        10 => match_node_cc4_4_11(bytes, ctx),
        11 => match_node_cc4_4_12(bytes, ctx),
        12 => match_node_cc4_4_13(bytes, ctx),
        13 => match_node_cc4_4_14(bytes, ctx),
        14 => match_node_cc4_4_15(bytes, ctx),
        15 => match_node_cc4_4_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_cc4_4_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_cc4_4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_cc4_4_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_cc4_4_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_cc4_4_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 4");
    4
}

fn match_node_cc4_4_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_cc4_4_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 6");
    6
}

fn match_node_cc4_4_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 7");
    7
}

fn match_node_cc4_4_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 8");
    8
}

fn match_node_cc4_4_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 9");
    9
}

fn match_node_cc4_4_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 10");
    10
}

fn match_node_cc4_4_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 11");
    11
}

fn match_node_cc4_4_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 12");
    12
}

fn match_node_cc4_4_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 13");
    13
}

fn match_node_cc4_4_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 14");
    14
}

fn match_node_cc4_4_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 15");
    15
}

fn match_node_cc4_e12_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 15;
    eprintln!("Trace node 0: SlaInstructionBits start=0, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_cc4_e12_1(bytes, ctx),
        1 => match_node_cc4_e12_2(bytes, ctx),
        2 => match_node_cc4_e12_3(bytes, ctx),
        3 => match_node_cc4_e12_4(bytes, ctx),
        4 => match_node_cc4_e12_5(bytes, ctx),
        5 => match_node_cc4_e12_6(bytes, ctx),
        6 => match_node_cc4_e12_7(bytes, ctx),
        7 => match_node_cc4_e12_8(bytes, ctx),
        8 => match_node_cc4_e12_9(bytes, ctx),
        9 => match_node_cc4_e12_10(bytes, ctx),
        10 => match_node_cc4_e12_11(bytes, ctx),
        11 => match_node_cc4_e12_12(bytes, ctx),
        12 => match_node_cc4_e12_13(bytes, ctx),
        13 => match_node_cc4_e12_14(bytes, ctx),
        14 => match_node_cc4_e12_15(bytes, ctx),
        15 => match_node_cc4_e12_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_cc4_e12_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_cc4_e12_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_cc4_e12_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_cc4_e12_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_cc4_e12_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 4");
    4
}

fn match_node_cc4_e12_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_cc4_e12_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 6");
    6
}

fn match_node_cc4_e12_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 7");
    7
}

fn match_node_cc4_e12_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 8");
    8
}

fn match_node_cc4_e12_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 9");
    9
}

fn match_node_cc4_e12_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 10");
    10
}

fn match_node_cc4_e12_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 11");
    11
}

fn match_node_cc4_e12_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 12");
    12
}

fn match_node_cc4_e12_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 13");
    13
}

fn match_node_cc4_e12_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 14");
    14
}

fn match_node_cc4_e12_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 15");
    15
}

fn match_node_ecc4_4_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 15;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ecc4_4_1(bytes, ctx),
        1 => match_node_ecc4_4_2(bytes, ctx),
        2 => match_node_ecc4_4_3(bytes, ctx),
        3 => match_node_ecc4_4_4(bytes, ctx),
        4 => match_node_ecc4_4_5(bytes, ctx),
        5 => match_node_ecc4_4_6(bytes, ctx),
        6 => match_node_ecc4_4_7(bytes, ctx),
        7 => match_node_ecc4_4_8(bytes, ctx),
        8 => match_node_ecc4_4_9(bytes, ctx),
        9 => match_node_ecc4_4_10(bytes, ctx),
        10 => match_node_ecc4_4_11(bytes, ctx),
        11 => match_node_ecc4_4_12(bytes, ctx),
        12 => match_node_ecc4_4_13(bytes, ctx),
        13 => match_node_ecc4_4_14(bytes, ctx),
        14 => match_node_ecc4_4_15(bytes, ctx),
        15 => match_node_ecc4_4_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ecc4_4_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_ecc4_4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_ecc4_4_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_ecc4_4_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_ecc4_4_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 4");
    4
}

fn match_node_ecc4_4_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_ecc4_4_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 6");
    6
}

fn match_node_ecc4_4_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 7");
    7
}

fn match_node_ecc4_4_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 8");
    8
}

fn match_node_ecc4_4_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 9");
    9
}

fn match_node_ecc4_4_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 10");
    10
}

fn match_node_ecc4_4_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 11");
    11
}

fn match_node_ecc4_4_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 12");
    12
}

fn match_node_ecc4_4_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 13");
    13
}

fn match_node_ecc4_4_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 14");
    14
}

fn match_node_ecc4_4_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 15");
    15
}

fn match_node_ecc8_4_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 15;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ecc8_4_1(bytes, ctx),
        1 => match_node_ecc8_4_2(bytes, ctx),
        2 => match_node_ecc8_4_3(bytes, ctx),
        3 => match_node_ecc8_4_4(bytes, ctx),
        4 => match_node_ecc8_4_5(bytes, ctx),
        5 => match_node_ecc8_4_6(bytes, ctx),
        6 => match_node_ecc8_4_7(bytes, ctx),
        7 => match_node_ecc8_4_8(bytes, ctx),
        8 => match_node_ecc8_4_9(bytes, ctx),
        9 => match_node_ecc8_4_10(bytes, ctx),
        10 => match_node_ecc8_4_11(bytes, ctx),
        11 => match_node_ecc8_4_12(bytes, ctx),
        12 => match_node_ecc8_4_13(bytes, ctx),
        13 => match_node_ecc8_4_14(bytes, ctx),
        14 => match_node_ecc8_4_15(bytes, ctx),
        15 => match_node_ecc8_4_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ecc8_4_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_ecc8_4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_ecc8_4_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_ecc8_4_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_ecc8_4_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 4");
    4
}

fn match_node_ecc8_4_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_ecc8_4_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 6");
    6
}

fn match_node_ecc8_4_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 7");
    7
}

fn match_node_ecc8_4_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 8");
    8
}

fn match_node_ecc8_4_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 9");
    9
}

fn match_node_ecc8_4_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 10");
    10
}

fn match_node_ecc8_4_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 11");
    11
}

fn match_node_ecc8_4_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 12");
    12
}

fn match_node_ecc8_4_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 13");
    13
}

fn match_node_ecc8_4_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 14");
    14
}

fn match_node_ecc8_4_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 15");
    15
}

fn match_node_instruction_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 7;
    eprintln!("Trace node 0: SlaInstructionBits start=0, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1(bytes, ctx),
        1 => match_node_instruction_32(bytes, ctx),
        2 => match_node_instruction_37(bytes, ctx),
        3 => match_node_instruction_68(bytes, ctx),
        4 => match_node_instruction_69(bytes, ctx),
        5 => match_node_instruction_74(bytes, ctx),
        6 => match_node_instruction_97(bytes, ctx),
        7 => match_node_instruction_144(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 3;
    eprintln!("Trace node 1: SlaInstructionBits start=7, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_2(bytes, ctx),
        1 => match_node_instruction_13(bytes, ctx),
        2 => match_node_instruction_22(bytes, ctx),
        3 => match_node_instruction_31(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_2(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 7;
    eprintln!("Trace node 2: SlaInstructionBits start=9, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_3(bytes, ctx),
        1 => match_node_instruction_6(bytes, ctx),
        2 => match_node_instruction_7(bytes, ctx),
        3 => match_node_instruction_8(bytes, ctx),
        4 => match_node_instruction_9(bytes, ctx),
        5 => match_node_instruction_10(bytes, ctx),
        6 => match_node_instruction_11(bytes, ctx),
        7 => match_node_instruction_12(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_3(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 3: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_4(bytes, ctx),
        1 => match_node_instruction_5(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 246");
    246
}

fn match_node_instruction_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 6");
    6
}

fn match_node_instruction_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 39");
    39
}

fn match_node_instruction_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 27");
    27
}

fn match_node_instruction_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 15");
    15
}

fn match_node_instruction_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 96");
    96
}

fn match_node_instruction_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 84");
    84
}

fn match_node_instruction_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 65");
    65
}

fn match_node_instruction_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 107");
    107
}

fn match_node_instruction_13(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 7;
    eprintln!("Trace node 13: SlaInstructionBits start=9, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_14(bytes, ctx),
        1 => match_node_instruction_15(bytes, ctx),
        2 => match_node_instruction_16(bytes, ctx),
        3 => match_node_instruction_17(bytes, ctx),
        4 => match_node_instruction_18(bytes, ctx),
        5 => match_node_instruction_19(bytes, ctx),
        6 => match_node_instruction_20(bytes, ctx),
        7 => match_node_instruction_21(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 81");
    81
}

fn match_node_instruction_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 140");
    140
}

fn match_node_instruction_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 213");
    213
}

fn match_node_instruction_17(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 17: Terminal matched constructor ID 205");
    205
}

fn match_node_instruction_18(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 18: Terminal matched constructor ID 194");
    194
}

fn match_node_instruction_19(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 19: Terminal matched constructor ID 214");
    214
}

fn match_node_instruction_20(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 20: Terminal matched constructor ID 206");
    206
}

fn match_node_instruction_21(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 21: Terminal matched constructor ID 195");
    195
}

fn match_node_instruction_22(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 7;
    eprintln!("Trace node 22: SlaInstructionBits start=9, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_23(bytes, ctx),
        1 => match_node_instruction_24(bytes, ctx),
        2 => match_node_instruction_25(bytes, ctx),
        3 => match_node_instruction_26(bytes, ctx),
        4 => match_node_instruction_27(bytes, ctx),
        5 => match_node_instruction_28(bytes, ctx),
        6 => match_node_instruction_29(bytes, ctx),
        7 => match_node_instruction_30(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_23(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 23: Terminal matched constructor ID 171");
    171
}

fn match_node_instruction_24(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 24: Terminal matched constructor ID 164");
    164
}

fn match_node_instruction_25(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 25: Terminal matched constructor ID 158");
    158
}

fn match_node_instruction_26(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 26: Terminal matched constructor ID 149");
    149
}

fn match_node_instruction_27(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 27: Terminal matched constructor ID 173");
    173
}

fn match_node_instruction_28(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 28: Terminal matched constructor ID 165");
    165
}

fn match_node_instruction_29(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 29: Terminal matched constructor ID 159");
    159
}

fn match_node_instruction_30(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 30: Terminal matched constructor ID 150");
    150
}

fn match_node_instruction_31(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 31: Terminal matched constructor ID 151");
    151
}

fn match_node_instruction_32(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 1;
    eprintln!("Trace node 32: SlaInstructionBits start=3, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_33(bytes, ctx),
        1 => match_node_instruction_36(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_33(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 33: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_34(bytes, ctx),
        1 => match_node_instruction_35(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_34(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 34: Terminal matched constructor ID 42");
    42
}

fn match_node_instruction_35(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 35: Terminal matched constructor ID 44");
    44
}

fn match_node_instruction_36(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 36: Terminal matched constructor ID 138");
    138
}

fn match_node_instruction_37(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 3;
    eprintln!("Trace node 37: SlaInstructionBits start=3, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_38(bytes, ctx),
        1 => match_node_instruction_39(bytes, ctx),
        2 => match_node_instruction_40(bytes, ctx),
        3 => match_node_instruction_41(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_38(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 38: Terminal matched constructor ID 186");
    186
}

fn match_node_instruction_39(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 39: Terminal matched constructor ID 184");
    184
}

fn match_node_instruction_40(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 40: Terminal matched constructor ID 220");
    220
}

fn match_node_instruction_41(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 1;
    eprintln!("Trace node 41: SlaInstructionBits start=5, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_42(bytes, ctx),
        1 => match_node_instruction_43(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_42(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 42: Terminal matched constructor ID 16");
    16
}

fn match_node_instruction_43(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 3;
    eprintln!("Trace node 43: SlaInstructionBits start=6, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_44(bytes, ctx),
        1 => match_node_instruction_61(bytes, ctx),
        2 => match_node_instruction_66(bytes, ctx),
        3 => match_node_instruction_67(bytes, ctx),
        _ => -1,
    }
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
    eprintln!("Trace node 45: Terminal matched constructor ID 2");
    2
}

fn match_node_instruction_46(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 46: Terminal matched constructor ID 37");
    37
}

fn match_node_instruction_47(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 47: Terminal matched constructor ID 19");
    19
}

fn match_node_instruction_48(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 48: Terminal matched constructor ID 25");
    25
}

fn match_node_instruction_49(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 49: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_50(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 50: Terminal matched constructor ID 117");
    117
}

fn match_node_instruction_51(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 51: Terminal matched constructor ID 115");
    115
}

fn match_node_instruction_52(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 52: Terminal matched constructor ID 116");
    116
}

fn match_node_instruction_53(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 53: Terminal matched constructor ID 114");
    114
}

fn match_node_instruction_54(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 54: Terminal matched constructor ID 112");
    112
}

fn match_node_instruction_55(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 55: Terminal matched constructor ID 126");
    126
}

fn match_node_instruction_56(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 56: Terminal matched constructor ID 124");
    124
}

fn match_node_instruction_57(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 57: Terminal matched constructor ID 125");
    125
}

fn match_node_instruction_58(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 58: Terminal matched constructor ID 82");
    82
}

fn match_node_instruction_59(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 59: Terminal matched constructor ID 53");
    53
}

fn match_node_instruction_60(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 60: Terminal matched constructor ID 136");
    136
}

fn match_node_instruction_61(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 61: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_62(bytes, ctx),
        1 => match_node_instruction_63(bytes, ctx),
        2 => match_node_instruction_64(bytes, ctx),
        3 => match_node_instruction_65(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_62(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 62: Terminal matched constructor ID 137");
    137
}

fn match_node_instruction_63(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 63: Terminal matched constructor ID 286");
    286
}

fn match_node_instruction_64(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 64: Terminal matched constructor ID 244");
    244
}

fn match_node_instruction_65(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 65: Terminal matched constructor ID 243");
    243
}

fn match_node_instruction_66(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 66: Terminal matched constructor ID 282");
    282
}

fn match_node_instruction_67(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 67: Terminal matched constructor ID 233");
    233
}

fn match_node_instruction_68(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 68: Terminal matched constructor ID 175");
    175
}

fn match_node_instruction_69(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 69: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_70(bytes, ctx),
        1 => match_node_instruction_73(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_70(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 70: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_71(bytes, ctx),
        1 => match_node_instruction_72(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_71(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 71: Terminal matched constructor ID 166");
    166
}

fn match_node_instruction_72(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 72: Terminal matched constructor ID 160");
    160
}

fn match_node_instruction_73(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 73: Terminal matched constructor ID 215");
    215
}

fn match_node_instruction_74(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 3;
    eprintln!("Trace node 74: SlaInstructionBits start=7, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_75(bytes, ctx),
        1 => match_node_instruction_76(bytes, ctx),
        2 => match_node_instruction_77(bytes, ctx),
        3 => match_node_instruction_92(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_75(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 75: Terminal matched constructor ID 207");
    207
}

fn match_node_instruction_76(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 76: Terminal matched constructor ID 196");
    196
}

fn match_node_instruction_77(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 3;
    eprintln!("Trace node 77: SlaInstructionBits start=9, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_78(bytes, ctx),
        1 => match_node_instruction_85(bytes, ctx),
        2 => match_node_instruction_90(bytes, ctx),
        3 => match_node_instruction_91(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_78(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 78: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_79(bytes, ctx),
        1 => match_node_instruction_82(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_79(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 79: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_80(bytes, ctx),
        1 => match_node_instruction_81(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_80(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 80: Terminal matched constructor ID 146");
    146
}

fn match_node_instruction_81(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 81: Terminal matched constructor ID 144");
    144
}

fn match_node_instruction_82(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 82: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_83(bytes, ctx),
        1 => match_node_instruction_84(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_83(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 83: Terminal matched constructor ID 145");
    145
}

fn match_node_instruction_84(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 84: Terminal matched constructor ID 202");
    202
}

fn match_node_instruction_85(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 85: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_86(bytes, ctx),
        1 => match_node_instruction_89(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_86(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 86: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_87(bytes, ctx),
        1 => match_node_instruction_88(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_87(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 87: Terminal matched constructor ID 200");
    200
}

fn match_node_instruction_88(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 88: Terminal matched constructor ID 201");
    201
}

fn match_node_instruction_89(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 89: Terminal matched constructor ID 59");
    59
}

fn match_node_instruction_90(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 90: Terminal matched constructor ID 128");
    128
}

fn match_node_instruction_91(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 91: Terminal matched constructor ID 131");
    131
}

fn match_node_instruction_92(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 3;
    eprintln!("Trace node 92: SlaInstructionBits start=9, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_93(bytes, ctx),
        1 => match_node_instruction_94(bytes, ctx),
        2 => match_node_instruction_95(bytes, ctx),
        3 => match_node_instruction_96(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_93(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 93: Terminal matched constructor ID 134");
    134
}

fn match_node_instruction_94(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 94: Terminal matched constructor ID 123");
    123
}

fn match_node_instruction_95(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 95: Terminal matched constructor ID 118");
    118
}

fn match_node_instruction_96(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 96: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_97(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 1;
    eprintln!("Trace node 97: SlaInstructionBits start=3, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_98(bytes, ctx),
        1 => match_node_instruction_103(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_98(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 98: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_99(bytes, ctx),
        1 => match_node_instruction_100(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_99(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 99: Terminal matched constructor ID 283");
    283
}

fn match_node_instruction_100(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 100: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_101(bytes, ctx),
        1 => match_node_instruction_102(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_101(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 101: Terminal matched constructor ID 285");
    285
}

fn match_node_instruction_102(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 102: Terminal matched constructor ID 289");
    289
}

fn match_node_instruction_103(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 15;
    eprintln!("Trace node 103: SlaInstructionBits start=12, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_104(bytes, ctx),
        1 => match_node_instruction_105(bytes, ctx),
        2 => match_node_instruction_106(bytes, ctx),
        3 => match_node_instruction_107(bytes, ctx),
        4 => match_node_instruction_132(bytes, ctx),
        5 => match_node_instruction_133(bytes, ctx),
        6 => match_node_instruction_134(bytes, ctx),
        7 => match_node_instruction_135(bytes, ctx),
        8 => match_node_instruction_136(bytes, ctx),
        9 => match_node_instruction_137(bytes, ctx),
        10 => match_node_instruction_138(bytes, ctx),
        11 => match_node_instruction_139(bytes, ctx),
        12 => match_node_instruction_140(bytes, ctx),
        13 => match_node_instruction_141(bytes, ctx),
        14 => match_node_instruction_142(bytes, ctx),
        15 => match_node_instruction_143(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_104(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 104: Terminal matched constructor ID 281");
    281
}

fn match_node_instruction_105(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 105: Terminal matched constructor ID 227");
    227
}

fn match_node_instruction_106(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 106: Terminal matched constructor ID 225");
    225
}

fn match_node_instruction_107(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 3;
    eprintln!("Trace node 107: SlaInstructionBits start=5, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_108(bytes, ctx),
        1 => match_node_instruction_109(bytes, ctx),
        2 => match_node_instruction_110(bytes, ctx),
        3 => match_node_instruction_111(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_108(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 108: Terminal matched constructor ID 237");
    237
}

fn match_node_instruction_109(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 109: Terminal matched constructor ID 249");
    249
}

fn match_node_instruction_110(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 110: Terminal matched constructor ID 236");
    236
}

fn match_node_instruction_111(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 3;
    eprintln!("Trace node 111: SlaInstructionBits start=7, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_112(bytes, ctx),
        1 => match_node_instruction_121(bytes, ctx),
        2 => match_node_instruction_122(bytes, ctx),
        3 => match_node_instruction_131(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_112(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 7;
    eprintln!("Trace node 112: SlaInstructionBits start=9, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_113(bytes, ctx),
        1 => match_node_instruction_114(bytes, ctx),
        2 => match_node_instruction_115(bytes, ctx),
        3 => match_node_instruction_116(bytes, ctx),
        4 => match_node_instruction_117(bytes, ctx),
        5 => match_node_instruction_118(bytes, ctx),
        6 => match_node_instruction_119(bytes, ctx),
        7 => match_node_instruction_120(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_113(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 113: Terminal matched constructor ID 292");
    292
}

fn match_node_instruction_114(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 114: Terminal matched constructor ID 293");
    293
}

fn match_node_instruction_115(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 115: Terminal matched constructor ID 291");
    291
}

fn match_node_instruction_116(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 116: Terminal matched constructor ID 294");
    294
}

fn match_node_instruction_117(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 117: Terminal matched constructor ID 251");
    251
}

fn match_node_instruction_118(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 118: Terminal matched constructor ID 252");
    252
}

fn match_node_instruction_119(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 119: Terminal matched constructor ID 253");
    253
}

fn match_node_instruction_120(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 120: Terminal matched constructor ID 234");
    234
}

fn match_node_instruction_121(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 121: Terminal matched constructor ID 296");
    296
}

fn match_node_instruction_122(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 7;
    eprintln!("Trace node 122: SlaInstructionBits start=9, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_123(bytes, ctx),
        1 => match_node_instruction_124(bytes, ctx),
        2 => match_node_instruction_125(bytes, ctx),
        3 => match_node_instruction_126(bytes, ctx),
        4 => match_node_instruction_127(bytes, ctx),
        5 => match_node_instruction_128(bytes, ctx),
        6 => match_node_instruction_129(bytes, ctx),
        7 => match_node_instruction_130(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_123(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 123: Terminal matched constructor ID 245");
    245
}

fn match_node_instruction_124(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 124: Terminal matched constructor ID 297");
    297
}

fn match_node_instruction_125(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 125: Terminal matched constructor ID 298");
    298
}

fn match_node_instruction_126(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 126: Terminal matched constructor ID 295");
    295
}

fn match_node_instruction_127(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 127: Terminal matched constructor ID 238");
    238
}

fn match_node_instruction_128(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 128: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_129(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 129: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_130(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 130: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_131(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 131: Terminal matched NOTHING");
    -1
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
    eprintln!("Trace node 134: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_135(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 135: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_136(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 136: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_137(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 137: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_138(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 138: Terminal matched constructor ID 226");
    226
}

fn match_node_instruction_139(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 139: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_140(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 140: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_141(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 141: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_142(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 142: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_143(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 143: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_144(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 15;
    eprintln!("Trace node 144: SlaInstructionBits start=7, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_145(bytes, ctx),
        1 => match_node_instruction_412(bytes, ctx),
        2 => match_node_instruction_413(bytes, ctx),
        3 => match_node_instruction_414(bytes, ctx),
        4 => match_node_instruction_415(bytes, ctx),
        5 => match_node_instruction_416(bytes, ctx),
        6 => match_node_instruction_417(bytes, ctx),
        7 => match_node_instruction_420(bytes, ctx),
        8 => match_node_instruction_425(bytes, ctx),
        9 => match_node_instruction_428(bytes, ctx),
        10 => match_node_instruction_431(bytes, ctx),
        11 => match_node_instruction_434(bytes, ctx),
        12 => match_node_instruction_437(bytes, ctx),
        13 => match_node_instruction_438(bytes, ctx),
        14 => match_node_instruction_681(bytes, ctx),
        15 => match_node_instruction_716(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_145(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 145: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_146(bytes, ctx),
        1 => match_node_instruction_331(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_146(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (18 + 6 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 18) & 63;
    eprintln!("Trace node 146: SlaInstructionBits start=18, size=6, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_147(bytes, ctx),
        1 => match_node_instruction_152(bytes, ctx),
        2 => match_node_instruction_157(bytes, ctx),
        3 => match_node_instruction_162(bytes, ctx),
        4 => match_node_instruction_167(bytes, ctx),
        5 => match_node_instruction_172(bytes, ctx),
        6 => match_node_instruction_177(bytes, ctx),
        7 => match_node_instruction_182(bytes, ctx),
        8 => match_node_instruction_187(bytes, ctx),
        9 => match_node_instruction_192(bytes, ctx),
        10 => match_node_instruction_197(bytes, ctx),
        11 => match_node_instruction_202(bytes, ctx),
        12 => match_node_instruction_207(bytes, ctx),
        13 => match_node_instruction_212(bytes, ctx),
        14 => match_node_instruction_217(bytes, ctx),
        15 => match_node_instruction_222(bytes, ctx),
        16 => match_node_instruction_225(bytes, ctx),
        17 => match_node_instruction_226(bytes, ctx),
        18 => match_node_instruction_227(bytes, ctx),
        19 => match_node_instruction_228(bytes, ctx),
        20 => match_node_instruction_229(bytes, ctx),
        21 => match_node_instruction_230(bytes, ctx),
        22 => match_node_instruction_231(bytes, ctx),
        23 => match_node_instruction_232(bytes, ctx),
        24 => match_node_instruction_233(bytes, ctx),
        25 => match_node_instruction_234(bytes, ctx),
        26 => match_node_instruction_235(bytes, ctx),
        27 => match_node_instruction_236(bytes, ctx),
        28 => match_node_instruction_237(bytes, ctx),
        29 => match_node_instruction_238(bytes, ctx),
        30 => match_node_instruction_239(bytes, ctx),
        31 => match_node_instruction_240(bytes, ctx),
        32 => match_node_instruction_241(bytes, ctx),
        33 => match_node_instruction_258(bytes, ctx),
        34 => match_node_instruction_263(bytes, ctx),
        35 => match_node_instruction_268(bytes, ctx),
        36 => match_node_instruction_285(bytes, ctx),
        37 => match_node_instruction_304(bytes, ctx),
        38 => match_node_instruction_305(bytes, ctx),
        39 => match_node_instruction_306(bytes, ctx),
        40 => match_node_instruction_307(bytes, ctx),
        41 => match_node_instruction_308(bytes, ctx),
        42 => match_node_instruction_309(bytes, ctx),
        43 => match_node_instruction_310(bytes, ctx),
        44 => match_node_instruction_311(bytes, ctx),
        45 => match_node_instruction_312(bytes, ctx),
        46 => match_node_instruction_313(bytes, ctx),
        47 => match_node_instruction_314(bytes, ctx),
        48 => match_node_instruction_315(bytes, ctx),
        49 => match_node_instruction_316(bytes, ctx),
        50 => match_node_instruction_317(bytes, ctx),
        51 => match_node_instruction_318(bytes, ctx),
        52 => match_node_instruction_319(bytes, ctx),
        53 => match_node_instruction_320(bytes, ctx),
        54 => match_node_instruction_321(bytes, ctx),
        55 => match_node_instruction_322(bytes, ctx),
        56 => match_node_instruction_323(bytes, ctx),
        57 => match_node_instruction_324(bytes, ctx),
        58 => match_node_instruction_325(bytes, ctx),
        59 => match_node_instruction_326(bytes, ctx),
        60 => match_node_instruction_327(bytes, ctx),
        61 => match_node_instruction_328(bytes, ctx),
        62 => match_node_instruction_329(bytes, ctx),
        63 => match_node_instruction_330(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_147(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 3;
    eprintln!("Trace node 147: SlaInstructionBits start=24, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_148(bytes, ctx),
        1 => match_node_instruction_149(bytes, ctx),
        2 => match_node_instruction_150(bytes, ctx),
        3 => match_node_instruction_151(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_148(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 148: Terminal matched constructor ID 8");
    8
}

fn match_node_instruction_149(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 149: Terminal matched constructor ID 4");
    4
}

fn match_node_instruction_150(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 150: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_151(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 151: Terminal matched constructor ID 366");
    366
}

fn match_node_instruction_152(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 3;
    eprintln!("Trace node 152: SlaInstructionBits start=24, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_153(bytes, ctx),
        1 => match_node_instruction_154(bytes, ctx),
        2 => match_node_instruction_155(bytes, ctx),
        3 => match_node_instruction_156(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_153(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 153: Terminal matched constructor ID 41");
    41
}

fn match_node_instruction_154(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 154: Terminal matched constructor ID 35");
    35
}

fn match_node_instruction_155(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 155: Terminal matched constructor ID 357");
    357
}

fn match_node_instruction_156(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 156: Terminal matched constructor ID 371");
    371
}

fn match_node_instruction_157(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 3;
    eprintln!("Trace node 157: SlaInstructionBits start=24, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_158(bytes, ctx),
        1 => match_node_instruction_159(bytes, ctx),
        2 => match_node_instruction_160(bytes, ctx),
        3 => match_node_instruction_161(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_158(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 158: Terminal matched constructor ID 148");
    148
}

fn match_node_instruction_159(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 159: Terminal matched constructor ID 60");
    60
}

fn match_node_instruction_160(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 160: Terminal matched constructor ID 358");
    358
}

fn match_node_instruction_161(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 161: Terminal matched constructor ID 365");
    365
}

fn match_node_instruction_162(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 3;
    eprintln!("Trace node 162: SlaInstructionBits start=24, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_163(bytes, ctx),
        1 => match_node_instruction_164(bytes, ctx),
        2 => match_node_instruction_165(bytes, ctx),
        3 => match_node_instruction_166(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_163(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 163: Terminal matched constructor ID 179");
    179
}

fn match_node_instruction_164(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 164: Terminal matched constructor ID 56");
    56
}

fn match_node_instruction_165(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 165: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_166(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 166: Terminal matched constructor ID 370");
    370
}

fn match_node_instruction_167(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 3;
    eprintln!("Trace node 167: SlaInstructionBits start=24, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_168(bytes, ctx),
        1 => match_node_instruction_169(bytes, ctx),
        2 => match_node_instruction_170(bytes, ctx),
        3 => match_node_instruction_171(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_168(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 168: Terminal matched constructor ID 168");
    168
}

fn match_node_instruction_169(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 169: Terminal matched constructor ID 62");
    62
}

fn match_node_instruction_170(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 170: Terminal matched constructor ID 353");
    353
}

fn match_node_instruction_171(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 171: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_172(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 3;
    eprintln!("Trace node 172: SlaInstructionBits start=24, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_173(bytes, ctx),
        1 => match_node_instruction_174(bytes, ctx),
        2 => match_node_instruction_175(bytes, ctx),
        3 => match_node_instruction_176(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_173(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 173: Terminal matched constructor ID 162");
    162
}

fn match_node_instruction_174(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 174: Terminal matched constructor ID 57");
    57
}

fn match_node_instruction_175(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 175: Terminal matched constructor ID 352");
    352
}

fn match_node_instruction_176(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 176: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_177(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 3;
    eprintln!("Trace node 177: SlaInstructionBits start=24, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_178(bytes, ctx),
        1 => match_node_instruction_179(bytes, ctx),
        2 => match_node_instruction_180(bytes, ctx),
        3 => match_node_instruction_181(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_178(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 178: Terminal matched constructor ID 156");
    156
}

fn match_node_instruction_179(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 179: Terminal matched constructor ID 63");
    63
}

fn match_node_instruction_180(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 180: Terminal matched constructor ID 354");
    354
}

fn match_node_instruction_181(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 181: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_182(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 3;
    eprintln!("Trace node 182: SlaInstructionBits start=24, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_183(bytes, ctx),
        1 => match_node_instruction_184(bytes, ctx),
        2 => match_node_instruction_185(bytes, ctx),
        3 => match_node_instruction_186(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_183(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 183: Terminal matched constructor ID 153");
    153
}

fn match_node_instruction_184(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 184: Terminal matched constructor ID 58");
    58
}

fn match_node_instruction_185(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 185: Terminal matched constructor ID 356");
    356
}

fn match_node_instruction_186(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 186: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_187(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 3;
    eprintln!("Trace node 187: SlaInstructionBits start=24, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_188(bytes, ctx),
        1 => match_node_instruction_189(bytes, ctx),
        2 => match_node_instruction_190(bytes, ctx),
        3 => match_node_instruction_191(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_188(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 188: Terminal matched constructor ID 204");
    204
}

fn match_node_instruction_189(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 189: Terminal matched constructor ID 127");
    127
}

fn match_node_instruction_190(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 190: Terminal matched constructor ID 359");
    359
}

fn match_node_instruction_191(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 191: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_192(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 3;
    eprintln!("Trace node 192: SlaInstructionBits start=24, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_193(bytes, ctx),
        1 => match_node_instruction_194(bytes, ctx),
        2 => match_node_instruction_195(bytes, ctx),
        3 => match_node_instruction_196(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_193(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 193: Terminal matched constructor ID 217");
    217
}

fn match_node_instruction_194(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 194: Terminal matched constructor ID 130");
    130
}

fn match_node_instruction_195(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 195: Terminal matched constructor ID 360");
    360
}

fn match_node_instruction_196(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 196: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_197(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 3;
    eprintln!("Trace node 197: SlaInstructionBits start=24, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_198(bytes, ctx),
        1 => match_node_instruction_199(bytes, ctx),
        2 => match_node_instruction_200(bytes, ctx),
        3 => match_node_instruction_201(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_198(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 198: Terminal matched constructor ID 209");
    209
}

fn match_node_instruction_199(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 199: Terminal matched constructor ID 133");
    133
}

fn match_node_instruction_200(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 200: Terminal matched constructor ID 361");
    361
}

fn match_node_instruction_201(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 201: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_202(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 3;
    eprintln!("Trace node 202: SlaInstructionBits start=24, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_203(bytes, ctx),
        1 => match_node_instruction_204(bytes, ctx),
        2 => match_node_instruction_205(bytes, ctx),
        3 => match_node_instruction_206(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_203(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 203: Terminal matched constructor ID 198");
    198
}

fn match_node_instruction_204(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 204: Terminal matched constructor ID 232");
    232
}

fn match_node_instruction_205(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 205: Terminal matched constructor ID 362");
    362
}

fn match_node_instruction_206(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 206: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_207(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 3;
    eprintln!("Trace node 207: SlaInstructionBits start=24, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_208(bytes, ctx),
        1 => match_node_instruction_209(bytes, ctx),
        2 => match_node_instruction_210(bytes, ctx),
        3 => match_node_instruction_211(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_208(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 208: Terminal matched constructor ID 54");
    54
}

fn match_node_instruction_209(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 209: Terminal matched constructor ID 21");
    21
}

fn match_node_instruction_210(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 210: Terminal matched constructor ID 355");
    355
}

fn match_node_instruction_211(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 211: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_212(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 3;
    eprintln!("Trace node 212: SlaInstructionBits start=24, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_213(bytes, ctx),
        1 => match_node_instruction_214(bytes, ctx),
        2 => match_node_instruction_215(bytes, ctx),
        3 => match_node_instruction_216(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_213(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 213: Terminal matched constructor ID 55");
    55
}

fn match_node_instruction_214(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 214: Terminal matched constructor ID 23");
    23
}

fn match_node_instruction_215(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 215: Terminal matched constructor ID 364");
    364
}

fn match_node_instruction_216(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 216: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_217(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 3;
    eprintln!("Trace node 217: SlaInstructionBits start=24, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_218(bytes, ctx),
        1 => match_node_instruction_219(bytes, ctx),
        2 => match_node_instruction_220(bytes, ctx),
        3 => match_node_instruction_221(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_218(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 218: Terminal matched constructor ID 351");
    351
}

fn match_node_instruction_219(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 219: Terminal matched constructor ID 12");
    12
}

fn match_node_instruction_220(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 220: Terminal matched constructor ID 363");
    363
}

fn match_node_instruction_221(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 221: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_222(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 1;
    eprintln!("Trace node 222: SlaInstructionBits start=24, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_223(bytes, ctx),
        1 => match_node_instruction_224(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_223(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 223: Terminal matched constructor ID 374");
    374
}

fn match_node_instruction_224(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 224: Terminal matched constructor ID 181");
    181
}

fn match_node_instruction_225(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 225: Terminal matched constructor ID 61");
    61
}

fn match_node_instruction_226(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 226: Terminal matched constructor ID 31");
    31
}

fn match_node_instruction_227(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 227: Terminal matched constructor ID 119");
    119
}

fn match_node_instruction_228(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 228: Terminal matched constructor ID 18");
    18
}

fn match_node_instruction_229(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 229: Terminal matched constructor ID 129");
    129
}

fn match_node_instruction_230(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 230: Terminal matched constructor ID 132");
    132
}

fn match_node_instruction_231(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 231: Terminal matched constructor ID 135");
    135
}

fn match_node_instruction_232(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 232: Terminal matched constructor ID 141");
    141
}

fn match_node_instruction_233(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 233: Terminal matched constructor ID 13");
    13
}

fn match_node_instruction_234(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 234: Terminal matched constructor ID 14");
    14
}

fn match_node_instruction_235(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 235: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_236(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 236: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_237(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 237: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_238(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 238: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_239(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 239: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_240(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 240: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_241(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 15;
    eprintln!("Trace node 241: SlaInstructionBits start=24, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_242(bytes, ctx),
        1 => match_node_instruction_243(bytes, ctx),
        2 => match_node_instruction_244(bytes, ctx),
        3 => match_node_instruction_245(bytes, ctx),
        4 => match_node_instruction_246(bytes, ctx),
        5 => match_node_instruction_247(bytes, ctx),
        6 => match_node_instruction_248(bytes, ctx),
        7 => match_node_instruction_249(bytes, ctx),
        8 => match_node_instruction_250(bytes, ctx),
        9 => match_node_instruction_251(bytes, ctx),
        10 => match_node_instruction_252(bytes, ctx),
        11 => match_node_instruction_253(bytes, ctx),
        12 => match_node_instruction_254(bytes, ctx),
        13 => match_node_instruction_255(bytes, ctx),
        14 => match_node_instruction_256(bytes, ctx),
        15 => match_node_instruction_257(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_242(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 242: Terminal matched constructor ID 305");
    305
}

fn match_node_instruction_243(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 243: Terminal matched constructor ID 334");
    334
}

fn match_node_instruction_244(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 244: Terminal matched constructor ID 314");
    314
}

fn match_node_instruction_245(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 245: Terminal matched constructor ID 345");
    345
}

fn match_node_instruction_246(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 246: Terminal matched constructor ID 311");
    311
}

fn match_node_instruction_247(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 247: Terminal matched constructor ID 344");
    344
}

fn match_node_instruction_248(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 248: Terminal matched constructor ID 331");
    331
}

fn match_node_instruction_249(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 249: Terminal matched constructor ID 348");
    348
}

fn match_node_instruction_250(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 250: Terminal matched constructor ID 310");
    310
}

fn match_node_instruction_251(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 251: Terminal matched constructor ID 343");
    343
}

fn match_node_instruction_252(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 252: Terminal matched constructor ID 330");
    330
}

fn match_node_instruction_253(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 253: Terminal matched constructor ID 347");
    347
}

fn match_node_instruction_254(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 254: Terminal matched constructor ID 307");
    307
}

fn match_node_instruction_255(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 255: Terminal matched constructor ID 340");
    340
}

fn match_node_instruction_256(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 256: Terminal matched constructor ID 315");
    315
}

fn match_node_instruction_257(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 257: Terminal matched constructor ID 346");
    346
}

fn match_node_instruction_258(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 3;
    eprintln!("Trace node 258: SlaInstructionBits start=24, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_259(bytes, ctx),
        1 => match_node_instruction_260(bytes, ctx),
        2 => match_node_instruction_261(bytes, ctx),
        3 => match_node_instruction_262(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_259(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 259: Terminal matched constructor ID 312");
    312
}

fn match_node_instruction_260(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 260: Terminal matched constructor ID 335");
    335
}

fn match_node_instruction_261(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 261: Terminal matched constructor ID 329");
    329
}

fn match_node_instruction_262(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 262: Terminal matched constructor ID 338");
    338
}

fn match_node_instruction_263(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 3;
    eprintln!("Trace node 263: SlaInstructionBits start=24, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_264(bytes, ctx),
        1 => match_node_instruction_265(bytes, ctx),
        2 => match_node_instruction_266(bytes, ctx),
        3 => match_node_instruction_267(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_264(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 264: Terminal matched constructor ID 328");
    328
}

fn match_node_instruction_265(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 265: Terminal matched constructor ID 337");
    337
}

fn match_node_instruction_266(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 266: Terminal matched constructor ID 313");
    313
}

fn match_node_instruction_267(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 267: Terminal matched constructor ID 336");
    336
}

fn match_node_instruction_268(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 15;
    eprintln!("Trace node 268: SlaInstructionBits start=24, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_269(bytes, ctx),
        1 => match_node_instruction_270(bytes, ctx),
        2 => match_node_instruction_271(bytes, ctx),
        3 => match_node_instruction_272(bytes, ctx),
        4 => match_node_instruction_273(bytes, ctx),
        5 => match_node_instruction_274(bytes, ctx),
        6 => match_node_instruction_275(bytes, ctx),
        7 => match_node_instruction_276(bytes, ctx),
        8 => match_node_instruction_277(bytes, ctx),
        9 => match_node_instruction_278(bytes, ctx),
        10 => match_node_instruction_279(bytes, ctx),
        11 => match_node_instruction_280(bytes, ctx),
        12 => match_node_instruction_281(bytes, ctx),
        13 => match_node_instruction_282(bytes, ctx),
        14 => match_node_instruction_283(bytes, ctx),
        15 => match_node_instruction_284(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_269(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 269: Terminal matched constructor ID 304");
    304
}

fn match_node_instruction_270(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 270: Terminal matched constructor ID 333");
    333
}

fn match_node_instruction_271(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 271: Terminal matched constructor ID 309");
    309
}

fn match_node_instruction_272(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 272: Terminal matched constructor ID 342");
    342
}

fn match_node_instruction_273(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 273: Terminal matched constructor ID 308");
    308
}

fn match_node_instruction_274(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 274: Terminal matched constructor ID 341");
    341
}

fn match_node_instruction_275(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 275: Terminal matched constructor ID 306");
    306
}

fn match_node_instruction_276(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 276: Terminal matched constructor ID 339");
    339
}

fn match_node_instruction_277(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 277: Terminal matched constructor ID 324");
    324
}

fn match_node_instruction_278(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 278: Terminal matched constructor ID 325");
    325
}

fn match_node_instruction_279(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 279: Terminal matched constructor ID 326");
    326
}

fn match_node_instruction_280(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 280: Terminal matched constructor ID 327");
    327
}

fn match_node_instruction_281(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 281: Terminal matched constructor ID 318");
    318
}

fn match_node_instruction_282(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 282: Terminal matched constructor ID 319");
    319
}

fn match_node_instruction_283(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 283: Terminal matched constructor ID 299");
    299
}

fn match_node_instruction_284(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 284: Terminal matched constructor ID 300");
    300
}

fn match_node_instruction_285(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 7;
    eprintln!("Trace node 285: SlaInstructionBits start=24, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_286(bytes, ctx),
        1 => match_node_instruction_289(bytes, ctx),
        2 => match_node_instruction_292(bytes, ctx),
        3 => match_node_instruction_295(bytes, ctx),
        4 => match_node_instruction_298(bytes, ctx),
        5 => match_node_instruction_299(bytes, ctx),
        6 => match_node_instruction_300(bytes, ctx),
        7 => match_node_instruction_303(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_286(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (27 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 27) & 1;
    eprintln!("Trace node 286: SlaInstructionBits start=27, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_287(bytes, ctx),
        1 => match_node_instruction_288(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_287(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 287: Terminal matched constructor ID 332");
    332
}

fn match_node_instruction_288(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 288: Terminal matched constructor ID 316");
    316
}

fn match_node_instruction_289(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (27 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 27) & 1;
    eprintln!("Trace node 289: SlaInstructionBits start=27, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_290(bytes, ctx),
        1 => match_node_instruction_291(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_290(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 290: Terminal matched constructor ID 320");
    320
}

fn match_node_instruction_291(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 291: Terminal matched constructor ID 322");
    322
}

fn match_node_instruction_292(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (27 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 27) & 1;
    eprintln!("Trace node 292: SlaInstructionBits start=27, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_293(bytes, ctx),
        1 => match_node_instruction_294(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_293(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 293: Terminal matched constructor ID 317");
    317
}

fn match_node_instruction_294(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 294: Terminal matched constructor ID 321");
    321
}

fn match_node_instruction_295(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (27 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 27) & 1;
    eprintln!("Trace node 295: SlaInstructionBits start=27, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_296(bytes, ctx),
        1 => match_node_instruction_297(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_296(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 296: Terminal matched constructor ID 323");
    323
}

fn match_node_instruction_297(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 297: Terminal matched constructor ID 303");
    303
}

fn match_node_instruction_298(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 298: Terminal matched constructor ID 349");
    349
}

fn match_node_instruction_299(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 299: Terminal matched constructor ID 350");
    350
}

fn match_node_instruction_300(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (27 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 27) & 1;
    eprintln!("Trace node 300: SlaInstructionBits start=27, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_301(bytes, ctx),
        1 => match_node_instruction_302(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_301(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 301: Terminal matched constructor ID 301");
    301
}

fn match_node_instruction_302(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 302: Terminal matched constructor ID 302");
    302
}

fn match_node_instruction_303(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 303: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_304(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 304: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_305(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 305: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_306(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 306: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_307(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 307: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_308(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 308: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_309(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 309: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_310(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 310: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_311(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 311: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_312(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 312: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_313(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 313: Terminal matched NOTHING");
    -1
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
    eprintln!("Trace node 328: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_329(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 329: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_330(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 330: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_331(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 15;
    eprintln!("Trace node 331: SlaInstructionBits start=3, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_332(bytes, ctx),
        1 => match_node_instruction_337(bytes, ctx),
        2 => match_node_instruction_342(bytes, ctx),
        3 => match_node_instruction_347(bytes, ctx),
        4 => match_node_instruction_352(bytes, ctx),
        5 => match_node_instruction_357(bytes, ctx),
        6 => match_node_instruction_362(bytes, ctx),
        7 => match_node_instruction_367(bytes, ctx),
        8 => match_node_instruction_372(bytes, ctx),
        9 => match_node_instruction_377(bytes, ctx),
        10 => match_node_instruction_382(bytes, ctx),
        11 => match_node_instruction_387(bytes, ctx),
        12 => match_node_instruction_392(bytes, ctx),
        13 => match_node_instruction_397(bytes, ctx),
        14 => match_node_instruction_402(bytes, ctx),
        15 => match_node_instruction_407(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_332(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (22 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 22) & 3;
    eprintln!("Trace node 332: SlaInstructionBits start=22, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_333(bytes, ctx),
        1 => match_node_instruction_334(bytes, ctx),
        2 => match_node_instruction_335(bytes, ctx),
        3 => match_node_instruction_336(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_333(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 333: Terminal matched constructor ID 77");
    77
}

fn match_node_instruction_334(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 334: Terminal matched constructor ID 77");
    77
}

fn match_node_instruction_335(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 335: Terminal matched constructor ID 77");
    77
}

fn match_node_instruction_336(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 336: Terminal matched constructor ID 77");
    77
}

fn match_node_instruction_337(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (22 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 22) & 3;
    eprintln!("Trace node 337: SlaInstructionBits start=22, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_338(bytes, ctx),
        1 => match_node_instruction_339(bytes, ctx),
        2 => match_node_instruction_340(bytes, ctx),
        3 => match_node_instruction_341(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_338(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 338: Terminal matched constructor ID 79");
    79
}

fn match_node_instruction_339(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 339: Terminal matched constructor ID 79");
    79
}

fn match_node_instruction_340(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 340: Terminal matched constructor ID 79");
    79
}

fn match_node_instruction_341(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 341: Terminal matched constructor ID 79");
    79
}

fn match_node_instruction_342(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (22 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 22) & 3;
    eprintln!("Trace node 342: SlaInstructionBits start=22, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_343(bytes, ctx),
        1 => match_node_instruction_344(bytes, ctx),
        2 => match_node_instruction_345(bytes, ctx),
        3 => match_node_instruction_346(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_343(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 343: Terminal matched constructor ID 73");
    73
}

fn match_node_instruction_344(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 344: Terminal matched constructor ID 73");
    73
}

fn match_node_instruction_345(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 345: Terminal matched constructor ID 73");
    73
}

fn match_node_instruction_346(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 346: Terminal matched constructor ID 73");
    73
}

fn match_node_instruction_347(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (22 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 22) & 3;
    eprintln!("Trace node 347: SlaInstructionBits start=22, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_348(bytes, ctx),
        1 => match_node_instruction_349(bytes, ctx),
        2 => match_node_instruction_350(bytes, ctx),
        3 => match_node_instruction_351(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_348(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 348: Terminal matched constructor ID 75");
    75
}

fn match_node_instruction_349(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 349: Terminal matched constructor ID 75");
    75
}

fn match_node_instruction_350(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 350: Terminal matched constructor ID 75");
    75
}

fn match_node_instruction_351(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 351: Terminal matched constructor ID 75");
    75
}

fn match_node_instruction_352(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (22 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 22) & 3;
    eprintln!("Trace node 352: SlaInstructionBits start=22, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_353(bytes, ctx),
        1 => match_node_instruction_354(bytes, ctx),
        2 => match_node_instruction_355(bytes, ctx),
        3 => match_node_instruction_356(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_353(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 353: Terminal matched constructor ID 106");
    106
}

fn match_node_instruction_354(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 354: Terminal matched constructor ID 106");
    106
}

fn match_node_instruction_355(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 355: Terminal matched constructor ID 106");
    106
}

fn match_node_instruction_356(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 356: Terminal matched constructor ID 106");
    106
}

fn match_node_instruction_357(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (22 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 22) & 3;
    eprintln!("Trace node 357: SlaInstructionBits start=22, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_358(bytes, ctx),
        1 => match_node_instruction_359(bytes, ctx),
        2 => match_node_instruction_360(bytes, ctx),
        3 => match_node_instruction_361(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_358(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 358: Terminal matched constructor ID 104");
    104
}

fn match_node_instruction_359(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 359: Terminal matched constructor ID 104");
    104
}

fn match_node_instruction_360(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 360: Terminal matched constructor ID 104");
    104
}

fn match_node_instruction_361(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 361: Terminal matched constructor ID 104");
    104
}

fn match_node_instruction_362(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (22 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 22) & 3;
    eprintln!("Trace node 362: SlaInstructionBits start=22, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_363(bytes, ctx),
        1 => match_node_instruction_364(bytes, ctx),
        2 => match_node_instruction_365(bytes, ctx),
        3 => match_node_instruction_366(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_363(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 363: Terminal matched constructor ID 94");
    94
}

fn match_node_instruction_364(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 364: Terminal matched constructor ID 94");
    94
}

fn match_node_instruction_365(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 365: Terminal matched constructor ID 94");
    94
}

fn match_node_instruction_366(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 366: Terminal matched constructor ID 94");
    94
}

fn match_node_instruction_367(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (22 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 22) & 3;
    eprintln!("Trace node 367: SlaInstructionBits start=22, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_368(bytes, ctx),
        1 => match_node_instruction_369(bytes, ctx),
        2 => match_node_instruction_370(bytes, ctx),
        3 => match_node_instruction_371(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_368(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 368: Terminal matched constructor ID 92");
    92
}

fn match_node_instruction_369(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 369: Terminal matched constructor ID 92");
    92
}

fn match_node_instruction_370(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 370: Terminal matched constructor ID 92");
    92
}

fn match_node_instruction_371(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 371: Terminal matched constructor ID 92");
    92
}

fn match_node_instruction_372(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (22 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 22) & 3;
    eprintln!("Trace node 372: SlaInstructionBits start=22, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_373(bytes, ctx),
        1 => match_node_instruction_374(bytes, ctx),
        2 => match_node_instruction_375(bytes, ctx),
        3 => match_node_instruction_376(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_373(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 373: Terminal matched constructor ID 129");
    129
}

fn match_node_instruction_374(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 374: Terminal matched constructor ID 132");
    132
}

fn match_node_instruction_375(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 375: Terminal matched constructor ID 135");
    135
}

fn match_node_instruction_376(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 376: Terminal matched constructor ID 288");
    288
}

fn match_node_instruction_377(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (22 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 22) & 3;
    eprintln!("Trace node 377: SlaInstructionBits start=22, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_378(bytes, ctx),
        1 => match_node_instruction_379(bytes, ctx),
        2 => match_node_instruction_380(bytes, ctx),
        3 => match_node_instruction_381(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_378(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 378: Terminal matched constructor ID 129");
    129
}

fn match_node_instruction_379(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 379: Terminal matched constructor ID 132");
    132
}

fn match_node_instruction_380(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 380: Terminal matched constructor ID 135");
    135
}

fn match_node_instruction_381(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 381: Terminal matched constructor ID 247");
    247
}

fn match_node_instruction_382(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (22 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 22) & 3;
    eprintln!("Trace node 382: SlaInstructionBits start=22, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_383(bytes, ctx),
        1 => match_node_instruction_384(bytes, ctx),
        2 => match_node_instruction_385(bytes, ctx),
        3 => match_node_instruction_386(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_383(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 383: Terminal matched constructor ID 129");
    129
}

fn match_node_instruction_384(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 384: Terminal matched constructor ID 132");
    132
}

fn match_node_instruction_385(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 385: Terminal matched constructor ID 135");
    135
}

fn match_node_instruction_386(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 386: Terminal matched constructor ID 235");
    235
}

fn match_node_instruction_387(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (22 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 22) & 3;
    eprintln!("Trace node 387: SlaInstructionBits start=22, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_388(bytes, ctx),
        1 => match_node_instruction_389(bytes, ctx),
        2 => match_node_instruction_390(bytes, ctx),
        3 => match_node_instruction_391(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_388(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 388: Terminal matched constructor ID 120");
    120
}

fn match_node_instruction_389(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 389: Terminal matched constructor ID 120");
    120
}

fn match_node_instruction_390(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 390: Terminal matched constructor ID 120");
    120
}

fn match_node_instruction_391(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 391: Terminal matched constructor ID 120");
    120
}

fn match_node_instruction_392(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (22 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 22) & 3;
    eprintln!("Trace node 392: SlaInstructionBits start=22, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_393(bytes, ctx),
        1 => match_node_instruction_394(bytes, ctx),
        2 => match_node_instruction_395(bytes, ctx),
        3 => match_node_instruction_396(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_393(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 393: Terminal matched constructor ID 121");
    121
}

fn match_node_instruction_394(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 394: Terminal matched constructor ID 121");
    121
}

fn match_node_instruction_395(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 395: Terminal matched constructor ID 121");
    121
}

fn match_node_instruction_396(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 396: Terminal matched constructor ID 121");
    121
}

fn match_node_instruction_397(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (22 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 22) & 3;
    eprintln!("Trace node 397: SlaInstructionBits start=22, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_398(bytes, ctx),
        1 => match_node_instruction_399(bytes, ctx),
        2 => match_node_instruction_400(bytes, ctx),
        3 => match_node_instruction_401(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_398(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 398: Terminal matched constructor ID 122");
    122
}

fn match_node_instruction_399(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 399: Terminal matched constructor ID 122");
    122
}

fn match_node_instruction_400(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 400: Terminal matched constructor ID 122");
    122
}

fn match_node_instruction_401(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 401: Terminal matched constructor ID 122");
    122
}

fn match_node_instruction_402(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (22 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 22) & 3;
    eprintln!("Trace node 402: SlaInstructionBits start=22, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_403(bytes, ctx),
        1 => match_node_instruction_404(bytes, ctx),
        2 => match_node_instruction_405(bytes, ctx),
        3 => match_node_instruction_406(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_403(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 403: Terminal matched constructor ID 129");
    129
}

fn match_node_instruction_404(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 404: Terminal matched constructor ID 132");
    132
}

fn match_node_instruction_405(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 405: Terminal matched constructor ID 135");
    135
}

fn match_node_instruction_406(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 406: Terminal matched constructor ID 143");
    143
}

fn match_node_instruction_407(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (22 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 22) & 3;
    eprintln!("Trace node 407: SlaInstructionBits start=22, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_408(bytes, ctx),
        1 => match_node_instruction_409(bytes, ctx),
        2 => match_node_instruction_410(bytes, ctx),
        3 => match_node_instruction_411(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_408(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 408: Terminal matched constructor ID 129");
    129
}

fn match_node_instruction_409(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 409: Terminal matched constructor ID 132");
    132
}

fn match_node_instruction_410(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 410: Terminal matched constructor ID 135");
    135
}

fn match_node_instruction_411(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 411: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_412(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 412: Terminal matched constructor ID 46");
    46
}

fn match_node_instruction_413(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 413: Terminal matched constructor ID 17");
    17
}

fn match_node_instruction_414(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 414: Terminal matched constructor ID 139");
    139
}

fn match_node_instruction_415(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 415: Terminal matched constructor ID 284");
    284
}

fn match_node_instruction_416(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 416: Terminal matched constructor ID 290");
    290
}

fn match_node_instruction_417(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 417: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_418(bytes, ctx),
        1 => match_node_instruction_419(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_418(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 418: Terminal matched constructor ID 48");
    48
}

fn match_node_instruction_419(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 419: Terminal matched constructor ID 372");
    372
}

fn match_node_instruction_420(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 420: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_421(bytes, ctx),
        1 => match_node_instruction_424(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_421(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 421: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_422(bytes, ctx),
        1 => match_node_instruction_423(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_422(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 422: Terminal matched constructor ID 147");
    147
}

fn match_node_instruction_423(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 423: Terminal matched constructor ID 203");
    203
}

fn match_node_instruction_424(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 424: Terminal matched constructor ID 177");
    177
}

fn match_node_instruction_425(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 425: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_426(bytes, ctx),
        1 => match_node_instruction_427(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_426(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 426: Terminal matched constructor ID 167");
    167
}

fn match_node_instruction_427(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 427: Terminal matched constructor ID 161");
    161
}

fn match_node_instruction_428(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 428: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_429(bytes, ctx),
        1 => match_node_instruction_430(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_429(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 429: Terminal matched constructor ID 155");
    155
}

fn match_node_instruction_430(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 430: Terminal matched constructor ID 152");
    152
}

fn match_node_instruction_431(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 431: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_432(bytes, ctx),
        1 => match_node_instruction_433(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_432(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 432: Terminal matched constructor ID 216");
    216
}

fn match_node_instruction_433(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 433: Terminal matched constructor ID 208");
    208
}

fn match_node_instruction_434(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 434: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_435(bytes, ctx),
        1 => match_node_instruction_436(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_435(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 435: Terminal matched constructor ID 197");
    197
}

fn match_node_instruction_436(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 436: Terminal matched constructor ID 219");
    219
}

fn match_node_instruction_437(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 437: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_438(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 3;
    eprintln!("Trace node 438: SlaInstructionBits start=3, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_439(bytes, ctx),
        1 => match_node_instruction_446(bytes, ctx),
        2 => match_node_instruction_603(bytes, ctx),
        3 => match_node_instruction_678(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_439(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 439: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_440(bytes, ctx),
        1 => match_node_instruction_441(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_440(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 440: Terminal matched constructor ID 254");
    254
}

fn match_node_instruction_441(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 3;
    eprintln!("Trace node 441: SlaInstructionBits start=5, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_442(bytes, ctx),
        1 => match_node_instruction_443(bytes, ctx),
        2 => match_node_instruction_444(bytes, ctx),
        3 => match_node_instruction_445(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_442(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 442: Terminal matched constructor ID 239");
    239
}

fn match_node_instruction_443(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 443: Terminal matched constructor ID 240");
    240
}

fn match_node_instruction_444(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 444: Terminal matched constructor ID 241");
    241
}

fn match_node_instruction_445(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 445: Terminal matched constructor ID 242");
    242
}

fn match_node_instruction_446(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 7 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 127;
    eprintln!("Trace node 446: SlaInstructionBits start=5, size=7, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_447(bytes, ctx),
        1 => match_node_instruction_448(bytes, ctx),
        2 => match_node_instruction_449(bytes, ctx),
        3 => match_node_instruction_450(bytes, ctx),
        4 => match_node_instruction_451(bytes, ctx),
        5 => match_node_instruction_452(bytes, ctx),
        6 => match_node_instruction_453(bytes, ctx),
        7 => match_node_instruction_454(bytes, ctx),
        8 => match_node_instruction_455(bytes, ctx),
        9 => match_node_instruction_456(bytes, ctx),
        10 => match_node_instruction_457(bytes, ctx),
        11 => match_node_instruction_458(bytes, ctx),
        12 => match_node_instruction_459(bytes, ctx),
        13 => match_node_instruction_460(bytes, ctx),
        14 => match_node_instruction_461(bytes, ctx),
        15 => match_node_instruction_462(bytes, ctx),
        16 => match_node_instruction_463(bytes, ctx),
        17 => match_node_instruction_464(bytes, ctx),
        18 => match_node_instruction_465(bytes, ctx),
        19 => match_node_instruction_466(bytes, ctx),
        20 => match_node_instruction_467(bytes, ctx),
        21 => match_node_instruction_468(bytes, ctx),
        22 => match_node_instruction_469(bytes, ctx),
        23 => match_node_instruction_470(bytes, ctx),
        24 => match_node_instruction_471(bytes, ctx),
        25 => match_node_instruction_472(bytes, ctx),
        26 => match_node_instruction_473(bytes, ctx),
        27 => match_node_instruction_476(bytes, ctx),
        28 => match_node_instruction_477(bytes, ctx),
        29 => match_node_instruction_478(bytes, ctx),
        30 => match_node_instruction_479(bytes, ctx),
        31 => match_node_instruction_480(bytes, ctx),
        32 => match_node_instruction_481(bytes, ctx),
        33 => match_node_instruction_482(bytes, ctx),
        34 => match_node_instruction_483(bytes, ctx),
        35 => match_node_instruction_484(bytes, ctx),
        36 => match_node_instruction_485(bytes, ctx),
        37 => match_node_instruction_486(bytes, ctx),
        38 => match_node_instruction_487(bytes, ctx),
        39 => match_node_instruction_488(bytes, ctx),
        40 => match_node_instruction_489(bytes, ctx),
        41 => match_node_instruction_490(bytes, ctx),
        42 => match_node_instruction_491(bytes, ctx),
        43 => match_node_instruction_492(bytes, ctx),
        44 => match_node_instruction_493(bytes, ctx),
        45 => match_node_instruction_494(bytes, ctx),
        46 => match_node_instruction_495(bytes, ctx),
        47 => match_node_instruction_496(bytes, ctx),
        48 => match_node_instruction_497(bytes, ctx),
        49 => match_node_instruction_498(bytes, ctx),
        50 => match_node_instruction_499(bytes, ctx),
        51 => match_node_instruction_500(bytes, ctx),
        52 => match_node_instruction_501(bytes, ctx),
        53 => match_node_instruction_502(bytes, ctx),
        54 => match_node_instruction_503(bytes, ctx),
        55 => match_node_instruction_504(bytes, ctx),
        56 => match_node_instruction_505(bytes, ctx),
        57 => match_node_instruction_506(bytes, ctx),
        58 => match_node_instruction_507(bytes, ctx),
        59 => match_node_instruction_510(bytes, ctx),
        60 => match_node_instruction_511(bytes, ctx),
        61 => match_node_instruction_512(bytes, ctx),
        62 => match_node_instruction_513(bytes, ctx),
        63 => match_node_instruction_514(bytes, ctx),
        64 => match_node_instruction_515(bytes, ctx),
        65 => match_node_instruction_516(bytes, ctx),
        66 => match_node_instruction_517(bytes, ctx),
        67 => match_node_instruction_518(bytes, ctx),
        68 => match_node_instruction_519(bytes, ctx),
        69 => match_node_instruction_520(bytes, ctx),
        70 => match_node_instruction_521(bytes, ctx),
        71 => match_node_instruction_522(bytes, ctx),
        72 => match_node_instruction_523(bytes, ctx),
        73 => match_node_instruction_524(bytes, ctx),
        74 => match_node_instruction_525(bytes, ctx),
        75 => match_node_instruction_526(bytes, ctx),
        76 => match_node_instruction_527(bytes, ctx),
        77 => match_node_instruction_528(bytes, ctx),
        78 => match_node_instruction_529(bytes, ctx),
        79 => match_node_instruction_530(bytes, ctx),
        80 => match_node_instruction_531(bytes, ctx),
        81 => match_node_instruction_532(bytes, ctx),
        82 => match_node_instruction_533(bytes, ctx),
        83 => match_node_instruction_534(bytes, ctx),
        84 => match_node_instruction_535(bytes, ctx),
        85 => match_node_instruction_536(bytes, ctx),
        86 => match_node_instruction_537(bytes, ctx),
        87 => match_node_instruction_538(bytes, ctx),
        88 => match_node_instruction_539(bytes, ctx),
        89 => match_node_instruction_540(bytes, ctx),
        90 => match_node_instruction_541(bytes, ctx),
        91 => match_node_instruction_550(bytes, ctx),
        92 => match_node_instruction_551(bytes, ctx),
        93 => match_node_instruction_552(bytes, ctx),
        94 => match_node_instruction_553(bytes, ctx),
        95 => match_node_instruction_554(bytes, ctx),
        96 => match_node_instruction_555(bytes, ctx),
        97 => match_node_instruction_556(bytes, ctx),
        98 => match_node_instruction_557(bytes, ctx),
        99 => match_node_instruction_558(bytes, ctx),
        100 => match_node_instruction_559(bytes, ctx),
        101 => match_node_instruction_560(bytes, ctx),
        102 => match_node_instruction_561(bytes, ctx),
        103 => match_node_instruction_562(bytes, ctx),
        104 => match_node_instruction_563(bytes, ctx),
        105 => match_node_instruction_564(bytes, ctx),
        106 => match_node_instruction_565(bytes, ctx),
        107 => match_node_instruction_566(bytes, ctx),
        108 => match_node_instruction_567(bytes, ctx),
        109 => match_node_instruction_568(bytes, ctx),
        110 => match_node_instruction_569(bytes, ctx),
        111 => match_node_instruction_570(bytes, ctx),
        112 => match_node_instruction_571(bytes, ctx),
        113 => match_node_instruction_572(bytes, ctx),
        114 => match_node_instruction_573(bytes, ctx),
        115 => match_node_instruction_574(bytes, ctx),
        116 => match_node_instruction_575(bytes, ctx),
        117 => match_node_instruction_576(bytes, ctx),
        118 => match_node_instruction_577(bytes, ctx),
        119 => match_node_instruction_578(bytes, ctx),
        120 => match_node_instruction_579(bytes, ctx),
        121 => match_node_instruction_580(bytes, ctx),
        122 => match_node_instruction_581(bytes, ctx),
        123 => match_node_instruction_598(bytes, ctx),
        124 => match_node_instruction_599(bytes, ctx),
        125 => match_node_instruction_600(bytes, ctx),
        126 => match_node_instruction_601(bytes, ctx),
        127 => match_node_instruction_602(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_447(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 447: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_448(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 448: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_449(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 449: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_450(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 450: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_451(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 451: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_452(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 452: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_453(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 453: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_454(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 454: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_455(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 455: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_456(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 456: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_457(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 457: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_458(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 458: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_459(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 459: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_460(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 460: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_461(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 461: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_462(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 462: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_463(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 463: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_464(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 464: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_465(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 465: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_466(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 466: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_467(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 467: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_468(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 468: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_469(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 469: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_470(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 470: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_471(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 471: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_472(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 472: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_473(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (19 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 19) & 1;
    eprintln!("Trace node 473: SlaInstructionBits start=19, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_474(bytes, ctx),
        1 => match_node_instruction_475(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_474(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 474: Terminal matched constructor ID 258");
    258
}

fn match_node_instruction_475(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 475: Terminal matched constructor ID 255");
    255
}

fn match_node_instruction_476(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 476: Terminal matched constructor ID 248");
    248
}

fn match_node_instruction_477(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 477: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_478(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 478: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_479(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 479: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_480(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 480: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_481(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 481: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_482(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 482: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_483(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 483: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_484(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 484: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_485(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 485: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_486(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 486: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_487(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 487: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_488(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 488: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_489(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 489: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_490(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 490: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_491(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 491: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_492(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 492: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_493(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 493: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_494(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 494: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_495(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 495: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_496(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 496: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_497(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 497: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_498(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 498: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_499(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 499: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_500(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 500: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_501(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 501: Terminal matched NOTHING");
    -1
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
    eprintln!("Trace node 504: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_505(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 505: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_506(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 506: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_507(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (19 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 19) & 1;
    eprintln!("Trace node 507: SlaInstructionBits start=19, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_508(bytes, ctx),
        1 => match_node_instruction_509(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_508(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 508: Terminal matched constructor ID 273");
    273
}

fn match_node_instruction_509(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 509: Terminal matched constructor ID 270");
    270
}

fn match_node_instruction_510(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 510: Terminal matched constructor ID 250");
    250
}

fn match_node_instruction_511(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 511: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_512(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 512: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_513(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 513: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_514(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 514: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_515(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 515: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_516(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 516: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_517(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 517: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_518(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 518: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_519(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 519: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_520(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 520: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_521(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 521: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_522(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 522: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_523(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 523: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_524(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 524: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_525(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 525: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_526(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 526: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_527(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 527: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_528(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 528: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_529(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 529: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_530(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 530: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_531(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 531: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_532(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 532: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_533(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 533: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_534(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 534: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_535(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 535: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_536(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 536: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_537(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 537: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_538(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 538: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_539(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 539: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_540(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 540: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_541(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (21 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 21) & 7;
    eprintln!("Trace node 541: SlaInstructionBits start=21, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_542(bytes, ctx),
        1 => match_node_instruction_543(bytes, ctx),
        2 => match_node_instruction_544(bytes, ctx),
        3 => match_node_instruction_545(bytes, ctx),
        4 => match_node_instruction_546(bytes, ctx),
        5 => match_node_instruction_547(bytes, ctx),
        6 => match_node_instruction_548(bytes, ctx),
        7 => match_node_instruction_549(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_542(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 542: Terminal matched constructor ID 265");
    265
}

fn match_node_instruction_543(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 543: Terminal matched constructor ID 264");
    264
}

fn match_node_instruction_544(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 544: Terminal matched constructor ID 280");
    280
}

fn match_node_instruction_545(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 545: Terminal matched constructor ID 279");
    279
}

fn match_node_instruction_546(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 546: Terminal matched constructor ID 263");
    263
}

fn match_node_instruction_547(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 547: Terminal matched constructor ID 278");
    278
}

fn match_node_instruction_548(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 548: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_549(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 549: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_550(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 550: Terminal matched constructor ID 111");
    111
}

fn match_node_instruction_551(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 551: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_552(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 552: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_553(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 553: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_554(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 554: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_555(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 555: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_556(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 556: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_557(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 557: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_558(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 558: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_559(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 559: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_560(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 560: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_561(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 561: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_562(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 562: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_563(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 563: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_564(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 564: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_565(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 565: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_566(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 566: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_567(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 567: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_568(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 568: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_569(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 569: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_570(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 570: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_571(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 571: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_572(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 572: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_573(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 573: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_574(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 574: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_575(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 575: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_576(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 576: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_577(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 577: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_578(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 578: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_579(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 579: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_580(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 580: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_581(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 3;
    eprintln!("Trace node 581: SlaInstructionBits start=24, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_582(bytes, ctx),
        1 => match_node_instruction_589(bytes, ctx),
        2 => match_node_instruction_596(bytes, ctx),
        3 => match_node_instruction_597(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_582(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (19 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 19) & 1;
    eprintln!("Trace node 582: SlaInstructionBits start=19, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_583(bytes, ctx),
        1 => match_node_instruction_588(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_583(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (26 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 26) & 3;
    eprintln!("Trace node 583: SlaInstructionBits start=26, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_584(bytes, ctx),
        1 => match_node_instruction_585(bytes, ctx),
        2 => match_node_instruction_586(bytes, ctx),
        3 => match_node_instruction_587(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_584(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 584: Terminal matched constructor ID 267");
    267
}

fn match_node_instruction_585(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 585: Terminal matched constructor ID 266");
    266
}

fn match_node_instruction_586(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 586: Terminal matched constructor ID 269");
    269
}

fn match_node_instruction_587(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 587: Terminal matched constructor ID 268");
    268
}

fn match_node_instruction_588(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 588: Terminal matched constructor ID 260");
    260
}

fn match_node_instruction_589(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (19 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 19) & 1;
    eprintln!("Trace node 589: SlaInstructionBits start=19, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_590(bytes, ctx),
        1 => match_node_instruction_593(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_590(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (26 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 26) & 1;
    eprintln!("Trace node 590: SlaInstructionBits start=26, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_591(bytes, ctx),
        1 => match_node_instruction_592(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_591(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 591: Terminal matched constructor ID 256");
    256
}

fn match_node_instruction_592(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 592: Terminal matched constructor ID 271");
    271
}

fn match_node_instruction_593(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (26 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 26) & 1;
    eprintln!("Trace node 593: SlaInstructionBits start=26, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_594(bytes, ctx),
        1 => match_node_instruction_595(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_594(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 594: Terminal matched constructor ID 257");
    257
}

fn match_node_instruction_595(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 595: Terminal matched constructor ID 257");
    257
}

fn match_node_instruction_596(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 596: Terminal matched constructor ID 275");
    275
}

fn match_node_instruction_597(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 597: Terminal matched constructor ID 272");
    272
}

fn match_node_instruction_598(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 598: Terminal matched constructor ID 113");
    113
}

fn match_node_instruction_599(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 599: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_600(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 600: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_601(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 601: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_602(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 602: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_603(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 1;
    eprintln!("Trace node 603: SlaInstructionBits start=5, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_604(bytes, ctx),
        1 => match_node_instruction_673(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_604(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 6 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 63;
    eprintln!("Trace node 604: SlaInstructionBits start=6, size=6, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_605(bytes, ctx),
        1 => match_node_instruction_606(bytes, ctx),
        2 => match_node_instruction_607(bytes, ctx),
        3 => match_node_instruction_608(bytes, ctx),
        4 => match_node_instruction_609(bytes, ctx),
        5 => match_node_instruction_610(bytes, ctx),
        6 => match_node_instruction_611(bytes, ctx),
        7 => match_node_instruction_612(bytes, ctx),
        8 => match_node_instruction_613(bytes, ctx),
        9 => match_node_instruction_614(bytes, ctx),
        10 => match_node_instruction_615(bytes, ctx),
        11 => match_node_instruction_616(bytes, ctx),
        12 => match_node_instruction_617(bytes, ctx),
        13 => match_node_instruction_618(bytes, ctx),
        14 => match_node_instruction_619(bytes, ctx),
        15 => match_node_instruction_620(bytes, ctx),
        16 => match_node_instruction_621(bytes, ctx),
        17 => match_node_instruction_622(bytes, ctx),
        18 => match_node_instruction_623(bytes, ctx),
        19 => match_node_instruction_624(bytes, ctx),
        20 => match_node_instruction_625(bytes, ctx),
        21 => match_node_instruction_626(bytes, ctx),
        22 => match_node_instruction_627(bytes, ctx),
        23 => match_node_instruction_628(bytes, ctx),
        24 => match_node_instruction_629(bytes, ctx),
        25 => match_node_instruction_630(bytes, ctx),
        26 => match_node_instruction_631(bytes, ctx),
        27 => match_node_instruction_632(bytes, ctx),
        28 => match_node_instruction_635(bytes, ctx),
        29 => match_node_instruction_636(bytes, ctx),
        30 => match_node_instruction_637(bytes, ctx),
        31 => match_node_instruction_638(bytes, ctx),
        32 => match_node_instruction_639(bytes, ctx),
        33 => match_node_instruction_640(bytes, ctx),
        34 => match_node_instruction_641(bytes, ctx),
        35 => match_node_instruction_642(bytes, ctx),
        36 => match_node_instruction_643(bytes, ctx),
        37 => match_node_instruction_644(bytes, ctx),
        38 => match_node_instruction_645(bytes, ctx),
        39 => match_node_instruction_646(bytes, ctx),
        40 => match_node_instruction_647(bytes, ctx),
        41 => match_node_instruction_648(bytes, ctx),
        42 => match_node_instruction_649(bytes, ctx),
        43 => match_node_instruction_650(bytes, ctx),
        44 => match_node_instruction_651(bytes, ctx),
        45 => match_node_instruction_652(bytes, ctx),
        46 => match_node_instruction_653(bytes, ctx),
        47 => match_node_instruction_654(bytes, ctx),
        48 => match_node_instruction_655(bytes, ctx),
        49 => match_node_instruction_656(bytes, ctx),
        50 => match_node_instruction_657(bytes, ctx),
        51 => match_node_instruction_658(bytes, ctx),
        52 => match_node_instruction_659(bytes, ctx),
        53 => match_node_instruction_660(bytes, ctx),
        54 => match_node_instruction_661(bytes, ctx),
        55 => match_node_instruction_662(bytes, ctx),
        56 => match_node_instruction_663(bytes, ctx),
        57 => match_node_instruction_664(bytes, ctx),
        58 => match_node_instruction_665(bytes, ctx),
        59 => match_node_instruction_666(bytes, ctx),
        60 => match_node_instruction_669(bytes, ctx),
        61 => match_node_instruction_670(bytes, ctx),
        62 => match_node_instruction_671(bytes, ctx),
        63 => match_node_instruction_672(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_605(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 605: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_606(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 606: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_607(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 607: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_608(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 608: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_609(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 609: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_610(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 610: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_611(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 611: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_612(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 612: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_613(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 613: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_614(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 614: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_615(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 615: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_616(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 616: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_617(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 617: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_618(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 618: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_619(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 619: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_620(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 620: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_621(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 621: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_622(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 622: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_623(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 623: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_624(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 624: Terminal matched NOTHING");
    -1
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
    eprintln!("Trace node 629: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_630(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 630: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_631(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 631: Terminal matched constructor ID 262");
    262
}

fn match_node_instruction_632(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (21 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 21) & 1;
    eprintln!("Trace node 632: SlaInstructionBits start=21, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_633(bytes, ctx),
        1 => match_node_instruction_634(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_633(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 633: Terminal matched constructor ID 369");
    369
}

fn match_node_instruction_634(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 634: Terminal matched constructor ID 373");
    373
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
    eprintln!("Trace node 637: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_638(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 638: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_639(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 639: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_640(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 640: Terminal matched NOTHING");
    -1
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
    eprintln!("Trace node 645: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_646(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 646: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_647(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 647: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_648(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 648: Terminal matched NOTHING");
    -1
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
    eprintln!("Trace node 657: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_658(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 658: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_659(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 659: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_660(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 660: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_661(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 661: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_662(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 662: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_663(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 663: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_664(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 664: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_665(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 665: Terminal matched constructor ID 261");
    261
}

fn match_node_instruction_666(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (21 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 21) & 1;
    eprintln!("Trace node 666: SlaInstructionBits start=21, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_667(bytes, ctx),
        1 => match_node_instruction_668(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_667(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 667: Terminal matched constructor ID 367");
    367
}

fn match_node_instruction_668(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 668: Terminal matched constructor ID 368");
    368
}

fn match_node_instruction_669(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 669: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_670(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 670: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_671(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 671: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_672(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 672: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_673(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 673: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_674(bytes, ctx),
        1 => match_node_instruction_677(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_674(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 1;
    eprintln!("Trace node 674: SlaInstructionBits start=6, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_675(bytes, ctx),
        1 => match_node_instruction_676(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_675(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 675: Terminal matched constructor ID 277");
    277
}

fn match_node_instruction_676(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 676: Terminal matched constructor ID 276");
    276
}

fn match_node_instruction_677(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 677: Terminal matched constructor ID 50");
    50
}

fn match_node_instruction_678(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 1;
    eprintln!("Trace node 678: SlaInstructionBits start=6, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_679(bytes, ctx),
        1 => match_node_instruction_680(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_679(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 679: Terminal matched constructor ID 142");
    142
}

fn match_node_instruction_680(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 680: Terminal matched constructor ID 33");
    33
}

fn match_node_instruction_681(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 681: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_682(bytes, ctx),
        1 => match_node_instruction_691(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_682(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 3;
    eprintln!("Trace node 682: SlaInstructionBits start=4, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_683(bytes, ctx),
        1 => match_node_instruction_684(bytes, ctx),
        2 => match_node_instruction_685(bytes, ctx),
        3 => match_node_instruction_688(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_683(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 683: Terminal matched constructor ID 221");
    221
}

fn match_node_instruction_684(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 684: Terminal matched constructor ID 224");
    224
}

fn match_node_instruction_685(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 1;
    eprintln!("Trace node 685: SlaInstructionBits start=6, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_686(bytes, ctx),
        1 => match_node_instruction_687(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_686(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 686: Terminal matched constructor ID 229");
    229
}

fn match_node_instruction_687(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 687: Terminal matched constructor ID 228");
    228
}

fn match_node_instruction_688(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 1;
    eprintln!("Trace node 688: SlaInstructionBits start=6, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_689(bytes, ctx),
        1 => match_node_instruction_690(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_689(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 689: Terminal matched constructor ID 231");
    231
}

fn match_node_instruction_690(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 690: Terminal matched constructor ID 230");
    230
}

fn match_node_instruction_691(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (16 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 16) & 3;
    eprintln!("Trace node 691: SlaInstructionBits start=16, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_692(bytes, ctx),
        1 => match_node_instruction_697(bytes, ctx),
        2 => match_node_instruction_698(bytes, ctx),
        3 => match_node_instruction_703(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_692(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (18 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 18) & 1;
    eprintln!("Trace node 692: SlaInstructionBits start=18, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_693(bytes, ctx),
        1 => match_node_instruction_694(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_693(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 693: Terminal matched constructor ID 188");
    188
}

fn match_node_instruction_694(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (19 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 19) & 1;
    eprintln!("Trace node 694: SlaInstructionBits start=19, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_695(bytes, ctx),
        1 => match_node_instruction_696(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_695(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 695: Terminal matched constructor ID 189");
    189
}

fn match_node_instruction_696(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 696: Terminal matched constructor ID 190");
    190
}

fn match_node_instruction_697(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 697: Terminal matched constructor ID 187");
    187
}

fn match_node_instruction_698(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (18 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 18) & 3;
    eprintln!("Trace node 698: SlaInstructionBits start=18, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_699(bytes, ctx),
        1 => match_node_instruction_700(bytes, ctx),
        2 => match_node_instruction_701(bytes, ctx),
        3 => match_node_instruction_702(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_699(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 699: Terminal matched constructor ID 191");
    191
}

fn match_node_instruction_700(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 700: Terminal matched constructor ID 192");
    192
}

fn match_node_instruction_701(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 701: Terminal matched constructor ID 193");
    193
}

fn match_node_instruction_702(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 702: Terminal matched constructor ID 108");
    108
}

fn match_node_instruction_703(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (18 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 18) & 3;
    eprintln!("Trace node 703: SlaInstructionBits start=18, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_704(bytes, ctx),
        1 => match_node_instruction_705(bytes, ctx),
        2 => match_node_instruction_706(bytes, ctx),
        3 => match_node_instruction_715(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_704(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 704: Terminal matched constructor ID 109");
    109
}

fn match_node_instruction_705(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 705: Terminal matched constructor ID 110");
    110
}

fn match_node_instruction_706(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 7;
    eprintln!("Trace node 706: SlaInstructionBits start=25, size=3, word={:08x}, probe={}", word, probe);
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
    eprintln!("Trace node 707: Terminal matched constructor ID 10");
    10
}

fn match_node_instruction_708(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 708: Terminal matched constructor ID 52");
    52
}

fn match_node_instruction_709(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 709: Terminal matched constructor ID 71");
    71
}

fn match_node_instruction_710(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 710: Terminal matched constructor ID 102");
    102
}

fn match_node_instruction_711(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 711: Terminal matched constructor ID 90");
    90
}

fn match_node_instruction_712(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 712: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_713(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 713: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_714(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 714: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_715(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 715: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_716(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 716: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_717(bytes, ctx),
        1 => match_node_instruction_754(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_717(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (16 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 16) & 3;
    eprintln!("Trace node 717: SlaInstructionBits start=16, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_718(bytes, ctx),
        1 => match_node_instruction_751(bytes, ctx),
        2 => match_node_instruction_752(bytes, ctx),
        3 => match_node_instruction_753(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_718(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (18 + 5 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 18) & 31;
    eprintln!("Trace node 718: SlaInstructionBits start=18, size=5, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_719(bytes, ctx),
        1 => match_node_instruction_720(bytes, ctx),
        2 => match_node_instruction_721(bytes, ctx),
        3 => match_node_instruction_722(bytes, ctx),
        4 => match_node_instruction_723(bytes, ctx),
        5 => match_node_instruction_724(bytes, ctx),
        6 => match_node_instruction_725(bytes, ctx),
        7 => match_node_instruction_726(bytes, ctx),
        8 => match_node_instruction_727(bytes, ctx),
        9 => match_node_instruction_728(bytes, ctx),
        10 => match_node_instruction_729(bytes, ctx),
        11 => match_node_instruction_730(bytes, ctx),
        12 => match_node_instruction_731(bytes, ctx),
        13 => match_node_instruction_732(bytes, ctx),
        14 => match_node_instruction_733(bytes, ctx),
        15 => match_node_instruction_734(bytes, ctx),
        16 => match_node_instruction_735(bytes, ctx),
        17 => match_node_instruction_736(bytes, ctx),
        18 => match_node_instruction_737(bytes, ctx),
        19 => match_node_instruction_738(bytes, ctx),
        20 => match_node_instruction_739(bytes, ctx),
        21 => match_node_instruction_740(bytes, ctx),
        22 => match_node_instruction_741(bytes, ctx),
        23 => match_node_instruction_742(bytes, ctx),
        24 => match_node_instruction_743(bytes, ctx),
        25 => match_node_instruction_744(bytes, ctx),
        26 => match_node_instruction_745(bytes, ctx),
        27 => match_node_instruction_746(bytes, ctx),
        28 => match_node_instruction_747(bytes, ctx),
        29 => match_node_instruction_748(bytes, ctx),
        30 => match_node_instruction_749(bytes, ctx),
        31 => match_node_instruction_750(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_719(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 719: Terminal matched constructor ID 67");
    67
}

fn match_node_instruction_720(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 720: Terminal matched constructor ID 69");
    69
}

fn match_node_instruction_721(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 721: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_722(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 722: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_723(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 723: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_724(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 724: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_725(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 725: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_726(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 726: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_727(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 727: Terminal matched constructor ID 98");
    98
}

fn match_node_instruction_728(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 728: Terminal matched constructor ID 100");
    100
}

fn match_node_instruction_729(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 729: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_730(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 730: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_731(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 731: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_732(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 732: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_733(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 733: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_734(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 734: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_735(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 735: Terminal matched constructor ID 86");
    86
}

fn match_node_instruction_736(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 736: Terminal matched constructor ID 88");
    88
}

fn match_node_instruction_737(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 737: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_738(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 738: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_739(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 739: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_740(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 740: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_741(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 741: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_742(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 742: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_743(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 743: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_744(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 744: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_745(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 745: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_746(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 746: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_747(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 747: Terminal matched NOTHING");
    -1
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
    eprintln!("Trace node 750: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_751(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 751: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_752(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 752: Terminal matched constructor ID 212");
    212
}

fn match_node_instruction_753(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 753: Terminal matched constructor ID 211");
    211
}

fn match_node_instruction_754(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 7;
    eprintln!("Trace node 754: SlaInstructionBits start=20, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_755(bytes, ctx),
        1 => match_node_instruction_756(bytes, ctx),
        2 => match_node_instruction_757(bytes, ctx),
        3 => match_node_instruction_758(bytes, ctx),
        4 => match_node_instruction_759(bytes, ctx),
        5 => match_node_instruction_760(bytes, ctx),
        6 => match_node_instruction_761(bytes, ctx),
        7 => match_node_instruction_762(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_755(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 755: Terminal matched constructor ID 182");
    182
}

fn match_node_instruction_756(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 756: Terminal matched constructor ID 169");
    169
}

fn match_node_instruction_757(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 757: Terminal matched constructor ID 163");
    163
}

fn match_node_instruction_758(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 758: Terminal matched constructor ID 157");
    157
}

fn match_node_instruction_759(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 759: Terminal matched constructor ID 154");
    154
}

fn match_node_instruction_760(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 760: Terminal matched constructor ID 218");
    218
}

fn match_node_instruction_761(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 761: Terminal matched constructor ID 210");
    210
}

fn match_node_instruction_762(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 762: Terminal matched constructor ID 199");
    199
}

fn match_node_retCond4Sub_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 3;
    eprintln!("Trace node 0: SlaInstructionBits start=14, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_retCond4Sub_1(bytes, ctx),
        1 => match_node_retCond4Sub_2(bytes, ctx),
        2 => match_node_retCond4Sub_3(bytes, ctx),
        3 => match_node_retCond4Sub_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_retCond4Sub_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 3");
    3
}

fn match_node_retCond4Sub_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 2");
    2
}

fn match_node_retCond4Sub_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 1");
    1
}

fn match_node_retCond4Sub_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 0");
    0
}

fn match_node_sDisp21_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_sDisp8_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

