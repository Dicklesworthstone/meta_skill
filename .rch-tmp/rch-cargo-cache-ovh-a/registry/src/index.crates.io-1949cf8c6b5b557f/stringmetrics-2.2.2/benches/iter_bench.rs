use criterion::{black_box, criterion_group, criterion_main, Criterion};

const SOMESTR: &str = "This is a new string with some amount of length";

pub fn bench_str(c: &mut Criterion) {
    let bb_str = SOMESTR;
    let bb_string = black_box(String::from(SOMESTR));

    c.bench_function("Iterate string chars", |b| {
        b.iter(|| black_box(bb_string.chars().count()))
    });
    c.bench_function("Iterate string bytes", |b| {
        b.iter(|| black_box(bb_string.bytes().count()))
    });
    c.bench_function("Iterate string asref chars", |b| {
        b.iter(|| AsRef::<str>::as_ref(black_box(&bb_string)).chars().count())
    });
    c.bench_function("Iterate str chars", |b| {
        b.iter(|| black_box(bb_str.chars().count()))
    });
    c.bench_function("Iterate str bytes", |b| {
        b.iter(|| black_box(bb_str.bytes().count()))
    });
    c.bench_function("Iterate str asref chars", |b| {
        b.iter(|| AsRef::<str>::as_ref(black_box(&bb_str)).chars().count())
    });
}

/// Test iterating a vector collection of chars vs. reconstructing the iterator
pub fn bench_collected(c: &mut Criterion) {
    let bb_str = SOMESTR;
    let vv: Vec<char> = bb_str.chars().collect();

    // We want a fair test so we loop within the benchmark here
    // Constructing the vector takes time, we just want to see if it iterates
    // faster
    c.bench_function("Collect chars", |b| {
        b.iter(|| {
            black_box(bb_str.chars().collect::<Vec<char>>());
        })
    });
    c.bench_function("Iterate collected chars", |b| {
        b.iter(|| {
            black_box(vv.iter().count());
        })
    });
    c.bench_function("Iterate collected chars deref", |b| {
        b.iter(|| {
            black_box(vv.iter().map(|x| *x).count());
        })
    });
    c.bench_function("Iterate uncollected chars", |b| {
        b.iter(|| {
            black_box(bb_str.chars().count());
        })
    });
    c.bench_function("Iterate uncollected bytes", |b| {
        b.iter(|| {
            black_box(bb_str.bytes().count());
        })
    });
}

// pub fn bench_iter_items(c: &mut Criterion) {
//     let bb_str = SOMESTR;
//     let vv: Vec<char> = bb_str.chars().collect();

//     // We want a fair test so we loop within the benchmark here
//     // Constructing the vector takes time, we just want to see if it iterates
//     // faster
//     c.bench_function("Collect chars", |b| {
//         b.iter(|| {
//             black_box(bb_str.chars().collect::<Vec<char>>());
//         })
//     });
//     c.bench_function("Iterate collected chars", |b| {
//         b.iter(|| {
//             black_box(vv.iter().count());
//         })
//     });
//     c.bench_function("Iterate collected chars deref", |b| {
//         b.iter(|| {
//             black_box(vv.iter().map(|x| *x).count());
//         })
//     });
//     c.bench_function("Iterate uncollected chars", |b| {
//         b.iter(|| {
//             black_box(bb_str.chars().count());
//         })
//     });
//     c.bench_function("Iterate uncollected bytes", |b| {
//         b.iter(|| {
//             black_box(bb_str.bytes().count());
//         })
//     });
// }

// #[inline]

criterion_group!(
    iter_bench,
    // bench_str,
    bench_collected
);
criterion_main!(iter_bench);
