//! This module includes errors used by this crate.

use std::fmt;

/// An error that arises when equal lengths are required but not found.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LengthMismatchError;

impl fmt::Display for LengthMismatchError {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "items of equal length are required")
    }
}
