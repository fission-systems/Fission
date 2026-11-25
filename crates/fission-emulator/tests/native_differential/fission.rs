//! Fission's side: lift the bytes with SLEIGH and interpret the p-code.

use std::path::PathBuf;

use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::interp::InterpExit;
use fission_emulator::jit::compiler::GuestInsn;
use fission_emulator::os::BareMetalEnv;
use fission_emulator::pcode::page_map::prot;
use fission_loader::loader::LoadedBinary;
use fission_pcode_ir::ir::{PcodeOp, PcodeOpcode};
use fission_sleigh::runtime::RuntimeSleighFrontend;

use crate::model::{BUF_LEN, CpuState, Isa};

/// One lifted instruction.
pub struct Lifted {
    pub mnemonic: String,
    pub text: String,
    pub len: usize,
    pub ops: Vec<PcodeOp>,
}

/// How the Fission run ended, when it did not produce a state.
pub enum FissionFail {
    /// A RAM access outside the mapped pages (the buffer and the code page).
    Fault(String),
    /// Anything else the engine refused: an error, an unimplemented opcode or
    /// an unanswered userop.
    Unmodeled(String),
    /// The evaluator panicked: always an evaluator bug.
    Panic(String),
}

type RegLoc = (u64, u64, u32);

pub struct FissionMachine {
    pub emu: Emulator,
    gpr: Vec<RegLoc>,
    flags: Vec<RegLoc>,
    vec: Vec<RegLoc>,
    control: RegLoc,
    control_value: u64,
    /// Register-space spans the instruction may touch, beyond those compared.
    pub register_space: u64,
    buf_addr: u64,
    code_addr: u64,
    code_len: usize,
}

