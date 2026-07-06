use crate::pcode::state::MachineState;
use crate::pcode::eval::{Evaluator, StepResult};
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;
use anyhow::{Result, Context};
use std::sync::Arc;

pub struct Emulator {
    pub state: MachineState,
    pub binary: LoadedBinary,
    pub sleigh: Arc<RuntimeSleighFrontend>,
    pub rip: u64,
    pub register_map: std::collections::HashMap<String, (u64, u64, u32)>,
}

impl Emulator {
    pub fn new(mut state: MachineState, binary: LoadedBinary, sleigh: RuntimeSleighFrontend) -> Result<Self> {
        let rip = binary.inner().entry_point;
        
        crate::loader::map_binary_to_state(&mut state, &binary)?;

        let register_map = if let Some(spec) = binary.load_spec() {
            fission_sleigh::runtime::register_map_for_load_spec(spec)
                .unwrap_or_default()
        } else {
            std::collections::HashMap::new()
        };

        // set RIP register (assume space 2, offset some specific value depending on arch).
        // Since we know we are x86/x64 mostly, Sleigh handles RIP internally, but for emulator loop we keep track of it here.
        let mut emu = Self {
            state,
            binary,
            sleigh: Arc::new(sleigh),
            rip,
            register_map,
        };

        // Initialize stack pointer (RSP/ESP)
        if emu.binary.inner().is_64bit {
            let _ = emu.write_register_u64("rsp", 0x7FFFFFFF0000);
        } else {
            let _ = emu.write_register_u64("esp", 0x7FFF0000);
        }

        Ok(emu)
    }

    pub fn read_register_u64(&mut self, name: &str) -> Result<u64> {
        let (space_id, offset, size) = self.register_map.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| *v)
            .ok_or_else(|| anyhow::anyhow!("Register {} not found", name))?;
        
        if size > 8 {
            anyhow::bail!("Register {} is too large to read as u64", name);
        }

        let bytes = self.state.read_space(space_id, offset, size as usize)?;
        let mut val = 0u64;
        for (i, &b) in bytes.iter().enumerate() {
            val |= (b as u64) << (i * 8);
        }
        Ok(val)
    }

    pub fn write_register_u64(&mut self, name: &str, mut val: u64) -> Result<()> {
        let (space_id, offset, size) = self.register_map.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| *v)
            .ok_or_else(|| anyhow::anyhow!("Register {} not found", name))?;
        
        if size > 8 {
            anyhow::bail!("Register {} is too large to write as u64", name);
        }

        let mut bytes = Vec::with_capacity(size as usize);
        for _ in 0..size {
            bytes.push((val & 0xFF) as u8);
            val >>= 8;
        }
        self.state.write_space(space_id, offset, &bytes)
    }

    pub fn run_instruction(&mut self) -> Result<bool> {
        tracing::debug!("Executing RIP=0x{:X}", self.rip);
        
        // Fetch up to 16 bytes for decoding from RAM (Space 3)
        let max_inst_len = 16;
        let bytes_vec = match self.state.read_space(3, self.rip, max_inst_len) {
            Ok(b) => b,
            Err(_) => {
                tracing::error!("Failed to fetch instruction memory at 0x{:X}", self.rip);
                return Ok(false);
            }
        };

        // Decode and lift
        let (pcode_ops, inst_len) = self.sleigh.decode_and_lift_with_len(&bytes_vec, self.rip)
            .with_context(|| format!("Failed to lift instruction at 0x{:X}", self.rip))?;

        // Update RIP register in space 2 for %rip-relative addressing and call instruction's return address pushing
        if self.binary.inner().is_64bit {
            let _ = self.write_register_u64("rip", self.rip + inst_len);
        } else {
            let _ = self.write_register_u64("eip", self.rip + inst_len);
        }

        let mut branched = false;
        let mut target_rip = 0;

        let mut evaluator = Evaluator::new(&mut self.state);
        for op in pcode_ops {
            tracing::debug!("    P-Code: {:?}", op.opcode);
            match evaluator.step(&op)? {
                StepResult::Next => {}
                StepResult::Branch(target) => {
                    branched = true;
                    target_rip = target;
                    break;
                }
            }
        }

        if branched {
            self.rip = target_rip;
        } else {
            self.rip += inst_len;
        }

        // HLE Trap Check
        if self.rip >= 0xFFFFFFF000000000 {
            let continue_exec = crate::os::windows::hle::dispatch(self, self.rip)?;
            if !continue_exec {
                return Ok(false);
            }
        }

        Ok(true)
    }

    pub fn run(&mut self) -> Result<()> {
        loop {
            if !self.run_instruction()? {
                break;
            }
        }
        Ok(())
    }
}
