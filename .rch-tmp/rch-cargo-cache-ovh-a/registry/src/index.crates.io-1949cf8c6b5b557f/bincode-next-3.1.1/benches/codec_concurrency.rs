#![cfg(feature = "async-fiber")]
// Concurrent Codec Throughput Comparison
//
// Compares five scheduler/codec combinations under realistic concurrent workloads:
//   1. bincode-next sync  + dtact v0.2  (lightweight stackful coroutines, no async I/O)
//   2. bincode-next fiber + tokio       (UFA async decode via decode_async)
//   3. postcard v1.1      + tokio       (sync codec, framed by AsyncReadExt / AsyncWriteExt)
//   4. bincode v1         + tokio       (sync codec, framed by AsyncReadExt / AsyncWriteExt)
//   5. bincode v2         + tokio       (sync codec, framed by AsyncReadExt / AsyncWriteExt)
//
// Task-stream variants:
//   - encode_stream   : every task encodes a NetworkFrame
//   - decode_stream   : every task decodes a pre-encoded NetworkFrame
//   - roundtrip_stream: even tasks encode, odd tasks decode (mixed concurrent workload)
//
// Concurrency levels: 1 000, 10 000, 100 000, 1 000 000

extern crate bincode_next as bincode;

use bincode::config;
use bincode::decode_async;
use criterion::BenchmarkId;
use criterion::Criterion;
use criterion::criterion_group;
use criterion::criterion_main;
use futures::future::join_all;
use serde::Deserialize;
use serde::Serialize;
use std::hint::black_box;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Context;
use std::task::Poll;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::task::JoinHandle;

// ---------------------------------------------------------------------------
// Data model: NetworkFrame
// Moderately complex – rich enough to exercise codec paths without dominating
// the measurement with memory allocation (each encoded frame ~250 bytes).
// ---------------------------------------------------------------------------

#[derive(bincode::Encode, bincode::Decode, Serialize, Deserialize, PartialEq, Debug, Clone)]
struct NetworkFrame {
    frame_id: u64,
    src_addr: u32,
    dst_addr: u32,
    src_port: u16,
    dst_port: u16,
    protocol: FrameProtocol,
    payload: Vec<u8>,
    headers: Vec<FrameHeader>,
    flags: u32,
    sequence: u32,
    timestamp_us: u64,
    priority: FramePriority,
    ttl: u8,
    fragment_offset: u16,
}

#[derive(bincode::Encode, bincode::Decode, Serialize, Deserialize, PartialEq, Debug, Clone)]
enum FrameProtocol {
    Tcp {
        window: u32,
        checksum: u16,
        ack_seq: u64,
    },
    Udp {
        checksum: u16,
    },
    Quic {
        stream_id: u64,
        offset: u64,
        fin: bool,
    },
    Icmp {
        code: u8,
        identifier: u16,
        message: String,
    },
    Raw {
        protocol_id: u8,
    },
}

#[derive(bincode::Encode, bincode::Decode, Serialize, Deserialize, PartialEq, Debug, Clone)]
struct FrameHeader {
    name: String,
    value: String,
}

#[derive(bincode::Encode, bincode::Decode, Serialize, Deserialize, PartialEq, Debug, Clone)]
enum FramePriority {
    Low,
    Normal,
    High,
    Critical(String),
}

fn make_frame() -> NetworkFrame {
    NetworkFrame {
        frame_id: 0xdeadbeef_cafebabe,
        src_addr: 0x0a000101,
        dst_addr: 0xc0a80101,
        src_port: 54321,
        dst_port: 443,
        protocol: FrameProtocol::Tcp {
            window: 65535,
            checksum: 0xabcd,
            ack_seq: 0x0000_1234_5678_9abc,
        },
        payload: vec![0xca; 64],
        headers: vec![
            FrameHeader {
                name: "Content-Type".into(),
                value: "application/octet-stream".into(),
            },
            FrameHeader {
                name: "X-Request-ID".into(),
                value: "550e8400-e29b-41d4-a716".into(),
            },
            FrameHeader {
                name: "X-Session".into(),
                value: "bincode-bench-v3".into(),
            },
            FrameHeader {
                name: "X-Priority".into(),
                value: "high".into(),
            },
            FrameHeader {
                name: "Authorization".into(),
                value: "Bearer tok_bench_placeholder".into(),
            },
        ],
        flags: 0b0001_0010,
        sequence: 0x0042_0000,
        timestamp_us: 1_700_000_000_000_000,
        priority: FramePriority::High,
        ttl: 64,
        fragment_offset: 0,
    }
}

