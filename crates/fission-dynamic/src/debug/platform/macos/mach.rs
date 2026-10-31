//! The few Mach and BSD calls the macOS debugger needs, declared directly.
//!
//! `libSystem` is always linked, so declaring the handful of functions used
//! here adds no crate to the build. Every function is a thin, checked wrapper:
//! a non-zero `kern_return_t` becomes an error carrying the call's name and
//! the code, and nothing here keeps a port alive longer than its owner.

#![allow(non_camel_case_types)]

use std::time::Duration;

pub type mach_port_t = u32;
pub type kern_return_t = i32;

pub const KERN_SUCCESS: kern_return_t = 0;

pub const VM_PROT_READ: i32 = 0x1;
pub const VM_PROT_WRITE: i32 = 0x2;
/// Copy-on-write: required to make a private writable copy of a code page.
pub const VM_PROT_COPY: i32 = 0x10;

const VM_REGION_BASIC_INFO_64: i32 = 9;
const VM_REGION_BASIC_INFO_COUNT_64: u32 = 9;
const TASK_DYLD_INFO: u32 = 17;
const THREAD_IDENTIFIER_INFO: u32 = 4;

pub const PT_TRACE_ME: i32 = 0;
pub const PT_CONTINUE: i32 = 7;
pub const PT_KILL: i32 = 8;
pub const PT_STEP: i32 = 9;
pub const PT_ATTACH: i32 = 10;
pub const PT_DETACH: i32 = 11;

unsafe extern "C" {
    static mach_task_self_: mach_port_t;

    fn task_for_pid(target: mach_port_t, pid: i32, task: *mut mach_port_t) -> kern_return_t;
    fn task_threads(
        task: mach_port_t,
        list: *mut *mut mach_port_t,
        count: *mut u32,
    ) -> kern_return_t;
    fn task_info(task: mach_port_t, flavor: u32, info: *mut i32, count: *mut u32) -> kern_return_t;
    fn thread_get_state(
        thread: mach_port_t,
        flavor: i32,
        state: *mut u32,
        count: *mut u32,
    ) -> kern_return_t;
    fn thread_set_state(
        thread: mach_port_t,
        flavor: i32,
        state: *const u32,
        count: u32,
    ) -> kern_return_t;
    fn thread_info(
        thread: mach_port_t,
        flavor: u32,
        info: *mut i32,
        count: *mut u32,
    ) -> kern_return_t;
    fn mach_vm_read_overwrite(
        task: mach_port_t,
        address: u64,
        size: u64,
        data: u64,
        out_size: *mut u64,
    ) -> kern_return_t;
    fn mach_vm_write(task: mach_port_t, address: u64, data: usize, count: u32) -> kern_return_t;
    fn mach_vm_protect(
        task: mach_port_t,
        address: u64,
        size: u64,
        set_maximum: i32,
        new_protection: i32,
    ) -> kern_return_t;
    fn mach_vm_region(
        task: mach_port_t,
        address: *mut u64,
        size: *mut u64,
        flavor: i32,
        info: *mut i32,
        count: *mut u32,
        object_name: *mut mach_port_t,
    ) -> kern_return_t;
    fn mach_port_deallocate(task: mach_port_t, name: mach_port_t) -> kern_return_t;
    fn vm_deallocate(task: mach_port_t, address: usize, size: usize) -> kern_return_t;

    fn getpagesize() -> i32;
    fn ptrace(request: i32, pid: i32, addr: *mut std::ffi::c_char, data: i32) -> i32;
    fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32;
}

pub const WNOHANG: i32 = 1;
pub const WUNTRACED: i32 = 2;

fn check(call: &str, kr: kern_return_t) -> Result<(), String> {
    if kr == KERN_SUCCESS {
        Ok(())
    } else {
        Err(format!("{call} failed: kern_return_t {kr}"))
    }
}

/// How `task_for_pid` ended.
#[derive(Debug, PartialEq, Eq)]
pub enum TaskPortError {
    /// The kernel or `taskgated` refused.
    Denied(kern_return_t),
    /// The request did not come back. On a machine where Developer Tools
    /// access has not been authorized, `taskgated` waits for an interactive
    /// authorization that a headless caller can never answer.
    TimedOut,
}

impl TaskPortError {
    /// What a person has to do about it.
    pub fn remediation(&self) -> &'static str {
        match self {
            Self::Denied(_) => {
                "the process may be hardened or owned by another user; \
                 debug a binary you built without the hardened runtime, or run as root"
            }
            Self::TimedOut => {
                "macOS is waiting for Developer Tools access to be authorized. \
                 Run `sudo DevToolsSecurity -enable` once (your user must be in the \
                 `_developer` group), or run fission as root"
            }
        }
    }
}

