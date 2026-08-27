//! Main functions

use super::{LevState, LevWeights};
use std::cmp::min;

/// The same algorithm as [`levenshtein_limit_iter`] but return an `Option` to
/// indicate if the limit is exceeded
///
/// Returns `Some(u32)` if a distance is found, `None` if a limit is hit
///
/// # Example
///
/// ```
/// use stringmetrics::try_levenshtein_iter;
///
/// assert_eq!(try_levenshtein_iter("abc".bytes(), "abd".bytes(), 10), Some(1));
/// assert_eq!(try_levenshtein_iter("abcdef".bytes(), "wxyz".bytes(), 3), None);
/// ```
#[inline]
pub fn try_levenshtein_iter<I, T, D>(a: I, b: I, limit: u32) -> Option<u32>
where
    I: IntoIterator<IntoIter = D>,
    D: DoubleEndedIterator<Item = T> + Clone,
    T: PartialEq,
{
    // Identical implementation to levenshtein_weight_iter, just avoiding
    let state = LevState::new(a.into_iter(), b.into_iter());
    let LevState {
        a_iter,
        b_iter,
        a_diff_len: a_len,
        b_diff_len: b_len,
    } = state;

    // Only check b_len because if a_len is 0, the loop won't happen
    if b_len == 0 {
        if a_len < limit {
            return Some(a_len);
        } else {
            return None;
        }
    }

    if b_len - a_len > limit {
        return None;
    }

    let mut work_vec: Vec<u32> = (1..=b_len).collect();
    let mut tmp_res = b_len;

    for (i, a_item) in a_iter.enumerate().take_while(|&(i, _)| i < a_len as usize) {
        // Our "horizontal" iterations always start with the leftmost column,
        // which is the insertion cost (or substitution above)
        // temp_res is also our insertion cost base
        let mut sub_base = i as u32;
        tmp_res = sub_base + 1;

        // Go through and do our calculations. we need to preserve the "up left"
        // (sub_base) and "left" (tmp_res) values, the rest can be overwritten
        for (j, b_item) in b_iter
            .clone()
            .enumerate()
            .take_while(|&(j, _)| j < b_len as usize)
        {
            let del_base = work_vec[j];

            // Insertion costs and deletion costs are their bases + 1
            // i.e., the value to the left or above plus 1
            // Substitution cost is equal to the up-left (sub_base) cost if equal,
            // otherwise up-left value + 1.
            if a_item == b_item {
                tmp_res = min(min(tmp_res, del_base) + 1, sub_base);
            } else {
                tmp_res = min(min(tmp_res, del_base), sub_base) + 1;
            }

            // As we shift to the right, our deletion square becomes our
            // substitution square
            sub_base = del_base;

            // Save our insertion cost for the next iteration
            work_vec[j] = tmp_res;
        }

        if tmp_res > limit {
            return None;
        }
    }

    Some(tmp_res)
}

/// Weighted Levenshthein distance computation on anything with [`Iterator`]
/// with items that have [`PartialEq`].
///
/// This can be used when Levenshthein distance is applicable to something other
/// than strings that has not yet been collected to a vector.
///
/// # Example
///
/// ```
/// use stringmetrics::{levenshtein_weight_iter, LevWeights};
///
/// let weights = LevWeights::default();
/// assert_eq!(levenshtein_weight_iter("abc".bytes(), "def".bytes(), 10, &weights), 3);
/// ```
#[inline]
pub fn levenshtein_weight_iter<I, T, D>(a: I, b: I, limit: u32, weights: &LevWeights) -> u32
where
    I: IntoIterator<IntoIter = D>,
    D: DoubleEndedIterator<Item = T> + Clone,
    T: PartialEq,
{
    try_levenshtein_weight_iter(a, b, limit, weights).unwrap_or(limit)
}

/// The same algorithm as [`levenshtein_weight_iter`] but return an `Option` to
/// indicate if the limit is exceeded
#[inline]
pub fn try_levenshtein_weight_iter<I, T, D>(
    a: I,
    b: I,
    limit: u32,
    weights: &LevWeights,
) -> Option<u32>
where
    I: IntoIterator<IntoIter = D>,
    D: DoubleEndedIterator<Item = T> + Clone,
    T: PartialEq,
{
    let mut weights = weights.clone();
    let state = LevState::new_weights(a.into_iter(), b.into_iter(), &mut weights);
    let LevState {
        a_iter,
        b_iter,
        a_diff_len: a_len,
        b_diff_len: b_len,
    } = state;
    let LevWeights {
        insertion: w_ins,
        deletion: w_del,
        substitution: w_sub,
    } = weights;

    // Only check b_len because if a_len is 0, the loop won't happen
    if b_len == 0 {
        let tmp = a_len * w_del;
        if tmp < limit {
            return Some(tmp);
        } else {
            return None;
        }
    }

    if b_len - a_len > limit {
        return None;
    }

    let equal_weights = w_ins == w_del && w_del == w_sub;

    let mut work_vec: Vec<u32> = (w_ins..=(b_len * w_ins)).step_by(w_ins as usize).collect();
    let mut tmp_res = b_len * w_ins;

    for (i, a_item) in a_iter.enumerate().take_while(|&(i, _)| i < a_len as usize) {
        // Our "horizontal" iterations always start with the leftmost column,
        // which is the insertion cost (or substitution above)
        // temp_res is also our insertion cost base
        let mut sub_base = i as u32 * w_del;
        tmp_res = sub_base + w_del;

        // Go through and do our calculations. we need to preserve the "up left"
        // (sub_base) and "left" (tmp_res) values, the rest can be overwritten
        for (j, b_item) in b_iter
            .clone()
            .enumerate()
            .take_while(|&(j, _)| j < b_len as usize)
        {
            let del_base = work_vec[j];

            // Insertion costs and deletion costs are their bases + 1
            // i.e., the value to the left or above plus 1
            if equal_weights {
                if a_item == b_item {
                    tmp_res = min(min(tmp_res, del_base) + w_ins, sub_base);
                } else {
                    tmp_res = min(min(tmp_res, del_base), sub_base) + w_ins;
                }
            } else if a_item == b_item {
                tmp_res = min(min(tmp_res + w_ins, del_base + w_del), sub_base);
            } else {
                tmp_res = min(min(tmp_res + w_ins, del_base + w_del), sub_base + w_sub);
            }

            // As we shift to the right, our deletion square becomes our
            // substitution square
            sub_base = del_base;

            // Save our insertion cost for the next iteration
            work_vec[j] = tmp_res;
        }

        if tmp_res > limit {
            return None;
        }
    }

    Some(tmp_res)
}
