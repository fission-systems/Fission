// Auto-generated Fission Native Backend
#[no_mangle]
pub extern "C" fn fission_decode_match(table_ptr: *const i8, bytes: *const u8, bytes_len: usize, ctx_ptr: *const u64) -> i32 {
    let table_name = unsafe { std::ffi::CStr::from_ptr(table_ptr).to_str().unwrap() };
    let bytes = unsafe { std::slice::from_raw_parts(bytes, bytes_len) };
    let ctx = unsafe { *ctx_ptr };
    match table_name {
        "*[localVariableArray]" => match_node___localVariableArray__0(bytes, ctx),
        "Branch" => match_node_Branch_0(bytes, ctx),
        "Branch_w" => match_node_Branch_w_0(bytes, ctx),
        "Default" => match_node_Default_0(bytes, ctx),
        "LookupSwitch_match" => match_node_LookupSwitch_match_0(bytes, ctx),
        "Switch_offset" => match_node_Switch_offset_0(bytes, ctx),
        "_arrayref" => match_node__arrayref_0(bytes, ctx),
        "_count" => match_node__count_0(bytes, ctx),
        "_object" => match_node__object_0(bytes, ctx),
        "_ref" => match_node__ref_0(bytes, ctx),
        "_res" => match_node__res_0(bytes, ctx),
        "_result" => match_node__result_0(bytes, ctx),
        "_value" => match_node__value_0(bytes, ctx),
        "dolookupswitch" => match_node_dolookupswitch_0(bytes, ctx),
        "dotableswitch" => match_node_dotableswitch_0(bytes, ctx),
        "fullConstant" => match_node_fullConstant_0(bytes, ctx),
        "fullIndex" => match_node_fullIndex_0(bytes, ctx),
        "getFieldCallOther(index" => match_node_getFieldCallOther_index_0(bytes, ctx),
        "getStaticCallOther(index" => match_node_getStaticCallOther_index_0(bytes, ctx),
        "instruction" => match_node_instruction_0(bytes, ctx),
        "intVal" => match_node_intVal_0(bytes, ctx),
        "invokedynamicCallOther(index" => match_node_invokedynamicCallOther_index_0(bytes, ctx),
        "invokeinterfaceCallOther(index" => match_node_invokeinterfaceCallOther_index_0(bytes, ctx),
        "invokespecialCallOther(index" => match_node_invokespecialCallOther_index_0(bytes, ctx),
        "invokestaticCallOther(index" => match_node_invokestaticCallOther_index_0(bytes, ctx),
        "invokevirtualCallOther(index" => match_node_invokevirtualCallOther_index_0(bytes, ctx),
        "ldc2_wCallOther(index" => match_node_ldc2_wCallOther_index_0(bytes, ctx),
        "ldc_wCallOther(index" => match_node_ldc_wCallOther_index_0(bytes, ctx),
        "multianewarrayCallOther(index" => match_node_multianewarrayCallOther_index_0(bytes, ctx),
        "padSwitch" => match_node_padSwitch_0(bytes, ctx),
        "putFieldCallOther(index" => match_node_putFieldCallOther_index_0(bytes, ctx),
        "putStaticCallOther(index" => match_node_putStaticCallOther_index_0(bytes, ctx),
        _ => -1
    }
}