impl FissionMachine {
    /// A machine for the language of `fixture`, with nothing mapped but the
    /// scratch buffer and the code page, at the addresses the native side uses.
    pub fn new(isa: &Isa, fixture: &str, buf_addr: u64, code_addr: u64, code_len: usize) -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join(fixture);
        let binary = LoadedBinary::from_file(&path).expect("load fixture");
        let load_spec = binary.load_spec().expect("load spec").clone();
        let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)
            .expect("frontend")
            .into_iter()
            .next()
            .expect("sleigh");
        let arch = ArchInfo::from_language_id(load_spec.pair.language_id.as_str(), Some(&binary))
            .expect("arch");
        let mut emu = Emulator::new(
            MachineState::new(),
            binary,
            sleigh,
            arch,
            Box::new(BareMetalEnv::new()),
        )
        .expect("emulator");
        emu.force_interpreter = true;
        emu.state
            .page_map
            .map_region(buf_addr, BUF_LEN as u64, prot::RW, true);
        emu.state
            .page_map
            .map_region(code_addr, code_len as u64, prot::RX, true);

        let lookup = |name: &str| -> RegLoc {
            *emu.register_map
                .get(name)
                .or_else(|| {
                    emu.register_map
                        .iter()
                        .find(|(k, _)| k.eq_ignore_ascii_case(name))
                        .map(|(_, v)| v)
                })
                .unwrap_or_else(|| panic!("register {name} missing from the register map"))
        };
        let gpr = isa.gpr.iter().map(|n| lookup(n)).collect();
        let flags = isa.flags.iter().map(|n| lookup(n)).collect();
        let vec = isa.vec.iter().map(|n| lookup(n)).collect();
        let control = lookup(isa.control.0);
        let register_space = emu.state.spaces_layout.register;
        Self {
            emu,
            gpr,
            flags,
            vec,
            control,
            control_value: isa.control.1,
            register_space,
            buf_addr,
            code_addr,
            code_len,
        }
    }

    pub fn lift(&self, bytes: &[u8], pc: u64) -> Option<Lifted> {
        let (decoded, ops, len, _) = self
            .emu
            .sleigh
            .decode_instruction_and_lift_with_context_override(bytes, pc, None)
            .ok()?;
        let len = usize::try_from(len).ok()?;
        if len == 0 || len > bytes.len() {
            return None;
        }
        Some(Lifted {
            mnemonic: decoded.mnemonic.to_ascii_uppercase(),
            text: format!("{} {}", decoded.mnemonic, decoded.operands_text)
                .trim()
                .to_string(),
            len,
            ops,
        })
    }

    pub fn userop_name(&self, op: &PcodeOp) -> String {
        let id = op
            .inputs
            .first()
            .map(|v| v.constant_val as u32)
            .unwrap_or(0);
        self.emu
            .userop_map
            .get(&id)
            .cloned()
            .unwrap_or_else(|| format!("userop_{id}"))
    }

    fn write_reg(&mut self, (space, offset, size): RegLoc, bytes: &[u8]) {
        let n = (size as usize).min(bytes.len());
        self.emu
            .state
            .write_space(space, offset, &bytes[..n])
            .expect("register write");
    }

    fn read_reg(&mut self, (space, offset, size): RegLoc) -> Vec<u8> {
        self.emu
            .state
            .read_space(space, offset, size as usize)
            .expect("register read")
    }

    /// Run `lifted`, which sits at `pc` inside `code_page`, from `input`.
    pub fn run(
        &mut self,
        lifted: &Lifted,
        pc: u64,
        code_page: &[u8],
        input: &CpuState,
        extra: &[(&str, u64)],
    ) -> Result<CpuState, FissionFail> {
        for (i, loc) in self.gpr.clone().into_iter().enumerate() {
            self.write_reg(loc, &input.gpr[i].to_le_bytes());
        }
        for (i, loc) in self.flags.clone().into_iter().enumerate() {
            self.write_reg(loc, &[input.flags[i]]);
        }
        for (i, loc) in self.vec.clone().into_iter().enumerate() {
            self.write_reg(loc, &input.vec[i]);
        }
        self.write_reg(self.control, &self.control_value.to_le_bytes());
        for (name, value) in extra {
            if let Some(&loc) = self.emu.register_map.get(*name) {
                self.write_reg(loc, &value.to_le_bytes());
            }
        }
        let ram = self.emu.state.ram_space();
        self.emu
            .state
            .load_image_range(self.code_addr, self.code_len as u64, code_page)
            .expect("code page");
        self.emu
            .state
            .write_space(ram, self.buf_addr, &input.buf)
            .expect("buffer");

        self.emu.metrics.unimplemented_opcodes.clear();
        self.emu.metrics.unhandled_userops.clear();
        self.emu.halt_requested = false;
        self.emu.pc = pc;
        let insn = GuestInsn {
            pc,
            len: lifted.len as u32,
            ops: lifted.ops.clone(),
        };
        let next = pc + lifted.len as u64;
        // A `rep` prefix re-enters its own instruction once per iteration.
        let mut iterations = 0u32;
        loop {
            iterations += 1;
            if iterations > 20_000 {
                return Err(FissionFail::Unmodeled("rep iteration limit".into()));
            }
            let emu = &mut self.emu;
            let step = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                emu.interpret_translation_block(std::slice::from_ref(&insn))
            }));
            let step = match step {
                Ok(step) => step,
                Err(payload) => {
                    let msg = payload
                        .downcast_ref::<&str>()
                        .map(|s| s.to_string())
                        .or_else(|| payload.downcast_ref::<String>().cloned())
                        .unwrap_or_else(|| "panic".into());
                    return Err(FissionFail::Panic(msg));
                }
            };
            match step {
                Ok(InterpExit::FallThrough(t)) | Ok(InterpExit::Branch(t)) if t == next => break,
                Ok(InterpExit::Branch(t)) if t == pc => {
                    self.emu.pc = pc;
                    continue;
                }
                Ok(InterpExit::Branch(t)) | Ok(InterpExit::FallThrough(t)) => {
                    return Err(FissionFail::Unmodeled(format!(
                        "left the instruction for {t:#x}"
                    )));
                }
                Ok(InterpExit::Halt) => return Err(FissionFail::Unmodeled("halt".into())),
                Err(e) => {
                    let msg = format!("{e:#}");
                    let lower = msg.to_ascii_lowercase();
                    let unmapped = ["fault", "unmapped", "not mapped", "protection"];
                    return Err(if unmapped.iter().any(|m| lower.contains(m)) {
                        FissionFail::Fault(msg)
                    } else {
                        FissionFail::Unmodeled(msg)
                    });
                }
            }
        }
        if !self.emu.metrics.unimplemented_opcodes.is_empty() {
            let names: Vec<_> = self
                .emu
                .metrics
                .unimplemented_opcodes
                .keys()
                .cloned()
                .collect();
            return Err(FissionFail::Unmodeled(format!(
                "unimplemented opcode {names:?}"
            )));
        }
        if !self.emu.metrics.unhandled_userops.is_empty() {
            let names: Vec<_> = self.emu.metrics.unhandled_userops.keys().cloned().collect();
            return Err(FissionFail::Unmodeled(format!(
                "unhandled userop {names:?}"
            )));
        }

        let mut out = CpuState::zeroed();
        for (i, loc) in self.gpr.clone().into_iter().enumerate() {
            let b = self.read_reg(loc);
            let mut v = [0u8; 8];
            v[..b.len().min(8)].copy_from_slice(&b[..b.len().min(8)]);
            out.gpr[i] = u64::from_le_bytes(v);
        }
        for (i, loc) in self.flags.clone().into_iter().enumerate() {
            out.flags[i] = self.read_reg(loc)[0];
        }
        for (i, loc) in self.vec.clone().into_iter().enumerate() {
            let b = self.read_reg(loc);
            out.vec[i].copy_from_slice(&b[..16]);
        }
        out.buf = self
            .emu
            .state
            .read_space(ram, self.buf_addr, BUF_LEN)
            .expect("buffer read");
        Ok(out)
    }
}

