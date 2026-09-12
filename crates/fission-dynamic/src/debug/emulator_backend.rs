use crate::debug::traits::ExecutionBackend;
use crate::debug::types::{ProcessInfo, RegisterState};
use fission_core::Result as FissionResult;
use fission_emulator::core::{Emulator, InstructionShape, RunOutcome};

/// The handle the debug layer uses for "the emulated process".
///
/// An emulator has no OS process and therefore no pid; this is a constant
/// rather than a magic number repeated at five call sites.
pub const EMULATED_PID: u32 = 9999;

/// The one thread an emulated process has.
///
/// The emulator runs a single guest context: there is no `clone`, no
/// scheduler, and nothing for a second thread id to mean. A front end that
/// asks for this one gets it and any other gets an error saying so, which is
/// a better answer than pretending to switch.
pub const EMULATED_THREAD_ID: u32 = 1;

pub struct EmulatorBackend {
    pub emulator: Option<Emulator>,
    /// Why the last `continue` stopped. `run` discards this, and a front end
    /// that cannot tell a breakpoint from a finished program cannot drive a
    /// session.
    last_outcome: Option<RunOutcome>,
    /// What has happened, oldest first, waiting to be polled.
    events: std::collections::VecDeque<crate::debug::types::DebugEvent>,
    /// How much of the guest's standard output has already been reported as
    /// an `OutputString` event.
    stdout_reported: usize,
}

impl EmulatorBackend {
    pub fn new() -> Self {
        Self {
            emulator: None,
            last_outcome: None,
            events: std::collections::VecDeque::new(),
            stdout_reported: 0,
        }
    }

    /// Turn a stop into the events a front end polls for.
    ///
    /// Called after everything that runs the machine, so the queue says what
    /// happened in the order it happened: the guest's output first, because it
    /// was produced *during* the run, then why the run ended.
    fn record_stop(&mut self, outcome: RunOutcome) {
        use crate::debug::types::DebugEvent;
        self.last_outcome = Some(outcome);
        self.drain_guest_output();
        let event = match outcome {
            RunOutcome::HitBreakpoint(address) => Some(DebugEvent::BreakpointHit {
                address,
                thread_id: EMULATED_THREAD_ID,
            }),
            RunOutcome::HitWatchpoint(hit) => Some(DebugEvent::WatchpointHit {
                address: hit.address,
                size: hit.size,
                write: hit.write,
                pc: hit.pc,
                thread_id: EMULATED_THREAD_ID,
            }),
            RunOutcome::Stepped => Some(DebugEvent::SingleStep {
                thread_id: EMULATED_THREAD_ID,
            }),
            RunOutcome::ProcessExited | RunOutcome::Halted => Some(DebugEvent::ProcessExited {
                exit_code: self
                    .emulator
                    .as_ref()
                    .and_then(|emu| emu.exit_code)
                    .unwrap_or(0),
            }),
            RunOutcome::Returned
            | RunOutcome::SymGate
            | RunOutcome::HitBudget
            | RunOutcome::LoopExit
            | RunOutcome::Interrupted => None,
        };
        if let Some(event) = event {
            self.events.push_back(event);
        }
    }

    /// Anything the guest has written to its standard output since the last
    /// time this looked.
    ///
    /// The emulator's simulated filesystem accumulates writes to descriptor 1,
    /// so the bytes are already there -- they were simply never surfaced. For
    /// a sample under examination this is often the whole point.
    fn drain_guest_output(&mut self) {
        const GUEST_STDOUT: u64 = 1;
        let Some(emu) = &self.emulator else {
            return;
        };
        let Some(total) = emu.vfs.file_size(GUEST_STDOUT) else {
            return;
        };
        if total <= self.stdout_reported {
            return;
        }
        let Ok(bytes) = emu.vfs.peek(
            GUEST_STDOUT,
            self.stdout_reported,
            total - self.stdout_reported,
        ) else {
            return;
        };
        self.stdout_reported = total;
        self.events
            .push_back(crate::debug::types::DebugEvent::OutputString {
                message: String::from_utf8_lossy(&bytes).into_owned(),
            });
    }

    /// Why execution last stopped, or `None` if it has not run yet.
    pub fn last_outcome(&self) -> Option<&RunOutcome> {
        self.last_outcome.as_ref()
    }

