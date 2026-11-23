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
    target_stdout: i32,
    target_stderr: i32,
    _devnull: std::fs::File,
}

#[cfg(unix)]
impl OutputSilencer {
    pub fn new() -> io::Result<Self> {
        Self::new_for_fds(libc::STDOUT_FILENO, libc::STDERR_FILENO)
    }

    fn new_for_fds(target_stdout: i32, target_stderr: i32) -> io::Result<Self> {
        let devnull = OpenOptions::new().write(true).open("/dev/null")?;
        let stdout_fd = unsafe { libc::dup(target_stdout) };
        if stdout_fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let stderr_fd = unsafe { libc::dup(target_stderr) };
        if stderr_fd < 0 {
            let err = io::Error::last_os_error();
            unsafe { libc::close(stdout_fd) };
            return Err(err);
        }

        let null_fd = devnull.as_raw_fd();
        if unsafe { libc::dup2(null_fd, target_stdout) } < 0 {
            let err = io::Error::last_os_error();
            unsafe {
                libc::close(stdout_fd);
                libc::close(stderr_fd);
            }
            return Err(err);
        }
        if unsafe { libc::dup2(null_fd, target_stderr) } < 0 {
            let err = io::Error::last_os_error();
            unsafe {
                libc::dup2(stdout_fd, target_stdout);
                libc::close(stdout_fd);
                libc::close(stderr_fd);
            }
            return Err(err);
        }

        Ok(Self {
            stdout_fd,
            stderr_fd,
            target_stdout,
            target_stderr,
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
            libc::dup2(self.stdout_fd, self.target_stdout);
            libc::dup2(self.stderr_fd, self.target_stderr);
            libc::close(self.stdout_fd);
            libc::close(self.stderr_fd);
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::OutputSilencer;
    use std::io::Read;
    use std::os::unix::io::FromRawFd;

    fn make_pipe() -> (i32, i32) {
        let mut fds = [0i32; 2];
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        (fds[0], fds[1])
    }

    #[test]
    fn failed_stderr_dup2_restores_stdout_instead_of_leaving_devnull() {
        let (read_fd, write_fd) = make_pipe();
        let err = OutputSilencer::new_for_fds(write_fd, -1)
            .err()
            .expect("invalid stderr fd must fail");
        assert_eq!(err.raw_os_error(), Some(libc::EBADF));

        let msg = b"restored";
        let written = unsafe { libc::write(write_fd, msg.as_ptr().cast(), msg.len()) };
        assert_eq!(written, msg.len() as isize);
        unsafe { libc::close(write_fd) };

        let mut reader = unsafe { std::fs::File::from_raw_fd(read_fd) };
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).expect("read pipe");
        assert_eq!(buf, msg);
    }

    #[test]
    fn silencer_redirects_while_alive_and_restores_on_drop() {
        let (out_r, out_w) = make_pipe();
        let (err_r, err_w) = make_pipe();

        let silencer = OutputSilencer::new_for_fds(out_w, err_w).expect("create silencer");
        let silenced = b"dropped";
        assert_eq!(
            unsafe { libc::write(out_w, silenced.as_ptr().cast(), silenced.len()) },
            silenced.len() as isize
        );
        assert_eq!(
            unsafe { libc::write(err_w, silenced.as_ptr().cast(), silenced.len()) },
            silenced.len() as isize
        );
        drop(silencer);

        let after = b"after";
        assert_eq!(
            unsafe { libc::write(out_w, after.as_ptr().cast(), after.len()) },
            after.len() as isize
        );
        assert_eq!(
            unsafe { libc::write(err_w, after.as_ptr().cast(), after.len()) },
            after.len() as isize
        );
        unsafe {
            libc::close(out_w);
            libc::close(err_w);
        }

        let mut out_buf = Vec::new();
        let mut err_buf = Vec::new();
        unsafe { std::fs::File::from_raw_fd(out_r) }
            .read_to_end(&mut out_buf)
            .unwrap();
        unsafe { std::fs::File::from_raw_fd(err_r) }
            .read_to_end(&mut err_buf)
            .unwrap();
        assert_eq!(out_buf, after);
        assert_eq!(err_buf, after);
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
