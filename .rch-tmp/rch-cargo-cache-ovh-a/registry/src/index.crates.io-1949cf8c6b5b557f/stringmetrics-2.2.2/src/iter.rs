//! Tools to help with processing iterators

use std::cmp::min;

#[cfg(not(feature = "bench"))]
#[derive(Debug, PartialEq)]
pub(crate) struct IterPairInfo {
    pub(crate) a_len: u32,
    pub(crate) b_len: u32,
    pub(crate) start_same: u32,
    pub(crate) end_same: u32,
}

#[cfg(feature = "bench")]
#[derive(Debug, PartialEq)]
pub struct IterPairInfo {
    pub(crate) a_len: u32,
    pub(crate) b_len: u32,
    pub(crate) start_same: u32,
    pub(crate) end_same: u32,
}

impl IterPairInfo {
    #[allow(dead_code)]
    pub(crate) const fn new(a_len: u32, b_len: u32, start_same: u32, end_same: u32) -> Self {
        Self {
            a_len,
            b_len,
            start_same,
            end_same,
        }
    }

    /// Length of `a` that is different from `b`
    #[inline]
    pub const fn a_diff_len(&self) -> u32 {
        self.a_len - self.start_same - self.end_same
    }

    /// Length of `b` that is different from `a`
    #[inline]
    pub const fn b_diff_len(&self) -> u32 {
        self.b_len - self.start_same - self.end_same
    }
}

/// This function has an unstable API
///
/// It is designed to identify the longest similar start and end sequences in a
/// pair of iterators, as well as their total length.
#[inline]
pub(crate) fn find_eq_end_items<I, T, D>(a: I, b: I) -> IterPairInfo
where
    I: IntoIterator<IntoIter = D>,
    D: DoubleEndedIterator<Item = T> + Clone,
    T: PartialEq,
{
    let a_len;
    let b_len;

    let mut start_same = 0usize;
    // let mut end_same = 0usize;
    let mut counting = true;

    // Weirdly, this outperformed manual += 1 counting
    let mut r_iter = 0..;
    let mut a_iter = a.into_iter();
    let mut b_iter = b.into_iter();

    // We need to recreate the iterator here so we can use it later
    // Note: this is likely doable without cloning if we work from the front,
    // then work from the back, and add the difference.
    let a_iter2 = a_iter.clone();
    let b_iter2 = b_iter.clone();

    loop {
        match (a_iter.next(), b_iter.next()) {
            (Some(a_v), Some(b_v)) => {
                r_iter.next();
                if counting && a_v == b_v {
                    start_same += 1;
                } else if counting {
                    counting = false;
                }
            }
            (Some(_), None) => {
                let common_len = r_iter.next().unwrap();
                a_len = common_len + 1 + a_iter.count();
                b_len = common_len;
                break;
            }
            (None, Some(_)) => {
                let common_len = r_iter.next().unwrap();
                b_len = common_len + 1 + b_iter.count();
                a_len = common_len;
                break;
            }
            (None, None) => {
                let common_len = r_iter.next().unwrap();
                a_len = common_len;
                b_len = common_len;
                break;
            }
        }
    }

    // Get characters at the end that are the same using iterators
    // #[allow(clippy::pattern_type_mismatch)]
    let end_same = end_same(a_iter2, b_iter2, a_len, b_len, start_same);

    IterPairInfo {
        a_len: a_len.try_into().expect("> u32::MAX items"),
        b_len: b_len.try_into().expect("> u32::MAX items"),
        start_same: start_same as u32,
        end_same: end_same as u32,
    }
}

// Get characters the same at the end of an iterator
#[inline]
#[allow(clippy::pattern_type_mismatch)]
fn end_same<D, T>(a_iter: D, b_iter: D, a_len: usize, b_len: usize, start_same: usize) -> usize
where
    D: DoubleEndedIterator<Item = T> + Clone,
    T: PartialEq,
{
    a_iter
        .rev()
        .zip(b_iter.rev())
        // Limit to this difference
        .take(min(a_len, b_len) - start_same)
        // Count if items are equal, break if not
        .take_while(|(a_item, b_item)| a_item == b_item)
        .count()
}

#[inline]
#[cfg(feature = "bench")]
pub fn find_eq_end_items_bench<I, T, D>(a: I, b: I) -> IterPairInfo
where
    I: IntoIterator<IntoIter = D>,
    D: DoubleEndedIterator<Item = T> + Clone,
    T: PartialEq,
{
    find_eq_end_items(a, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_iterpair_difflen() {
        let info = IterPairInfo::new(10, 20, 5, 4);
        assert_eq!(info.a_diff_len(), 1);
        assert_eq!(info.b_diff_len(), 11);
    }

    #[test]
    fn test_find_eq_items() {
        assert_eq!(
            find_eq_end_items("".chars(), "".chars()),
            IterPairInfo::new(0, 0, 0, 0)
        );
        assert_eq!(
            find_eq_end_items("aaaa".chars(), "".chars()),
            IterPairInfo::new(4, 0, 0, 0)
        );
        assert_eq!(
            find_eq_end_items("".chars(), "aaaa".chars()),
            IterPairInfo::new(0, 4, 0, 0)
        );
        assert_eq!(
            find_eq_end_items("abcd".chars(), "abcd".chars()),
            IterPairInfo::new(4, 4, 4, 0)
        );
        assert_eq!(
            find_eq_end_items("aaaa".chars(), "aa".chars()),
            IterPairInfo::new(4, 2, 2, 0)
        );
        assert_eq!(
            find_eq_end_items("aaaa".chars(), "aabbbb".chars()),
            IterPairInfo::new(4, 6, 2, 0)
        );
        assert_eq!(
            find_eq_end_items("xxaa".chars(), "yyyyaa".chars()),
            IterPairInfo::new(4, 6, 0, 2)
        );
        assert_eq!(
            find_eq_end_items("aaxxxxbb".chars(), "aaaabbbb".chars()),
            IterPairInfo::new(8, 8, 2, 2)
        );
        assert_eq!(
            find_eq_end_items("aaaa".chars(), "bbbb".chars()),
            IterPairInfo::new(4, 4, 0, 0)
        );
    }

    #[test]
    fn test_tricky() {
        assert_eq!(
            find_eq_end_items("notate".chars(), "to ate".chars()),
            IterPairInfo::new(6, 6, 0, 3)
        );
        assert_eq!(
            find_eq_end_items("to ate".chars(), "notate".chars()),
            IterPairInfo::new(6, 6, 0, 3)
        );
        assert_eq!(
            find_eq_end_items("to be a".chars(), "not to".chars()),
            IterPairInfo::new(7, 6, 0, 0)
        );
        assert_eq!(
            find_eq_end_items("not to".chars(), "to be a".chars()),
            IterPairInfo::new(6, 7, 0, 0)
        );
        assert_eq!(
            find_eq_end_items("abccc".chars(), "accc".chars()),
            IterPairInfo::new(5, 4, 1, 3)
        );
    }
}