    /// Whether the machine has stopped for good.
    ///
    /// `LoopExit` belongs here with the two obvious ones: it is the run loop
    /// finding that there is nothing further to execute -- the fixture's
    /// `main` returning to a null return address, or a fatal signal. Leaving
    /// it out let a second `continue` walk into "Failed to fetch instruction
    /// bytes at 0x0", which is true and tells a reader nothing.
    fn has_finished(&self) -> bool {
        matches!(
            self.last_outcome,
            Some(RunOutcome::ProcessExited | RunOutcome::Halted | RunOutcome::LoopExit)
        )
    }

    /// The emulator to run, or an error if the program already ended.
    ///
    /// `run_inner` clears `halt_requested` on entry, so resuming an exited
    /// process dispatches its exit stub again -- and again. Stepping a
    /// finished program printed the same address and the same instruction
    /// count forever instead of saying it was over.
    fn runnable(&mut self) -> FissionResult<&mut Emulator> {
        if self.has_finished() {
            return Err(fission_core::err!(
                debug,
                "The program has stopped for good ({}); there is nothing left to run",
                self.stop_reason().unwrap_or_else(|| "exited".into())
            ));
        }
        self.emulator
            .as_mut()
            .ok_or_else(|| fission_core::err!(debug, "Emulator not running"))
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

    /// Attach to the emulated process this backend is already running.
    ///
    /// There is no OS process table to search: an emulated machine exists only
    /// inside this backend and only for as long as it does. So attaching means
    /// the one thing it can mean -- taking hold of the machine this backend
    /// launched -- and any other pid gets an error that says why rather than a
    /// blanket "not supported".
    fn attach(&mut self, pid: u32) -> FissionResult<()> {
        if self.emulator.is_none() {
            return Err(fission_core::err!(
                debug,
                "Nothing to attach to: an emulated process exists only inside this \
                 backend, so it has to be launched here first"
            ));
        }
        if pid != EMULATED_PID {
            return Err(fission_core::err!(
                debug,
                "No such emulated process {}: the one running here is {}",
                pid,
                EMULATED_PID
            ));
        }
        Ok(())
    }

    fn stop_reason(&self) -> Option<String> {
        let reason = match self.last_outcome? {
            RunOutcome::HitBreakpoint(address) => format!("breakpoint:0x{address:x}"),
            RunOutcome::HitWatchpoint(hit) => format!("watchpoint:0x{:x}", hit.address),
            RunOutcome::Stepped => "stepped".into(),
            RunOutcome::ProcessExited => "exited".into(),
            RunOutcome::Halted => "halted".into(),
            RunOutcome::HitBudget => "instruction-budget".into(),
            RunOutcome::Returned => "returned".into(),
            RunOutcome::SymGate => "symbolic-gate".into(),
            RunOutcome::LoopExit => "no-more-code".into(),
            RunOutcome::Interrupted => "interrupted".into(),
        };
        Some(reason)
    }

    fn set_current_thread(&mut self, thread_id: u32) -> FissionResult<()> {
        if self.emulator.is_none() {
            return Err(fission_core::err!(debug, "Emulator not running"));
        }
        if thread_id != EMULATED_THREAD_ID {
            return Err(fission_core::err!(
                debug,
                "No such thread {}: the emulator runs one guest context, which is thread {}",
                thread_id,
                EMULATED_THREAD_ID
            ));
        }
        Ok(())
    }

    /// The next thing that happened, or `None` if nothing has.
    ///
    /// `timeout_ms` is ignored: an emulated machine only runs when this
    /// backend is running it, so there is nothing that could arrive while a
    /// caller waits. Returning immediately is the honest answer -- sleeping
    /// would only make a front end slower at learning the same thing.
    fn poll_event(
        &mut self,
        timeout_ms: u32,
    ) -> FissionResult<Option<crate::debug::types::DebugEvent>> {
        let _ = timeout_ms;
        if self.emulator.is_none() {
            return Err(fission_core::err!(debug, "Emulator not running"));
        }
        // Output the guest produced but nothing has asked about yet.
        self.drain_guest_output();
        Ok(self.events.pop_front())
    }

    fn detach(&mut self) -> FissionResult<()> {
        // The machine goes with it: an emulated process has nowhere else to
        // live, so detaching is the end of it and the leftover events and
        // output position belong to a process that no longer exists.
        self.emulator = None;
        self.last_outcome = None;
        self.events.clear();
        self.stdout_reported = 0;
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
        let emu = self.runnable()?;
        let outcome = emu.resume()?;
        self.record_stop(outcome);
        Ok(())
    }

    fn single_step(&mut self) -> FissionResult<()> {
        let emu = self.runnable()?;
        // `run_instruction` runs a whole translation block; this used to call
        // it, so a "single step" advanced between one and eight instructions
        // depending on where the block boundaries fell.
        let outcome = emu.step_instruction()?;
        self.record_stop(outcome);
        Ok(())
    }

    /// Step one instruction, except that a call runs to completion.
    ///
    /// A temporary breakpoint on the call's fall-through, the way every
    /// debugger does it, plus the stack-pointer guard that makes it survive
    /// recursion: the same address is reached by the recursive call's own
    /// return, and only the frame that made the call has a stack pointer back
    /// at or above where it started.
    fn step_over(&mut self) -> FissionResult<()> {
        let emu = self.runnable()?;
        let shape = emu
            .instruction_at_pc()
            .map_err(|e| fission_core::err!(debug, "Cannot decode at 0x{:x}: {}", emu.pc, e))?;
        let outcome = if shape.is_call {
            run_past_call(emu, shape)?
        } else {
            emu.step_instruction()?
        };
        self.record_stop(outcome);
        Ok(())
    }

    /// Run until the current function returns.
    ///
    /// Stepping, except that every call inside is stepped *over* -- so the
    /// cost is the instruction count of this function's own body, and
    /// everything it calls runs compiled at full speed. That is gdb's
    /// `finish`, and it needs no unwind information, which is the point: there
    /// is none for a stripped binary.
    fn step_out(&mut self) -> FissionResult<()> {
        let emu = self.runnable()?;
        let outcome = step_out_of_frame(emu)?;
        self.record_stop(outcome);
        Ok(())
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

    /// A watchpoint, which is what a memory breakpoint is here.
    ///
    /// The native backends implement this with guard pages; the emulator sees
    /// every guest access already, so it needs no page tricks and can watch a
    /// single byte without disturbing the rest of its page.
    fn set_memory_breakpoint(
        &mut self,
        address: u64,
        size: usize,
        kind: crate::debug::types::MemoryBpKind,
    ) -> FissionResult<()> {
        use crate::debug::types::MemoryBpKind;
        let Some(emu) = &mut self.emulator else {
            return Err(fission_core::err!(debug, "Emulator not running"));
        };
        let (on_read, on_write) = match kind {
            MemoryBpKind::Read => (true, false),
            MemoryBpKind::Write => (false, true),
            MemoryBpKind::Access => (true, true),
            // Execute is a code breakpoint wearing a memory breakpoint's name;
            // answering it with a data watch would stop on nothing.
            MemoryBpKind::Execute => {
                return Err(fission_core::err!(
                    debug,
                    "Execute memory breakpoints are code breakpoints here -- use a breakpoint at 0x{:x}",
                    address
                ));
            }
        };
        emu.set_watchpoint(address, size as u64, on_read, on_write);
        Ok(())
    }

    fn remove_memory_breakpoint(&mut self, address: u64) -> FissionResult<()> {
        let Some(emu) = &mut self.emulator else {
            return Err(fission_core::err!(debug, "Emulator not running"));
        };
        if emu.clear_watchpoint(address) == 0 {
            return Err(fission_core::err!(
                debug,
                "No memory breakpoint at 0x{:x}",
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

    fn set_registers(&mut self, thread_id: u32, regs: &RegisterState) -> FissionResult<()> {
        let _ = thread_id;
        let Some(emu) = &mut self.emulator else {
            return Err(fission_core::err!(debug, "Emulator not running"));
        };
        // Every name the caller supplied, and an error naming the first one
        // this machine does not have -- writing the ones it recognises and
        // dropping the rest would leave the caller believing all of it landed.
        for (name, value) in regs.iter() {
            emu.write_register_u64(name, value)
                .map_err(|e| fission_core::err!(debug, "Cannot write {}: {}", name, e))?;
        }
        emu.pc = regs.pc;
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
        //
        // The *debugger's* view: the general-purpose registers, the stack
        // pointer and the program counter. `register_state` records every
        // register the language defines because a TTD snapshot must restore
        // them all, and a register dump of five hundred mostly-zero entries
        // is not a register dump.
        Ok(emu.debug_register_state())
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
        // A fresh machine: nothing it did before this belongs to it.
        self.last_outcome = None;
        self.events.clear();
        self.stdout_reported = 0;
        self.events
            .push_back(crate::debug::types::DebugEvent::ProcessCreated {
                pid: EMULATED_PID,
                main_thread_id: EMULATED_THREAD_ID,
            });

        // There is no OS process, so there is no pid. This is the handle the
        // rest of the debug layer uses to refer to "the emulated process".
        Ok(EMULATED_PID)
    }

    fn get_state(&self) -> crate::debug::types::DebugState {
        let mut state = crate::debug::types::DebugState::default();
        if let Some(emu) = &self.emulator {
            state.attached_pid = Some(EMULATED_PID);
            state.main_thread_id = Some(EMULATED_THREAD_ID);
            // The machine ran to the end of the program; there is nothing
            // left to step or resume, and reporting `Suspended` made a
            // finished run look like one waiting at a breakpoint.
            state.status = if self.has_finished() {
                crate::debug::types::DebugStatus::Terminated
            } else {
                crate::debug::types::DebugStatus::Suspended
            };

            // Add a single dummy thread
            state.threads.insert(
                EMULATED_THREAD_ID,
                crate::debug::types::ThreadInfo {
                    thread_id: EMULATED_THREAD_ID,
                    start_address: emu.pc,
                    suspended: true,
                    is_main: true,
                },
            );
        }
        state
    }
}

/// Resume until a call made at `shape` comes back to its fall-through.
///
/// The stack-pointer guard is what makes this right under recursion: the
/// fall-through address is also where a recursive call returns to, and only
/// the frame that made the outermost call is back at the stack pointer it had.
///
/// Reports `Returned` when its own temporary breakpoint did its job, and
/// `HitBreakpoint` only when the address was one the caller had set -- a
/// distinction `step_out` needs, since it calls this once per call
/// instruction and has to tell "the call came back" from "stop, the user
/// wanted to see this".
fn run_past_call(emu: &mut Emulator, shape: InstructionShape) -> FissionResult<RunOutcome> {
    let target = shape.fall_through();
    let sp_name = emu.arch.sp_reg;
    let sp_before = emu.read_register_u64(sp_name).unwrap_or(0);
    let user_breakpoint = emu.breakpoints().any(|address| address == target);
    if !user_breakpoint {
        emu.set_breakpoint(target);
    }

    let outcome = loop {
        let outcome = emu.resume()?;
        if outcome != RunOutcome::HitBreakpoint(target) {
            break outcome;
        }
        let sp_now = emu.read_register_u64(sp_name).unwrap_or(0);
        if sp_now >= sp_before {
            break if user_breakpoint {
                outcome
            } else {
                RunOutcome::Returned
            };
        }
        // A deeper frame returning to the same address: keep going.
    };

    if !user_breakpoint {
        emu.clear_breakpoint(target);
    }
    Ok(outcome)
}

/// Run until the current function returns.
///
/// Stepping, except that every call inside is stepped *over* -- so the cost is
/// the instruction count of this function's own body, and everything it calls
/// runs compiled at full speed. That is gdb's `finish`, and it needs no unwind
/// information, which is the point: a stripped binary has none.
fn step_out_of_frame(emu: &mut Emulator) -> FissionResult<RunOutcome> {
    // A bound, because a function that never returns would otherwise hang the
    // front end with no way to tell whether it was working.
    const MAX_STEPS: u64 = 5_000_000;
    for _ in 0..MAX_STEPS {
        let shape = emu
            .instruction_at_pc()
            .map_err(|e| fission_core::err!(debug, "Cannot decode at 0x{:x}: {}", emu.pc, e))?;
        let outcome = if shape.is_call {
            run_past_call(emu, shape)?
        } else {
            emu.step_instruction()?
        };
        // Anything else means the machine stopped for a reason the caller has
        // to see: a breakpoint inside the function, the process exiting, the
        // budget running out.
        if !matches!(outcome, RunOutcome::Stepped | RunOutcome::Returned) {
            return Ok(outcome);
        }
        if shape.is_return {
            return Ok(outcome);
        }
    }
    Err(fission_core::err!(
        debug,
        "step_out gave up after {} instructions without returning",
        MAX_STEPS
    ))
}
