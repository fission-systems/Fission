pub mod aarch64;
pub mod arm;
pub mod atmel;
pub mod bpf;
pub mod cp1600;
pub mod cr16;
pub mod dalvik;
pub mod data;
pub mod ebpf;
pub mod hcs08;
pub mod hcs12;
pub mod jvm;
pub mod loongarch;
pub mod m16c;
pub mod m8c;
pub mod mc6800;
pub mod mcs96;
pub mod mips;
pub mod nds32;
pub mod p_6502;
pub mod p_68000;
pub mod p_8048;
pub mod p_8051;
pub mod p_8085;
pub mod pa_risc;
pub mod pic;
pub mod powerpc;
pub mod riscv;
pub mod sparc;
pub mod superh;
pub mod superh4;
pub mod ti_msp430;
pub mod toy;
pub mod tricore;
pub mod v850;
pub mod x86;
pub mod xtensa;
pub mod z80;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessorSkeleton {
    pub ghidra_processor: &'static str,
    pub module_name: &'static str,
    pub executable_candidate: bool,
}

pub const PROCESSOR_SKELETONS: &[ProcessorSkeleton] = &[
    p_6502::SKELETON,
    p_68000::SKELETON,
    p_8048::SKELETON,
    p_8051::SKELETON,
    p_8085::SKELETON,
    aarch64::SKELETON,
    arm::SKELETON,
    atmel::SKELETON,
    bpf::SKELETON,
    cp1600::SKELETON,
    cr16::SKELETON,
    data::SKELETON,
    dalvik::SKELETON,
    hcs08::SKELETON,
    hcs12::SKELETON,
    jvm::SKELETON,
    loongarch::SKELETON,
    m16c::SKELETON,
    m8c::SKELETON,
    mc6800::SKELETON,
    mcs96::SKELETON,
    mips::SKELETON,
    nds32::SKELETON,
    pa_risc::SKELETON,
    pic::SKELETON,
    powerpc::SKELETON,
    riscv::SKELETON,
    sparc::SKELETON,
    superh::SKELETON,
    superh4::SKELETON,
    ti_msp430::SKELETON,
    toy::SKELETON,
    v850::SKELETON,
    xtensa::SKELETON,
    z80::SKELETON,
    ebpf::SKELETON,
    tricore::SKELETON,
    x86::SKELETON,
];
