# Stringmetrics

This is a Rust library for approximate string matching that implements simple
algorithms such has Hamming distance, Levenshtein distance, Jaccard similarity,
and more.

Here are some useful quick links:

- Crate info: <https://crates.io/crates/stringmetrics>
- Crate docs: <https://docs.rs/stringmetrics/>
- Python library page: <https://pypi.org/project/stringmetrics/>
- Crate source: <https://github.com/pluots/stringmetrics>


## Algorithms

The main purpose of this library is to provide a variety of string
metric functions. Included algorithms are:

- Levenshtein Distance
- Limited & Weighted Levenshtein Distance
- Jaccard Similarity
- Hamming Distance

See [the documentation](https://docs.rs/stringmetrics/) for full information.
Some examples are below:

```rs
// Basic levenshtein distance
use stringmetrics::levenshtein;

assert_eq!(levenshtein("kitten", "sitting"), 3);
```

```rs
// Levenshtein distance with a limit to save computation time
use stringmetrics::levenshtein_limit;

assert_eq!(levenshtein_limit("a very long string", "short!", 4), 4);
```

```rs
// Set custom weights
use stringmetrics::{levenshtein_weight, LevWeights};

// This struct holds insertion, deletion, and substitution costs
let weights = LevWeights::new(4, 3, 2);
assert_eq!(levenshtein_weight("kitten", "sitting", 100, &weights), 8);
```

```rs
// Basic hamming distance
use stringmetrics::hamming;

let a = "abcdefg";
let b = "aaadefa";
assert_eq!(hamming(a, b), Ok(3));
```

## Future Algorithms & Direction

Eventually, this library aims to add support for more algorithms. Intended work
includes:

1. Update levenshtein distance to have a more performant algorithm for short
   (<64 characters) and long (>100 characters) strings
2. Add the Damerau–Levenshtein distance
3. Add the Jaro–Winkler distance
4. Add the Tversky index
5. Add Cosine similarity
6. Add some useful tokenizers with examples

## License

See the LICENSE file for license information. The provided license does allow
for proprietary use and adaptation; that being said, I kindly suggest that if
you come up with an improvement, you submit a pull request and help us all out
:)
