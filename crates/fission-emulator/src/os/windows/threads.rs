//! Deterministic Windows guest threads, events and waits.
//!
//! Threads run cooperatively on the emulator's one host thread, the same model
//! as `os::linux::task`: a thread gives up the CPU only inside an API that
//! blocks or yields -- a wait that is not yet satisfied, `Sleep`,
//! `SwitchToThread`, or its own exit. Each thread keeps its registers and its
//! own TEB, because a thread's TLS lives there and programs read it through the
//! segment register rather than an API (the Go runtime keeps `g` in
//! `gs:[0x1480 + slot * 8]`).
//!
//! Time is the process clock (`Emulator::tick_count`). It moves at the APIs that
//! move it, and when every thread is blocked and some wait has a timeout, it
//! jumps to the earliest deadline -- so a timed wait expires exactly when
//! nothing else could have satisfied it first. This is not a preemptive
//! scheduler: a thread that spins without calling an API runs until it does.

use crate::core::Emulator;
use crate::os::env::HleResult;
use crate::pcode::page_map::prot;
use anyhow::Result;
use fission_ttd::RegisterState;
use std::collections::{BTreeMap, VecDeque};

pub const WAIT_OBJECT_0: u64 = 0;
pub const WAIT_TIMEOUT: u64 = 0x102;
pub const WAIT_FAILED: u64 = 0xFFFF_FFFF;
pub const INFINITE: u64 = 0xFFFF_FFFF;
/// `GetExitCodeThread`'s answer for a thread that has not exited.
pub const STILL_ACTIVE: u64 = 259;
/// `TlsAlloc`'s failure value.
pub const TLS_OUT_OF_INDEXES: u64 = 0xFFFF_FFFF;
/// `TlsSetValue`/`TlsGetValue` reach these slots; `TlsExpansionSlots` is not
/// modelled.
pub const TLS_MINIMUM_AVAILABLE: u64 = 64;

/// The thread the process starts with; `GetCurrentThreadId` has always
/// answered 1000 for it.
pub const ROOT_TID: u64 = 1000;

/// Handles for the objects this module owns. Clear of the console cookies
/// (0x50..) and the file handles the VFS hands out.
const FIRST_HANDLE: u64 = 0x1_0000;

/// TEBs of threads after the first: one region, below the CRT stream page and
/// clear of everything else the loader places at fixed addresses.
const THREAD_TEB_BASE: u64 = 0x7FF0_0000;
const THREAD_TEB_END: u64 = 0x7FFB_0000;

/// A thread's stack when its creator asks for the default, and the unmapped
/// gap left below each one.
const DEFAULT_STACK: u64 = 0x10_0000;
const STACK_GUARD: u64 = 0x1_0000;

/// TEB layout, by pointer size: (StackBase, StackLimit, Self, ClientId.UniqueThread,
/// ProcessEnvironmentBlock, TlsSlots, size of the mapping).
struct TebLayout {
    stack_base: u64,
    stack_limit: u64,
    self_ptr: u64,
    unique_process: u64,
    unique_thread: u64,
    peb: u64,
    tls_slots: u64,
    size: u64,
}

impl TebLayout {
    fn for_pointer_size(pointer_size: u8) -> Self {
        if pointer_size >= 8 {
            Self {
                stack_base: 0x08,
                stack_limit: 0x10,
                self_ptr: 0x30,
                unique_process: 0x40,
                unique_thread: 0x48,
                peb: 0x60,
                tls_slots: 0x1480,
                size: 0x2000,
            }
        } else {
            Self {
                stack_base: 0x04,
                stack_limit: 0x08,
                self_ptr: 0x18,
                unique_process: 0x20,
                unique_thread: 0x24,
                peb: 0x30,
                tls_slots: 0xE10,
                size: 0x1000,
            }
        }
    }
}

/// How a thread's start routine is called, which decides where its argument
/// goes on a 32-bit stack. Both put it in the first argument register on x64.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartConvention {
    /// `CreateThread`, `_beginthreadex`: `stdcall`, returns the exit code.
    Stdcall,
    /// `_beginthread`: `cdecl`, returns nothing.
    Cdecl,
}

