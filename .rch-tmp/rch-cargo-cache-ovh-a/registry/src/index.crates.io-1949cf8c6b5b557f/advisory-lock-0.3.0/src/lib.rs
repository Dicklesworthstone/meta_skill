//! Advisory lock provides simple and convenient API for using file locks.
//!
//! These are called advisory because they don't prevent other processes from
//! accessing the files directly, bypassing the locks.
//! However, if multiple processes agree on acquiring file locks, they should
//! work as expected.
//!
//! The main entity of the crate is [`AdvisoryFileLock`] which is effectively
//! a [`RwLock`] but for [`File`].
//!
//! Example:
//! ```
//! use std::fs::File;
//! use advisory_lock::{AdvisoryFileLock, FileLockMode, FileLockError};
//! #
//! #
//! // Create the file and obtain its exclusive advisory lock
//! let exclusive_file = File::create("foo.txt").unwrap();
//! exclusive_file.lock(FileLockMode::Exclusive)?;
//!
//! let shared_file = File::open("foo.txt")?;
//!
//! // Try to acquire the lock in non-blocking way
//! assert!(matches!(shared_file.try_lock(FileLockMode::Shared), Err(FileLockError::AlreadyLocked)));
//!
//! exclusive_file.unlock()?;
//!
//! shared_file.try_lock(FileLockMode::Shared).expect("Works, because the exclusive lock was released");
//!
//! let shared_file_2 = File::open("foo.txt")?;
//!
//! shared_file_2.lock(FileLockMode::Shared).expect("Should be fine to have multiple shared locks");
//!
//! // Nope, now we have to wait until all shared locks are released...
//! assert!(matches!(exclusive_file.try_lock(FileLockMode::Exclusive), Err(FileLockError::AlreadyLocked)));
//!
//! // We can unlock them explicitly and handle the potential error
//! shared_file.unlock()?;
//! // Or drop the lock, such that we `log::error!()` if it happens and discard it
//! drop(shared_file_2);
//!
//! exclusive_file.lock(FileLockMode::Exclusive).expect("All other locks should have been released");
//! #
//! # std::fs::remove_file("foo.txt")?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! [`AdvisoryFileLock`]: struct.AdvisoryFileLock.html
//! [`RwLock`]: https://doc.rust-lang.org/stable/std/sync/struct.RwLock.html
//! [`File`]: https://doc.rust-lang.org/stable/std/fs/struct.File.html
use std::{fmt, io};

#[cfg(windows)]
mod windows;

#[cfg(unix)]
mod unix;

/// An enumeration of possible errors which can occur while trying to acquire a lock.
#[derive(Debug)]
pub enum FileLockError {
    /// The file is already locked by other process.
    AlreadyLocked,
    /// The error occurred during I/O operations.
    Io(io::Error),
}

impl fmt::Display for FileLockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FileLockError::AlreadyLocked => f.write_str("the file is already locked"),
            FileLockError::Io(err) => write!(f, "I/O error: {}", err),
        }
    }
}

impl std::error::Error for FileLockError {}

/// An enumeration of types which represents how to acquire an advisory lock.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum FileLockMode {
    /// Obtain an exclusive file lock.
    Exclusive,
    /// Obtain a shared file lock.
    Shared,
}

/// An advisory lock for files.
///
/// An advisory lock provides a mutual-exclusion mechanism among processes which explicitly
/// acquires and releases the lock. Processes that are not aware of the lock will ignore it.
///
/// `AdvisoryFileLock` provides following features:
/// - Blocking or non-blocking operations.
/// - Shared or exclusive modes.
/// - All operations are thread-safe.
///
/// ## Notes
///
/// `AdvisoryFileLock` has following limitations:
/// - Locks are allowed only on files, but not directories.
pub trait AdvisoryFileLock {
    /// Acquire the advisory file lock.
    ///
    /// `lock` is blocking; it will block the current thread until it succeeds or errors.
    fn lock(&self, file_lock_mode: FileLockMode) -> Result<(), FileLockError>;
    /// Try to acquire the advisory file lock.
    ///
    /// `try_lock` returns immediately.
    fn try_lock(&self, file_lock_mode: FileLockMode) -> Result<(), FileLockError>;
    /// Unlock this advisory file lock.
    fn unlock(&self) -> Result<(), FileLockError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;
    use std::fs::File;

    #[test]
    fn simple_shared_lock() {
        let mut test_file = temp_dir();
        test_file.push("shared_lock");
        File::create(&test_file).unwrap();
        {
            let f1 = File::open(&test_file).unwrap();
            f1.lock(FileLockMode::Shared).unwrap();
            let f2 = File::open(&test_file).unwrap();
            f2.lock(FileLockMode::Shared).unwrap();
        }
        std::fs::remove_file(&test_file).unwrap();
    }

    #[test]
    fn simple_exclusive_lock() {
        let mut test_file = temp_dir();
        test_file.push("exclusive_lock");
        File::create(&test_file).unwrap();
        {
            let f1 = File::open(&test_file).unwrap();
            f1.lock(FileLockMode::Exclusive).unwrap();
            let f2 = File::open(&test_file).unwrap();
            assert!(f2.try_lock(FileLockMode::Exclusive).is_err());
        }
        std::fs::remove_file(&test_file).unwrap();
    }

    #[test]
    fn simple_shared_exclusive_lock() {
        let mut test_file = temp_dir();
        test_file.push("shared_exclusive_lock");
        File::create(&test_file).unwrap();
        {
            let f1 = File::open(&test_file).unwrap();
            f1.lock(FileLockMode::Shared).unwrap();
            let f2 = File::open(&test_file).unwrap();
            assert!(matches!(
                f2.try_lock(FileLockMode::Exclusive),
                Err(FileLockError::AlreadyLocked)
            ));
        }
        std::fs::remove_file(&test_file).unwrap();
    }

    #[test]
    fn simple_exclusive_shared_lock() {
        let mut test_file = temp_dir();
        test_file.push("exclusive_shared_lock");
        File::create(&test_file).unwrap();
        {
            let f1 = File::open(&test_file).unwrap();
            f1.lock(FileLockMode::Exclusive).unwrap();
            let f2 = File::open(&test_file).unwrap();
            assert!(f2.try_lock(FileLockMode::Shared).is_err());
        }
        std::fs::remove_file(&test_file).unwrap();
    }
}
