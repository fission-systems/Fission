use crate::debug::traits::ExecutionBackend;
use crate::debug::types::{ProcessInfo, RegisterState};
use fission_core::Result as FissionResult;
use fission_emulator::core::{Emulator, RunOutcome};

/// The handle the debug layer uses for "the emulated process".
///
/// An emulator has no OS process and therefore no pid; this is a constant
/// rather than a magic number repeated at five call sites.
pub const EMULATED_PID: u32 = 9999;

pub struct EmulatorBackend {
    pub emulator: Option<Emulator>,
    /// Why the last `continue` stopped. `run` discards this, and a front end
    /// that cannot tell a breakpoint from a finished program cannot drive a
    /// session.
    last_outcome: Option<RunOutcome>,
}

impl EmulatorBackend {
    pub fn new() -> Self {
        Self {
            emulator: None,
            last_outcome: None,
        }
    }

    /// Why execution last stopped, or `None` if it has not run yet.
    pub fn last_outcome(&self) -> Option<&RunOutcome> {
        self.last_outcome.as_ref()
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
        let Some(emu) = &mut self.emulator else {
            return Err(fission_core::err!(debug, "Emulator not running"));
        };
        self.last_outcome = Some(emu.resume()?);
        Ok(())
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
        let Some(emu) = &mut self.emulator else {
            return Err(fission_core::err!(debug, "Emulator not running"));
        };
        // No int3 to patch in: the run loop compares the program counter
        // directly, which also means the guest cannot see the breakpoint the
        // way it can see a patched byte.
        emu.set_breakpoint(address);
        Ok(())
    }

    fn remove_sw_breakpoint(&mut self, address: u64) -> FissionResult<()> {
        let Some(emu) = &mut self.emulator else {
            return Err(fission_core::err!(debug, "Emulator not running"));
        };
        if !emu.clear_breakpoint(address) {
            return Err(fission_core::err!(
                debug,
                "No breakpoint at 0x{:x}",
                address
            ));
        }
        Ok(())
    }

    fn read_memory(&self, address: u64, size: usize) -> FissionResult<Vec<u8>> {
        let Some(emu) = &self.emulator else {
            return Err(fission_core::err!(debug, "Emulator not running"));
        };
        // The emulator's memory is sparse and zero-fills on demand, which is
        // right for execution and a lie to a debugger: an address nothing ever
        // mapped read back as sixteen zero bytes. Ask the page map first, the
        // way `gdb` answers "Cannot access memory at address".
        use fission_emulator::pcode::page_map::AccessKind;
        emu.state
            .page_map
            .check_range(address, size, AccessKind::Read)
            .map_err(|e| {
                fission_core::err!(debug, "Cannot access memory at 0x{:x}: {}", address, e)
            })?;
        // `ram_space()`, not the literal 3: the RAM space index comes from the
        // language, and a hard-coded one is right until it is not. One read of
        // the whole range, not one per byte.
        emu.state
            .read_space_readonly(emu.state.ram_space(), address, size)
            .map_err(|e| fission_core::err!(debug, "Memory read failed at 0x{:x}: {}", address, e))
    }

    fn write_memory(&mut self, address: u64, data: &[u8]) -> FissionResult<()> {
        let Some(emu) = &mut self.emulator else {
            return Err(fission_core::err!(debug, "Emulator not running"));
        };
        let ram = emu.state.ram_space();
        // The failure was discarded here, so a write to an unmapped address
        // reported success and the caller went on believing it had landed.
        emu.state.write_space(ram, address, data).map_err(|e| {
            fission_core::err!(debug, "Memory write failed at 0x{:x}: {}", address, e)
        })?;
        // Self-modifying code, and a debugger patching an instruction, are the
        // same thing to the block cache: what it compiled is no longer what is
        // there.
        emu.invalidate_translations(address, data.len());
        Ok(())
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
            state.status = match self.last_outcome {
                // The machine ran to the end of the program; there is nothing
                // left to step or resume, and reporting `Suspended` made a
                // finished run look like one waiting at a breakpoint.
                Some(RunOutcome::ProcessExited | RunOutcome::Halted) => {
                    crate::debug::types::DebugStatus::Terminated
                }
                _ => crate::debug::types::DebugStatus::Suspended,
            };

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
