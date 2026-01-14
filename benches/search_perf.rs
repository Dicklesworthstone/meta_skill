use criterion::{criterion_group, criterion_main, Criterion};

fn search_benchmark(_c: &mut Criterion) {
    // TODO: Implement search benchmarks
    // - BM25 search performance
    // - Hash embedding performance
    // - RRF fusion performance
}

criterion_group!(benches, search_benchmark);
criterion_main!(benches);
