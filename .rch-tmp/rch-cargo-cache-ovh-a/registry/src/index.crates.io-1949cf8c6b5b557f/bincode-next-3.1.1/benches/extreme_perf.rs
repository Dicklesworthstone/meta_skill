extern crate bincode_next as bincode;
use bincode::config;
use criterion::Criterion;
use criterion::criterion_group;
use criterion::criterion_main;
use rand::distr::Distribution;
use std::hint::black_box;
use std::time::Duration;

fn bench_vec_u64_small_varint(c: &mut Criterion) {
    let mut rng = rand::rng();
    let dist = rand::distr::Uniform::new(0, 250u64).unwrap();
    let input: Vec<u64> = (0..10_000).map(|_| dist.sample(&mut rng)).collect();
    let config = config::standard();
    let bytes = bincode::encode_to_vec(&input, config).unwrap();

    // v2 original (rc3)
    let bytes_v2 = bincode_v2::encode_to_vec(&input, bincode_v2::config::standard()).unwrap();

    let mut group = c.benchmark_group("vec_u64_small_varint_decode");

    group
        .warm_up_time(Duration::from_secs(20))
        .measurement_time(Duration::from_secs(40))
        .sample_size(1000);

    group.bench_function("bincode-next (current)", |b| {
        b.iter(|| {
            let res: (Vec<u64>, usize) =
                bincode::decode_from_slice(black_box(&bytes), config).unwrap();
            black_box(res);
        })
    });

    group.bench_function("bincode-v2 (original)", |b| {
        b.iter(|| {
            let res: (Vec<u64>, usize) =
                bincode_v2::decode_from_slice(black_box(&bytes_v2), bincode_v2::config::standard())
                    .unwrap();
            black_box(res);
        })
    });

    group.finish();
}

fn bench_vec_u64_large_varint(c: &mut Criterion) {
    let mut rng = rand::rng();
    let dist = rand::distr::Uniform::new(251, u64::MAX).unwrap();
    let input: Vec<u64> = (0..10_000).map(|_| dist.sample(&mut rng)).collect();
    let config = config::standard();
    let bytes = bincode::encode_to_vec(&input, config).unwrap();

    let bytes_v2 = bincode_v2::encode_to_vec(&input, bincode_v2::config::standard()).unwrap();

    let mut group = c.benchmark_group("vec_u64_large_varint_decode");

    group
        .warm_up_time(Duration::from_secs(20))
        .measurement_time(Duration::from_secs(40))
        .sample_size(1000);

    group.bench_function("bincode-next (current)", |b| {
        b.iter(|| {
            let res: (Vec<u64>, usize) =
                bincode::decode_from_slice(black_box(&bytes), config).unwrap();
            black_box(res);
        })
    });

    group.bench_function("bincode-v2 (original)", |b| {
        b.iter(|| {
            let res: (Vec<u64>, usize) =
                bincode_v2::decode_from_slice(black_box(&bytes_v2), bincode_v2::config::standard())
                    .unwrap();
            black_box(res);
        })
    });

    group.finish();
}

fn bench_vec_u64_fixint_native(c: &mut Criterion) {
    let mut rng = rand::rng();
    let dist = rand::distr::StandardUniform;
    let input: Vec<u64> = (0..10_000).map(|_| dist.sample(&mut rng)).collect();
    let config = config::standard().with_fixed_int_encoding();
    let bytes = bincode::encode_to_vec(&input, config).unwrap();

    let bytes_v2 = bincode_v2::encode_to_vec(
        &input,
        bincode_v2::config::standard().with_fixed_int_encoding(),
    )
    .unwrap();

    let mut group = c.benchmark_group("vec_u64_fixint_native_decode");

    group
        .warm_up_time(Duration::from_secs(20))
        .measurement_time(Duration::from_secs(40))
        .sample_size(1000);

    group.bench_function("bincode-next (current)", |b| {
        b.iter(|| {
            let res: (Vec<u64>, usize) =
                bincode::decode_from_slice(black_box(&bytes), config).unwrap();
            black_box(res);
        })
    });

    group.bench_function("bincode-v2 (original)", |b| {
        b.iter(|| {
            let res: (Vec<u64>, usize) = bincode_v2::decode_from_slice(
                black_box(&bytes_v2),
                bincode_v2::config::standard().with_fixed_int_encoding(),
            )
            .unwrap();
            black_box(res);
        })
    });

    group.bench_function("bincode-v1", |b| {
        let bytes_v1 = bincode_1::serialize(&input).unwrap();
        b.iter(|| {
            let res: Vec<u64> = bincode_1::deserialize(black_box(&bytes_v1)).unwrap();
            black_box(res);
        })
    });

    group.finish();
}

fn bench_vec_u8_bulk(c: &mut Criterion) {
    let mut rng = rand::rng();
    let dist = rand::distr::StandardUniform;
    let input: Vec<u8> = (0..10_000).map(|_| dist.sample(&mut rng)).collect();
    let config = config::standard();
    let bytes = bincode::encode_to_vec(&input, config).unwrap();

    let bytes_v2 = bincode_v2::encode_to_vec(&input, bincode_v2::config::standard()).unwrap();

    let mut group = c.benchmark_group("vec_u8_bulk_decode");

    group
        .warm_up_time(Duration::from_secs(20))
        .measurement_time(Duration::from_secs(40))
        .sample_size(1000);

    group.bench_function("bincode-next (current)", |b| {
        b.iter(|| {
            let res: (Vec<u8>, usize) =
                bincode::decode_from_slice(black_box(&bytes), config).unwrap();
            black_box(res);
        })
    });

    group.bench_function("bincode-v2 (original)", |b| {
        b.iter(|| {
            let res: (Vec<u8>, usize) =
                bincode_v2::decode_from_slice(black_box(&bytes_v2), bincode_v2::config::standard())
                    .unwrap();
            black_box(res);
        })
    });

    group.bench_function("bincode-v1", |b| {
        let bytes_v1 = bincode_1::serialize(&input).unwrap();
        b.iter(|| {
            let res: Vec<u8> = bincode_1::deserialize(black_box(&bytes_v1)).unwrap();
            black_box(res);
        })
    });

    group.finish();
}

criterion_group!(
    extreme_perf,
    bench_vec_u64_small_varint,
    bench_vec_u64_large_varint,
    bench_vec_u64_fixint_native,
    bench_vec_u8_bulk,
);
criterion_main!(extreme_perf);
