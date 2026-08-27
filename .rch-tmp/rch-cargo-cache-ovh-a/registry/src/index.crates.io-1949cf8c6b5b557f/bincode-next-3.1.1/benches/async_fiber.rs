use bincode_next::config;
#[cfg(feature = "async-fiber")]
use bincode_next::decode_async;
use bincode_next::decode_from_slice;
use bincode_next::encode_to_vec;
use criterion::BenchmarkId;
use criterion::Criterion;
use criterion::criterion_group;
use criterion::criterion_main;

#[derive(bincode_next::Encode, bincode_next::Decode, PartialEq, Debug, Clone)]
struct BenchPayload {
    a: u64,
    b: String,
    c: Vec<u8>,
}

fn generate_payload() -> BenchPayload {
    BenchPayload {
        a: 1234567890123456789,
        b: "This is a reasonably long string used for benchmarking decoding performance"
            .to_string(),
        c: vec![42; 1024],
    }
}

pub fn bench_async_fiber(c: &mut Criterion) {
    let payload = generate_payload();
    let encoded = encode_to_vec(&payload, config::standard()).unwrap();

    let mut group = c.benchmark_group("async_fiber");

    group.bench_with_input(BenchmarkId::new("sync_slice", 0), &encoded, |b, encoded| {
        b.iter(|| {
            let (decoded, _len): (BenchPayload, usize) =
                decode_from_slice(encoded, config::standard()).unwrap();
            std::hint::black_box(decoded);
        })
    });

    #[cfg(feature = "async-fiber")]
    {
        fn dummy_waker() -> std::task::Waker {
            static VTABLE: std::task::RawWakerVTable = std::task::RawWakerVTable::new(
                |_| std::task::RawWaker::new(std::ptr::null(), &VTABLE),
                |_| {},
                |_| {},
                |_| {},
            );
            unsafe {
                std::task::Waker::from_raw(std::task::RawWaker::new(std::ptr::null(), &VTABLE))
            }
        }

        group.bench_with_input(
            BenchmarkId::new("async_fiber_raw", 0),
            &encoded,
            |b, encoded| {
                let waker = dummy_waker();
                let mut cx = std::task::Context::from_waker(&waker);
                b.iter(|| {
                    let reader = encoded.as_slice();
                    let mut fut = Box::pin(decode_async(config::standard(), reader));
                    match fut.as_mut().poll(&mut cx) {
                        | std::task::Poll::Ready(Ok(decoded)) => std::hint::black_box(decoded),
                        | _ => std::hint::black_box(()),
                    }
                })
            },
        );

        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();

        group.bench_with_input(
            BenchmarkId::new("async_fiber_chunked", 0),
            &encoded,
            |b, encoded| {
                b.to_async(&rt).iter(|| {
                    async {
                        let reader = encoded.as_slice();
                        let decoded: BenchPayload =
                            decode_async(config::standard(), reader).await.unwrap();
                        std::hint::black_box(decoded);
                    }
                })
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_async_fiber);
criterion_main!(benches);
