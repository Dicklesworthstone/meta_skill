/// CBOR Codec Comparison: bincode-next vs cbor4ii v1.2 vs minicbor v2.2
///
/// Data model: complex IoT telemetry hierarchy with nested enums, vecs, and scalars.
/// Each codec encodes/decodes the same logical value; wire sizes naturally differ.
extern crate bincode_next as bincode;

use bincode::config;
use criterion::Criterion;
use criterion::criterion_group;
use criterion::criterion_main;
use serde::Deserialize;
use serde::Serialize;
use std::hint::black_box;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Data model – annotated for all three codecs simultaneously.
// minicbor requires #[n(N)] on every field and variant; bincode-next and serde
// ignore those attributes, so a single type definition covers all three.
// ---------------------------------------------------------------------------

#[derive(
    bincode::Encode,
    bincode::Decode,
    Serialize,
    Deserialize,
    minicbor::Encode,
    minicbor::Decode,
    PartialEq,
    Debug,
    Clone,
)]
struct TelemetryBatch {
    #[n(0)]
    batch_id: u64,
    #[n(1)]
    source: String,
    #[n(2)]
    agent_version: String,
    #[n(3)]
    collection_ts: u64,
    #[n(4)]
    events: Vec<TelemetryEvent>,
    #[n(5)]
    aggregates: Vec<Aggregate>,
    #[n(6)]
    metadata: BatchMetadata,
}

#[derive(
    bincode::Encode,
    bincode::Decode,
    Serialize,
    Deserialize,
    minicbor::Encode,
    minicbor::Decode,
    PartialEq,
    Debug,
    Clone,
)]
struct TelemetryEvent {
    #[n(0)]
    event_id: u64,
    #[n(1)]
    category: EventCategory,
    #[n(2)]
    severity: Severity,
    #[n(3)]
    message: String,
    #[n(4)]
    attributes: Vec<Attribute>,
    #[n(5)]
    timestamp_us: u64,
    #[n(6)]
    duration_ns: u64,
    #[n(7)]
    trace_id: u64,
    #[n(8)]
    span_id: u64,
}

#[derive(
    bincode::Encode,
    bincode::Decode,
    Serialize,
    Deserialize,
    minicbor::Encode,
    minicbor::Decode,
    PartialEq,
    Debug,
    Clone,
)]
enum EventCategory {
    #[n(0)]
    Http {
        #[n(0)]
        method: String,
        #[n(1)]
        status_code: u16,
        #[n(2)]
        path: String,
        #[n(3)]
        latency_ms: u32,
    },
    #[n(1)]
    Database {
        #[n(0)]
        query_kind: QueryKind,
        #[n(1)]
        table: String,
        #[n(2)]
        rows_affected: u32,
        #[n(3)]
        latency_us: u64,
    },
    #[n(2)]
    Cache {
        #[n(0)]
        operation: String,
        #[n(1)]
        hit: bool,
        #[n(2)]
        latency_us: u32,
    },
    #[n(3)]
    Grpc {
        #[n(0)]
        service: String,
        #[n(1)]
        method: String,
        #[n(2)]
        status_code: u32,
    },
    #[n(4)]
    Custom(#[n(0)] String),
}

#[derive(
    bincode::Encode,
    bincode::Decode,
    Serialize,
    Deserialize,
    minicbor::Encode,
    minicbor::Decode,
    PartialEq,
    Debug,
    Clone,
)]
enum QueryKind {
    #[n(0)]
    Select,
    #[n(1)]
    Insert,
    #[n(2)]
    Update,
    #[n(3)]
    Delete,
    #[n(4)]
    Transaction,
    #[n(5)]
    Other(#[n(0)] String),
}

#[derive(
    bincode::Encode,
    bincode::Decode,
    Serialize,
    Deserialize,
    minicbor::Encode,
    minicbor::Decode,
    PartialEq,
    Debug,
    Clone,
)]
enum Severity {
    #[n(0)]
    Debug,
    #[n(1)]
    Info,
    #[n(2)]
    Warn,
    #[n(3)]
    Error {
        #[n(0)]
        code: u32,
    },
    #[n(4)]
    Fatal {
        #[n(0)]
        code: u32,
        #[n(1)]
        component: String,
    },
}

