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
}

impl Emulator {
    pub fn new(mut state: MachineState, binary: LoadedBinary, sleigh: RuntimeSleighFrontend) -> Result<Self> {
        let rip = binary.inner().entry_point;
        
        crate::loader::map_binary_to_state(&mut state, &binary)?;

        // set RIP register (assume space 2, offset some specific value depending on arch).
        // Since we know we are x86/x64 mostly, Sleigh handles RIP internally, but for emulator loop we keep track of it here.
        Ok(Self {
            state,
            binary,
            sleigh: Arc::new(sleigh),
            rip,
        })
    }

    pub fn run_instruction(&mut self) -> Result<bool> {
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

        let mut branched = false;
        let mut target_rip = 0;

        let mut evaluator = Evaluator::new(&mut self.state);
        for op in pcode_ops {
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
            crate::os::windows::hle::dispatch(self, self.rip)?;
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
