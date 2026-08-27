use super::*;

#[test]
fn test_levweights_swap() {
    let mut w = LevWeights::new(10, 20, 30);
    w.swap();
    let expected = LevWeights {
        insertion: 20,
        deletion: 10,
        substitution: 30,
    };
    assert_eq!(w, expected);
}

#[test]
fn test_levstate_new() {
    let a = "aaxxxxxc";
    let b = "aaabbbccc";
    let state = LevState::new(a.bytes(), b.bytes());
    assert_eq!(state.a_diff_len, 5);
    assert_eq!(state.b_diff_len, 6);
}

#[test]
fn test_levenshtein_equal() {
    assert_eq!(levenshtein("abcdef", "abcdef"), 0);
}

#[test]
fn test_levenshtein_empty() {
    assert_eq!(levenshtein("", ""), 0);
    assert_eq!(levenshtein("abcdef", ""), 6);
    assert_eq!(levenshtein("", "abcdef"), 6);
}

#[test]
fn test_levenshtein_basic() {
    assert_eq!(levenshtein("abcd", "ab"), 2);
    assert_eq!(levenshtein("ab", "abcd"), 2);
    assert_eq!(levenshtein("abcd", "ad"), 2);
    assert_eq!(levenshtein("abcd", "cd"), 2);
    assert_eq!(levenshtein("abcd", "a"), 3);
    assert_eq!(levenshtein("abcd", "c"), 3);
    assert_eq!(levenshtein("abcd", "accd"), 1);
    assert_eq!(levenshtein("kitten", "sitting"), 3);
    assert_eq!(levenshtein("sitting", "kitten"), 3);
    assert_eq!(levenshtein("not", "to a"), 3);
    assert_eq!(levenshtein("to be a bee", "not to bee"), 6);
}

#[test]
fn test_levenshtein_trick_skips() {
    // Try to trick the part that skips forward and backward
    assert_eq!(levenshtein("abcd", "abcd"), 0);
    assert_eq!(levenshtein("abcd", "ad"), 2);
    assert_eq!(levenshtein("abcd", "cd"), 2);
    assert_eq!(levenshtein("abcd", "a"), 3);
    assert_eq!(levenshtein("abcd", "b"), 3);
    assert_eq!(levenshtein("abcd", "c"), 3);
    assert_eq!(levenshtein("abcd", "d"), 3);
    assert_eq!(levenshtein("a", "abcd"), 3);
    assert_eq!(levenshtein("d", "abcd"), 3);
    assert_eq!(levenshtein("notate", "to ate"), 2);
    assert_eq!(levenshtein("to ate", "notate"), 2);
    assert_eq!(levenshtein("to be a", "not to"), 6);
    assert_eq!(levenshtein("not to", "to be a"), 6);
    assert_eq!(levenshtein("abccc", "accc"), 1);
}

#[test]
fn test_levenshtein_limit_one_empty() {
    assert_eq!(levenshtein_limit("abcdef", "", 3), 3);
    assert_eq!(levenshtein_limit("", "abcdef", 3), 3);
    assert_eq!(levenshtein_limit("abcdef", "", 8), 6);
    assert_eq!(levenshtein_limit("", "abcdef", 8), 6);
}

#[test]
fn test_levenshtein_limit() {
    // Most of this is tested via levenshtein()
    // just need to validate limits
    assert_eq!(levenshtein_limit("abcdef", "000000", 3), 3);
    assert_eq!(levenshtein_limit("ab", "0000", 3), 3);
}

#[test]
fn test_levenshtein_weight_insertion() {
    let weights = LevWeights::new(10, 1, 1);
    assert_eq!(levenshtein_weight("", "a", 100, &weights), 10);
    assert_eq!(levenshtein_weight("a", "", 100, &weights), 1);
    assert_eq!(levenshtein_weight("", "ab", 100, &weights), 20);
    assert_eq!(levenshtein_weight("ab", "", 100, &weights), 2);
    assert_eq!(levenshtein_weight("ab", "abcd", 100, &weights), 20);
    assert_eq!(levenshtein_weight("kitten", "sitting", 100, &weights), 12);
}

#[test]
fn test_levenshtein_weight_deletion() {
    let weights = LevWeights::new(1, 10, 1);
    assert_eq!(levenshtein_weight("", "a", 100, &weights), 1);
    assert_eq!(levenshtein_weight("a", "", 100, &weights), 10);
    assert_eq!(levenshtein_weight("", "ab", 100, &weights), 2);
    assert_eq!(levenshtein_weight("ab", "", 100, &weights), 20);
    assert_eq!(levenshtein_weight("kitten", "sitting", 100, &weights), 3);

    let weights = LevWeights::new(1, 10, 2);
    assert_eq!(levenshtein_weight("abc", "ac", 100, &weights), 10);
    assert_eq!(levenshtein_weight("abcd", "ac", 100, &weights), 20);
}

#[test]
fn test_levenshtein_weight_substitution() {
    // Note that when substitution cost is high, the algorithm will prefer
    // a deletion and insertion
    let weights = LevWeights::new(10, 10, 5);
    assert_eq!(levenshtein_weight("a", "b", 100, &weights), 5);
    let weights = LevWeights::new(10, 10, 2);
    assert_eq!(levenshtein_weight("abcd", "acc", 100, &weights), 12);
    let weights = LevWeights::new(4, 3, 2);
    assert_eq!(levenshtein_weight("kitten", "sitting", 100, &weights), 8);
}

#[test]
fn test_try_levenshtein() {
    assert_eq!(try_levenshtein("abcd", "ab", 2), Some(2));
    assert_eq!(try_levenshtein("abcde", "ab", 2), None);
    assert_eq!(try_levenshtein("abcdef", "", 3), None);
    assert_eq!(try_levenshtein("", "abcdef", 3), None);
    assert_eq!(try_levenshtein("", "abc", 3), Some(3));
    assert_eq!(try_levenshtein("abcdef", "", 8), Some(6));
    assert_eq!(try_levenshtein("", "abcdef", 8), Some(6));
    assert_eq!(try_levenshtein("", "abcdef", 8), Some(6));
    assert_eq!(try_levenshtein("ab", "0000", 3), None);
    assert_eq!(try_levenshtein("abcd", "wxya", 1), None);
    assert_eq!(try_levenshtein("angelica", "tortise", 1), None);
}
