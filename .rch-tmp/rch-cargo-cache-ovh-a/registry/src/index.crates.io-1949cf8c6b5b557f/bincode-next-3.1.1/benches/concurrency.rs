#![cfg(feature = "async-fiber")]

use bincode_next::Decode;
use bincode_next::Encode;
use bincode_next::config;
use bincode_next::decode_async;
use criterion::BenchmarkId;
use criterion::Criterion;
use criterion::criterion_group;
use criterion::criterion_main;
use futures::TryStreamExt;
use futures_io::AsyncRead;
use serde::Deserialize;
use serde::Serialize;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;
use std::time::Duration;
use tokio::io::AsyncRead as TokioAsyncRead;
use tokio::task::JoinHandle;

#[derive(Encode, Decode, Serialize, Deserialize, PartialEq, Debug, Clone)]
struct BenchPayload {
    id: u64,
    data: String,
    metadata: Vec<u8>,
}

#[derive(Clone)]
struct YieldingReader {
    data: Vec<u8>,
    pos: usize,
    chunk_size: usize,
}

impl AsyncRead for YieldingReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        if self.pos >= self.data.len() || buf.is_empty() {
            return Poll::Ready(Ok(0));
        }

        let to_copy = std::cmp::min(self.chunk_size, self.data.len() - self.pos);
        let to_copy = std::cmp::min(to_copy, buf.len());

        buf[..to_copy].copy_from_slice(&self.data[self.pos..self.pos + to_copy]);
        self.pos += to_copy;

        cx.waker().wake_by_ref();
        Poll::Ready(Ok(to_copy))
    }
}

impl TokioAsyncRead for YieldingReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if self.pos >= self.data.len() || buf.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }

        let to_copy = std::cmp::min(self.chunk_size, self.data.len() - self.pos);
        let to_copy = std::cmp::min(to_copy, buf.remaining());

        buf.put_slice(&self.data[self.pos..self.pos + to_copy]);
        self.pos += to_copy;

        cx.waker().wake_by_ref();
        Poll::Ready(Ok(()))
    }
}

async fn serialize_for_async_bincode(payload: &BenchPayload) -> Vec<u8> {
    use async_bincode::tokio::AsyncBincodeWriter;
    use futures::SinkExt;

    let mut buffer = Vec::new();
    let mut writer = AsyncBincodeWriter::from(&mut buffer).for_async();
    writer.send(payload.clone()).await.unwrap();
    buffer
}

pub fn bench_concurrency(c: &mut Criterion) {
    let payload = BenchPayload {
        id: 999999999,
        data: "Benchmarking async fiber performance vs standard state machine".to_string(),
        metadata: vec![1, 2, 3, 4, 5, 255, 128, 64, 32],
    };

    let encoded_next = bincode_next::encode_to_vec(&payload, config::standard()).unwrap();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .unwrap();

    let concurrency_levels = [1_000, 10_000, 50_000, 500_000, 5_000_000, 50_000_000];
    let mut group = c.benchmark_group("High Concurrency");
    let encoded_for_async = rt.block_on(serialize_for_async_bincode(&payload));
    group.sample_size(10); // Reduce samples for heavy concurrency tests

    for concurrency in concurrency_levels.iter() {
        group
            .bench_with_input(
                BenchmarkId::new("bincode-next (fiber)", concurrency),
                concurrency,
                |b, &tasks| {
                    b.to_async(&rt).iter_custom(|iters| {
                        let encoded_next = encoded_next.clone();
                        async move {
                            let start = std::time::Instant::now();
                            for _ in 0..iters {
                                let mut handles: Vec<JoinHandle<()>> = Vec::with_capacity(tasks);
                                for _ in 0..tasks {
                                    let enc = encoded_next.clone();
                                    handles.push(tokio::spawn(async move {
                                        let reader = YieldingReader {
                                            data: enc,
                                            pos: 0,
                                            chunk_size: 16, // Yield every 16 bytes
                                        };
                                        let _decoded: BenchPayload =
                                            decode_async(config::standard(), reader).await.unwrap();
                                    }));
                                }
                                futures::future::join_all(handles).await;
                            }
                            start.elapsed()
                        }
                    })
                },
            )
            .measurement_time(Duration::from_secs(600));

        group
            .bench_with_input(
                BenchmarkId::new("async-bincode (state machine)", concurrency),
                concurrency,
                |b, &tasks| {
                    let data_for_bench = encoded_for_async.clone();

                    b.to_async(&rt).iter_custom(|iters| {
                        let data_for_iters = data_for_bench.clone();

                        async move {
                            let start = std::time::Instant::now();
                            for _ in 0..iters {
                                let mut handles = Vec::with_capacity(tasks);
                                for _ in 0..tasks {
                                    let enc = data_for_iters.clone();
                                    handles.push(tokio::spawn(async move {
                                        let reader = YieldingReader {
                                            data: enc,
                                            pos: 0,
                                            chunk_size: 16,
                                        };
                                        use futures::StreamExt;

                                        let mut stream =
                                            async_bincode::tokio::AsyncBincodeReader::<
                                                _,
                                                BenchPayload,
                                            >::from(
                                                reader
                                            )
                                            .into_stream();

                                        let _decoded = stream.next().await.unwrap().unwrap();
                                    }));
                                }
                                futures::future::join_all(handles).await;
                            }
                            start.elapsed()
                        }
                    })
                },
            )
            .measurement_time(Duration::from_secs(600));
    }

    group.finish();
}

criterion_group!(benches, bench_concurrency);
criterion_main!(benches);
