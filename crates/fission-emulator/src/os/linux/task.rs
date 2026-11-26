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
const FUTEX_WAIT_BITSET: u64 = 9;
const FUTEX_WAKE_BITSET: u64 = 10;
const FUTEX_PRIVATE_FLAG: u64 = 0x80;
const FUTEX_CLOCK_REALTIME: u64 = 0x100;
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct ThreadSignals {
    pending: u64,
    blocked: u64,
    return_pc: Option<u64>,
    current: Option<i32>,
    frames: Vec<crate::os::linux::signal::SignalFrame>,
}

impl ThreadSignals {
    fn capture(emu: &Emulator) -> Self {
        Self {
            pending: emu.signals.pending,
            blocked: emu.signals.blocked,
            return_pc: emu.signals.return_pc,
            current: emu.signals.current,
            frames: emu.signals.frames.clone(),
        }
    }

    fn restore(&self, emu: &mut Emulator) {
        emu.signals.pending = self.pending;
        emu.signals.blocked = self.blocked;
        emu.signals.return_pc = self.return_pc;
        emu.signals.current = self.current;
        emu.signals.frames = self.frames.clone();
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
        child_context.signals.frames.clear();

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

        if raw_op & !(FUTEX_CMD_MASK | FUTEX_PRIVATE_FLAG | FUTEX_CLOCK_REALTIME) != 0 {
            return set_errno(emu, ENOSYS);
        }
        if address == 0 || address & 3 != 0 {
            return set_errno(emu, EINVAL);
        }
        if command != FUTEX_WAIT
            && command != FUTEX_WAKE
            && command != FUTEX_WAIT_BITSET
            && command != FUTEX_WAKE_BITSET
        {
            tracing::warn!("Unsupported Linux futex command: {}", command);
            return set_errno(emu, ENOSYS);
        }
        if (command == FUTEX_WAIT_BITSET || command == FUTEX_WAKE_BITSET) && emu.syscall_arg(5) == 0
        {
            return set_errno(emu, EINVAL);
        }
        let current = match emu.state.read_space(emu.state.ram_space(), address, 4) {
            Ok(bytes) => read_guest_u32(emu, bytes.try_into().unwrap_or([0; 4])),
            Err(_) => return set_errno(emu, EFAULT),
        };

        match command {
            FUTEX_WAIT | FUTEX_WAIT_BITSET => {
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
            FUTEX_WAKE | FUTEX_WAKE_BITSET => {
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

    /// `write(1, buf, (size_t)-1)` writes what is readable from `buf` on, as
    /// Linux does, and a buffer that is not readable at all is `EFAULT`. The
    /// read of the guest buffer allocated the requested count on the host.
    #[test]
    fn write_of_an_impossible_count_writes_the_readable_part() {
        let env = LinuxEnv::new();
        let mut emu = emulator();
        // The last five bytes of a mapping, with nothing mapped after it.
        const MAP_PRIVATE_ANONYMOUS: u64 = 0x22;
        dispatch_syscall(
            &env,
            &mut emu,
            9,
            [0, 0x1000, 3, MAP_PRIVATE_ANONYMOUS, u64::MAX, 0],
        );
        let page = read_syscall_result(&mut emu) as u64;
        assert!(!emu.state.page_map.is_mapped(page + 0x1000));
        let text = page + 0x1000 - 5;
        emu.state
            .write_space(emu.state.ram_space(), text, b"hello")
            .unwrap();

        dispatch_syscall(&env, &mut emu, 1, [1, text, u64::MAX, 0, 0, 0]);
        assert_eq!(read_syscall_result(&mut emu), 5);
        assert!(emu.vfs.files[&1].content.ends_with(b"hello"));

        dispatch_syscall(&env, &mut emu, 1, [1, page + 0x1000, u64::MAX, 0, 0, 0]);
        assert_eq!(read_syscall_result(&mut emu), -14, "EFAULT");
    }

    /// A new mapping is zero for its whole length, whatever was mapped there
    /// before. Only the first megabyte was cleared, so memory a program had
    /// used, unmapped and mapped again came back holding its old contents --
    /// to a `calloc` that trusts fresh pages, somebody else's data.
    #[test]
    fn a_reused_mapping_is_zero_past_its_first_megabyte() {
        let env = LinuxEnv::new();
        let mut emu = emulator();
        const MAP_PRIVATE_ANONYMOUS: u64 = 0x22;
        const MAP_FIXED: u64 = 0x10;
        const LEN: u64 = 0x30_0000;
        dispatch_syscall(
            &env,
            &mut emu,
            9,
            [0, LEN, 3, MAP_PRIVATE_ANONYMOUS, u64::MAX, 0],
        );
        let base = read_syscall_result(&mut emu) as u64;
        let ram = emu.state.ram_space();
        let (near, far) = (base + 0x800, base + 0x28_0000);
        emu.state.write_space(ram, near, b"near").unwrap();
        emu.state.write_space(ram, far, b"far!").unwrap();
        // Positive control: the bytes are there before the remap.
        assert_eq!(emu.state.read_space(ram, far, 4).unwrap(), b"far!");

        dispatch_syscall(&env, &mut emu, 11, [base, LEN, 0, 0, 0, 0]);
        assert_eq!(read_syscall_result(&mut emu), 0);
        dispatch_syscall(
            &env,
            &mut emu,
            9,
            [base, LEN, 3, MAP_PRIVATE_ANONYMOUS | MAP_FIXED, u64::MAX, 0],
        );
        assert_eq!(read_syscall_result(&mut emu) as u64, base);
        assert_eq!(emu.state.read_space(ram, near, 4).unwrap(), [0u8; 4]);
        assert_eq!(emu.state.read_space(ram, far, 4).unwrap(), [0u8; 4]);
    }

    /// A file mapped without `PROT_WRITE` holds the file. Its bytes were
    /// stored through the guest's own write check, which a read-only mapping
    /// fails, so the mapping read as zeros.
    #[test]
    fn a_read_only_file_mapping_holds_the_file() {
        let env = LinuxEnv::new();
        let mut emu = emulator();
        let fd = emu.vfs.open("lib.so", b"\x7fELF and the rest".to_vec());
        const MAP_PRIVATE: u64 = 2;
        const PROT_READ: u64 = 1;
        dispatch_syscall(
            &env,
            &mut emu,
            9,
            [0, 0x2000, PROT_READ, MAP_PRIVATE, fd, 4],
        );
        let base = read_syscall_result(&mut emu) as u64;
        assert!(base > 0x1000, "mapped: 0x{base:X}");
        let ram = emu.state.ram_space();
        assert_eq!(
            emu.state.read_space(ram, base, 12).unwrap(),
            b" and the res"
        );
        // Past the end of the file the mapping is zero, and it stays
        // read-only to the guest.
        assert_eq!(
            emu.state.read_space(ram, base + 0x1000, 8).unwrap(),
            [0u8; 8]
        );
        assert!(emu.state.write_space(ram, base, b"x").is_err());
    }

    /// A terabyte of address space is one mapping, as it is to a kernel --
    /// not a page-table entry (and a loop iteration) per page, which made
    /// each of these calls a way to exhaust the host.
    #[test]
    fn a_huge_reservation_costs_one_mapping() {
        let env = LinuxEnv::new();
        let mut emu = emulator();
        const MAP_PRIVATE_ANONYMOUS_NORESERVE: u64 = 0x4022;
        const LEN: u64 = 1 << 40;
        let before = emu.state.page_map.mappings().len();
        dispatch_syscall(
            &env,
            &mut emu,
            9,
            [0, LEN, 0, MAP_PRIVATE_ANONYMOUS_NORESERVE, u64::MAX, 0],
        );
        let base = read_syscall_result(&mut emu) as u64;
        assert!(base > 0x1000 && base < 1 << 47, "mapped: 0x{base:X}");
        assert_eq!(emu.state.page_map.mappings().len(), before + 1);
        assert!(emu.state.page_map.is_mapped(base + LEN - 1));

        // mprotect the middle, write there, then unmap all of it -- with a
        // length that runs to the end of the address space.
        dispatch_syscall(&env, &mut emu, 10, [base + (LEN >> 1), 0x2000, 3, 0, 0, 0]);
        assert_eq!(read_syscall_result(&mut emu), 0);
        let ram = emu.state.ram_space();
        emu.state
            .write_space(ram, base + (LEN >> 1), b"data")
            .expect("the re-protected page takes a write");
        assert!(emu.state.write_space(ram, base, b"data").is_err());
        dispatch_syscall(&env, &mut emu, 11, [base, u64::MAX - base, 0, 0, 0, 0]);
        assert_eq!(read_syscall_result(&mut emu), 0);
        assert!(!emu.state.page_map.is_mapped(base + (LEN >> 1)));
    }

    /// `mmap` of zero bytes is `EINVAL`, and of more than the address space
    /// holds `ENOMEM`. The first was taken as one byte and mapped.
    #[test]
    fn mmap_rejects_an_empty_or_impossible_length() {
        let env = LinuxEnv::new();
        let mut emu = emulator();
        const MAP_PRIVATE_ANONYMOUS: u64 = 0x22;
        dispatch_syscall(
            &env,
            &mut emu,
            9,
            [0, 0, 3, MAP_PRIVATE_ANONYMOUS, u64::MAX, 0],
        );
        assert_eq!(read_syscall_result(&mut emu), -EINVAL);
        dispatch_syscall(
            &env,
            &mut emu,
            9,
            [0, u64::MAX - 0x100, 3, MAP_PRIVATE_ANONYMOUS, u64::MAX, 0],
        );
        assert_eq!(read_syscall_result(&mut emu), -12, "ENOMEM");
        dispatch_syscall(
            &env,
            &mut emu,
            9,
            [0, 0x2000, 3, MAP_PRIVATE_ANONYMOUS, u64::MAX, 0],
        );
        assert!(
            read_syscall_result(&mut emu) > 0,
            "an ordinary mapping still works"
        );
    }

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
            dispatch_syscall(
                &env,
                &mut emu,
                202,
                [
                    futex,
                    FUTEX_PRIVATE_FLAG | FUTEX_WAIT_BITSET,
                    0,
                    0,
                    0,
                    0xffff_ffff
                ]
            ),
            HleResult::Continue
        ));
        assert_eq!(read_syscall_result(&mut emu), -EAGAIN);

        assert!(matches!(
            dispatch_syscall(
                &env,
                &mut emu,
                202,
                [futex, FUTEX_PRIVATE_FLAG | FUTEX_WAIT_BITSET, 1, 0, 0, 0]
            ),
            HleResult::Continue
        ));
        assert_eq!(read_syscall_result(&mut emu), -EINVAL);

        assert!(matches!(
            dispatch_syscall(
                &env,
                &mut emu,
                202,
                [
                    futex,
                    FUTEX_PRIVATE_FLAG | FUTEX_CLOCK_REALTIME | FUTEX_WAIT_BITSET,
                    0,
                    0,
                    0,
                    0xffff_ffff
                ]
            ),
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
    fn ioctl_tcgets_and_tiocgwinsz_return_enotty() {
        let env = LinuxEnv::new();
        let mut emu = emulator();
        const TCGETS: u64 = 0x5401;
        const TIOCGWINSZ: u64 = 0x5413;

        assert!(matches!(
            dispatch_syscall(&env, &mut emu, 16, [1, TCGETS, 0x1000, 0, 0, 0]),
            HleResult::Continue
        ));
        assert_eq!(read_syscall_result(&mut emu), -25);

        assert!(matches!(
            dispatch_syscall(&env, &mut emu, 16, [1, TIOCGWINSZ, 0x1000, 0, 0, 0]),
            HleResult::Continue
        ));
        assert_eq!(read_syscall_result(&mut emu), -25);
    }

    #[test]
    fn prlimit64_populates_old_limit() {
        let env = LinuxEnv::new();
        let mut emu = emulator();
        let old_limit = emu.heap_alloc(16).expect("allocate rlimit");

        assert!(matches!(
            dispatch_syscall(&env, &mut emu, 302, [0, 3, 0, old_limit, 0, 0]),
            HleResult::Continue
        ));
        assert_eq!(read_syscall_result(&mut emu), 0);

        let bytes = emu
            .state
            .read_space(emu.state.ram_space(), old_limit, 16)
            .expect("read rlimit bytes");
        let cur = u64::from_le_bytes(bytes[..8].try_into().unwrap());
        let max = u64::from_le_bytes(bytes[8..].try_into().unwrap());
        assert_eq!(cur, 8 * 1024 * 1024);
        assert_eq!(max, u64::MAX);

        assert!(matches!(
            dispatch_syscall(&env, &mut emu, 302, [0, 20, 0, old_limit, 0, 0]),
            HleResult::Continue
        ));
        assert_eq!(read_syscall_result(&mut emu), -22);
    }

    #[test]
    fn ioctl_terminal_group_and_size_setters_return_enotty() {
        let env = LinuxEnv::new();
        let mut emu = emulator();
        for command in [0x540f, 0x5410, 0x5414] {
            dispatch_syscall(&env, &mut emu, 16, [1, command, 0x1000, 0, 0, 0]);
            assert_eq!(read_syscall_result(&mut emu), -25, "command={command:#x}");
        }
    }

    #[test]
    fn prlimit64_negative_pid_is_not_a_self_alias() {
        let env = LinuxEnv::new();
        let mut emu = emulator();
        let sentinel = [0xa5; 16];
        let old_limit = guest_bytes(&mut emu, &sentinel);
        dispatch_syscall(&env, &mut emu, 302, [u64::MAX, 3, 0, old_limit, 0, 0]);
        assert_eq!(read_syscall_result(&mut emu), -3);
        assert_eq!(
            emu.state
                .read_space(emu.state.ram_space(), old_limit, 16)
                .unwrap(),
            sentinel
        );
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

    #[test]
    fn read_handles_bad_fd_zero_count_and_eof_without_faulting() {
        let env = LinuxEnv::new();
        let mut emu = emulator();
        let ram = emu.state.ram_space();

        // Invalid descriptor fails with -EBADF before checking buffer/count.
        dispatch_syscall(&env, &mut emu, 0, [99, 0, 10, 0, 0, 0]);
        assert_eq!(read_syscall_result(&mut emu), -9);
        dispatch_syscall(&env, &mut emu, 0, [99, 0, 0, 0, 0, 0]);
        assert_eq!(read_syscall_result(&mut emu), -9);

        let ro_page = 0x5000_0000u64;
        emu.state
            .page_map
            .map_region(ro_page, 0x1000, crate::pcode::page_map::prot::READ, true);
        let fd = emu.vfs.open("data.bin", b"abcdef".to_vec());

        // count == 0 succeeds without faulting or moving cursor/eof.
        dispatch_syscall(&env, &mut emu, 0, [fd, 0, 0, 0, 0, 0]);
        assert_eq!(read_syscall_result(&mut emu), 0);
        dispatch_syscall(&env, &mut emu, 0, [fd, ro_page, 0, 0, 0, 0]);
        assert_eq!(read_syscall_result(&mut emu), 0);
        assert_eq!(emu.vfs.files[&fd].cursor, 0);
        assert!(!emu.vfs.files[&fd].eof);

        // Consume all bytes, then verify EOF with NULL, RO, and RW buffers.
        let rw_buf = emu.heap_alloc(16).expect("rw buffer");
        dispatch_syscall(&env, &mut emu, 0, [fd, rw_buf, 6, 0, 0, 0]);
        assert_eq!(read_syscall_result(&mut emu), 6);
        assert_eq!(emu.state.read_space(ram, rw_buf, 6).unwrap(), b"abcdef");

        for dst in [0, ro_page, rw_buf] {
            dispatch_syscall(&env, &mut emu, 0, [fd, dst, 10, 0, 0, 0]);
            assert_eq!(read_syscall_result(&mut emu), 0);
            assert!(emu.vfs.files[&fd].eof);
        }
    }

    #[test]
    fn read_first_page_fault_returns_efault_and_preserves_cursor_and_memory() {
        let env = LinuxEnv::new();
        let mut emu = emulator();
        let ram = emu.state.ram_space();
        let ro_page = 0x5000_0000u64;
        let none_page = 0x5000_2000u64;
        emu.state
            .page_map
            .map_region(ro_page, 0x1000, crate::pcode::page_map::prot::READ, true);
        emu.state
            .page_map
            .map_region(none_page, 0x1000, crate::pcode::page_map::prot::NONE, true);
        emu.state.write_ram_unprotected(ro_page, b"KEEPME").unwrap();

        let fd = emu.vfs.open("secret.bin", b"0123456789".to_vec());
        for bad_buf in [0, ro_page, none_page] {
            dispatch_syscall(&env, &mut emu, 0, [fd, bad_buf, 10, 0, 0, 0]);
            assert_eq!(read_syscall_result(&mut emu), -EFAULT);
            assert_eq!(emu.vfs.files[&fd].cursor, 0);
            assert!(!emu.vfs.files[&fd].eof);
        }

        // Unreadable/read-only target bytes stay untouched; force unprotected read.
        emu.state.enforce_page_faults = false;
        assert_eq!(emu.state.read_space(ram, ro_page, 6).unwrap(), b"KEEPME");
        emu.state.enforce_page_faults = true;

        // Repeat read into a valid writable buffer receives all bytes from offset 0.
        let rw_buf = emu.heap_alloc(16).expect("rw buffer");
        dispatch_syscall(&env, &mut emu, 0, [fd, rw_buf, 10, 0, 0, 0]);
        assert_eq!(read_syscall_result(&mut emu), 10);
        assert_eq!(
            emu.state.read_space(ram, rw_buf, 10).unwrap(),
            b"0123456789"
        );
    }

    #[test]
    fn read_partial_page_crossing_commits_only_writable_prefix_and_resumes() {
        let env = LinuxEnv::new();
        let mut emu = emulator();
        let ram = emu.state.ram_space();

        // Scenario 1: RW page followed by unmapped page.
        let rw_base = 0x5100_0000u64;
        emu.state
            .page_map
            .map_region(rw_base, 0x1000, crate::pcode::page_map::prot::RW, true);
        let partial = rw_base + 0x1000 - 4;
        emu.state.write_space(ram, partial - 1, &[0xAA]).unwrap();

        let fd = emu.vfs.open("cross1.bin", b"0123456789".to_vec());
        dispatch_syscall(&env, &mut emu, 0, [fd, partial, 10, 0, 0, 0]);
        assert_eq!(read_syscall_result(&mut emu), 4);
        assert_eq!(emu.vfs.files[&fd].cursor, 4);
        assert!(!emu.vfs.files[&fd].eof);
        assert_eq!(emu.state.read_space(ram, partial, 4).unwrap(), b"0123");
        assert_eq!(emu.state.read_space(ram, partial - 1, 1).unwrap(), [0xAA]);

        let tail = emu.heap_alloc(16).expect("tail buffer");
        dispatch_syscall(&env, &mut emu, 0, [fd, tail, 10, 0, 0, 0]);
        assert_eq!(read_syscall_result(&mut emu), 6);
        assert_eq!(emu.vfs.files[&fd].cursor, 10);
        assert!(emu.vfs.files[&fd].eof);
        assert_eq!(emu.state.read_space(ram, tail, 6).unwrap(), b"456789");

        // Scenario 2: RW page followed by PROT_READ page.
        let ro_pair = 0x5200_0000u64;
        emu.state
            .page_map
            .map_region(ro_pair, 0x1000, crate::pcode::page_map::prot::RW, true);
        emu.state.page_map.map_region(
            ro_pair + 0x1000,
            0x1000,
            crate::pcode::page_map::prot::READ,
            true,
        );
        emu.state
            .write_ram_unprotected(ro_pair + 0x1000, b"UNCHANGED")
            .unwrap();

        let fd2 = emu.vfs.open("cross2.bin", b"ABCDEFGHIJ".to_vec());
        dispatch_syscall(&env, &mut emu, 0, [fd2, ro_pair + 0x1000 - 4, 10, 0, 0, 0]);
        assert_eq!(read_syscall_result(&mut emu), 4);
        assert_eq!(emu.vfs.files[&fd2].cursor, 4);
        assert_eq!(
            emu.state.read_space(ram, ro_pair + 0x1000 - 4, 4).unwrap(),
            b"ABCD"
        );
        assert_eq!(
            emu.state.read_space(ram, ro_pair + 0x1000, 9).unwrap(),
            b"UNCHANGED"
        );
    }

    #[test]
    fn read_huge_count_and_multi_chunk_transfer_stay_bounded() {
        let env = LinuxEnv::new();
        let mut emu = emulator();
        let ram = emu.state.ram_space();

        let total = (1usize << 20) + 123;
        let payload: Vec<u8> = (0..total).map(|i| (i % 251) as u8).collect();
        let fd = emu.vfs.open("big.bin", payload.clone());

        let base = 0x6000_0000u64;
        let map_len = 0x20_1000u64;
        emu.state
            .page_map
            .map_region(base, map_len, crate::pcode::page_map::prot::RW, true);

        // Full multi-chunk transfer with (size_t)-1 does not panic or OOM.
        dispatch_syscall(&env, &mut emu, 0, [fd, base, u64::MAX, 0, 0, 0]);
        assert_eq!(read_syscall_result(&mut emu), total as i64);
        assert_eq!(emu.vfs.files[&fd].cursor, total);
        assert_eq!(emu.state.read_space(ram, base, total).unwrap(), payload);

        // Partial crossing in the second IO_CHUNK stops cleanly at the unmapped end.
        emu.vfs.seek(fd, 0).unwrap();
        let writable_len = (1usize << 20) + 100;
        let near_top = base + map_len - writable_len as u64;
        dispatch_syscall(&env, &mut emu, 0, [fd, near_top, u64::MAX, 0, 0, 0]);
        assert_eq!(read_syscall_result(&mut emu), writable_len as i64);
        assert_eq!(emu.vfs.files[&fd].cursor, writable_len);
        assert!(!emu.vfs.files[&fd].eof);
        assert_eq!(
            emu.state.read_space(ram, near_top, writable_len).unwrap(),
            payload[..writable_len]
        );

        let rest = emu.heap_alloc(64).expect("rest buffer");
        dispatch_syscall(&env, &mut emu, 0, [fd, rest, 64, 0, 0, 0]);
        assert_eq!(read_syscall_result(&mut emu), 23);
        assert_eq!(
            emu.state.read_space(ram, rest, 23).unwrap(),
            payload[writable_len..]
        );
    }

    #[test]
    fn read_stdin_updates_symbolic_and_taint_only_for_transferred_bytes_and_caps_aggregates() {
        let env = LinuxEnv::new();

        // Symbolic mode: fault preserves stdin and cursor; partial crossing marks only written bytes.
        let mut sym_emu = emulator();
        sym_emu.set_shadow_mode(crate::observe::ShadowMode::Symbolic);
        sym_emu.seed_stdin(b"ABCDEF");
        let page = 0x5300_0000u64;
        sym_emu
            .state
            .page_map
            .map_region(page, 0x1000, crate::pcode::page_map::prot::RW, true);

        dispatch_syscall(&env, &mut sym_emu, 0, [0, 0, 4, 0, 0, 0]);
        assert_eq!(read_syscall_result(&mut sym_emu), -EFAULT);
        assert_eq!(sym_emu.stdin_buffer.as_deref(), Some(b"ABCDEF".as_slice()));
        assert_eq!(sym_emu.vfs.files[&0].cursor, 0);
        assert_eq!(sym_emu.symbolic_stdin_bytes, 0);

        let partial = page + 0x1000 - 2;
        dispatch_syscall(&env, &mut sym_emu, 0, [0, partial, 6, 0, 0, 0]);
        assert_eq!(read_syscall_result(&mut sym_emu), 2);
        assert_eq!(sym_emu.stdin_buffer.as_deref(), Some(b"CDEF".as_slice()));
        assert_eq!(sym_emu.vfs.files[&0].cursor, 2);
        assert_eq!(sym_emu.symbolic_stdin_bytes, 2);
        let ram = sym_emu.state.ram_space();
        assert!(sym_emu.state.get_shadow_memory(ram, partial - 1).is_none());
        assert!(sym_emu.state.get_shadow_memory(ram, partial).is_some());
        assert!(sym_emu.state.get_shadow_memory(ram, partial + 1).is_some());

        // Aggregate symbolic stdin limit bounds large reads.
        let sym_cap_buf = 0x5400_0000u64;
        let big_len = crate::core::MAX_SYMBOLIC_STDIN_BYTES + 1024;
        sym_emu.state.page_map.map_region(
            sym_cap_buf,
            big_len as u64 + 0x1000,
            crate::pcode::page_map::prot::RW,
            true,
        );
        sym_emu.seed_stdin(&vec![b'x'; big_len]);
        dispatch_syscall(
            &env,
            &mut sym_emu,
            0,
            [0, sym_cap_buf, big_len as u64, 0, 0, 0],
        );
        assert_eq!(read_syscall_result(&mut sym_emu), big_len as i64);
        assert_eq!(
            sym_emu.symbolic_stdin_bytes,
            crate::core::MAX_SYMBOLIC_STDIN_BYTES
        );

        // Taint mode: fault records nothing; partial crossing marks only written bytes and creates no solver vars.
        let mut taint_emu = emulator();
        taint_emu.set_shadow_mode(crate::observe::ShadowMode::Taint);
        taint_emu.seed_stdin(b"XYZ123");
        let ro_page = 0x5500_0000u64;
        taint_emu.state.page_map.map_region(
            ro_page,
            0x1000,
            crate::pcode::page_map::prot::READ,
            true,
        );
        taint_emu
            .state
            .page_map
            .map_region(page, 0x1000, crate::pcode::page_map::prot::RW, true);

        dispatch_syscall(&env, &mut taint_emu, 0, [0, ro_page, 6, 0, 0, 0]);
        assert_eq!(read_syscall_result(&mut taint_emu), -EFAULT);
        assert!(taint_emu.taint.is_empty());
        assert_eq!(taint_emu.taint_marked_bytes, 0);

        let partial3 = page + 0x1000 - 3;
        dispatch_syscall(&env, &mut taint_emu, 0, [0, partial3, 6, 0, 0, 0]);
        assert_eq!(read_syscall_result(&mut taint_emu), 3);
        assert_eq!(taint_emu.taint.sources().len(), 1);
        assert_eq!(taint_emu.taint.sources()[0].len, 3);
        assert_eq!(taint_emu.taint_marked_bytes, 3);
        assert!(taint_emu.solver.nodes().is_empty());
        let tram = taint_emu.state.ram_space();
        assert!(
            taint_emu
                .state
                .get_shadow_memory(tram, partial3 - 1)
                .is_none()
        );
        assert!(taint_emu.state.get_shadow_memory(tram, partial3).is_some());

        // Aggregate taint cap bounds repeated/large reads.
        let buf5 = 0x5600_0000u64;
        taint_emu
            .state
            .page_map
            .map_region(buf5, 1 << 20, crate::pcode::page_map::prot::RW, true);
        let fd_big = taint_emu.vfs.open("taint_big", vec![b'A'; 5 << 20]);
        for _ in 0..5 {
            dispatch_syscall(&env, &mut taint_emu, 0, [fd_big, buf5, 1 << 20, 0, 0, 0]);
            assert_eq!(read_syscall_result(&mut taint_emu), 1 << 20);
        }
        assert_eq!(
            taint_emu.taint_marked_bytes,
            crate::core::MAX_TAINT_MARKED_BYTES
        );
    }

    #[test]
    fn read_respects_32bit_guest_address_space_top_without_wrapping() {
        let env = LinuxEnv::new();
        let mut emu = emulator();
        emu.state.page_map.set_64bit(false);
        emu.state
            .page_map
            .map_region(0xFFFF_F000, 0x1000, crate::pcode::page_map::prot::RW, true);
        let fd = emu.vfs.open("top32.bin", b"0123456789ABCDEF".to_vec());

        // Starting 8 bytes below 4 GiB transfers only 8 bytes, never wrapping to 0.
        dispatch_syscall(&env, &mut emu, 0, [fd, 0xFFFF_FFF8, 32, 0, 0, 0]);
        assert_eq!(read_syscall_result(&mut emu), 8);
        assert_eq!(emu.vfs.files[&fd].cursor, 8);
        assert_eq!(
            emu.state
                .read_space(emu.state.ram_space(), 0xFFFF_FFF8, 8)
                .unwrap(),
            b"01234567"
        );

        // An address above 2^32 is rejected with -EFAULT without advancing cursor.
        dispatch_syscall(&env, &mut emu, 0, [fd, 0x1_0000_0000, 8, 0, 0, 0]);
        assert_eq!(read_syscall_result(&mut emu), -EFAULT);
        assert_eq!(emu.vfs.files[&fd].cursor, 8);
    }

    // ── Host stdin read errors (#1319 review) ───────────────────────────────
    //
    // A real terminal cannot be made to return `EINTR` or `EIO` on demand, so
    // these drive `read(0, ...)` through an injected host reader that follows
    // a script. Each failing case checks the whole guest-visible state: the
    // return value, the destination bytes, the shadow memory over them, the
    // symbolic-stdin budget and the stdin mock.

    const EIO: i64 = 5;

    #[derive(Clone)]
    enum HostStep {
        Data(Vec<u8>),
        Fail(std::io::ErrorKind),
        /// Fails with this kind on every call from here on.
        FailForever(std::io::ErrorKind),
        /// Claims one byte more than the buffer it was handed.
        Overrun,
    }

    struct ScriptedStdin {
        steps: VecDeque<HostStep>,
        requested: std::sync::Arc<std::sync::Mutex<Vec<usize>>>,
    }

    impl std::io::Read for ScriptedStdin {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.requested.lock().unwrap().push(buf.len());
            let step = match self.steps.front() {
                Some(HostStep::FailForever(kind)) => {
                    return Err(std::io::Error::new(*kind, "scripted host stdin failure"));
                }
                _ => self.steps.pop_front(),
            };
            match step {
                None => Ok(0),
                Some(HostStep::Data(data)) => {
                    let n = data.len().min(buf.len());
                    buf[..n].copy_from_slice(&data[..n]);
                    Ok(n)
                }
                Some(HostStep::Fail(kind)) | Some(HostStep::FailForever(kind)) => {
                    Err(std::io::Error::new(kind, "scripted host stdin failure"))
                }
                Some(HostStep::Overrun) => Ok(buf.len() + 1),
            }
        }
    }

    /// An emulator whose fd 0 has no mock and falls through to a scripted
    /// host stdin; returns the log of buffer sizes the host was asked for.
    fn host_stdin_emulator(
        steps: Vec<HostStep>,
    ) -> (Emulator, std::sync::Arc<std::sync::Mutex<Vec<usize>>>) {
        let mut emu = emulator();
        emu.host_stdin = true;
        let requested = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        emu.set_host_stdin_reader(Box::new(ScriptedStdin {
            steps: steps.into(),
            requested: requested.clone(),
        }));
        (emu, requested)
    }

    const SENTINEL: &[u8; 16] = b"SENTINEL-SENTINE";

    /// Nothing the guest can observe moved: the buffer still holds the
    /// sentinel, no shadow node covers it, no symbolic stdin byte was spent,
    /// and fd 0 / the mock are where they were.
    fn assert_read_left_no_trace(emu: &Emulator, buf: u64) {
        let ram = emu.state.ram_space();
        assert_eq!(
            emu.state
                .read_space_readonly(ram, buf, SENTINEL.len())
                .unwrap(),
            SENTINEL,
            "destination bytes changed"
        );
        for i in 0..SENTINEL.len() as u64 {
            assert_eq!(emu.state.get_shadow_memory(ram, buf + i), None);
        }
        assert_eq!(emu.symbolic_stdin_bytes, 0);
        assert!(emu.taint.is_empty());
        assert!(emu.stdin_buffer.is_none());
        assert_eq!(emu.vfs.files[&0].cursor, 0);
        assert!(!emu.vfs.files[&0].eof);
    }

    #[test]
    fn host_stdin_hard_error_is_eio_not_eof_and_leaves_no_trace() {
        for kind in [
            std::io::ErrorKind::Other,
            std::io::ErrorKind::BrokenPipe,
            std::io::ErrorKind::PermissionDenied,
            std::io::ErrorKind::InvalidData,
        ] {
            let env = LinuxEnv::new();
            let (mut emu, requested) = host_stdin_emulator(vec![HostStep::Fail(kind)]);
            let buf = guest_bytes(&mut emu, SENTINEL);
            dispatch_syscall(&env, &mut emu, 0, [0, buf, 16, 0, 0, 0]);
            assert_eq!(
                read_syscall_result(&mut emu),
                -EIO,
                "{kind:?} must not read as end-of-file"
            );
            assert_eq!(
                requested.lock().unwrap().len(),
                1,
                "hard errors are not retried"
            );
            assert_read_left_no_trace(&emu, buf);
        }
    }

    #[test]
    fn host_stdin_would_block_is_eagain_and_leaves_no_trace() {
        let env = LinuxEnv::new();
        let (mut emu, _) =
            host_stdin_emulator(vec![HostStep::Fail(std::io::ErrorKind::WouldBlock)]);
        let buf = guest_bytes(&mut emu, SENTINEL);
        dispatch_syscall(&env, &mut emu, 0, [0, buf, 16, 0, 0, 0]);
        assert_eq!(read_syscall_result(&mut emu), -EAGAIN);
        assert_read_left_no_trace(&emu, buf);
    }

    #[test]
    fn host_stdin_interrupted_read_is_retried_and_then_transfers() {
        let env = LinuxEnv::new();
        let (mut emu, requested) = host_stdin_emulator(vec![
            HostStep::Fail(std::io::ErrorKind::Interrupted),
            HostStep::Fail(std::io::ErrorKind::Interrupted),
            HostStep::Data(b"abc".to_vec()),
        ]);
        let buf = guest_bytes(&mut emu, SENTINEL);
        dispatch_syscall(&env, &mut emu, 0, [0, buf, 16, 0, 0, 0]);
        assert_eq!(read_syscall_result(&mut emu), 3);
        assert_eq!(requested.lock().unwrap().as_slice(), [16, 16, 16]);

        let ram = emu.state.ram_space();
        let after = emu.state.read_space_readonly(ram, buf, 16).unwrap();
        assert_eq!(&after[..3], b"abc");
        assert_eq!(&after[3..], &SENTINEL[3..], "bytes past the transfer moved");
        for i in 0..3 {
            assert!(emu.state.get_shadow_memory(ram, buf + i).is_some());
        }
        for i in 3..16 {
            assert_eq!(emu.state.get_shadow_memory(ram, buf + i), None);
        }
        assert_eq!(emu.symbolic_stdin_bytes, 3);
    }

    #[test]
    fn host_stdin_persistent_eintr_is_reported_after_bounded_retries() {
        let env = LinuxEnv::new();
        let (mut emu, requested) =
            host_stdin_emulator(vec![HostStep::FailForever(std::io::ErrorKind::Interrupted)]);
        let buf = guest_bytes(&mut emu, SENTINEL);
        dispatch_syscall(&env, &mut emu, 0, [0, buf, 16, 0, 0, 0]);
        assert_eq!(read_syscall_result(&mut emu), -EINTR);
        let calls = requested.lock().unwrap().len();
        assert!(
            (2..=64).contains(&calls),
            "EINTR must be retried a bounded number of times, was {calls}"
        );
        assert_read_left_no_trace(&emu, buf);
    }

    #[test]
    fn host_stdin_reader_claiming_more_than_its_buffer_is_eio() {
        let env = LinuxEnv::new();
        let (mut emu, _) = host_stdin_emulator(vec![HostStep::Overrun]);
        let buf = guest_bytes(&mut emu, SENTINEL);
        dispatch_syscall(&env, &mut emu, 0, [0, buf, 16, 0, 0, 0]);
        assert_eq!(read_syscall_result(&mut emu), -EIO);
        assert_read_left_no_trace(&emu, buf);
    }

    #[test]
    fn host_stdin_short_read_commits_only_transferred_bytes_then_eof() {
        let env = LinuxEnv::new();
        let (mut emu, _) = host_stdin_emulator(vec![HostStep::Data(b"12345".to_vec())]);
        let buf = guest_bytes(&mut emu, SENTINEL);
        let ram = emu.state.ram_space();

        dispatch_syscall(&env, &mut emu, 0, [0, buf, 16, 0, 0, 0]);
        assert_eq!(read_syscall_result(&mut emu), 5);
        let after = emu.state.read_space_readonly(ram, buf, 16).unwrap();
        assert_eq!(&after[..5], b"12345");
        assert_eq!(&after[5..], &SENTINEL[5..]);
        assert_eq!(emu.symbolic_stdin_bytes, 5);

        // The script is exhausted: the host reports a clean end-of-file.
        dispatch_syscall(&env, &mut emu, 0, [0, buf, 16, 0, 0, 0]);
        assert_eq!(read_syscall_result(&mut emu), 0);
        assert_eq!(emu.symbolic_stdin_bytes, 5);
    }

    #[test]
    fn host_stdin_is_asked_only_for_the_writable_bounded_prefix() {
        let env = LinuxEnv::new();
        let (mut emu, requested) = host_stdin_emulator(vec![
            HostStep::Data(vec![b'x'; 8]),
            HostStep::Data(vec![b'y'; 8]),
        ]);
        let ram = emu.state.ram_space();

        // A buffer four bytes before an unmapped page: the host is offered
        // four bytes, not `count`, so nothing is consumed that cannot land.
        let rw_base = 0x5100_0000u64;
        emu.state
            .page_map
            .map_region(rw_base, 0x1000, crate::pcode::page_map::prot::RW, true);
        let edge = rw_base + 0x1000 - 4;
        dispatch_syscall(&env, &mut emu, 0, [0, edge, u64::MAX, 0, 0, 0]);
        assert_eq!(read_syscall_result(&mut emu), 4);
        assert_eq!(emu.state.read_space(ram, edge, 4).unwrap(), b"xxxx");

        // A huge count over a large writable region holds at most one I/O
        // chunk on the host.
        let big = 0x5200_0000u64;
        emu.state
            .page_map
            .map_region(big, 4 << 20, crate::pcode::page_map::prot::RW, true);
        dispatch_syscall(&env, &mut emu, 0, [0, big, u64::MAX, 0, 0, 0]);
        assert_eq!(read_syscall_result(&mut emu), 8);
        assert_eq!(requested.lock().unwrap().as_slice(), [4, 1 << 20]);
    }

    #[test]
    fn host_stdin_is_not_consumed_by_zero_count_or_unwritable_buffers() {
        let env = LinuxEnv::new();
        let (mut emu, requested) = host_stdin_emulator(vec![HostStep::Data(b"kept".to_vec())]);
        let ro_page = 0x5000_0000u64;
        emu.state
            .page_map
            .map_region(ro_page, 0x1000, crate::pcode::page_map::prot::READ, true);
        let buf = guest_bytes(&mut emu, SENTINEL);

        dispatch_syscall(&env, &mut emu, 0, [0, buf, 0, 0, 0, 0]);
        assert_eq!(read_syscall_result(&mut emu), 0);
        for bad in [0, ro_page, u64::MAX - 1] {
            dispatch_syscall(&env, &mut emu, 0, [0, bad, 16, 0, 0, 0]);
            assert_eq!(read_syscall_result(&mut emu), -EFAULT, "buffer {bad:#x}");
        }
        assert!(
            requested.lock().unwrap().is_empty(),
            "host stdin was consumed"
        );
        assert_read_left_no_trace(&emu, buf);

        dispatch_syscall(&env, &mut emu, 0, [0, buf, 16, 0, 0, 0]);
        assert_eq!(read_syscall_result(&mut emu), 4);
    }

    #[test]
    fn host_stdin_under_taint_marks_only_transferred_bytes_and_nothing_on_error() {
        let env = LinuxEnv::new();
        let (mut emu, _) = host_stdin_emulator(vec![
            HostStep::Fail(std::io::ErrorKind::Other),
            HostStep::Data(b"ab".to_vec()),
        ]);
        emu.set_shadow_mode(crate::observe::ShadowMode::Taint);
        let buf = guest_bytes(&mut emu, SENTINEL);

        dispatch_syscall(&env, &mut emu, 0, [0, buf, 16, 0, 0, 0]);
        assert_eq!(read_syscall_result(&mut emu), -EIO);
        assert!(emu.taint.is_empty());

        dispatch_syscall(&env, &mut emu, 0, [0, buf, 16, 0, 0, 0]);
        assert_eq!(read_syscall_result(&mut emu), 2);
        assert_eq!(emu.taint.sources().len(), 1);
        assert_eq!(emu.taint.sources()[0].len, 2);
        assert_eq!(
            emu.symbolic_stdin_bytes, 0,
            "taint mode owns the shadow slots"
        );
    }
}
