#[cfg(target_os = "windows")]
use windows::Win32::Foundation::HANDLE;

#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub process_id: u32,
    pub thread_id: u32,

    #[cfg(target_os = "windows")]
    pub process_handle: HANDLE,
    #[cfg(not(target_os = "windows"))]
    pub process_handle: usize,

    #[cfg(target_os = "windows")]
    pub thread_handle: HANDLE,
    #[cfg(not(target_os = "windows"))]
    pub thread_handle: usize,

    pub image_base: u64,
    pub entry_point: u64,
}

#[derive(Debug, Clone)]
pub enum DebugEvent {
    ProcessCreated(ProcessInfo),
    ThreadCreated(u32),
    Exception {
        code: u32,
        address: u64,
        first_chance: bool,
    },
    Breakpoint(u64), // Address
    ProcessExit,
}
