//! Deterministic Linux guest threads and futex scheduling.
//!
//! The scheduler runs guest tasks cooperatively on the emulator's one host
//! thread. Tasks share the process RAM, VFS, and syscall table; each task keeps
//! its own CPU registers, TLS base, signal mask, pending thread signal, and
//! `clear_child_tid` pointer. A task yields only at a supported scheduling
//! syscall (`futex`, `sched_yield`, or thread exit). This is not a preemptive
//! Linux scheduler or a process model.

use crate::arch::Endianness;
use crate::core::Emulator;
use crate::os::env::HleResult;
use crate::os::linux::syscall_conv::SyscallAbi;
use anyhow::Result;
use fission_ttd::RegisterState;
use std::collections::{BTreeMap, VecDeque};

const ESRCH: i64 = 3;
const EINTR: i64 = 4;
const EAGAIN: i64 = 11;
const EFAULT: i64 = 14;
const EINVAL: i64 = 22;
const ENOSYS: i64 = 38;
const ETIMEDOUT: i64 = 110;

const FUTEX_WAIT: u64 = 0;
const FUTEX_WAKE: u64 = 1;
const FUTEX_PRIVATE_FLAG: u64 = 0x80;
const FUTEX_CMD_MASK: u64 = 0x7f;

const CLONE_VM: u64 = 0x0000_0100;
const CLONE_FS: u64 = 0x0000_0200;
const CLONE_FILES: u64 = 0x0000_0400;
const CLONE_SIGHAND: u64 = 0x0000_0800;
const CLONE_THREAD: u64 = 0x0001_0000;
const CLONE_SYSVSEM: u64 = 0x0004_0000;
const CLONE_SETTLS: u64 = 0x0008_0000;
const CLONE_PARENT_SETTID: u64 = 0x0010_0000;
const CLONE_CHILD_CLEARTID: u64 = 0x0020_0000;
const CLONE_CHILD_SETTID: u64 = 0x0100_0000;

const REQUIRED_THREAD_FLAGS: u64 = CLONE_VM | CLONE_FS | CLONE_FILES | CLONE_SIGHAND | CLONE_THREAD;
const SUPPORTED_CLONE_FLAGS: u64 = REQUIRED_THREAD_FLAGS
    | CLONE_SYSVSEM
    | CLONE_SETTLS
    | CLONE_PARENT_SETTID
    | CLONE_CHILD_CLEARTID
    | CLONE_CHILD_SETTID;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ThreadSignals {
    pending: u64,
    blocked: u64,
    return_pc: Option<u64>,
    current: Option<i32>,
}

impl ThreadSignals {
    fn capture(emu: &Emulator) -> Self {
        Self {
            pending: emu.signals.pending,
            blocked: emu.signals.blocked,
            return_pc: emu.signals.return_pc,
            current: emu.signals.current,
        }
    }

    fn restore(self, emu: &mut Emulator) {
        emu.signals.pending = self.pending;
        emu.signals.blocked = self.blocked;
        emu.signals.return_pc = self.return_pc;
        emu.signals.current = self.current;
    }
}

#[derive(Clone)]
struct ThreadContext {
    registers: RegisterState,
    fs_base: u64,
    gs_base: u64,
    clear_child_tid: u64,
    signals: ThreadSignals,
}

impl ThreadContext {
    fn capture(emu: &mut Emulator, resume_pc: u64) -> Self {
        Self {
            registers: emu.register_state_at(resume_pc),
            fs_base: emu.fs_base,
            gs_base: emu.gs_base,
            clear_child_tid: emu.clear_child_tid,
            signals: ThreadSignals::capture(emu),
        }
    }

    fn restore(&self, tid: u64, emu: &mut Emulator) -> Result<()> {
        emu.restore_thread_register_state(&self.registers)?;
        emu.fs_base = self.fs_base;
        emu.gs_base = self.gs_base;
        emu.clear_child_tid = self.clear_child_tid;
        emu.current_tid = tid;
        self.signals.restore(emu);
        Ok(())
    }

