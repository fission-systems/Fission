// Auto-generated Fission Native Backend
#[no_mangle]
pub extern "C" fn fission_decode_match(table_ptr: *const i8, bytes: *const u8, bytes_len: usize, ctx_ptr: *const u64) -> i32 {
    let table_name = unsafe { std::ffi::CStr::from_ptr(table_ptr).to_str().unwrap() };
    let bytes = unsafe { std::slice::from_raw_parts(bytes, bytes_len) };
    let ctx = unsafe { *ctx_ptr };
    match table_name {
        "AFLAG" => match_node_AFLAG_0(bytes, ctx),
        "Addr11" => match_node_Addr11_0(bytes, ctx),
        "Addr24" => match_node_Addr24_0(bytes, ctx),
        "Addr5" => match_node_Addr5_0(bytes, ctx),
        "Addr8" => match_node_Addr8_0(bytes, ctx),
        "ArmPCRelImmed12" => match_node_ArmPCRelImmed12_0(bytes, ctx),
        "ByteRotate" => match_node_ByteRotate_0(bytes, ctx),
        "COND" => match_node_COND_0(bytes, ctx),
        "CheckInIT_CZN" => match_node_CheckInIT_CZN_0(bytes, ctx),
        "CheckInIT_CZNO" => match_node_CheckInIT_CZNO_0(bytes, ctx),
        "CheckInIT_ZN" => match_node_CheckInIT_ZN_0(bytes, ctx),
        "Dd" => match_node_Dd_0(bytes, ctx),
        "Dm" => match_node_Dm_0(bytes, ctx),
        "Dn" => match_node_Dn_0(bytes, ctx),
        "Dreg" => match_node_Dreg_0(bytes, ctx),
        "FFLAG" => match_node_FFLAG_0(bytes, ctx),
        "HAddr24" => match_node_HAddr24_0(bytes, ctx),
        "Hrd0002" => match_node_Hrd0002_0(bytes, ctx),
        "Hrm0305" => match_node_Hrm0305_0(bytes, ctx),
        "Hrn0002" => match_node_Hrn0002_0(bytes, ctx),
        "IFLAG" => match_node_IFLAG_0(bytes, ctx),
        "IFLAGS" => match_node_IFLAGS_0(bytes, ctx),
        "Immed12" => match_node_Immed12_0(bytes, ctx),
        "Immed16" => match_node_Immed16_0(bytes, ctx),
        "Immed3" => match_node_Immed3_0(bytes, ctx),
        "Immed4" => match_node_Immed4_0(bytes, ctx),
        "Immed5" => match_node_Immed5_0(bytes, ctx),
        "Immed7_4" => match_node_Immed7_4_0(bytes, ctx),
        "Immed8" => match_node_Immed8_0(bytes, ctx),
        "Immed8_4" => match_node_Immed8_4_0(bytes, ctx),
        "ItCond" => match_node_ItCond_0(bytes, ctx),
        "LdRtype0" => match_node_LdRtype0_0(bytes, ctx),
        "LdRtype1" => match_node_LdRtype1_0(bytes, ctx),
        "LdRtype2" => match_node_LdRtype2_0(bytes, ctx),
        "LdRtype3" => match_node_LdRtype3_0(bytes, ctx),
        "LdRtype4" => match_node_LdRtype4_0(bytes, ctx),
        "LdRtype5" => match_node_LdRtype5_0(bytes, ctx),
        "LdRtype6" => match_node_LdRtype6_0(bytes, ctx),
        "NegPcrelImmed12Addr" => match_node_NegPcrelImmed12Addr_0(bytes, ctx),
        "Pcrel" => match_node_Pcrel_0(bytes, ctx),
        "Pcrel8" => match_node_Pcrel8_0(bytes, ctx),
        "Pcrel8Indirect" => match_node_Pcrel8Indirect_0(bytes, ctx),
        "Pcrel8_s8" => match_node_Pcrel8_s8_0(bytes, ctx),
        "PcrelImmed12Addr" => match_node_PcrelImmed12Addr_0(bytes, ctx),
        "PcrelOffset12" => match_node_PcrelOffset12_0(bytes, ctx),
        "PshType1" => match_node_PshType1_0(bytes, ctx),
        "PshType2" => match_node_PshType2_0(bytes, ctx),
        "PshType3" => match_node_PshType3_0(bytes, ctx),
        "PshType4" => match_node_PshType4_0(bytes, ctx),
        "PshType5" => match_node_PshType5_0(bytes, ctx),
        "PshType6" => match_node_PshType6_0(bytes, ctx),
        "PshType7" => match_node_PshType7_0(bytes, ctx),
        "RnIndirect1" => match_node_RnIndirect1_0(bytes, ctx),
        "RnIndirect12" => match_node_RnIndirect12_0(bytes, ctx),
        "RnIndirect2" => match_node_RnIndirect2_0(bytes, ctx),
        "RnIndirect4" => match_node_RnIndirect4_0(bytes, ctx),
        "RnIndirectPUW" => match_node_RnIndirectPUW_0(bytes, ctx),
        "RnIndirectPUW1" => match_node_RnIndirectPUW1_0(bytes, ctx),
        "RnRmIndirect" => match_node_RnRmIndirect_0(bytes, ctx),
        "Rn_exclaim" => match_node_Rn_exclaim_0(bytes, ctx),
        "Rn_exclaim_WB" => match_node_Rn_exclaim_WB_0(bytes, ctx),
        "RtGotoCheck" => match_node_RtGotoCheck_0(bytes, ctx),
        "SBIT_CZNO" => match_node_SBIT_CZNO_0(bytes, ctx),
        "SBIT_ZN" => match_node_SBIT_ZN_0(bytes, ctx),
        "SRSMode" => match_node_SRSMode_0(bytes, ctx),
        "Sd" => match_node_Sd_0(bytes, ctx),
        "SetMode" => match_node_SetMode_0(bytes, ctx),
        "Sm" => match_node_Sm_0(bytes, ctx),
        "SmNext" => match_node_SmNext_0(bytes, ctx),
        "Sn" => match_node_Sn_0(bytes, ctx),
        "Sprel8" => match_node_Sprel8_0(bytes, ctx),
        "Sprel8Indirect" => match_node_Sprel8Indirect_0(bytes, ctx),
        "Sreg" => match_node_Sreg_0(bytes, ctx),
        "StrType0" => match_node_StrType0_0(bytes, ctx),
        "StrType1" => match_node_StrType1_0(bytes, ctx),
        "StrType2" => match_node_StrType2_0(bytes, ctx),
        "StrType3" => match_node_StrType3_0(bytes, ctx),
        "StrType4" => match_node_StrType4_0(bytes, ctx),
        "StrType5" => match_node_StrType5_0(bytes, ctx),
        "StrType6" => match_node_StrType6_0(bytes, ctx),
        "StrType7" => match_node_StrType7_0(bytes, ctx),
        "ThAddr20" => match_node_ThAddr20_0(bytes, ctx),
        "ThAddr24" => match_node_ThAddr24_0(bytes, ctx),
        "ThArmAddr23" => match_node_ThArmAddr23_0(bytes, ctx),
        "ThumbExpandImm12" => match_node_ThumbExpandImm12_0(bytes, ctx),
        "VRd" => match_node_VRd_0(bytes, ctx),
        "VRn" => match_node_VRn_0(bytes, ctx),
        "X" => match_node_X_0(bytes, ctx),
        "XBIT" => match_node_XBIT_0(bytes, ctx),
        "XYZ" => match_node_XYZ_0(bytes, ctx),
        "Y" => match_node_Y_0(bytes, ctx),
        "YBIT" => match_node_YBIT_0(bytes, ctx),
        "Z" => match_node_Z_0(bytes, ctx),
        "addr2shift" => match_node_addr2shift_0(bytes, ctx),
        "addrmode2" => match_node_addrmode2_0(bytes, ctx),
        "addrmode3" => match_node_addrmode3_0(bytes, ctx),
        "addrmode5" => match_node_addrmode5_0(bytes, ctx),
        "aflag" => match_node_aflag_0(bytes, ctx),
        "apsr" => match_node_apsr_0(bytes, ctx),
        "armEndianNess" => match_node_armEndianNess_0(bytes, ctx),
        "bitWidth" => match_node_bitWidth_0(bytes, ctx),
        "buildVldmDdList" => match_node_buildVldmDdList_0(bytes, ctx),
        "buildVldmSdList" => match_node_buildVldmSdList_0(bytes, ctx),
        "buildVpopSd64List" => match_node_buildVpopSd64List_0(bytes, ctx),
        "buildVpopSdList" => match_node_buildVpopSdList_0(bytes, ctx),
        "buildVpushSd64List" => match_node_buildVpushSd64List_0(bytes, ctx),
        "buildVpushSdList" => match_node_buildVpushSdList_0(bytes, ctx),
        "buildVstmDdList" => match_node_buildVstmDdList_0(bytes, ctx),
        "buildVstmSdList" => match_node_buildVstmSdList_0(bytes, ctx),
        "bxns" => match_node_bxns_0(bytes, ctx),
        "cc" => match_node_cc_0(bytes, ctx),
        "cpsrmask" => match_node_cpsrmask_0(bytes, ctx),
        "fflag" => match_node_fflag_0(bytes, ctx),
        "iflag" => match_node_iflag_0(bytes, ctx),
        "iflags" => match_node_iflags_0(bytes, ctx),
        "immed12_4" => match_node_immed12_4_0(bytes, ctx),
        "instruction" => match_node_instruction_0(bytes, ctx),
        "it_thfcc" => match_node_it_thfcc_0(bytes, ctx),
        "ldbrace" => match_node_ldbrace_0(bytes, ctx),
        "ldec0" => match_node_ldec0_0(bytes, ctx),
        "ldec1" => match_node_ldec1_0(bytes, ctx),
        "ldec10" => match_node_ldec10_0(bytes, ctx),
        "ldec11" => match_node_ldec11_0(bytes, ctx),
        "ldec12" => match_node_ldec12_0(bytes, ctx),
        "ldec13" => match_node_ldec13_0(bytes, ctx),
        "ldec14" => match_node_ldec14_0(bytes, ctx),
        "ldec15" => match_node_ldec15_0(bytes, ctx),
        "ldec2" => match_node_ldec2_0(bytes, ctx),
        "ldec3" => match_node_ldec3_0(bytes, ctx),
        "ldec4" => match_node_ldec4_0(bytes, ctx),
        "ldec5" => match_node_ldec5_0(bytes, ctx),
        "ldec6" => match_node_ldec6_0(bytes, ctx),
        "ldec7" => match_node_ldec7_0(bytes, ctx),
        "ldec8" => match_node_ldec8_0(bytes, ctx),
        "ldec9" => match_node_ldec9_0(bytes, ctx),
        "ldlist" => match_node_ldlist_0(bytes, ctx),
        "ldlist_dec" => match_node_ldlist_dec_0(bytes, ctx),
        "ldlist_inc" => match_node_ldlist_inc_0(bytes, ctx),
        "linc0" => match_node_linc0_0(bytes, ctx),
        "linc1" => match_node_linc1_0(bytes, ctx),
        "linc10" => match_node_linc10_0(bytes, ctx),
        "linc11" => match_node_linc11_0(bytes, ctx),
        "linc12" => match_node_linc12_0(bytes, ctx),
        "linc13" => match_node_linc13_0(bytes, ctx),
        "linc14" => match_node_linc14_0(bytes, ctx),
        "linc15" => match_node_linc15_0(bytes, ctx),
        "linc2" => match_node_linc2_0(bytes, ctx),
        "linc3" => match_node_linc3_0(bytes, ctx),
        "linc4" => match_node_linc4_0(bytes, ctx),
        "linc5" => match_node_linc5_0(bytes, ctx),
        "linc6" => match_node_linc6_0(bytes, ctx),
        "linc7" => match_node_linc7_0(bytes, ctx),
        "linc8" => match_node_linc8_0(bytes, ctx),
        "linc9" => match_node_linc9_0(bytes, ctx),
        "lsbImm" => match_node_lsbImm_0(bytes, ctx),
        "mcrOperands" => match_node_mcrOperands_0(bytes, ctx),
        "mdir" => match_node_mdir_0(bytes, ctx),
        "msbImm" => match_node_msbImm_0(bytes, ctx),
        "nanx" => match_node_nanx_0(bytes, ctx),
        "optionImm" => match_node_optionImm_0(bytes, ctx),
        "part2thcc" => match_node_part2thcc_0(bytes, ctx),
        "pclbrace" => match_node_pclbrace_0(bytes, ctx),
        "pcpbrace" => match_node_pcpbrace_0(bytes, ctx),
        "psbrace" => match_node_psbrace_0(bytes, ctx),
        "pshlist" => match_node_pshlist_0(bytes, ctx),
        "reglist" => match_node_reglist_0(bytes, ctx),
        "rm" => match_node_rm_0(bytes, ctx),
        "rn" => match_node_rn_0(bytes, ctx),
        "ror1" => match_node_ror1_0(bytes, ctx),
        "roundMode" => match_node_roundMode_0(bytes, ctx),
        "rs" => match_node_rs_0(bytes, ctx),
        "sSatImm4" => match_node_sSatImm4_0(bytes, ctx),
        "sSatImm5" => match_node_sSatImm5_0(bytes, ctx),
        "sdec0" => match_node_sdec0_0(bytes, ctx),
        "sdec1" => match_node_sdec1_0(bytes, ctx),
        "sdec10" => match_node_sdec10_0(bytes, ctx),
        "sdec11" => match_node_sdec11_0(bytes, ctx),
        "sdec12" => match_node_sdec12_0(bytes, ctx),
        "sdec13" => match_node_sdec13_0(bytes, ctx),
        "sdec14" => match_node_sdec14_0(bytes, ctx),
        "sdec15" => match_node_sdec15_0(bytes, ctx),
        "sdec2" => match_node_sdec2_0(bytes, ctx),
        "sdec3" => match_node_sdec3_0(bytes, ctx),
        "sdec4" => match_node_sdec4_0(bytes, ctx),
        "sdec5" => match_node_sdec5_0(bytes, ctx),
        "sdec6" => match_node_sdec6_0(bytes, ctx),
        "sdec7" => match_node_sdec7_0(bytes, ctx),
        "sdec8" => match_node_sdec8_0(bytes, ctx),
        "sdec9" => match_node_sdec9_0(bytes, ctx),
        "shift1" => match_node_shift1_0(bytes, ctx),
        "shift2" => match_node_shift2_0(bytes, ctx),
        "shift3" => match_node_shift3_0(bytes, ctx),
        "shift4" => match_node_shift4_0(bytes, ctx),
        "sinc0" => match_node_sinc0_0(bytes, ctx),
        "sinc1" => match_node_sinc1_0(bytes, ctx),
        "sinc10" => match_node_sinc10_0(bytes, ctx),
        "sinc11" => match_node_sinc11_0(bytes, ctx),
        "sinc12" => match_node_sinc12_0(bytes, ctx),
        "sinc13" => match_node_sinc13_0(bytes, ctx),
        "sinc14" => match_node_sinc14_0(bytes, ctx),
        "sinc15" => match_node_sinc15_0(bytes, ctx),
        "sinc2" => match_node_sinc2_0(bytes, ctx),
        "sinc3" => match_node_sinc3_0(bytes, ctx),
        "sinc4" => match_node_sinc4_0(bytes, ctx),
        "sinc5" => match_node_sinc5_0(bytes, ctx),
        "sinc6" => match_node_sinc6_0(bytes, ctx),
        "sinc7" => match_node_sinc7_0(bytes, ctx),
        "sinc8" => match_node_sinc8_0(bytes, ctx),
        "sinc9" => match_node_sinc9_0(bytes, ctx),
        "spsrmask" => match_node_spsrmask_0(bytes, ctx),
        "stbrace" => match_node_stbrace_0(bytes, ctx),
        "stlist_dec" => match_node_stlist_dec_0(bytes, ctx),
        "stlist_inc" => match_node_stlist_inc_0(bytes, ctx),
        "strlist" => match_node_strlist_0(bytes, ctx),
        "taddrmode5" => match_node_taddrmode5_0(bytes, ctx),
        "th2_SetMode" => match_node_th2_SetMode_0(bytes, ctx),
        "th2_aflag" => match_node_th2_aflag_0(bytes, ctx),
        "th2_fflag" => match_node_th2_fflag_0(bytes, ctx),
        "th2_iflag" => match_node_th2_iflag_0(bytes, ctx),
        "th2_iflags" => match_node_th2_iflags_0(bytes, ctx),
        "th2_shift0" => match_node_th2_shift0_0(bytes, ctx),
        "th2_shift1" => match_node_th2_shift1_0(bytes, ctx),
        "thBitWidth" => match_node_thBitWidth_0(bytes, ctx),
        "thLsbImm" => match_node_thLsbImm_0(bytes, ctx),
        "thMsbImm" => match_node_thMsbImm_0(bytes, ctx),
        "thSBIT_CZN" => match_node_thSBIT_CZN_0(bytes, ctx),
        "thSBIT_CZNO" => match_node_thSBIT_CZNO_0(bytes, ctx),
        "thSBIT_ZN" => match_node_thSBIT_ZN_0(bytes, ctx),
        "thSRSMode" => match_node_thSRSMode_0(bytes, ctx),
        "thWidthMinus1" => match_node_thWidthMinus1_0(bytes, ctx),
        "thXBIT" => match_node_thXBIT_0(bytes, ctx),
        "thYBIT" => match_node_thYBIT_0(bytes, ctx),
        "thcc" => match_node_thcc_0(bytes, ctx),
        "thcpsrmask" => match_node_thcpsrmask_0(bytes, ctx),
        "thdXbot" => match_node_thdXbot_0(bytes, ctx),
        "thdXtop" => match_node_thdXtop_0(bytes, ctx),
        "thfcc" => match_node_thfcc_0(bytes, ctx),
        "thldrlist_dec" => match_node_thldrlist_dec_0(bytes, ctx),
        "thldrlist_inc" => match_node_thldrlist_inc_0(bytes, ctx),
        "thpsrmask" => match_node_thpsrmask_0(bytes, ctx),
        "thrldec1" => match_node_thrldec1_0(bytes, ctx),
        "thrldec10" => match_node_thrldec10_0(bytes, ctx),
        "thrldec11" => match_node_thrldec11_0(bytes, ctx),
        "thrldec12" => match_node_thrldec12_0(bytes, ctx),
        "thrldec13" => match_node_thrldec13_0(bytes, ctx),
        "thrldec14" => match_node_thrldec14_0(bytes, ctx),
        "thrldec15" => match_node_thrldec15_0(bytes, ctx),
        "thrldec2" => match_node_thrldec2_0(bytes, ctx),
        "thrldec3" => match_node_thrldec3_0(bytes, ctx),
        "thrldec4" => match_node_thrldec4_0(bytes, ctx),
        "thrldec5" => match_node_thrldec5_0(bytes, ctx),
        "thrldec6" => match_node_thrldec6_0(bytes, ctx),
        "thrldec7" => match_node_thrldec7_0(bytes, ctx),
        "thrldec8" => match_node_thrldec8_0(bytes, ctx),
        "thrldec9" => match_node_thrldec9_0(bytes, ctx),
        "thrlist1" => match_node_thrlist1_0(bytes, ctx),
        "thrlist10" => match_node_thrlist10_0(bytes, ctx),
        "thrlist11" => match_node_thrlist11_0(bytes, ctx),
        "thrlist12" => match_node_thrlist12_0(bytes, ctx),
        "thrlist13" => match_node_thrlist13_0(bytes, ctx),
        "thrlist14" => match_node_thrlist14_0(bytes, ctx),
        "thrlist15" => match_node_thrlist15_0(bytes, ctx),
        "thrlist2" => match_node_thrlist2_0(bytes, ctx),
        "thrlist3" => match_node_thrlist3_0(bytes, ctx),
        "thrlist4" => match_node_thrlist4_0(bytes, ctx),
        "thrlist5" => match_node_thrlist5_0(bytes, ctx),
        "thrlist6" => match_node_thrlist6_0(bytes, ctx),
        "thrlist7" => match_node_thrlist7_0(bytes, ctx),
        "thrlist8" => match_node_thrlist8_0(bytes, ctx),
        "thrlist9" => match_node_thrlist9_0(bytes, ctx),
        "thsdec1" => match_node_thsdec1_0(bytes, ctx),
        "thsdec10" => match_node_thsdec10_0(bytes, ctx),
        "thsdec11" => match_node_thsdec11_0(bytes, ctx),
        "thsdec12" => match_node_thsdec12_0(bytes, ctx),
        "thsdec13" => match_node_thsdec13_0(bytes, ctx),
        "thsdec14" => match_node_thsdec14_0(bytes, ctx),
        "thsdec15" => match_node_thsdec15_0(bytes, ctx),
        "thsdec2" => match_node_thsdec2_0(bytes, ctx),
        "thsdec3" => match_node_thsdec3_0(bytes, ctx),
        "thsdec4" => match_node_thsdec4_0(bytes, ctx),
        "thsdec5" => match_node_thsdec5_0(bytes, ctx),
        "thsdec6" => match_node_thsdec6_0(bytes, ctx),
        "thsdec7" => match_node_thsdec7_0(bytes, ctx),
        "thsdec8" => match_node_thsdec8_0(bytes, ctx),
        "thsdec9" => match_node_thsdec9_0(bytes, ctx),
        "thshift2" => match_node_thshift2_0(bytes, ctx),
        "thsinc1" => match_node_thsinc1_0(bytes, ctx),
        "thsinc10" => match_node_thsinc10_0(bytes, ctx),
        "thsinc11" => match_node_thsinc11_0(bytes, ctx),
        "thsinc12" => match_node_thsinc12_0(bytes, ctx),
        "thsinc13" => match_node_thsinc13_0(bytes, ctx),
        "thsinc14" => match_node_thsinc14_0(bytes, ctx),
        "thsinc15" => match_node_thsinc15_0(bytes, ctx),
        "thsinc2" => match_node_thsinc2_0(bytes, ctx),
        "thsinc3" => match_node_thsinc3_0(bytes, ctx),
        "thsinc4" => match_node_thsinc4_0(bytes, ctx),
        "thsinc5" => match_node_thsinc5_0(bytes, ctx),
        "thsinc6" => match_node_thsinc6_0(bytes, ctx),
        "thsinc7" => match_node_thsinc7_0(bytes, ctx),
        "thsinc8" => match_node_thsinc8_0(bytes, ctx),
        "thsinc9" => match_node_thsinc9_0(bytes, ctx),
        "thspsrmask" => match_node_thspsrmask_0(bytes, ctx),
        "thstrlist_dec" => match_node_thstrlist_dec_0(bytes, ctx),
        "thstrlist_inc" => match_node_thstrlist_inc_0(bytes, ctx),
        "thumbEndianNess" => match_node_thumbEndianNess_0(bytes, ctx),
        "uSatImm4" => match_node_uSatImm4_0(bytes, ctx),
        "uSatImm5" => match_node_uSatImm5_0(bytes, ctx),
        "vldmDdList" => match_node_vldmDdList_0(bytes, ctx),
        "vldmOffset" => match_node_vldmOffset_0(bytes, ctx),
        "vldmRn" => match_node_vldmRn_0(bytes, ctx),
        "vldmSdList" => match_node_vldmSdList_0(bytes, ctx),
        "vldmUpdate" => match_node_vldmUpdate_0(bytes, ctx),
        "vldrRn" => match_node_vldrRn_0(bytes, ctx),
        "vmrsReg" => match_node_vmrsReg_0(bytes, ctx),
        "vpopSd64List" => match_node_vpopSd64List_0(bytes, ctx),
        "vpopSdList" => match_node_vpopSdList_0(bytes, ctx),
        "vpushSd64List" => match_node_vpushSd64List_0(bytes, ctx),
        "vpushSdList" => match_node_vpushSdList_0(bytes, ctx),
        "vstmDdList" => match_node_vstmDdList_0(bytes, ctx),
        "vstmSdList" => match_node_vstmSdList_0(bytes, ctx),
        "widthMinus1" => match_node_widthMinus1_0(bytes, ctx),
        "zero" => match_node_zero_0(bytes, ctx),
        _ => -1
    }
}

fn match_node_AFLAG_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (23 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 23) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=23, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_AFLAG_1(bytes, ctx),
        1 => match_node_AFLAG_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_AFLAG_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_AFLAG_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_Addr11_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_Addr24_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_Addr5_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_Addr8_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_ArmPCRelImmed12_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ArmPCRelImmed12_1(bytes, ctx),
        1 => match_node_ArmPCRelImmed12_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ArmPCRelImmed12_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_ArmPCRelImmed12_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_ByteRotate_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_COND_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 0: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_COND_1(bytes, ctx),
        1 => match_node_COND_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_COND_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_COND_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 2");
    2
}

fn match_node_CheckInIT_CZN_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 5) & 1;
    eprintln!("Trace node 0: SlaContextBits start=5, size=1, probe={}", probe);
    match probe {
        0 => match_node_CheckInIT_CZN_1(bytes, ctx),
        1 => match_node_CheckInIT_CZN_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_CheckInIT_CZN_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 3");
    3
}

fn match_node_CheckInIT_CZN_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_CheckInIT_CZNO_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 5) & 1;
    eprintln!("Trace node 0: SlaContextBits start=5, size=1, probe={}", probe);
    match probe {
        0 => match_node_CheckInIT_CZNO_1(bytes, ctx),
        1 => match_node_CheckInIT_CZNO_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_CheckInIT_CZNO_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 3");
    3
}

fn match_node_CheckInIT_CZNO_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_CheckInIT_ZN_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 5) & 1;
    eprintln!("Trace node 0: SlaContextBits start=5, size=1, probe={}", probe);
    match probe {
        0 => match_node_CheckInIT_ZN_1(bytes, ctx),
        1 => match_node_CheckInIT_ZN_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_CheckInIT_ZN_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 3");
    3
}

fn match_node_CheckInIT_ZN_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_Dd_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 0: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_Dd_1(bytes, ctx),
        1 => match_node_Dd_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Dd_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_Dd_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_Dm_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 0: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_Dm_1(bytes, ctx),
        1 => match_node_Dm_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Dm_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_Dm_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_Dn_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 0: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_Dn_1(bytes, ctx),
        1 => match_node_Dn_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Dn_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_Dn_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_Dreg_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 19) & 31;
    eprintln!("Trace node 0: SlaContextBits start=19, size=5, probe={}", probe);
    match probe {
        0 => match_node_Dreg_1(bytes, ctx),
        1 => match_node_Dreg_2(bytes, ctx),
        2 => match_node_Dreg_3(bytes, ctx),
        3 => match_node_Dreg_4(bytes, ctx),
        4 => match_node_Dreg_5(bytes, ctx),
        5 => match_node_Dreg_6(bytes, ctx),
        6 => match_node_Dreg_7(bytes, ctx),
        7 => match_node_Dreg_8(bytes, ctx),
        8 => match_node_Dreg_9(bytes, ctx),
        9 => match_node_Dreg_10(bytes, ctx),
        10 => match_node_Dreg_11(bytes, ctx),
        11 => match_node_Dreg_12(bytes, ctx),
        12 => match_node_Dreg_13(bytes, ctx),
        13 => match_node_Dreg_14(bytes, ctx),
        14 => match_node_Dreg_15(bytes, ctx),
        15 => match_node_Dreg_16(bytes, ctx),
        16 => match_node_Dreg_17(bytes, ctx),
        17 => match_node_Dreg_18(bytes, ctx),
        18 => match_node_Dreg_19(bytes, ctx),
        19 => match_node_Dreg_20(bytes, ctx),
        20 => match_node_Dreg_21(bytes, ctx),
        21 => match_node_Dreg_22(bytes, ctx),
        22 => match_node_Dreg_23(bytes, ctx),
        23 => match_node_Dreg_24(bytes, ctx),
        24 => match_node_Dreg_25(bytes, ctx),
        25 => match_node_Dreg_26(bytes, ctx),
        26 => match_node_Dreg_27(bytes, ctx),
        27 => match_node_Dreg_28(bytes, ctx),
        28 => match_node_Dreg_29(bytes, ctx),
        29 => match_node_Dreg_30(bytes, ctx),
        30 => match_node_Dreg_31(bytes, ctx),
        31 => match_node_Dreg_32(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Dreg_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_Dreg_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_Dreg_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_Dreg_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_Dreg_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 4");
    4
}

fn match_node_Dreg_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_Dreg_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 6");
    6
}

fn match_node_Dreg_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 7");
    7
}

fn match_node_Dreg_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 8");
    8
}

fn match_node_Dreg_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 9");
    9
}

fn match_node_Dreg_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 10");
    10
}

fn match_node_Dreg_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 11");
    11
}

fn match_node_Dreg_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 12");
    12
}

fn match_node_Dreg_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 13");
    13
}

fn match_node_Dreg_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 14");
    14
}

fn match_node_Dreg_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 15");
    15
}

fn match_node_Dreg_17(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 17: Terminal matched NOTHING");
    -1
}

fn match_node_Dreg_18(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 18: Terminal matched NOTHING");
    -1
}

fn match_node_Dreg_19(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 19: Terminal matched NOTHING");
    -1
}

fn match_node_Dreg_20(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 20: Terminal matched NOTHING");
    -1
}

fn match_node_Dreg_21(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 21: Terminal matched NOTHING");
    -1
}

fn match_node_Dreg_22(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 22: Terminal matched NOTHING");
    -1
}

fn match_node_Dreg_23(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 23: Terminal matched NOTHING");
    -1
}

fn match_node_Dreg_24(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 24: Terminal matched NOTHING");
    -1
}

fn match_node_Dreg_25(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 25: Terminal matched NOTHING");
    -1
}

fn match_node_Dreg_26(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 26: Terminal matched NOTHING");
    -1
}

fn match_node_Dreg_27(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 27: Terminal matched NOTHING");
    -1
}

fn match_node_Dreg_28(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 28: Terminal matched NOTHING");
    -1
}

fn match_node_Dreg_29(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 29: Terminal matched NOTHING");
    -1
}

fn match_node_Dreg_30(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 30: Terminal matched NOTHING");
    -1
}

