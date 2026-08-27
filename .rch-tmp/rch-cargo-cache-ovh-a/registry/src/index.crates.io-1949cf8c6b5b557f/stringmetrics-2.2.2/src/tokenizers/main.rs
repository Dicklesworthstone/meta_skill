// Characters to be removed on all occasions
const REMOVE_CHARS: [char; 11] = ['(', ')', ',', '\"', '.', ';', ':', '?', '!', '[', ']'];
// Remove these from the ends only
const END_REMOVE_CHARS: [char; 2] = ['-', '\''];

/// Split by whitespace an remove all punctuation
///
/// Standard spellcheck tokenizer
///
/// # Example
///
/// ```
/// use stringmetrics::tokenizers::split_whitespace_remove_punc;
///
/// let s = "Module's word collection! What do you think-";
/// let x: Vec<String> = split_whitespace_remove_punc(s).collect();
/// assert_eq!(x, vec!["Module's", "word", "collection", "What", "do", "you", "think"]);
/// ```
#[inline]
pub fn split_whitespace_remove_punc(s: &str) -> impl Iterator<Item = String> + '_ {
    // TODO: benchmark whether it is faster to replace first, or at the end
    // Note that we leave the "'" since it's useful for apostrophe
    s.split_whitespace()
        .map(|word| {
            word.trim_matches(&END_REMOVE_CHARS[..])
                .replace(&REMOVE_CHARS[..], "")
        })
        .filter(|word| !word.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split() {
        let s = " w1\tw2\nw3 w4 ";
        let x: Vec<String> = split_whitespace_remove_punc(s).collect();
        assert_eq!(x, vec!["w1", "w2", "w3", "w4"]);
    }

    #[test]
    fn test_trim_end() {
        let s = "w1- w2-w2 w3' w4's";
        let x: Vec<String> = split_whitespace_remove_punc(s).collect();
        assert_eq!(x, vec!["w1", "w2-w2", "w3", "w4's"]);
    }

    #[test]
    fn test_remove_chars() {
        let s = "w(w)w; abc! [def]";
        let x: Vec<String> = split_whitespace_remove_punc(s).collect();
        assert_eq!(x, vec!["www", "abc", "def"]);
    }
}