// ---------------------------------------------------------------------------
// Dtact runtime initialisation (idempotent; safe to call multiple times).
// ---------------------------------------------------------------------------

fn init_dtact() {
    let _ = dtact::GLOBAL_RUNTIME.get_or_init(|| {
        let scheduler =
            dtact::dta_scheduler::DtaScheduler::new(4, dtact::dta_scheduler::TopologyMode::P2PMesh);
        let pool = dtact::memory_management::ContextPool::new(
            8192,
            64 * 1024,
            dtact::memory_management::SafetyLevel::Safety0,
            0,
        )
        .expect("dtact ContextPool init failed");
        dtact::Runtime {
            scheduler,
            pool,
            started: core::sync::atomic::AtomicBool::new(false),
            shutdown: core::sync::atomic::AtomicBool::new(false),
        }
    });
    if let Some(rt) = dtact::GLOBAL_RUNTIME.get() {
        rt.start();
    }
}

// ---------------------------------------------------------------------------
// Async I/O helpers
// ---------------------------------------------------------------------------

/// Yielding reader that implements both `futures_io::AsyncRead` (for decode_async)
/// and `tokio::io::AsyncRead` (for AsyncReadExt on the tokio side).
/// Delivers `chunk_size` bytes per poll to simulate a chunked network stream.
#[derive(Clone)]
struct YieldingReader {
    data: Arc<Vec<u8>>,
    pos: usize,
    chunk_size: usize,
}

impl futures_io::AsyncRead for YieldingReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        if self.pos >= self.data.len() || buf.is_empty() {
            return Poll::Ready(Ok(0));
        }
        let n = (self.chunk_size)
            .min(self.data.len() - self.pos)
            .min(buf.len());
        buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
        self.pos += n;
        Poll::Ready(Ok(n))
    }
}

impl tokio::io::AsyncRead for YieldingReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if self.pos >= self.data.len() || buf.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        let n = (self.chunk_size)
            .min(self.data.len() - self.pos)
            .min(buf.remaining());
        buf.put_slice(&self.data[self.pos..self.pos + n]);
        self.pos += n;
        Poll::Ready(Ok(()))
    }
}

fn make_tokio_rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .unwrap()
}

const CONCURRENCY_LEVELS: &[usize] = &[1_000, 10_000, 100_000, 1_000_000];

// ---------------------------------------------------------------------------
// Benchmark 1: encode_stream
// Every task encodes a NetworkFrame; for tokio variants the encoded bytes are
// then async-written to tokio::io::sink() to include I/O scheduling overhead.
// ---------------------------------------------------------------------------

