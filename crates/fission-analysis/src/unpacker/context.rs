#[cfg(target_os = "windows")]
use windows::{
    Win32::Foundation::*, Win32::System::Diagnostics::Debug::*, Win32::System::Threading::*,
    core::*,
};

/// Wrapper around platform-specific CONTEXT structure.
/// Provides safe access to registers.
#[derive(Debug, Clone)]
pub struct ThreadContext {
    #[cfg(target_os = "windows")]
    pub raw: CONTEXT,
    #[cfg(not(target_os = "windows"))]
    pub raw: (),
}

impl ThreadContext {
    #[cfg(target_os = "windows")]
    pub fn new() -> Self {
        Self {
            raw: CONTEXT::default(),
        }
    }

    #[cfg(not(target_os = "windows"))]
    pub fn new() -> Self {
        Self { raw: () }
    }

    // --- Register Accessors (x64) ---

    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    pub fn rip(&self) -> u64 {
        self.raw.Rip
    }

    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    pub fn set_rip(&mut self, value: u64) {
        self.raw.Rip = value;
    }

    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    pub fn rax(&self) -> u64 {
        self.raw.Rax
    }

    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    pub fn set_rax(&mut self, value: u64) {
        self.raw.Rax = value;
    }

    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    pub fn rsp(&self) -> u64 {
        self.raw.Rsp
    }

    // Add more registers as needed...
}

/// Get thread context.
///
/// # Arguments
/// * `thread_handle` - Handle to the thread.
/// * `suspend` - Whether to suspend the thread before getting context (recommended if running).
#[cfg(target_os = "windows")]
pub fn get_thread_context(thread_handle: HANDLE, suspend: bool) -> Result<ThreadContext, String> {
    unsafe {
        if suspend {
            if SuspendThread(thread_handle) == u32::MAX {
                return Err(format!(
                    "SuspendThread failed: {:?}",
                    std::io::Error::last_os_error()
                ));
            }
        }

        let mut ctx = CONTEXT::default();
        ctx.ContextFlags = CONTEXT_ALL; // Request all registers

        let result = GetThreadContext(thread_handle, &mut ctx);

        if suspend {
            ResumeThread(thread_handle);
        }

        if result.as_bool() {
            Ok(ThreadContext { raw: ctx })
        } else {
            Err(format!(
                "GetThreadContext failed: {:?}",
                std::io::Error::last_os_error()
            ))
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn get_thread_context(_thread_handle: usize, _suspend: bool) -> Result<ThreadContext, String> {
    Err("Not supported on this OS".to_string())
}

/// Set thread context.
#[cfg(target_os = "windows")]
pub fn set_thread_context(
    thread_handle: HANDLE,
    context: &ThreadContext,
    suspend: bool,
) -> Result<(), String> {
    unsafe {
        if suspend {
            if SuspendThread(thread_handle) == u32::MAX {
                return Err(format!(
                    "SuspendThread failed: {:?}",
                    std::io::Error::last_os_error()
                ));
            }
        }

        let result = SetThreadContext(thread_handle, &context.raw);

        if suspend {
            ResumeThread(thread_handle);
        }

        if result.as_bool() {
            Ok(())
        } else {
            Err(format!(
                "SetThreadContext failed: {:?}",
                std::io::Error::last_os_error()
            ))
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn set_thread_context(
    _thread_handle: usize,
    _context: &ThreadContext,
    _suspend: bool,
) -> Result<(), String> {
    Err("Not supported on this OS".to_string())
}
