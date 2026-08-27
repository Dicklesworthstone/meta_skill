use bincode_next::relative_ptr::ZeroArray;
use bincode_next::relative_ptr::ZeroString;
use criterion::Criterion;
use criterion::criterion_group;
use criterion::criterion_main;
use std::hint::black_box;

fn bench_zero_array_decode(c: &mut Criterion) {
    let mut buffer = [0u8; 12];
    let offset: i32 = 4;
    buffer[0..4].copy_from_slice(&offset.to_ne_bytes());

    let vals: [u32; 2] = [100, 200];
    let vals_bytes = unsafe { core::slice::from_raw_parts(vals.as_ptr() as *const u8, 8) };
    buffer[4..12].copy_from_slice(vals_bytes);

    c.bench_function("zero_array_get", |b| {
        b.iter(|| {
            let arr = unsafe { &*(buffer.as_ptr() as *const ZeroArray<u32, 2, 4>) };
            let resolved = arr.get(black_box(&buffer)).unwrap();
            black_box(resolved[0]);
        })
    });
}

fn bench_zero_string_decode(c: &mut Criterion) {
    let mut buffer = [0u8; 256];
    let offset: i32 = 4;
    buffer[0..4].copy_from_slice(&offset.to_ne_bytes());

    let text = b"hello this is a long enough string to decode zero copy";
    buffer[4..4 + text.len()].copy_from_slice(text);

    // We set CAP to text.len()
    // It's a const generic, so we can benchmark a hardcoded size.
    c.bench_function("zero_string_get", |b| {
        b.iter(|| {
            let z_str = unsafe { &*(buffer.as_ptr() as *const ZeroString<54>) };
            let resolved = z_str.get().unwrap();
            black_box(resolved.len());
        })
    });
}

criterion_group!(benches, bench_zero_array_decode, bench_zero_string_decode);
criterion_main!(benches);