fn match_node_Dreg_31(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 31: Terminal matched NOTHING");
    -1
}

fn match_node_Dreg_32(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 32: Terminal matched constructor ID 16");
    16
}

fn match_node_FFLAG_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=25, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_FFLAG_1(bytes, ctx),
        1 => match_node_FFLAG_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_FFLAG_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_FFLAG_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_HAddr24_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_Hrd0002_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_Hrd0002_1(bytes, ctx),
        1 => match_node_Hrd0002_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Hrd0002_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_Hrd0002_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 2");
    2
}

fn match_node_Hrm0305_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_Hrm0305_1(bytes, ctx),
        1 => match_node_Hrm0305_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Hrm0305_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_Hrm0305_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 2");
    2
}

fn match_node_Hrn0002_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_Hrn0002_1(bytes, ctx),
        1 => match_node_Hrn0002_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Hrn0002_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_Hrn0002_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 2");
    2
}

fn match_node_IFLAG_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=24, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_IFLAG_1(bytes, ctx),
        1 => match_node_IFLAG_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_IFLAG_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_IFLAG_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_IFLAGS_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_Immed12_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_Immed16_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_Immed3_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_Immed4_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_Immed5_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_Immed7_4_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_Immed8_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_Immed8_4_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_ItCond_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 10) & 1;
    eprintln!("Trace node 0: SlaContextBits start=10, size=1, probe={}", probe);
    match probe {
        0 => match_node_ItCond_1(bytes, ctx),
        1 => match_node_ItCond_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ItCond_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_ItCond_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 3");
    3
}

fn match_node_LdRtype0_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LdRtype0_1(bytes, ctx),
        1 => match_node_LdRtype0_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LdRtype0_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_LdRtype0_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LdRtype1_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LdRtype1_1(bytes, ctx),
        1 => match_node_LdRtype1_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LdRtype1_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_LdRtype1_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LdRtype2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LdRtype2_1(bytes, ctx),
        1 => match_node_LdRtype2_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LdRtype2_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_LdRtype2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LdRtype3_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LdRtype3_1(bytes, ctx),
        1 => match_node_LdRtype3_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LdRtype3_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_LdRtype3_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LdRtype4_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LdRtype4_1(bytes, ctx),
        1 => match_node_LdRtype4_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LdRtype4_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_LdRtype4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LdRtype5_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LdRtype5_1(bytes, ctx),
        1 => match_node_LdRtype5_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LdRtype5_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_LdRtype5_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_LdRtype6_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_LdRtype6_1(bytes, ctx),
        1 => match_node_LdRtype6_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_LdRtype6_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_LdRtype6_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_NegPcrelImmed12Addr_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_Pcrel_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_Pcrel_1(bytes, ctx),
        1 => match_node_Pcrel_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Pcrel_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_Pcrel_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_Pcrel8_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_Pcrel8Indirect_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_Pcrel8_s8_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_PcrelImmed12Addr_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_PcrelOffset12_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_PcrelOffset12_1(bytes, ctx),
        1 => match_node_PcrelOffset12_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_PcrelOffset12_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_PcrelOffset12_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_PshType1_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_PshType1_1(bytes, ctx),
        1 => match_node_PshType1_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_PshType1_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_PshType1_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_PshType2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_PshType2_1(bytes, ctx),
        1 => match_node_PshType2_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_PshType2_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_PshType2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_PshType3_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_PshType3_1(bytes, ctx),
        1 => match_node_PshType3_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_PshType3_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_PshType3_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_PshType4_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_PshType4_1(bytes, ctx),
        1 => match_node_PshType4_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_PshType4_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_PshType4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_PshType5_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_PshType5_1(bytes, ctx),
        1 => match_node_PshType5_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_PshType5_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_PshType5_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_PshType6_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_PshType6_1(bytes, ctx),
        1 => match_node_PshType6_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_PshType6_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_PshType6_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_PshType7_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_PshType7_1(bytes, ctx),
        1 => match_node_PshType7_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_PshType7_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_PshType7_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_RnIndirect1_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_RnIndirect12_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_RnIndirect2_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_RnIndirect4_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_RnIndirectPUW_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (21 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 21) & 7;
    eprintln!("Trace node 0: SlaInstructionBits start=21, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_RnIndirectPUW_1(bytes, ctx),
        1 => match_node_RnIndirectPUW_2(bytes, ctx),
        2 => match_node_RnIndirectPUW_3(bytes, ctx),
        3 => match_node_RnIndirectPUW_4(bytes, ctx),
        4 => match_node_RnIndirectPUW_5(bytes, ctx),
        5 => match_node_RnIndirectPUW_6(bytes, ctx),
        6 => match_node_RnIndirectPUW_7(bytes, ctx),
        7 => match_node_RnIndirectPUW_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_RnIndirectPUW_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched NOTHING");
    -1
}

fn match_node_RnIndirectPUW_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_RnIndirectPUW_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched NOTHING");
    -1
}

fn match_node_RnIndirectPUW_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 1");
    1
}

fn match_node_RnIndirectPUW_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 2");
    2
}

fn match_node_RnIndirectPUW_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 3");
    3
}

fn match_node_RnIndirectPUW_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched NOTHING");
    -1
}

fn match_node_RnIndirectPUW_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 4");
    4
}

fn match_node_RnIndirectPUW1_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 15;
    eprintln!("Trace node 0: SlaInstructionBits start=7, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_RnIndirectPUW1_1(bytes, ctx),
        1 => match_node_RnIndirectPUW1_2(bytes, ctx),
        2 => match_node_RnIndirectPUW1_3(bytes, ctx),
        3 => match_node_RnIndirectPUW1_4(bytes, ctx),
        4 => match_node_RnIndirectPUW1_5(bytes, ctx),
        5 => match_node_RnIndirectPUW1_6(bytes, ctx),
        6 => match_node_RnIndirectPUW1_7(bytes, ctx),
        7 => match_node_RnIndirectPUW1_8(bytes, ctx),
        8 => match_node_RnIndirectPUW1_9(bytes, ctx),
        9 => match_node_RnIndirectPUW1_10(bytes, ctx),
        10 => match_node_RnIndirectPUW1_11(bytes, ctx),
        11 => match_node_RnIndirectPUW1_12(bytes, ctx),
        12 => match_node_RnIndirectPUW1_13(bytes, ctx),
        13 => match_node_RnIndirectPUW1_14(bytes, ctx),
        14 => match_node_RnIndirectPUW1_15(bytes, ctx),
        15 => match_node_RnIndirectPUW1_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_RnIndirectPUW1_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched NOTHING");
    -1
}

fn match_node_RnIndirectPUW1_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched NOTHING");
    -1
}

fn match_node_RnIndirectPUW1_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched NOTHING");
    -1
}

fn match_node_RnIndirectPUW1_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 0");
    0
}

fn match_node_RnIndirectPUW1_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched NOTHING");
    -1
}

fn match_node_RnIndirectPUW1_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched NOTHING");
    -1
}

fn match_node_RnIndirectPUW1_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched NOTHING");
    -1
}

fn match_node_RnIndirectPUW1_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 1");
    1
}

fn match_node_RnIndirectPUW1_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched NOTHING");
    -1
}

fn match_node_RnIndirectPUW1_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched NOTHING");
    -1
}

fn match_node_RnIndirectPUW1_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 2");
    2
}

fn match_node_RnIndirectPUW1_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 3");
    3
}

fn match_node_RnIndirectPUW1_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched NOTHING");
    -1
}

fn match_node_RnIndirectPUW1_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched NOTHING");
    -1
}

fn match_node_RnIndirectPUW1_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 4");
    4
}

fn match_node_RnIndirectPUW1_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 5");
    5
}

fn match_node_RnRmIndirect_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_Rn_exclaim_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 7;
    eprintln!("Trace node 0: SlaInstructionBits start=5, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_Rn_exclaim_1(bytes, ctx),
        1 => match_node_Rn_exclaim_2(bytes, ctx),
        2 => match_node_Rn_exclaim_3(bytes, ctx),
        3 => match_node_Rn_exclaim_4(bytes, ctx),
        4 => match_node_Rn_exclaim_5(bytes, ctx),
        5 => match_node_Rn_exclaim_6(bytes, ctx),
        6 => match_node_Rn_exclaim_7(bytes, ctx),
        7 => match_node_Rn_exclaim_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Rn_exclaim_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_Rn_exclaim_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_Rn_exclaim_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_Rn_exclaim_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_Rn_exclaim_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 4");
    4
}

fn match_node_Rn_exclaim_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_Rn_exclaim_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 6");
    6
}

fn match_node_Rn_exclaim_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 7");
    7
}

fn match_node_Rn_exclaim_WB_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 7;
    eprintln!("Trace node 0: SlaInstructionBits start=5, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_Rn_exclaim_WB_1(bytes, ctx),
        1 => match_node_Rn_exclaim_WB_2(bytes, ctx),
        2 => match_node_Rn_exclaim_WB_3(bytes, ctx),
        3 => match_node_Rn_exclaim_WB_4(bytes, ctx),
        4 => match_node_Rn_exclaim_WB_5(bytes, ctx),
        5 => match_node_Rn_exclaim_WB_6(bytes, ctx),
        6 => match_node_Rn_exclaim_WB_7(bytes, ctx),
        7 => match_node_Rn_exclaim_WB_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Rn_exclaim_WB_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_Rn_exclaim_WB_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_Rn_exclaim_WB_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_Rn_exclaim_WB_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_Rn_exclaim_WB_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 4");
    4
}

fn match_node_Rn_exclaim_WB_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_Rn_exclaim_WB_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 6");
    6
}

fn match_node_Rn_exclaim_WB_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 7");
    7
}

fn match_node_RtGotoCheck_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_SBIT_CZNO_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_SBIT_CZNO_1(bytes, ctx),
        1 => match_node_SBIT_CZNO_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_SBIT_CZNO_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_SBIT_CZNO_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_SBIT_ZN_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_SBIT_ZN_1(bytes, ctx),
        1 => match_node_SBIT_ZN_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_SBIT_ZN_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_SBIT_ZN_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_SRSMode_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (28 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 28) & 15;
    eprintln!("Trace node 0: SlaInstructionBits start=28, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_SRSMode_1(bytes, ctx),
        1 => match_node_SRSMode_2(bytes, ctx),
        2 => match_node_SRSMode_3(bytes, ctx),
        3 => match_node_SRSMode_4(bytes, ctx),
        4 => match_node_SRSMode_5(bytes, ctx),
        5 => match_node_SRSMode_6(bytes, ctx),
        6 => match_node_SRSMode_7(bytes, ctx),
        7 => match_node_SRSMode_8(bytes, ctx),
        8 => match_node_SRSMode_9(bytes, ctx),
        9 => match_node_SRSMode_10(bytes, ctx),
        10 => match_node_SRSMode_11(bytes, ctx),
        11 => match_node_SRSMode_12(bytes, ctx),
        12 => match_node_SRSMode_13(bytes, ctx),
        13 => match_node_SRSMode_14(bytes, ctx),
        14 => match_node_SRSMode_15(bytes, ctx),
        15 => match_node_SRSMode_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_SRSMode_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 8");
    8
}

fn match_node_SRSMode_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 8");
    8
}

fn match_node_SRSMode_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 8");
    8
}

fn match_node_SRSMode_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 6");
    6
}

fn match_node_SRSMode_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 8");
    8
}

fn match_node_SRSMode_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 8");
    8
}

fn match_node_SRSMode_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 8");
    8
}

fn match_node_SRSMode_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 7");
    7
}

fn match_node_SRSMode_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 0");
    0
}

fn match_node_SRSMode_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 1");
    1
}

fn match_node_SRSMode_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 2");
    2
}

fn match_node_SRSMode_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 3");
    3
}

fn match_node_SRSMode_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 8");
    8
}

fn match_node_SRSMode_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 8");
    8
}

fn match_node_SRSMode_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 4");
    4
}

fn match_node_SRSMode_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 5");
    5
}

fn match_node_Sd_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 0: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_Sd_1(bytes, ctx),
        1 => match_node_Sd_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Sd_1(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 1: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_Sd_2(bytes, ctx),
        1 => match_node_Sd_3(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Sd_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_Sd_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 1");
    1
}

fn match_node_Sd_4(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 4: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_Sd_5(bytes, ctx),
        1 => match_node_Sd_6(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Sd_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 2");
    2
}

fn match_node_Sd_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 3");
    3
}

fn match_node_SetMode_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (28 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 28) & 15;
    eprintln!("Trace node 0: SlaInstructionBits start=28, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_SetMode_1(bytes, ctx),
        1 => match_node_SetMode_2(bytes, ctx),
        2 => match_node_SetMode_3(bytes, ctx),
        3 => match_node_SetMode_4(bytes, ctx),
        4 => match_node_SetMode_5(bytes, ctx),
        5 => match_node_SetMode_6(bytes, ctx),
        6 => match_node_SetMode_7(bytes, ctx),
        7 => match_node_SetMode_8(bytes, ctx),
        8 => match_node_SetMode_9(bytes, ctx),
        9 => match_node_SetMode_10(bytes, ctx),
        10 => match_node_SetMode_11(bytes, ctx),
        11 => match_node_SetMode_12(bytes, ctx),
        12 => match_node_SetMode_13(bytes, ctx),
        13 => match_node_SetMode_14(bytes, ctx),
        14 => match_node_SetMode_15(bytes, ctx),
        15 => match_node_SetMode_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_SetMode_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_SetMode_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_SetMode_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_SetMode_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_SetMode_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched NOTHING");
    -1
}

fn match_node_SetMode_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched NOTHING");
    -1
}

fn match_node_SetMode_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 4");
    4
}

fn match_node_SetMode_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 5");
    5
}

fn match_node_SetMode_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched NOTHING");
    -1
}

fn match_node_SetMode_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched NOTHING");
    -1
}

fn match_node_SetMode_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched NOTHING");
    -1
}

fn match_node_SetMode_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 6");
    6
}

fn match_node_SetMode_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched NOTHING");
    -1
}

fn match_node_SetMode_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched NOTHING");
    -1
}

fn match_node_SetMode_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched NOTHING");
    -1
}

fn match_node_SetMode_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 7");
    7
}

fn match_node_Sm_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 0: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_Sm_1(bytes, ctx),
        1 => match_node_Sm_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Sm_1(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (26 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 26) & 1;
    eprintln!("Trace node 1: SlaInstructionBits start=26, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_Sm_2(bytes, ctx),
        1 => match_node_Sm_3(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Sm_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_Sm_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 1");
    1
}

fn match_node_Sm_4(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (26 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 26) & 1;
    eprintln!("Trace node 4: SlaInstructionBits start=26, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_Sm_5(bytes, ctx),
        1 => match_node_Sm_6(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Sm_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 2");
    2
}

fn match_node_Sm_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 3");
    3
}

fn match_node_SmNext_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 0: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_SmNext_1(bytes, ctx),
        1 => match_node_SmNext_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_SmNext_1(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (26 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 26) & 1;
    eprintln!("Trace node 1: SlaInstructionBits start=26, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_SmNext_2(bytes, ctx),
        1 => match_node_SmNext_3(bytes, ctx),
        _ => -1,
    }
}

fn match_node_SmNext_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_SmNext_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 1");
    1
}

fn match_node_SmNext_4(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (26 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 26) & 1;
    eprintln!("Trace node 4: SlaInstructionBits start=26, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_SmNext_5(bytes, ctx),
        1 => match_node_SmNext_6(bytes, ctx),
        _ => -1,
    }
}

fn match_node_SmNext_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 2");
    2
}

fn match_node_SmNext_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 3");
    3
}

fn match_node_Sn_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 0: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_Sn_1(bytes, ctx),
        1 => match_node_Sn_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Sn_1(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 1;
    eprintln!("Trace node 1: SlaInstructionBits start=24, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_Sn_2(bytes, ctx),
        1 => match_node_Sn_3(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Sn_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_Sn_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 1");
    1
}

fn match_node_Sn_4(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 1;
    eprintln!("Trace node 4: SlaInstructionBits start=24, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_Sn_5(bytes, ctx),
        1 => match_node_Sn_6(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Sn_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 2");
    2
}

fn match_node_Sn_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 3");
    3
}

fn match_node_Sprel8_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_Sprel8Indirect_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_Sreg_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 19) & 31;
    eprintln!("Trace node 0: SlaContextBits start=19, size=5, probe={}", probe);
    match probe {
        0 => match_node_Sreg_1(bytes, ctx),
        1 => match_node_Sreg_2(bytes, ctx),
        2 => match_node_Sreg_3(bytes, ctx),
        3 => match_node_Sreg_4(bytes, ctx),
        4 => match_node_Sreg_5(bytes, ctx),
        5 => match_node_Sreg_6(bytes, ctx),
        6 => match_node_Sreg_7(bytes, ctx),
        7 => match_node_Sreg_8(bytes, ctx),
        8 => match_node_Sreg_9(bytes, ctx),
        9 => match_node_Sreg_10(bytes, ctx),
        10 => match_node_Sreg_11(bytes, ctx),
        11 => match_node_Sreg_12(bytes, ctx),
        12 => match_node_Sreg_13(bytes, ctx),
        13 => match_node_Sreg_14(bytes, ctx),
        14 => match_node_Sreg_15(bytes, ctx),
        15 => match_node_Sreg_16(bytes, ctx),
        16 => match_node_Sreg_17(bytes, ctx),
        17 => match_node_Sreg_18(bytes, ctx),
        18 => match_node_Sreg_19(bytes, ctx),
        19 => match_node_Sreg_20(bytes, ctx),
        20 => match_node_Sreg_21(bytes, ctx),
        21 => match_node_Sreg_22(bytes, ctx),
        22 => match_node_Sreg_23(bytes, ctx),
        23 => match_node_Sreg_24(bytes, ctx),
        24 => match_node_Sreg_25(bytes, ctx),
        25 => match_node_Sreg_26(bytes, ctx),
        26 => match_node_Sreg_27(bytes, ctx),
        27 => match_node_Sreg_28(bytes, ctx),
        28 => match_node_Sreg_29(bytes, ctx),
        29 => match_node_Sreg_30(bytes, ctx),
        30 => match_node_Sreg_31(bytes, ctx),
        31 => match_node_Sreg_32(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Sreg_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_Sreg_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_Sreg_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_Sreg_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_Sreg_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 4");
    4
}

fn match_node_Sreg_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_Sreg_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 6");
    6
}

fn match_node_Sreg_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 7");
    7
}

fn match_node_Sreg_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 8");
    8
}

fn match_node_Sreg_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 9");
    9
}

fn match_node_Sreg_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 10");
    10
}

fn match_node_Sreg_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 11");
    11
}

fn match_node_Sreg_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 12");
    12
}

fn match_node_Sreg_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 13");
    13
}

fn match_node_Sreg_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 14");
    14
}

fn match_node_Sreg_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 15");
    15
}

fn match_node_Sreg_17(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 17: Terminal matched constructor ID 16");
    16
}

fn match_node_Sreg_18(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 18: Terminal matched constructor ID 17");
    17
}

fn match_node_Sreg_19(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 19: Terminal matched constructor ID 18");
    18
}

fn match_node_Sreg_20(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 20: Terminal matched constructor ID 19");
    19
}

fn match_node_Sreg_21(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 21: Terminal matched constructor ID 20");
    20
}

fn match_node_Sreg_22(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 22: Terminal matched constructor ID 21");
    21
}

fn match_node_Sreg_23(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 23: Terminal matched constructor ID 22");
    22
}

fn match_node_Sreg_24(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 24: Terminal matched constructor ID 23");
    23
}

fn match_node_Sreg_25(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 25: Terminal matched constructor ID 24");
    24
}

fn match_node_Sreg_26(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 26: Terminal matched constructor ID 25");
    25
}

fn match_node_Sreg_27(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 27: Terminal matched constructor ID 26");
    26
}

fn match_node_Sreg_28(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 28: Terminal matched constructor ID 27");
    27
}

fn match_node_Sreg_29(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 29: Terminal matched constructor ID 28");
    28
}

fn match_node_Sreg_30(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 30: Terminal matched constructor ID 29");
    29
}

fn match_node_Sreg_31(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 31: Terminal matched constructor ID 30");
    30
}

fn match_node_Sreg_32(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 32: Terminal matched constructor ID 31");
    31
}

fn match_node_StrType0_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_StrType0_1(bytes, ctx),
        1 => match_node_StrType0_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_StrType0_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_StrType0_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_StrType1_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_StrType1_1(bytes, ctx),
        1 => match_node_StrType1_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_StrType1_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_StrType1_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_StrType2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_StrType2_1(bytes, ctx),
        1 => match_node_StrType2_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_StrType2_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_StrType2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_StrType3_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_StrType3_1(bytes, ctx),
        1 => match_node_StrType3_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_StrType3_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_StrType3_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_StrType4_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_StrType4_1(bytes, ctx),
        1 => match_node_StrType4_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_StrType4_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_StrType4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_StrType5_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_StrType5_1(bytes, ctx),
        1 => match_node_StrType5_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_StrType5_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_StrType5_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_StrType6_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_StrType6_1(bytes, ctx),
        1 => match_node_StrType6_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_StrType6_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_StrType6_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_StrType7_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_StrType7_1(bytes, ctx),
        1 => match_node_StrType7_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_StrType7_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_StrType7_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_ThAddr20_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=5, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ThAddr20_1(bytes, ctx),
        1 => match_node_ThAddr20_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ThAddr20_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_ThAddr20_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_ThAddr24_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=5, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ThAddr24_1(bytes, ctx),
        1 => match_node_ThAddr24_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ThAddr24_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_ThAddr24_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_ThArmAddr23_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=5, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ThArmAddr23_1(bytes, ctx),
        1 => match_node_ThArmAddr23_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ThArmAddr23_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_ThArmAddr23_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_ThumbExpandImm12_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=5, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ThumbExpandImm12_1(bytes, ctx),
        1 => match_node_ThumbExpandImm12_6(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ThumbExpandImm12_1(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (18 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 18) & 3;
    eprintln!("Trace node 1: SlaInstructionBits start=18, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ThumbExpandImm12_2(bytes, ctx),
        1 => match_node_ThumbExpandImm12_3(bytes, ctx),
        2 => match_node_ThumbExpandImm12_4(bytes, ctx),
        3 => match_node_ThumbExpandImm12_5(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ThumbExpandImm12_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_ThumbExpandImm12_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 1");
    1
}

fn match_node_ThumbExpandImm12_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 2");
    2
}

fn match_node_ThumbExpandImm12_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 3");
    3
}

fn match_node_ThumbExpandImm12_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_VRd_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 0: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_VRd_1(bytes, ctx),
        1 => match_node_VRd_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_VRd_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_VRd_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_VRn_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 0: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_VRn_1(bytes, ctx),
        1 => match_node_VRn_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_VRn_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_VRn_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_X_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 3;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_X_1(bytes, ctx),
        1 => match_node_X_8(bytes, ctx),
        2 => match_node_X_15(bytes, ctx),
        3 => match_node_X_22(bytes, ctx),
        _ => -1,
    }
}

fn match_node_X_1(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 1: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_X_2(bytes, ctx),
        1 => match_node_X_7(bytes, ctx),
        _ => -1,
    }
}

fn match_node_X_2(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 2: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_X_3(bytes, ctx),
        1 => match_node_X_6(bytes, ctx),
        _ => -1,
    }
}

fn match_node_X_3(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 3: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_X_4(bytes, ctx),
        1 => match_node_X_5(bytes, ctx),
        _ => -1,
    }
}

fn match_node_X_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 2");
    2
}

fn match_node_X_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 0");
    0
}

fn match_node_X_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 0");
    0
}

fn match_node_X_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 0");
    0
}

fn match_node_X_8(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 8: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_X_9(bytes, ctx),
        1 => match_node_X_14(bytes, ctx),
        _ => -1,
    }
}

fn match_node_X_9(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 9: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_X_10(bytes, ctx),
        1 => match_node_X_13(bytes, ctx),
        _ => -1,
    }
}

fn match_node_X_10(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 10: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_X_11(bytes, ctx),
        1 => match_node_X_12(bytes, ctx),
        _ => -1,
    }
}

fn match_node_X_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 2");
    2
}

fn match_node_X_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 1");
    1
}

fn match_node_X_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 1");
    1
}

fn match_node_X_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 1");
    1
}

fn match_node_X_15(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 15: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_X_16(bytes, ctx),
        1 => match_node_X_21(bytes, ctx),
        _ => -1,
    }
}

fn match_node_X_16(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 16: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_X_17(bytes, ctx),
        1 => match_node_X_20(bytes, ctx),
        _ => -1,
    }
}

fn match_node_X_17(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 17: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_X_18(bytes, ctx),
        1 => match_node_X_19(bytes, ctx),
        _ => -1,
    }
}

fn match_node_X_18(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 18: Terminal matched constructor ID 2");
    2
}

fn match_node_X_19(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 19: Terminal matched constructor ID 1");
    1
}

fn match_node_X_20(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 20: Terminal matched constructor ID 1");
    1
}

fn match_node_X_21(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 21: Terminal matched constructor ID 1");
    1
}

fn match_node_X_22(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 22: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_X_23(bytes, ctx),
        1 => match_node_X_28(bytes, ctx),
        _ => -1,
    }
}

fn match_node_X_23(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 23: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_X_24(bytes, ctx),
        1 => match_node_X_27(bytes, ctx),
        _ => -1,
    }
}

fn match_node_X_24(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 24: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_X_25(bytes, ctx),
        1 => match_node_X_26(bytes, ctx),
        _ => -1,
    }
}

fn match_node_X_25(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 25: Terminal matched constructor ID 2");
    2
}

fn match_node_X_26(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 26: Terminal matched constructor ID 0");
    0
}

fn match_node_X_27(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 27: Terminal matched constructor ID 0");
    0
}

fn match_node_X_28(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 28: Terminal matched constructor ID 0");
    0
}

fn match_node_XBIT_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (26 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 26) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=26, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_XBIT_1(bytes, ctx),
        1 => match_node_XBIT_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_XBIT_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_XBIT_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_XYZ_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_Y_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_Y_1(bytes, ctx),
        1 => match_node_Y_12(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Y_1(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 1: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_Y_2(bytes, ctx),
        1 => match_node_Y_7(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Y_2(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 2: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_Y_3(bytes, ctx),
        1 => match_node_Y_6(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Y_3(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 3: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_Y_4(bytes, ctx),
        1 => match_node_Y_5(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Y_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 2");
    2
}

fn match_node_Y_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 0");
    0
}

fn match_node_Y_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 0");
    0
}

fn match_node_Y_7(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 7: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_Y_8(bytes, ctx),
        1 => match_node_Y_11(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Y_8(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 8: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_Y_9(bytes, ctx),
        1 => match_node_Y_10(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Y_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 2");
    2
}

fn match_node_Y_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 1");
    1
}

fn match_node_Y_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 1");
    1
}

fn match_node_Y_12(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 12: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_Y_13(bytes, ctx),
        1 => match_node_Y_18(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Y_13(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 13: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_Y_14(bytes, ctx),
        1 => match_node_Y_17(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Y_14(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 14: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_Y_15(bytes, ctx),
        1 => match_node_Y_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Y_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 2");
    2
}

fn match_node_Y_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 1");
    1
}

fn match_node_Y_17(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 17: Terminal matched constructor ID 1");
    1
}

fn match_node_Y_18(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 18: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_Y_19(bytes, ctx),
        1 => match_node_Y_22(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Y_19(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 19: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_Y_20(bytes, ctx),
        1 => match_node_Y_21(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Y_20(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 20: Terminal matched constructor ID 2");
    2
}

fn match_node_Y_21(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 21: Terminal matched constructor ID 0");
    0
}

fn match_node_Y_22(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 22: Terminal matched constructor ID 0");
    0
}

fn match_node_YBIT_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=25, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_YBIT_1(bytes, ctx),
        1 => match_node_YBIT_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_YBIT_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_YBIT_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_Z_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_Z_1(bytes, ctx),
        1 => match_node_Z_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Z_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_Z_2(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 2: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_Z_3(bytes, ctx),
        1 => match_node_Z_6(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Z_3(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 3: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_Z_4(bytes, ctx),
        1 => match_node_Z_5(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Z_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 0");
    0
}

fn match_node_Z_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 1");
    1
}

fn match_node_Z_6(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 6: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_Z_7(bytes, ctx),
        1 => match_node_Z_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_Z_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 1");
    1
}

fn match_node_Z_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 0");
    0
}

fn match_node_addr2shift_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 3;
    eprintln!("Trace node 0: SlaInstructionBits start=25, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_addr2shift_1(bytes, ctx),
        1 => match_node_addr2shift_2(bytes, ctx),
        2 => match_node_addr2shift_3(bytes, ctx),
        3 => match_node_addr2shift_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_addr2shift_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_addr2shift_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 3");
    3
}

fn match_node_addr2shift_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 5");
    5
}

fn match_node_addr2shift_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 7");
    7
}

fn match_node_addrmode2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 7;
    eprintln!("Trace node 0: SlaInstructionBits start=6, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_addrmode2_1(bytes, ctx),
        1 => match_node_addrmode2_4(bytes, ctx),
        2 => match_node_addrmode2_7(bytes, ctx),
        3 => match_node_addrmode2_10(bytes, ctx),
        4 => match_node_addrmode2_13(bytes, ctx),
        5 => match_node_addrmode2_16(bytes, ctx),
        6 => match_node_addrmode2_19(bytes, ctx),
        7 => match_node_addrmode2_22(bytes, ctx),
        _ => -1,
    }
}

fn match_node_addrmode2_1(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_addrmode2_2(bytes, ctx),
        1 => match_node_addrmode2_3(bytes, ctx),
        _ => -1,
    }
}

fn match_node_addrmode2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 11");
    11
}

fn match_node_addrmode2_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 15");
    15
}

fn match_node_addrmode2_4(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 4: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_addrmode2_5(bytes, ctx),
        1 => match_node_addrmode2_6(bytes, ctx),
        _ => -1,
    }
}

fn match_node_addrmode2_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 10");
    10
}

fn match_node_addrmode2_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 14");
    14
}

fn match_node_addrmode2_7(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 7: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_addrmode2_8(bytes, ctx),
        1 => match_node_addrmode2_9(bytes, ctx),
        _ => -1,
    }
}

fn match_node_addrmode2_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 1");
    1
}

fn match_node_addrmode2_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 7");
    7
}

fn match_node_addrmode2_10(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 10: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_addrmode2_11(bytes, ctx),
        1 => match_node_addrmode2_12(bytes, ctx),
        _ => -1,
    }
}