/// Why an instruction cannot be run natively, judged from its p-code: it
/// leaves the instruction, or asks the host for something (a syscall, a
/// timer, an identity) the two sides cannot share. A branch back to the
/// instruction itself is allowed only where `repeats` says the instruction
/// re-executes by design (x86 `rep` string operations); anywhere else it is a
/// loop (`jbe $`), which would never reach the trap after it.
pub fn control_flow_skip(
    lifted: &Lifted,
    pc: u64,
    fission: &FissionMachine,
    repeats: bool,
) -> Option<String> {
    let next = pc + lifted.len as u64;
    // A branch to the next instruction is allowed: SLEIGH writes conditional
    // moves, bit scans and the like as `goto inst_next`, and the decoder's
    // flow kind cannot tell those from a `jcc` whose offset is zero. A real
    // branch that leaves anyway (AMD truncates a 0x66-prefixed near branch's
    // target to 16 bits: `66 e2 00` jumps to 0x0119) faults outside the stub,
    // and the native runner reports that fault as the case's (`IN_STUB`).
    for op in &lifted.ops {
        match op.opcode {
            PcodeOpcode::Branch | PcodeOpcode::CBranch => {
                let dest = &op.inputs[0];
                if dest.is_constant || dest.space_id == 0 {
                    continue; // relative: inside the instruction
                }
                if dest.offset != next && !(repeats && dest.offset == pc) {
                    return Some("branch".into());
                }
            }
            PcodeOpcode::BranchInd => return Some("branch".into()),
            PcodeOpcode::Call | PcodeOpcode::CallInd => return Some("call".into()),
            PcodeOpcode::Return => return Some("return".into()),
            PcodeOpcode::CallOther => {
                let name = fission.userop_name(op);
                if !fission_emulator::arch::vector::is_wide_userop(op)
                    && !ORDERING_USEROPS.contains(&name.as_str())
                {
                    return Some(format!("userop:{name}"));
                }
            }
            _ => {}
        }
    }
    None
}

/// Userops that only order or hint memory accesses: a no-op for one thread on
/// either side (`os::env` classifies them as barriers and hints), so an
/// instruction that carries one (`lock xadd`, `ldar`) is still compared.
const ORDERING_USEROPS: &[&str] = &[
    "DataMemoryBarrier",
    "DataSynchronizationBarrier",
    "InstructionSynchronizationBarrier",
    "SpeculationBarrier",
    "LOAcquire",
    "LORelease",
    "LOCK",
    "UNLOCK",
    "XACQUIRE",
    "XRELEASE",
    "HintPreloadData",
    "HintPreloadDataForWrite",
    "HintPreloadInstruction",
];

/// Whether any varnode of the p-code lies in register space in `[lo, hi)`,
/// as an input (`read`) or output.
pub fn touches_registers(
    ops: &[PcodeOp],
    register_space: u64,
    lo: u64,
    hi: u64,
    outputs_only: bool,
) -> bool {
    let hit = |v: &fission_pcode_ir::ir::Varnode| {
        !v.is_constant
            && v.space_id == register_space
            && v.offset < hi
            && v.offset + u64::from(v.size) > lo
    };
    ops.iter().any(|op| {
        op.output.as_ref().is_some_and(hit) || (!outputs_only && op.inputs.iter().any(hit))
    })
}
