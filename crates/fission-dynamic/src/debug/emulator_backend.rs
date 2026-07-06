use crate::debug::traits::ExecutionBackend;
use crate::debug::types::{ProcessInfo, RegisterState};
use fission_core::Result as FissionResult;
use fission_emulator::core::Emulator;

pub struct EmulatorBackend {
    pub emulator: Option<Emulator>,
}

impl EmulatorBackend {
    pub fn new() -> Self {
        Self { emulator: None }
    }
}

impl Default for EmulatorBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutionBackend for EmulatorBackend {
    fn enumerate_processes() -> Vec<ProcessInfo> {
        // Emulators don't have OS processes to enumerate
        Vec::new()
    }

    fn attach(&mut self, pid: u32) -> FissionResult<()> {
        let _ = pid;
        Err(fission_core::err!(
            debug,
            "attach(pid) is not supported by EmulatorBackend. Use launch(path) instead."
        ))
    }

    fn detach(&mut self) -> FissionResult<()> {
        self.emulator = None;
        Ok(())
    }

    fn is_attached(&self) -> bool {
        self.emulator.is_some()
    }

    fn attached_pid(&self) -> Option<u32> {
        // Return a dummy PID to keep TUI happy if it requires one
        if self.emulator.is_some() {
            Some(9999)
        } else {
            None
        }
    }

    fn continue_execution(&mut self) -> FissionResult<()> {
        if let Some(emu) = &mut self.emulator {
            emu.run()?;
            Ok(())
        } else {
            Err(fission_core::err!(debug, "Emulator not running"))
        }
    }

    fn single_step(&mut self) -> FissionResult<()> {
        if let Some(emu) = &mut self.emulator {
            // Emulate one instruction
            let _ = emu.run_instruction()?;
            Ok(())
        } else {
            Err(fission_core::err!(debug, "Emulator not running"))
        }
    }

    fn set_sw_breakpoint(&mut self, address: u64) -> FissionResult<()> {
        let _ = address;
        // Store in a breakpoint map, checked during step loop
        // We'll leave it empty for now, as run_instruction can check a set of BPs
        Err(fission_core::err!(debug, "SW breakpoints not yet implemented in EmulatorBackend"))
    }

    fn remove_sw_breakpoint(&mut self, address: u64) -> FissionResult<()> {
        let _ = address;
        Err(fission_core::err!(debug, "SW breakpoints not yet implemented in EmulatorBackend"))
    }

    fn read_memory(&self, address: u64, size: usize) -> FissionResult<Vec<u8>> {
        if let Some(emu) = &self.emulator {
            let mut buf = vec![0u8; size];
            // Access space id 3 for RAM (as defined in loader mapping)
            for i in 0..size {
                if let Ok(b) = emu.state.read_space_readonly(3, address + i as u64, 1) {
                    if !b.is_empty() { buf[i] = b[0]; }
                } else {
                    return Err(fission_core::err!(debug, "Memory read failed at 0x{:x}", address + i as u64));
                }
            }
            Ok(buf)
        } else {
            Err(fission_core::err!(debug, "Emulator not running"))
        }
    }

    fn write_memory(&mut self, address: u64, data: &[u8]) -> FissionResult<()> {
        if let Some(emu) = &mut self.emulator {
            for (i, b) in data.iter().enumerate() {
                let _ = emu.state.write_space(3, address + i as u64, &[*b]);
            }
            Ok(())
        } else {
            Err(fission_core::err!(debug, "Emulator not running"))
        }
    }

    fn fetch_registers(&mut self, thread_id: u32) -> FissionResult<RegisterState> {
        let _ = thread_id;
        if let Some(emu) = &self.emulator {
            // Create dummy RegisterState and fill with what we can get
            let mut state = RegisterState::default();
            state.rip = emu.rip;
            // In a full implementation, we'd query registers from emu.state.read(1, offset, size)
            // Need Sleigh to know register offsets, or hardcode typical x86_64 for now
            Ok(state)
        } else {
            Err(fission_core::err!(debug, "Emulator not running"))
        }
    }

    fn launch(&mut self, path: &str, args: &[String]) -> FissionResult<u32> {
        let _ = args;
        
        let binary = fission_loader::loader::LoadedBinary::from_file(path)
            .map_err(|e| fission_core::err!(debug, "Loader error: {}", e))?;
        
        let sleigh = fission_sleigh::runtime::RuntimeSleighFrontend::new_for_language("x86-64")
            .map_err(|e| fission_core::err!(debug, "Sleigh init failed: {}", e))?;
        
        let state = fission_emulator::pcode::state::MachineState::new();
        
        let emu = Emulator::new(state, binary, sleigh).map_err(|e| fission_core::err!(debug, "Emulator init failed: {}", e))?;
        self.emulator = Some(emu);
        
        Ok(9999) // Return dummy PID
    }

    fn get_state(&self) -> crate::debug::types::DebugState {
        let mut state = crate::debug::types::DebugState::default();
        if let Some(emu) = &self.emulator {
            state.attached_pid = Some(9999);
            state.main_thread_id = Some(1);
            state.status = crate::debug::types::DebugStatus::Suspended;
            
            // Add a single dummy thread
            state.threads.insert(1, crate::debug::types::ThreadInfo {
                thread_id: 1,
                start_address: emu.rip,
                suspended: true,
                is_main: true,
            });
        }
        state
    }
}
