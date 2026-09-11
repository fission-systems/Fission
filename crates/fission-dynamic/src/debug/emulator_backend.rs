use crate::debug::traits::ExecutionBackend;
use crate::debug::types::{ProcessInfo, RegisterState};
use fission_core::Result as FissionResult;
use fission_emulator::core::Emulator;

/// The handle the debug layer uses for "the emulated process".
///
/// An emulator has no OS process and therefore no pid; this is a constant
/// rather than a magic number repeated at five call sites.
pub const EMULATED_PID: u32 = 9999;

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
        Err(fission_core::err!(
            debug,
            "SW breakpoints not yet implemented in EmulatorBackend"
        ))
    }

    fn remove_sw_breakpoint(&mut self, address: u64) -> FissionResult<()> {
        let _ = address;
        Err(fission_core::err!(
            debug,
            "SW breakpoints not yet implemented in EmulatorBackend"
        ))
    }

    fn read_memory(&self, address: u64, size: usize) -> FissionResult<Vec<u8>> {
        if let Some(emu) = &self.emulator {
            let mut buf = vec![0u8; size];
            // Access space id 3 for RAM (as defined in loader mapping)
            for i in 0..size {
                if let Ok(b) = emu.state.read_space_readonly(3, address + i as u64, 1) {
                    if !b.is_empty() {
                        buf[i] = b[0];
                    }
                } else {
                    return Err(fission_core::err!(
                        debug,
                        "Memory read failed at 0x{:x}",
                        address + i as u64
                    ));
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
        let Some(emu) = &mut self.emulator else {
            return Err(fission_core::err!(debug, "Emulator not running"));
        };
        // The emulator's own answer, so a front end sees the registers the
        // machine actually has. Naming the x86-64 sixteen here reported
        // sixteen zeroes for every aarch64, ARM and MIPS image.
        Ok(emu.register_state())
    }

    fn launch(&mut self, path: &str, args: &[String]) -> FissionResult<u32> {
        let _ = args;

        let binary = fission_loader::loader::LoadedBinary::from_file(path)
            .map_err(|e| fission_core::err!(debug, "Loader error: {}", e))?;

        // The language comes from the image rather than being assumed to be
        // x86-64, and the OS layer and loader come with it -- an emulator
        // launched without them has no stack, no image mapped and no syscalls.
        let load_spec = binary
            .load_spec()
            .cloned()
            .ok_or_else(|| fission_core::err!(debug, "no load spec for {}", path))?;
        let sleigh =
            fission_sleigh::runtime::RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(
                &load_spec,
            )
            .map_err(|e| fission_core::err!(debug, "Sleigh init failed: {}", e))?
            .into_iter()
            .next()
            .ok_or_else(|| fission_core::err!(debug, "no SLEIGH frontend for {}", path))?;
        let arch = fission_emulator::arch::ArchInfo::from_language_id(
            load_spec.pair.language_id.as_str(),
            Some(&binary),
        )
        .map_err(|e| fission_core::err!(debug, "Unsupported architecture: {}", e))?;

        let mut state = fission_emulator::pcode::state::MachineState::new();
        let is_pe = binary.format == "PE";
        let image = if is_pe {
            fission_emulator::os::windows::loader::load_pe(&mut state, &binary)
                .map(Ok)
                .map_err(|e| fission_core::err!(debug, "PE load failed: {}", e))?
        } else {
            fission_emulator::os::linux::loader::load_elf(&mut state, &binary)
                .map(Err)
                .map_err(|e| fission_core::err!(debug, "ELF load failed: {}", e))?
        };
        let os: Box<dyn fission_emulator::os::OsEnvironment> = if is_pe {
            Box::new(fission_emulator::os::WindowsEnv::new())
        } else {
            Box::new(fission_emulator::os::LinuxEnv::new())
        };

        let mut emu = Emulator::new(state, binary, sleigh, arch, os)
            .map_err(|e| fission_core::err!(debug, "Emulator init failed: {}", e))?;
        match image {
            Ok(pe) => emu
                .apply_windows_image(pe)
                .map_err(|e| fission_core::err!(debug, "PE image failed: {}", e))?,
            Err(elf) => emu
                .apply_linux_image(elf)
                .map_err(|e| fission_core::err!(debug, "ELF image failed: {}", e))?,
        }
        self.emulator = Some(emu);

        // There is no OS process, so there is no pid. This is the handle the
        // rest of the debug layer uses to refer to "the emulated process".
        Ok(EMULATED_PID)
    }

    fn get_state(&self) -> crate::debug::types::DebugState {
        let mut state = crate::debug::types::DebugState::default();
        if let Some(emu) = &self.emulator {
            state.attached_pid = Some(EMULATED_PID);
            state.main_thread_id = Some(1);
            state.status = crate::debug::types::DebugStatus::Suspended;

            // Add a single dummy thread
            state.threads.insert(
                1,
                crate::debug::types::ThreadInfo {
                    thread_id: 1,
                    start_address: emu.pc,
                    suspended: true,
                    is_main: true,
                },
            );
        }
        state
    }
}
