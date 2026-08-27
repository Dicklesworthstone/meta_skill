//! # Levenshtein text distance calculations module
//!
//! This module contains functions for applying various closeness algorithms. It is not reexporeted.

mod implementation;
mod structures;
pub use implementation::*;
pub use structures::*;

// fn get_similar_count<I,

/// Basic Levenshtein distance computation
///
/// This runs the levenshtein distance algorithm with all costs equal to 1 and
/// with no limits, which is suitable for cases where an exact distance is
/// needed. Use cases are usually those where the strings are known to not be
/// "very different" (e.g., strings have similar lengths). In most cases it is
/// better to use [`levenshtein_limit`] to avoid unnecessary computation.
///
/// Behind the scenes, this wraps [`levenshtein_limit_iter`]. For details on
/// operation, see the [algorithms](crate::algorithms) page.
///
/// # Example
///
/// ```
/// use stringmetrics::levenshtein;
/// let a = "this is a book";
/// let b = "i am a cook";
/// assert_eq!(levenshtein(a, b), 6);
/// ```
///
/// Note that sometimes the levenshtein distance is defined as having a default
/// weight of 2 for substitutions. That isn't the case for this implementation -
/// if you need that functionality, please use [`levenshtein_weight`].
#[inline]
pub fn levenshtein(a: &str, b: &str) -> u32 {
    levenshtein_limit_iter(a.bytes(), b.bytes(), u32::MAX)
}

/// Levenshtein distance computation with a limit
///
/// This will limitate the levshtein distance up to a given maximum value. The
/// usual reason for wanting to do this is to avoid unnecessary computation when
/// a match between two strings can quickly be pruned as "different".
///
/// This function also wraps [`levenshtein_limit_iter`].
///
/// # Example
///
/// ```
/// use stringmetrics::levenshtein_limit;
///
/// let a = "abcdefg";
/// let b = "mmmmmmm";
/// assert_eq!(levenshtein_limit(a, b, 3), 3);
/// ```
#[inline]
pub fn levenshtein_limit(a: &str, b: &str, limit: u32) -> u32 {
    levenshtein_limit_iter(a.bytes(), b.bytes(), limit)
}

/// The same alrogithm as [`levenshtein_limit`] but return an `Option` to
/// indicate if the limit is exceeded
///
/// ```
/// use stringmetrics::try_levenshtein;
///
/// let a = "abcdefg";
/// let b = "mmmmmmmmm";
/// assert_eq!(try_levenshtein(a, b, 3), None);
/// ```
#[inline]
pub fn try_levenshtein(a: &str, b: &str, limit: u32) -> Option<u32> {
    try_levenshtein_iter(a.bytes(), b.bytes(), limit)
}

/// Levenshtein distance computations with adjustable weights and a limit
///
/// Allows setting costs for inserts, deletes and substitutions. See
/// [algorithms](crate::algorithms) for details on weight computation.
///
/// Behind the scenes, this wraps [`levenshtein_weight_iter`].
///
/// # Example
///
/// In this example, an insertion weight of 4, deletion weight of 3, and
/// substitution weight of 2 are used. A limit of 6 is applied, and we see that
/// we hit that limit.
///
/// ```
/// use stringmetrics::{levenshtein_weight, LevWeights};
///
/// let weights = LevWeights::new(4, 3, 2);
/// assert_eq!(levenshtein_weight("kitten", "sitting", 6, &weights), 6);
/// ```
///
/// With a more reasonable limit, we get a representative result. The 8 comes
/// from one added letter (4) and two substitutions.
///
/// ```
/// use stringmetrics::{levenshtein_weight, LevWeights};
///
/// let weights = LevWeights::new(4, 3, 2);
/// assert_eq!(levenshtein_weight("kitten", "sitting", 100, &weights), 8);
/// ```
#[inline]
pub fn levenshtein_weight(a: &str, b: &str, limit: u32, weights: &LevWeights) -> u32 {
    levenshtein_weight_iter(a.bytes(), b.bytes(), limit, weights)
}

/// The same algorithm as [`levenshtein_weight`] but return an `Option` to
/// indicate if the limit is exceeded
#[inline]
pub fn try_levenshtein_weight(a: &str, b: &str, limit: u32, weights: &LevWeights) -> Option<u32> {
    try_levenshtein_weight_iter(a.bytes(), b.bytes(), limit, weights)
}

/// Levenshthein distance computation on anything with [`Iterator`] with items
/// that have [`PartialEq`].
///
/// This can be used when Levenshthein distance is applicable to something other
/// than strings. It wraps [`try_levenshtein_iter`].
///
/// # Example
///
/// ```
/// use stringmetrics::levenshtein_limit_iter;
///
/// assert_eq!(levenshtein_limit_iter("abc".bytes(), "def".bytes(), 10), 3);
/// assert_eq!(levenshtein_limit_iter("abc".bytes(), "def".bytes(), 10), 3);
/// ```
#[inline]
pub fn levenshtein_limit_iter<I, T, D>(a: I, b: I, limit: u32) -> u32
where
    I: IntoIterator<IntoIter = D>,
    D: DoubleEndedIterator<Item = T> + Clone,
    T: PartialEq,
{
    try_levenshtein_iter(a, b, limit).unwrap_or(limit)
}

#[cfg(test)]
mod tests;