    fn set_syscall_result(&mut self, emu: &Emulator, value: u64) -> bool {
        set_context_register(
            emu,
            &mut self.registers,
            SyscallAbi::for_arch(&emu.arch).result,
            value,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TaskStatus {
    Running,
    Runnable,
    FutexWait { address: u64, timed: bool },
    Exited(u32),
}

struct GuestTask {
    context: ThreadContext,
    status: TaskStatus,
}

/// Per-process cooperative thread state. `LinuxEnv` protects this with a
/// mutex because its HLE interface is shared; guest execution itself remains
/// single-host-thread and deterministic.
#[derive(Default)]
pub struct LinuxTaskScheduler {
    current_tid: Option<u64>,
    root_tid: Option<u64>,
    next_tid: u64,
    tasks: BTreeMap<u64, GuestTask>,
    runnable: VecDeque<u64>,
    futex_waiters: BTreeMap<u64, VecDeque<u64>>,
}

impl LinuxTaskScheduler {
    /// Handle the Linux `clone` thread subset. Process-creation flags, unknown
    /// flags, `clone3`, and architectures without a declared clone ABI return
    /// `ENOSYS` before creating a partially configured task.
    pub fn clone_thread(&mut self, emu: &mut Emulator) -> Result<HleResult> {
        let Some(layout) = CloneLayout::for_arch(emu) else {
            return set_errno(emu, ENOSYS);
        };
        self.ensure_current(emu);

        let flags = emu.syscall_arg(0);
        let child_stack = emu.syscall_arg(1);
        let parent_tid = emu.syscall_arg(2);
        let tls = emu.syscall_arg(layout.tls_arg);
        let child_tid = emu.syscall_arg(layout.child_tid_arg);

        if flags & 0xff != 0
            || flags & REQUIRED_THREAD_FLAGS != REQUIRED_THREAD_FLAGS
            || flags & !SUPPORTED_CLONE_FLAGS != 0
        {
            return set_errno(emu, ENOSYS);
        }
        if child_stack == 0 || child_stack & 0xf != 0 {
            return set_errno(emu, EINVAL);
        }
        if emu.state.enforce_page_faults
            && emu
                .state
                .page_map
                .check_range(
                    child_stack.saturating_sub(1),
                    1,
                    crate::pcode::page_map::AccessKind::Write,
                )
                .is_err()
        {
            return set_errno(emu, EFAULT);
        }

        let writes_parent_tid = flags & CLONE_PARENT_SETTID != 0;
        let writes_child_tid = flags & CLONE_CHILD_SETTID != 0;
        for (ptr, enabled) in [
            (parent_tid, writes_parent_tid),
            (child_tid, writes_child_tid),
        ] {
            if enabled
                && (ptr == 0
                    || ptr & 3 != 0
                    || (emu.state.enforce_page_faults
                        && emu
                            .state
                            .page_map
                            .check_range(ptr, 4, crate::pcode::page_map::AccessKind::Write)
                            .is_err()))
            {
                return set_errno(emu, EFAULT);
            }
        }

        if flags & CLONE_SETTLS != 0 && !layout.can_set_tls(emu) {
            return set_errno(emu, ENOSYS);
        }

        let tid = self.allocate_tid();
        emu.set_syscall_return(tid)?;
        let resume_pc = emu.current_instruction_fallthrough();
        let parent_context = ThreadContext::capture(emu, resume_pc);
        let mut child_context = parent_context.clone();
        child_context.set_syscall_result(emu, 0);
        let sp_name = emu.arch.sp_reg;
        if !set_context_register(emu, &mut child_context.registers, sp_name, child_stack) {
            return set_errno(emu, ENOSYS);
        }
        child_context.clear_child_tid = if flags & CLONE_CHILD_CLEARTID != 0 {
            child_tid
        } else {
            0
        };
        child_context.signals.pending = 0;
        child_context.signals.return_pc = None;
        child_context.signals.current = None;

        if flags & CLONE_SETTLS != 0 && !layout.set_tls(&mut child_context, emu, tls) {
            return set_errno(emu, ENOSYS);
        }

        if writes_parent_tid {
            if write_tid(emu, parent_tid, tid).is_err() {
                return set_errno(emu, EFAULT);
            }
        }
        if writes_child_tid {
            if write_tid(emu, child_tid, tid).is_err() {
                return set_errno(emu, EFAULT);
            }
        }

        let parent = self.current_tid.expect("ensure_current set the root task");
        self.tasks.insert(
            parent,
            GuestTask {
                context: parent_context,
                status: TaskStatus::Running,
            },
        );
        self.tasks.insert(
            tid,
            GuestTask {
                context: child_context,
                status: TaskStatus::Runnable,
            },
        );
        self.runnable.push_back(tid);
        Ok(HleResult::Continue)
    }

    /// Handle the supported `FUTEX_WAIT[_PRIVATE]` and
    /// `FUTEX_WAKE[_PRIVATE]` commands. Positive timeouts are deterministic
    /// scheduler timers: they expire when no runnable task remains. A zero
    /// timeout expires immediately.
    pub fn futex(&mut self, emu: &mut Emulator) -> Result<HleResult> {
        let address = emu.syscall_arg(0);
        let raw_op = emu.syscall_arg(1);
        let command = raw_op & FUTEX_CMD_MASK;
        let value = emu.syscall_arg(2);

        if raw_op & !(FUTEX_CMD_MASK | FUTEX_PRIVATE_FLAG) != 0 {
            return set_errno(emu, ENOSYS);
        }
        if address == 0 || address & 3 != 0 {
            return set_errno(emu, EINVAL);
        }
        if command != FUTEX_WAIT && command != FUTEX_WAKE {
            tracing::warn!("Unsupported Linux futex command: {}", command);
            return set_errno(emu, ENOSYS);
        }
        let current = match emu.state.read_space(emu.state.ram_space(), address, 4) {
            Ok(bytes) => read_guest_u32(emu, bytes.try_into().unwrap_or([0; 4])),
            Err(_) => return set_errno(emu, EFAULT),
        };

        match command {
            FUTEX_WAIT => {
                if u64::from(current) != (value & u64::from(u32::MAX)) {
                    return set_errno(emu, EAGAIN);
                }
                if emu.signals.pending & !emu.signals.blocked != 0 {
                    return set_errno(emu, EINTR);
                }
                let timeout_ptr = emu.syscall_arg(3);
                let timed = match timeout_is_zero_or_valid(emu, timeout_ptr) {
                    Ok(is_zero) => is_zero,
                    Err(errno) => return set_errno(emu, errno),
                };
                if timed == Some(true) {
                    return set_errno(emu, ETIMEDOUT);
                }

                self.ensure_current(emu);
                let tid = self.current_tid.expect("ensure_current set the root task");
                let context = ThreadContext::capture(emu, emu.current_instruction_fallthrough());
                self.tasks.insert(
                    tid,
                    GuestTask {
                        context,
                        status: TaskStatus::FutexWait {
                            address,
                            timed: timed == Some(false),
                        },
                    },
                );
                self.futex_waiters
                    .entry(address)
                    .or_default()
                    .push_back(tid);
                self.current_tid = None;
                self.schedule_or_deadlock(emu)
            }
            FUTEX_WAKE => {
                self.ensure_current(emu);
                let maximum = usize::try_from(value).unwrap_or(usize::MAX);
                let woken = self.wake_futex(address, maximum, 0, emu);
                emu.set_syscall_return(woken)?;
                Ok(HleResult::Continue)
            }
            _ => unreachable!("futex command was filtered above"),
        }
    }

    /// Yield the current guest task to the next runnable task.
    pub fn sched_yield(&mut self, emu: &mut Emulator) -> Result<HleResult> {
        self.ensure_current(emu);
        let current = self.current_tid.expect("ensure_current set the root task");
        emu.set_syscall_return(0)?;
        let context = ThreadContext::capture(emu, emu.current_instruction_fallthrough());
        self.tasks.insert(
            current,
            GuestTask {
                context,
                status: TaskStatus::Runnable,
            },
        );
        self.runnable.push_back(current);

        if let Some(next_tid) = self.pop_runnable_except(current) {
            self.activate(next_tid, emu)
        } else {
            if let Some(task) = self.tasks.get_mut(&current) {
                task.status = TaskStatus::Running;
            }
            self.runnable.retain(|tid| *tid != current);
            Ok(HleResult::Continue)
        }
    }

    /// Return the identity of the currently running guest task.
    pub fn gettid(&mut self, emu: &mut Emulator) -> Result<HleResult> {
        self.ensure_current(emu);
        let tid = self.current_tid.expect("ensure_current set the root task");
        emu.set_syscall_return(tid)?;
        Ok(HleResult::Continue)
    }

    /// Set the task-local address cleared and woken when this task exits.
    pub fn set_tid_address(&mut self, emu: &mut Emulator) -> Result<HleResult> {
        let address = emu.syscall_arg(0);
        self.ensure_current(emu);
        let tid = self.current_tid.expect("ensure_current set the root task");
        emu.clear_child_tid = address;
        emu.set_syscall_return(tid)?;
        Ok(HleResult::Continue)
    }

    /// Terminate one Linux guest task. The process ends only after every task
    /// exits; `exit_group` remains a process-wide halt in `LinuxEnv`.
    pub fn exit_thread(&mut self, emu: &mut Emulator) -> Result<HleResult> {
        self.ensure_current(emu);
        let tid = self.current_tid.expect("ensure_current set the root task");
        let code = (emu.syscall_arg(0) as u32) & 0xff;
        let clear_child_tid = emu.clear_child_tid;
        if clear_child_tid != 0 {
            let _ = write_tid(emu, clear_child_tid, 0);
            self.wake_futex(clear_child_tid, 1, 0, emu);
        }
        if let Some(task) = self.tasks.get_mut(&tid) {
            task.status = TaskStatus::Exited(code);
        }
        self.current_tid = None;
        self.schedule_or_deadlock(emu)
    }

    /// Deliver a signal to one modeled guest task. A signal interrupts an
    /// unmasked futex wait and is delivered when that task is scheduled again.
    pub fn tkill(&mut self, emu: &mut Emulator) -> Result<HleResult> {
        self.ensure_current(emu);
        let target_tid = emu.syscall_arg(0);
        let signo = emu.syscall_arg(1) as i32;
        let Some(target) = self.tasks.get_mut(&target_tid) else {
            return set_errno(emu, ESRCH);
        };
        if matches!(target.status, TaskStatus::Exited(_)) {
            return set_errno(emu, ESRCH);
        }
        if !(0..=64).contains(&signo) {
            return set_errno(emu, EINVAL);
        }
        emu.set_syscall_return(0)?;
        if signo == 0 {
            return Ok(HleResult::Continue);
        }

        if target_tid == self.current_tid.unwrap_or(0) {
            emu.raise_signal(signo);
            return Ok(HleResult::Continue);
        }

        let bit = 1u64 << (signo as u32 - 1);
        target.context.signals.pending |= bit;
        let should_interrupt = matches!(target.status, TaskStatus::FutexWait { .. })
            && target.context.signals.blocked & bit == 0;
        if should_interrupt {
            self.make_runnable(target_tid, (-EINTR) as u64, emu);
        }
        Ok(HleResult::Continue)
    }

    fn ensure_current(&mut self, emu: &mut Emulator) {
        let tid = self
            .current_tid
            .unwrap_or_else(|| emu.current_tid.max(1000));
        self.current_tid = Some(tid);
        if self.root_tid.is_none() {
            self.root_tid = Some(tid);
        }
        self.next_tid = self.next_tid.max(tid.saturating_add(1));
        self.tasks.entry(tid).or_insert_with(|| GuestTask {
            context: ThreadContext::capture(emu, emu.pc),
            status: TaskStatus::Running,
        });
        if let Some(task) = self.tasks.get_mut(&tid) {
            task.status = TaskStatus::Running;
        }
        emu.current_tid = tid;
    }

    fn allocate_tid(&mut self) -> u64 {
        if self.next_tid < 1001 {
            self.next_tid = 1001;
        }
        let tid = self.next_tid;
        self.next_tid = self.next_tid.saturating_add(1);
        tid
    }

    fn pop_runnable(&mut self) -> Option<u64> {
        while let Some(tid) = self.runnable.pop_front() {
            if self
                .tasks
                .get(&tid)
                .is_some_and(|task| task.status == TaskStatus::Runnable)
            {
                return Some(tid);
            }
        }
        None
    }

    fn pop_runnable_except(&mut self, excluded: u64) -> Option<u64> {
        let count = self.runnable.len();
        for _ in 0..count {
            let tid = self.runnable.pop_front()?;
            if tid != excluded
                && self
                    .tasks
                    .get(&tid)
                    .is_some_and(|task| task.status == TaskStatus::Runnable)
            {
                return Some(tid);
            }
            self.runnable.push_back(tid);
        }
        None
    }

    fn activate(&mut self, tid: u64, emu: &mut Emulator) -> Result<HleResult> {
        let Some(task) = self.tasks.get_mut(&tid) else {
            return Ok(HleResult::Deadlock);
        };
        task.status = TaskStatus::Running;
        let context = task.context.clone();
        self.current_tid = Some(tid);
        context.restore(tid, emu)?;
        emu.pc_override = Some(context.registers.pc);
        Ok(HleResult::Schedule)
    }

    fn schedule_or_deadlock(&mut self, emu: &mut Emulator) -> Result<HleResult> {
        if let Some(tid) = self.pop_runnable() {
            return self.activate(tid, emu);
        }

        // Virtual time advances only when no task is runnable. Expire the
        // oldest timed wait, then resume it with ETIMEDOUT.
        let timed_tid = self.tasks.iter().find_map(|(&tid, task)| {
            matches!(task.status, TaskStatus::FutexWait { timed: true, .. }).then_some(tid)
        });
        if let Some(tid) = timed_tid {
            self.make_runnable(tid, (-ETIMEDOUT) as u64, emu);
            if let Some(next_tid) = self.pop_runnable() {
                return self.activate(next_tid, emu);
            }
        }

        if self
            .tasks
            .values()
            .all(|task| matches!(task.status, TaskStatus::Exited(_)))
        {
            let code = self
                .root_tid
                .and_then(|tid| self.tasks.get(&tid))
                .and_then(|task| match task.status {
                    TaskStatus::Exited(code) => Some(code),
                    _ => None,
                })
                .or_else(|| {
                    self.tasks
                        .values()
                        .rev()
                        .find_map(|task| match task.status {
                            TaskStatus::Exited(code) => Some(code),
                            _ => None,
                        })
                })
                .unwrap_or(0);
            return Ok(HleResult::Halt(code));
        }

        emu.metrics.exit_reason = Some("guest_deadlock".into());
        Ok(HleResult::Deadlock)
    }

    fn wake_futex(&mut self, address: u64, maximum: usize, result: u64, emu: &Emulator) -> u64 {
        if maximum == 0 {
            return 0;
        }
        let Some(mut waiters) = self.futex_waiters.remove(&address) else {
            return 0;
        };
        let mut remaining = VecDeque::new();
        let mut woken = 0usize;
        while let Some(tid) = waiters.pop_front() {
            let is_waiting = self.tasks.get(&tid).is_some_and(|task| {
                matches!(task.status, TaskStatus::FutexWait { address: waiting, .. } if waiting == address)
            });
            if is_waiting && woken < maximum {
                self.make_runnable(tid, result, emu);
                woken += 1;
            } else if is_waiting {
                remaining.push_back(tid);
            }
        }
        if !remaining.is_empty() {
            self.futex_waiters.insert(address, remaining);
        }
        woken as u64
    }

    fn make_runnable(&mut self, tid: u64, result: u64, emu: &Emulator) {
        let Some(task) = self.tasks.get_mut(&tid) else {
            return;
        };
        if !matches!(task.status, TaskStatus::FutexWait { .. }) {
            return;
        }
        task.context.set_syscall_result(emu, result);
        task.status = TaskStatus::Runnable;
        self.runnable.push_back(tid);
    }
}

#[derive(Clone, Copy)]
struct CloneLayout {
    child_tid_arg: usize,
    tls_arg: usize,
    aarch64_tls: bool,
}

impl CloneLayout {
    fn for_arch(emu: &Emulator) -> Option<Self> {
        if emu.arch.name.starts_with("x86:LE:64") {
            Some(Self {
                child_tid_arg: 3,
                tls_arg: 4,
                aarch64_tls: false,
            })
        } else if emu.arch.name.starts_with("AARCH64") {
            Some(Self {
                child_tid_arg: 4,
                tls_arg: 3,
                aarch64_tls: true,
            })
        } else {
            None
        }
    }

    fn can_set_tls(self, emu: &Emulator) -> bool {
        let name = if self.aarch64_tls {
            "TPIDR_EL0"
        } else {
            "FS_OFFSET"
        };
        emu.register_map
            .keys()
            .any(|known| known.eq_ignore_ascii_case(name))
    }

    fn set_tls(self, context: &mut ThreadContext, emu: &Emulator, value: u64) -> bool {
        let name = if self.aarch64_tls {
            "TPIDR_EL0"
        } else {
            context.fs_base = value;
            "FS_OFFSET"
        };
        set_context_register(emu, &mut context.registers, name, value)
    }
}

fn set_context_register(
    emu: &Emulator,
    registers: &mut RegisterState,
    name: &str,
    value: u64,
) -> bool {
    if emu
        .register_map
        .keys()
        .any(|known| known.eq_ignore_ascii_case(name))
    {
        registers.set(name, value);
        true
    } else {
        false
    }
}

fn set_errno(emu: &mut Emulator, errno: i64) -> Result<HleResult> {
    emu.set_syscall_return((-errno) as u64)?;
    Ok(HleResult::Continue)
}

fn write_tid(emu: &mut Emulator, address: u64, tid: u64) -> Result<()> {
    let ram = emu.state.ram_space();
    let tid = tid as u32;
    let bytes = match emu.arch.endian {
        Endianness::Little => tid.to_le_bytes(),
        Endianness::Big => tid.to_be_bytes(),
    };
    emu.state.write_space(ram, address, &bytes)
}

fn read_guest_u32(emu: &Emulator, bytes: [u8; 4]) -> u32 {
    match emu.arch.endian {
        Endianness::Little => u32::from_le_bytes(bytes),
        Endianness::Big => u32::from_be_bytes(bytes),
    }
}

fn read_guest_i32(emu: &Emulator, bytes: [u8; 4]) -> i64 {
    let value = match emu.arch.endian {
        Endianness::Little => i32::from_le_bytes(bytes),
        Endianness::Big => i32::from_be_bytes(bytes),
    };
    i64::from(value)
}

fn read_guest_i64(emu: &Emulator, bytes: [u8; 8]) -> i64 {
    match emu.arch.endian {
        Endianness::Little => i64::from_le_bytes(bytes),
        Endianness::Big => i64::from_be_bytes(bytes),
    }
}

fn timeout_is_zero_or_valid(emu: &mut Emulator, address: u64) -> Result<Option<bool>, i64> {
    if address == 0 {
        return Ok(None);
    }
    let word = usize::from(emu.arch.pointer_size);
    if !(word == 4 || word == 8) {
        return Err(EINVAL);
    }
    let ram = emu.state.ram_space();
    let bytes = emu
        .state
        .read_space(ram, address, word * 2)
        .map_err(|_| EFAULT)?;
    let (seconds, nanos) = if word == 8 {
        (
            read_guest_i64(emu, bytes[..8].try_into().unwrap_or([0; 8])),
            read_guest_i64(emu, bytes[8..16].try_into().unwrap_or([0; 8])),
        )
    } else {
        (
            read_guest_i32(emu, bytes[..4].try_into().unwrap_or([0; 4])),
            read_guest_i32(emu, bytes[4..8].try_into().unwrap_or([0; 4])),
        )
    };
    if seconds < 0 || !(0..1_000_000_000).contains(&nanos) {
        return Err(EINVAL);
    }
    Ok(Some(seconds == 0 && nanos == 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arch::ArchInfo;
    use crate::os::env::OsEnvironment;
    use crate::os::linux::LinuxEnv;
    use crate::pcode::state::MachineState;
    use fission_loader::loader::LoadedBinary;
    use fission_sleigh::runtime::RuntimeSleighFrontend;
    use std::path::PathBuf;

    fn emulator() -> Emulator {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/linux_x64_hello_sys.elf");
        let binary = LoadedBinary::from_file(path).expect("load x86-64 ELF fixture");
        let mut state = MachineState::new();
        let image =
            crate::os::linux::loader::load_elf(&mut state, &binary).expect("load fixture image");
        let load_spec = binary.load_spec().expect("fixture load spec").clone();
        let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)
            .expect("candidate Sleigh frontends")
            .into_iter()
            .next()
            .expect("Sleigh frontend");
        let arch = ArchInfo::from_language_id(load_spec.pair.language_id.as_str(), Some(&binary))
            .expect("fixture architecture");
        let mut emu = Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new()))
            .expect("construct emulator");
        emu.apply_linux_image(image).expect("apply Linux image");
        emu
    }

    fn dispatch_syscall(
        env: &LinuxEnv,
        emu: &mut Emulator,
        number: u64,
        args: [u64; 6],
    ) -> HleResult {
        for (name, value) in ["RDI", "RSI", "RDX", "R10", "R8", "R9"]
            .into_iter()
            .zip(args)
        {
            emu.write_register_u64(name, value)
                .expect("set syscall argument");
        }
        emu.write_register_u64("RAX", number)
            .expect("set syscall number");
        env.dispatch_hle(emu, "syscall").expect("dispatch syscall")
    }

    fn guest_bytes(emu: &mut Emulator, bytes: &[u8]) -> u64 {
        let address = emu
            .heap_alloc(bytes.len() as u64)
            .expect("allocate guest bytes");
        emu.state
            .write_space(emu.state.ram_space(), address, bytes)
            .expect("write guest bytes");
        address
    }

    fn read_syscall_result(emu: &mut Emulator) -> i64 {
        emu.read_register_u64("RAX").expect("read syscall result") as i64
    }

    const THREAD_FLAGS: u64 = CLONE_VM | CLONE_FS | CLONE_FILES | CLONE_SIGHAND | CLONE_THREAD;

    #[test]
    fn futex_wait_mismatch_and_unsupported_commands_return_errors() {
        let env = LinuxEnv::new();
        let mut emu = emulator();
        let futex = guest_bytes(&mut emu, &1u32.to_le_bytes());

        assert!(matches!(
            dispatch_syscall(&env, &mut emu, 202, [futex, FUTEX_PRIVATE_FLAG, 0, 0, 0, 0]),
            HleResult::Continue
        ));
        assert_eq!(read_syscall_result(&mut emu), -EAGAIN);

        assert!(matches!(
            dispatch_syscall(&env, &mut emu, 202, [futex, 2, 1, 0, 0, 0]),
            HleResult::Continue
        ));
        assert_eq!(read_syscall_result(&mut emu), -ENOSYS);

        assert!(matches!(
            dispatch_syscall(
                &env,
                &mut emu,
                202,
                [0x1234_5000, FUTEX_PRIVATE_FLAG | FUTEX_WAKE, 1, 0, 0, 0]
            ),
            HleResult::Continue
        ));
        assert_eq!(read_syscall_result(&mut emu), -EFAULT);
    }

    #[test]
    fn unsupported_process_creation_syscalls_return_enosys() {
        let env = LinuxEnv::new();
        let mut emu = emulator();
        for syscall in [57, 58, 59, 435] {
            assert!(matches!(
                dispatch_syscall(&env, &mut emu, syscall, [0; 6]),
                HleResult::Continue
            ));
            assert_eq!(read_syscall_result(&mut emu), -ENOSYS, "syscall {syscall}");
        }

        let stack = emu.heap_alloc(4096).expect("allocate clone stack");
        assert!(matches!(
            dispatch_syscall(
                &env,
                &mut emu,
                56,
                [THREAD_FLAGS | 17, stack + 4096, 0, 0, 0, 0]
            ),
            HleResult::Continue
        ));
        assert_eq!(read_syscall_result(&mut emu), -ENOSYS);

        assert!(matches!(
            dispatch_syscall(
                &env,
                &mut emu,
                56,
                [THREAD_FLAGS | CLONE_PARENT_SETTID, stack + 4096, 0, 0, 0, 0]
            ),
            HleResult::Continue
        ));
        assert_eq!(read_syscall_result(&mut emu), -EFAULT);
    }

    #[test]
    fn futex_wake_respects_the_limit_and_reports_each_woken_task() {
        let env = LinuxEnv::new();
        let mut emu = emulator();
        let futex = guest_bytes(&mut emu, &0u32.to_le_bytes());
        let stacks = emu.heap_alloc(8192).expect("allocate clone stacks");

        for offset in [4096, 8192] {
            assert!(matches!(
                dispatch_syscall(
                    &env,
                    &mut emu,
                    56,
                    [THREAD_FLAGS, stacks + offset, 0, 0, 0, 0]
                ),
                HleResult::Continue
            ));
        }
        for _ in 0..2 {
            assert!(matches!(
                dispatch_syscall(&env, &mut emu, 202, [futex, FUTEX_PRIVATE_FLAG, 0, 0, 0, 0]),
                HleResult::Schedule
            ));
        }

        for expected in [1, 1, 0] {
            assert!(matches!(
                dispatch_syscall(
                    &env,
                    &mut emu,
                    202,
                    [futex, FUTEX_PRIVATE_FLAG | FUTEX_WAKE, 1, 0, 0, 0]
                ),
                HleResult::Continue
            ));
            assert_eq!(read_syscall_result(&mut emu), expected);
        }
    }

    #[test]
    fn clone_keeps_parent_registers_and_tls_separate_from_child() {
        let env = LinuxEnv::new();
        let mut emu = emulator();
        let stack = emu.heap_alloc(4096).expect("allocate clone stack");
        let tls = emu.heap_alloc(64).expect("allocate task TLS");
        emu.write_register_u64("R12", 0xfeed)
            .expect("seed task register");

        assert!(matches!(
            dispatch_syscall(
                &env,
                &mut emu,
                56,
                [THREAD_FLAGS | CLONE_SETTLS, stack + 4096, 0, 0, tls, 0]
            ),
            HleResult::Continue
        ));
        let parent_tid = read_syscall_result(&mut emu);
        assert_eq!(parent_tid, 1001);
        assert_eq!(emu.fs_base, 0);

        assert!(matches!(
            dispatch_syscall(&env, &mut emu, 24, [0; 6]),
            HleResult::Schedule
        ));
        assert_eq!(emu.current_tid, parent_tid as u64);
        assert_eq!(read_syscall_result(&mut emu), 0);
        assert_eq!(emu.fs_base, tls);
        assert_eq!(emu.read_register_u64("R12").unwrap(), 0xfeed);
        assert_eq!(
            emu.read_register_u64("RSP").expect("child stack pointer"),
            stack + 4096
        );
        emu.write_register_u64("R12", 0xcafe)
            .expect("change child register");

        assert!(matches!(
            dispatch_syscall(&env, &mut emu, 24, [0; 6]),
            HleResult::Schedule
        ));
        assert_eq!(emu.current_tid, 1000);
        assert_eq!(read_syscall_result(&mut emu), 0);
        assert_eq!(emu.fs_base, 0);
        assert_eq!(emu.read_register_u64("R12").unwrap(), 0xfeed);
    }

    #[test]
    fn futex_and_tid_words_follow_guest_byte_order() {
        let mut emu = emulator();
        emu.arch.endian = Endianness::Big;
        let address = guest_bytes(&mut emu, &[0, 0, 0, 1]);
        let word = emu
            .state
            .read_space(emu.state.ram_space(), address, 4)
            .expect("read futex word");
        assert_eq!(read_guest_u32(&emu, word.try_into().unwrap()), 1);

        write_tid(&mut emu, address, 0x1234).expect("write guest TID");
        assert_eq!(
            emu.state
                .read_space(emu.state.ram_space(), address, 4)
                .expect("read guest TID"),
            [0, 0, 0x12, 0x34]
        );

        let timeout = guest_bytes(&mut emu, &[0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(timeout_is_zero_or_valid(&mut emu, timeout), Ok(Some(false)));
    }

    #[test]
    fn wait_timeout_and_signal_interruption_are_reported_to_waiter() {
        let env = LinuxEnv::new();
        let mut emu = emulator();
        let futex = guest_bytes(&mut emu, &0u32.to_le_bytes());
        let zero_timeout = guest_bytes(&mut emu, &[0; 16]);
        assert!(matches!(
            dispatch_syscall(
                &env,
                &mut emu,
                202,
                [futex, FUTEX_PRIVATE_FLAG, 0, zero_timeout, 0, 0]
            ),
            HleResult::Continue
        ));
        assert_eq!(read_syscall_result(&mut emu), -ETIMEDOUT);

        let positive_timeout = guest_bytes(
            &mut emu,
            &1i64
                .to_le_bytes()
                .into_iter()
                .chain(0i64.to_le_bytes())
                .collect::<Vec<_>>(),
        );
        assert!(matches!(
            dispatch_syscall(
                &env,
                &mut emu,
                202,
                [futex, FUTEX_PRIVATE_FLAG, 0, positive_timeout, 0, 0]
            ),
            HleResult::Schedule
        ));
        assert_eq!(read_syscall_result(&mut emu), -ETIMEDOUT);

        let stack = emu.heap_alloc(4096).expect("allocate clone stack");
        assert!(matches!(
            dispatch_syscall(&env, &mut emu, 56, [THREAD_FLAGS, stack + 4096, 0, 0, 0, 0]),
            HleResult::Continue
        ));
        assert!(matches!(
            dispatch_syscall(&env, &mut emu, 202, [futex, FUTEX_PRIVATE_FLAG, 0, 0, 0, 0]),
            HleResult::Schedule
        ));
        assert!(matches!(
            dispatch_syscall(&env, &mut emu, 200, [1000, 10, 0, 0, 0, 0]),
            HleResult::Continue
        ));
        assert!(matches!(
            dispatch_syscall(&env, &mut emu, 24, [0; 6]),
            HleResult::Schedule
        ));
        assert_eq!(read_syscall_result(&mut emu), -EINTR);
        assert_ne!(emu.signals.pending & (1 << 9), 0);
    }
}
