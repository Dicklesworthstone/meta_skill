// Need to feature gate since we benchmark internal modules
#![cfg(feature = "bench")]

// use std::cmp::min;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use stringmetrics::iter::find_eq_end_items_bench as find_eq_end_items;

#[derive(Debug, PartialEq)]
pub struct IterPairInfo {
    a_len: usize,
    b_len: usize,
    start_same: usize,
    end_same: usize,
}

impl IterPairInfo {
    pub fn new(a_len: usize, b_len: usize, start_same: usize, end_same: usize) -> Self {
        IterPairInfo {
            a_len,
            b_len,
            start_same,
            end_same,
        }
    }

    pub fn expand(&self) -> (usize, usize, usize, usize) {
        (self.a_len, self.b_len, self.start_same, self.end_same)
    }
}

// // More rusty way to solve the problem
// // Seems to be faster for larger strings, can't really figure out why
// #[inline]
// fn baseline(a: &str, b: &str) -> IterPairInfo {
//     let a_len = a.chars().count();
//     let b_len = b.chars().count();
//     let mut start_same = 0usize;
//     let mut end_same = 0usize;

//     for (a_char, b_char) in a.chars().zip(b.chars()) {
//         if a_char == b_char {
//             start_same += 1
//         } else {
//             break;
//         }
//     }

//     let end_max_iters = min(a_len, b_len) - start_same;

//     for (i, (a_char, b_char)) in a.chars().rev().zip(b.chars().rev()).enumerate() {
//         if i >= end_max_iters {
//             break;
//         }
//         if a_char == b_char {
//             end_same += 1
//         } else {
//             break;
//         }
//     }

//     IterPairInfo::new(a_len, b_len, start_same, end_same)
// }

pub fn bench_iter_match(c: &mut Criterion) {
    c.bench_function("Iterate single-pass implementation", |b| {
        b.iter(|| {
            find_eq_end_items(
                black_box("aaaaaaa0000000".chars()),
                black_box("aaaaaaa".chars()),
            )
        })
    });
    // c.bench_function("Iterate double-pass implementation", |b| {
    //     b.iter(|| baseline(black_box("aaaaaaa0000000"), black_box("aaaaaaa")))
    // });
    c.bench_function("Iterate single-pass implementation empty", |b| {
        b.iter(|| find_eq_end_items(black_box("".chars()), black_box("".chars())))
    });
    // c.bench_function("Iterate double-pass implementation empty", |b| {
    //     b.iter(|| baseline(black_box(""), black_box("")))
    // });
    c.bench_function("Iterate single-pass implementation left", |b| {
        b.iter(|| find_eq_end_items(black_box("aaaaaaa0000000".chars()), black_box("".chars())))
    });
    // c.bench_function("Iterate double-pass implementation left", |b| {
    //     b.iter(|| baseline(black_box("aaaaaaa0000000"), black_box("")))
    // });
    c.bench_function("Iterate single-pass implementation right", |b| {
        b.iter(|| {
            find_eq_end_items(
                black_box("aaaaaaa0000000".chars()),
                black_box("aaaaaaa0000000".chars()),
            )
        })
    });
    // c.bench_function("Iterate double-pass implementation right", |b| {
    //     b.iter(|| baseline(black_box("aaaaaaa0000000"), black_box("aaaaaaa0000000")))
    // });
    c.bench_function("Iterate single-pass implementation dbl", |b| {
        b.iter(|| {
            find_eq_end_items(
                black_box("aaaaaaa0000000bbb".chars()),
                black_box("aaabbb".chars()),
            )
        })
    });
    // c.bench_function("Iterate double-pass implementation dbl", |b| {
    //     b.iter(|| baseline(black_box("aaaaaaa0000000bbb"), black_box("aaabbb")))
    // });
    c.bench_function("Iterate single-pass implementation long", |b| {
        b.iter(|| {
            find_eq_end_items(
                "aaaaaaaaaaaaaaaaaaaa0000000bbbbbbbbbbbbbbb".chars(),
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbb"
                    .chars(),
            )
        })
    });
    // c.bench_function("Iterate double-pass implementation long", |b| {
    //     b.iter(|| {
    //         baseline(
    //             "aaaaaaaaaaaaaaaaaaaa0000000bbbbbbbbbbbbbbb",
    //             "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbb",
    //         )
    //     })
    // });
    c.bench_function("Iterate single-pass implementation xlong", |b| {
        b.iter(|| {
            find_eq_end_items(
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\
                aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\
                00000000000000000000000000000000000000000000000000\
                00000000000000000000000000000000000000000000000000\
                bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\
                bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                    .chars(),
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\
                aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\
                aa000000000000000000000000000000000000000000000000\
                00000000000000000000000000000000000000000000000000\
                cccccccccccccccccccccccccccccccccccccccccccccccccc\
                bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\
                bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                    .chars(),
            )
        })
    });
    // c.bench_function("Iterate double-pass implementation xlong", |b| {
    //     b.iter(|| {
    //         baseline(
    //             "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\
    //             aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\
    //             00000000000000000000000000000000000000000000000000\
    //             00000000000000000000000000000000000000000000000000\
    //             bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\
    //             bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    //             "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\
    //             aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\
    //             aa000000000000000000000000000000000000000000000000\
    //             00000000000000000000000000000000000000000000000000\
    //             cccccccccccccccccccccccccccccccccccccccccccccccccc\
    //             bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\
    //             bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    //         )
    //     })
    // });
    c.bench_function("Iterate single-pass implementation diff short", |b| {
        b.iter(|| find_eq_end_items("aaaaaa".chars(), "bbbb".chars()))
    });
    // c.bench_function("Iterate double-pass implementation diff short", |b| {
    //     b.iter(|| baseline("aaaaaa", "bbbb"))
    // });
    c.bench_function("Iterate single-pass implementation diff", |b| {
        b.iter(|| find_eq_end_items("aaaaaaaaaaaaaa".chars(), "bbbbbbbbbbbbbb".chars()))
    });
    // c.bench_function("Iterate double-pass implementation diff", |b| {
    //     b.iter(|| baseline("aaaaaaaaaaaaaa", "bbbbbbbbbbbbbb"))
    // });
}

criterion_group!(
    iter_match_bench,
    // bench_str,
    bench_iter_match
);
criterion_main!(iter_match_bench);