/// The task port for `pid`, or why it could not be had.
///
/// The call runs on its own thread so that an authorization prompt nobody can
/// answer ends as [`TaskPortError::TimedOut`] instead of hanging the caller.
/// A thread that never returns is left behind; it holds nothing.
pub fn task_port_for_pid(pid: u32, timeout: Duration) -> Result<mach_port_t, TaskPortError> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut task: mach_port_t = 0;
        // SAFETY: `task` is a valid out-pointer; `mach_task_self_` is set by libSystem.
        let kr = unsafe { task_for_pid(mach_task_self_, pid as i32, &mut task) };
        let _ = sender.send((kr, task));
    });
    match receiver.recv_timeout(timeout) {
        Ok((KERN_SUCCESS, task)) => Ok(task),
        Ok((kr, _)) => Err(TaskPortError::Denied(kr)),
        Err(_) => Err(TaskPortError::TimedOut),
    }
}

/// Release a send right obtained from this module.
pub fn release_port(port: mach_port_t) {
    // SAFETY: deallocating a right this process holds; failure is not actionable.
    unsafe {
        mach_port_deallocate(mach_task_self_, port);
    }
}

/// The threads of `task`, as owned send rights.
pub fn threads(task: mach_port_t) -> Result<Vec<mach_port_t>, String> {
    let mut list: *mut mach_port_t = std::ptr::null_mut();
    let mut count: u32 = 0;
    // SAFETY: valid out-pointers; the returned array is released below.
    check("task_threads", unsafe {
        task_threads(task, &mut list, &mut count)
    })?;
    // SAFETY: the kernel returned `count` ports at `list`.
    let ports = unsafe { std::slice::from_raw_parts(list, count as usize) }.to_vec();
    // SAFETY: `list` was allocated in this task by the call above.
    unsafe {
        vm_deallocate(
            mach_task_self_,
            list as usize,
            count as usize * std::mem::size_of::<mach_port_t>(),
        );
    }
    Ok(ports)
}

/// The kernel's unique id for a thread.
pub fn thread_unique_id(thread: mach_port_t) -> Result<u64, String> {
    let mut info = [0i32; 6];
    let mut count = info.len() as u32;
    // SAFETY: `info` has room for THREAD_IDENTIFIER_INFO_COUNT (6) words.
    check("thread_info", unsafe {
        thread_info(
            thread,
            THREAD_IDENTIFIER_INFO,
            info.as_mut_ptr(),
            &mut count,
        )
    })?;
    Ok(u64::from(info[0] as u32) | (u64::from(info[1] as u32) << 32))
}

/// Read a thread's register state of `flavor` into `state`.
pub fn get_thread_state(thread: mach_port_t, flavor: i32, state: &mut [u32]) -> Result<(), String> {
    let mut count = state.len() as u32;
    // SAFETY: `state` has `count` words available.
    check("thread_get_state", unsafe {
        thread_get_state(thread, flavor, state.as_mut_ptr(), &mut count)
    })
}

/// Write a thread's register state of `flavor`.
pub fn set_thread_state(thread: mach_port_t, flavor: i32, state: &[u32]) -> Result<(), String> {
    // SAFETY: `state` holds `len` valid words.
    check("thread_set_state", unsafe {
        thread_set_state(thread, flavor, state.as_ptr(), state.len() as u32)
    })
}

/// Read `size` bytes at `address`; fails if any part is unreadable.
pub fn read_memory(task: mach_port_t, address: u64, size: usize) -> Result<Vec<u8>, String> {
    let mut buffer = vec![0u8; size];
    let mut read: u64 = 0;
    // SAFETY: `buffer` is `size` writable bytes.
    check("mach_vm_read_overwrite", unsafe {
        mach_vm_read_overwrite(
            task,
            address,
            size as u64,
            buffer.as_mut_ptr() as u64,
            &mut read,
        )
    })?;
    buffer.truncate(read as usize);
    Ok(buffer)
}

/// The protection of the region containing `address`.
pub fn region_protection(task: mach_port_t, address: u64) -> Result<(u64, u64, i32), String> {
    let mut start = address;
    let mut size: u64 = 0;
    let mut info = [0i32; 9];
    let mut count = VM_REGION_BASIC_INFO_COUNT_64;
    let mut object: mach_port_t = 0;
    // SAFETY: `info` has the nine words VM_REGION_BASIC_INFO_64 returns.
    check("mach_vm_region", unsafe {
        mach_vm_region(
            task,
            &mut start,
            &mut size,
            VM_REGION_BASIC_INFO_64,
            info.as_mut_ptr(),
            &mut count,
            &mut object,
        )
    })?;
    if object != 0 {
        release_port(object);
    }
    Ok((start, size, info[0]))
}

/// Change protection on `[address, address + size)`.
pub fn protect(task: mach_port_t, address: u64, size: u64, protection: i32) -> Result<(), String> {
    // SAFETY: plain value arguments.
    check("mach_vm_protect", unsafe {
        mach_vm_protect(task, address, size, 0, protection)
    })
}