fn match_node___localVariableArray__0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_Branch_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_Branch_w_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_Default_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_LookupSwitch_match_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_Switch_offset_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node__arrayref_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node__count_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node__object_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node__ref_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node__res_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node__result_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node__value_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_dolookupswitch_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_dotableswitch_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_fullConstant_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_fullIndex_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_getFieldCallOther_index_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_getStaticCallOther_index_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_0(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 97) & 7;
    eprintln!("Trace node 0: SlaContextBits start=97, size=3, probe={}", probe);
    match probe {
        0 => match_node_instruction_1(bytes, ctx),
        1 => match_node_instruction_330(bytes, ctx),
        2 => match_node_instruction_331(bytes, ctx),
        3 => match_node_instruction_332(bytes, ctx),
        4 => match_node_instruction_333(bytes, ctx),
        5 => match_node_instruction_334(bytes, ctx),
        6 => match_node_instruction_335(bytes, ctx),
        7 => match_node_instruction_336(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_1(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (0 + 8 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 0) & 255;
    eprintln!("Trace node 1: SlaInstructionBits start=0, size=8, word={:08x}, probe={}", word, probe);
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
        16 => match_node_instruction_18(bytes, ctx),
        17 => match_node_instruction_19(bytes, ctx),
        18 => match_node_instruction_20(bytes, ctx),
        19 => match_node_instruction_21(bytes, ctx),
        20 => match_node_instruction_22(bytes, ctx),
        21 => match_node_instruction_23(bytes, ctx),
        22 => match_node_instruction_24(bytes, ctx),
        23 => match_node_instruction_25(bytes, ctx),
        24 => match_node_instruction_26(bytes, ctx),
        25 => match_node_instruction_27(bytes, ctx),
        26 => match_node_instruction_28(bytes, ctx),
        27 => match_node_instruction_29(bytes, ctx),
        28 => match_node_instruction_30(bytes, ctx),
        29 => match_node_instruction_31(bytes, ctx),
        30 => match_node_instruction_32(bytes, ctx),
        31 => match_node_instruction_33(bytes, ctx),
        32 => match_node_instruction_34(bytes, ctx),
        33 => match_node_instruction_35(bytes, ctx),
        34 => match_node_instruction_36(bytes, ctx),
        35 => match_node_instruction_37(bytes, ctx),
        36 => match_node_instruction_38(bytes, ctx),
        37 => match_node_instruction_39(bytes, ctx),
        38 => match_node_instruction_40(bytes, ctx),
        39 => match_node_instruction_41(bytes, ctx),
        40 => match_node_instruction_42(bytes, ctx),
        41 => match_node_instruction_43(bytes, ctx),
        42 => match_node_instruction_44(bytes, ctx),
        43 => match_node_instruction_45(bytes, ctx),
        44 => match_node_instruction_46(bytes, ctx),
        45 => match_node_instruction_47(bytes, ctx),
        46 => match_node_instruction_48(bytes, ctx),
        47 => match_node_instruction_49(bytes, ctx),
        48 => match_node_instruction_50(bytes, ctx),
        49 => match_node_instruction_51(bytes, ctx),
        50 => match_node_instruction_52(bytes, ctx),
        51 => match_node_instruction_53(bytes, ctx),
        52 => match_node_instruction_54(bytes, ctx),
        53 => match_node_instruction_55(bytes, ctx),
        54 => match_node_instruction_56(bytes, ctx),
        55 => match_node_instruction_57(bytes, ctx),
        56 => match_node_instruction_58(bytes, ctx),
        57 => match_node_instruction_59(bytes, ctx),
        58 => match_node_instruction_60(bytes, ctx),
        59 => match_node_instruction_61(bytes, ctx),
        60 => match_node_instruction_62(bytes, ctx),
        61 => match_node_instruction_63(bytes, ctx),
        62 => match_node_instruction_64(bytes, ctx),
        63 => match_node_instruction_65(bytes, ctx),
        64 => match_node_instruction_66(bytes, ctx),
        65 => match_node_instruction_67(bytes, ctx),
        66 => match_node_instruction_68(bytes, ctx),
        67 => match_node_instruction_69(bytes, ctx),
        68 => match_node_instruction_70(bytes, ctx),
        69 => match_node_instruction_71(bytes, ctx),
        70 => match_node_instruction_72(bytes, ctx),
        71 => match_node_instruction_73(bytes, ctx),
        72 => match_node_instruction_74(bytes, ctx),
        73 => match_node_instruction_75(bytes, ctx),
        74 => match_node_instruction_76(bytes, ctx),
        75 => match_node_instruction_77(bytes, ctx),
        76 => match_node_instruction_78(bytes, ctx),
        77 => match_node_instruction_79(bytes, ctx),
        78 => match_node_instruction_80(bytes, ctx),
        79 => match_node_instruction_81(bytes, ctx),
        80 => match_node_instruction_82(bytes, ctx),
        81 => match_node_instruction_83(bytes, ctx),
        82 => match_node_instruction_84(bytes, ctx),
        83 => match_node_instruction_85(bytes, ctx),
        84 => match_node_instruction_86(bytes, ctx),
        85 => match_node_instruction_87(bytes, ctx),
        86 => match_node_instruction_88(bytes, ctx),
        87 => match_node_instruction_89(bytes, ctx),
        88 => match_node_instruction_90(bytes, ctx),
        89 => match_node_instruction_91(bytes, ctx),
        90 => match_node_instruction_92(bytes, ctx),
        91 => match_node_instruction_93(bytes, ctx),
        92 => match_node_instruction_94(bytes, ctx),
        93 => match_node_instruction_95(bytes, ctx),
        94 => match_node_instruction_96(bytes, ctx),
        95 => match_node_instruction_97(bytes, ctx),
        96 => match_node_instruction_98(bytes, ctx),
        97 => match_node_instruction_99(bytes, ctx),
        98 => match_node_instruction_100(bytes, ctx),
        99 => match_node_instruction_101(bytes, ctx),
        100 => match_node_instruction_102(bytes, ctx),
        101 => match_node_instruction_103(bytes, ctx),
        102 => match_node_instruction_104(bytes, ctx),
        103 => match_node_instruction_105(bytes, ctx),
        104 => match_node_instruction_106(bytes, ctx),
        105 => match_node_instruction_107(bytes, ctx),
        106 => match_node_instruction_108(bytes, ctx),
        107 => match_node_instruction_109(bytes, ctx),
        108 => match_node_instruction_110(bytes, ctx),
        109 => match_node_instruction_111(bytes, ctx),
        110 => match_node_instruction_112(bytes, ctx),
        111 => match_node_instruction_113(bytes, ctx),
        112 => match_node_instruction_114(bytes, ctx),
        113 => match_node_instruction_115(bytes, ctx),
        114 => match_node_instruction_116(bytes, ctx),
        115 => match_node_instruction_117(bytes, ctx),
        116 => match_node_instruction_118(bytes, ctx),
        117 => match_node_instruction_119(bytes, ctx),
        118 => match_node_instruction_120(bytes, ctx),
        119 => match_node_instruction_121(bytes, ctx),
        120 => match_node_instruction_122(bytes, ctx),
        121 => match_node_instruction_123(bytes, ctx),
        122 => match_node_instruction_124(bytes, ctx),
        123 => match_node_instruction_125(bytes, ctx),
        124 => match_node_instruction_126(bytes, ctx),
        125 => match_node_instruction_127(bytes, ctx),
        126 => match_node_instruction_128(bytes, ctx),
        127 => match_node_instruction_129(bytes, ctx),
        128 => match_node_instruction_130(bytes, ctx),
        129 => match_node_instruction_131(bytes, ctx),
        130 => match_node_instruction_132(bytes, ctx),
        131 => match_node_instruction_133(bytes, ctx),
        132 => match_node_instruction_134(bytes, ctx),
        133 => match_node_instruction_135(bytes, ctx),
        134 => match_node_instruction_136(bytes, ctx),
        135 => match_node_instruction_137(bytes, ctx),
        136 => match_node_instruction_138(bytes, ctx),
        137 => match_node_instruction_139(bytes, ctx),
        138 => match_node_instruction_140(bytes, ctx),
        139 => match_node_instruction_141(bytes, ctx),
        140 => match_node_instruction_142(bytes, ctx),
        141 => match_node_instruction_143(bytes, ctx),
        142 => match_node_instruction_144(bytes, ctx),
        143 => match_node_instruction_145(bytes, ctx),
        144 => match_node_instruction_146(bytes, ctx),
        145 => match_node_instruction_147(bytes, ctx),
        146 => match_node_instruction_148(bytes, ctx),
        147 => match_node_instruction_149(bytes, ctx),
        148 => match_node_instruction_150(bytes, ctx),
        149 => match_node_instruction_151(bytes, ctx),
        150 => match_node_instruction_152(bytes, ctx),
        151 => match_node_instruction_153(bytes, ctx),
        152 => match_node_instruction_154(bytes, ctx),
        153 => match_node_instruction_155(bytes, ctx),
        154 => match_node_instruction_156(bytes, ctx),
        155 => match_node_instruction_157(bytes, ctx),
        156 => match_node_instruction_158(bytes, ctx),
        157 => match_node_instruction_159(bytes, ctx),
        158 => match_node_instruction_160(bytes, ctx),
        159 => match_node_instruction_161(bytes, ctx),
        160 => match_node_instruction_162(bytes, ctx),
        161 => match_node_instruction_163(bytes, ctx),
        162 => match_node_instruction_164(bytes, ctx),
        163 => match_node_instruction_165(bytes, ctx),
        164 => match_node_instruction_166(bytes, ctx),
        165 => match_node_instruction_167(bytes, ctx),
        166 => match_node_instruction_168(bytes, ctx),
        167 => match_node_instruction_169(bytes, ctx),
        168 => match_node_instruction_170(bytes, ctx),
        169 => match_node_instruction_171(bytes, ctx),
        170 => match_node_instruction_172(bytes, ctx),
        171 => match_node_instruction_177(bytes, ctx),
        172 => match_node_instruction_182(bytes, ctx),
        173 => match_node_instruction_183(bytes, ctx),
        174 => match_node_instruction_184(bytes, ctx),
        175 => match_node_instruction_185(bytes, ctx),
        176 => match_node_instruction_186(bytes, ctx),
        177 => match_node_instruction_187(bytes, ctx),
        178 => match_node_instruction_188(bytes, ctx),
        179 => match_node_instruction_189(bytes, ctx),
        180 => match_node_instruction_190(bytes, ctx),
        181 => match_node_instruction_191(bytes, ctx),
        182 => match_node_instruction_192(bytes, ctx),
        183 => match_node_instruction_193(bytes, ctx),
        184 => match_node_instruction_194(bytes, ctx),
        185 => match_node_instruction_195(bytes, ctx),
        186 => match_node_instruction_196(bytes, ctx),
        187 => match_node_instruction_197(bytes, ctx),
        188 => match_node_instruction_198(bytes, ctx),
        189 => match_node_instruction_199(bytes, ctx),
        190 => match_node_instruction_200(bytes, ctx),
        191 => match_node_instruction_201(bytes, ctx),
        192 => match_node_instruction_202(bytes, ctx),
        193 => match_node_instruction_203(bytes, ctx),
        194 => match_node_instruction_204(bytes, ctx),
        195 => match_node_instruction_205(bytes, ctx),
        196 => match_node_instruction_206(bytes, ctx),
        197 => match_node_instruction_271(bytes, ctx),
        198 => match_node_instruction_272(bytes, ctx),
        199 => match_node_instruction_273(bytes, ctx),
        200 => match_node_instruction_274(bytes, ctx),
        201 => match_node_instruction_275(bytes, ctx),
        202 => match_node_instruction_276(bytes, ctx),
        203 => match_node_instruction_277(bytes, ctx),
        204 => match_node_instruction_278(bytes, ctx),
        205 => match_node_instruction_279(bytes, ctx),
        206 => match_node_instruction_280(bytes, ctx),
        207 => match_node_instruction_281(bytes, ctx),
        208 => match_node_instruction_282(bytes, ctx),
        209 => match_node_instruction_283(bytes, ctx),
        210 => match_node_instruction_284(bytes, ctx),
        211 => match_node_instruction_285(bytes, ctx),
        212 => match_node_instruction_286(bytes, ctx),
        213 => match_node_instruction_287(bytes, ctx),
        214 => match_node_instruction_288(bytes, ctx),
        215 => match_node_instruction_289(bytes, ctx),
        216 => match_node_instruction_290(bytes, ctx),
        217 => match_node_instruction_291(bytes, ctx),
        218 => match_node_instruction_292(bytes, ctx),
        219 => match_node_instruction_293(bytes, ctx),
        220 => match_node_instruction_294(bytes, ctx),
        221 => match_node_instruction_295(bytes, ctx),
        222 => match_node_instruction_296(bytes, ctx),
        223 => match_node_instruction_297(bytes, ctx),
        224 => match_node_instruction_298(bytes, ctx),
        225 => match_node_instruction_299(bytes, ctx),
        226 => match_node_instruction_300(bytes, ctx),
        227 => match_node_instruction_301(bytes, ctx),
        228 => match_node_instruction_302(bytes, ctx),
        229 => match_node_instruction_303(bytes, ctx),
        230 => match_node_instruction_304(bytes, ctx),
        231 => match_node_instruction_305(bytes, ctx),
        232 => match_node_instruction_306(bytes, ctx),
        233 => match_node_instruction_307(bytes, ctx),
        234 => match_node_instruction_308(bytes, ctx),
        235 => match_node_instruction_309(bytes, ctx),
        236 => match_node_instruction_310(bytes, ctx),
        237 => match_node_instruction_311(bytes, ctx),
        238 => match_node_instruction_312(bytes, ctx),
        239 => match_node_instruction_313(bytes, ctx),
        240 => match_node_instruction_314(bytes, ctx),
        241 => match_node_instruction_315(bytes, ctx),
        242 => match_node_instruction_316(bytes, ctx),
        243 => match_node_instruction_317(bytes, ctx),
        244 => match_node_instruction_318(bytes, ctx),
        245 => match_node_instruction_319(bytes, ctx),
        246 => match_node_instruction_320(bytes, ctx),
        247 => match_node_instruction_321(bytes, ctx),
        248 => match_node_instruction_322(bytes, ctx),
        249 => match_node_instruction_323(bytes, ctx),
        250 => match_node_instruction_324(bytes, ctx),
        251 => match_node_instruction_325(bytes, ctx),
        252 => match_node_instruction_326(bytes, ctx),
        253 => match_node_instruction_327(bytes, ctx),
        254 => match_node_instruction_328(bytes, ctx),
        255 => match_node_instruction_329(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_2(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 2: Terminal matched constructor ID 194");
    194
}

fn match_node_instruction_3(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 3: Terminal matched constructor ID 2");
    2
}

fn match_node_instruction_4(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 4: Terminal matched constructor ID 96");
    96
}

fn match_node_instruction_5(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 5: Terminal matched constructor ID 97");
    97
}

fn match_node_instruction_6(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 6: Terminal matched constructor ID 98");
    98
}

fn match_node_instruction_7(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 7: Terminal matched constructor ID 99");
    99
}

fn match_node_instruction_8(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 8: Terminal matched constructor ID 100");
    100
}

fn match_node_instruction_9(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 9: Terminal matched constructor ID 101");
    101
}

fn match_node_instruction_10(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 10: Terminal matched constructor ID 102");
    102
}

fn match_node_instruction_11(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 11: Terminal matched constructor ID 157");
    157
}

fn match_node_instruction_12(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 12: Terminal matched constructor ID 158");
    158
}

fn match_node_instruction_13(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 13: Terminal matched constructor ID 63");
    63
}

fn match_node_instruction_14(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 14: Terminal matched constructor ID 64");
    64
}

fn match_node_instruction_15(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 15: Terminal matched constructor ID 65");
    65
}

fn match_node_instruction_16(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 16: Terminal matched constructor ID 31");
    31
}

fn match_node_instruction_17(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 17: Terminal matched constructor ID 32");
    32
}

fn match_node_instruction_18(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 18: Terminal matched constructor ID 19");
    19
}

fn match_node_instruction_19(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 19: Terminal matched constructor ID 203");
    203
}

fn match_node_instruction_20(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 20: Terminal matched constructor ID 159");
    159
}

fn match_node_instruction_21(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 21: Terminal matched constructor ID 160");
    160
}

fn match_node_instruction_22(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 22: Terminal matched constructor ID 161");
    161
}

fn match_node_instruction_23(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 23: Terminal matched constructor ID 121");
    121
}

fn match_node_instruction_24(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 24: Terminal matched constructor ID 163");
    163
}

fn match_node_instruction_25(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 25: Terminal matched constructor ID 67");
    67
}

fn match_node_instruction_26(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 26: Terminal matched constructor ID 34");
    34
}

fn match_node_instruction_27(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 27: Terminal matched constructor ID 3");
    3
}

fn match_node_instruction_28(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 28: Terminal matched constructor ID 122");
    122
}

fn match_node_instruction_29(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 29: Terminal matched constructor ID 123");
    123
}

fn match_node_instruction_30(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 30: Terminal matched constructor ID 124");
    124
}

fn match_node_instruction_31(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 31: Terminal matched constructor ID 125");
    125
}

fn match_node_instruction_32(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 32: Terminal matched constructor ID 164");
    164
}

fn match_node_instruction_33(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 33: Terminal matched constructor ID 165");
    165
}

fn match_node_instruction_34(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 34: Terminal matched constructor ID 166");
    166
}

fn match_node_instruction_35(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 35: Terminal matched constructor ID 167");
    167
}

fn match_node_instruction_36(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 36: Terminal matched constructor ID 68");
    68
}

fn match_node_instruction_37(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 37: Terminal matched constructor ID 69");
    69
}

fn match_node_instruction_38(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 38: Terminal matched constructor ID 70");
    70
}

fn match_node_instruction_39(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 39: Terminal matched constructor ID 71");
    71
}

fn match_node_instruction_40(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 40: Terminal matched constructor ID 35");
    35
}

fn match_node_instruction_41(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 41: Terminal matched constructor ID 36");
    36
}

fn match_node_instruction_42(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 42: Terminal matched constructor ID 37");
    37
}

fn match_node_instruction_43(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 43: Terminal matched constructor ID 38");
    38
}

fn match_node_instruction_44(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 44: Terminal matched constructor ID 4");
    4
}

fn match_node_instruction_45(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 45: Terminal matched constructor ID 5");
    5
}

fn match_node_instruction_46(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 46: Terminal matched constructor ID 6");
    6
}

fn match_node_instruction_47(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 47: Terminal matched constructor ID 7");
    7
}

fn match_node_instruction_48(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 48: Terminal matched constructor ID 93");
    93
}

fn match_node_instruction_49(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 49: Terminal matched constructor ID 153");
    153
}

fn match_node_instruction_50(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 50: Terminal matched constructor ID 59");
    59
}

fn match_node_instruction_51(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 51: Terminal matched constructor ID 27");
    27
}

fn match_node_instruction_52(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 52: Terminal matched constructor ID 0");
    0
}

fn match_node_instruction_53(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 53: Terminal matched constructor ID 17");
    17
}

fn match_node_instruction_54(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 54: Terminal matched constructor ID 20");
    20
}

fn match_node_instruction_55(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 55: Terminal matched constructor ID 201");
    201
}

fn match_node_instruction_56(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 56: Terminal matched constructor ID 139");
    139
}

fn match_node_instruction_57(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 57: Terminal matched constructor ID 181");
    181
}

fn match_node_instruction_58(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 58: Terminal matched constructor ID 76");
    76
}

fn match_node_instruction_59(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 59: Terminal matched constructor ID 43");
    43
}

fn match_node_instruction_60(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 60: Terminal matched constructor ID 11");
    11
}

fn match_node_instruction_61(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 61: Terminal matched constructor ID 140");
    140
}

fn match_node_instruction_62(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 62: Terminal matched constructor ID 141");
    141
}

fn match_node_instruction_63(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 63: Terminal matched constructor ID 142");
    142
}

fn match_node_instruction_64(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 64: Terminal matched constructor ID 143");
    143
}

fn match_node_instruction_65(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 65: Terminal matched constructor ID 182");
    182
}

fn match_node_instruction_66(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 66: Terminal matched constructor ID 183");
    183
}

fn match_node_instruction_67(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 67: Terminal matched constructor ID 184");
    184
}

fn match_node_instruction_68(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 68: Terminal matched constructor ID 185");
    185
}

fn match_node_instruction_69(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 69: Terminal matched constructor ID 77");
    77
}

fn match_node_instruction_70(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 70: Terminal matched constructor ID 78");
    78
}

fn match_node_instruction_71(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 71: Terminal matched constructor ID 79");
    79
}

fn match_node_instruction_72(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 72: Terminal matched constructor ID 80");
    80
}

fn match_node_instruction_73(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 73: Terminal matched constructor ID 44");
    44
}

fn match_node_instruction_74(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 74: Terminal matched constructor ID 45");
    45
}

fn match_node_instruction_75(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 75: Terminal matched constructor ID 46");
    46
}

fn match_node_instruction_76(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 76: Terminal matched constructor ID 47");
    47
}

fn match_node_instruction_77(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 77: Terminal matched constructor ID 12");
    12
}

fn match_node_instruction_78(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 78: Terminal matched constructor ID 13");
    13
}

fn match_node_instruction_79(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 79: Terminal matched constructor ID 14");
    14
}

fn match_node_instruction_80(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 80: Terminal matched constructor ID 15");
    15
}

fn match_node_instruction_81(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 81: Terminal matched constructor ID 95");
    95
}

fn match_node_instruction_82(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 82: Terminal matched constructor ID 155");
    155
}

fn match_node_instruction_83(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 83: Terminal matched constructor ID 60");
    60
}

fn match_node_instruction_84(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 84: Terminal matched constructor ID 28");
    28
}

fn match_node_instruction_85(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 85: Terminal matched constructor ID 1");
    1
}

fn match_node_instruction_86(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 86: Terminal matched constructor ID 18");
    18
}

fn match_node_instruction_87(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 87: Terminal matched constructor ID 21");
    21
}

fn match_node_instruction_88(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 88: Terminal matched constructor ID 202");
    202
}

fn match_node_instruction_89(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 89: Terminal matched constructor ID 195");
    195
}

fn match_node_instruction_90(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 90: Terminal matched constructor ID 196");
    196
}

fn match_node_instruction_91(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 91: Terminal matched constructor ID 49");
    49
}

fn match_node_instruction_92(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 92: Terminal matched constructor ID 50");
    50
}

fn match_node_instruction_93(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 93: Terminal matched constructor ID 51");
    51
}

fn match_node_instruction_94(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 94: Terminal matched constructor ID 52");
    52
}

fn match_node_instruction_95(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 95: Terminal matched constructor ID 53");
    53
}

fn match_node_instruction_96(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 96: Terminal matched constructor ID 54");
    54
}

fn match_node_instruction_97(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 97: Terminal matched constructor ID 204");
    204
}

fn match_node_instruction_98(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 98: Terminal matched constructor ID 92");
    92
}

fn match_node_instruction_99(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 99: Terminal matched constructor ID 152");
    152
}

fn match_node_instruction_100(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 100: Terminal matched constructor ID 58");
    58
}

fn match_node_instruction_101(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 101: Terminal matched constructor ID 26");
    26
}

fn match_node_instruction_102(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 102: Terminal matched constructor ID 144");
    144
}

fn match_node_instruction_103(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 103: Terminal matched constructor ID 186");
    186
}

fn match_node_instruction_104(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 104: Terminal matched constructor ID 81");
    81
}

fn match_node_instruction_105(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 105: Terminal matched constructor ID 48");
    48
}

fn match_node_instruction_106(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 106: Terminal matched constructor ID 126");
    126
}

fn match_node_instruction_107(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 107: Terminal matched constructor ID 168");
    168
}

fn match_node_instruction_108(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 108: Terminal matched constructor ID 72");
    72
}

fn match_node_instruction_109(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 109: Terminal matched constructor ID 39");
    39
}

fn match_node_instruction_110(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 110: Terminal matched constructor ID 103");
    103
}

fn match_node_instruction_111(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 111: Terminal matched constructor ID 162");
    162
}

fn match_node_instruction_112(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 112: Terminal matched constructor ID 66");
    66
}

fn match_node_instruction_113(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 113: Terminal matched constructor ID 33");
    33
}

fn match_node_instruction_114(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 114: Terminal matched constructor ID 135");
    135
}

fn match_node_instruction_115(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 115: Terminal matched constructor ID 177");
    177
}

fn match_node_instruction_116(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 116: Terminal matched constructor ID 74");
    74
}

fn match_node_instruction_117(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 117: Terminal matched constructor ID 41");
    41
}

fn match_node_instruction_118(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 118: Terminal matched constructor ID 127");
    127
}

fn match_node_instruction_119(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 119: Terminal matched constructor ID 169");
    169
}

fn match_node_instruction_120(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 120: Terminal matched constructor ID 73");
    73
}

fn match_node_instruction_121(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 121: Terminal matched constructor ID 40");
    40
}

fn match_node_instruction_122(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 122: Terminal matched constructor ID 137");
    137
}

fn match_node_instruction_123(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 123: Terminal matched constructor ID 179");
    179
}

fn match_node_instruction_124(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 124: Terminal matched constructor ID 138");
    138
}

fn match_node_instruction_125(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 125: Terminal matched constructor ID 180");
    180
}

fn match_node_instruction_126(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 126: Terminal matched constructor ID 145");
    145
}

fn match_node_instruction_127(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 127: Terminal matched constructor ID 187");
    187
}

fn match_node_instruction_128(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 128: Terminal matched constructor ID 94");
    94
}

fn match_node_instruction_129(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 129: Terminal matched constructor ID 154");
    154
}

fn match_node_instruction_130(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 130: Terminal matched constructor ID 134");
    134
}

fn match_node_instruction_131(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 131: Terminal matched constructor ID 176");
    176
}

fn match_node_instruction_132(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 132: Terminal matched constructor ID 146");
    146
}

fn match_node_instruction_133(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 133: Terminal matched constructor ID 188");
    188
}

fn match_node_instruction_134(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 134: Terminal matched constructor ID 120");
    120
}

fn match_node_instruction_135(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 135: Terminal matched constructor ID 90");
    90
}

fn match_node_instruction_136(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 136: Terminal matched constructor ID 89");
    89
}

fn match_node_instruction_137(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 137: Terminal matched constructor ID 88");
    88
}

fn match_node_instruction_138(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 138: Terminal matched constructor ID 151");
    151
}

fn match_node_instruction_139(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 139: Terminal matched constructor ID 150");
    150
}

fn match_node_instruction_140(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 140: Terminal matched constructor ID 149");
    149
}

fn match_node_instruction_141(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 141: Terminal matched constructor ID 56");
    56
}

fn match_node_instruction_142(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 142: Terminal matched constructor ID 57");
    57
}

fn match_node_instruction_143(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 143: Terminal matched constructor ID 55");
    55
}

fn match_node_instruction_144(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 144: Terminal matched constructor ID 24");
    24
}

fn match_node_instruction_145(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 145: Terminal matched constructor ID 25");
    25
}

fn match_node_instruction_146(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 146: Terminal matched constructor ID 23");
    23
}

fn match_node_instruction_147(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 147: Terminal matched constructor ID 86");
    86
}

fn match_node_instruction_148(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 148: Terminal matched constructor ID 87");
    87
}

fn match_node_instruction_149(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 149: Terminal matched constructor ID 91");
    91
}

fn match_node_instruction_150(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 150: Terminal matched constructor ID 156");
    156
}

fn match_node_instruction_151(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 151: Terminal matched constructor ID 62");
    62
}

fn match_node_instruction_152(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 152: Terminal matched constructor ID 61");
    61
}

fn match_node_instruction_153(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 153: Terminal matched constructor ID 30");
    30
}

fn match_node_instruction_154(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 154: Terminal matched constructor ID 29");
    29
}

fn match_node_instruction_155(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 155: Terminal matched constructor ID 112");
    112
}

fn match_node_instruction_156(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 156: Terminal matched constructor ID 113");
    113
}

fn match_node_instruction_157(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 157: Terminal matched constructor ID 114");
    114
}

fn match_node_instruction_158(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 158: Terminal matched constructor ID 115");
    115
}

fn match_node_instruction_159(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 159: Terminal matched constructor ID 116");
    116
}

fn match_node_instruction_160(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 160: Terminal matched constructor ID 117");
    117
}

fn match_node_instruction_161(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 161: Terminal matched constructor ID 106");
    106
}

fn match_node_instruction_162(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 162: Terminal matched constructor ID 107");
    107
}

fn match_node_instruction_163(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 163: Terminal matched constructor ID 108");
    108
}

fn match_node_instruction_164(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 164: Terminal matched constructor ID 109");
    109
}

fn match_node_instruction_165(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 165: Terminal matched constructor ID 110");
    110
}

fn match_node_instruction_166(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 166: Terminal matched constructor ID 111");
    111
}

fn match_node_instruction_167(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 167: Terminal matched constructor ID 104");
    104
}

fn match_node_instruction_168(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 168: Terminal matched constructor ID 105");
    105
}

fn match_node_instruction_169(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 169: Terminal matched constructor ID 84");
    84
}

fn match_node_instruction_170(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 170: Terminal matched constructor ID 147");
    147
}

fn match_node_instruction_171(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 171: Terminal matched constructor ID 199");
    199
}

fn match_node_instruction_172(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 100) & 3;
    eprintln!("Trace node 172: SlaContextBits start=100, size=2, probe={}", probe);
    match probe {
        0 => match_node_instruction_173(bytes, ctx),
        1 => match_node_instruction_174(bytes, ctx),
        2 => match_node_instruction_175(bytes, ctx),
        3 => match_node_instruction_176(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_173(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 173: Terminal matched constructor ID 207");
    207
}

fn match_node_instruction_174(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 174: Terminal matched constructor ID 208");
    208
}

fn match_node_instruction_175(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 175: Terminal matched constructor ID 209");
    209
}

fn match_node_instruction_176(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 176: Terminal matched constructor ID 210");
    210
}

fn match_node_instruction_177(bytes: &[u8], ctx: u64) -> i32 {
    let probe = (ctx >> 100) & 3;
    eprintln!("Trace node 177: SlaContextBits start=100, size=2, probe={}", probe);
    match probe {
        0 => match_node_instruction_178(bytes, ctx),
        1 => match_node_instruction_179(bytes, ctx),
        2 => match_node_instruction_180(bytes, ctx),
        3 => match_node_instruction_181(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_178(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 178: Terminal matched constructor ID 172");
    172
}

fn match_node_instruction_179(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 179: Terminal matched constructor ID 173");
    173
}

fn match_node_instruction_180(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 180: Terminal matched constructor ID 174");
    174
}

fn match_node_instruction_181(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 181: Terminal matched constructor ID 175");
    175
}

fn match_node_instruction_182(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 182: Terminal matched constructor ID 136");
    136
}

fn match_node_instruction_183(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 183: Terminal matched constructor ID 178");
    178
}

fn match_node_instruction_184(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 184: Terminal matched constructor ID 75");
    75
}

fn match_node_instruction_185(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 185: Terminal matched constructor ID 42");
    42
}

fn match_node_instruction_186(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 186: Terminal matched constructor ID 9");
    9
}

fn match_node_instruction_187(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 187: Terminal matched constructor ID 200");
    200
}

fn match_node_instruction_188(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 188: Terminal matched constructor ID 83");
    83
}

fn match_node_instruction_189(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 189: Terminal matched constructor ID 198");
    198
}

fn match_node_instruction_190(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 190: Terminal matched constructor ID 82");
    82
}

fn match_node_instruction_191(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 191: Terminal matched constructor ID 197");
    197
}

fn match_node_instruction_192(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 192: Terminal matched constructor ID 133");
    133
}

fn match_node_instruction_193(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 193: Terminal matched constructor ID 131");
    131
}

fn match_node_instruction_194(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 194: Terminal matched constructor ID 132");
    132
}

fn match_node_instruction_195(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 195: Terminal matched constructor ID 130");
    130
}

fn match_node_instruction_196(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 196: Terminal matched constructor ID 129");
    129
}

fn match_node_instruction_197(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 197: Terminal matched constructor ID 192");
    192
}

fn match_node_instruction_198(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 198: Terminal matched constructor ID 193");
    193
}

fn match_node_instruction_199(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 199: Terminal matched constructor ID 8");
    8
}

fn match_node_instruction_200(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 200: Terminal matched constructor ID 10");
    10
}

fn match_node_instruction_201(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 201: Terminal matched constructor ID 16");
    16
}

fn match_node_instruction_202(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 202: Terminal matched constructor ID 22");
    22
}

fn match_node_instruction_203(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 203: Terminal matched constructor ID 128");
    128
}

fn match_node_instruction_204(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 204: Terminal matched constructor ID 189");
    189
}

fn match_node_instruction_205(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 205: Terminal matched constructor ID 190");
    190
}

fn match_node_instruction_206(bytes: &[u8], ctx: u64) -> i32 {
    let byte_cnt = (10 + 6 + 7) / 8;
    let mut word = 0u64;
    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }
    let probe = (word >> 10) & 63;
    eprintln!("Trace node 206: SlaInstructionBits start=10, size=6, word={:08x}, probe={}", word, probe);
    match probe {
        0 => match_node_instruction_207(bytes, ctx),
        1 => match_node_instruction_208(bytes, ctx),
        2 => match_node_instruction_209(bytes, ctx),
        3 => match_node_instruction_210(bytes, ctx),
        4 => match_node_instruction_211(bytes, ctx),
        5 => match_node_instruction_212(bytes, ctx),
        6 => match_node_instruction_213(bytes, ctx),
        7 => match_node_instruction_214(bytes, ctx),
        8 => match_node_instruction_215(bytes, ctx),
        9 => match_node_instruction_216(bytes, ctx),
        10 => match_node_instruction_217(bytes, ctx),
        11 => match_node_instruction_218(bytes, ctx),
        12 => match_node_instruction_219(bytes, ctx),
        13 => match_node_instruction_220(bytes, ctx),
        14 => match_node_instruction_221(bytes, ctx),
        15 => match_node_instruction_222(bytes, ctx),
        16 => match_node_instruction_223(bytes, ctx),
        17 => match_node_instruction_224(bytes, ctx),
        18 => match_node_instruction_225(bytes, ctx),
        19 => match_node_instruction_226(bytes, ctx),
        20 => match_node_instruction_227(bytes, ctx),
        21 => match_node_instruction_228(bytes, ctx),
        22 => match_node_instruction_229(bytes, ctx),
        23 => match_node_instruction_230(bytes, ctx),
        24 => match_node_instruction_231(bytes, ctx),
        25 => match_node_instruction_232(bytes, ctx),
        26 => match_node_instruction_233(bytes, ctx),
        27 => match_node_instruction_234(bytes, ctx),
        28 => match_node_instruction_235(bytes, ctx),
        29 => match_node_instruction_236(bytes, ctx),
        30 => match_node_instruction_237(bytes, ctx),
        31 => match_node_instruction_238(bytes, ctx),
        32 => match_node_instruction_239(bytes, ctx),
        33 => match_node_instruction_240(bytes, ctx),
        34 => match_node_instruction_241(bytes, ctx),
        35 => match_node_instruction_242(bytes, ctx),
        36 => match_node_instruction_243(bytes, ctx),
        37 => match_node_instruction_244(bytes, ctx),
        38 => match_node_instruction_245(bytes, ctx),
        39 => match_node_instruction_246(bytes, ctx),
        40 => match_node_instruction_247(bytes, ctx),
        41 => match_node_instruction_248(bytes, ctx),
        42 => match_node_instruction_249(bytes, ctx),
        43 => match_node_instruction_250(bytes, ctx),
        44 => match_node_instruction_251(bytes, ctx),
        45 => match_node_instruction_252(bytes, ctx),
        46 => match_node_instruction_253(bytes, ctx),
        47 => match_node_instruction_254(bytes, ctx),
        48 => match_node_instruction_255(bytes, ctx),
        49 => match_node_instruction_256(bytes, ctx),
        50 => match_node_instruction_257(bytes, ctx),
        51 => match_node_instruction_258(bytes, ctx),
        52 => match_node_instruction_259(bytes, ctx),
        53 => match_node_instruction_260(bytes, ctx),
        54 => match_node_instruction_261(bytes, ctx),
        55 => match_node_instruction_262(bytes, ctx),
        56 => match_node_instruction_263(bytes, ctx),
        57 => match_node_instruction_264(bytes, ctx),
        58 => match_node_instruction_265(bytes, ctx),
        59 => match_node_instruction_266(bytes, ctx),
        60 => match_node_instruction_267(bytes, ctx),
        61 => match_node_instruction_268(bytes, ctx),
        62 => match_node_instruction_269(bytes, ctx),
        63 => match_node_instruction_270(bytes, ctx),
        _ => -1,
    }
}

fn match_node_instruction_207(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 207: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_208(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 208: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_209(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 209: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_210(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 210: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_211(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 211: Terminal matched constructor ID 222");
    222
}

fn match_node_instruction_212(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 212: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_213(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 213: Terminal matched NOTHING");
    -1
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
    eprintln!("Trace node 219: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_220(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 220: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_221(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 221: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_222(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 222: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_223(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 223: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_224(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 224: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_225(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 225: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_226(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 226: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_227(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 227: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_228(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 228: Terminal matched constructor ID 211");
    211
}

fn match_node_instruction_229(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 229: Terminal matched constructor ID 214");
    214
}

fn match_node_instruction_230(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 230: Terminal matched constructor ID 212");
    212
}

fn match_node_instruction_231(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 231: Terminal matched constructor ID 215");
    215
}

fn match_node_instruction_232(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 232: Terminal matched constructor ID 213");
    213
}

fn match_node_instruction_233(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 233: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_234(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 234: Terminal matched NOTHING");
    -1
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
    eprintln!("Trace node 241: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_242(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 242: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_243(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 243: Terminal matched NOTHING");
    -1
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
    eprintln!("Trace node 248: Terminal matched constructor ID 221");
    221
}

fn match_node_instruction_249(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 249: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_250(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 250: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_251(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 251: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_252(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 252: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_253(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 253: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_254(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 254: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_255(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 255: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_256(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 256: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_257(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 257: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_258(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 258: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_259(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 259: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_260(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 260: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_261(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 261: Terminal matched constructor ID 216");
    216
}

fn match_node_instruction_262(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 262: Terminal matched constructor ID 219");
    219
}

fn match_node_instruction_263(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 263: Terminal matched constructor ID 217");
    217
}

fn match_node_instruction_264(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 264: Terminal matched constructor ID 220");
    220
}

fn match_node_instruction_265(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 265: Terminal matched constructor ID 218");
    218
}

fn match_node_instruction_266(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 266: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_267(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 267: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_268(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 268: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_269(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 269: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_270(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 270: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_271(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 271: Terminal matched constructor ID 191");
    191
}

fn match_node_instruction_272(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 272: Terminal matched constructor ID 119");
    119
}

fn match_node_instruction_273(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 273: Terminal matched constructor ID 118");
    118
}

fn match_node_instruction_274(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 274: Terminal matched constructor ID 85");
    85
}

fn match_node_instruction_275(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 275: Terminal matched constructor ID 148");
    148
}

fn match_node_instruction_276(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 276: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_277(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 277: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_278(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 278: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_279(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 279: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_280(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 280: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_281(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 281: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_282(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 282: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_283(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 283: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_284(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 284: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_285(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 285: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_286(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 286: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_287(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 287: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_288(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 288: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_289(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 289: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_290(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 290: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_291(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 291: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_292(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 292: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_293(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 293: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_294(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 294: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_295(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 295: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_296(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 296: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_297(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 297: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_298(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 298: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_299(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 299: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_300(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 300: Terminal matched NOTHING");
    -1
}

fn match_node_instruction_301(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 301: Terminal matched NOTHING");
    -1
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
    eprintln!("Trace node 330: Terminal matched constructor ID 171");
    171
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
    eprintln!("Trace node 333: Terminal matched constructor ID 206");
    206
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

fn match_node_intVal_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_invokedynamicCallOther_index_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_invokeinterfaceCallOther_index_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_invokespecialCallOther_index_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_invokestaticCallOther_index_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_invokevirtualCallOther_index_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_ldc2_wCallOther_index_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_ldc_wCallOther_index_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_multianewarrayCallOther_index_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_padSwitch_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_putFieldCallOther_index_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

fn match_node_putStaticCallOther_index_0(bytes: &[u8], ctx: u64) -> i32 {
    eprintln!("Trace node 0: Terminal matched constructor ID 0");
    0
}

