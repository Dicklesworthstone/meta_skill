use std::fs::File;
use std::io::Error;
use std::os::unix::io::{AsRawFd, RawFd};

use crate::{AdvisoryFileLock, FileLockError, FileLockMode};

impl AdvisoryFileLock for File {
    fn lock(&self, file_lock_mode: FileLockMode) -> Result<(), FileLockError> {
        self.as_raw_fd().lock(file_lock_mode)
    }

    fn try_lock(&self, file_lock_mode: FileLockMode) -> Result<(), FileLockError> {
        self.as_raw_fd().try_lock(file_lock_mode)
    }

    fn unlock(&self) -> Result<(), FileLockError> {
        self.as_raw_fd().unlock()
    }
}

impl AdvisoryFileLock for RawFd {
    fn lock(&self, file_lock_mode: FileLockMode) -> Result<(), FileLockError> {
        lock_file(*self, file_lock_mode, false)
    }

    fn try_lock(&self, file_lock_mode: FileLockMode) -> Result<(), FileLockError> {
        lock_file(*self, file_lock_mode, true)
    }

    fn unlock(&self) -> Result<(), FileLockError> {
        unlock_file(*self)
    }
}

fn lock_file(
    raw_fd: RawFd,
    file_lock_mode: FileLockMode,
    immediate: bool,
) -> Result<(), FileLockError> {
    let mut flags = match file_lock_mode {
        FileLockMode::Shared => libc::LOCK_SH,
        FileLockMode::Exclusive => libc::LOCK_EX,
    };
    if immediate {
        flags |= libc::LOCK_NB;
    }

    let result = unsafe { libc::flock(raw_fd, flags) };
    if result != 0 {
        let last_os_error = Error::last_os_error();
        return Err(match last_os_error.raw_os_error() {
            Some(code) if code == libc::EWOULDBLOCK => FileLockError::AlreadyLocked,
            _ => FileLockError::Io(last_os_error),
        });
    }

    Ok(())
}

fn unlock_file(raw_fd: RawFd) -> Result<(), FileLockError> {
    let result = unsafe { libc::flock(raw_fd, libc::LOCK_UN) };
    if result == 0 {
        Ok(())
    } else {
        Err(FileLockError::Io(Error::last_os_error()))
    }
}
