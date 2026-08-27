/// Binary Format Comparison: bincode-next (varint & fixed) vs postcard v1.1
///
/// Same complex telemetry payload as cbor_comparison to keep data fixtures consistent.
/// Focuses on the bincode wire format vs postcard's compact encoding.
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
// Data model – same structure as cbor_comparison without minicbor annotations.
// ---------------------------------------------------------------------------

#[derive(bincode::Encode, bincode::Decode, Serialize, Deserialize, PartialEq, Debug, Clone)]
struct TelemetryBatch {
    batch_id: u64,
    source: String,
    agent_version: String,
    collection_ts: u64,
    events: Vec<TelemetryEvent>,
    aggregates: Vec<Aggregate>,
    metadata: BatchMetadata,
}

#[derive(bincode::Encode, bincode::Decode, Serialize, Deserialize, PartialEq, Debug, Clone)]
struct TelemetryEvent {
    event_id: u64,
    category: EventCategory,
    severity: Severity,
    message: String,
    attributes: Vec<Attribute>,
    timestamp_us: u64,
    duration_ns: u64,
    trace_id: u64,
    span_id: u64,
}

#[derive(bincode::Encode, bincode::Decode, Serialize, Deserialize, PartialEq, Debug, Clone)]
enum EventCategory {
    Http {
        method: String,
        status_code: u16,
        path: String,
        latency_ms: u32,
    },
    Database {
        query_kind: QueryKind,
        table: String,
        rows_affected: u32,
        latency_us: u64,
    },
    Cache {
        operation: String,
        hit: bool,
        latency_us: u32,
    },
    Grpc {
        service: String,
        method: String,
        status_code: u32,
    },
    Custom(String),
}

#[derive(bincode::Encode, bincode::Decode, Serialize, Deserialize, PartialEq, Debug, Clone)]
enum QueryKind {
    Select,
    Insert,
    Update,
    Delete,
    Transaction,
    Other(String),
}

#[derive(bincode::Encode, bincode::Decode, Serialize, Deserialize, PartialEq, Debug, Clone)]
enum Severity {
    Debug,
    Info,
    Warn,
    Error { code: u32 },
    Fatal { code: u32, component: String },
}

#[derive(bincode::Encode, bincode::Decode, Serialize, Deserialize, PartialEq, Debug, Clone)]
struct Attribute {
    key: String,
    value: AttrValue,
}

#[derive(bincode::Encode, bincode::Decode, Serialize, Deserialize, PartialEq, Debug, Clone)]
enum AttrValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    Text(String),
    Bytes(Vec<u8>),
}

#[derive(bincode::Encode, bincode::Decode, Serialize, Deserialize, PartialEq, Debug, Clone)]
struct Aggregate {
    name: String,
    agg_type: AggregateType,
    value: f64,
    count: u64,
    unit: String,
    labels: Vec<String>,
}

#[derive(bincode::Encode, bincode::Decode, Serialize, Deserialize, PartialEq, Debug, Clone)]
enum AggregateType {
    Counter,
    Gauge,
    Histogram {
        min: f64,
        max: f64,
        p50: f64,
        p95: f64,
        p99: f64,
    },
    Summary {
        p50: f64,
        p95: f64,
        p99: f64,
        sample_count: u64,
    },
}

#[derive(bincode::Encode, bincode::Decode, Serialize, Deserialize, PartialEq, Debug, Clone)]
struct BatchMetadata {
    host: String,
    region: String,
    env: String,
    service: String,
    instance_id: String,
    tag_keys: Vec<String>,
    tag_values: Vec<String>,
    schema_version: u32,
}

// ---------------------------------------------------------------------------
// Fixture
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

fn bench_postcard_encode(c: &mut Criterion) {
    let batch = make_batch();
    let cfg_varint = config::standard();
    let cfg_fixed = config::legacy();

    let mut group = c.benchmark_group("postcard_encode");
    group
        .warm_up_time(Duration::from_secs(10))
        .measurement_time(Duration::from_secs(30))
        .sample_size(500);

    group.bench_function("bincode-next/varint", |b| {
        b.iter(|| black_box(bincode::encode_to_vec(black_box(&batch), cfg_varint).unwrap()))
    });

    group.bench_function("bincode-next/fixed", |b| {
        b.iter(|| black_box(bincode::encode_to_vec(black_box(&batch), cfg_fixed).unwrap()))
    });

    group.bench_function("postcard", |b| {
        b.iter(|| black_box(postcard::to_stdvec(black_box(&batch)).unwrap()))
    });

    group.finish();
}

fn bench_postcard_decode(c: &mut Criterion) {
    let batch = make_batch();
    let cfg_varint = config::standard();
    let cfg_fixed = config::legacy();

    let bytes_varint = bincode::encode_to_vec(&batch, cfg_varint).unwrap();
    let bytes_fixed = bincode::encode_to_vec(&batch, cfg_fixed).unwrap();
    let bytes_postcard = postcard::to_stdvec(&batch).unwrap();

    let (rt, _): (TelemetryBatch, usize) =
        bincode::decode_from_slice(&bytes_varint, cfg_varint).unwrap();
    assert_eq!(rt, batch);
    let rt_pc: TelemetryBatch = postcard::from_bytes(&bytes_postcard).unwrap();
    assert_eq!(rt_pc, batch);

    let mut group = c.benchmark_group("postcard_decode");
    group
        .warm_up_time(Duration::from_secs(10))
        .measurement_time(Duration::from_secs(30))
        .sample_size(500);

    group.bench_function("bincode-next/varint", |b| {
        b.iter(|| {
            let (v, _): (TelemetryBatch, usize) =
                bincode::decode_from_slice(black_box(&bytes_varint), cfg_varint).unwrap();
            black_box(v)
        })
    });

    group.bench_function("bincode-next/fixed", |b| {
        b.iter(|| {
            let (v, _): (TelemetryBatch, usize) =
                bincode::decode_from_slice(black_box(&bytes_fixed), cfg_fixed).unwrap();
            black_box(v)
        })
    });

    group.bench_function("postcard", |b| {
        b.iter(|| {
            let v: TelemetryBatch = postcard::from_bytes(black_box(&bytes_postcard)).unwrap();
            black_box(v)
        })
    });

    group.finish();
}

criterion_group!(
    postcard_comparison,
    bench_postcard_encode,
    bench_postcard_decode
);
criterion_main!(postcard_comparison);
