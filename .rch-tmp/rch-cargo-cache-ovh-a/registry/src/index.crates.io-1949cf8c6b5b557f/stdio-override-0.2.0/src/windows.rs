use std::fs::File;
use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, IntoRawHandle, RawHandle};
use std::ptr;

use winapi::shared::minwindef::{BOOL, DWORD, FALSE, TRUE};
use winapi::um::handleapi::{CloseHandle, DuplicateHandle, GetHandleInformation, INVALID_HANDLE_VALUE};
use winapi::um::processenv::{GetStdHandle, SetStdHandle};
use winapi::um::processthreadsapi::GetCurrentProcess;
use winapi::um::winbase::HANDLE_FLAG_INHERIT;
use winapi::um::winbase::{STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE};
use winapi::um::winnt::DUPLICATE_SAME_ACCESS;

pub(crate) use std::os::windows::io::{AsRawHandle as AsRaw, IntoRawHandle as IntoRaw, RawHandle as Raw};

pub(crate) fn as_raw(io: &impl AsRawHandle) -> RawHandle {
    io.as_raw_handle()
}
pub(crate) fn into_raw(io: impl IntoRawHandle) -> RawHandle {
    io.into_raw_handle()
}

pub(crate) fn override_stdin(io: RawHandle, owned: bool) -> io::Result<File> {
    override_stdio(STD_INPUT_HANDLE, io, owned)
}
pub(crate) fn override_stdout(io: RawHandle, owned: bool) -> io::Result<File> {
    override_stdio(STD_OUTPUT_HANDLE, io, owned)
}
pub(crate) fn override_stderr(io: RawHandle, owned: bool) -> io::Result<File> {
    override_stdio(STD_ERROR_HANDLE, io, owned)
}

pub(crate) fn reset_stdin(old: RawHandle) -> io::Result<()> {
    reset_stdio(STD_INPUT_HANDLE, old)
}
pub(crate) fn reset_stdout(old: RawHandle) -> io::Result<()> {
    reset_stdio(STD_OUTPUT_HANDLE, old)
}
pub(crate) fn reset_stderr(old: RawHandle) -> io::Result<()> {
    reset_stdio(STD_ERROR_HANDLE, old)
}

fn override_stdio(stdio: DWORD, other: RawHandle, owned: bool) -> io::Result<File> {
    let original = handle_res(unsafe { GetStdHandle(stdio) })?;

    let other = if owned {
        other
    } else {
        // If it isn't owned, duplicate the handle to prevent closing the original handle from
        // closing the stdio handle.

        let process = unsafe { GetCurrentProcess() };

        let mut handle_information = 0;
        io_res(unsafe { GetHandleInformation(other, &mut handle_information as *mut DWORD) })?;
        let inherit_handle = if handle_information & HANDLE_FLAG_INHERIT == HANDLE_FLAG_INHERIT { TRUE } else { FALSE };

        let mut target = ptr::null_mut();
        io_res(unsafe {
            DuplicateHandle(
                process,
                other,
                process,
                &mut target as *mut RawHandle,
                0, // ignored
                inherit_handle,
                DUPLICATE_SAME_ACCESS,
            )
        })?;

        target
    };

    io_res(unsafe { SetStdHandle(stdio, other) })?;

    Ok(unsafe { File::from_raw_handle(original) })
}
fn reset_stdio(stdio: DWORD, other: RawHandle) -> io::Result<()> {
    let current = handle_res(unsafe { GetStdHandle(stdio) })?;

    io_res(unsafe { SetStdHandle(stdio, other) })?;

    io_res(unsafe { CloseHandle(current) })?;

    Ok(())
}

fn io_res(res: BOOL) -> io::Result<()> {
    if res == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn handle_res(res: RawHandle) -> io::Result<RawHandle> {
    if res == INVALID_HANDLE_VALUE {
        Err(io::Error::last_os_error())
    } else {
        Ok(res)
    }
}

impl AsRawHandle for crate::StdinOverride {
    fn as_raw_handle(&self) -> RawHandle {
        self.original.as_raw_handle()
    }
}
impl AsRawHandle for crate::StdoutOverride {
    fn as_raw_handle(&self) -> RawHandle {
        self.original.as_raw_handle()
    }
}
impl AsRawHandle for crate::StderrOverride {
    fn as_raw_handle(&self) -> RawHandle {
        self.original.as_raw_handle()
    }
}
