use std::fs::File;
use std::io;
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};

use libc::c_int;
use libc::{STDERR_FILENO, STDIN_FILENO, STDOUT_FILENO};

pub(crate) use std::os::unix::io::{AsRawFd as AsRaw, IntoRawFd as IntoRaw, RawFd as Raw};

pub(crate) fn as_raw(io: &impl AsRawFd) -> RawFd {
    io.as_raw_fd()
}
pub(crate) fn into_raw(io: impl IntoRawFd) -> RawFd {
    io.into_raw_fd()
}

pub(crate) fn override_stdin(io: RawFd, owned: bool) -> io::Result<File> {
    override_stdio(STDIN_FILENO, io, owned)
}
pub(crate) fn override_stdout(io: RawFd, owned: bool) -> io::Result<File> {
    override_stdio(STDOUT_FILENO, io, owned)
}
pub(crate) fn override_stderr(io: RawFd, owned: bool) -> io::Result<File> {
    override_stdio(STDERR_FILENO, io, owned)
}

pub(crate) fn reset_stdin(old: RawFd) -> io::Result<()> {
    set_stdio(STDIN_FILENO, old)
}
pub(crate) fn reset_stdout(old: RawFd) -> io::Result<()> {
    set_stdio(STDOUT_FILENO, old)
}
pub(crate) fn reset_stderr(old: RawFd) -> io::Result<()> {
    set_stdio(STDERR_FILENO, old)
}

fn override_stdio(stdio: RawFd, other: RawFd, owned: bool) -> io::Result<File> {
    let original = io_res(unsafe { libc::dup(stdio) })?;
    set_stdio(stdio, other)?;

    if owned {
        io_res(unsafe { libc::close(other) })?;
    }

    Ok(unsafe { File::from_raw_fd(original) })
}

fn set_stdio(stdio: RawFd, other: RawFd) -> io::Result<()> {
    io_res(unsafe { libc::dup2(other, stdio) })?;
    Ok(())
}

#[cfg(test)]
#[test]
fn test_original() -> io::Result<()> {
    use std::io::{Read, Write};

    let (mut rx, tx) = os_pipe::pipe()?;

    let real_stdout = override_stdio(STDOUT_FILENO, tx.into_raw_fd(), true)?.into_raw_fd();

    println!("Let's see where it's saved");
    io::stdout().lock().flush()?;

    set_stdio(STDOUT_FILENO, real_stdout)?;

    let mut contents = String::new();
    rx.read_to_string(&mut contents)?;
    assert_eq!("Let\'s see where it\'s saved\n", contents);

    println!("got back");

    Ok(())
}

fn io_res(res: c_int) -> io::Result<c_int> {
    if res == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(res)
    }
}

impl AsRawFd for crate::StdinOverride {
    fn as_raw_fd(&self) -> RawFd {
        self.original.as_raw_fd()
    }
}
impl AsRawFd for crate::StdoutOverride {
    fn as_raw_fd(&self) -> RawFd {
        self.original.as_raw_fd()
    }
}
impl AsRawFd for crate::StderrOverride {
    fn as_raw_fd(&self) -> RawFd {
        self.original.as_raw_fd()
    }
}