fn match_node_addrmode2_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 0");
    0
}

fn match_node_addrmode2_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 6");
    6
}

fn match_node_addrmode2_13(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 13: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_addrmode2_14(bytes, ctx),
        1 => match_node_addrmode2_15(bytes, ctx),
        _ => -1,
    }
}

fn match_node_addrmode2_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 13");
    13
}

fn match_node_addrmode2_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 17");
    17
}

fn match_node_addrmode2_16(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 16: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_addrmode2_17(bytes, ctx),
        1 => match_node_addrmode2_18(bytes, ctx),
        _ => -1,
    }
}

fn match_node_addrmode2_17(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 17: Terminal matched constructor ID 12");
    12
}

fn match_node_addrmode2_18(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 18: Terminal matched constructor ID 16");
    16
}

fn match_node_addrmode2_19(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 19: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_addrmode2_20(bytes, ctx),
        1 => match_node_addrmode2_21(bytes, ctx),
        _ => -1,
    }
}

fn match_node_addrmode2_20(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 20: Terminal matched constructor ID 5");
    5
}

fn match_node_addrmode2_21(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 21: Terminal matched constructor ID 9");
    9
}

fn match_node_addrmode2_22(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 22: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_addrmode2_23(bytes, ctx),
        1 => match_node_addrmode2_24(bytes, ctx),
        _ => -1,
    }
}

fn match_node_addrmode2_23(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 23: Terminal matched constructor ID 4");
    4
}

fn match_node_addrmode2_24(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 24: Terminal matched constructor ID 8");
    8
}

fn match_node_addrmode3_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 7;
    eprintln!("Trace node 0: SlaInstructionBits start=7, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_addrmode3_1(bytes, ctx),
        1 => match_node_addrmode3_2(bytes, ctx),
        2 => match_node_addrmode3_3(bytes, ctx),
        3 => match_node_addrmode3_4(bytes, ctx),
        4 => match_node_addrmode3_5(bytes, ctx),
        5 => match_node_addrmode3_8(bytes, ctx),
        6 => match_node_addrmode3_11(bytes, ctx),
        7 => match_node_addrmode3_14(bytes, ctx),
        _ => -1,
    }
}

fn match_node_addrmode3_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 13");
    13
}

fn match_node_addrmode3_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 11");
    11
}

fn match_node_addrmode3_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 12");
    12
}

fn match_node_addrmode3_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 10");
    10
}

fn match_node_addrmode3_5(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 5: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_addrmode3_6(bytes, ctx),
        1 => match_node_addrmode3_7(bytes, ctx),
        _ => -1,
    }
}

fn match_node_addrmode3_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_addrmode3_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 9");
    9
}

fn match_node_addrmode3_8(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 8: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_addrmode3_9(bytes, ctx),
        1 => match_node_addrmode3_10(bytes, ctx),
        _ => -1,
    }
}

fn match_node_addrmode3_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 1");
    1
}

fn match_node_addrmode3_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 7");
    7
}

fn match_node_addrmode3_11(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 11: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_addrmode3_12(bytes, ctx),
        1 => match_node_addrmode3_13(bytes, ctx),
        _ => -1,
    }
}

fn match_node_addrmode3_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 4");
    4
}

fn match_node_addrmode3_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 8");
    8
}

fn match_node_addrmode3_14(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 14: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_addrmode3_15(bytes, ctx),
        1 => match_node_addrmode3_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_addrmode3_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 0");
    0
}

fn match_node_addrmode3_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 6");
    6
}

fn match_node_addrmode5_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 3;
    eprintln!("Trace node 0: SlaInstructionBits start=7, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_addrmode5_1(bytes, ctx),
        1 => match_node_addrmode5_2(bytes, ctx),
        2 => match_node_addrmode5_5(bytes, ctx),
        3 => match_node_addrmode5_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_addrmode5_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 5");
    5
}

fn match_node_addrmode5_2(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 2: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_addrmode5_3(bytes, ctx),
        1 => match_node_addrmode5_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_addrmode5_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 6");
    6
}

fn match_node_addrmode5_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 4");
    4
}

fn match_node_addrmode5_5(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 5: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_addrmode5_6(bytes, ctx),
        1 => match_node_addrmode5_7(bytes, ctx),
        _ => -1,
    }
}

fn match_node_addrmode5_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 1");
    1
}

fn match_node_addrmode5_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 3");
    3
}

fn match_node_addrmode5_8(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 8: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_addrmode5_9(bytes, ctx),
        1 => match_node_addrmode5_10(bytes, ctx),
        _ => -1,
    }
}

fn match_node_addrmode5_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 0");
    0
}

fn match_node_addrmode5_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 2");
    2
}

fn match_node_aflag_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_aflag_1(bytes, ctx),
        1 => match_node_aflag_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_aflag_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_aflag_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_apsr_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_armEndianNess_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (22 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 22) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=22, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_armEndianNess_1(bytes, ctx),
        1 => match_node_armEndianNess_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_armEndianNess_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_armEndianNess_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_bitWidth_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_buildVldmDdList_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 18) & 1;
    eprintln!("Trace node 0: SlaContextBits start=18, size=1, probe={}", probe);
    match probe {
        0 => match_node_buildVldmDdList_1(bytes, ctx),
        1 => match_node_buildVldmDdList_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_buildVldmDdList_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_buildVldmDdList_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_buildVldmSdList_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 18) & 1;
    eprintln!("Trace node 0: SlaContextBits start=18, size=1, probe={}", probe);
    match probe {
        0 => match_node_buildVldmSdList_1(bytes, ctx),
        1 => match_node_buildVldmSdList_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_buildVldmSdList_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_buildVldmSdList_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_buildVpopSd64List_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_buildVpopSdList_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_buildVpushSd64List_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_buildVpushSdList_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_buildVstmDdList_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 18) & 1;
    eprintln!("Trace node 0: SlaContextBits start=18, size=1, probe={}", probe);
    match probe {
        0 => match_node_buildVstmDdList_1(bytes, ctx),
        1 => match_node_buildVstmDdList_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_buildVstmDdList_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_buildVstmDdList_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_buildVstmSdList_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 18) & 1;
    eprintln!("Trace node 0: SlaContextBits start=18, size=1, probe={}", probe);
    match probe {
        0 => match_node_buildVstmSdList_1(bytes, ctx),
        1 => match_node_buildVstmSdList_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_buildVstmSdList_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_buildVstmSdList_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_bxns_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 1");
    1
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

fn match_node_cpsrmask_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 15;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_cpsrmask_1(bytes, ctx),
        1 => match_node_cpsrmask_2(bytes, ctx),
        2 => match_node_cpsrmask_3(bytes, ctx),
        3 => match_node_cpsrmask_4(bytes, ctx),
        4 => match_node_cpsrmask_5(bytes, ctx),
        5 => match_node_cpsrmask_6(bytes, ctx),
        6 => match_node_cpsrmask_7(bytes, ctx),
        7 => match_node_cpsrmask_8(bytes, ctx),
        8 => match_node_cpsrmask_9(bytes, ctx),
        9 => match_node_cpsrmask_10(bytes, ctx),
        10 => match_node_cpsrmask_11(bytes, ctx),
        11 => match_node_cpsrmask_12(bytes, ctx),
        12 => match_node_cpsrmask_13(bytes, ctx),
        13 => match_node_cpsrmask_14(bytes, ctx),
        14 => match_node_cpsrmask_15(bytes, ctx),
        15 => match_node_cpsrmask_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_cpsrmask_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_cpsrmask_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_cpsrmask_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_cpsrmask_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_cpsrmask_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 4");
    4
}

fn match_node_cpsrmask_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_cpsrmask_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 6");
    6
}

fn match_node_cpsrmask_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 7");
    7
}

fn match_node_cpsrmask_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 8");
    8
}

fn match_node_cpsrmask_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 9");
    9
}

fn match_node_cpsrmask_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 10");
    10
}

fn match_node_cpsrmask_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 11");
    11
}

fn match_node_cpsrmask_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 12");
    12
}

fn match_node_cpsrmask_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 13");
    13
}

fn match_node_cpsrmask_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 14");
    14
}

fn match_node_cpsrmask_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 15");
    15
}

