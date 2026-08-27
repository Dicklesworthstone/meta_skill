use criterion::{black_box, criterion_group, criterion_main, Criterion};
use stringmetrics::{levenshtein, levenshtein_limit, levenshtein_weight, LevWeights};

const BENCH_WEIGHTS: bool = false;
const BENCH_LENDIFF: bool = true;

const STR_A: &str = "an orange cat";
const STR_B: &str = "an auburn bat";

const STR_A_LENDIFF: &str = "an orange cat";
const STR_B_LENDIFF: &str = "an auburn bat and a thing and maybe a thing or two";

const STR_A_LONG: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing \
    elit. Nunc non dictum elit. Curabitur vitae dapibus dolor, id consequat \
    velit. Proin a sem orci. Aenean at auctor enim, ut imperdiet metus. Donec \
    quis nisl congue, sagittis leo id, consequat est. Sed vitae pulvinar ante. \
    Donec efficitur tellus in nisi congue malesuada. Etiam hendrerit quis \
    metus a facilisis. Pellentesque lobortis rhoncus mauris, at aliquam diam \
    luctus at. In dapibus bibendum nisl. Pellentesque habitant morbi tristique \
    senectus et netus et malesuada fames ac turpis egestas. Sed commodo, urna \
    id consectetur condimentum, ex ex finibus ipsum, at mollis leo urna mattis \
    lacus. Sed egestas, nunc eu rhoncus posuere, velit libero pharetra massa, nec
    pharetra nisi nisi in est. Suspendisse a libero ipsum.";
const STR_B_LONG: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing \
    elit. Suspendisse pharetra dapibus eros ut malesuada. Nunc quis nunc odio. \
    Nunc elementum malesuada vulputate. Donec ac ex id purus sodales porttitor \
    nec et magna. Etiam tortor nunc, vulputate vel molestie et, viverra nec \
    orci. Curabitur sagittis nisl id elit congue, et lacinia erat maximus. In \
    non pharetra arcu. Praesent vitae viverra lorem. Phasellus sodales luctus \
    ullamcorper. Maecenas lorem eros, condimentum vel nisi id, sagittis pretium \
    quam. Ut non congue nulla. Donec laoreet viverra tellus, eu placerat sapien \
    malesuada id. sodales enim scelerisque ut. Vestibulum ut ex in sem placerat \
    ultrices. Mauris felis leo, fermentum id suscipit eget, sagittis non mauris\
    . Donec ut interdum risus non ipsum.";

pub fn bench_lev(c: &mut Criterion) {
    let weights = LevWeights::default();
    c.bench_function("Levenshtein", |b| {
        b.iter(|| levenshtein(black_box(STR_A), black_box(STR_B)))
    });
    c.bench_function("Levenshtein Limit (no hit)", |b| {
        b.iter(|| levenshtein_limit(black_box(STR_A), black_box(STR_B), black_box(40)))
    });
    if BENCH_LENDIFF {
        c.bench_function("Levenshtein Different Lengths", |b| {
            b.iter(|| levenshtein(black_box(STR_A_LENDIFF), black_box(STR_B_LENDIFF)))
        });
    }
    if BENCH_WEIGHTS {
        c.bench_function("Levenshtein Weights", |b| {
            b.iter(|| levenshtein_weight(black_box(STR_A), black_box(STR_B), 100, &weights))
        });
    }
}

pub fn bench_lev_empty(c: &mut Criterion) {
    if BENCH_LENDIFF {
        c.bench_function("Levenshtein Empty A", |b| {
            b.iter(|| levenshtein(black_box(""), black_box(STR_B)))
        });
    }

    c.bench_function("Levenshtein Empty B", |b| {
        b.iter(|| levenshtein(black_box(STR_A), black_box("")))
    });

    if BENCH_WEIGHTS {
        let weights = LevWeights::default();
        c.bench_function("Levenshtein Weights Empty B", |b| {
            b.iter(|| levenshtein_weight(black_box(STR_A_LONG), black_box(""), 100, &weights))
        });
    }
}

pub fn bench_lev_long(c: &mut Criterion) {
    c.bench_function("Levenshtein Long", |b| {
        b.iter(|| levenshtein(black_box(STR_A_LONG), black_box(STR_B_LONG)))
    });
    c.bench_function("Levenshtein Limit Long (hit limit)", |b| {
        b.iter(|| levenshtein_limit(black_box(STR_A_LONG), black_box(STR_B_LONG), 100))
    });
    if BENCH_LENDIFF {
        c.bench_function("Levenshtein Long Flip", |b| {
            b.iter(|| levenshtein(black_box(STR_B_LONG), black_box(STR_A_LONG)))
        });
    }
    if BENCH_WEIGHTS {
        let weights = LevWeights::default();
        c.bench_function("Levenshtein Weights Long (no hit)", |b| {
            b.iter(|| {
                levenshtein_weight(black_box(STR_A_LONG), black_box(STR_B_LONG), 5000, &weights)
            })
        });
    }
}

criterion_group!(bench, bench_lev, bench_lev_empty, bench_lev_long);
criterion_main!(bench);