#[derive(
    bincode::Encode,
    bincode::Decode,
    Serialize,
    Deserialize,
    minicbor::Encode,
    minicbor::Decode,
    PartialEq,
    Debug,
    Clone,
)]
struct Attribute {
    #[n(0)]
    key: String,
    #[n(1)]
    value: AttrValue,
}

#[derive(
    bincode::Encode,
    bincode::Decode,
    Serialize,
    Deserialize,
    minicbor::Encode,
    minicbor::Decode,
    PartialEq,
    Debug,
    Clone,
)]
enum AttrValue {
    #[n(0)]
    Int(#[n(0)] i64),
    #[n(1)]
    Float(#[n(0)] f64),
    #[n(2)]
    Bool(#[n(0)] bool),
    #[n(3)]
    Text(#[n(0)] String),
    #[n(4)]
    Bytes(#[n(0)] Vec<u8>),
}

#[derive(
    bincode::Encode,
    bincode::Decode,
    Serialize,
    Deserialize,
    minicbor::Encode,
    minicbor::Decode,
    PartialEq,
    Debug,
    Clone,
)]
struct Aggregate {
    #[n(0)]
    name: String,
    #[n(1)]
    agg_type: AggregateType,
    #[n(2)]
    value: f64,
    #[n(3)]
    count: u64,
    #[n(4)]
    unit: String,
    #[n(5)]
    labels: Vec<String>,
}

#[derive(
    bincode::Encode,
    bincode::Decode,
    Serialize,
    Deserialize,
    minicbor::Encode,
    minicbor::Decode,
    PartialEq,
    Debug,
    Clone,
)]
enum AggregateType {
    #[n(0)]
    Counter,
    #[n(1)]
    Gauge,
    #[n(2)]
    Histogram {
        #[n(0)]
        min: f64,
        #[n(1)]
        max: f64,
        #[n(2)]
        p50: f64,
        #[n(3)]
        p95: f64,
        #[n(4)]
        p99: f64,
    },
    #[n(3)]
    Summary {
        #[n(0)]
        p50: f64,
        #[n(1)]
        p95: f64,
        #[n(2)]
        p99: f64,
        #[n(3)]
        sample_count: u64,
    },
}

#[derive(
    bincode::Encode,
    bincode::Decode,
    Serialize,
    Deserialize,
    minicbor::Encode,
    minicbor::Decode,
    PartialEq,
    Debug,
    Clone,
)]
struct BatchMetadata {
    #[n(0)]
    host: String,
    #[n(1)]
    region: String,
    #[n(2)]
    env: String,
    #[n(3)]
    service: String,
    #[n(4)]
    instance_id: String,
    #[n(5)]
    tag_keys: Vec<String>,
    #[n(6)]
    tag_values: Vec<String>,
    #[n(7)]
    schema_version: u32,
}

// ---------------------------------------------------------------------------
// Fixture generator
// ---------------------------------------------------------------------------