#[derive(Clone)]
struct Context {
    registers: RegisterState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Wait {
    handles: Vec<u64>,
    all: bool,
    /// In process-clock milliseconds; `None` waits forever.
    deadline: Option<u64>,
    /// Order the wait began in, so waiters are served first come first served
    /// and the earliest of equal deadlines expires first.
    seq: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Status {
    Running,
    Runnable,
    Suspended,
    Waiting(Wait),
    Exited(u32),
}

struct Thread {
    handle: u64,
    teb: u64,
    /// Saved while the thread is not running.
    context: Option<Context>,
    status: Status,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Object {
    Event { manual: bool, signaled: bool },
    Thread(u64),
}

#[derive(Default)]
pub struct WindowsThreads {
    current: Option<u64>,
    threads: BTreeMap<u64, Thread>,
    runnable: VecDeque<u64>,
    objects: BTreeMap<u64, Object>,
    next_handle: u64,
    next_tid: u64,
    wait_seq: u64,
    next_teb: u64,
    next_stack_top: u64,
    tls_used: u64,
    /// The exit code of the thread that exited last, which becomes the
    /// process's once no thread is left.
    last_exit: Option<u32>,
}

impl WindowsThreads {
    /// The thread that is running now.
    pub fn current_tid(&self) -> u64 {
        self.current.unwrap_or(ROOT_TID)
    }

    /// Whether any thread besides the first was ever created. Until one is,
    /// every API here answers exactly as it did before threads existed.
    pub fn has_other_threads(&self) -> bool {
        self.threads.len() > 1
    }

    /// The real handle of the running thread, for `DuplicateHandle` on the
    /// `GetCurrentThread()` pseudo-handle.
    pub fn current_handle(&mut self, emu: &Emulator) -> u64 {
        self.ensure_root(emu);
        let tid = self.current_tid();
        self.threads.get(&tid).map(|t| t.handle).unwrap_or(0)
    }

    fn ensure_root(&mut self, emu: &Emulator) {
        if !self.threads.is_empty() {
            return;
        }
        self.next_handle = self.next_handle.max(FIRST_HANDLE);
        self.next_tid = self.next_tid.max(ROOT_TID + 4);
        let handle = self.alloc_handle(Object::Thread(ROOT_TID));
        self.threads.insert(
            ROOT_TID,
            Thread {
                handle,
                teb: segment_base(emu),
                context: None,
                status: Status::Running,
            },
        );
        self.current = Some(ROOT_TID);
    }

    fn alloc_handle(&mut self, object: Object) -> u64 {
        self.next_handle = self.next_handle.max(FIRST_HANDLE);
        let handle = self.next_handle;
        self.next_handle += 4;
        self.objects.insert(handle, object);
        handle
    }

    // ── Events ──────────────────────────────────────────────────────────────

    pub fn create_event(&mut self, emu: &Emulator, manual: bool, signaled: bool) -> u64 {
        self.ensure_root(emu);
        self.alloc_handle(Object::Event { manual, signaled })
    }

    /// `SetEvent`. Wakes whoever the event now satisfies; the caller keeps
    /// running. `false` for a handle that is not an event.
    pub fn set_event(&mut self, emu: &Emulator, handle: u64) -> bool {
        match self.objects.get_mut(&handle) {
            Some(Object::Event { signaled, .. }) => *signaled = true,
            _ => return false,
        }
        self.wake_satisfied(emu);
        true
    }

    pub fn reset_event(&mut self, handle: u64) -> bool {
        match self.objects.get_mut(&handle) {
            Some(Object::Event { signaled, .. }) => {
                *signaled = false;
                true
            }
            _ => false,
        }
    }

    fn is_signaled(&self, handle: u64) -> bool {
        match self.objects.get(&handle) {
            Some(Object::Event { signaled, .. }) => *signaled,
            Some(Object::Thread(tid)) => self
                .threads
                .get(tid)
                .is_some_and(|t| matches!(t.status, Status::Exited(_))),
            // The process itself is signalled only once it has exited, so a
            // wait on it times out -- the Go runtime sleeps that way,
            // `WaitForSingleObject(INVALID_HANDLE_VALUE, ms)`, and returning
            // at once made its monitor thread spin.
            None if is_process_handle(handle) => false,
            // Not ours: a file, a console. Waits on those have always
            // succeeded at once, and still do.
            None => true,
        }
    }

    /// Satisfying a wait consumes an auto-reset event.
    fn consume(&mut self, handle: u64) {
        if let Some(Object::Event {
            manual: false,
            signaled,
        }) = self.objects.get_mut(&handle)
        {
            *signaled = false;
        }
    }

    /// The `WAIT_OBJECT_0 + n` a wait would return now, consuming what it
    /// takes; `None` if it would block.
    fn try_satisfy(&mut self, handles: &[u64], all: bool) -> Option<u64> {
        if handles.is_empty() {
            return None;
        }
        if all {
            if handles.iter().all(|&h| self.is_signaled(h)) {
                for &h in handles {
                    self.consume(h);
                }
                return Some(WAIT_OBJECT_0);
            }
            return None;
        }
        let index = handles.iter().position(|&h| self.is_signaled(h))?;
        self.consume(handles[index]);
        Some(WAIT_OBJECT_0 + index as u64)
    }

    /// Make runnable every waiting thread whose wait is now satisfied, in the
    /// order the waits began.
    fn wake_satisfied(&mut self, emu: &Emulator) {
        let mut waiting: Vec<(u64, u64)> = self
            .threads
            .iter()
            .filter_map(|(&tid, t)| match &t.status {
                Status::Waiting(w) => Some((w.seq, tid)),
                _ => None,
            })
            .collect();
        waiting.sort();
        for (_, tid) in waiting {
            let Some(Status::Waiting(wait)) = self.threads.get(&tid).map(|t| t.status.clone())
            else {
                continue;
            };
            if let Some(result) = self.try_satisfy(&wait.handles, wait.all) {
                self.make_runnable(emu, tid, result);
            }
        }
    }

    fn make_runnable(&mut self, emu: &Emulator, tid: u64, result: u64) {
        let Some(thread) = self.threads.get_mut(&tid) else {
            return;
        };
        if let Some(context) = thread.context.as_mut() {
            context.registers.set(emu.arch.cc.return_reg(), result);
        }
        thread.status = Status::Runnable;
        self.runnable.push_back(tid);
    }

    // ── Waiting and scheduling ──────────────────────────────────────────────

    /// `WaitForSingleObject` / `WaitForMultipleObjects`, after the arguments
    /// are read. `api` is the name the call came in under, for the return.
    pub fn wait(
        &mut self,
        emu: &mut Emulator,
        api: &str,
        handles: Vec<u64>,
        all: bool,
        timeout_ms: u64,
    ) -> Result<HleResult> {
        self.ensure_root(emu);
        if let Some(result) = self.try_satisfy(&handles, all) {
            emu.write_return_val(result)?;
            return Ok(HleResult::Continue);
        }
        if timeout_ms == 0 {
            emu.write_return_val(WAIT_TIMEOUT)?;
            return Ok(HleResult::Continue);
        }
        let deadline = (timeout_ms != INFINITE).then(|| emu.tick_count.wrapping_add(timeout_ms));
        self.block(emu, api, handles, all, deadline)
    }

    /// `Sleep(ms)`.
    pub fn sleep(&mut self, emu: &mut Emulator, api: &str, ms: u64) -> Result<HleResult> {
        self.ensure_root(emu);
        if ms == 0 {
            return self.yield_now(emu, api);
        }
        let deadline = (ms != INFINITE).then(|| emu.tick_count.wrapping_add(ms));
        emu.write_return_val(0)?;
        self.block(emu, api, Vec::new(), false, deadline)
    }

    /// `SwitchToThread`, `Sleep(0)`: let a runnable thread go first, if there
    /// is one. Returns whether there was.
    pub fn yield_now(&mut self, emu: &mut Emulator, api: &str) -> Result<HleResult> {
        self.ensure_root(emu);
        let me = self.current_tid();
        let others = self.runnable.iter().any(|&tid| {
            tid != me
                && self
                    .threads
                    .get(&tid)
                    .is_some_and(|t| t.status == Status::Runnable)
        });
        emu.write_return_val(u64::from(others))?;
        if !others {
            return Ok(HleResult::Continue);
        }
        self.save_current(emu, api)?;
        if let Some(thread) = self.threads.get_mut(&me) {
            thread.status = Status::Runnable;
        }
        self.runnable.push_back(me);
        self.switch(emu)
    }

    /// Return from `api` in the running thread, park it with `handles`, and
    /// run something else.
    fn block(
        &mut self,
        emu: &mut Emulator,
        api: &str,
        handles: Vec<u64>,
        all: bool,
        deadline: Option<u64>,
    ) -> Result<HleResult> {
        self.save_current(emu, api)?;
        self.wait_seq += 1;
        let me = self.current_tid();
        if let Some(thread) = self.threads.get_mut(&me) {
            thread.status = Status::Waiting(Wait {
                handles,
                all,
                deadline,
                seq: self.wait_seq,
            });
        }
        self.switch(emu)
    }

    /// Complete `api`'s return in the running thread and keep its registers.
    fn save_current(&mut self, emu: &mut Emulator, api: &str) -> Result<()> {
        emu.simulate_return_for(api)?;
        let me = self.current_tid();
        let context = Context {
            registers: emu.register_state_at(emu.pc),
        };
        if let Some(thread) = self.threads.get_mut(&me) {
            thread.context = Some(context);
        }
        Ok(())
    }

    fn pop_runnable(&mut self) -> Option<u64> {
        while let Some(tid) = self.runnable.pop_front() {
            if self
                .threads
                .get(&tid)
                .is_some_and(|t| t.status == Status::Runnable)
            {
                return Some(tid);
            }
        }
        None
    }

    /// Run the next runnable thread. With none, let the clock reach the
    /// earliest deadline and expire that wait; with none of those either,
    /// every thread has exited or the process is deadlocked.
    fn switch(&mut self, emu: &mut Emulator) -> Result<HleResult> {
        if let Some(tid) = self.pop_runnable() {
            return self.activate(emu, tid);
        }

        let earliest = self
            .threads
            .iter()
            .filter_map(|(&tid, t)| match &t.status {
                Status::Waiting(Wait {
                    deadline: Some(deadline),
                    seq,
                    ..
                }) => Some((*deadline, *seq, tid)),
                _ => None,
            })
            .min();
        if let Some((deadline, _, tid)) = earliest {
            if deadline > emu.tick_count {
                crate::os::windows::hle::advance_clock(emu, deadline - emu.tick_count);
            }
            let sleeping = matches!(
                self.threads.get(&tid).map(|t| &t.status),
                Some(Status::Waiting(w)) if w.handles.is_empty()
            );
            self.make_runnable(emu, tid, if sleeping { 0 } else { WAIT_TIMEOUT });
            let tid = self.pop_runnable().expect("just made runnable");
            return self.activate(emu, tid);
        }

        if let Some(code) = self.process_exit_code() {
            return Ok(HleResult::Halt(code));
        }
        emu.metrics.exit_reason = Some("guest_deadlock".into());
        Ok(HleResult::Deadlock)
    }

    /// When every thread has exited, the process exits with the code of the
    /// last one, as Windows does.
    fn process_exit_code(&self) -> Option<u32> {
        if self
            .threads
            .values()
            .all(|t| matches!(t.status, Status::Exited(_)))
        {
            return self.last_exit;
        }
        None
    }

    fn activate(&mut self, emu: &mut Emulator, tid: u64) -> Result<HleResult> {
        let Some(thread) = self.threads.get_mut(&tid) else {
            return Ok(HleResult::Deadlock);
        };
        thread.status = Status::Running;
        let teb = thread.teb;
        let context = thread
            .context
            .take()
            .ok_or_else(|| anyhow::anyhow!("thread {tid} has no saved context"))?;
        self.current = Some(tid);
        emu.restore_thread_register_state(&context.registers)?;
        set_segment_base(emu, teb);
        emu.current_tid = tid;
        emu.pc_override = Some(context.registers.pc);
        Ok(HleResult::Schedule)
    }

    // ── Thread lifetime ─────────────────────────────────────────────────────

    /// Create a thread that starts at `start` with `parameter`, returning
    /// `(handle, tid)`. It runs the first time the running thread blocks or
    /// yields. `return_stub` is where its start routine returns to, which
    /// ends the thread with the routine's return value.
    #[allow(clippy::too_many_arguments)]
    pub fn create_thread(
        &mut self,
        emu: &mut Emulator,
        start: u64,
        parameter: u64,
        stack_size: u64,
        suspended: bool,
        convention: StartConvention,
        return_stub: u64,
    ) -> Result<(u64, u64)> {
        self.ensure_root(emu);
        let pointer_size = u64::from(emu.arch.pointer_size);
        let layout = TebLayout::for_pointer_size(emu.arch.pointer_size);
        let root_teb = self.threads.get(&ROOT_TID).map(|t| t.teb).unwrap_or(0);

        // The stack, below the previous one.
        if self.next_stack_top == 0 {
            let main_limit = read_pointer(emu, root_teb + layout.stack_limit)?;
            anyhow::ensure!(main_limit != 0, "no main stack limit in the TEB");
            self.next_stack_top = main_limit - STACK_GUARD;
        }
        let size = stack_size.max(DEFAULT_STACK).next_multiple_of(STACK_GUARD);
        let stack_top = self.next_stack_top;
        let stack_limit = stack_top
            .checked_sub(size)
            .ok_or_else(|| anyhow::anyhow!("no room for a thread stack"))?;
        self.next_stack_top = stack_limit - STACK_GUARD;
        emu.state
            .page_map
            .map_region(stack_limit, size, prot::RW | prot::ANON, true);

        // Its TEB.
        if self.next_teb == 0 {
            self.next_teb = THREAD_TEB_BASE;
        }
        let teb = self.next_teb;
        anyhow::ensure!(
            teb + layout.size <= THREAD_TEB_END,
            "no room for another TEB"
        );
        self.next_teb += layout.size;
        emu.state
            .page_map
            .map_region(teb, layout.size, prot::RW | prot::ANON, true);
        let tid = self.next_tid.max(ROOT_TID + 4);
        self.next_tid = tid + 4;
        let peb = read_pointer(emu, root_teb + layout.peb)?;
        write_pointer(emu, teb, u64::MAX)?; // no SEH frame
        write_pointer(emu, teb + layout.stack_base, stack_top)?;
        write_pointer(emu, teb + layout.stack_limit, stack_limit)?;
        write_pointer(emu, teb + layout.self_ptr, teb)?;
        let pid = read_pointer(emu, root_teb + layout.unique_process)?;
        write_pointer(emu, teb + layout.unique_process, pid)?;
        write_pointer(emu, teb + layout.unique_thread, tid)?;
        write_pointer(emu, teb + layout.peb, peb)?;

        // The call into the start routine: return address on the stack, the
        // argument where the convention puts it.
        let mut registers = emu.register_state_at(start);
        let sp = if pointer_size >= 8 {
            // Entry RSP is 8 past a 16-byte boundary, with the 32-byte home
            // area above the return address belonging to the caller.
            let sp = (stack_top & !0xF) - 0x30 - 8;
            write_pointer(emu, sp, return_stub)?;
            let first_arg = emu.arch.cc.arg_regs()[0];
            registers.set(first_arg, parameter);
            sp
        } else {
            let sp = (stack_top & !0xF) - 0x10;
            write_pointer(emu, sp, return_stub)?;
            write_pointer(emu, sp + pointer_size, parameter)?;
            sp
        };
        registers.set(emu.arch.sp_reg, sp);
        // The layout above serves both conventions: a `stdcall` routine pops
        // its argument on return and a `cdecl` one does not, and either way
        // the return lands on the stub, which does not look at the stack.
        let _ = convention;

        let handle = self.alloc_handle(Object::Thread(tid));
        self.threads.insert(
            tid,
            Thread {
                handle,
                teb,
                context: Some(Context { registers }),
                status: if suspended {
                    Status::Suspended
                } else {
                    Status::Runnable
                },
            },
        );
        if !suspended {
            self.runnable.push_back(tid);
        }
        Ok((handle, tid))
    }

    /// `ResumeThread`: the previous suspend count, or `WAIT_FAILED` (-1) for a
    /// handle that is not a thread.
    pub fn resume_thread(&mut self, handle: u64) -> u64 {
        let Some(Object::Thread(tid)) = self.objects.get(&handle).copied() else {
            return WAIT_FAILED;
        };
        match self.threads.get_mut(&tid) {
            Some(thread) if thread.status == Status::Suspended => {
                thread.status = Status::Runnable;
                self.runnable.push_back(tid);
                1
            }
            Some(_) => 0,
            None => WAIT_FAILED,
        }
    }

    /// `GetExitCodeThread`.
    pub fn exit_code(&self, handle: u64) -> Option<u64> {
        let Some(Object::Thread(tid)) = self.objects.get(&handle) else {
            return None;
        };
        self.threads.get(tid).map(|t| match t.status {
            Status::Exited(code) => u64::from(code),
            _ => STILL_ACTIVE,
        })
    }

    /// End the running thread with `code`. Whoever waited on it wakes; the
    /// process exits once no thread is left.
    pub fn exit_current(&mut self, emu: &mut Emulator, code: u32) -> Result<HleResult> {
        self.ensure_root(emu);
        let me = self.current_tid();
        if let Some(thread) = self.threads.get_mut(&me) {
            thread.status = Status::Exited(code);
            thread.context = None;
        }
        self.last_exit = Some(code);
        self.wake_satisfied(emu);
        self.switch(emu)
    }

    // ── TLS ─────────────────────────────────────────────────────────────────

    pub fn tls_alloc(&mut self) -> u64 {
        for index in 0..TLS_MINIMUM_AVAILABLE {
            if self.tls_used & (1 << index) == 0 {
                self.tls_used |= 1 << index;
                return index;
            }
        }
        TLS_OUT_OF_INDEXES
    }

    pub fn tls_free(&mut self, index: u64) -> bool {
        if index >= TLS_MINIMUM_AVAILABLE || self.tls_used & (1 << index) == 0 {
            return false;
        }
        self.tls_used &= !(1 << index);
        true
    }

    /// The running thread's slot `index`, in its TEB.
    pub fn tls_slot(&self, emu: &Emulator, index: u64) -> Option<u64> {
        if index >= TLS_MINIMUM_AVAILABLE {
            return None;
        }
        let layout = TebLayout::for_pointer_size(emu.arch.pointer_size);
        Some(segment_base(emu) + layout.tls_slots + index * u64::from(emu.arch.pointer_size))
    }
}

/// The current process: the `GetCurrentProcess()` pseudo-handle (-1, which is
/// also `INVALID_HANDLE_VALUE`) at either pointer width, and the real handle
/// `DuplicateHandle` gives for it.
fn is_process_handle(handle: u64) -> bool {
    handle == u64::MAX
        || handle == 0xFFFF_FFFF
        || handle == crate::os::windows::hle::DUPLICATED_SELF_HANDLE
}

/// The TEB of the running thread: `gs` on x64, `fs` on x86.
pub fn segment_base(emu: &Emulator) -> u64 {
    if emu.arch.pointer_size >= 8 {
        emu.gs_base
    } else {
        emu.fs_base
    }
}

fn set_segment_base(emu: &mut Emulator, teb: u64) {
    if emu.arch.pointer_size >= 8 {
        emu.set_gs_base(teb);
    } else {
        emu.set_fs_base(teb);
    }
}

pub fn read_pointer(emu: &mut Emulator, address: u64) -> Result<u64> {
    let size = usize::from(emu.arch.pointer_size);
    let ram = emu.state.ram_space();
    let bytes = emu.state.read_space(ram, address, size)?;
    Ok(crate::arch::calling_convention::le_bytes_to_u64(&bytes))
}

pub fn write_pointer(emu: &mut Emulator, address: u64, value: u64) -> Result<()> {
    let size = usize::from(emu.arch.pointer_size);
    let ram = emu.state.ram_space();
    emu.state
        .write_space(ram, address, &value.to_le_bytes()[..size])?;
    Ok(())
}
