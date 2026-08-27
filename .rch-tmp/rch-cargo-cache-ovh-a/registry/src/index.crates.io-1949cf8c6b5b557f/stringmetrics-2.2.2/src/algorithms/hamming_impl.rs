//! # Hamming Distance Computations
//!
//! The hamming distance between two equal length strings is the number of
//! substitutions required to change one string into the other. The functions in
//! this module help to calculate that.

use crate::errors::LengthMismatchError;

/// Hamming distance computations
///
/// Calculate the number of substitutions required to change one string into the
/// other.
///
/// # Example
///
/// ```
/// use stringmetrics::hamming;
/// use stringmetrics::errors::LengthMismatchError;
///
/// assert_eq!(hamming("abcdefg", "aaadefa"), Ok(3));
/// assert_eq!(hamming("abcdefg", "xaaadefa"), Err(LengthMismatchError));
/// ```
///
/// # Errors
///
/// Returns a [`LengthMismatchError`] if the two elements are not of the same
/// length
#[inline]
pub fn hamming(a: &str, b: &str) -> Result<u32, LengthMismatchError> {
    // Error case already handled
    hamming_iter(a.chars(), b.chars())
}

/// Apply the hamming function to iterators
///
/// This is useful for avoiding the need to collect data before performing the
/// calculation. `a` and `b` are mutable iterators; mutability is required as
/// the iterator will be consumed.
///
/// Returns a [`Result`] with `Ok(v)` if the function completed successfully, or
/// `Err(s)` if there was an iterator length mismatch.
///
/// # Example
///
/// ```
/// use stringmetrics::hamming_iter;
/// let a = "abcdefg";
/// let b = "aaadefa";
/// assert_eq!(hamming_iter(a.chars(), b.chars()), Ok(3));
///
/// assert_eq!(hamming_iter(1..3, 2..4), Ok(2));
/// ```
///
/// # Errors
///
/// Returns a [`LengthMismatchError`] if the two iterators are not of the same
/// length
#[inline]
pub fn hamming_iter<I: IntoIterator<Item = T>, T: PartialEq>(
    a: I,
    b: I,
) -> Result<u32, LengthMismatchError> {
    let mut distance = 0u32;
    let mut a_iter = a.into_iter();
    let mut b_iter = b.into_iter();

    loop {
        match (a_iter.next(), b_iter.next()) {
            (Some(aa), Some(bb)) => {
                if aa != bb {
                    distance += 1;
                }
            }
            (Some(_), None) | (None, Some(_)) => return Err(LengthMismatchError),
            (None, None) => break,
        }
    }

    Ok(distance)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_err_not_equal() {
        assert_eq!(hamming("abc", "ab"), Err(LengthMismatchError));
        assert_eq!(hamming("abcd", "ab"), Err(LengthMismatchError));
        assert_eq!(hamming("ab", "abc"), Err(LengthMismatchError));
        assert_eq!(hamming("ab", "abcd"), Err(LengthMismatchError));
    }

    #[test]
    fn test_empty_string() {
        assert_eq!(hamming("", ""), Ok(0));
    }

    #[test]
    fn test_basic() {
        assert_eq!(hamming("abcdefg", "0bc1ef2"), Ok(3));
    }

    #[test]
    fn test_iter() {
        assert_eq!(hamming_iter(1..3, 2..4), Ok(2));
    }
}