fn make_batch() -> TelemetryBatch {
    let categories = [
        EventCategory::Http {
            method: "POST".into(),
            status_code: 200,
            path: "/api/v3/ingest".into(),
            latency_ms: 12,
        },
        EventCategory::Database {
            query_kind: QueryKind::Select,
            table: "sensor_readings".into(),
            rows_affected: 512,
            latency_us: 3400,
        },
        EventCategory::Cache {
            operation: "GET".into(),
            hit: true,
            latency_us: 45,
        },
        EventCategory::Grpc {
            service: "TelemetryService".into(),
            method: "PushBatch".into(),
            status_code: 0,
        },
        EventCategory::Custom("mqtt.publish".into()),
        EventCategory::Database {
            query_kind: QueryKind::Transaction,
            table: "events".into(),
            rows_affected: 1,
            latency_us: 8900,
        },
        EventCategory::Http {
            method: "GET".into(),
            status_code: 404,
            path: "/api/v2/config".into(),
            latency_ms: 3,
        },
        EventCategory::Database {
            query_kind: QueryKind::Other("UPSERT".into()),
            table: "aggregates".into(),
            rows_affected: 256,
            latency_us: 5100,
        },
    ];

    let severities = [
        Severity::Debug,
        Severity::Info,
        Severity::Warn,
        Severity::Error { code: 503 },
        Severity::Fatal {
            code: 1001,
            component: "storage_engine".into(),
        },
    ];

    let attrs_template = [
        Attribute {
            key: "host.ip".into(),
            value: AttrValue::Text("10.0.1.42".into()),
        },
        Attribute {
            key: "request.size".into(),
            value: AttrValue::Int(4096),
        },
        Attribute {
            key: "success".into(),
            value: AttrValue::Bool(true),
        },
        Attribute {
            key: "latency".into(),
            value: AttrValue::Float(1.234),
        },
        Attribute {
            key: "raw_frame".into(),
            value: AttrValue::Bytes(vec![0xde, 0xad, 0xbe, 0xef, 0xca, 0xfe, 0x00, 0x01]),
        },
        Attribute {
            key: "span.name".into(),
            value: AttrValue::Text("db.query".into()),
        },
        Attribute {
            key: "retry_count".into(),
            value: AttrValue::Int(2),
        },
    ];

    let mut events = Vec::with_capacity(40);
    for i in 0..40u64 {
        events.push(TelemetryEvent {
            event_id: 1_000_000 + i,
            category: categories[i as usize % categories.len()].clone(),
            severity: severities[i as usize % severities.len()].clone(),
            message: format!("event {} processed by worker-{}", i, i % 8),
            attributes: attrs_template[..(3 + (i as usize % 5))].to_vec(),
            timestamp_us: 1_700_000_000_000_000 + i * 1000,
            duration_ns: 50_000 + i * 123,
            trace_id: 0xdeadbeef00000000 + i,
            span_id: 0xcafe000000000000 + i,
        });
    }

    let aggregates = vec![
        Aggregate {
            name: "http.request.duration".into(),
            agg_type: AggregateType::Histogram {
                min: 0.5,
                max: 9823.1,
                p50: 12.3,
                p95: 450.0,
                p99: 2100.7,
            },
            value: 12.3,
            count: 100_000,
            unit: "ms".into(),
            labels: vec![
                "region:us-east-1".into(),
                "env:prod".into(),
                "service:api".into(),
            ],
        },
        Aggregate {
            name: "db.query.count".into(),
            agg_type: AggregateType::Counter,
            value: 58320.0,
            count: 58320,
            unit: "requests".into(),
            labels: vec!["db:primary".into(), "query:select".into()],
        },
        Aggregate {
            name: "cache.hit_ratio".into(),
            agg_type: AggregateType::Gauge,
            value: 0.923,
            count: 1,
            unit: "ratio".into(),
            labels: vec!["layer:l1".into()],
        },
        Aggregate {
            name: "queue.depth".into(),
            agg_type: AggregateType::Summary {
                p50: 12.0,
                p95: 87.0,
                p99: 213.0,
                sample_count: 10_000,
            },
            value: 15.4,
            count: 10_000,
            unit: "messages".into(),
            labels: vec!["queue:ingest".into(), "region:eu-west-1".into()],
        },
    ];

    TelemetryBatch {
        batch_id: 9_876_543_210,
        source: "edge-collector-7f3a".into(),
        agent_version: "3.0.0-rc.15".into(),
        collection_ts: 1_700_000_001_234,
        events,
        aggregates,
        metadata: BatchMetadata {
            host: "edge-node-42.prod.example.com".into(),
            region: "us-east-1".into(),
            env: "production".into(),
            service: "telemetry-collector".into(),
            instance_id: "i-0abc123def456789a".into(),
            tag_keys: vec![
                "team".into(),
                "project".into(),
                "cost_center".into(),
                "on_call".into(),
            ],
            tag_values: vec![
                "platform".into(),
                "bincode-v3".into(),
                "CC-1042".into(),
                "alice@example.com".into(),
            ],
            schema_version: 3,
        },
    }
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

fn bench_cbor_encode(c: &mut Criterion) {
    let batch = make_batch();

    let config_cbor = config::standard().with_cbor_format();
    let config_cbor_det = config::standard().with_deterministic_cbor();

    let mut group = c.benchmark_group("cbor_encode");
    group
        .warm_up_time(Duration::from_secs(10))
        .measurement_time(Duration::from_secs(30))
        .sample_size(500);

    group.bench_function("bincode-next/cbor", |b| {
        b.iter(|| black_box(bincode::encode_to_vec(black_box(&batch), config_cbor).unwrap()))
    });

    group.bench_function("bincode-next/cbor-deterministic", |b| {
        b.iter(|| black_box(bincode::encode_to_vec(black_box(&batch), config_cbor_det).unwrap()))
    });

    group.bench_function("cbor4ii", |b| {
        b.iter(|| black_box(cbor4ii::serde::to_vec(Vec::new(), black_box(&batch)).unwrap()))
    });

    group.bench_function("minicbor", |b| {
        b.iter(|| black_box(minicbor::to_vec(black_box(&batch)).unwrap()))
    });

    group.finish();
}

fn bench_cbor_decode(c: &mut Criterion) {
    let batch = make_batch();

    let config_cbor = config::standard().with_cbor_format();
    let config_cbor_det = config::standard().with_deterministic_cbor();

    let bytes_cbor = bincode::encode_to_vec(&batch, config_cbor).unwrap();
    let bytes_cbor_det = bincode::encode_to_vec(&batch, config_cbor_det).unwrap();
    let bytes_cbor4ii = cbor4ii::serde::to_vec(Vec::new(), &batch).unwrap();
    let bytes_minicbor = minicbor::to_vec(&batch).unwrap();

    // Sanity-check round-trips before measuring.
    let (rt_cbor, _): (TelemetryBatch, usize) =
        bincode::decode_from_slice(&bytes_cbor, config_cbor).unwrap();
    assert_eq!(rt_cbor, batch);
    let rt_cbor4ii: TelemetryBatch = cbor4ii::serde::from_slice(&bytes_cbor4ii).unwrap();
    assert_eq!(rt_cbor4ii, batch);
    let rt_mini: TelemetryBatch = minicbor::decode(&bytes_minicbor).unwrap();
    assert_eq!(rt_mini, batch);

    let mut group = c.benchmark_group("cbor_decode");
    group
        .warm_up_time(Duration::from_secs(10))
        .measurement_time(Duration::from_secs(30))
        .sample_size(500);

    group.bench_function("bincode-next/cbor", |b| {
        b.iter(|| {
            let (v, _): (TelemetryBatch, usize) =
                bincode::decode_from_slice(black_box(&bytes_cbor), config_cbor).unwrap();
            black_box(v)
        })
    });

    group.bench_function("bincode-next/cbor-deterministic", |b| {
        b.iter(|| {
            let (v, _): (TelemetryBatch, usize) =
                bincode::decode_from_slice(black_box(&bytes_cbor_det), config_cbor_det).unwrap();
            black_box(v)
        })
    });

    group.bench_function("cbor4ii", |b| {
        b.iter(|| {
            let v: TelemetryBatch = cbor4ii::serde::from_slice(black_box(&bytes_cbor4ii)).unwrap();
            black_box(v)
        })
    });

    group.bench_function("minicbor", |b| {
        b.iter(|| {
            let v: TelemetryBatch = minicbor::decode(black_box(&bytes_minicbor)).unwrap();
            black_box(v)
        })
    });

    group.finish();
}

criterion_group!(cbor_comparison, bench_cbor_encode, bench_cbor_decode);
criterion_main!(cbor_comparison);
