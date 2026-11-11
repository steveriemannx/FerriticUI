//! A PTY session: a non-blocking master and a shell child.
//!
//! The setup mirrors orbterm's: `/dev/ptmx` + grantpt/unlockpt, a shell with
//! the slave as stdio, `setsid`/`TIOCSCTTY` in the child, and a window size
//! set with `TIOCSWINSZ`. On Redox the pty scheme delivers packets whose first
//! byte is a flag; on other Unix hosts the master is a raw byte stream.

use std::ffi::{CStr, CString};
use std::fs::{File, OpenOptions};
use std::io::{self, ErrorKind, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};

extern "C" {
    fn ptsname(fd: libc::c_int) -> *const libc::c_char;
    fn grantpt(fd: libc::c_int) -> libc::c_int;
    fn unlockpt(fd: libc::c_int) -> libc::c_int;
}

pub struct Session {
    master: File,
    child: Child,
    closed: bool,
}

impl Session {
    pub fn spawn(shell: &str, cols: u16, rows: u16) -> io::Result<Self> {
        let (master_fd, tty_path) = getpty(cols, rows)?;
        let master = unsafe { File::from_raw_fd(master_fd) };
        let (slave_in, slave_out, slave_err) = slave_stdio(&tty_path)?;

        let mut command = Command::new(shell);
        command
            .stdin(Stdio::from(slave_in))
            .stdout(Stdio::from(slave_out))
            .stderr(Stdio::from(slave_err))
            .env_remove("COLUMNS")
            .env_remove("LINES")
            .env("TTY", &tty_path)
            .env("TERM", "xterm-256color");
        unsafe {
            command.pre_exec(before_exec);
        }
        let child = command.spawn()?;

        Ok(Self {
            master,
            child,
            closed: false,
        })
    }

    /// Reads all currently available output into `out`, returning the number
    /// of bytes appended.
    pub fn read(&mut self, out: &mut Vec<u8>) -> io::Result<usize> {
        let start = out.len();
        let mut buffer = [0u8; 8192];
        loop {
            match self.master.read(&mut buffer) {
                Ok(0) => {
                    self.closed = true;
                    break;
                }
                Ok(count) => {
                    #[cfg(target_os = "redox")]
                    {
                        // The Redox pty scheme prefixes each packet with a
                        // flag byte (redraw hint); the payload follows it.
                        if count > 1 {
                            out.extend_from_slice(&buffer[1..count]);
                        }
                    }
                    #[cfg(not(target_os = "redox"))]
                    {
                        out.extend_from_slice(&buffer[..count]);
                    }
                }
                Err(ref error) if error.kind() == ErrorKind::WouldBlock => break,
                Err(ref error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        Ok(out.len() - start)
    }

    pub fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.master.write_all(bytes)?;
        self.master.flush()
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        let size = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        unsafe {
            libc::ioctl(self.master.as_raw_fd(), libc::TIOCSWINSZ, &size);
        }
    }

    pub fn exited(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)))
    }

    pub fn closed(&self) -> bool {
        self.closed
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn getpty(cols: u16, rows: u16) -> io::Result<(RawFd, String)> {
    let master_fd = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open("/dev/ptmx")?
        .into_raw_fd();

    unsafe {
        if grantpt(master_fd) < 0 {
            return Err(io::Error::last_os_error());
        }
        if unlockpt(master_fd) < 0 {
            return Err(io::Error::last_os_error());
        }
        let size = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        if libc::ioctl(master_fd, libc::TIOCSWINSZ, &size) < 0 {
            return Err(io::Error::last_os_error());
        }
        let name = ptsname(master_fd);
        if name.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok((
            master_fd,
            CStr::from_ptr(name).to_string_lossy().into_owned(),
        ))
    }
}

fn slave_stdio(tty_path: &str) -> io::Result<(File, File, File)> {
    let path = CString::new(tty_path)
        .map_err(|_| io::Error::new(ErrorKind::InvalidInput, "pty path contains a nul byte"))?;
    let open = |flags: libc::c_int| -> io::Result<File> {
        let fd = unsafe { libc::open(path.as_ptr(), flags | libc::O_CLOEXEC) };
        if fd < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(unsafe { File::from_raw_fd(fd) })
        }
    };
    Ok((
        open(libc::O_RDONLY)?,
        open(libc::O_WRONLY)?,
        open(libc::O_WRONLY)?,
    ))
}

fn before_exec() -> io::Result<()> {
    unsafe {
        if libc::setsid() < 0 {
            return Err(io::Error::last_os_error());
        }
        // Redox's relibc does not implement TIOCSCTTY (it logs a TODO and
        // returns success); the slave still becomes the shell's tty.
        #[cfg(not(target_os = "redox"))]
        {
            if libc::ioctl(0, libc::TIOCSCTTY, 0) < 0 {
                return Err(io::Error::last_os_error());
            }
        }
    }
    Ok(())
}