/// Write `data` at `address`, making a private copy of read-only pages.
///
/// A code page is mapped read+execute. Writing a breakpoint into it needs the
/// page made writable first (copy-on-write, so the file-backed original is not
/// touched), and the original protection put back afterwards.
pub fn write_memory(task: mach_port_t, address: u64, data: &[u8]) -> Result<(), String> {
    if data.is_empty() {
        return Ok(());
    }
    let (_, _, original) = region_protection(task, address)?;
    let end = address + data.len() as u64;
    let writable = original & VM_PROT_WRITE != 0;
    // Page granularity: protect the whole span the write touches. Apple
    // Silicon pages are 16 KiB, Intel's 4 KiB; ask rather than assume.
    // SAFETY: no arguments.
    let page = u64::from(unsafe { getpagesize() }.max(0x1000) as u32);
    let span_start = address & !(page - 1);
    let span_end = (end + page - 1) & !(page - 1);
    if !writable {
        protect(
            task,
            span_start,
            span_end - span_start,
            VM_PROT_READ | VM_PROT_WRITE | VM_PROT_COPY,
        )?;
    }
    // SAFETY: `data` is a live slice for the duration of the call.
    let written = check("mach_vm_write", unsafe {
        mach_vm_write(task, address, data.as_ptr() as usize, data.len() as u32)
    });
    if !writable {
        // Put the protection back even if the write failed.
        let restored = protect(task, span_start, span_end - span_start, original);
        written?;
        restored?;
        return Ok(());
    }
    written
}

/// `(all_image_info_addr, all_image_info_size)` of the target's dyld.
pub fn dyld_info(task: mach_port_t) -> Result<(u64, u64), String> {
    let mut info = [0i32; 5];
    let mut count = info.len() as u32;
    // SAFETY: TASK_DYLD_INFO returns five words.
    check("task_info", unsafe {
        task_info(task, TASK_DYLD_INFO, info.as_mut_ptr(), &mut count)
    })?;
    let address = u64::from(info[0] as u32) | (u64::from(info[1] as u32) << 32);
    let size = u64::from(info[2] as u32) | (u64::from(info[3] as u32) << 32);
    Ok((address, size))
}

/// `ptrace(2)` with an optional address and a data word. Returns the errno on failure.
pub fn ptrace_request(request: i32, pid: u32, addr: usize, data: i32) -> Result<i32, i32> {
    // SAFETY: the kernel validates the request against the tracee; `addr` is
    // only ever 0 or the documented `1` ("continue from here").
    let result = unsafe {
        *libc_errno() = 0;
        ptrace(request, pid as i32, addr as *mut std::ffi::c_char, data)
    };
    if result == -1 {
        // SAFETY: reading the thread-local errno.
        let errno = unsafe { *libc_errno() };
        if errno != 0 {
            return Err(errno);
        }
    }
    Ok(result)
}

unsafe extern "C" {
    #[link_name = "__error"]
    fn libc_errno() -> *mut i32;
}

/// `waitpid(2)`; `Ok(None)` when `WNOHANG` found nothing.
pub fn wait_pid(pid: u32, options: i32) -> Result<Option<i32>, i32> {
    let mut status = 0i32;
    // SAFETY: `status` is a valid out-pointer.
    let result = unsafe { waitpid(pid as i32, &mut status, options) };
    match result {
        -1 => Err(unsafe { *libc_errno() }),
        0 => Ok(None),
        _ => Ok(Some(status)),
    }
}

/// How a `waitpid` status word decodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitOutcome {
    Exited(i32),
    Signaled(i32),
    Stopped(i32),
}

/// Decode a `waitpid` status (the `W*` macros of `<sys/wait.h>`).
pub fn decode_wait_status(status: i32) -> WaitOutcome {
    let low = status & 0x7f;
    if low == 0 {
        WaitOutcome::Exited((status >> 8) & 0xff)
    } else if low == 0x7f {
        WaitOutcome::Stopped((status >> 8) & 0xff)
    } else {
        WaitOutcome::Signaled(low)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wait_status_words_decode_like_the_wait_macros() {
        // exit(3): status = 3 << 8.
        assert_eq!(decode_wait_status(3 << 8), WaitOutcome::Exited(3));
        // stopped by SIGTRAP (5): (5 << 8) | 0x7f.
        assert_eq!(decode_wait_status((5 << 8) | 0x7f), WaitOutcome::Stopped(5));
        // killed by SIGKILL (9).
        assert_eq!(decode_wait_status(9), WaitOutcome::Signaled(9));
    }

    #[test]
    fn a_process_that_does_not_exist_has_no_task_port() {
        // pid 0x7fff_fff0 is not a process. The call must come back (not hang)
        // with a refusal.
        let result = task_port_for_pid(0x7fff_fff0, Duration::from_secs(5));
        assert!(
            matches!(result, Err(TaskPortError::Denied(_))),
            "{result:?}"
        );
    }
}