fn match_node_fflag_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_fflag_1(bytes, ctx),
        1 => match_node_fflag_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_fflag_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_fflag_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_iflag_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_iflag_1(bytes, ctx),
        1 => match_node_iflag_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_iflag_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_iflag_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_iflags_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_immed12_4_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 35) & 1;
    eprintln!("Trace node 0: SlaContextBits start=35, size=1, probe={}", probe);
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
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 2: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_instruction_3(bytes, ctx),
        1 => match_node_instruction_792(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_3(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 7;
    eprintln!("Trace node 3: SlaInstructionBits start=4, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_4(bytes, ctx),
        1 => match_node_instruction_159(bytes, ctx),
        2 => match_node_instruction_198(bytes, ctx),
        3 => match_node_instruction_213(bytes, ctx),
        4 => match_node_instruction_364(bytes, ctx),
        5 => match_node_instruction_405(bytes, ctx),
        6 => match_node_instruction_422(bytes, ctx),
        7 => match_node_instruction_495(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_4(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (27 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 27) & 1;
    eprintln!("Trace node 4: SlaInstructionBits start=27, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_5(bytes, ctx),
        1 => match_node_instruction_68(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_5(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 15;
    eprintln!("Trace node 5: SlaInstructionBits start=7, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_6(bytes, ctx),
        1 => match_node_instruction_7(bytes, ctx),
        2 => match_node_instruction_8(bytes, ctx),
        3 => match_node_instruction_9(bytes, ctx),
        4 => match_node_instruction_10(bytes, ctx),
        5 => match_node_instruction_11(bytes, ctx),
        6 => match_node_instruction_12(bytes, ctx),
        7 => match_node_instruction_13(bytes, ctx),
        8 => match_node_instruction_14(bytes, ctx),
        9 => match_node_instruction_35(bytes, ctx),
        10 => match_node_instruction_48(bytes, ctx),
        11 => match_node_instruction_53(bytes, ctx),
        12 => match_node_instruction_58(bytes, ctx),
        13 => match_node_instruction_59(bytes, ctx),
        14 => match_node_instruction_66(bytes, ctx),
        15 => match_node_instruction_67(bytes, ctx),
        _ => -1,
    }
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
    eprintln!("Trace node 10: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 0");
    0
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
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 14: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_15(bytes, ctx),
        1 => match_node_instruction_34(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_15(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 7;
    eprintln!("Trace node 15: SlaInstructionBits start=12, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_16(bytes, ctx),
        1 => match_node_instruction_21(bytes, ctx),
        2 => match_node_instruction_22(bytes, ctx),
        3 => match_node_instruction_23(bytes, ctx),
        4 => match_node_instruction_24(bytes, ctx),
        5 => match_node_instruction_27(bytes, ctx),
        6 => match_node_instruction_28(bytes, ctx),
        7 => match_node_instruction_31(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_16(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 1;
    eprintln!("Trace node 16: SlaInstructionBits start=24, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_17(bytes, ctx),
        1 => match_node_instruction_20(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_17(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (22 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 22) & 1;
    eprintln!("Trace node 17: SlaInstructionBits start=22, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_18(bytes, ctx),
        1 => match_node_instruction_19(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_18(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 18: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_19(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 19: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_20(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 20: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_21(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 21: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_22(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 22: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_23(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 23: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_24(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 34) & 1;
    eprintln!("Trace node 24: SlaContextBits start=34, size=1, probe={}", probe);
    match probe {
        0 => match_node_instruction_25(bytes, ctx),
        1 => match_node_instruction_26(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_25(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 25: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_26(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 26: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_27(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 27: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_28(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 34) & 1;
    eprintln!("Trace node 28: SlaContextBits start=34, size=1, probe={}", probe);
    match probe {
        0 => match_node_instruction_29(bytes, ctx),
        1 => match_node_instruction_30(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_29(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 29: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_30(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 30: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_31(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 1;
    eprintln!("Trace node 31: SlaInstructionBits start=24, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_32(bytes, ctx),
        1 => match_node_instruction_33(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_32(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 32: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_33(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 33: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_34(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 34: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_35(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 35: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_36(bytes, ctx),
        1 => match_node_instruction_45(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_36(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 1;
    eprintln!("Trace node 36: SlaInstructionBits start=24, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_37(bytes, ctx),
        1 => match_node_instruction_42(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_37(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 37: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_38(bytes, ctx),
        1 => match_node_instruction_39(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_38(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 38: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_39(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 2) & 1;
    eprintln!("Trace node 39: SlaContextBits start=2, size=1, probe={}", probe);
    match probe {
        0 => match_node_instruction_40(bytes, ctx),
        1 => match_node_instruction_41(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_40(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 40: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_41(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 41: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_42(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (26 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 26) & 1;
    eprintln!("Trace node 42: SlaInstructionBits start=26, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_43(bytes, ctx),
        1 => match_node_instruction_44(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_43(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 43: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_44(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 44: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_45(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (16 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 16) & 1;
    eprintln!("Trace node 45: SlaInstructionBits start=16, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_46(bytes, ctx),
        1 => match_node_instruction_47(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_46(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 46: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_47(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 47: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_48(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 48: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_49(bytes, ctx),
        1 => match_node_instruction_52(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_49(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 1;
    eprintln!("Trace node 49: SlaInstructionBits start=24, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_50(bytes, ctx),
        1 => match_node_instruction_51(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_50(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 50: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_51(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 51: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_52(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 52: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_53(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 53: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_54(bytes, ctx),
        1 => match_node_instruction_57(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_54(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (16 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 16) & 1;
    eprintln!("Trace node 54: SlaInstructionBits start=16, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_55(bytes, ctx),
        1 => match_node_instruction_56(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_55(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 55: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_56(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 56: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_57(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 57: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_58(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 58: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_59(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 34) & 1;
    eprintln!("Trace node 59: SlaContextBits start=34, size=1, probe={}", probe);
    match probe {
        0 => match_node_instruction_60(bytes, ctx),
        1 => match_node_instruction_61(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_60(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 60: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_61(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (19 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 19) & 1;
    eprintln!("Trace node 61: SlaInstructionBits start=19, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_62(bytes, ctx),
        1 => match_node_instruction_63(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_62(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 62: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_63(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 1) & 1;
    eprintln!("Trace node 63: SlaContextBits start=1, size=1, probe={}", probe);
    match probe {
        0 => match_node_instruction_64(bytes, ctx),
        1 => match_node_instruction_65(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_64(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 64: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_65(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 65: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_66(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 66: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_67(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 67: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_68(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 1;
    eprintln!("Trace node 68: SlaInstructionBits start=24, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_69(bytes, ctx),
        1 => match_node_instruction_118(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_69(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 15;
    eprintln!("Trace node 69: SlaInstructionBits start=7, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_70(bytes, ctx),
        1 => match_node_instruction_71(bytes, ctx),
        2 => match_node_instruction_72(bytes, ctx),
        3 => match_node_instruction_73(bytes, ctx),
        4 => match_node_instruction_74(bytes, ctx),
        5 => match_node_instruction_75(bytes, ctx),
        6 => match_node_instruction_76(bytes, ctx),
        7 => match_node_instruction_77(bytes, ctx),
        8 => match_node_instruction_78(bytes, ctx),
        9 => match_node_instruction_89(bytes, ctx),
        10 => match_node_instruction_102(bytes, ctx),
        11 => match_node_instruction_107(bytes, ctx),
        12 => match_node_instruction_114(bytes, ctx),
        13 => match_node_instruction_115(bytes, ctx),
        14 => match_node_instruction_116(bytes, ctx),
        15 => match_node_instruction_117(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_70(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 70: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_71(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 71: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_72(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 72: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_73(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 73: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_74(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 74: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_75(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 75: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_76(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 76: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_77(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 77: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_78(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 78: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_79(bytes, ctx),
        1 => match_node_instruction_88(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_79(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (26 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 26) & 1;
    eprintln!("Trace node 79: SlaInstructionBits start=26, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_80(bytes, ctx),
        1 => match_node_instruction_87(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_80(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 34) & 1;
    eprintln!("Trace node 80: SlaContextBits start=34, size=1, probe={}", probe);
    match probe {
        0 => match_node_instruction_81(bytes, ctx),
        1 => match_node_instruction_86(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_81(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 3;
    eprintln!("Trace node 81: SlaInstructionBits start=12, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_82(bytes, ctx),
        1 => match_node_instruction_83(bytes, ctx),
        2 => match_node_instruction_84(bytes, ctx),
        3 => match_node_instruction_85(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_82(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 82: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_83(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 83: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_84(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 84: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_85(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 85: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_86(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 86: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_87(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 87: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_88(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 88: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_89(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 89: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_90(bytes, ctx),
        1 => match_node_instruction_99(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_90(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 3;
    eprintln!("Trace node 90: SlaInstructionBits start=25, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_91(bytes, ctx),
        1 => match_node_instruction_96(bytes, ctx),
        2 => match_node_instruction_97(bytes, ctx),
        3 => match_node_instruction_98(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_91(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 2) & 1;
    eprintln!("Trace node 91: SlaContextBits start=2, size=1, probe={}", probe);
    match probe {
        0 => match_node_instruction_92(bytes, ctx),
        1 => match_node_instruction_95(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_92(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 1) & 1;
    eprintln!("Trace node 92: SlaContextBits start=1, size=1, probe={}", probe);
    match probe {
        0 => match_node_instruction_93(bytes, ctx),
        1 => match_node_instruction_94(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_93(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 93: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_94(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 94: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_95(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 95: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_96(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 96: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_97(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 97: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_98(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 98: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_99(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (16 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 16) & 1;
    eprintln!("Trace node 99: SlaInstructionBits start=16, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_100(bytes, ctx),
        1 => match_node_instruction_101(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_100(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 100: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_101(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 101: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_102(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 102: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_103(bytes, ctx),
        1 => match_node_instruction_106(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_103(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (26 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 26) & 1;
    eprintln!("Trace node 103: SlaInstructionBits start=26, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_104(bytes, ctx),
        1 => match_node_instruction_105(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_104(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 104: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_105(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 105: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_106(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 106: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_107(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 107: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_108(bytes, ctx),
        1 => match_node_instruction_113(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_108(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 3;
    eprintln!("Trace node 108: SlaInstructionBits start=25, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_109(bytes, ctx),
        1 => match_node_instruction_110(bytes, ctx),
        2 => match_node_instruction_111(bytes, ctx),
        3 => match_node_instruction_112(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_109(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 109: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_110(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 110: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_111(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 111: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_112(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 112: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_113(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 113: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_114(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 114: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_115(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 115: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_116(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 116: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_117(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 117: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_118(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (26 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 26) & 1;
    eprintln!("Trace node 118: SlaInstructionBits start=26, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_119(bytes, ctx),
        1 => match_node_instruction_152(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_119(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 34) & 1;
    eprintln!("Trace node 119: SlaContextBits start=34, size=1, probe={}", probe);
    match probe {
        0 => match_node_instruction_120(bytes, ctx),
        1 => match_node_instruction_123(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_120(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 120: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_121(bytes, ctx),
        1 => match_node_instruction_122(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_121(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 121: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_122(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 122: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_123(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 1;
    eprintln!("Trace node 123: SlaInstructionBits start=25, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_124(bytes, ctx),
        1 => match_node_instruction_149(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_124(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 15;
    eprintln!("Trace node 124: SlaInstructionBits start=7, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_125(bytes, ctx),
        1 => match_node_instruction_126(bytes, ctx),
        2 => match_node_instruction_127(bytes, ctx),
        3 => match_node_instruction_128(bytes, ctx),
        4 => match_node_instruction_129(bytes, ctx),
        5 => match_node_instruction_130(bytes, ctx),
        6 => match_node_instruction_131(bytes, ctx),
        7 => match_node_instruction_132(bytes, ctx),
        8 => match_node_instruction_133(bytes, ctx),
        9 => match_node_instruction_134(bytes, ctx),
        10 => match_node_instruction_135(bytes, ctx),
        11 => match_node_instruction_136(bytes, ctx),
        12 => match_node_instruction_137(bytes, ctx),
        13 => match_node_instruction_140(bytes, ctx),
        14 => match_node_instruction_143(bytes, ctx),
        15 => match_node_instruction_146(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_125(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 125: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_126(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 126: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_127(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 127: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_128(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 128: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_129(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 129: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_130(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 130: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_131(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 131: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_132(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 132: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_133(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 133: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_134(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 134: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_135(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 135: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_136(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 136: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_137(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 137: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_138(bytes, ctx),
        1 => match_node_instruction_139(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_138(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 138: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_139(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 139: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_140(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 140: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_141(bytes, ctx),
        1 => match_node_instruction_142(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_141(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 141: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_142(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 142: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_143(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 143: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_144(bytes, ctx),
        1 => match_node_instruction_145(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_144(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 144: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_145(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 145: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_146(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 146: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_147(bytes, ctx),
        1 => match_node_instruction_148(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_147(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 147: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_148(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 148: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_149(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 149: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_150(bytes, ctx),
        1 => match_node_instruction_151(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_150(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 150: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_151(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 151: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_152(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 152: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_153(bytes, ctx),
        1 => match_node_instruction_156(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_153(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 1;
    eprintln!("Trace node 153: SlaInstructionBits start=25, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_154(bytes, ctx),
        1 => match_node_instruction_155(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_154(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 154: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_155(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 155: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_156(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 1;
    eprintln!("Trace node 156: SlaInstructionBits start=25, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_157(bytes, ctx),
        1 => match_node_instruction_158(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_157(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 157: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_158(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 158: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_159(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 15;
    eprintln!("Trace node 159: SlaInstructionBits start=7, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_160(bytes, ctx),
        1 => match_node_instruction_161(bytes, ctx),
        2 => match_node_instruction_162(bytes, ctx),
        3 => match_node_instruction_165(bytes, ctx),
        4 => match_node_instruction_166(bytes, ctx),
        5 => match_node_instruction_167(bytes, ctx),
        6 => match_node_instruction_168(bytes, ctx),
        7 => match_node_instruction_169(bytes, ctx),
        8 => match_node_instruction_170(bytes, ctx),
        9 => match_node_instruction_173(bytes, ctx),
        10 => match_node_instruction_188(bytes, ctx),
        11 => match_node_instruction_191(bytes, ctx),
        12 => match_node_instruction_194(bytes, ctx),
        13 => match_node_instruction_195(bytes, ctx),
        14 => match_node_instruction_196(bytes, ctx),
        15 => match_node_instruction_197(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_160(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 160: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_161(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 161: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_162(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 162: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_163(bytes, ctx),
        1 => match_node_instruction_164(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_163(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 163: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_164(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 164: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_165(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 165: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_166(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 166: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_167(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 167: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_168(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 168: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_169(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 169: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_170(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 170: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_171(bytes, ctx),
        1 => match_node_instruction_172(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_171(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 171: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_172(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 172: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_173(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 173: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_174(bytes, ctx),
        1 => match_node_instruction_185(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_174(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 1;
    eprintln!("Trace node 174: SlaInstructionBits start=24, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_175(bytes, ctx),
        1 => match_node_instruction_184(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_175(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (29 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 29) & 7;
    eprintln!("Trace node 175: SlaInstructionBits start=29, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_176(bytes, ctx),
        1 => match_node_instruction_177(bytes, ctx),
        2 => match_node_instruction_178(bytes, ctx),
        3 => match_node_instruction_179(bytes, ctx),
        4 => match_node_instruction_180(bytes, ctx),
        5 => match_node_instruction_181(bytes, ctx),
        6 => match_node_instruction_182(bytes, ctx),
        7 => match_node_instruction_183(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_176(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 176: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_177(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 177: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_178(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 178: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_179(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 179: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_180(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 180: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_181(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 181: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_182(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 182: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_183(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 183: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_184(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 184: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_185(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (16 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 16) & 1;
    eprintln!("Trace node 185: SlaInstructionBits start=16, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_186(bytes, ctx),
        1 => match_node_instruction_187(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_186(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 186: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_187(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 187: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_188(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 188: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_189(bytes, ctx),
        1 => match_node_instruction_190(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_189(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 189: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_190(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 190: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_191(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 191: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_192(bytes, ctx),
        1 => match_node_instruction_193(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_192(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 192: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_193(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 193: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_194(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 194: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_195(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 195: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_196(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 196: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_197(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 197: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_198(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 198: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_199(bytes, ctx),
        1 => match_node_instruction_206(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_199(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 199: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_200(bytes, ctx),
        1 => match_node_instruction_201(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_200(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 200: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_201(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 201: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_202(bytes, ctx),
        1 => match_node_instruction_205(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_202(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 202: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_203(bytes, ctx),
        1 => match_node_instruction_204(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_203(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 203: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_204(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 204: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_205(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 205: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_206(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 206: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_207(bytes, ctx),
        1 => match_node_instruction_208(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_207(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 207: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_208(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 208: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_209(bytes, ctx),
        1 => match_node_instruction_210(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_209(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 209: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_210(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 210: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_211(bytes, ctx),
        1 => match_node_instruction_212(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_211(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 211: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_212(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 212: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_213(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 213: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_214(bytes, ctx),
        1 => match_node_instruction_285(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_214(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (27 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 27) & 1;
    eprintln!("Trace node 214: SlaInstructionBits start=27, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_215(bytes, ctx),
        1 => match_node_instruction_222(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_215(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 215: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_216(bytes, ctx),
        1 => match_node_instruction_217(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_216(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 216: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_217(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 217: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_218(bytes, ctx),
        1 => match_node_instruction_221(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_218(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 218: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_219(bytes, ctx),
        1 => match_node_instruction_220(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_219(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 219: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_220(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 220: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_221(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 221: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_222(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 15;
    eprintln!("Trace node 222: SlaInstructionBits start=7, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_223(bytes, ctx),
        1 => match_node_instruction_232(bytes, ctx),
        2 => match_node_instruction_253(bytes, ctx),
        3 => match_node_instruction_254(bytes, ctx),
        4 => match_node_instruction_255(bytes, ctx),
        5 => match_node_instruction_260(bytes, ctx),
        6 => match_node_instruction_271(bytes, ctx),
        7 => match_node_instruction_272(bytes, ctx),
        8 => match_node_instruction_273(bytes, ctx),
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

fn match_node_instruction_223(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 7;
    eprintln!("Trace node 223: SlaInstructionBits start=24, size=3, word={:08x}, probe={}", word, probe);
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
    eprintln!("Trace node 224: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_225(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 225: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_226(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 226: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_227(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 227: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_228(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 228: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_229(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 229: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_230(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 230: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_231(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 231: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_232(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 7;
    eprintln!("Trace node 232: SlaInstructionBits start=24, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_233(bytes, ctx),
        1 => match_node_instruction_236(bytes, ctx),
        2 => match_node_instruction_239(bytes, ctx),
        3 => match_node_instruction_242(bytes, ctx),
        4 => match_node_instruction_245(bytes, ctx),
        5 => match_node_instruction_248(bytes, ctx),
        6 => match_node_instruction_249(bytes, ctx),
        7 => match_node_instruction_250(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_233(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 233: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_234(bytes, ctx),
        1 => match_node_instruction_235(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_234(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 234: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_235(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 235: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_236(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 236: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_237(bytes, ctx),
        1 => match_node_instruction_238(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_237(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 237: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_238(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 238: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_239(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 239: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_240(bytes, ctx),
        1 => match_node_instruction_241(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_240(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 240: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_241(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 241: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_242(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 242: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_243(bytes, ctx),
        1 => match_node_instruction_244(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_243(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 243: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_244(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 244: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_245(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 245: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_246(bytes, ctx),
        1 => match_node_instruction_247(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_246(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 246: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_247(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 247: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_248(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 248: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_249(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 249: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_250(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 250: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_251(bytes, ctx),
        1 => match_node_instruction_252(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_251(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 251: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_252(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 252: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_253(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 253: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_254(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 254: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_255(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 3;
    eprintln!("Trace node 255: SlaInstructionBits start=25, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_256(bytes, ctx),
        1 => match_node_instruction_257(bytes, ctx),
        2 => match_node_instruction_258(bytes, ctx),
        3 => match_node_instruction_259(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_256(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 256: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_257(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 257: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_258(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 258: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_259(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 259: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_260(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (26 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 26) & 1;
    eprintln!("Trace node 260: SlaInstructionBits start=26, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_261(bytes, ctx),
        1 => match_node_instruction_262(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_261(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 261: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_262(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 3;
    eprintln!("Trace node 262: SlaInstructionBits start=24, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_263(bytes, ctx),
        1 => match_node_instruction_266(bytes, ctx),
        2 => match_node_instruction_269(bytes, ctx),
        3 => match_node_instruction_270(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_263(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 263: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_264(bytes, ctx),
        1 => match_node_instruction_265(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_264(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 264: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_265(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 265: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_266(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 266: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_267(bytes, ctx),
        1 => match_node_instruction_268(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_267(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 267: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_268(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 268: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_269(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 269: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_270(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 270: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_271(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 271: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_272(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 272: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_273(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 3;
    eprintln!("Trace node 273: SlaInstructionBits start=25, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_274(bytes, ctx),
        1 => match_node_instruction_275(bytes, ctx),
        2 => match_node_instruction_276(bytes, ctx),
        3 => match_node_instruction_277(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_274(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 274: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_275(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 275: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_276(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 276: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_277(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 277: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_278(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 278: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_279(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 279: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_280(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 280: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_281(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 281: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_282(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 282: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_283(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 283: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_284(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 284: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_285(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (27 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 27) & 1;
    eprintln!("Trace node 285: SlaInstructionBits start=27, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_286(bytes, ctx),
        1 => match_node_instruction_293(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_286(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 286: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_287(bytes, ctx),
        1 => match_node_instruction_290(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_287(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 287: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_288(bytes, ctx),
        1 => match_node_instruction_289(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_288(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 288: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_289(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 289: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_290(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 290: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_291(bytes, ctx),
        1 => match_node_instruction_292(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_291(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 291: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_292(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 292: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_293(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 15;
    eprintln!("Trace node 293: SlaInstructionBits start=7, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_294(bytes, ctx),
        1 => match_node_instruction_295(bytes, ctx),
        2 => match_node_instruction_296(bytes, ctx),
        3 => match_node_instruction_305(bytes, ctx),
        4 => match_node_instruction_326(bytes, ctx),
        5 => match_node_instruction_327(bytes, ctx),
        6 => match_node_instruction_328(bytes, ctx),
        7 => match_node_instruction_331(bytes, ctx),
        8 => match_node_instruction_342(bytes, ctx),
        9 => match_node_instruction_343(bytes, ctx),
        10 => match_node_instruction_344(bytes, ctx),
        11 => match_node_instruction_357(bytes, ctx),
        12 => match_node_instruction_358(bytes, ctx),
        13 => match_node_instruction_359(bytes, ctx),
        14 => match_node_instruction_360(bytes, ctx),
        15 => match_node_instruction_361(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_294(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 294: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_295(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 295: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_296(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 7;
    eprintln!("Trace node 296: SlaInstructionBits start=24, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_297(bytes, ctx),
        1 => match_node_instruction_298(bytes, ctx),
        2 => match_node_instruction_299(bytes, ctx),
        3 => match_node_instruction_300(bytes, ctx),
        4 => match_node_instruction_301(bytes, ctx),
        5 => match_node_instruction_302(bytes, ctx),
        6 => match_node_instruction_303(bytes, ctx),
        7 => match_node_instruction_304(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_297(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 297: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_298(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 298: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_299(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 299: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_300(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 300: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_301(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 301: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_302(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 302: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_303(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 303: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_304(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 304: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_305(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 7;
    eprintln!("Trace node 305: SlaInstructionBits start=24, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_306(bytes, ctx),
        1 => match_node_instruction_309(bytes, ctx),
        2 => match_node_instruction_312(bytes, ctx),
        3 => match_node_instruction_315(bytes, ctx),
        4 => match_node_instruction_318(bytes, ctx),
        5 => match_node_instruction_321(bytes, ctx),
        6 => match_node_instruction_322(bytes, ctx),
        7 => match_node_instruction_323(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_306(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 306: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_307(bytes, ctx),
        1 => match_node_instruction_308(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_307(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 307: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_308(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 308: Terminal matched constructor ID 0");
    0
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
    eprintln!("Trace node 310: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_311(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 311: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_312(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 312: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_313(bytes, ctx),
        1 => match_node_instruction_314(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_313(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 313: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_314(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 314: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_315(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 315: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_316(bytes, ctx),
        1 => match_node_instruction_317(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_316(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 316: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_317(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 317: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_318(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 318: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_319(bytes, ctx),
        1 => match_node_instruction_320(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_319(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 319: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_320(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 320: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_321(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 321: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_322(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 322: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_323(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 323: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_324(bytes, ctx),
        1 => match_node_instruction_325(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_324(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 324: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_325(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 325: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_326(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 326: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_327(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 327: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_328(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 328: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_329(bytes, ctx),
        1 => match_node_instruction_330(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_329(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 329: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_330(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 330: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_331(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (26 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 26) & 1;
    eprintln!("Trace node 331: SlaInstructionBits start=26, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_332(bytes, ctx),
        1 => match_node_instruction_333(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_332(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 332: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_333(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 3;
    eprintln!("Trace node 333: SlaInstructionBits start=24, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_334(bytes, ctx),
        1 => match_node_instruction_337(bytes, ctx),
        2 => match_node_instruction_340(bytes, ctx),
        3 => match_node_instruction_341(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_334(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 334: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_335(bytes, ctx),
        1 => match_node_instruction_336(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_335(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 335: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_336(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 336: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_337(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 337: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_338(bytes, ctx),
        1 => match_node_instruction_339(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_338(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 338: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_339(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 339: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_340(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 340: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_341(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 341: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_342(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 342: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_343(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 343: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_344(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 7;
    eprintln!("Trace node 344: SlaInstructionBits start=24, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_345(bytes, ctx),
        1 => match_node_instruction_348(bytes, ctx),
        2 => match_node_instruction_351(bytes, ctx),
        3 => match_node_instruction_352(bytes, ctx),
        4 => match_node_instruction_353(bytes, ctx),
        5 => match_node_instruction_354(bytes, ctx),
        6 => match_node_instruction_355(bytes, ctx),
        7 => match_node_instruction_356(bytes, ctx),
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
    eprintln!("Trace node 346: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_347(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 347: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_348(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 348: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_349(bytes, ctx),
        1 => match_node_instruction_350(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_349(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 349: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_350(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 350: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_351(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 351: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_352(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 352: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_353(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 353: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_354(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 354: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_355(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 355: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_356(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 356: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_357(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 357: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_358(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 358: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_359(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 359: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_360(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 360: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_361(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (26 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 26) & 1;
    eprintln!("Trace node 361: SlaInstructionBits start=26, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_362(bytes, ctx),
        1 => match_node_instruction_363(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_362(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 362: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_363(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 363: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_364(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 364: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_365(bytes, ctx),
        1 => match_node_instruction_384(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_365(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 34) & 1;
    eprintln!("Trace node 365: SlaContextBits start=34, size=1, probe={}", probe);
    match probe {
        0 => match_node_instruction_366(bytes, ctx),
        1 => match_node_instruction_383(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_366(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 15;
    eprintln!("Trace node 366: SlaInstructionBits start=7, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_367(bytes, ctx),
        1 => match_node_instruction_368(bytes, ctx),
        2 => match_node_instruction_369(bytes, ctx),
        3 => match_node_instruction_370(bytes, ctx),
        4 => match_node_instruction_371(bytes, ctx),
        5 => match_node_instruction_372(bytes, ctx),
        6 => match_node_instruction_373(bytes, ctx),
        7 => match_node_instruction_374(bytes, ctx),
        8 => match_node_instruction_375(bytes, ctx),
        9 => match_node_instruction_376(bytes, ctx),
        10 => match_node_instruction_377(bytes, ctx),
        11 => match_node_instruction_378(bytes, ctx),
        12 => match_node_instruction_379(bytes, ctx),
        13 => match_node_instruction_380(bytes, ctx),
        14 => match_node_instruction_381(bytes, ctx),
        15 => match_node_instruction_382(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_367(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 367: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_368(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 368: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_369(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 369: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_370(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 370: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_371(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 371: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_372(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 372: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_373(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 373: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_374(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 374: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_375(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 375: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_376(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 376: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_377(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 377: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_378(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 378: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_379(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 379: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_380(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 380: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_381(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 381: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_382(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 382: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_383(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 383: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_384(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 34) & 1;
    eprintln!("Trace node 384: SlaContextBits start=34, size=1, probe={}", probe);
    match probe {
        0 => match_node_instruction_385(bytes, ctx),
        1 => match_node_instruction_402(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_385(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 15;
    eprintln!("Trace node 385: SlaInstructionBits start=7, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_386(bytes, ctx),
        1 => match_node_instruction_387(bytes, ctx),
        2 => match_node_instruction_388(bytes, ctx),
        3 => match_node_instruction_389(bytes, ctx),
        4 => match_node_instruction_390(bytes, ctx),
        5 => match_node_instruction_391(bytes, ctx),
        6 => match_node_instruction_392(bytes, ctx),
        7 => match_node_instruction_393(bytes, ctx),
        8 => match_node_instruction_394(bytes, ctx),
        9 => match_node_instruction_395(bytes, ctx),
        10 => match_node_instruction_396(bytes, ctx),
        11 => match_node_instruction_397(bytes, ctx),
        12 => match_node_instruction_398(bytes, ctx),
        13 => match_node_instruction_399(bytes, ctx),
        14 => match_node_instruction_400(bytes, ctx),
        15 => match_node_instruction_401(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_386(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 386: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_387(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 387: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_388(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 388: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_389(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 389: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_390(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 390: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_391(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 391: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_392(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 392: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_393(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 393: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_394(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 394: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_395(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 395: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_396(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 396: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_397(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 397: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_398(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 398: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_399(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 399: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_400(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 400: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_401(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 401: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_402(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (16 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 16) & 1;
    eprintln!("Trace node 402: SlaInstructionBits start=16, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_403(bytes, ctx),
        1 => match_node_instruction_404(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_403(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 403: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_404(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 404: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_405(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 405: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_406(bytes, ctx),
        1 => match_node_instruction_411(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_406(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 1;
    eprintln!("Trace node 406: SlaInstructionBits start=3, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_407(bytes, ctx),
        1 => match_node_instruction_408(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_407(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 407: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_408(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 3) & 1;
    eprintln!("Trace node 408: SlaContextBits start=3, size=1, probe={}", probe);
    match probe {
        0 => match_node_instruction_409(bytes, ctx),
        1 => match_node_instruction_410(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_409(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 409: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_410(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 410: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_411(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 3) & 1;
    eprintln!("Trace node 411: SlaContextBits start=3, size=1, probe={}", probe);
    match probe {
        0 => match_node_instruction_412(bytes, ctx),
        1 => match_node_instruction_417(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_412(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 1;
    eprintln!("Trace node 412: SlaInstructionBits start=3, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_413(bytes, ctx),
        1 => match_node_instruction_414(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_413(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 413: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_414(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 34) & 1;
    eprintln!("Trace node 414: SlaContextBits start=34, size=1, probe={}", probe);
    match probe {
        0 => match_node_instruction_415(bytes, ctx),
        1 => match_node_instruction_416(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_415(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 415: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_416(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 416: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_417(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 1;
    eprintln!("Trace node 417: SlaInstructionBits start=3, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_418(bytes, ctx),
        1 => match_node_instruction_419(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_418(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 418: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_419(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 34) & 1;
    eprintln!("Trace node 419: SlaContextBits start=34, size=1, probe={}", probe);
    match probe {
        0 => match_node_instruction_420(bytes, ctx),
        1 => match_node_instruction_421(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_420(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 420: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_421(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 421: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_422(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 422: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_423(bytes, ctx),
        1 => match_node_instruction_454(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_423(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 34) & 1;
    eprintln!("Trace node 423: SlaContextBits start=34, size=1, probe={}", probe);
    match probe {
        0 => match_node_instruction_424(bytes, ctx),
        1 => match_node_instruction_427(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_424(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 424: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_425(bytes, ctx),
        1 => match_node_instruction_426(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_425(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 425: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_426(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 426: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_427(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 427: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_428(bytes, ctx),
        1 => match_node_instruction_441(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_428(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 428: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_429(bytes, ctx),
        1 => match_node_instruction_434(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_429(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 429: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_430(bytes, ctx),
        1 => match_node_instruction_431(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_430(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 430: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_431(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (23 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 23) & 1;
    eprintln!("Trace node 431: SlaInstructionBits start=23, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_432(bytes, ctx),
        1 => match_node_instruction_433(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_432(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 432: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_433(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 433: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_434(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 434: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_435(bytes, ctx),
        1 => match_node_instruction_438(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_435(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (23 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 23) & 1;
    eprintln!("Trace node 435: SlaInstructionBits start=23, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_436(bytes, ctx),
        1 => match_node_instruction_437(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_436(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 436: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_437(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 437: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_438(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (23 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 23) & 1;
    eprintln!("Trace node 438: SlaInstructionBits start=23, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_439(bytes, ctx),
        1 => match_node_instruction_440(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_439(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 439: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_440(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 440: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_441(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (23 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 23) & 1;
    eprintln!("Trace node 441: SlaInstructionBits start=23, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_442(bytes, ctx),
        1 => match_node_instruction_449(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_442(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 442: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_443(bytes, ctx),
        1 => match_node_instruction_446(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_443(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 443: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_444(bytes, ctx),
        1 => match_node_instruction_445(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_444(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 444: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_445(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 445: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_446(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 446: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_447(bytes, ctx),
        1 => match_node_instruction_448(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_447(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 447: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_448(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 448: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_449(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 449: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_450(bytes, ctx),
        1 => match_node_instruction_453(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_450(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 450: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_451(bytes, ctx),
        1 => match_node_instruction_452(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_451(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 451: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_452(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 452: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_453(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 453: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_454(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 34) & 1;
    eprintln!("Trace node 454: SlaContextBits start=34, size=1, probe={}", probe);
    match probe {
        0 => match_node_instruction_455(bytes, ctx),
        1 => match_node_instruction_468(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_455(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 3;
    eprintln!("Trace node 455: SlaInstructionBits start=7, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_456(bytes, ctx),
        1 => match_node_instruction_459(bytes, ctx),
        2 => match_node_instruction_462(bytes, ctx),
        3 => match_node_instruction_465(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_456(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 456: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_457(bytes, ctx),
        1 => match_node_instruction_458(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_457(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 457: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_458(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 458: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_459(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 459: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_460(bytes, ctx),
        1 => match_node_instruction_461(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_460(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 460: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_461(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 461: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_462(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 462: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_463(bytes, ctx),
        1 => match_node_instruction_464(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_463(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 463: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_464(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 464: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_465(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 465: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_466(bytes, ctx),
        1 => match_node_instruction_467(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_466(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 466: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_467(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 467: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_468(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 468: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_469(bytes, ctx),
        1 => match_node_instruction_482(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_469(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 469: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_470(bytes, ctx),
        1 => match_node_instruction_475(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_470(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 470: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_471(bytes, ctx),
        1 => match_node_instruction_472(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_471(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 471: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_472(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (23 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 23) & 1;
    eprintln!("Trace node 472: SlaInstructionBits start=23, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_473(bytes, ctx),
        1 => match_node_instruction_474(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_473(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 473: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_474(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 474: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_475(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (23 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 23) & 1;
    eprintln!("Trace node 475: SlaInstructionBits start=23, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_476(bytes, ctx),
        1 => match_node_instruction_479(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_476(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 476: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_477(bytes, ctx),
        1 => match_node_instruction_478(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_477(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 477: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_478(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 478: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_479(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 479: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_480(bytes, ctx),
        1 => match_node_instruction_481(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_480(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 480: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_481(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 481: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_482(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 482: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_483(bytes, ctx),
        1 => match_node_instruction_488(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_483(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 483: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_484(bytes, ctx),
        1 => match_node_instruction_487(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_484(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (23 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 23) & 1;
    eprintln!("Trace node 484: SlaInstructionBits start=23, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_485(bytes, ctx),
        1 => match_node_instruction_486(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_485(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 485: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_486(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 486: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_487(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 487: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_488(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 488: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_489(bytes, ctx),
        1 => match_node_instruction_492(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_489(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (23 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 23) & 1;
    eprintln!("Trace node 489: SlaInstructionBits start=23, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_490(bytes, ctx),
        1 => match_node_instruction_491(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_490(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 490: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_491(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 491: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_492(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (23 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 23) & 1;
    eprintln!("Trace node 492: SlaInstructionBits start=23, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_493(bytes, ctx),
        1 => match_node_instruction_494(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_493(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 493: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_494(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 494: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_495(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 34) & 1;
    eprintln!("Trace node 495: SlaContextBits start=34, size=1, probe={}", probe);
    match probe {
        0 => match_node_instruction_496(bytes, ctx),
        1 => match_node_instruction_501(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_496(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (27 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 27) & 1;
    eprintln!("Trace node 496: SlaInstructionBits start=27, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_497(bytes, ctx),
        1 => match_node_instruction_498(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_497(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 497: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_498(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 498: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_499(bytes, ctx),
        1 => match_node_instruction_500(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_499(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 499: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_500(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 500: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_501(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 501: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_502(bytes, ctx),
        1 => match_node_instruction_791(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_502(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (27 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 27) & 1;
    eprintln!("Trace node 502: SlaInstructionBits start=27, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_503(bytes, ctx),
        1 => match_node_instruction_608(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_503(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (22 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 22) & 3;
    eprintln!("Trace node 503: SlaInstructionBits start=22, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_504(bytes, ctx),
        1 => match_node_instruction_505(bytes, ctx),
        2 => match_node_instruction_532(bytes, ctx),
        3 => match_node_instruction_571(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_504(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 504: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_505(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 505: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_506(bytes, ctx),
        1 => match_node_instruction_507(bytes, ctx),
        2 => match_node_instruction_510(bytes, ctx),
        3 => match_node_instruction_511(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_506(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 506: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_507(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 1;
    eprintln!("Trace node 507: SlaInstructionBits start=25, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_508(bytes, ctx),
        1 => match_node_instruction_509(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_508(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 508: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_509(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 509: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_510(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 510: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_511(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 511: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_512(bytes, ctx),
        1 => match_node_instruction_513(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_512(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 512: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_513(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 15;
    eprintln!("Trace node 513: SlaInstructionBits start=12, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_514(bytes, ctx),
        1 => match_node_instruction_515(bytes, ctx),
        2 => match_node_instruction_516(bytes, ctx),
        3 => match_node_instruction_517(bytes, ctx),
        4 => match_node_instruction_518(bytes, ctx),
        5 => match_node_instruction_519(bytes, ctx),
        6 => match_node_instruction_520(bytes, ctx),
        7 => match_node_instruction_521(bytes, ctx),
        8 => match_node_instruction_522(bytes, ctx),
        9 => match_node_instruction_525(bytes, ctx),
        10 => match_node_instruction_526(bytes, ctx),
        11 => match_node_instruction_527(bytes, ctx),
        12 => match_node_instruction_528(bytes, ctx),
        13 => match_node_instruction_529(bytes, ctx),
        14 => match_node_instruction_530(bytes, ctx),
        15 => match_node_instruction_531(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_514(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 514: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_515(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 515: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_516(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 516: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_517(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 517: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_518(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 518: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_519(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 519: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_520(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 520: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_521(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 521: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_522(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 1;
    eprintln!("Trace node 522: SlaInstructionBits start=24, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_523(bytes, ctx),
        1 => match_node_instruction_524(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_523(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 523: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_524(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 524: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_525(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 525: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_526(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 526: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_527(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 527: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_528(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 528: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_529(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 529: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_530(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 530: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_531(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 531: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_532(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 532: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_533(bytes, ctx),
        1 => match_node_instruction_538(bytes, ctx),
        2 => match_node_instruction_541(bytes, ctx),
        3 => match_node_instruction_544(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_533(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 533: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_534(bytes, ctx),
        1 => match_node_instruction_537(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_534(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 1;
    eprintln!("Trace node 534: SlaInstructionBits start=25, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_535(bytes, ctx),
        1 => match_node_instruction_536(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_535(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 535: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_536(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 536: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_537(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 537: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_538(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 1;
    eprintln!("Trace node 538: SlaInstructionBits start=25, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_539(bytes, ctx),
        1 => match_node_instruction_540(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_539(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 539: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_540(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 540: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_541(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 1;
    eprintln!("Trace node 541: SlaInstructionBits start=25, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_542(bytes, ctx),
        1 => match_node_instruction_543(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_542(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 542: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_543(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 543: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_544(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 544: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_545(bytes, ctx),
        1 => match_node_instruction_548(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_545(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 1;
    eprintln!("Trace node 545: SlaInstructionBits start=25, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_546(bytes, ctx),
        1 => match_node_instruction_547(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_546(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 546: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_547(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 547: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_548(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 15;
    eprintln!("Trace node 548: SlaInstructionBits start=12, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_549(bytes, ctx),
        1 => match_node_instruction_552(bytes, ctx),
        2 => match_node_instruction_555(bytes, ctx),
        3 => match_node_instruction_556(bytes, ctx),
        4 => match_node_instruction_557(bytes, ctx),
        5 => match_node_instruction_558(bytes, ctx),
        6 => match_node_instruction_559(bytes, ctx),
        7 => match_node_instruction_560(bytes, ctx),
        8 => match_node_instruction_561(bytes, ctx),
        9 => match_node_instruction_564(bytes, ctx),
        10 => match_node_instruction_565(bytes, ctx),
        11 => match_node_instruction_566(bytes, ctx),
        12 => match_node_instruction_567(bytes, ctx),
        13 => match_node_instruction_568(bytes, ctx),
        14 => match_node_instruction_569(bytes, ctx),
        15 => match_node_instruction_570(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_549(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 1;
    eprintln!("Trace node 549: SlaInstructionBits start=24, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_550(bytes, ctx),
        1 => match_node_instruction_551(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_550(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 550: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_551(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 551: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_552(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 1;
    eprintln!("Trace node 552: SlaInstructionBits start=24, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_553(bytes, ctx),
        1 => match_node_instruction_554(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_553(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 553: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_554(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 554: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_555(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 555: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_556(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 556: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_557(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 557: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_558(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 558: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_559(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 559: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_560(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 560: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_561(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 1;
    eprintln!("Trace node 561: SlaInstructionBits start=24, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_562(bytes, ctx),
        1 => match_node_instruction_563(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_562(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 562: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_563(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 563: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_564(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 564: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_565(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 565: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_566(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 566: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_567(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 567: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_568(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 568: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_569(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 569: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_570(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 570: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_571(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 571: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_572(bytes, ctx),
        1 => match_node_instruction_577(bytes, ctx),
        2 => match_node_instruction_580(bytes, ctx),
        3 => match_node_instruction_583(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_572(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 572: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_573(bytes, ctx),
        1 => match_node_instruction_576(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_573(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 1;
    eprintln!("Trace node 573: SlaInstructionBits start=25, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_574(bytes, ctx),
        1 => match_node_instruction_575(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_574(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 574: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_575(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 575: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_576(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 576: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_577(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 1;
    eprintln!("Trace node 577: SlaInstructionBits start=25, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_578(bytes, ctx),
        1 => match_node_instruction_579(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_578(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 578: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_579(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 579: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_580(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 1;
    eprintln!("Trace node 580: SlaInstructionBits start=25, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_581(bytes, ctx),
        1 => match_node_instruction_582(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_581(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 581: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_582(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 582: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_583(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 583: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_584(bytes, ctx),
        1 => match_node_instruction_587(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_584(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 1;
    eprintln!("Trace node 584: SlaInstructionBits start=25, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_585(bytes, ctx),
        1 => match_node_instruction_586(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_585(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 585: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_586(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 586: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_587(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 15;
    eprintln!("Trace node 587: SlaInstructionBits start=12, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_588(bytes, ctx),
        1 => match_node_instruction_591(bytes, ctx),
        2 => match_node_instruction_592(bytes, ctx),
        3 => match_node_instruction_593(bytes, ctx),
        4 => match_node_instruction_594(bytes, ctx),
        5 => match_node_instruction_595(bytes, ctx),
        6 => match_node_instruction_596(bytes, ctx),
        7 => match_node_instruction_597(bytes, ctx),
        8 => match_node_instruction_598(bytes, ctx),
        9 => match_node_instruction_601(bytes, ctx),
        10 => match_node_instruction_602(bytes, ctx),
        11 => match_node_instruction_603(bytes, ctx),
        12 => match_node_instruction_604(bytes, ctx),
        13 => match_node_instruction_605(bytes, ctx),
        14 => match_node_instruction_606(bytes, ctx),
        15 => match_node_instruction_607(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_588(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 1;
    eprintln!("Trace node 588: SlaInstructionBits start=24, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_589(bytes, ctx),
        1 => match_node_instruction_590(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_589(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 589: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_590(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 590: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_591(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 591: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_592(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 592: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_593(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 593: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_594(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 594: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_595(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 595: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_596(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 596: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_597(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 597: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_598(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 1;
    eprintln!("Trace node 598: SlaInstructionBits start=24, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_599(bytes, ctx),
        1 => match_node_instruction_600(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_599(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 599: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_600(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 600: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_601(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 601: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_602(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 602: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_603(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 603: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_604(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 604: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_605(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 605: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_606(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 606: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_607(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 607: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_608(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 608: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_609(bytes, ctx),
        1 => match_node_instruction_700(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_609(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (21 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 21) & 1;
    eprintln!("Trace node 609: SlaInstructionBits start=21, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_610(bytes, ctx),
        1 => match_node_instruction_613(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_610(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 610: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_611(bytes, ctx),
        1 => match_node_instruction_612(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_611(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 611: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_612(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 612: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_613(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 15;
    eprintln!("Trace node 613: SlaInstructionBits start=12, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_614(bytes, ctx),
        1 => match_node_instruction_625(bytes, ctx),
        2 => match_node_instruction_636(bytes, ctx),
        3 => match_node_instruction_641(bytes, ctx),
        4 => match_node_instruction_642(bytes, ctx),
        5 => match_node_instruction_643(bytes, ctx),
        6 => match_node_instruction_646(bytes, ctx),
        7 => match_node_instruction_649(bytes, ctx),
        8 => match_node_instruction_680(bytes, ctx),
        9 => match_node_instruction_685(bytes, ctx),
        10 => match_node_instruction_686(bytes, ctx),
        11 => match_node_instruction_687(bytes, ctx),
        12 => match_node_instruction_688(bytes, ctx),
        13 => match_node_instruction_689(bytes, ctx),
        14 => match_node_instruction_698(bytes, ctx),
        15 => match_node_instruction_699(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_614(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 614: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_615(bytes, ctx),
        1 => match_node_instruction_624(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_615(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (30 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 30) & 3;
    eprintln!("Trace node 615: SlaInstructionBits start=30, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_616(bytes, ctx),
        1 => match_node_instruction_621(bytes, ctx),
        2 => match_node_instruction_622(bytes, ctx),
        3 => match_node_instruction_623(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_616(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 3;
    eprintln!("Trace node 616: SlaInstructionBits start=25, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_617(bytes, ctx),
        1 => match_node_instruction_618(bytes, ctx),
        2 => match_node_instruction_619(bytes, ctx),
        3 => match_node_instruction_620(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_617(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 617: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_618(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 618: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_619(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 619: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_620(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 620: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_621(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 621: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_622(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 622: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_623(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 623: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_624(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 624: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_625(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (31 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 31) & 1;
    eprintln!("Trace node 625: SlaInstructionBits start=31, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_626(bytes, ctx),
        1 => match_node_instruction_631(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_626(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 3;
    eprintln!("Trace node 626: SlaInstructionBits start=25, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_627(bytes, ctx),
        1 => match_node_instruction_628(bytes, ctx),
        2 => match_node_instruction_629(bytes, ctx),
        3 => match_node_instruction_630(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_627(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 627: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_628(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 628: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_629(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 629: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_630(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 630: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_631(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 3;
    eprintln!("Trace node 631: SlaInstructionBits start=25, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_632(bytes, ctx),
        1 => match_node_instruction_633(bytes, ctx),
        2 => match_node_instruction_634(bytes, ctx),
        3 => match_node_instruction_635(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_632(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 632: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_633(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 633: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_634(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 634: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_635(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 635: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_636(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 3;
    eprintln!("Trace node 636: SlaInstructionBits start=25, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_637(bytes, ctx),
        1 => match_node_instruction_638(bytes, ctx),
        2 => match_node_instruction_639(bytes, ctx),
        3 => match_node_instruction_640(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_637(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 637: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_638(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 638: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_639(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 639: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_640(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 640: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_641(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 641: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_642(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 642: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_643(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (26 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 26) & 1;
    eprintln!("Trace node 643: SlaInstructionBits start=26, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_644(bytes, ctx),
        1 => match_node_instruction_645(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_644(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 644: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_645(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 645: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_646(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 1;
    eprintln!("Trace node 646: SlaInstructionBits start=25, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_647(bytes, ctx),
        1 => match_node_instruction_648(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_647(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 647: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_648(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 648: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_649(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (28 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 28) & 7;
    eprintln!("Trace node 649: SlaInstructionBits start=28, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_650(bytes, ctx),
        1 => match_node_instruction_651(bytes, ctx),
        2 => match_node_instruction_652(bytes, ctx),
        3 => match_node_instruction_661(bytes, ctx),
        4 => match_node_instruction_666(bytes, ctx),
        5 => match_node_instruction_667(bytes, ctx),
        6 => match_node_instruction_676(bytes, ctx),
        7 => match_node_instruction_677(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_650(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 650: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_651(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 651: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_652(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 7;
    eprintln!("Trace node 652: SlaInstructionBits start=24, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_653(bytes, ctx),
        1 => match_node_instruction_654(bytes, ctx),
        2 => match_node_instruction_655(bytes, ctx),
        3 => match_node_instruction_656(bytes, ctx),
        4 => match_node_instruction_657(bytes, ctx),
        5 => match_node_instruction_658(bytes, ctx),
        6 => match_node_instruction_659(bytes, ctx),
        7 => match_node_instruction_660(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_653(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 653: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_654(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 654: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_655(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 655: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_656(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 656: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_657(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 657: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_658(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 658: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_659(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 659: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_660(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 660: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_661(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 3;
    eprintln!("Trace node 661: SlaInstructionBits start=25, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_662(bytes, ctx),
        1 => match_node_instruction_663(bytes, ctx),
        2 => match_node_instruction_664(bytes, ctx),
        3 => match_node_instruction_665(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_662(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 662: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_663(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 663: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_664(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 664: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_665(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 665: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_666(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 666: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_667(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 7;
    eprintln!("Trace node 667: SlaInstructionBits start=24, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_668(bytes, ctx),
        1 => match_node_instruction_669(bytes, ctx),
        2 => match_node_instruction_670(bytes, ctx),
        3 => match_node_instruction_671(bytes, ctx),
        4 => match_node_instruction_672(bytes, ctx),
        5 => match_node_instruction_673(bytes, ctx),
        6 => match_node_instruction_674(bytes, ctx),
        7 => match_node_instruction_675(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_668(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 668: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_669(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 669: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_670(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 670: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_671(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 671: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_672(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 672: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_673(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 673: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_674(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 674: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_675(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 675: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_676(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 676: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_677(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (26 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 26) & 1;
    eprintln!("Trace node 677: SlaInstructionBits start=26, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_678(bytes, ctx),
        1 => match_node_instruction_679(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_678(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 678: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_679(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 679: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_680(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 3;
    eprintln!("Trace node 680: SlaInstructionBits start=25, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_681(bytes, ctx),
        1 => match_node_instruction_682(bytes, ctx),
        2 => match_node_instruction_683(bytes, ctx),
        3 => match_node_instruction_684(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_681(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 681: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_682(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 682: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_683(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 683: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_684(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 684: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_685(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 685: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_686(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 686: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_687(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 687: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_688(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 688: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_689(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 7;
    eprintln!("Trace node 689: SlaInstructionBits start=24, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_690(bytes, ctx),
        1 => match_node_instruction_691(bytes, ctx),
        2 => match_node_instruction_692(bytes, ctx),
        3 => match_node_instruction_693(bytes, ctx),
        4 => match_node_instruction_694(bytes, ctx),
        5 => match_node_instruction_695(bytes, ctx),
        6 => match_node_instruction_696(bytes, ctx),
        7 => match_node_instruction_697(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_690(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 690: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_691(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 691: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_692(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 692: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_693(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 693: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_694(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 694: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_695(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 695: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_696(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 696: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_697(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 697: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_698(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 698: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_699(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 699: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_700(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (21 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 21) & 1;
    eprintln!("Trace node 700: SlaInstructionBits start=21, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_701(bytes, ctx),
        1 => match_node_instruction_704(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_701(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 701: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_702(bytes, ctx),
        1 => match_node_instruction_703(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_702(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 702: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_703(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 703: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_704(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 15;
    eprintln!("Trace node 704: SlaInstructionBits start=12, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_705(bytes, ctx),
        1 => match_node_instruction_716(bytes, ctx),
        2 => match_node_instruction_727(bytes, ctx),
        3 => match_node_instruction_732(bytes, ctx),
        4 => match_node_instruction_733(bytes, ctx),
        5 => match_node_instruction_734(bytes, ctx),
        6 => match_node_instruction_737(bytes, ctx),
        7 => match_node_instruction_740(bytes, ctx),
        8 => match_node_instruction_771(bytes, ctx),
        9 => match_node_instruction_776(bytes, ctx),
        10 => match_node_instruction_777(bytes, ctx),
        11 => match_node_instruction_778(bytes, ctx),
        12 => match_node_instruction_779(bytes, ctx),
        13 => match_node_instruction_780(bytes, ctx),
        14 => match_node_instruction_789(bytes, ctx),
        15 => match_node_instruction_790(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_705(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 705: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_706(bytes, ctx),
        1 => match_node_instruction_715(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_706(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (30 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 30) & 3;
    eprintln!("Trace node 706: SlaInstructionBits start=30, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_707(bytes, ctx),
        1 => match_node_instruction_712(bytes, ctx),
        2 => match_node_instruction_713(bytes, ctx),
        3 => match_node_instruction_714(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_707(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 3;
    eprintln!("Trace node 707: SlaInstructionBits start=25, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_708(bytes, ctx),
        1 => match_node_instruction_709(bytes, ctx),
        2 => match_node_instruction_710(bytes, ctx),
        3 => match_node_instruction_711(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_708(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 708: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_709(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 709: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_710(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 710: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_711(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 711: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_712(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 712: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_713(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 713: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_714(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 714: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_715(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 715: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_716(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (31 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 31) & 1;
    eprintln!("Trace node 716: SlaInstructionBits start=31, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_717(bytes, ctx),
        1 => match_node_instruction_722(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_717(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 3;
    eprintln!("Trace node 717: SlaInstructionBits start=25, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_718(bytes, ctx),
        1 => match_node_instruction_719(bytes, ctx),
        2 => match_node_instruction_720(bytes, ctx),
        3 => match_node_instruction_721(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_718(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 718: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_719(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 719: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_720(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 720: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_721(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 721: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_722(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 3;
    eprintln!("Trace node 722: SlaInstructionBits start=25, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_723(bytes, ctx),
        1 => match_node_instruction_724(bytes, ctx),
        2 => match_node_instruction_725(bytes, ctx),
        3 => match_node_instruction_726(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_723(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 723: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_724(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 724: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_725(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 725: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_726(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 726: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_727(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 3;
    eprintln!("Trace node 727: SlaInstructionBits start=25, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_728(bytes, ctx),
        1 => match_node_instruction_729(bytes, ctx),
        2 => match_node_instruction_730(bytes, ctx),
        3 => match_node_instruction_731(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_728(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 728: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_729(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 729: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_730(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 730: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_731(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 731: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_732(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 732: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_733(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 733: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_734(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (26 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 26) & 1;
    eprintln!("Trace node 734: SlaInstructionBits start=26, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_735(bytes, ctx),
        1 => match_node_instruction_736(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_735(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 735: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_736(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 736: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_737(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 1;
    eprintln!("Trace node 737: SlaInstructionBits start=25, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_738(bytes, ctx),
        1 => match_node_instruction_739(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_738(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 738: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_739(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 739: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_740(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (28 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 28) & 7;
    eprintln!("Trace node 740: SlaInstructionBits start=28, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_741(bytes, ctx),
        1 => match_node_instruction_742(bytes, ctx),
        2 => match_node_instruction_743(bytes, ctx),
        3 => match_node_instruction_752(bytes, ctx),
        4 => match_node_instruction_757(bytes, ctx),
        5 => match_node_instruction_758(bytes, ctx),
        6 => match_node_instruction_767(bytes, ctx),
        7 => match_node_instruction_768(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_741(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 741: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_742(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 742: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_743(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 7;
    eprintln!("Trace node 743: SlaInstructionBits start=24, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_744(bytes, ctx),
        1 => match_node_instruction_745(bytes, ctx),
        2 => match_node_instruction_746(bytes, ctx),
        3 => match_node_instruction_747(bytes, ctx),
        4 => match_node_instruction_748(bytes, ctx),
        5 => match_node_instruction_749(bytes, ctx),
        6 => match_node_instruction_750(bytes, ctx),
        7 => match_node_instruction_751(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_744(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 744: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_745(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 745: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_746(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 746: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_747(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 747: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_748(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 748: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_749(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 749: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_750(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 750: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_751(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 751: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_752(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 3;
    eprintln!("Trace node 752: SlaInstructionBits start=25, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_753(bytes, ctx),
        1 => match_node_instruction_754(bytes, ctx),
        2 => match_node_instruction_755(bytes, ctx),
        3 => match_node_instruction_756(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_753(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 753: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_754(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 754: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_755(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 755: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_756(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 756: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_757(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 757: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_758(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 7;
    eprintln!("Trace node 758: SlaInstructionBits start=24, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_759(bytes, ctx),
        1 => match_node_instruction_760(bytes, ctx),
        2 => match_node_instruction_761(bytes, ctx),
        3 => match_node_instruction_762(bytes, ctx),
        4 => match_node_instruction_763(bytes, ctx),
        5 => match_node_instruction_764(bytes, ctx),
        6 => match_node_instruction_765(bytes, ctx),
        7 => match_node_instruction_766(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_759(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 759: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_760(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 760: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_761(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 761: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_762(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 762: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_763(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 763: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_764(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 764: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_765(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 765: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_766(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 766: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_767(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 767: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_768(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (26 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 26) & 1;
    eprintln!("Trace node 768: SlaInstructionBits start=26, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_769(bytes, ctx),
        1 => match_node_instruction_770(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_769(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 769: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_770(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 770: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_771(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 3;
    eprintln!("Trace node 771: SlaInstructionBits start=25, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_772(bytes, ctx),
        1 => match_node_instruction_773(bytes, ctx),
        2 => match_node_instruction_774(bytes, ctx),
        3 => match_node_instruction_775(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_772(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 772: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_773(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 773: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_774(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 774: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_775(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 775: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_776(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 776: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_777(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 777: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_778(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 778: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_779(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 779: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_780(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 7;
    eprintln!("Trace node 780: SlaInstructionBits start=24, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_781(bytes, ctx),
        1 => match_node_instruction_782(bytes, ctx),
        2 => match_node_instruction_783(bytes, ctx),
        3 => match_node_instruction_784(bytes, ctx),
        4 => match_node_instruction_785(bytes, ctx),
        5 => match_node_instruction_786(bytes, ctx),
        6 => match_node_instruction_787(bytes, ctx),
        7 => match_node_instruction_788(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_781(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 781: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_782(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 782: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_783(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 783: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_784(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 784: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_785(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 785: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_786(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 786: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_787(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 787: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_788(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 788: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_789(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 789: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_790(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 790: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_791(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 791: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_792(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 15;
    eprintln!("Trace node 792: SlaInstructionBits start=0, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_793(bytes, ctx),
        1 => match_node_instruction_796(bytes, ctx),
        2 => match_node_instruction_803(bytes, ctx),
        3 => match_node_instruction_806(bytes, ctx),
        4 => match_node_instruction_809(bytes, ctx),
        5 => match_node_instruction_842(bytes, ctx),
        6 => match_node_instruction_851(bytes, ctx),
        7 => match_node_instruction_854(bytes, ctx),
        8 => match_node_instruction_857(bytes, ctx),
        9 => match_node_instruction_860(bytes, ctx),
        10 => match_node_instruction_863(bytes, ctx),
        11 => match_node_instruction_866(bytes, ctx),
        12 => match_node_instruction_915(bytes, ctx),
        13 => match_node_instruction_918(bytes, ctx),
        14 => match_node_instruction_921(bytes, ctx),
        15 => match_node_instruction_1170(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_793(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 793: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_794(bytes, ctx),
        1 => match_node_instruction_795(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_794(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 794: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_795(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 795: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_796(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 796: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_797(bytes, ctx),
        1 => match_node_instruction_798(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_797(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 797: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_798(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 3;
    eprintln!("Trace node 798: SlaInstructionBits start=5, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_799(bytes, ctx),
        1 => match_node_instruction_800(bytes, ctx),
        2 => match_node_instruction_801(bytes, ctx),
        3 => match_node_instruction_802(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_799(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 799: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_800(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 800: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_801(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 801: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_802(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 802: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_803(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 803: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_804(bytes, ctx),
        1 => match_node_instruction_805(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_804(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 804: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_805(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 805: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_806(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 806: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_807(bytes, ctx),
        1 => match_node_instruction_808(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_807(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 807: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_808(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 808: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_809(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 809: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_810(bytes, ctx),
        1 => match_node_instruction_841(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_810(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 7;
    eprintln!("Trace node 810: SlaInstructionBits start=5, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_811(bytes, ctx),
        1 => match_node_instruction_816(bytes, ctx),
        2 => match_node_instruction_821(bytes, ctx),
        3 => match_node_instruction_826(bytes, ctx),
        4 => match_node_instruction_831(bytes, ctx),
        5 => match_node_instruction_832(bytes, ctx),
        6 => match_node_instruction_833(bytes, ctx),
        7 => match_node_instruction_838(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_811(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 3;
    eprintln!("Trace node 811: SlaInstructionBits start=8, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_812(bytes, ctx),
        1 => match_node_instruction_813(bytes, ctx),
        2 => match_node_instruction_814(bytes, ctx),
        3 => match_node_instruction_815(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_812(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 812: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_813(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 813: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_814(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 814: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_815(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 815: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_816(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 3;
    eprintln!("Trace node 816: SlaInstructionBits start=8, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_817(bytes, ctx),
        1 => match_node_instruction_818(bytes, ctx),
        2 => match_node_instruction_819(bytes, ctx),
        3 => match_node_instruction_820(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_817(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 817: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_818(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 818: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_819(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 819: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_820(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 820: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_821(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 3;
    eprintln!("Trace node 821: SlaInstructionBits start=8, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_822(bytes, ctx),
        1 => match_node_instruction_823(bytes, ctx),
        2 => match_node_instruction_824(bytes, ctx),
        3 => match_node_instruction_825(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_822(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 822: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_823(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 823: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_824(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 824: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_825(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 825: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_826(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 3;
    eprintln!("Trace node 826: SlaInstructionBits start=8, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_827(bytes, ctx),
        1 => match_node_instruction_828(bytes, ctx),
        2 => match_node_instruction_829(bytes, ctx),
        3 => match_node_instruction_830(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_827(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 827: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_828(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 828: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_829(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 829: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_830(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 830: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_831(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 831: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_832(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 832: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_833(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 3;
    eprintln!("Trace node 833: SlaInstructionBits start=14, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_834(bytes, ctx),
        1 => match_node_instruction_835(bytes, ctx),
        2 => match_node_instruction_836(bytes, ctx),
        3 => match_node_instruction_837(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_834(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 834: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_835(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 835: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_836(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 836: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_837(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 837: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_838(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 838: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_839(bytes, ctx),
        1 => match_node_instruction_840(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_839(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 839: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_840(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 840: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_841(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 841: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_842(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 7;
    eprintln!("Trace node 842: SlaInstructionBits start=4, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_843(bytes, ctx),
        1 => match_node_instruction_844(bytes, ctx),
        2 => match_node_instruction_845(bytes, ctx),
        3 => match_node_instruction_846(bytes, ctx),
        4 => match_node_instruction_847(bytes, ctx),
        5 => match_node_instruction_848(bytes, ctx),
        6 => match_node_instruction_849(bytes, ctx),
        7 => match_node_instruction_850(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_843(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 843: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_844(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 844: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_845(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 845: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_846(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 846: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_847(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 847: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_848(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 848: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_849(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 849: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_850(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 850: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_851(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 851: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_852(bytes, ctx),
        1 => match_node_instruction_853(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_852(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 852: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_853(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 853: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_854(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 854: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_855(bytes, ctx),
        1 => match_node_instruction_856(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_855(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 855: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_856(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 856: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_857(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 857: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_858(bytes, ctx),
        1 => match_node_instruction_859(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_858(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 858: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_859(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 859: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_860(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 860: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_861(bytes, ctx),
        1 => match_node_instruction_862(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_861(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 861: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_862(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 862: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_863(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 863: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_864(bytes, ctx),
        1 => match_node_instruction_865(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_864(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 864: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_865(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 865: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_866(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 3;
    eprintln!("Trace node 866: SlaInstructionBits start=4, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_867(bytes, ctx),
        1 => match_node_instruction_882(bytes, ctx),
        2 => match_node_instruction_895(bytes, ctx),
        3 => match_node_instruction_902(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_867(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 867: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_868(bytes, ctx),
        1 => match_node_instruction_881(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_868(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 7;
    eprintln!("Trace node 868: SlaInstructionBits start=6, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_869(bytes, ctx),
        1 => match_node_instruction_870(bytes, ctx),
        2 => match_node_instruction_871(bytes, ctx),
        3 => match_node_instruction_872(bytes, ctx),
        4 => match_node_instruction_873(bytes, ctx),
        5 => match_node_instruction_876(bytes, ctx),
        6 => match_node_instruction_879(bytes, ctx),
        7 => match_node_instruction_880(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_869(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 869: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_870(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 870: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_871(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 871: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_872(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 872: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_873(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 873: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_874(bytes, ctx),
        1 => match_node_instruction_875(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_874(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 874: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_875(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 875: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_876(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 876: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_877(bytes, ctx),
        1 => match_node_instruction_878(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_877(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 877: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_878(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 878: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_879(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 879: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_880(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 880: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_881(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 881: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_882(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 3;
    eprintln!("Trace node 882: SlaInstructionBits start=6, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_883(bytes, ctx),
        1 => match_node_instruction_884(bytes, ctx),
        2 => match_node_instruction_885(bytes, ctx),
        3 => match_node_instruction_894(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_883(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 883: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_884(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 884: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_885(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 7;
    eprintln!("Trace node 885: SlaInstructionBits start=10, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_886(bytes, ctx),
        1 => match_node_instruction_887(bytes, ctx),
        2 => match_node_instruction_888(bytes, ctx),
        3 => match_node_instruction_889(bytes, ctx),
        4 => match_node_instruction_890(bytes, ctx),
        5 => match_node_instruction_891(bytes, ctx),
        6 => match_node_instruction_892(bytes, ctx),
        7 => match_node_instruction_893(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_886(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 886: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_887(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 887: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_888(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 888: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_889(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 889: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_890(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 890: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_891(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 891: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_892(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 892: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_893(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 893: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_894(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 894: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_895(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 895: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_896(bytes, ctx),
        1 => match_node_instruction_901(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_896(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 3;
    eprintln!("Trace node 896: SlaInstructionBits start=8, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_897(bytes, ctx),
        1 => match_node_instruction_898(bytes, ctx),
        2 => match_node_instruction_899(bytes, ctx),
        3 => match_node_instruction_900(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_897(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 897: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_898(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 898: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_899(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 899: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_900(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 900: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_901(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 901: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_902(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 3;
    eprintln!("Trace node 902: SlaInstructionBits start=6, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_903(bytes, ctx),
        1 => match_node_instruction_904(bytes, ctx),
        2 => match_node_instruction_905(bytes, ctx),
        3 => match_node_instruction_906(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_903(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 903: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_904(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 904: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_905(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 905: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_906(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 906: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_907(bytes, ctx),
        1 => match_node_instruction_912(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_907(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 907: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
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
    eprintln!("Trace node 909: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_910(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 910: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_911(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 911: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_912(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 912: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_913(bytes, ctx),
        1 => match_node_instruction_914(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_913(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 913: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_914(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 914: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_915(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 915: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_916(bytes, ctx),
        1 => match_node_instruction_917(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_916(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 916: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_917(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 917: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_918(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 918: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_919(bytes, ctx),
        1 => match_node_instruction_920(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_919(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 919: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_920(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 920: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_921(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 921: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_922(bytes, ctx),
        1 => match_node_instruction_923(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_922(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 922: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_923(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 3;
    eprintln!("Trace node 923: SlaInstructionBits start=5, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_924(bytes, ctx),
        1 => match_node_instruction_981(bytes, ctx),
        2 => match_node_instruction_1004(bytes, ctx),
        3 => match_node_instruction_1059(bytes, ctx),
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
        1 => match_node_instruction_952(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_925(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 925: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_926(bytes, ctx),
        1 => match_node_instruction_943(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_926(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 15;
    eprintln!("Trace node 926: SlaInstructionBits start=7, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_927(bytes, ctx),
        1 => match_node_instruction_928(bytes, ctx),
        2 => match_node_instruction_929(bytes, ctx),
        3 => match_node_instruction_930(bytes, ctx),
        4 => match_node_instruction_931(bytes, ctx),
        5 => match_node_instruction_932(bytes, ctx),
        6 => match_node_instruction_933(bytes, ctx),
        7 => match_node_instruction_934(bytes, ctx),
        8 => match_node_instruction_935(bytes, ctx),
        9 => match_node_instruction_936(bytes, ctx),
        10 => match_node_instruction_937(bytes, ctx),
        11 => match_node_instruction_938(bytes, ctx),
        12 => match_node_instruction_939(bytes, ctx),
        13 => match_node_instruction_940(bytes, ctx),
        14 => match_node_instruction_941(bytes, ctx),
        15 => match_node_instruction_942(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_927(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 927: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_928(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 928: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_929(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 929: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_930(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 930: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_931(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 931: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_932(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 932: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_933(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 933: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_934(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 934: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_935(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 935: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_936(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 936: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_937(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 937: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_938(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 938: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_939(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 939: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_940(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 940: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_941(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 941: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_942(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 942: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_943(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 943: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_944(bytes, ctx),
        1 => match_node_instruction_951(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_944(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 944: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_945(bytes, ctx),
        1 => match_node_instruction_950(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_945(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 3;
    eprintln!("Trace node 945: SlaInstructionBits start=24, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_946(bytes, ctx),
        1 => match_node_instruction_947(bytes, ctx),
        2 => match_node_instruction_948(bytes, ctx),
        3 => match_node_instruction_949(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_946(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 946: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_947(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 947: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_948(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 948: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_949(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 949: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_950(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 950: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_951(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 951: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_952(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 952: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_953(bytes, ctx),
        1 => match_node_instruction_972(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_953(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 15;
    eprintln!("Trace node 953: SlaInstructionBits start=7, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_954(bytes, ctx),
        1 => match_node_instruction_955(bytes, ctx),
        2 => match_node_instruction_956(bytes, ctx),
        3 => match_node_instruction_957(bytes, ctx),
        4 => match_node_instruction_958(bytes, ctx),
        5 => match_node_instruction_959(bytes, ctx),
        6 => match_node_instruction_962(bytes, ctx),
        7 => match_node_instruction_963(bytes, ctx),
        8 => match_node_instruction_964(bytes, ctx),
        9 => match_node_instruction_965(bytes, ctx),
        10 => match_node_instruction_966(bytes, ctx),
        11 => match_node_instruction_967(bytes, ctx),
        12 => match_node_instruction_968(bytes, ctx),
        13 => match_node_instruction_969(bytes, ctx),
        14 => match_node_instruction_970(bytes, ctx),
        15 => match_node_instruction_971(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_954(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 954: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_955(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 955: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_956(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 956: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_957(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 957: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_958(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 958: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_959(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (16 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 16) & 1;
    eprintln!("Trace node 959: SlaInstructionBits start=16, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_960(bytes, ctx),
        1 => match_node_instruction_961(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_960(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 960: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_961(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 961: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_962(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 962: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_963(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 963: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_964(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 964: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_965(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 965: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_966(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 966: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_967(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 967: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_968(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 968: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_969(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 969: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_970(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 970: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_971(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 971: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_972(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 972: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_973(bytes, ctx),
        1 => match_node_instruction_980(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_973(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 973: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_974(bytes, ctx),
        1 => match_node_instruction_979(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_974(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 974: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_975(bytes, ctx),
        1 => match_node_instruction_976(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_975(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 975: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_976(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (27 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 27) & 1;
    eprintln!("Trace node 976: SlaInstructionBits start=27, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_977(bytes, ctx),
        1 => match_node_instruction_978(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_977(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 977: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_978(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 978: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_979(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 979: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_980(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 980: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_981(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 15;
    eprintln!("Trace node 981: SlaInstructionBits start=7, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_982(bytes, ctx),
        1 => match_node_instruction_983(bytes, ctx),
        2 => match_node_instruction_984(bytes, ctx),
        3 => match_node_instruction_989(bytes, ctx),
        4 => match_node_instruction_990(bytes, ctx),
        5 => match_node_instruction_991(bytes, ctx),
        6 => match_node_instruction_992(bytes, ctx),
        7 => match_node_instruction_995(bytes, ctx),
        8 => match_node_instruction_996(bytes, ctx),
        9 => match_node_instruction_997(bytes, ctx),
        10 => match_node_instruction_998(bytes, ctx),
        11 => match_node_instruction_999(bytes, ctx),
        12 => match_node_instruction_1000(bytes, ctx),
        13 => match_node_instruction_1001(bytes, ctx),
        14 => match_node_instruction_1002(bytes, ctx),
        15 => match_node_instruction_1003(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_982(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 982: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_983(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 983: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_984(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (26 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 26) & 3;
    eprintln!("Trace node 984: SlaInstructionBits start=26, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_985(bytes, ctx),
        1 => match_node_instruction_986(bytes, ctx),
        2 => match_node_instruction_987(bytes, ctx),
        3 => match_node_instruction_988(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_985(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 985: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_986(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 986: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_987(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 987: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_988(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 988: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_989(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 989: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_990(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 990: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_991(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 991: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_992(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (26 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 26) & 1;
    eprintln!("Trace node 992: SlaInstructionBits start=26, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_993(bytes, ctx),
        1 => match_node_instruction_994(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_993(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 993: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_994(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 994: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_995(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 995: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_996(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 996: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_997(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 997: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_998(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 998: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_999(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 999: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1000(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1000: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1001(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1001: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1002(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1002: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1003(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1003: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1004(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1004: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1005(bytes, ctx),
        1 => match_node_instruction_1032(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1005(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 1005: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1006(bytes, ctx),
        1 => match_node_instruction_1019(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1006(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 1006: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1007(bytes, ctx),
        1 => match_node_instruction_1012(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1007(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 1007: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1008(bytes, ctx),
        1 => match_node_instruction_1009(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1008(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1008: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1009(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (23 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 23) & 1;
    eprintln!("Trace node 1009: SlaInstructionBits start=23, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1010(bytes, ctx),
        1 => match_node_instruction_1011(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1010(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1010: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1011(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1011: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1012(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 1012: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1013(bytes, ctx),
        1 => match_node_instruction_1016(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1013(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (23 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 23) & 1;
    eprintln!("Trace node 1013: SlaInstructionBits start=23, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1014(bytes, ctx),
        1 => match_node_instruction_1015(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1014(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1014: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1015(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1015: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1016(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (23 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 23) & 1;
    eprintln!("Trace node 1016: SlaInstructionBits start=23, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1017(bytes, ctx),
        1 => match_node_instruction_1018(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1017(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1017: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1018(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1018: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1019(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (23 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 23) & 1;
    eprintln!("Trace node 1019: SlaInstructionBits start=23, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1020(bytes, ctx),
        1 => match_node_instruction_1027(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1020(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1020: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1021(bytes, ctx),
        1 => match_node_instruction_1024(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1021(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 1021: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1022(bytes, ctx),
        1 => match_node_instruction_1023(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1022(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1022: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1023(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1023: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1024(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 1024: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1025(bytes, ctx),
        1 => match_node_instruction_1026(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1025(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1025: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1026(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1026: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1027(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 1027: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1028(bytes, ctx),
        1 => match_node_instruction_1031(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1028(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1028: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1029(bytes, ctx),
        1 => match_node_instruction_1030(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1029(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1029: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1030(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1030: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1031(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1031: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1032(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 1032: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1033(bytes, ctx),
        1 => match_node_instruction_1046(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1033(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 1033: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1034(bytes, ctx),
        1 => match_node_instruction_1039(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1034(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 1034: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1035(bytes, ctx),
        1 => match_node_instruction_1036(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1035(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1035: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1036(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (23 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 23) & 1;
    eprintln!("Trace node 1036: SlaInstructionBits start=23, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1037(bytes, ctx),
        1 => match_node_instruction_1038(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1037(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1037: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1038(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1038: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1039(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (23 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 23) & 1;
    eprintln!("Trace node 1039: SlaInstructionBits start=23, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1040(bytes, ctx),
        1 => match_node_instruction_1043(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1040(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 1040: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1041(bytes, ctx),
        1 => match_node_instruction_1042(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1041(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1041: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1042(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1042: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1043(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 1043: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1044(bytes, ctx),
        1 => match_node_instruction_1045(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1044(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1044: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1045(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1045: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1046(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1046: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1047(bytes, ctx),
        1 => match_node_instruction_1052(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1047(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 1047: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1048(bytes, ctx),
        1 => match_node_instruction_1051(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1048(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (23 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 23) & 1;
    eprintln!("Trace node 1048: SlaInstructionBits start=23, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1049(bytes, ctx),
        1 => match_node_instruction_1050(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1049(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1049: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1050(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1050: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1051(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1051: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1052(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 1052: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1053(bytes, ctx),
        1 => match_node_instruction_1056(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1053(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (23 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 23) & 1;
    eprintln!("Trace node 1053: SlaInstructionBits start=23, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1054(bytes, ctx),
        1 => match_node_instruction_1055(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1054(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1054: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1055(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1055: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1056(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (23 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 23) & 1;
    eprintln!("Trace node 1056: SlaInstructionBits start=23, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1057(bytes, ctx),
        1 => match_node_instruction_1058(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1057(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1057: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1058(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1058: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1059(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (27 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 27) & 1;
    eprintln!("Trace node 1059: SlaInstructionBits start=27, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1060(bytes, ctx),
        1 => match_node_instruction_1163(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1060(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (22 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 22) & 3;
    eprintln!("Trace node 1060: SlaInstructionBits start=22, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1061(bytes, ctx),
        1 => match_node_instruction_1062(bytes, ctx),
        2 => match_node_instruction_1089(bytes, ctx),
        3 => match_node_instruction_1126(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1061(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1061: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1062(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 1062: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1063(bytes, ctx),
        1 => match_node_instruction_1064(bytes, ctx),
        2 => match_node_instruction_1067(bytes, ctx),
        3 => match_node_instruction_1068(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1063(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1063: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1064(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 1;
    eprintln!("Trace node 1064: SlaInstructionBits start=25, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1065(bytes, ctx),
        1 => match_node_instruction_1066(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1065(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1065: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1066(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1066: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1067(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1067: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1068(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 1068: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1069(bytes, ctx),
        1 => match_node_instruction_1070(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1069(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1069: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1070(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 15;
    eprintln!("Trace node 1070: SlaInstructionBits start=12, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1071(bytes, ctx),
        1 => match_node_instruction_1072(bytes, ctx),
        2 => match_node_instruction_1073(bytes, ctx),
        3 => match_node_instruction_1074(bytes, ctx),
        4 => match_node_instruction_1075(bytes, ctx),
        5 => match_node_instruction_1076(bytes, ctx),
        6 => match_node_instruction_1077(bytes, ctx),
        7 => match_node_instruction_1078(bytes, ctx),
        8 => match_node_instruction_1079(bytes, ctx),
        9 => match_node_instruction_1082(bytes, ctx),
        10 => match_node_instruction_1083(bytes, ctx),
        11 => match_node_instruction_1084(bytes, ctx),
        12 => match_node_instruction_1085(bytes, ctx),
        13 => match_node_instruction_1086(bytes, ctx),
        14 => match_node_instruction_1087(bytes, ctx),
        15 => match_node_instruction_1088(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1071(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1071: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1072(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1072: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1073(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1073: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1074(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1074: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1075(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1075: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1076(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1076: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1077(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1077: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1078(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1078: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1079(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 1;
    eprintln!("Trace node 1079: SlaInstructionBits start=24, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1080(bytes, ctx),
        1 => match_node_instruction_1081(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1080(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1080: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1081(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1081: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1082(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1082: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1083(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1083: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1084(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1084: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1085(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1085: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1086(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1086: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1087(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1087: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1088(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1088: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1089(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 1089: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1090(bytes, ctx),
        1 => match_node_instruction_1095(bytes, ctx),
        2 => match_node_instruction_1098(bytes, ctx),
        3 => match_node_instruction_1101(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1090(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 1090: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1091(bytes, ctx),
        1 => match_node_instruction_1094(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1091(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 1;
    eprintln!("Trace node 1091: SlaInstructionBits start=25, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1092(bytes, ctx),
        1 => match_node_instruction_1093(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1092(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1092: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1093(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1093: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1094(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1094: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1095(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 1;
    eprintln!("Trace node 1095: SlaInstructionBits start=25, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1096(bytes, ctx),
        1 => match_node_instruction_1097(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1096(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1096: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1097(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1097: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1098(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 1;
    eprintln!("Trace node 1098: SlaInstructionBits start=25, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1099(bytes, ctx),
        1 => match_node_instruction_1100(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1099(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1099: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1100(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1100: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1101(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 1101: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1102(bytes, ctx),
        1 => match_node_instruction_1105(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1102(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 1;
    eprintln!("Trace node 1102: SlaInstructionBits start=25, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1103(bytes, ctx),
        1 => match_node_instruction_1104(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1103(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1103: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1104(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1104: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1105(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 15;
    eprintln!("Trace node 1105: SlaInstructionBits start=12, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1106(bytes, ctx),
        1 => match_node_instruction_1109(bytes, ctx),
        2 => match_node_instruction_1110(bytes, ctx),
        3 => match_node_instruction_1111(bytes, ctx),
        4 => match_node_instruction_1112(bytes, ctx),
        5 => match_node_instruction_1113(bytes, ctx),
        6 => match_node_instruction_1114(bytes, ctx),
        7 => match_node_instruction_1115(bytes, ctx),
        8 => match_node_instruction_1116(bytes, ctx),
        9 => match_node_instruction_1119(bytes, ctx),
        10 => match_node_instruction_1120(bytes, ctx),
        11 => match_node_instruction_1121(bytes, ctx),
        12 => match_node_instruction_1122(bytes, ctx),
        13 => match_node_instruction_1123(bytes, ctx),
        14 => match_node_instruction_1124(bytes, ctx),
        15 => match_node_instruction_1125(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1106(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 1;
    eprintln!("Trace node 1106: SlaInstructionBits start=24, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1107(bytes, ctx),
        1 => match_node_instruction_1108(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1107(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1107: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1108(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1108: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1109(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1109: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1110(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1110: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1111(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1111: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1112(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1112: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1113(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1113: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1114(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1114: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1115(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1115: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1116(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 1;
    eprintln!("Trace node 1116: SlaInstructionBits start=24, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1117(bytes, ctx),
        1 => match_node_instruction_1118(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1117(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1117: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1118(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1118: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1119(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1119: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1120(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1120: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1121(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1121: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1122(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1122: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1123(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1123: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1124(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1124: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1125(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1125: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1126(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 1126: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1127(bytes, ctx),
        1 => match_node_instruction_1132(bytes, ctx),
        2 => match_node_instruction_1135(bytes, ctx),
        3 => match_node_instruction_1138(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1127(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 1127: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1128(bytes, ctx),
        1 => match_node_instruction_1131(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1128(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 1;
    eprintln!("Trace node 1128: SlaInstructionBits start=25, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1129(bytes, ctx),
        1 => match_node_instruction_1130(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1129(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1129: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1130(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1130: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1131(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1131: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1132(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 1;
    eprintln!("Trace node 1132: SlaInstructionBits start=25, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1133(bytes, ctx),
        1 => match_node_instruction_1134(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1133(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1133: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1134(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1134: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1135(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 1;
    eprintln!("Trace node 1135: SlaInstructionBits start=25, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1136(bytes, ctx),
        1 => match_node_instruction_1137(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1136(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1136: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1137(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1137: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1138(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 1138: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1139(bytes, ctx),
        1 => match_node_instruction_1142(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1139(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 1;
    eprintln!("Trace node 1139: SlaInstructionBits start=25, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1140(bytes, ctx),
        1 => match_node_instruction_1141(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1140(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1140: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1141(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1141: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1142(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 15;
    eprintln!("Trace node 1142: SlaInstructionBits start=12, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1143(bytes, ctx),
        1 => match_node_instruction_1146(bytes, ctx),
        2 => match_node_instruction_1147(bytes, ctx),
        3 => match_node_instruction_1148(bytes, ctx),
        4 => match_node_instruction_1149(bytes, ctx),
        5 => match_node_instruction_1150(bytes, ctx),
        6 => match_node_instruction_1151(bytes, ctx),
        7 => match_node_instruction_1152(bytes, ctx),
        8 => match_node_instruction_1153(bytes, ctx),
        9 => match_node_instruction_1156(bytes, ctx),
        10 => match_node_instruction_1157(bytes, ctx),
        11 => match_node_instruction_1158(bytes, ctx),
        12 => match_node_instruction_1159(bytes, ctx),
        13 => match_node_instruction_1160(bytes, ctx),
        14 => match_node_instruction_1161(bytes, ctx),
        15 => match_node_instruction_1162(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1143(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 1;
    eprintln!("Trace node 1143: SlaInstructionBits start=24, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1144(bytes, ctx),
        1 => match_node_instruction_1145(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1144(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1144: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1145(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1145: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1146(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1146: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1147(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1147: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1148(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1148: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1149(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1149: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1150(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1150: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1151(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1151: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1152(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1152: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1153(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 1;
    eprintln!("Trace node 1153: SlaInstructionBits start=24, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1154(bytes, ctx),
        1 => match_node_instruction_1155(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1154(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1154: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1155(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1155: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1156(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1156: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1157(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1157: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1158(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1158: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1159(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1159: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1160(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1160: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1161(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1161: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1162(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1162: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1163(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1163: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1164(bytes, ctx),
        1 => match_node_instruction_1167(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1164(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 1164: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1165(bytes, ctx),
        1 => match_node_instruction_1166(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1165(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1165: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1166(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1166: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1167(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 1167: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1168(bytes, ctx),
        1 => match_node_instruction_1169(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1168(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1168: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1169(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1169: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1170(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 1170: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1171(bytes, ctx),
        1 => match_node_instruction_1370(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1171(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (16 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 16) & 1;
    eprintln!("Trace node 1171: SlaInstructionBits start=16, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1172(bytes, ctx),
        1 => match_node_instruction_1211(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1172(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 5 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 31;
    eprintln!("Trace node 1172: SlaInstructionBits start=6, size=5, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1173(bytes, ctx),
        1 => match_node_instruction_1174(bytes, ctx),
        2 => match_node_instruction_1175(bytes, ctx),
        3 => match_node_instruction_1176(bytes, ctx),
        4 => match_node_instruction_1177(bytes, ctx),
        5 => match_node_instruction_1178(bytes, ctx),
        6 => match_node_instruction_1179(bytes, ctx),
        7 => match_node_instruction_1180(bytes, ctx),
        8 => match_node_instruction_1181(bytes, ctx),
        9 => match_node_instruction_1182(bytes, ctx),
        10 => match_node_instruction_1183(bytes, ctx),
        11 => match_node_instruction_1184(bytes, ctx),
        12 => match_node_instruction_1185(bytes, ctx),
        13 => match_node_instruction_1186(bytes, ctx),
        14 => match_node_instruction_1187(bytes, ctx),
        15 => match_node_instruction_1188(bytes, ctx),
        16 => match_node_instruction_1189(bytes, ctx),
        17 => match_node_instruction_1192(bytes, ctx),
        18 => match_node_instruction_1193(bytes, ctx),
        19 => match_node_instruction_1194(bytes, ctx),
        20 => match_node_instruction_1195(bytes, ctx),
        21 => match_node_instruction_1196(bytes, ctx),
        22 => match_node_instruction_1199(bytes, ctx),
        23 => match_node_instruction_1200(bytes, ctx),
        24 => match_node_instruction_1201(bytes, ctx),
        25 => match_node_instruction_1202(bytes, ctx),
        26 => match_node_instruction_1203(bytes, ctx),
        27 => match_node_instruction_1204(bytes, ctx),
        28 => match_node_instruction_1205(bytes, ctx),
        29 => match_node_instruction_1206(bytes, ctx),
        30 => match_node_instruction_1209(bytes, ctx),
        31 => match_node_instruction_1210(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1173(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1173: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1174(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1174: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1175(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1175: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1176(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1176: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1177(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1177: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1178(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1178: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1179(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1179: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1180(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1180: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1181(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1181: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1182(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1182: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1183(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1183: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1184(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1184: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1185(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1185: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1186(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1186: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1187(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1187: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1188(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1188: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1189(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 1189: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1190(bytes, ctx),
        1 => match_node_instruction_1191(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1190(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1190: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1191(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1191: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1192(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1192: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1193(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1193: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1194(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1194: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1195(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1195: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1196(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 1196: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1197(bytes, ctx),
        1 => match_node_instruction_1198(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1197(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1197: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1198(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1198: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1199(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1199: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1200(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1200: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1201(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1201: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1202(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1202: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1203(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1203: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1204(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1204: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1205(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1205: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1206(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (19 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 19) & 1;
    eprintln!("Trace node 1206: SlaInstructionBits start=19, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1207(bytes, ctx),
        1 => match_node_instruction_1208(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1207(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1207: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1208(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1208: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1209(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1209: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1210(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1210: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1211(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (17 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 17) & 1;
    eprintln!("Trace node 1211: SlaInstructionBits start=17, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1212(bytes, ctx),
        1 => match_node_instruction_1367(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1212(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (19 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 19) & 1;
    eprintln!("Trace node 1212: SlaInstructionBits start=19, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1213(bytes, ctx),
        1 => match_node_instruction_1366(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1213(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 1;
    eprintln!("Trace node 1213: SlaInstructionBits start=6, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1214(bytes, ctx),
        1 => match_node_instruction_1215(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1214(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1214: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1215(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 1215: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1216(bytes, ctx),
        1 => match_node_instruction_1217(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1216(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1216: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1217(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 1217: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1218(bytes, ctx),
        1 => match_node_instruction_1219(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1218(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1218: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1219(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 7 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 127;
    eprintln!("Trace node 1219: SlaInstructionBits start=5, size=7, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1220(bytes, ctx),
        1 => match_node_instruction_1221(bytes, ctx),
        2 => match_node_instruction_1222(bytes, ctx),
        3 => match_node_instruction_1223(bytes, ctx),
        4 => match_node_instruction_1224(bytes, ctx),
        5 => match_node_instruction_1225(bytes, ctx),
        6 => match_node_instruction_1226(bytes, ctx),
        7 => match_node_instruction_1227(bytes, ctx),
        8 => match_node_instruction_1228(bytes, ctx),
        9 => match_node_instruction_1229(bytes, ctx),
        10 => match_node_instruction_1230(bytes, ctx),
        11 => match_node_instruction_1231(bytes, ctx),
        12 => match_node_instruction_1232(bytes, ctx),
        13 => match_node_instruction_1233(bytes, ctx),
        14 => match_node_instruction_1234(bytes, ctx),
        15 => match_node_instruction_1235(bytes, ctx),
        16 => match_node_instruction_1236(bytes, ctx),
        17 => match_node_instruction_1237(bytes, ctx),
        18 => match_node_instruction_1238(bytes, ctx),
        19 => match_node_instruction_1239(bytes, ctx),
        20 => match_node_instruction_1240(bytes, ctx),
        21 => match_node_instruction_1241(bytes, ctx),
        22 => match_node_instruction_1242(bytes, ctx),
        23 => match_node_instruction_1243(bytes, ctx),
        24 => match_node_instruction_1244(bytes, ctx),
        25 => match_node_instruction_1245(bytes, ctx),
        26 => match_node_instruction_1246(bytes, ctx),
        27 => match_node_instruction_1247(bytes, ctx),
        28 => match_node_instruction_1248(bytes, ctx),
        29 => match_node_instruction_1249(bytes, ctx),
        30 => match_node_instruction_1250(bytes, ctx),
        31 => match_node_instruction_1251(bytes, ctx),
        32 => match_node_instruction_1252(bytes, ctx),
        33 => match_node_instruction_1253(bytes, ctx),
        34 => match_node_instruction_1254(bytes, ctx),
        35 => match_node_instruction_1255(bytes, ctx),
        36 => match_node_instruction_1256(bytes, ctx),
        37 => match_node_instruction_1257(bytes, ctx),
        38 => match_node_instruction_1258(bytes, ctx),
        39 => match_node_instruction_1259(bytes, ctx),
        40 => match_node_instruction_1260(bytes, ctx),
        41 => match_node_instruction_1261(bytes, ctx),
        42 => match_node_instruction_1262(bytes, ctx),
        43 => match_node_instruction_1263(bytes, ctx),
        44 => match_node_instruction_1264(bytes, ctx),
        45 => match_node_instruction_1265(bytes, ctx),
        46 => match_node_instruction_1266(bytes, ctx),
        47 => match_node_instruction_1267(bytes, ctx),
        48 => match_node_instruction_1268(bytes, ctx),
        49 => match_node_instruction_1269(bytes, ctx),
        50 => match_node_instruction_1270(bytes, ctx),
        51 => match_node_instruction_1271(bytes, ctx),
        52 => match_node_instruction_1272(bytes, ctx),
        53 => match_node_instruction_1273(bytes, ctx),
        54 => match_node_instruction_1274(bytes, ctx),
        55 => match_node_instruction_1275(bytes, ctx),
        56 => match_node_instruction_1276(bytes, ctx),
        57 => match_node_instruction_1277(bytes, ctx),
        58 => match_node_instruction_1278(bytes, ctx),
        59 => match_node_instruction_1291(bytes, ctx),
        60 => match_node_instruction_1296(bytes, ctx),
        61 => match_node_instruction_1297(bytes, ctx),
        62 => match_node_instruction_1298(bytes, ctx),
        63 => match_node_instruction_1299(bytes, ctx),
        64 => match_node_instruction_1300(bytes, ctx),
        65 => match_node_instruction_1301(bytes, ctx),
        66 => match_node_instruction_1302(bytes, ctx),
        67 => match_node_instruction_1303(bytes, ctx),
        68 => match_node_instruction_1304(bytes, ctx),
        69 => match_node_instruction_1305(bytes, ctx),
        70 => match_node_instruction_1306(bytes, ctx),
        71 => match_node_instruction_1307(bytes, ctx),
        72 => match_node_instruction_1308(bytes, ctx),
        73 => match_node_instruction_1309(bytes, ctx),
        74 => match_node_instruction_1310(bytes, ctx),
        75 => match_node_instruction_1311(bytes, ctx),
        76 => match_node_instruction_1312(bytes, ctx),
        77 => match_node_instruction_1313(bytes, ctx),
        78 => match_node_instruction_1314(bytes, ctx),
        79 => match_node_instruction_1315(bytes, ctx),
        80 => match_node_instruction_1316(bytes, ctx),
        81 => match_node_instruction_1317(bytes, ctx),
        82 => match_node_instruction_1318(bytes, ctx),
        83 => match_node_instruction_1319(bytes, ctx),
        84 => match_node_instruction_1320(bytes, ctx),
        85 => match_node_instruction_1321(bytes, ctx),
        86 => match_node_instruction_1322(bytes, ctx),
        87 => match_node_instruction_1323(bytes, ctx),
        88 => match_node_instruction_1324(bytes, ctx),
        89 => match_node_instruction_1325(bytes, ctx),
        90 => match_node_instruction_1326(bytes, ctx),
        91 => match_node_instruction_1327(bytes, ctx),
        92 => match_node_instruction_1328(bytes, ctx),
        93 => match_node_instruction_1329(bytes, ctx),
        94 => match_node_instruction_1330(bytes, ctx),
        95 => match_node_instruction_1331(bytes, ctx),
        96 => match_node_instruction_1332(bytes, ctx),
        97 => match_node_instruction_1333(bytes, ctx),
        98 => match_node_instruction_1334(bytes, ctx),
        99 => match_node_instruction_1335(bytes, ctx),
        100 => match_node_instruction_1336(bytes, ctx),
        101 => match_node_instruction_1337(bytes, ctx),
        102 => match_node_instruction_1338(bytes, ctx),
        103 => match_node_instruction_1339(bytes, ctx),
        104 => match_node_instruction_1340(bytes, ctx),
        105 => match_node_instruction_1341(bytes, ctx),
        106 => match_node_instruction_1342(bytes, ctx),
        107 => match_node_instruction_1343(bytes, ctx),
        108 => match_node_instruction_1344(bytes, ctx),
        109 => match_node_instruction_1345(bytes, ctx),
        110 => match_node_instruction_1346(bytes, ctx),
        111 => match_node_instruction_1347(bytes, ctx),
        112 => match_node_instruction_1348(bytes, ctx),
        113 => match_node_instruction_1349(bytes, ctx),
        114 => match_node_instruction_1350(bytes, ctx),
        115 => match_node_instruction_1351(bytes, ctx),
        116 => match_node_instruction_1352(bytes, ctx),
        117 => match_node_instruction_1353(bytes, ctx),
        118 => match_node_instruction_1354(bytes, ctx),
        119 => match_node_instruction_1355(bytes, ctx),
        120 => match_node_instruction_1356(bytes, ctx),
        121 => match_node_instruction_1357(bytes, ctx),
        122 => match_node_instruction_1358(bytes, ctx),
        123 => match_node_instruction_1359(bytes, ctx),
        124 => match_node_instruction_1360(bytes, ctx),
        125 => match_node_instruction_1361(bytes, ctx),
        126 => match_node_instruction_1362(bytes, ctx),
        127 => match_node_instruction_1363(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1220(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1220: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1221(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1221: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1222(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1222: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1223(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1223: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1224(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1224: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1225(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1225: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1226(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1226: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1227(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1227: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1228(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1228: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1229(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1229: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1230(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1230: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1231(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1231: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1232(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1232: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1233(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1233: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1234(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1234: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1235(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1235: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1236(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1236: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1237(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1237: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1238(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1238: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1239(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1239: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1240(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1240: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1241(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1241: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1242(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1242: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1243(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1243: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1244(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1244: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1245(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1245: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1246(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1246: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1247(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1247: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1248(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1248: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1249(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1249: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1250(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1250: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1251(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1251: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1252(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1252: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1253(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1253: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1254(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1254: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1255(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1255: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1256(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1256: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1257(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1257: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1258(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1258: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1259(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1259: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1260(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1260: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1261(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1261: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1262(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1262: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1263(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1263: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1264(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1264: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1265(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1265: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1266(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1266: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1267(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1267: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1268(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1268: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1269(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1269: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1270(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1270: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1271(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1271: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1272(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1272: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1273(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1273: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1274(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1274: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1275(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1275: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1276(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1276: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1277(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1277: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1278(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (27 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 27) & 1;
    eprintln!("Trace node 1278: SlaInstructionBits start=27, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1279(bytes, ctx),
        1 => match_node_instruction_1284(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1279(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (30 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 30) & 3;
    eprintln!("Trace node 1279: SlaInstructionBits start=30, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1280(bytes, ctx),
        1 => match_node_instruction_1281(bytes, ctx),
        2 => match_node_instruction_1282(bytes, ctx),
        3 => match_node_instruction_1283(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1280(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1280: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1281(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1281: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1282(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1282: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1283(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1283: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1284(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (21 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 21) & 3;
    eprintln!("Trace node 1284: SlaInstructionBits start=21, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1285(bytes, ctx),
        1 => match_node_instruction_1288(bytes, ctx),
        2 => match_node_instruction_1289(bytes, ctx),
        3 => match_node_instruction_1290(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1285(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (23 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 23) & 1;
    eprintln!("Trace node 1285: SlaInstructionBits start=23, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1286(bytes, ctx),
        1 => match_node_instruction_1287(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1286(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1286: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1287(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1287: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1288(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1288: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1289(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1289: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1290(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1290: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1291(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (26 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 26) & 3;
    eprintln!("Trace node 1291: SlaInstructionBits start=26, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1292(bytes, ctx),
        1 => match_node_instruction_1293(bytes, ctx),
        2 => match_node_instruction_1294(bytes, ctx),
        3 => match_node_instruction_1295(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1292(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1292: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1293(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1293: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1294(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1294: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1295(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1295: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1296(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1296: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1297(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1297: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1298(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1298: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1299(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1299: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1300(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1300: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1301(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1301: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1302(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1302: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1303(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1303: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1304(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1304: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1305(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1305: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1306(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1306: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1307(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1307: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1308(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1308: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1309(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1309: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1310(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1310: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1311(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1311: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1312(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1312: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1313(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1313: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1314(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1314: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1315(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1315: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1316(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1316: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1317(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1317: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1318(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1318: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1319(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1319: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1320(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1320: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1321(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1321: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1322(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1322: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1323(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1323: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1324(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1324: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1325(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1325: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1326(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1326: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1327(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1327: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1328(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1328: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1329(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1329: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1330(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1330: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1331(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1331: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1332(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1332: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1333(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1333: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1334(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1334: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1335(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1335: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1336(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1336: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1337(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1337: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1338(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1338: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1339(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1339: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1340(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1340: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1341(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1341: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1342(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1342: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1343(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1343: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1344(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1344: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1345(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1345: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1346(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1346: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1347(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1347: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1348(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1348: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1349(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1349: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1350(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1350: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1351(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1351: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1352(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1352: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1353(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1353: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1354(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1354: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1355(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1355: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1356(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1356: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1357(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1357: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1358(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1358: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1359(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1359: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1360(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1360: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1361(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1361: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1362(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1362: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1363(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (18 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 18) & 1;
    eprintln!("Trace node 1363: SlaInstructionBits start=18, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1364(bytes, ctx),
        1 => match_node_instruction_1365(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1364(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1364: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1365(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1365: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1366(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1366: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1367(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (19 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 19) & 1;
    eprintln!("Trace node 1367: SlaInstructionBits start=19, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1368(bytes, ctx),
        1 => match_node_instruction_1369(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1368(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1368: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1369(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1369: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1370(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 3;
    eprintln!("Trace node 1370: SlaInstructionBits start=5, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1371(bytes, ctx),
        1 => match_node_instruction_1450(bytes, ctx),
        2 => match_node_instruction_1591(bytes, ctx),
        3 => match_node_instruction_1598(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1371(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 7;
    eprintln!("Trace node 1371: SlaInstructionBits start=9, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1372(bytes, ctx),
        1 => match_node_instruction_1381(bytes, ctx),
        2 => match_node_instruction_1400(bytes, ctx),
        3 => match_node_instruction_1409(bytes, ctx),
        4 => match_node_instruction_1428(bytes, ctx),
        5 => match_node_instruction_1437(bytes, ctx),
        6 => match_node_instruction_1448(bytes, ctx),
        7 => match_node_instruction_1449(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1372(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 1372: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1373(bytes, ctx),
        1 => match_node_instruction_1380(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1373(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 1373: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1374(bytes, ctx),
        1 => match_node_instruction_1375(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1374(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1374: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1375(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (23 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 23) & 1;
    eprintln!("Trace node 1375: SlaInstructionBits start=23, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1376(bytes, ctx),
        1 => match_node_instruction_1379(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1376(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (22 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 22) & 1;
    eprintln!("Trace node 1376: SlaInstructionBits start=22, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1377(bytes, ctx),
        1 => match_node_instruction_1378(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1377(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1377: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1378(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1378: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1379(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1379: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1380(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1380: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1381(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 1381: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1382(bytes, ctx),
        1 => match_node_instruction_1391(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1382(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 1382: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1383(bytes, ctx),
        1 => match_node_instruction_1390(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1383(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 1383: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1384(bytes, ctx),
        1 => match_node_instruction_1385(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1384(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1384: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1385(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (23 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 23) & 1;
    eprintln!("Trace node 1385: SlaInstructionBits start=23, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1386(bytes, ctx),
        1 => match_node_instruction_1389(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1386(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (22 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 22) & 1;
    eprintln!("Trace node 1386: SlaInstructionBits start=22, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1387(bytes, ctx),
        1 => match_node_instruction_1388(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1387(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1387: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1388(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1388: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1389(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1389: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1390(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1390: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1391(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 1391: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1392(bytes, ctx),
        1 => match_node_instruction_1399(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1392(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 1392: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1393(bytes, ctx),
        1 => match_node_instruction_1394(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1393(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1393: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1394(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (23 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 23) & 1;
    eprintln!("Trace node 1394: SlaInstructionBits start=23, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1395(bytes, ctx),
        1 => match_node_instruction_1398(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1395(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (22 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 22) & 1;
    eprintln!("Trace node 1395: SlaInstructionBits start=22, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1396(bytes, ctx),
        1 => match_node_instruction_1397(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1396(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1396: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1397(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1397: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1398(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1398: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1399(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1399: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1400(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 1400: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1401(bytes, ctx),
        1 => match_node_instruction_1408(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1401(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 1401: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1402(bytes, ctx),
        1 => match_node_instruction_1403(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1402(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1402: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1403(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (23 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 23) & 1;
    eprintln!("Trace node 1403: SlaInstructionBits start=23, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1404(bytes, ctx),
        1 => match_node_instruction_1407(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1404(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (22 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 22) & 1;
    eprintln!("Trace node 1404: SlaInstructionBits start=22, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1405(bytes, ctx),
        1 => match_node_instruction_1406(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1405(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1405: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1406(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1406: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1407(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1407: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1408(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1408: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1409(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 1409: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1410(bytes, ctx),
        1 => match_node_instruction_1419(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1410(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 1410: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1411(bytes, ctx),
        1 => match_node_instruction_1418(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1411(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 1411: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1412(bytes, ctx),
        1 => match_node_instruction_1413(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1412(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1412: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1413(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (23 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 23) & 1;
    eprintln!("Trace node 1413: SlaInstructionBits start=23, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1414(bytes, ctx),
        1 => match_node_instruction_1417(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1414(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (22 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 22) & 1;
    eprintln!("Trace node 1414: SlaInstructionBits start=22, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1415(bytes, ctx),
        1 => match_node_instruction_1416(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1415(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1415: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1416(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1416: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1417(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1417: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1418(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1418: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1419(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 1419: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1420(bytes, ctx),
        1 => match_node_instruction_1427(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1420(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 1420: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1421(bytes, ctx),
        1 => match_node_instruction_1422(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1421(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1421: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1422(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (23 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 23) & 1;
    eprintln!("Trace node 1422: SlaInstructionBits start=23, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1423(bytes, ctx),
        1 => match_node_instruction_1426(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1423(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (22 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 22) & 1;
    eprintln!("Trace node 1423: SlaInstructionBits start=22, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1424(bytes, ctx),
        1 => match_node_instruction_1425(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1424(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1424: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1425(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1425: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1426(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1426: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1427(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1427: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1428(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 1428: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1429(bytes, ctx),
        1 => match_node_instruction_1436(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1429(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 1429: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1430(bytes, ctx),
        1 => match_node_instruction_1431(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1430(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1430: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1431(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (23 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 23) & 1;
    eprintln!("Trace node 1431: SlaInstructionBits start=23, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1432(bytes, ctx),
        1 => match_node_instruction_1435(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1432(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (22 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 22) & 1;
    eprintln!("Trace node 1432: SlaInstructionBits start=22, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1433(bytes, ctx),
        1 => match_node_instruction_1434(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1433(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1433: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1434(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1434: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1435(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1435: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1436(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1436: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1437(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 1437: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1438(bytes, ctx),
        1 => match_node_instruction_1447(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1438(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (23 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 23) & 1;
    eprintln!("Trace node 1438: SlaInstructionBits start=23, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1439(bytes, ctx),
        1 => match_node_instruction_1444(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1439(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (21 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 21) & 3;
    eprintln!("Trace node 1439: SlaInstructionBits start=21, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1440(bytes, ctx),
        1 => match_node_instruction_1441(bytes, ctx),
        2 => match_node_instruction_1442(bytes, ctx),
        3 => match_node_instruction_1443(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1440(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1440: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1441(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1441: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1442(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1442: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1443(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1443: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1444(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 1444: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1445(bytes, ctx),
        1 => match_node_instruction_1446(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1445(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1445: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1446(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1446: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1447(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1447: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1448(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1448: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1449(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1449: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1450(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 15;
    eprintln!("Trace node 1450: SlaInstructionBits start=7, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1451(bytes, ctx),
        1 => match_node_instruction_1456(bytes, ctx),
        2 => match_node_instruction_1461(bytes, ctx),
        3 => match_node_instruction_1466(bytes, ctx),
        4 => match_node_instruction_1467(bytes, ctx),
        5 => match_node_instruction_1504(bytes, ctx),
        6 => match_node_instruction_1523(bytes, ctx),
        7 => match_node_instruction_1544(bytes, ctx),
        8 => match_node_instruction_1553(bytes, ctx),
        9 => match_node_instruction_1562(bytes, ctx),
        10 => match_node_instruction_1565(bytes, ctx),
        11 => match_node_instruction_1570(bytes, ctx),
        12 => match_node_instruction_1575(bytes, ctx),
        13 => match_node_instruction_1578(bytes, ctx),
        14 => match_node_instruction_1581(bytes, ctx),
        15 => match_node_instruction_1588(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1451(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 1;
    eprintln!("Trace node 1451: SlaInstructionBits start=24, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1452(bytes, ctx),
        1 => match_node_instruction_1453(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1452(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1452: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1453(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1453: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1454(bytes, ctx),
        1 => match_node_instruction_1455(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1454(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1454: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1455(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1455: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1456(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 1;
    eprintln!("Trace node 1456: SlaInstructionBits start=24, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1457(bytes, ctx),
        1 => match_node_instruction_1458(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1457(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1457: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1458(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1458: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1459(bytes, ctx),
        1 => match_node_instruction_1460(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1459(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1459: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1460(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1460: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1461(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 1;
    eprintln!("Trace node 1461: SlaInstructionBits start=24, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1462(bytes, ctx),
        1 => match_node_instruction_1463(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1462(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1462: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1463(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1463: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1464(bytes, ctx),
        1 => match_node_instruction_1465(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1464(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1464: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1465(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1465: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1466(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1466: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1467(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 15;
    eprintln!("Trace node 1467: SlaInstructionBits start=24, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1468(bytes, ctx),
        1 => match_node_instruction_1471(bytes, ctx),
        2 => match_node_instruction_1474(bytes, ctx),
        3 => match_node_instruction_1477(bytes, ctx),
        4 => match_node_instruction_1478(bytes, ctx),
        5 => match_node_instruction_1481(bytes, ctx),
        6 => match_node_instruction_1484(bytes, ctx),
        7 => match_node_instruction_1487(bytes, ctx),
        8 => match_node_instruction_1488(bytes, ctx),
        9 => match_node_instruction_1491(bytes, ctx),
        10 => match_node_instruction_1494(bytes, ctx),
        11 => match_node_instruction_1497(bytes, ctx),
        12 => match_node_instruction_1500(bytes, ctx),
        13 => match_node_instruction_1501(bytes, ctx),
        14 => match_node_instruction_1502(bytes, ctx),
        15 => match_node_instruction_1503(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1468(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1468: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1469(bytes, ctx),
        1 => match_node_instruction_1470(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1469(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1469: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1470(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1470: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1471(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1471: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1472(bytes, ctx),
        1 => match_node_instruction_1473(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1472(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1472: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1473(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1473: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1474(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1474: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1475(bytes, ctx),
        1 => match_node_instruction_1476(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1475(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1475: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1476(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1476: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1477(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1477: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1478(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1478: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1479(bytes, ctx),
        1 => match_node_instruction_1480(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1479(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1479: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1480(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1480: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1481(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1481: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1482(bytes, ctx),
        1 => match_node_instruction_1483(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1482(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1482: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1483(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1483: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1484(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1484: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1485(bytes, ctx),
        1 => match_node_instruction_1486(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1485(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1485: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1486(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1486: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1487(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1487: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1488(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1488: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1489(bytes, ctx),
        1 => match_node_instruction_1490(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1489(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1489: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1490(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1490: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1491(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1491: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1492(bytes, ctx),
        1 => match_node_instruction_1493(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1492(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1492: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1493(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1493: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1494(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1494: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1495(bytes, ctx),
        1 => match_node_instruction_1496(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1495(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1495: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1496(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1496: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1497(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1497: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1498(bytes, ctx),
        1 => match_node_instruction_1499(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1498(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1498: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1499(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1499: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1500(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1500: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1501(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1501: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1502(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1502: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1503(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1503: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1504(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1504: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1505(bytes, ctx),
        1 => match_node_instruction_1522(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1505(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 15;
    eprintln!("Trace node 1505: SlaInstructionBits start=24, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1506(bytes, ctx),
        1 => match_node_instruction_1507(bytes, ctx),
        2 => match_node_instruction_1508(bytes, ctx),
        3 => match_node_instruction_1509(bytes, ctx),
        4 => match_node_instruction_1510(bytes, ctx),
        5 => match_node_instruction_1511(bytes, ctx),
        6 => match_node_instruction_1512(bytes, ctx),
        7 => match_node_instruction_1513(bytes, ctx),
        8 => match_node_instruction_1514(bytes, ctx),
        9 => match_node_instruction_1515(bytes, ctx),
        10 => match_node_instruction_1516(bytes, ctx),
        11 => match_node_instruction_1517(bytes, ctx),
        12 => match_node_instruction_1518(bytes, ctx),
        13 => match_node_instruction_1519(bytes, ctx),
        14 => match_node_instruction_1520(bytes, ctx),
        15 => match_node_instruction_1521(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1506(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1506: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1507(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1507: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1508(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1508: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1509(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1509: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1510(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1510: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1511(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1511: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1512(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1512: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1513(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1513: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1514(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1514: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1515(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1515: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1516(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1516: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1517(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1517: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1518(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1518: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1519(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1519: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1520(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1520: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1521(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1521: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1522(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1522: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1523(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 7;
    eprintln!("Trace node 1523: SlaInstructionBits start=25, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1524(bytes, ctx),
        1 => match_node_instruction_1527(bytes, ctx),
        2 => match_node_instruction_1530(bytes, ctx),
        3 => match_node_instruction_1533(bytes, ctx),
        4 => match_node_instruction_1534(bytes, ctx),
        5 => match_node_instruction_1537(bytes, ctx),
        6 => match_node_instruction_1540(bytes, ctx),
        7 => match_node_instruction_1543(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1524(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1524: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1525(bytes, ctx),
        1 => match_node_instruction_1526(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1525(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1525: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1526(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1526: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1527(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1527: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1528(bytes, ctx),
        1 => match_node_instruction_1529(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1528(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1528: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1529(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1529: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1530(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1530: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1531(bytes, ctx),
        1 => match_node_instruction_1532(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1531(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1531: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1532(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1532: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1533(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1533: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1534(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1534: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1535(bytes, ctx),
        1 => match_node_instruction_1536(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1535(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1535: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1536(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1536: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1537(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1537: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1538(bytes, ctx),
        1 => match_node_instruction_1539(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1538(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1538: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1539(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1539: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1540(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1540: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1541(bytes, ctx),
        1 => match_node_instruction_1542(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1541(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1541: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1542(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1542: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1543(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1543: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1544(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 3 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 7;
    eprintln!("Trace node 1544: SlaInstructionBits start=25, size=3, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1545(bytes, ctx),
        1 => match_node_instruction_1546(bytes, ctx),
        2 => match_node_instruction_1547(bytes, ctx),
        3 => match_node_instruction_1548(bytes, ctx),
        4 => match_node_instruction_1549(bytes, ctx),
        5 => match_node_instruction_1550(bytes, ctx),
        6 => match_node_instruction_1551(bytes, ctx),
        7 => match_node_instruction_1552(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1545(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1545: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1546(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1546: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1547(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1547: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1548(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1548: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1549(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1549: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1550(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1550: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1551(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1551: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1552(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1552: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1553(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1553: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1554(bytes, ctx),
        1 => match_node_instruction_1557(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1554(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (27 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 27) & 1;
    eprintln!("Trace node 1554: SlaInstructionBits start=27, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1555(bytes, ctx),
        1 => match_node_instruction_1556(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1555(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1555: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1556(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1556: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1557(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (26 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 26) & 3;
    eprintln!("Trace node 1557: SlaInstructionBits start=26, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1558(bytes, ctx),
        1 => match_node_instruction_1559(bytes, ctx),
        2 => match_node_instruction_1560(bytes, ctx),
        3 => match_node_instruction_1561(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1558(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1558: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1559(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1559: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1560(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1560: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1561(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1561: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1562(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1562: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1563(bytes, ctx),
        1 => match_node_instruction_1564(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1563(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1563: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1564(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1564: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1565(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1565: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1566(bytes, ctx),
        1 => match_node_instruction_1567(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1566(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1566: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1567(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (27 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 27) & 1;
    eprintln!("Trace node 1567: SlaInstructionBits start=27, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1568(bytes, ctx),
        1 => match_node_instruction_1569(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1568(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1568: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1569(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1569: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1570(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1570: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1571(bytes, ctx),
        1 => match_node_instruction_1574(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1571(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (27 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 27) & 1;
    eprintln!("Trace node 1571: SlaInstructionBits start=27, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1572(bytes, ctx),
        1 => match_node_instruction_1573(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1572(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1572: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1573(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1573: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1574(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1574: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1575(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1575: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1576(bytes, ctx),
        1 => match_node_instruction_1577(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1576(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1576: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1577(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1577: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1578(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1578: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1579(bytes, ctx),
        1 => match_node_instruction_1580(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1579(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1579: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1580(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1580: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1581(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 3;
    eprintln!("Trace node 1581: SlaInstructionBits start=24, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1582(bytes, ctx),
        1 => match_node_instruction_1583(bytes, ctx),
        2 => match_node_instruction_1584(bytes, ctx),
        3 => match_node_instruction_1585(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1582(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1582: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1583(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1583: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1584(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1584: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1585(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1585: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1586(bytes, ctx),
        1 => match_node_instruction_1587(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1586(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1586: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1587(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1587: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1588(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 1;
    eprintln!("Trace node 1588: SlaInstructionBits start=25, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1589(bytes, ctx),
        1 => match_node_instruction_1590(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1589(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1589: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1590(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1590: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1591(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1591: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1592(bytes, ctx),
        1 => match_node_instruction_1595(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1592(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 1592: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1593(bytes, ctx),
        1 => match_node_instruction_1594(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1593(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1593: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1594(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1594: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1595(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 1595: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1596(bytes, ctx),
        1 => match_node_instruction_1597(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1596(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1596: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1597(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1597: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1598(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (27 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 27) & 1;
    eprintln!("Trace node 1598: SlaInstructionBits start=27, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1599(bytes, ctx),
        1 => match_node_instruction_1600(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1599(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1599: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1600(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 1600: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_1601(bytes, ctx),
        1 => match_node_instruction_1602(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1601(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1601: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_1602(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1602: Terminal matched constructor ID 0");
    0
}

fn match_node_it_thfcc_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 15;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_it_thfcc_1(bytes, ctx),
        1 => match_node_it_thfcc_2(bytes, ctx),
        2 => match_node_it_thfcc_3(bytes, ctx),
        3 => match_node_it_thfcc_4(bytes, ctx),
        4 => match_node_it_thfcc_5(bytes, ctx),
        5 => match_node_it_thfcc_6(bytes, ctx),
        6 => match_node_it_thfcc_7(bytes, ctx),
        7 => match_node_it_thfcc_8(bytes, ctx),
        8 => match_node_it_thfcc_9(bytes, ctx),
        9 => match_node_it_thfcc_10(bytes, ctx),
        10 => match_node_it_thfcc_11(bytes, ctx),
        11 => match_node_it_thfcc_12(bytes, ctx),
        12 => match_node_it_thfcc_13(bytes, ctx),
        13 => match_node_it_thfcc_14(bytes, ctx),
        14 => match_node_it_thfcc_15(bytes, ctx),
        15 => match_node_it_thfcc_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_it_thfcc_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_it_thfcc_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_it_thfcc_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_it_thfcc_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_it_thfcc_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 4");
    4
}

fn match_node_it_thfcc_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_it_thfcc_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 6");
    6
}

fn match_node_it_thfcc_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 7");
    7
}

fn match_node_it_thfcc_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 8");
    8
}

fn match_node_it_thfcc_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 9");
    9
}

fn match_node_it_thfcc_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 10");
    10
}

fn match_node_it_thfcc_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 11");
    11
}

fn match_node_it_thfcc_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 12");
    12
}

fn match_node_it_thfcc_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 13");
    13
}

fn match_node_it_thfcc_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 14");
    14
}

fn match_node_it_thfcc_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched NOTHING");
    -1
}

fn match_node_ldbrace_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_ldec0_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (31 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 31) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=31, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ldec0_1(bytes, ctx),
        1 => match_node_ldec0_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ldec0_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_ldec0_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_ldec1_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (30 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 30) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=30, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ldec1_1(bytes, ctx),
        1 => match_node_ldec1_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ldec1_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_ldec1_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_ldec10_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (21 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 21) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=21, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ldec10_1(bytes, ctx),
        1 => match_node_ldec10_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ldec10_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_ldec10_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_ldec11_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ldec11_1(bytes, ctx),
        1 => match_node_ldec11_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ldec11_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_ldec11_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_ldec12_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (19 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 19) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=19, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ldec12_1(bytes, ctx),
        1 => match_node_ldec12_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ldec12_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_ldec12_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_ldec13_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (18 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 18) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=18, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ldec13_1(bytes, ctx),
        1 => match_node_ldec13_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ldec13_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_ldec13_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_ldec14_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (17 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 17) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=17, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ldec14_1(bytes, ctx),
        1 => match_node_ldec14_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ldec14_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_ldec14_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_ldec15_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (16 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 16) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=16, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ldec15_1(bytes, ctx),
        1 => match_node_ldec15_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ldec15_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_ldec15_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_ldec2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (29 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 29) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=29, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ldec2_1(bytes, ctx),
        1 => match_node_ldec2_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ldec2_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_ldec2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_ldec3_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (28 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 28) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=28, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ldec3_1(bytes, ctx),
        1 => match_node_ldec3_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ldec3_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_ldec3_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_ldec4_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (27 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 27) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=27, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ldec4_1(bytes, ctx),
        1 => match_node_ldec4_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ldec4_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_ldec4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_ldec5_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (26 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 26) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=26, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ldec5_1(bytes, ctx),
        1 => match_node_ldec5_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ldec5_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_ldec5_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_ldec6_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=25, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ldec6_1(bytes, ctx),
        1 => match_node_ldec6_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ldec6_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_ldec6_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_ldec7_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=24, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ldec7_1(bytes, ctx),
        1 => match_node_ldec7_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ldec7_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_ldec7_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_ldec8_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (23 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 23) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=23, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ldec8_1(bytes, ctx),
        1 => match_node_ldec8_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ldec8_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_ldec8_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_ldec9_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (22 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 22) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=22, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ldec9_1(bytes, ctx),
        1 => match_node_ldec9_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ldec9_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_ldec9_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_ldlist_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ldlist_1(bytes, ctx),
        1 => match_node_ldlist_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ldlist_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_ldlist_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_ldlist_dec_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_ldlist_inc_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_linc0_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (16 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 16) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=16, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_linc0_1(bytes, ctx),
        1 => match_node_linc0_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_linc0_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_linc0_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_linc1_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (17 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 17) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=17, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_linc1_1(bytes, ctx),
        1 => match_node_linc1_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_linc1_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_linc1_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_linc10_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (26 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 26) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=26, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_linc10_1(bytes, ctx),
        1 => match_node_linc10_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_linc10_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_linc10_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_linc11_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (27 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 27) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=27, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_linc11_1(bytes, ctx),
        1 => match_node_linc11_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_linc11_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_linc11_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_linc12_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (28 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 28) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=28, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_linc12_1(bytes, ctx),
        1 => match_node_linc12_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_linc12_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_linc12_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_linc13_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (29 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 29) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=29, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_linc13_1(bytes, ctx),
        1 => match_node_linc13_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_linc13_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_linc13_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_linc14_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (30 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 30) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=30, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_linc14_1(bytes, ctx),
        1 => match_node_linc14_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_linc14_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_linc14_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_linc15_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (31 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 31) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=31, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_linc15_1(bytes, ctx),
        1 => match_node_linc15_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_linc15_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_linc15_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_linc2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (18 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 18) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=18, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_linc2_1(bytes, ctx),
        1 => match_node_linc2_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_linc2_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_linc2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_linc3_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (19 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 19) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=19, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_linc3_1(bytes, ctx),
        1 => match_node_linc3_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_linc3_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_linc3_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_linc4_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_linc4_1(bytes, ctx),
        1 => match_node_linc4_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_linc4_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_linc4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_linc5_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (21 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 21) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=21, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_linc5_1(bytes, ctx),
        1 => match_node_linc5_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_linc5_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_linc5_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_linc6_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (22 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 22) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=22, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_linc6_1(bytes, ctx),
        1 => match_node_linc6_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_linc6_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_linc6_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_linc7_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (23 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 23) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=23, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_linc7_1(bytes, ctx),
        1 => match_node_linc7_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_linc7_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_linc7_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_linc8_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=24, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_linc8_1(bytes, ctx),
        1 => match_node_linc8_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_linc8_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_linc8_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_linc9_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=25, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_linc9_1(bytes, ctx),
        1 => match_node_linc9_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_linc9_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_linc9_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_lsbImm_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_mcrOperands_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_mdir_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 3;
    eprintln!("Trace node 0: SlaInstructionBits start=7, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_mdir_1(bytes, ctx),
        1 => match_node_mdir_2(bytes, ctx),
        2 => match_node_mdir_3(bytes, ctx),
        3 => match_node_mdir_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_mdir_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_mdir_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_mdir_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 3");
    3
}

fn match_node_mdir_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 1");
    1
}

fn match_node_msbImm_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_nanx_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=24, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_nanx_1(bytes, ctx),
        1 => match_node_nanx_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_nanx_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_nanx_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_optionImm_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_part2thcc_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 15;
    eprintln!("Trace node 0: SlaInstructionBits start=6, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_part2thcc_1(bytes, ctx),
        1 => match_node_part2thcc_2(bytes, ctx),
        2 => match_node_part2thcc_3(bytes, ctx),
        3 => match_node_part2thcc_4(bytes, ctx),
        4 => match_node_part2thcc_5(bytes, ctx),
        5 => match_node_part2thcc_6(bytes, ctx),
        6 => match_node_part2thcc_7(bytes, ctx),
        7 => match_node_part2thcc_8(bytes, ctx),
        8 => match_node_part2thcc_9(bytes, ctx),
        9 => match_node_part2thcc_10(bytes, ctx),
        10 => match_node_part2thcc_11(bytes, ctx),
        11 => match_node_part2thcc_12(bytes, ctx),
        12 => match_node_part2thcc_13(bytes, ctx),
        13 => match_node_part2thcc_14(bytes, ctx),
        14 => match_node_part2thcc_15(bytes, ctx),
        15 => match_node_part2thcc_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_part2thcc_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_part2thcc_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_part2thcc_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_part2thcc_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_part2thcc_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 4");
    4
}

fn match_node_part2thcc_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_part2thcc_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 6");
    6
}

fn match_node_part2thcc_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 7");
    7
}

fn match_node_part2thcc_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 8");
    8
}

fn match_node_part2thcc_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 9");
    9
}

fn match_node_part2thcc_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 10");
    10
}

fn match_node_part2thcc_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 11");
    11
}

fn match_node_part2thcc_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 12");
    12
}

fn match_node_part2thcc_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 13");
    13
}

fn match_node_part2thcc_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched NOTHING");
    -1
}

fn match_node_part2thcc_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched NOTHING");
    -1
}

fn match_node_pclbrace_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 1");
    1
}

fn match_node_pcpbrace_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 1");
    1
}

fn match_node_psbrace_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_pshlist_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_pshlist_1(bytes, ctx),
        1 => match_node_pshlist_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_pshlist_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_pshlist_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_reglist_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 5 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 31;
    eprintln!("Trace node 0: SlaInstructionBits start=7, size=5, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_reglist_1(bytes, ctx),
        1 => match_node_reglist_2(bytes, ctx),
        2 => match_node_reglist_3(bytes, ctx),
        3 => match_node_reglist_4(bytes, ctx),
        4 => match_node_reglist_5(bytes, ctx),
        5 => match_node_reglist_6(bytes, ctx),
        6 => match_node_reglist_7(bytes, ctx),
        7 => match_node_reglist_8(bytes, ctx),
        8 => match_node_reglist_9(bytes, ctx),
        9 => match_node_reglist_10(bytes, ctx),
        10 => match_node_reglist_11(bytes, ctx),
        11 => match_node_reglist_12(bytes, ctx),
        12 => match_node_reglist_13(bytes, ctx),
        13 => match_node_reglist_14(bytes, ctx),
        14 => match_node_reglist_15(bytes, ctx),
        15 => match_node_reglist_16(bytes, ctx),
        16 => match_node_reglist_17(bytes, ctx),
        17 => match_node_reglist_18(bytes, ctx),
        18 => match_node_reglist_19(bytes, ctx),
        19 => match_node_reglist_20(bytes, ctx),
        20 => match_node_reglist_21(bytes, ctx),
        21 => match_node_reglist_22(bytes, ctx),
        22 => match_node_reglist_23(bytes, ctx),
        23 => match_node_reglist_24(bytes, ctx),
        24 => match_node_reglist_25(bytes, ctx),
        25 => match_node_reglist_26(bytes, ctx),
        26 => match_node_reglist_27(bytes, ctx),
        27 => match_node_reglist_28(bytes, ctx),
        28 => match_node_reglist_29(bytes, ctx),
        29 => match_node_reglist_30(bytes, ctx),
        30 => match_node_reglist_31(bytes, ctx),
        31 => match_node_reglist_32(bytes, ctx),
        _ => -1,
    }
}

fn match_node_reglist_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 23");
    23
}

fn match_node_reglist_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 8");
    8
}

fn match_node_reglist_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 25");
    25
}

fn match_node_reglist_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 10");
    10
}

fn match_node_reglist_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 24");
    24
}

fn match_node_reglist_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 9");
    9
}

fn match_node_reglist_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched NOTHING");
    -1
}

fn match_node_reglist_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 11");
    11
}

fn match_node_reglist_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 16");
    16
}

fn match_node_reglist_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 0");
    0
}

fn match_node_reglist_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 19");
    19
}

fn match_node_reglist_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 2");
    2
}

fn match_node_reglist_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 17");
    17
}

fn match_node_reglist_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 1");
    1
}

fn match_node_reglist_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 18");
    18
}

fn match_node_reglist_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 3");
    3
}

fn match_node_reglist_17(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 17: Terminal matched constructor ID 26");
    26
}

fn match_node_reglist_18(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 18: Terminal matched constructor ID 12");
    12
}

fn match_node_reglist_19(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 19: Terminal matched constructor ID 28");
    28
}

fn match_node_reglist_20(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 20: Terminal matched constructor ID 14");
    14
}

fn match_node_reglist_21(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 21: Terminal matched constructor ID 27");
    27
}

fn match_node_reglist_22(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 22: Terminal matched constructor ID 13");
    13
}

fn match_node_reglist_23(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 23: Terminal matched NOTHING");
    -1
}

fn match_node_reglist_24(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 24: Terminal matched constructor ID 15");
    15
}

fn match_node_reglist_25(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 25: Terminal matched constructor ID 20");
    20
}

fn match_node_reglist_26(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 26: Terminal matched constructor ID 4");
    4
}

fn match_node_reglist_27(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 27: Terminal matched constructor ID 22");
    22
}

fn match_node_reglist_28(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 28: Terminal matched constructor ID 6");
    6
}

fn match_node_reglist_29(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 29: Terminal matched constructor ID 21");
    21
}

fn match_node_reglist_30(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 30: Terminal matched constructor ID 5");
    5
}

fn match_node_reglist_31(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 31: Terminal matched NOTHING");
    -1
}

fn match_node_reglist_32(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 32: Terminal matched constructor ID 7");
    7
}

fn match_node_rm_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_rn_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_ror1_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 3;
    eprintln!("Trace node 0: SlaInstructionBits start=20, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_ror1_1(bytes, ctx),
        1 => match_node_ror1_2(bytes, ctx),
        2 => match_node_ror1_3(bytes, ctx),
        3 => match_node_ror1_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_ror1_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_ror1_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_ror1_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_ror1_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_roundMode_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 0: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_roundMode_1(bytes, ctx),
        1 => match_node_roundMode_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_roundMode_1(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 1;
    eprintln!("Trace node 1: SlaInstructionBits start=24, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_roundMode_2(bytes, ctx),
        1 => match_node_roundMode_3(bytes, ctx),
        _ => -1,
    }
}

fn match_node_roundMode_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_roundMode_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 1");
    1
}

fn match_node_roundMode_4(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 1;
    eprintln!("Trace node 4: SlaInstructionBits start=24, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_roundMode_5(bytes, ctx),
        1 => match_node_roundMode_6(bytes, ctx),
        _ => -1,
    }
}

fn match_node_roundMode_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 2");
    2
}

fn match_node_roundMode_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 3");
    3
}

fn match_node_rs_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_sSatImm4_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_sSatImm5_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_sdec0_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (31 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 31) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=31, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_sdec0_1(bytes, ctx),
        1 => match_node_sdec0_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_sdec0_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_sdec0_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_sdec1_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (30 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 30) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=30, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_sdec1_1(bytes, ctx),
        1 => match_node_sdec1_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_sdec1_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_sdec1_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_sdec10_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (21 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 21) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=21, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_sdec10_1(bytes, ctx),
        1 => match_node_sdec10_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_sdec10_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_sdec10_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_sdec11_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_sdec11_1(bytes, ctx),
        1 => match_node_sdec11_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_sdec11_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_sdec11_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_sdec12_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (19 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 19) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=19, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_sdec12_1(bytes, ctx),
        1 => match_node_sdec12_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_sdec12_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_sdec12_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_sdec13_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (18 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 18) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=18, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_sdec13_1(bytes, ctx),
        1 => match_node_sdec13_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_sdec13_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_sdec13_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_sdec14_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (17 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 17) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=17, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_sdec14_1(bytes, ctx),
        1 => match_node_sdec14_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_sdec14_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_sdec14_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_sdec15_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (16 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 16) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=16, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_sdec15_1(bytes, ctx),
        1 => match_node_sdec15_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_sdec15_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_sdec15_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_sdec2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (29 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 29) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=29, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_sdec2_1(bytes, ctx),
        1 => match_node_sdec2_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_sdec2_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_sdec2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_sdec3_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (28 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 28) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=28, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_sdec3_1(bytes, ctx),
        1 => match_node_sdec3_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_sdec3_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_sdec3_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_sdec4_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (27 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 27) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=27, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_sdec4_1(bytes, ctx),
        1 => match_node_sdec4_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_sdec4_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_sdec4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_sdec5_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (26 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 26) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=26, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_sdec5_1(bytes, ctx),
        1 => match_node_sdec5_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_sdec5_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_sdec5_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_sdec6_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=25, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_sdec6_1(bytes, ctx),
        1 => match_node_sdec6_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_sdec6_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_sdec6_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_sdec7_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=24, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_sdec7_1(bytes, ctx),
        1 => match_node_sdec7_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_sdec7_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_sdec7_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_sdec8_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (23 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 23) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=23, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_sdec8_1(bytes, ctx),
        1 => match_node_sdec8_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_sdec8_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_sdec8_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_sdec9_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (22 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 22) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=22, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_sdec9_1(bytes, ctx),
        1 => match_node_sdec9_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_sdec9_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_sdec9_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_shift1_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_shift2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 3;
    eprintln!("Trace node 0: SlaInstructionBits start=25, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_shift2_1(bytes, ctx),
        1 => match_node_shift2_2(bytes, ctx),
        2 => match_node_shift2_3(bytes, ctx),
        3 => match_node_shift2_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_shift2_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_shift2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 2");
    2
}

fn match_node_shift2_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 4");
    4
}

fn match_node_shift2_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 6");
    6
}

fn match_node_shift3_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 3;
    eprintln!("Trace node 0: SlaInstructionBits start=25, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_shift3_1(bytes, ctx),
        1 => match_node_shift3_2(bytes, ctx),
        2 => match_node_shift3_3(bytes, ctx),
        3 => match_node_shift3_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_shift3_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_shift3_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_shift3_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_shift3_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_shift4_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=25, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_shift4_1(bytes, ctx),
        1 => match_node_shift4_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_shift4_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_shift4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 2");
    2
}

fn match_node_sinc0_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (16 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 16) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=16, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_sinc0_1(bytes, ctx),
        1 => match_node_sinc0_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_sinc0_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_sinc0_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_sinc1_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (17 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 17) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=17, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_sinc1_1(bytes, ctx),
        1 => match_node_sinc1_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_sinc1_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_sinc1_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_sinc10_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (26 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 26) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=26, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_sinc10_1(bytes, ctx),
        1 => match_node_sinc10_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_sinc10_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_sinc10_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_sinc11_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (27 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 27) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=27, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_sinc11_1(bytes, ctx),
        1 => match_node_sinc11_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_sinc11_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_sinc11_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_sinc12_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (28 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 28) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=28, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_sinc12_1(bytes, ctx),
        1 => match_node_sinc12_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_sinc12_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_sinc12_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_sinc13_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (29 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 29) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=29, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_sinc13_1(bytes, ctx),
        1 => match_node_sinc13_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_sinc13_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_sinc13_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_sinc14_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (30 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 30) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=30, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_sinc14_1(bytes, ctx),
        1 => match_node_sinc14_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_sinc14_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_sinc14_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_sinc15_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (31 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 31) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=31, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_sinc15_1(bytes, ctx),
        1 => match_node_sinc15_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_sinc15_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_sinc15_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_sinc2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (18 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 18) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=18, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_sinc2_1(bytes, ctx),
        1 => match_node_sinc2_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_sinc2_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_sinc2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_sinc3_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (19 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 19) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=19, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_sinc3_1(bytes, ctx),
        1 => match_node_sinc3_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_sinc3_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_sinc3_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_sinc4_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (20 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 20) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=20, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_sinc4_1(bytes, ctx),
        1 => match_node_sinc4_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_sinc4_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_sinc4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_sinc5_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (21 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 21) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=21, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_sinc5_1(bytes, ctx),
        1 => match_node_sinc5_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_sinc5_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_sinc5_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_sinc6_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (22 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 22) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=22, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_sinc6_1(bytes, ctx),
        1 => match_node_sinc6_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_sinc6_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_sinc6_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_sinc7_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (23 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 23) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=23, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_sinc7_1(bytes, ctx),
        1 => match_node_sinc7_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_sinc7_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_sinc7_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_sinc8_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (24 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 24) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=24, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_sinc8_1(bytes, ctx),
        1 => match_node_sinc8_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_sinc8_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_sinc8_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_sinc9_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (25 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 25) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=25, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_sinc9_1(bytes, ctx),
        1 => match_node_sinc9_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_sinc9_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_sinc9_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_spsrmask_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 15;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_spsrmask_1(bytes, ctx),
        1 => match_node_spsrmask_2(bytes, ctx),
        2 => match_node_spsrmask_3(bytes, ctx),
        3 => match_node_spsrmask_4(bytes, ctx),
        4 => match_node_spsrmask_5(bytes, ctx),
        5 => match_node_spsrmask_6(bytes, ctx),
        6 => match_node_spsrmask_7(bytes, ctx),
        7 => match_node_spsrmask_8(bytes, ctx),
        8 => match_node_spsrmask_9(bytes, ctx),
        9 => match_node_spsrmask_10(bytes, ctx),
        10 => match_node_spsrmask_11(bytes, ctx),
        11 => match_node_spsrmask_12(bytes, ctx),
        12 => match_node_spsrmask_13(bytes, ctx),
        13 => match_node_spsrmask_14(bytes, ctx),
        14 => match_node_spsrmask_15(bytes, ctx),
        15 => match_node_spsrmask_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_spsrmask_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_spsrmask_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_spsrmask_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_spsrmask_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_spsrmask_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 4");
    4
}

fn match_node_spsrmask_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_spsrmask_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 6");
    6
}

fn match_node_spsrmask_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 7");
    7
}

fn match_node_spsrmask_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 8");
    8
}

fn match_node_spsrmask_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 9");
    9
}

fn match_node_spsrmask_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 10");
    10
}

fn match_node_spsrmask_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 11");
    11
}

fn match_node_spsrmask_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 12");
    12
}

fn match_node_spsrmask_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 13");
    13
}

fn match_node_spsrmask_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 14");
    14
}

fn match_node_spsrmask_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 15");
    15
}

fn match_node_stbrace_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_stlist_dec_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_stlist_inc_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_strlist_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_taddrmode5_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 3;
    eprintln!("Trace node 0: SlaInstructionBits start=7, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_taddrmode5_1(bytes, ctx),
        1 => match_node_taddrmode5_2(bytes, ctx),
        2 => match_node_taddrmode5_5(bytes, ctx),
        3 => match_node_taddrmode5_8(bytes, ctx),
        _ => -1,
    }
}

fn match_node_taddrmode5_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 5");
    5
}

fn match_node_taddrmode5_2(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 2: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_taddrmode5_3(bytes, ctx),
        1 => match_node_taddrmode5_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_taddrmode5_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 6");
    6
}

fn match_node_taddrmode5_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 4");
    4
}

fn match_node_taddrmode5_5(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 5: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_taddrmode5_6(bytes, ctx),
        1 => match_node_taddrmode5_7(bytes, ctx),
        _ => -1,
    }
}

fn match_node_taddrmode5_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 1");
    1
}

fn match_node_taddrmode5_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 3");
    3
}

fn match_node_taddrmode5_8(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 8: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_taddrmode5_9(bytes, ctx),
        1 => match_node_taddrmode5_10(bytes, ctx),
        _ => -1,
    }
}

fn match_node_taddrmode5_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 0");
    0
}

fn match_node_taddrmode5_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 2");
    2
}

fn match_node_th2_SetMode_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 15;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_th2_SetMode_1(bytes, ctx),
        1 => match_node_th2_SetMode_2(bytes, ctx),
        2 => match_node_th2_SetMode_3(bytes, ctx),
        3 => match_node_th2_SetMode_4(bytes, ctx),
        4 => match_node_th2_SetMode_5(bytes, ctx),
        5 => match_node_th2_SetMode_6(bytes, ctx),
        6 => match_node_th2_SetMode_7(bytes, ctx),
        7 => match_node_th2_SetMode_8(bytes, ctx),
        8 => match_node_th2_SetMode_9(bytes, ctx),
        9 => match_node_th2_SetMode_10(bytes, ctx),
        10 => match_node_th2_SetMode_11(bytes, ctx),
        11 => match_node_th2_SetMode_12(bytes, ctx),
        12 => match_node_th2_SetMode_13(bytes, ctx),
        13 => match_node_th2_SetMode_14(bytes, ctx),
        14 => match_node_th2_SetMode_15(bytes, ctx),
        15 => match_node_th2_SetMode_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_th2_SetMode_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_th2_SetMode_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_th2_SetMode_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_th2_SetMode_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_th2_SetMode_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched NOTHING");
    -1
}

fn match_node_th2_SetMode_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched NOTHING");
    -1
}

fn match_node_th2_SetMode_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 4");
    4
}

fn match_node_th2_SetMode_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 5");
    5
}

fn match_node_th2_SetMode_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched NOTHING");
    -1
}

fn match_node_th2_SetMode_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched NOTHING");
    -1
}

fn match_node_th2_SetMode_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched NOTHING");
    -1
}

fn match_node_th2_SetMode_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 6");
    6
}

fn match_node_th2_SetMode_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched NOTHING");
    -1
}

fn match_node_th2_SetMode_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched NOTHING");
    -1
}

fn match_node_th2_SetMode_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched NOTHING");
    -1
}

fn match_node_th2_SetMode_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 7");
    7
}

fn match_node_th2_aflag_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_th2_aflag_1(bytes, ctx),
        1 => match_node_th2_aflag_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_th2_aflag_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_th2_aflag_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_th2_fflag_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_th2_fflag_1(bytes, ctx),
        1 => match_node_th2_fflag_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_th2_fflag_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_th2_fflag_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_th2_iflag_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_th2_iflag_1(bytes, ctx),
        1 => match_node_th2_iflag_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_th2_iflag_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_th2_iflag_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_th2_iflags_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_th2_shift0_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_th2_shift1_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 1");
    1
}

fn match_node_thBitWidth_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_thLsbImm_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_thMsbImm_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_thSBIT_CZN_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thSBIT_CZN_1(bytes, ctx),
        1 => match_node_thSBIT_CZN_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thSBIT_CZN_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_thSBIT_CZN_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_thSBIT_CZNO_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thSBIT_CZNO_1(bytes, ctx),
        1 => match_node_thSBIT_CZNO_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thSBIT_CZNO_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_thSBIT_CZNO_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_thSBIT_ZN_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thSBIT_ZN_1(bytes, ctx),
        1 => match_node_thSBIT_ZN_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thSBIT_ZN_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_thSBIT_ZN_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_thSRSMode_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 15;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thSRSMode_1(bytes, ctx),
        1 => match_node_thSRSMode_2(bytes, ctx),
        2 => match_node_thSRSMode_3(bytes, ctx),
        3 => match_node_thSRSMode_4(bytes, ctx),
        4 => match_node_thSRSMode_5(bytes, ctx),
        5 => match_node_thSRSMode_6(bytes, ctx),
        6 => match_node_thSRSMode_7(bytes, ctx),
        7 => match_node_thSRSMode_8(bytes, ctx),
        8 => match_node_thSRSMode_9(bytes, ctx),
        9 => match_node_thSRSMode_10(bytes, ctx),
        10 => match_node_thSRSMode_11(bytes, ctx),
        11 => match_node_thSRSMode_12(bytes, ctx),
        12 => match_node_thSRSMode_13(bytes, ctx),
        13 => match_node_thSRSMode_14(bytes, ctx),
        14 => match_node_thSRSMode_15(bytes, ctx),
        15 => match_node_thSRSMode_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thSRSMode_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 8");
    8
}

fn match_node_thSRSMode_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 8");
    8
}

fn match_node_thSRSMode_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 8");
    8
}

fn match_node_thSRSMode_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 6");
    6
}

fn match_node_thSRSMode_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 8");
    8
}

fn match_node_thSRSMode_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 8");
    8
}

fn match_node_thSRSMode_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 8");
    8
}

fn match_node_thSRSMode_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 7");
    7
}

fn match_node_thSRSMode_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 0");
    0
}

fn match_node_thSRSMode_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 1");
    1
}

fn match_node_thSRSMode_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 2");
    2
}

fn match_node_thSRSMode_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 3");
    3
}

fn match_node_thSRSMode_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 8");
    8
}

fn match_node_thSRSMode_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 8");
    8
}

fn match_node_thSRSMode_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 4");
    4
}

fn match_node_thSRSMode_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 5");
    5
}

fn match_node_thWidthMinus1_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_thXBIT_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (26 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 26) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=26, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thXBIT_1(bytes, ctx),
        1 => match_node_thXBIT_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thXBIT_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_thXBIT_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_thYBIT_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thYBIT_1(bytes, ctx),
        1 => match_node_thYBIT_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thYBIT_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_thYBIT_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_thcc_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 15;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thcc_1(bytes, ctx),
        1 => match_node_thcc_2(bytes, ctx),
        2 => match_node_thcc_3(bytes, ctx),
        3 => match_node_thcc_4(bytes, ctx),
        4 => match_node_thcc_5(bytes, ctx),
        5 => match_node_thcc_6(bytes, ctx),
        6 => match_node_thcc_7(bytes, ctx),
        7 => match_node_thcc_8(bytes, ctx),
        8 => match_node_thcc_9(bytes, ctx),
        9 => match_node_thcc_10(bytes, ctx),
        10 => match_node_thcc_11(bytes, ctx),
        11 => match_node_thcc_12(bytes, ctx),
        12 => match_node_thcc_13(bytes, ctx),
        13 => match_node_thcc_14(bytes, ctx),
        14 => match_node_thcc_15(bytes, ctx),
        15 => match_node_thcc_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thcc_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_thcc_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_thcc_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_thcc_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_thcc_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 4");
    4
}

fn match_node_thcc_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_thcc_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 6");
    6
}

fn match_node_thcc_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 7");
    7
}

fn match_node_thcc_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 8");
    8
}

fn match_node_thcc_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 9");
    9
}

fn match_node_thcc_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 10");
    10
}

fn match_node_thcc_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 11");
    11
}

fn match_node_thcc_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 12");
    12
}

fn match_node_thcc_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 13");
    13
}

fn match_node_thcc_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched NOTHING");
    -1
}

fn match_node_thcc_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched NOTHING");
    -1
}

fn match_node_thcpsrmask_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_thdXbot_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thdXbot_1(bytes, ctx),
        1 => match_node_thdXbot_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thdXbot_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_thdXbot_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_thdXtop_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thdXtop_1(bytes, ctx),
        1 => match_node_thdXtop_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thdXtop_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_thdXtop_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_thfcc_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 6) & 15;
    eprintln!("Trace node 0: SlaContextBits start=6, size=4, probe={}", probe);
    match probe {
        0 => match_node_thfcc_1(bytes, ctx),
        1 => match_node_thfcc_2(bytes, ctx),
        2 => match_node_thfcc_3(bytes, ctx),
        3 => match_node_thfcc_4(bytes, ctx),
        4 => match_node_thfcc_5(bytes, ctx),
        5 => match_node_thfcc_6(bytes, ctx),
        6 => match_node_thfcc_7(bytes, ctx),
        7 => match_node_thfcc_8(bytes, ctx),
        8 => match_node_thfcc_9(bytes, ctx),
        9 => match_node_thfcc_10(bytes, ctx),
        10 => match_node_thfcc_11(bytes, ctx),
        11 => match_node_thfcc_12(bytes, ctx),
        12 => match_node_thfcc_13(bytes, ctx),
        13 => match_node_thfcc_14(bytes, ctx),
        14 => match_node_thfcc_15(bytes, ctx),
        15 => match_node_thfcc_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thfcc_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_thfcc_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_thfcc_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_thfcc_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_thfcc_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 4");
    4
}

fn match_node_thfcc_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_thfcc_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 6");
    6
}

fn match_node_thfcc_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 7");
    7
}

fn match_node_thfcc_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 8");
    8
}

fn match_node_thfcc_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 9");
    9
}

fn match_node_thfcc_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 10");
    10
}

fn match_node_thfcc_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 11");
    11
}

fn match_node_thfcc_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 12");
    12
}

fn match_node_thfcc_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 13");
    13
}

fn match_node_thfcc_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 14");
    14
}

fn match_node_thfcc_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched NOTHING");
    -1
}

fn match_node_thldrlist_dec_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thldrlist_dec_1(bytes, ctx),
        1 => match_node_thldrlist_dec_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thldrlist_dec_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thldrlist_dec_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thldrlist_inc_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=0, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thldrlist_inc_1(bytes, ctx),
        1 => match_node_thldrlist_inc_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thldrlist_inc_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_thldrlist_inc_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thpsrmask_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 15;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thpsrmask_1(bytes, ctx),
        1 => match_node_thpsrmask_2(bytes, ctx),
        2 => match_node_thpsrmask_3(bytes, ctx),
        3 => match_node_thpsrmask_4(bytes, ctx),
        4 => match_node_thpsrmask_5(bytes, ctx),
        5 => match_node_thpsrmask_6(bytes, ctx),
        6 => match_node_thpsrmask_7(bytes, ctx),
        7 => match_node_thpsrmask_8(bytes, ctx),
        8 => match_node_thpsrmask_9(bytes, ctx),
        9 => match_node_thpsrmask_10(bytes, ctx),
        10 => match_node_thpsrmask_11(bytes, ctx),
        11 => match_node_thpsrmask_12(bytes, ctx),
        12 => match_node_thpsrmask_13(bytes, ctx),
        13 => match_node_thpsrmask_14(bytes, ctx),
        14 => match_node_thpsrmask_15(bytes, ctx),
        15 => match_node_thpsrmask_16(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thpsrmask_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_thpsrmask_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_thpsrmask_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_thpsrmask_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 3");
    3
}

fn match_node_thpsrmask_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 4");
    4
}

fn match_node_thpsrmask_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 5");
    5
}

fn match_node_thpsrmask_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 6");
    6
}

fn match_node_thpsrmask_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 7");
    7
}

fn match_node_thpsrmask_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 8");
    8
}

fn match_node_thpsrmask_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 9");
    9
}

fn match_node_thpsrmask_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 10");
    10
}

fn match_node_thpsrmask_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 11");
    11
}

fn match_node_thpsrmask_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 12");
    12
}

fn match_node_thpsrmask_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 13");
    13
}

fn match_node_thpsrmask_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 14");
    14
}

fn match_node_thpsrmask_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 15");
    15
}

fn match_node_thrldec1_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thrldec1_1(bytes, ctx),
        1 => match_node_thrldec1_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thrldec1_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thrldec1_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thrldec10_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=5, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thrldec10_1(bytes, ctx),
        1 => match_node_thrldec10_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thrldec10_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thrldec10_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thrldec11_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thrldec11_1(bytes, ctx),
        1 => match_node_thrldec11_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thrldec11_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thrldec11_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thrldec12_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=3, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thrldec12_1(bytes, ctx),
        1 => match_node_thrldec12_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thrldec12_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thrldec12_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thrldec13_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=2, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thrldec13_1(bytes, ctx),
        1 => match_node_thrldec13_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thrldec13_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thrldec13_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thrldec14_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thrldec14_1(bytes, ctx),
        1 => match_node_thrldec14_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thrldec14_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thrldec14_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thrldec15_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=0, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thrldec15_1(bytes, ctx),
        1 => match_node_thrldec15_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thrldec15_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_thrldec15_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thrldec2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thrldec2_1(bytes, ctx),
        1 => match_node_thrldec2_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thrldec2_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thrldec2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thrldec3_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thrldec3_1(bytes, ctx),
        1 => match_node_thrldec3_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thrldec3_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thrldec3_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thrldec4_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thrldec4_1(bytes, ctx),
        1 => match_node_thrldec4_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thrldec4_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thrldec4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thrldec5_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thrldec5_1(bytes, ctx),
        1 => match_node_thrldec5_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thrldec5_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thrldec5_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thrldec6_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thrldec6_1(bytes, ctx),
        1 => match_node_thrldec6_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thrldec6_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thrldec6_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thrldec7_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thrldec7_1(bytes, ctx),
        1 => match_node_thrldec7_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thrldec7_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thrldec7_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thrldec8_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thrldec8_1(bytes, ctx),
        1 => match_node_thrldec8_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thrldec8_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thrldec8_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thrldec9_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=6, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thrldec9_1(bytes, ctx),
        1 => match_node_thrldec9_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thrldec9_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thrldec9_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thrlist1_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thrlist1_1(bytes, ctx),
        1 => match_node_thrlist1_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thrlist1_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thrlist1_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thrlist10_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thrlist10_1(bytes, ctx),
        1 => match_node_thrlist10_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thrlist10_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thrlist10_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thrlist11_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thrlist11_1(bytes, ctx),
        1 => match_node_thrlist11_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thrlist11_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thrlist11_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thrlist12_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thrlist12_1(bytes, ctx),
        1 => match_node_thrlist12_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thrlist12_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thrlist12_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thrlist13_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thrlist13_1(bytes, ctx),
        1 => match_node_thrlist13_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thrlist13_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thrlist13_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thrlist14_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thrlist14_1(bytes, ctx),
        1 => match_node_thrlist14_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thrlist14_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thrlist14_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thrlist15_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thrlist15_1(bytes, ctx),
        1 => match_node_thrlist15_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thrlist15_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thrlist15_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thrlist2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=2, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thrlist2_1(bytes, ctx),
        1 => match_node_thrlist2_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thrlist2_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thrlist2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thrlist3_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=3, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thrlist3_1(bytes, ctx),
        1 => match_node_thrlist3_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thrlist3_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thrlist3_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thrlist4_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thrlist4_1(bytes, ctx),
        1 => match_node_thrlist4_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thrlist4_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thrlist4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thrlist5_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=5, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thrlist5_1(bytes, ctx),
        1 => match_node_thrlist5_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thrlist5_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thrlist5_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thrlist6_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=6, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thrlist6_1(bytes, ctx),
        1 => match_node_thrlist6_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thrlist6_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thrlist6_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thrlist7_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thrlist7_1(bytes, ctx),
        1 => match_node_thrlist7_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thrlist7_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thrlist7_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thrlist8_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thrlist8_1(bytes, ctx),
        1 => match_node_thrlist8_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thrlist8_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thrlist8_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thrlist9_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thrlist9_1(bytes, ctx),
        1 => match_node_thrlist9_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thrlist9_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thrlist9_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thsdec1_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thsdec1_1(bytes, ctx),
        1 => match_node_thsdec1_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thsdec1_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thsdec1_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thsdec10_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=5, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thsdec10_1(bytes, ctx),
        1 => match_node_thsdec10_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thsdec10_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thsdec10_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thsdec11_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thsdec11_1(bytes, ctx),
        1 => match_node_thsdec11_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thsdec11_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thsdec11_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thsdec12_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=3, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thsdec12_1(bytes, ctx),
        1 => match_node_thsdec12_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thsdec12_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thsdec12_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thsdec13_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=2, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thsdec13_1(bytes, ctx),
        1 => match_node_thsdec13_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thsdec13_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thsdec13_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thsdec14_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thsdec14_1(bytes, ctx),
        1 => match_node_thsdec14_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thsdec14_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thsdec14_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thsdec15_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=0, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thsdec15_1(bytes, ctx),
        1 => match_node_thsdec15_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thsdec15_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_thsdec15_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thsdec2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thsdec2_1(bytes, ctx),
        1 => match_node_thsdec2_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thsdec2_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thsdec2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thsdec3_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thsdec3_1(bytes, ctx),
        1 => match_node_thsdec3_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thsdec3_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thsdec3_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thsdec4_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thsdec4_1(bytes, ctx),
        1 => match_node_thsdec4_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thsdec4_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thsdec4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thsdec5_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thsdec5_1(bytes, ctx),
        1 => match_node_thsdec5_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thsdec5_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thsdec5_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thsdec6_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thsdec6_1(bytes, ctx),
        1 => match_node_thsdec6_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thsdec6_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thsdec6_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thsdec7_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thsdec7_1(bytes, ctx),
        1 => match_node_thsdec7_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thsdec7_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thsdec7_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thsdec8_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thsdec8_1(bytes, ctx),
        1 => match_node_thsdec8_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thsdec8_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thsdec8_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thsdec9_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=6, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thsdec9_1(bytes, ctx),
        1 => match_node_thsdec9_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thsdec9_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thsdec9_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thshift2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 2 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 3;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=2, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thshift2_1(bytes, ctx),
        1 => match_node_thshift2_2(bytes, ctx),
        2 => match_node_thshift2_3(bytes, ctx),
        3 => match_node_thshift2_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thshift2_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_thshift2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 2");
    2
}

fn match_node_thshift2_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 4");
    4
}

fn match_node_thshift2_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 6");
    6
}

fn match_node_thsinc1_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (1 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 1) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=1, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thsinc1_1(bytes, ctx),
        1 => match_node_thsinc1_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thsinc1_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thsinc1_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thsinc10_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thsinc10_1(bytes, ctx),
        1 => match_node_thsinc10_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thsinc10_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thsinc10_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thsinc11_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (11 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 11) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=11, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thsinc11_1(bytes, ctx),
        1 => match_node_thsinc11_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thsinc11_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thsinc11_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thsinc12_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thsinc12_1(bytes, ctx),
        1 => match_node_thsinc12_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thsinc12_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thsinc12_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thsinc13_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (13 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 13) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=13, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thsinc13_1(bytes, ctx),
        1 => match_node_thsinc13_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thsinc13_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thsinc13_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thsinc14_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (14 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 14) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=14, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thsinc14_1(bytes, ctx),
        1 => match_node_thsinc14_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thsinc14_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thsinc14_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thsinc15_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thsinc15_1(bytes, ctx),
        1 => match_node_thsinc15_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thsinc15_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thsinc15_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thsinc2_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (2 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 2) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=2, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thsinc2_1(bytes, ctx),
        1 => match_node_thsinc2_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thsinc2_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thsinc2_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thsinc3_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (3 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 3) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=3, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thsinc3_1(bytes, ctx),
        1 => match_node_thsinc3_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thsinc3_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thsinc3_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thsinc4_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (4 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 4) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=4, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thsinc4_1(bytes, ctx),
        1 => match_node_thsinc4_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thsinc4_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thsinc4_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thsinc5_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (5 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 5) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=5, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thsinc5_1(bytes, ctx),
        1 => match_node_thsinc5_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thsinc5_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thsinc5_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thsinc6_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (6 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 6) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=6, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thsinc6_1(bytes, ctx),
        1 => match_node_thsinc6_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thsinc6_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thsinc6_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thsinc7_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (7 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 7) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=7, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thsinc7_1(bytes, ctx),
        1 => match_node_thsinc7_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thsinc7_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thsinc7_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thsinc8_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thsinc8_1(bytes, ctx),
        1 => match_node_thsinc8_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thsinc8_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thsinc8_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thsinc9_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (9 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 9) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=9, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thsinc9_1(bytes, ctx),
        1 => match_node_thsinc9_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thsinc9_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thsinc9_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thspsrmask_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_thstrlist_dec_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (15 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 15) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=15, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thstrlist_dec_1(bytes, ctx),
        1 => match_node_thstrlist_dec_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thstrlist_dec_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 2");
    2
}

fn match_node_thstrlist_dec_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thstrlist_inc_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=0, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thstrlist_inc_1(bytes, ctx),
        1 => match_node_thstrlist_inc_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thstrlist_inc_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 1");
    1
}

fn match_node_thstrlist_inc_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_thumbEndianNess_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 1;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_thumbEndianNess_1(bytes, ctx),
        1 => match_node_thumbEndianNess_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_thumbEndianNess_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_thumbEndianNess_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_uSatImm4_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_uSatImm5_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_vldmDdList_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 0: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_vldmDdList_1(bytes, ctx),
        1 => match_node_vldmDdList_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_vldmDdList_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_vldmDdList_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_vldmOffset_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 0: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_vldmOffset_1(bytes, ctx),
        1 => match_node_vldmOffset_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_vldmOffset_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_vldmOffset_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_vldmRn_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 0: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_vldmRn_1(bytes, ctx),
        1 => match_node_vldmRn_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_vldmRn_1(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_vldmRn_2(bytes, ctx),
        1 => match_node_vldmRn_3(bytes, ctx),
        _ => -1,
    }
}

fn match_node_vldmRn_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_vldmRn_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 1");
    1
}

fn match_node_vldmRn_4(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 4: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_vldmRn_5(bytes, ctx),
        1 => match_node_vldmRn_6(bytes, ctx),
        _ => -1,
    }
}

fn match_node_vldmRn_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 2");
    2
}

fn match_node_vldmRn_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 3");
    3
}

fn match_node_vldmSdList_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 0: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_vldmSdList_1(bytes, ctx),
        1 => match_node_vldmSdList_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_vldmSdList_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_vldmSdList_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_vldmUpdate_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 0: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_vldmUpdate_1(bytes, ctx),
        1 => match_node_vldmUpdate_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_vldmUpdate_1(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 1: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_vldmUpdate_2(bytes, ctx),
        1 => match_node_vldmUpdate_3(bytes, ctx),
        _ => -1,
    }
}

fn match_node_vldmUpdate_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_vldmUpdate_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 1");
    1
}

fn match_node_vldmUpdate_4(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 1;
    eprintln!("Trace node 4: SlaInstructionBits start=10, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_vldmUpdate_5(bytes, ctx),
        1 => match_node_vldmUpdate_6(bytes, ctx),
        _ => -1,
    }
}

fn match_node_vldmUpdate_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 2");
    2
}

fn match_node_vldmUpdate_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 3");
    3
}

fn match_node_vldrRn_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 0: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_vldrRn_1(bytes, ctx),
        1 => match_node_vldrRn_4(bytes, ctx),
        _ => -1,
    }
}

fn match_node_vldrRn_1(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 1: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_vldrRn_2(bytes, ctx),
        1 => match_node_vldrRn_3(bytes, ctx),
        _ => -1,
    }
}

fn match_node_vldrRn_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 4");
    4
}

fn match_node_vldrRn_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 5");
    5
}

fn match_node_vldrRn_4(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (8 + 1 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 8) & 1;
    eprintln!("Trace node 4: SlaInstructionBits start=8, size=1, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_vldrRn_5(bytes, ctx),
        1 => match_node_vldrRn_6(bytes, ctx),
        _ => -1,
    }
}

fn match_node_vldrRn_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 12");
    12
}

fn match_node_vldrRn_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 13");
    13
}

fn match_node_vmrsReg_0(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (12 + 4 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 12) & 15;
    eprintln!("Trace node 0: SlaInstructionBits start=12, size=4, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_vmrsReg_1(bytes, ctx),
        1 => match_node_vmrsReg_4(bytes, ctx),
        2 => match_node_vmrsReg_7(bytes, ctx),
        3 => match_node_vmrsReg_8(bytes, ctx),
        4 => match_node_vmrsReg_9(bytes, ctx),
        5 => match_node_vmrsReg_10(bytes, ctx),
        6 => match_node_vmrsReg_13(bytes, ctx),
        7 => match_node_vmrsReg_16(bytes, ctx),
        8 => match_node_vmrsReg_19(bytes, ctx),
        9 => match_node_vmrsReg_22(bytes, ctx),
        10 => match_node_vmrsReg_25(bytes, ctx),
        11 => match_node_vmrsReg_28(bytes, ctx),
        12 => match_node_vmrsReg_29(bytes, ctx),
        13 => match_node_vmrsReg_30(bytes, ctx),
        14 => match_node_vmrsReg_31(bytes, ctx),
        15 => match_node_vmrsReg_32(bytes, ctx),
        _ => -1,
    }
}

fn match_node_vmrsReg_1(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 1: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_vmrsReg_2(bytes, ctx),
        1 => match_node_vmrsReg_3(bytes, ctx),
        _ => -1,
    }
}

fn match_node_vmrsReg_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 0");
    0
}

fn match_node_vmrsReg_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 0");
    0
}

fn match_node_vmrsReg_4(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 4: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_vmrsReg_5(bytes, ctx),
        1 => match_node_vmrsReg_6(bytes, ctx),
        _ => -1,
    }
}

fn match_node_vmrsReg_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 1");
    1
}

fn match_node_vmrsReg_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 1");
    1
}

fn match_node_vmrsReg_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched NOTHING");
    -1
}

fn match_node_vmrsReg_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched NOTHING");
    -1
}

fn match_node_vmrsReg_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched NOTHING");
    -1
}

fn match_node_vmrsReg_10(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 10: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_vmrsReg_11(bytes, ctx),
        1 => match_node_vmrsReg_12(bytes, ctx),
        _ => -1,
    }
}

fn match_node_vmrsReg_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 2");
    2
}

fn match_node_vmrsReg_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 2");
    2
}

fn match_node_vmrsReg_13(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 13: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_vmrsReg_14(bytes, ctx),
        1 => match_node_vmrsReg_15(bytes, ctx),
        _ => -1,
    }
}

fn match_node_vmrsReg_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 3");
    3
}

fn match_node_vmrsReg_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 3");
    3
}

fn match_node_vmrsReg_16(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 16: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_vmrsReg_17(bytes, ctx),
        1 => match_node_vmrsReg_18(bytes, ctx),
        _ => -1,
    }
}

fn match_node_vmrsReg_17(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 17: Terminal matched constructor ID 4");
    4
}

fn match_node_vmrsReg_18(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 18: Terminal matched constructor ID 4");
    4
}

fn match_node_vmrsReg_19(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 19: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_vmrsReg_20(bytes, ctx),
        1 => match_node_vmrsReg_21(bytes, ctx),
        _ => -1,
    }
}

fn match_node_vmrsReg_20(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 20: Terminal matched constructor ID 5");
    5
}

fn match_node_vmrsReg_21(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 21: Terminal matched constructor ID 5");
    5
}

fn match_node_vmrsReg_22(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 22: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_vmrsReg_23(bytes, ctx),
        1 => match_node_vmrsReg_24(bytes, ctx),
        _ => -1,
    }
}

fn match_node_vmrsReg_23(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 23: Terminal matched constructor ID 6");
    6
}

fn match_node_vmrsReg_24(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 24: Terminal matched constructor ID 6");
    6
}

fn match_node_vmrsReg_25(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 25: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_vmrsReg_26(bytes, ctx),
        1 => match_node_vmrsReg_27(bytes, ctx),
        _ => -1,
    }
}

fn match_node_vmrsReg_26(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 26: Terminal matched constructor ID 7");
    7
}

fn match_node_vmrsReg_27(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 27: Terminal matched constructor ID 7");
    7
}

fn match_node_vmrsReg_28(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 28: Terminal matched NOTHING");
    -1
}

fn match_node_vmrsReg_29(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 29: Terminal matched NOTHING");
    -1
}

fn match_node_vmrsReg_30(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 30: Terminal matched NOTHING");
    -1
}

fn match_node_vmrsReg_31(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 31: Terminal matched NOTHING");
    -1
}

fn match_node_vmrsReg_32(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 32: Terminal matched NOTHING");
    -1
}

fn match_node_vpopSd64List_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 0: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_vpopSd64List_1(bytes, ctx),
        1 => match_node_vpopSd64List_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_vpopSd64List_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_vpopSd64List_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_vpopSdList_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 0: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_vpopSdList_1(bytes, ctx),
        1 => match_node_vpopSdList_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_vpopSdList_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_vpopSdList_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_vpushSd64List_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 0: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_vpushSd64List_1(bytes, ctx),
        1 => match_node_vpushSd64List_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_vpushSd64List_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_vpushSd64List_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_vpushSdList_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 0: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_vpushSdList_1(bytes, ctx),
        1 => match_node_vpushSdList_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_vpushSdList_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_vpushSdList_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_vstmDdList_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 0: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_vstmDdList_1(bytes, ctx),
        1 => match_node_vstmDdList_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_vstmDdList_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_vstmDdList_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_vstmSdList_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 0) & 1;
    eprintln!("Trace node 0: SlaContextBits start=0, size=1, probe={}", probe);
    match probe {
        0 => match_node_vstmSdList_1(bytes, ctx),
        1 => match_node_vstmSdList_2(bytes, ctx),
        _ => -1,
    }
}

fn match_node_vstmSdList_1(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 1: Terminal matched constructor ID 0");
    0
}

fn match_node_vstmSdList_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 1");
    1
}

fn match_node_widthMinus1_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_zero_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

