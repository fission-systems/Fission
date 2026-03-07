#[cfg(unix)]
use std::fs::OpenOptions;
#[cfg(unix)]
use std::io;

#[cfg(unix)]
use std::os::unix::io::AsRawFd;

#[cfg(unix)]
pub struct OutputSilencer {
    stdout_fd: i32,
    stderr_fd: i32,
    _devnull: std::fs::File,
}

#[cfg(unix)]
impl OutputSilencer {
    pub fn new() -> io::Result<Self> {
        let devnull = OpenOptions::new().write(true).open("/dev/null")?;
        let stdout_fd = unsafe { libc::dup(libc::STDOUT_FILENO) };
        if stdout_fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let stderr_fd = unsafe { libc::dup(libc::STDERR_FILENO) };
        if stderr_fd < 0 {
            unsafe { libc::close(stdout_fd) };
            return Err(io::Error::last_os_error());
        }

        let null_fd = devnull.as_raw_fd();
        if unsafe { libc::dup2(null_fd, libc::STDOUT_FILENO) } < 0 {
            unsafe {
                libc::close(stdout_fd);
                libc::close(stderr_fd);
            }
            return Err(io::Error::last_os_error());
        }
        if unsafe { libc::dup2(null_fd, libc::STDERR_FILENO) } < 0 {
            unsafe {
                libc::close(stdout_fd);
                libc::close(stderr_fd);
            }
            return Err(io::Error::last_os_error());
        }

        Ok(Self {
            stdout_fd,
            stderr_fd,
            _devnull: devnull,
        })
    }

    pub fn new_if(enabled: bool) -> Option<Self> {
        if enabled { Self::new().ok() } else { None }
    }
}

#[cfg(unix)]
impl Drop for OutputSilencer {
    fn drop(&mut self) {
        unsafe {
            libc::dup2(self.stdout_fd, libc::STDOUT_FILENO);
            libc::dup2(self.stderr_fd, libc::STDERR_FILENO);
            libc::close(self.stdout_fd);
            libc::close(self.stderr_fd);
        }
    }
}

#[cfg(not(unix))]
pub struct OutputSilencer;

#[cfg(not(unix))]
impl OutputSilencer {
    pub fn new_if(_enabled: bool) -> Option<Self> {
        None
    }
}
