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
                if let Ok(b) = emu.state.read(3, address + i as u64, 1) {
                    buf[i] = b as u8;
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
                let _ = emu.state.write(3, address + i as u64, 1, *b as u64);
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
            state.rip = emu.state.get_rip();
            // In a full implementation, we'd query registers from emu.state.read(1, offset, size)
            // Need Sleigh to know register offsets, or hardcode typical x86_64 for now
            Ok(state)
        } else {
            Err(fission_core::err!(debug, "Emulator not running"))
        }
    }

    fn launch(&mut self, path: &str, args: &[String]) -> FissionResult<u32> {
        let _ = args;
        // Initialize the emulator components
        // This mirrors the logic in `fission_cli::cli::oneshot::run_sandbox`
        
        let file_data = std::fs::read(path).map_err(|e| fission_core::err!(debug, "Failed to read binary: {}", e))?;
        let binary = fission_loader::loader::BinaryLoader::load(path.as_ref(), &file_data)
            .map_err(|e| fission_core::err!(debug, "Loader error: {}", e))?;
        
        let path_config = fission_core::core::path_config::PathConfig::detect(None);
        let bundle = fission_core::core::path_config::resolve_bundle(&path_config)?;
        
        let spec_path = bundle.sleigh_root.join("x86-64").join("x86-64.sla");
        let sla_data = std::fs::read(&spec_path).map_err(|e| fission_core::err!(debug, "Failed to read sla: {}", e))?;
        let spec = fission_sleigh::spec::SleighSpec::parse(&sla_data)
            .map_err(|e| fission_core::err!(debug, "Spec error: {}", e))?;
        
        let sleigh = fission_sleigh::runtime::RuntimeSleighFrontend::new(spec);
        let mut state = fission_emulator::core::MachineState::new();
        
        // Map PE sections
        if let fission_loader::loader::BinaryType::Pe(pe) = &binary.binary_type {
            if let Some(headers) = &pe.headers {
                for sec in &headers.sections {
                    let addr = headers.optional_header.image_base + sec.virtual_address as u64;
                    let size = sec.virtual_size as usize;
                    let file_size = sec.size_of_raw_data as usize;
                    
                    let mut data = vec![0u8; size];
                    let copy_size = std::cmp::min(size, file_size);
                    if copy_size > 0 {
                        let offset = sec.pointer_to_raw_data as usize;
                        if offset + copy_size <= file_data.len() {
                            data[..copy_size].copy_from_slice(&file_data[offset..offset+copy_size]);
                        }
                    }
                    
                    for (i, b) in data.iter().enumerate() {
                        let _ = state.write(3, addr + i as u64, 1, *b as u64);
                    }
                }
            }
        }
        
        state.set_rip(binary.entry_point.unwrap_or(0));
        
        let emu = Emulator::new(state, binary, sleigh).map_err(|e| fission_core::err!(debug, "Emulator init failed: {}", e))?;
        self.emulator = Some(emu);
        
        Ok(9999) // Return dummy PID
    }
}
