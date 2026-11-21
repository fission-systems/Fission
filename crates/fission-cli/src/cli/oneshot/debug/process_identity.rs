#[cfg(any(target_os = "linux", windows))]
use anyhow::Context;
use anyhow::Result;

#[cfg(target_os = "linux")]
pub(super) fn process_identity(pid: u32) -> Result<String> {
    let boot = std::fs::read_to_string("/proc/sys/kernel/random/boot_id")?;
    let stat = std::fs::read(format!("/proc/{pid}/stat"))?;
    // The command name can contain spaces, parentheses, and non-UTF-8 bytes.
    let end = stat
        .iter()
        .rposition(|&byte| byte == b')')
        .context("missing process command")?;
    let fields = std::str::from_utf8(&stat[end + 1..])?;
    // Fields after the command start with field 3. Start time is field 22.
    let start: u64 = fields
        .split_ascii_whitespace()
        .nth(19)
        .context("missing process start time")?
        .parse()?;
    Ok(format!("{}:{start}", boot.trim()))
}

#[cfg(target_os = "macos")]
pub(super) fn process_identity(pid: u32) -> Result<String> {
    let pid = i32::try_from(pid)?;
    anyhow::ensure!(pid > 0, "invalid process ID");
    let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::uninit();
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as i32;
    // SAFETY: the buffer is aligned and writable for exactly `size` bytes.
    let written = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    anyhow::ensure!(written == size, "process start information is unavailable");
    // SAFETY: PROC_PIDTBSDINFO reported a complete proc_bsdinfo.
    let info = unsafe { info.assume_init() };
    Ok(format!(
        "{}:{}",
        info.pbi_start_tvsec, info.pbi_start_tvusec
    ))
}

#[cfg(windows)]
pub(super) fn process_identity(pid: u32) -> Result<String> {
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME};
    use windows_sys::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    // SAFETY: OpenProcess has no pointer parameters and returns an owned handle.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return Err(std::io::Error::last_os_error()).context("open process for start information");
    }
    let mut created = FILETIME::default();
    let (mut exited, mut kernel, mut user) = (created, created, created);
    // SAFETY: the handle is open and each output is a distinct, writable FILETIME.
    let success =
        unsafe { GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) };
    let error = (success == 0).then(std::io::Error::last_os_error);
    // SAFETY: no code will use the handle after this release.
    unsafe { CloseHandle(handle) };
    if let Some(error) = error {
        return Err(error).context("read process start information");
    }
    Ok(format!(
        "{}:{}",
        created.dwHighDateTime, created.dwLowDateTime
    ))
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
pub(super) fn process_identity(_pid: u32) -> Result<String> {
    anyhow::bail!("process identity is unavailable on this platform");
}