fn bench_encode_stream(c: &mut Criterion) {
    init_dtact();
    let rt = make_tokio_rt();

    let frame = Arc::new(make_frame());

    // Pre-compute encoded bytes for all formats so task closures are symmetric.
    let cfg = config::standard();
    let cfg_fixed = config::legacy();
    let enc_varint = Arc::new(bincode::encode_to_vec(frame.as_ref(), cfg).unwrap());
    let enc_fixed = Arc::new(bincode::encode_to_vec(frame.as_ref(), cfg_fixed).unwrap());
    let enc_postcard = Arc::new(postcard::to_stdvec(frame.as_ref()).unwrap());
    let enc_v1 = Arc::new(bincode_1::serialize(frame.as_ref()).unwrap());
    let enc_v2 = Arc::new(
        bincode_v2::serde::encode_to_vec(frame.as_ref(), bincode_v2::config::standard()).unwrap(),
    );

    let mut group = c.benchmark_group("encode_stream");
    group.sample_size(10);

    for &n in CONCURRENCY_LEVELS {
        // --- 1. bincode-next sync + dtact ---
        let frame_d = Arc::clone(&frame);
        group
            .bench_with_input(
                BenchmarkId::new("bincode-next/dtact", n),
                &n,
                |b, &tasks| {
                    b.iter_custom(|iters| {
                        let frame_d = Arc::clone(&frame_d);
                        let start = std::time::Instant::now();
                        for _ in 0..iters {
                            let h =
                                dtact::SpawnBuilder::<dtact::CrossThreadNoFloat>::new().spawn({
                                    let frame_d = Arc::clone(&frame_d);
                                    async move {
                                        let mut handles = Vec::with_capacity(tasks);
                                        for _ in 0..tasks {
                                            let f = Arc::clone(&frame_d);
                                            handles.push(
                                        dtact::SpawnBuilder::<dtact::CrossThreadNoFloat>::new()
                                            .spawn(async move {
                                                let bytes = bincode::encode_to_vec(
                                                    f.as_ref(),
                                                    config::standard(),
                                                )
                                                .unwrap();
                                                black_box(bytes);
                                            }),
                                    );
                                        }
                                        for h in handles {
                                            dtact::dtact_await(h);
                                        }
                                    }
                                });
                            dtact::dtact_await(h);
                        }
                        start.elapsed()
                    })
                },
            )
            .measurement_time(Duration::from_secs(600));

        // --- 2. bincode-next fiber + tokio ---
        {
            let frame_t = Arc::clone(&frame);
            let enc = Arc::clone(&enc_varint);
            group
                .bench_with_input(
                    BenchmarkId::new("bincode-next/fiber+tokio", n),
                    &n,
                    |b, &tasks| {
                        b.to_async(&rt).iter_custom(|iters| {
                            let frame_t = Arc::clone(&frame_t);
                            let _enc = Arc::clone(&enc);
                            async move {
                                let start = std::time::Instant::now();
                                for _ in 0..iters {
                                    let mut handles: Vec<JoinHandle<()>> =
                                        Vec::with_capacity(tasks);
                                    for _ in 0..tasks {
                                        let f = Arc::clone(&frame_t);
                                        handles.push(tokio::spawn(async move {
                                            let bytes = bincode::encode_to_vec(
                                                f.as_ref(),
                                                config::standard(),
                                            )
                                            .unwrap();
                                            // Write to sink to include async I/O scheduling overhead.
                                            tokio::io::sink().write_all(&bytes).await.unwrap();
                                            black_box(bytes.len());
                                        }));
                                    }
                                    join_all(handles).await;
                                }
                                start.elapsed()
                            }
                        })
                    },
                )
                .measurement_time(Duration::from_secs(600));
        }

        // --- 3. postcard + tokio ---
        {
            let frame_t = Arc::clone(&frame);
            group
                .bench_with_input(BenchmarkId::new("postcard/tokio", n), &n, |b, &tasks| {
                    b.to_async(&rt).iter_custom(|iters| {
                        let frame_t = Arc::clone(&frame_t);
                        async move {
                            let start = std::time::Instant::now();
                            for _ in 0..iters {
                                let mut handles: Vec<JoinHandle<()>> = Vec::with_capacity(tasks);
                                for _ in 0..tasks {
                                    let f = Arc::clone(&frame_t);
                                    handles.push(tokio::spawn(async move {
                                        let bytes = postcard::to_stdvec(f.as_ref()).unwrap();
                                        tokio::io::sink().write_all(&bytes).await.unwrap();
                                        black_box(bytes.len());
                                    }));
                                }
                                join_all(handles).await;
                            }
                            start.elapsed()
                        }
                    })
                })
                .measurement_time(Duration::from_secs(600));
        }

        // --- 4. bincode v1 + tokio ---
        {
            let frame_t = Arc::clone(&frame);
            group
                .bench_with_input(BenchmarkId::new("bincode-v1/tokio", n), &n, |b, &tasks| {
                    b.to_async(&rt).iter_custom(|iters| {
                        let frame_t = Arc::clone(&frame_t);
                        async move {
                            let start = std::time::Instant::now();
                            for _ in 0..iters {
                                let mut handles: Vec<JoinHandle<()>> = Vec::with_capacity(tasks);
                                for _ in 0..tasks {
                                    let f = Arc::clone(&frame_t);
                                    handles.push(tokio::spawn(async move {
                                        let bytes = bincode_1::serialize(f.as_ref()).unwrap();
                                        tokio::io::sink().write_all(&bytes).await.unwrap();
                                        black_box(bytes.len());
                                    }));
                                }
                                join_all(handles).await;
                            }
                            start.elapsed()
                        }
                    })
                })
                .measurement_time(Duration::from_secs(600));
        }

        // --- 5. bincode v2 + tokio ---
        {
            let frame_t = Arc::clone(&frame);
            group
                .bench_with_input(BenchmarkId::new("bincode-v2/tokio", n), &n, |b, &tasks| {
                    b.to_async(&rt).iter_custom(|iters| {
                        let frame_t = Arc::clone(&frame_t);
                        async move {
                            let start = std::time::Instant::now();
                            for _ in 0..iters {
                                let mut handles: Vec<JoinHandle<()>> = Vec::with_capacity(tasks);
                                for _ in 0..tasks {
                                    let f = Arc::clone(&frame_t);
                                    handles.push(tokio::spawn(async move {
                                        let bytes = bincode_v2::serde::encode_to_vec(
                                            f.as_ref(),
                                            bincode_v2::config::standard(),
                                        )
                                        .unwrap();
                                        tokio::io::sink().write_all(&bytes).await.unwrap();
                                        black_box(bytes.len());
                                    }));
                                }
                                join_all(handles).await;
                            }
                            start.elapsed()
                        }
                    })
                })
                .measurement_time(Duration::from_secs(600));
        }

        // Keep unused-variable warnings away from the pre-computed enc_* buffers.
        let _ = (&enc_fixed, &enc_postcard, &enc_v1, &enc_v2);
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Benchmark 2: decode_stream
// Every task reads pre-encoded bytes from a YieldingReader (simulates a chunked
// network stream) and deserializes the frame.
// ---------------------------------------------------------------------------

fn bench_decode_stream(c: &mut Criterion) {
    init_dtact();
    let rt = make_tokio_rt();

    let frame = make_frame();
    let cfg = config::standard();
    let cfg_fixed = config::legacy();

    let enc_varint = Arc::new(bincode::encode_to_vec(&frame, cfg).unwrap());
    let enc_fixed = Arc::new(bincode::encode_to_vec(&frame, cfg_fixed).unwrap());
    let enc_postcard = Arc::new(postcard::to_stdvec(&frame).unwrap());
    let enc_v1 = Arc::new(bincode_1::serialize(&frame).unwrap());
    let enc_v2 =
        Arc::new(bincode_v2::serde::encode_to_vec(&frame, bincode_v2::config::standard()).unwrap());

    let chunk = 32usize;

    let mut group = c.benchmark_group("decode_stream");
    group.sample_size(10);

    for &n in CONCURRENCY_LEVELS {
        // --- 1. bincode-next sync + dtact ---
        {
            let enc = Arc::clone(&enc_varint);
            group
                .bench_with_input(
                    BenchmarkId::new("bincode-next/dtact", n),
                    &n,
                    |b, &tasks| {
                        b.iter_custom(|iters| {
                            let enc = Arc::clone(&enc);
                            let start = std::time::Instant::now();
                            for _ in 0..iters {
                                let h = dtact::SpawnBuilder::<dtact::CrossThreadNoFloat>::new()
                                    .spawn({
                                        let enc = Arc::clone(&enc);
                                        async move {
                                            let mut handles = Vec::with_capacity(tasks);
                                            for _ in 0..tasks {
                                                let e = Arc::clone(&enc);
                                                handles.push(
                                                    dtact::SpawnBuilder::<
                                                        dtact::CrossThreadNoFloat,
                                                    >::new()
                                                    .spawn(async move {
                                                        let (v, _): (NetworkFrame, usize) =
                                                            bincode::decode_from_slice(
                                                                e.as_ref(),
                                                                config::standard(),
                                                            )
                                                            .unwrap();
                                                        black_box(v);
                                                    }),
                                                );
                                            }
                                            for h in handles {
                                                dtact::dtact_await(h);
                                            }
                                        }
                                    });
                                dtact::dtact_await(h);
                            }
                            start.elapsed()
                        })
                    },
                )
                .measurement_time(Duration::from_secs(600));
        }

        // --- 2. bincode-next fiber + tokio ---
        {
            let enc = Arc::clone(&enc_varint);
            group
                .bench_with_input(
                    BenchmarkId::new("bincode-next/fiber+tokio", n),
                    &n,
                    |b, &tasks| {
                        b.to_async(&rt).iter_custom(|iters| {
                            let enc = Arc::clone(&enc);
                            async move {
                                let start = std::time::Instant::now();
                                for _ in 0..iters {
                                    let mut handles: Vec<JoinHandle<()>> =
                                        Vec::with_capacity(tasks);
                                    for _ in 0..tasks {
                                        let e = Arc::clone(&enc);
                                        handles.push(tokio::spawn(async move {
                                            let reader = YieldingReader {
                                                data: e,
                                                pos: 0,
                                                chunk_size: chunk,
                                            };
                                            let v: NetworkFrame =
                                                decode_async(config::standard(), reader)
                                                    .await
                                                    .unwrap();
                                            black_box(v);
                                        }));
                                    }
                                    join_all(handles).await;
                                }
                                start.elapsed()
                            }
                        })
                    },
                )
                .measurement_time(Duration::from_secs(600));
        }

        // --- 3. postcard + tokio (AsyncReadExt) ---
        {
            let enc = Arc::clone(&enc_postcard);
            group
                .bench_with_input(BenchmarkId::new("postcard/tokio", n), &n, |b, &tasks| {
                    b.to_async(&rt).iter_custom(|iters| {
                        let enc = Arc::clone(&enc);
                        async move {
                            let start = std::time::Instant::now();
                            for _ in 0..iters {
                                let mut handles: Vec<JoinHandle<()>> = Vec::with_capacity(tasks);
                                for _ in 0..tasks {
                                    let e = Arc::clone(&enc);
                                    handles.push(tokio::spawn(async move {
                                        let mut reader = YieldingReader {
                                            data: Arc::clone(&e),
                                            pos: 0,
                                            chunk_size: chunk,
                                        };
                                        let mut buf = [0u8; 1024];
                                        assert!(
                                            e.len() <= buf.len(),
                                            "Encoded frame size {} exceeds buffer capacity",
                                            e.len()
                                        );
                                        let buf = &mut buf[..e.len()];
                                        reader.read_exact(buf).await.unwrap();
                                        let v: NetworkFrame = postcard::from_bytes(&buf).unwrap();
                                        black_box(v);
                                    }));
                                }
                                join_all(handles).await;
                            }
                            start.elapsed()
                        }
                    })
                })
                .measurement_time(Duration::from_secs(600));
        }

        // --- 4. bincode v1 + tokio (AsyncReadExt) ---
        {
            let enc = Arc::clone(&enc_v1);
            group
                .bench_with_input(BenchmarkId::new("bincode-v1/tokio", n), &n, |b, &tasks| {
                    b.to_async(&rt).iter_custom(|iters| {
                        let enc = Arc::clone(&enc);
                        async move {
                            let start = std::time::Instant::now();
                            for _ in 0..iters {
                                let mut handles: Vec<JoinHandle<()>> = Vec::with_capacity(tasks);
                                for _ in 0..tasks {
                                    let e = Arc::clone(&enc);
                                    handles.push(tokio::spawn(async move {
                                        let mut reader = YieldingReader {
                                            data: Arc::clone(&e),
                                            pos: 0,
                                            chunk_size: chunk,
                                        };
                                        let mut buf = [0u8; 1024];
                                        assert!(
                                            e.len() <= buf.len(),
                                            "Encoded frame size {} exceeds buffer capacity",
                                            e.len()
                                        );
                                        let buf = &mut buf[..e.len()];
                                        reader.read_exact(buf).await.unwrap();
                                        let v: NetworkFrame = bincode_1::deserialize(&buf).unwrap();
                                        black_box(v);
                                    }));
                                }
                                join_all(handles).await;
                            }
                            start.elapsed()
                        }
                    })
                })
                .measurement_time(Duration::from_secs(600));
        }

        // --- 5. bincode v2 + tokio (AsyncReadExt) ---
        {
            let enc = Arc::clone(&enc_v2);
            group
                .bench_with_input(BenchmarkId::new("bincode-v2/tokio", n), &n, |b, &tasks| {
                    b.to_async(&rt).iter_custom(|iters| {
                        let enc = Arc::clone(&enc);
                        async move {
                            let start = std::time::Instant::now();
                            for _ in 0..iters {
                                let mut handles: Vec<JoinHandle<()>> = Vec::with_capacity(tasks);
                                for _ in 0..tasks {
                                    let e = Arc::clone(&enc);
                                    handles.push(tokio::spawn(async move {
                                        let mut reader = YieldingReader {
                                            data: Arc::clone(&e),
                                            pos: 0,
                                            chunk_size: chunk,
                                        };
                                        let mut buf = [0u8; 1024];
                                        assert!(
                                            e.len() <= buf.len(),
                                            "Encoded frame size {} exceeds buffer capacity",
                                            e.len()
                                        );
                                        let buf = &mut buf[..e.len()];
                                        reader.read_exact(buf).await.unwrap();
                                        let (v, _): (NetworkFrame, usize) =
                                            bincode_v2::serde::decode_from_slice(
                                                &buf,
                                                bincode_v2::config::standard(),
                                            )
                                            .unwrap();
                                        black_box(v);
                                    }));
                                }
                                join_all(handles).await;
                            }
                            start.elapsed()
                        }
                    })
                })
                .measurement_time(Duration::from_secs(600));
        }

        let _ = (&enc_fixed,);
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Benchmark 3: roundtrip_stream  (mixed concurrent workload)
// Even-indexed tasks encode; odd-indexed tasks decode from pre-encoded bytes.
// This simulates a realistic server that simultaneously handles incoming frames
// (decode) and forwards transformed frames (encode).
// ---------------------------------------------------------------------------

fn bench_roundtrip_stream(c: &mut Criterion) {
    init_dtact();
    let rt = make_tokio_rt();

    let frame = Arc::new(make_frame());
    let cfg = config::standard();
    let cfg_fixed = config::legacy();

    let enc_varint = Arc::new(bincode::encode_to_vec(frame.as_ref(), cfg).unwrap());
    let enc_postcard = Arc::new(postcard::to_stdvec(frame.as_ref()).unwrap());
    let enc_v1 = Arc::new(bincode_1::serialize(frame.as_ref()).unwrap());
    let enc_v2 = Arc::new(
        bincode_v2::serde::encode_to_vec(frame.as_ref(), bincode_v2::config::standard()).unwrap(),
    );

    let chunk = 32usize;

    let mut group = c.benchmark_group("roundtrip_stream");
    group.sample_size(10);

    for &n in CONCURRENCY_LEVELS {
        // --- 1. bincode-next sync + dtact ---
        {
            let frame_d = Arc::clone(&frame);
            let enc_d = Arc::clone(&enc_varint);
            group
                .bench_with_input(
                    BenchmarkId::new("bincode-next/dtact", n),
                    &n,
                    |b, &tasks| {
                        b.iter_custom(|iters| {
                            let frame_d = Arc::clone(&frame_d);
                            let enc_d = Arc::clone(&enc_d);
                            let start = std::time::Instant::now();
                            for _ in 0..iters {
                                let h = dtact::SpawnBuilder::<dtact::CrossThreadNoFloat>::new()
                                    .spawn({
                                        let frame_d = Arc::clone(&frame_d);
                                        let enc_d = Arc::clone(&enc_d);
                                        async move {
                                            let mut handles = Vec::with_capacity(tasks);
                                            for i in 0..tasks {
                                                let f = Arc::clone(&frame_d);
                                                let e = Arc::clone(&enc_d);
                                                handles.push(
                                                    dtact::SpawnBuilder::<
                                                        dtact::CrossThreadNoFloat,
                                                    >::new()
                                                    .spawn(async move {
                                                        if i % 2 == 0 {
                                                            let bytes = bincode::encode_to_vec(
                                                                f.as_ref(),
                                                                config::standard(),
                                                            )
                                                            .unwrap();
                                                            black_box(bytes);
                                                        } else {
                                                            let (v, _): (NetworkFrame, usize) =
                                                                bincode::decode_from_slice(
                                                                    e.as_ref(),
                                                                    config::standard(),
                                                                )
                                                                .unwrap();
                                                            black_box(v);
                                                        }
                                                    }),
                                                );
                                            }
                                            for h in handles {
                                                dtact::dtact_await(h);
                                            }
                                        }
                                    });
                                dtact::dtact_await(h);
                            }
                            start.elapsed()
                        })
                    },
                )
                .measurement_time(Duration::from_secs(600));
        }

        // --- 2. bincode-next fiber + tokio ---
        {
            let frame_t = Arc::clone(&frame);
            let enc_t = Arc::clone(&enc_varint);
            group
                .bench_with_input(
                    BenchmarkId::new("bincode-next/fiber+tokio", n),
                    &n,
                    |b, &tasks| {
                        b.to_async(&rt).iter_custom(|iters| {
                            let frame_t = Arc::clone(&frame_t);
                            let enc_t = Arc::clone(&enc_t);
                            async move {
                                let start = std::time::Instant::now();
                                for _ in 0..iters {
                                    let mut handles: Vec<JoinHandle<()>> =
                                        Vec::with_capacity(tasks);
                                    for i in 0..tasks {
                                        let f = Arc::clone(&frame_t);
                                        let e = Arc::clone(&enc_t);
                                        handles.push(tokio::spawn(async move {
                                            if i % 2 == 0 {
                                                let bytes = bincode::encode_to_vec(
                                                    f.as_ref(),
                                                    config::standard(),
                                                )
                                                .unwrap();
                                                tokio::io::sink().write_all(&bytes).await.unwrap();
                                                black_box(bytes.len());
                                            } else {
                                                let reader = YieldingReader {
                                                    data: e,
                                                    pos: 0,
                                                    chunk_size: chunk,
                                                };
                                                let v: NetworkFrame =
                                                    decode_async(config::standard(), reader)
                                                        .await
                                                        .unwrap();
                                                black_box(v);
                                            }
                                        }));
                                    }
                                    join_all(handles).await;
                                }
                                start.elapsed()
                            }
                        })
                    },
                )
                .measurement_time(Duration::from_secs(600));
        }

        // --- 3. postcard + tokio (AsyncWriteExt / AsyncReadExt) ---
        {
            let frame_t = Arc::clone(&frame);
            let enc_t = Arc::clone(&enc_postcard);
            group
                .bench_with_input(BenchmarkId::new("postcard/tokio", n), &n, |b, &tasks| {
                    b.to_async(&rt).iter_custom(|iters| {
                        let frame_t = Arc::clone(&frame_t);
                        let enc_t = Arc::clone(&enc_t);
                        async move {
                            let start = std::time::Instant::now();
                            for _ in 0..iters {
                                let mut handles: Vec<JoinHandle<()>> = Vec::with_capacity(tasks);
                                for i in 0..tasks {
                                    let f = Arc::clone(&frame_t);
                                    let e = Arc::clone(&enc_t);
                                    handles.push(tokio::spawn(async move {
                                        if i % 2 == 0 {
                                            let bytes = postcard::to_stdvec(f.as_ref()).unwrap();
                                            tokio::io::sink().write_all(&bytes).await.unwrap();
                                            black_box(bytes.len());
                                        } else {
                                            let mut reader = YieldingReader {
                                                data: Arc::clone(&e),
                                                pos: 0,
                                                chunk_size: chunk,
                                            };
                                            let mut buf = [0u8; 1024];
                                            assert!(
                                                e.len() <= buf.len(),
                                                "Encoded frame size {} exceeds buffer capacity",
                                                e.len()
                                            );
                                            let buf = &mut buf[..e.len()];
                                            reader.read_exact(buf).await.unwrap();
                                            let v: NetworkFrame =
                                                postcard::from_bytes(&buf).unwrap();
                                            black_box(v);
                                        }
                                    }));
                                }
                                join_all(handles).await;
                            }
                            start.elapsed()
                        }
                    })
                })
                .measurement_time(Duration::from_secs(600));
        }

        // --- 4. bincode v1 + tokio (AsyncWriteExt / AsyncReadExt) ---
        {
            let frame_t = Arc::clone(&frame);
            let enc_t = Arc::clone(&enc_v1);
            group
                .bench_with_input(BenchmarkId::new("bincode-v1/tokio", n), &n, |b, &tasks| {
                    b.to_async(&rt).iter_custom(|iters| {
                        let frame_t = Arc::clone(&frame_t);
                        let enc_t = Arc::clone(&enc_t);
                        async move {
                            let start = std::time::Instant::now();
                            for _ in 0..iters {
                                let mut handles: Vec<JoinHandle<()>> = Vec::with_capacity(tasks);
                                for i in 0..tasks {
                                    let f = Arc::clone(&frame_t);
                                    let e = Arc::clone(&enc_t);
                                    handles.push(tokio::spawn(async move {
                                        if i % 2 == 0 {
                                            let bytes = bincode_1::serialize(f.as_ref()).unwrap();
                                            tokio::io::sink().write_all(&bytes).await.unwrap();
                                            black_box(bytes.len());
                                        } else {
                                            let mut reader = YieldingReader {
                                                data: Arc::clone(&e),
                                                pos: 0,
                                                chunk_size: chunk,
                                            };
                                            let mut buf = [0u8; 1024];
                                            assert!(
                                                e.len() <= buf.len(),
                                                "Encoded frame size {} exceeds buffer capacity",
                                                e.len()
                                            );
                                            let buf = &mut buf[..e.len()];
                                            reader.read_exact(buf).await.unwrap();
                                            let v: NetworkFrame =
                                                bincode_1::deserialize(&buf).unwrap();
                                            black_box(v);
                                        }
                                    }));
                                }
                                join_all(handles).await;
                            }
                            start.elapsed()
                        }
                    })
                })
                .measurement_time(Duration::from_secs(600));
        }

        // --- 5. bincode v2 + tokio (AsyncWriteExt / AsyncReadExt) ---
        {
            let frame_t = Arc::clone(&frame);
            let enc_t = Arc::clone(&enc_v2);
            group
                .bench_with_input(BenchmarkId::new("bincode-v2/tokio", n), &n, |b, &tasks| {
                    b.to_async(&rt).iter_custom(|iters| {
                        let frame_t = Arc::clone(&frame_t);
                        let enc_t = Arc::clone(&enc_t);
                        async move {
                            let start = std::time::Instant::now();
                            for _ in 0..iters {
                                let mut handles: Vec<JoinHandle<()>> = Vec::with_capacity(tasks);
                                for i in 0..tasks {
                                    let f = Arc::clone(&frame_t);
                                    let e = Arc::clone(&enc_t);
                                    handles.push(tokio::spawn(async move {
                                        if i % 2 == 0 {
                                            let bytes = bincode_v2::serde::encode_to_vec(
                                                f.as_ref(),
                                                bincode_v2::config::standard(),
                                            )
                                            .unwrap();
                                            tokio::io::sink().write_all(&bytes).await.unwrap();
                                            black_box(bytes.len());
                                        } else {
                                            let mut reader = YieldingReader {
                                                data: Arc::clone(&e),
                                                pos: 0,
                                                chunk_size: chunk,
                                            };
                                            let mut buf = [0u8; 1024];
                                            assert!(
                                                e.len() <= buf.len(),
                                                "Encoded frame size {} exceeds buffer capacity",
                                                e.len()
                                            );
                                            let buf = &mut buf[..e.len()];
                                            reader.read_exact(buf).await.unwrap();
                                            let (v, _): (NetworkFrame, usize) =
                                                bincode_v2::serde::decode_from_slice(
                                                    &buf,
                                                    bincode_v2::config::standard(),
                                                )
                                                .unwrap();
                                            black_box(v);
                                        }
                                    }));
                                }
                                join_all(handles).await;
                            }
                            start.elapsed()
                        }
                    })
                })
                .measurement_time(Duration::from_secs(600));
        }

        let _ = &cfg_fixed;
    }

    group.finish();
}

criterion_group!(
    codec_concurrency,
    bench_encode_stream,
    bench_decode_stream,
    bench_roundtrip_stream
);
criterion_main!(codec_concurrency);
