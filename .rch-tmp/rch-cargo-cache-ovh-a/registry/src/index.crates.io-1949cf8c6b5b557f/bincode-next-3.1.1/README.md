# Bincode-Next

<img align="right" src="./logo.svg" height="200" />

[![Discord Server](https://img.shields.io/discord/1459399539403522074.svg?label=Discord&logo=discord&color=blue)](https://discord.gg/D5e2czMTT9)
[![License](https://img.shields.io/crates/l/bincode-next)](https://opensource.org/licenses/MIT)
[![Scc Count Badge Code](https://sloc.xyz/github/Apich-Organization/bincode/?category=code)](https://github.com/Apich-Organization/bincode/)
[![DOI](https://zenodo.org/badge/DOI/10.6084/m9.figshare.31410402.svg)](https://doi.org/10.6084/m9.figshare.31410402)
[![OpenSSF Best Practices](https://www.bestpractices.dev/projects/12960/badge)](https://www.bestpractices.dev/projects/12960)
[![OpenSSF Scorecard](https://api.scorecard.dev/projects/github.com/Apich-Organization/bincode/badge)](https://securityscorecards.dev/viewer/?uri=github.com/Apich-Organization/bincode)

**Bincode-Next** is a high-performance binary encoder/decoder pair that uses a zero-fluff encoding scheme. It is a modernized fork of the original `bincode` library, maintained by the Apich Organization to ensure continued development and extreme performance optimizations for the Rust ecosystem.

The size of the encoded object will be the same or smaller than the size that the object takes up in memory in a running Rust program.

## Key Features

- **Performance**: Leverages SIMD (SSE2 on x86_64, NEON on AArch64) for rapid varint scanning and bulk primitive copying for massive throughput.
- **Zero-Copy**: Nested zero-copy support via Relative Pointers and const alignment. (optional `zero-copy` feature)
- **Bit-Packing**: Bit-level packing for space-optimized serialization. (`BitPacked` derive + `config.with_bit_packing()`)
- **Schema Fingerprinting**: 64-bit schema hash covering field names, types, order, and full configuration — format changes (Bincode vs CBOR), endianness, integer encoding, and CBOR options all produce distinct fingerprints. (`Fingerprint` derive + `config.with_fingerprint()`)
- **Compile-time Memory Bounds**: `StaticSize` gives a worst-case byte bound at compile time; `PACKED_MAX_SIZE` gives the tighter bound when bit-packing is active. (`static-size` feature)
- **CBOR Format**: Full RFC 8949 CBOR encoding/decoding with deterministic modes. (`config.with_cbor_format()`)
- **Async Fiber Decoding**: Zero-cost async decoding via Unified Fiber-backed Async (UFA). (`async-fiber` feature)
- **Stream Support**: Works seamlessly with `std::io` (Reader/Writer) and `no_std` environments.

## Getting Started

Add `bincode-next` to your `Cargo.toml`:

```toml
[dependencies]
bincode-next = "3.1.1"
```

### Basic Encode / Decode

```rust
use bincode_next::{config, Decode, Encode};

#[derive(Encode, Decode, PartialEq, Debug)]
struct Entity {
    x: f32,
    y: f32,
}

#[derive(Encode, Decode, PartialEq, Debug)]
struct World(Vec<Entity>);

fn main() {
    let config = config::standard();
    let world = World(vec![Entity { x: 0.0, y: 4.0 }, Entity { x: 10.0, y: 20.5 }]);

    let encoded: Vec<u8> = bincode_next::encode_to_vec(&world, config).unwrap();
    let (decoded, len): (World, usize) =
        bincode_next::decode_from_slice(&encoded[..], config).unwrap();

    assert_eq!(world, decoded);
    assert_eq!(len, encoded.len());
}
```

---

### Serde Compatibility

Bincode-Next works with any type that already derives `serde::Serialize` /
`serde::Deserialize` — no need to re-derive `Encode`/`Decode` at all. Enable the
`serde` feature and use the `bincode_next::serde::*` entry points.

```toml
[dependencies]
bincode-next = { version = "3.1.1", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
```

```rust
# #[cfg(feature = "serde")] {
use serde::{Deserialize, Serialize};

// Only serde derives — no Encode/Decode needed.
#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct Config {
    host: String,
    port: u16,
    #[serde(default)]
    retries: u8,
}

fn main() {
    let cfg = Config { host: "localhost".into(), port: 8080, retries: 3 };

    // Encode via serde — honours all #[serde(...)] attributes
    let bytes = bincode_next::serde::encode_to_vec(&cfg, bincode_next::config::standard()).unwrap();

    let (decoded, _): (Config, usize) =
        bincode_next::serde::decode_from_slice(&bytes, bincode_next::config::standard()).unwrap();
    assert_eq!(cfg, decoded);
}
# }
```

You can also mix: derive both `Serialize` and `Encode` on the same type, then use
`#[bincode(with_serde)]` on individual fields to route specific fields through their
serde impl (useful for types that only implement `Serialize`, not `Encode`).

---

### Bit-Packing

Enable bit-packing in your configuration to pack fields at bit granularity. Consecutive
`#[bincode(bits = N)]` fields share bytes — 3 bits + 5 bits = exactly 1 byte on the wire.

```rust
use bincode_next::{config, BitPacked};

#[derive(BitPacked, PartialEq, Debug)]
struct Telemetry {
    #[bincode(bits = 1)]
    is_active: bool,
    #[bincode(bits = 1)]
    has_error: bool,
    #[bincode(bits = 3)]
    mode: u8,
    // ↑ 5 bits total → 1 byte on the wire when bit-packing is enabled
}

fn main() {
    let config = config::standard().with_bit_packing();
    let t = Telemetry { is_active: true, has_error: false, mode: 5 };

    let encoded = bincode_next::encode_to_vec(&t, config).unwrap();
    assert_eq!(encoded.len(), 1); // 5 bits packed into 1 byte

    let (decoded, _): (Telemetry, usize) =
        bincode_next::decode_from_slice(&encoded, config).unwrap();
    assert_eq!(decoded, t);
}
```

---

### Zero-Copy Structures

The `zero-copy` feature lets you build flat byte blobs that can be accessed as typed
Rust references **without any deserialization step** — ideal for memory-mapped files,
shared memory, and IPC.

`#[derive(ZeroCopy)]` on a `#[repr(C, u8)]` enum generates a companion `*Builder`
type that mirrors every variant. Use `ZeroBuilder` to accumulate bytes, `reserve::<T>()`
to claim space, and `build_to_target()` to write and get back a live typed reference
directly into the buffer.

```rust
#[cfg(all(feature = "zero-copy", feature = "alloc"))]
use bincode_next::{ZeroBuilder, ZeroCopyBuilder, DeepValidator};

/// Packet layout stored verbatim in the byte blob.
#[derive(bincode_derive_next::ZeroCopy, Debug, PartialEq, Eq)]
#[repr(C, u8)]
enum Packet {
    Ping,
    Data { seq: u32, value: u64 },
    Error(u32),
}

#[cfg(all(feature = "zero-copy", feature = "alloc"))]
fn main() {
    let mut builder = ZeroBuilder::new();

    // — Ping ----------------------------------------------------------------
    let ping_offset = builder.reserve::<Packet>();
    let ping_view = PacketBuilder::Ping.build_to_target(&mut builder, ping_offset);
    assert_eq!(ping_view, Packet::Ping);

    // — Data ----------------------------------------------------------------
    let data_offset = builder.reserve::<Packet>();
    let data_view =
        PacketBuilder::Data { seq: 7, value: 0xDEAD_BEEF }
            .build_to_target(&mut builder, data_offset);

    match data_view {
        Packet::Data { seq, value } => {
            assert_eq!(seq, 7);
            assert_eq!(value, 0xDEAD_BEEF);
        }
        _ => unreachable!(),
    }

    // — Error ---------------------------------------------------------------
    let err_offset = builder.reserve::<Packet>();
    let err_view = PacketBuilder::Error(404).build_to_target(&mut builder, err_offset);

    match err_view {
        Packet::Error(code) => assert_eq!(code, 404),
        _ => unreachable!(),
    }

    // All three packets live in one contiguous allocation — no heap per variant.
    let _bytes = builder.finish();
}
```

For lower-level use, `RelativePtr<T, OFFSET_SIZE>` lets you embed self-relative
pointers inside any `#[repr(C)]` struct:

```rust
#[cfg(feature = "zero-copy")]
use bincode_next::{RelativePtr, DeepValidator};

#[repr(align(8))]
struct AlignedBuf<const N: usize>(pub [u8; N]);

#[cfg(feature = "zero-copy")]
fn relative_ptr_example() {
    let mut buf = AlignedBuf([0u8; 12]);
    let b = &mut buf.0;

    b[0..4].copy_from_slice(&8i32.to_ne_bytes()); // 4-byte signed offset stored at position 0
    b[8..12].copy_from_slice(&42u32.to_ne_bytes()); // target value at position 8

    let ptr = unsafe { &*(b.as_ptr() as *const RelativePtr<u32, 4>) };
    // is_valid_deep also validates any nested relative pointers recursively
    assert!(ptr.is_valid_deep(b));
    assert_eq!(*ptr.get(b).unwrap(), 42);
}
```

---

### Compile-time Memory Bounds (`StaticSize`)

`StaticSize` gives a compile-time upper bound on encoded size — useful for stack
allocation and `no_std` fixed-size buffers. Enable with the `static-size` feature.

`MAX_SIZE` assumes worst-case varint encoding; `PACKED_MAX_SIZE` is tighter when
bit-packing is active (consecutive `#[bincode(bits = N)]` fields share bytes).

```rust
#[cfg(feature = "static-size")]
use bincode_next::{StaticSize, BitPacked};

#[cfg(feature = "static-size")]
#[derive(bincode_next::Encode, bincode_next::Decode, StaticSize, PartialEq, Debug)]
struct Packet {
    seq: u32,   // varint: up to 5 bytes
    data: u64,  // varint: up to 9 bytes
}

#[cfg(feature = "static-size")]
#[derive(BitPacked, StaticSize, PartialEq, Debug)]
struct Flags {
    #[bincode(bits = 4)]
    kind: u8,
    #[bincode(bits = 4)]
    priority: u8,
}

#[cfg(feature = "static-size")]
fn main() {
    // Packet: 5 (u32) + 9 (u64) = 14 bytes worst-case
    assert_eq!(Packet::MAX_SIZE, 14);

    // Flags without packing: two full u8s = 2 bytes
    assert_eq!(Flags::MAX_SIZE, 2);
    // Flags with packing: 4+4 bits = 1 byte
    assert_eq!(Flags::PACKED_MAX_SIZE, 1);

    // Use MAX_SIZE for a guaranteed-large-enough stack buffer
    let val = Packet { seq: 1, data: 42 };
    let mut buf = [0u8; Packet::MAX_SIZE];
    let _ = bincode_next::encode_into_slice(&val, &mut buf, bincode_next::config::standard()).unwrap();

    // decode_from_slice_static takes &[u8; N] — pass the whole fixed-size array
    let decoded: Packet =
        bincode_next::decode_from_slice_static(&buf, bincode_next::config::standard()).unwrap();
    assert_eq!(val, decoded);
}
```

---

### Schema Fingerprinting

Fingerprinting embeds a 64-bit schema hash into each encoded message. The hash covers
field names, types, ordering, **and the full configuration** — including format
(Bincode vs CBOR), endianness, integer encoding, and all CBOR options. Any mismatch
between encoder and decoder returns a `DecodeError::SchemaHashMismatch`.

```rust
use bincode_next::{config, Decode, Encode, Fingerprint};

#[derive(Encode, Decode, Fingerprint, PartialEq, Debug, Clone)]
struct PlayerV1 {
    id: u32,
    score: u64,
}

// Adding a field changes the schema hash → decode_from_slice returns an error
#[derive(Encode, Decode, Fingerprint, PartialEq, Debug, Clone)]
struct PlayerV2 {
    id: u32,
    score: u64,
    level: u32, // new field
}

fn main() {
    let config = config::standard().with_fingerprint();
    let player = PlayerV1 { id: 1, score: 9001 };

    let encoded = bincode_next::encode_to_vec(&player, config).unwrap();

    // Decoding as V1 succeeds
    let (decoded, _): (PlayerV1, usize) =
        bincode_next::decode_from_slice(&encoded, config).unwrap();
    assert_eq!(decoded, player);

    // Decoding as V2 fails — schema hashes differ
    let result = bincode_next::decode_from_slice::<PlayerV2, _>(&encoded, config);
    assert!(result.is_err());

    // Switching formats also changes the hash; cross-format decoding is caught too
    let cbor_config = config::standard().with_fingerprint().with_cbor_format();
    let result = bincode_next::decode_from_slice::<PlayerV1, _>(&encoded, cbor_config);
    assert!(result.is_err());
}
```

---

### CBOR Format

Bincode-Next implements full RFC 8949 CBOR encoding. Switch formats with a single
config call; all existing derives work unchanged.

```rust
use bincode_next::{config, Decode, Encode};

#[derive(Encode, Decode, PartialEq, Debug)]
struct Event {
    timestamp: u64,
    value: f32,
}

fn main() {
    let config = config::standard().with_cbor_format();
    let event = Event { timestamp: 1_700_000_000, value: 3.14 };

    let encoded = bincode_next::encode_to_vec(&event, config).unwrap();
    let (decoded, _): (Event, usize) =
        bincode_next::decode_from_slice(&encoded, config).unwrap();
    assert_eq!(event, decoded);

    // Deterministic (canonical) CBOR for hashing or signing
    let det_config = config::standard().with_deterministic_cbor();
    let det_encoded = bincode_next::encode_to_vec(&event, det_config).unwrap();
    let (det_decoded, _): (Event, usize) =
        bincode_next::decode_from_slice(&det_encoded, det_config).unwrap();
    assert_eq!(event, det_decoded);
}
```

---

### Async Fiber Decoding

Bincode-Next supports true zero-cost asynchronous decoding using **Unified Fiber-backed
Async (UFA)**. Synchronous `Decode` traits run on a dedicated lightweight fiber stack,
avoiding state-machine code generation overhead entirely.

```rust
use bincode_next::{config, decode_async, encode_to_vec, Decode, Encode};

#[derive(Encode, Decode, PartialEq, Debug)]
struct Entity { x: f32, y: f32 }

#[tokio::main]
#[cfg_attr(miri, ignore)]
async fn main() {
    if cfg!(miri) { return; }

    let entity = Entity { x: 1.0, y: 2.0 };
    let encoded = encode_to_vec(&entity, config::standard()).unwrap();

    // Any type implementing `futures_io::AsyncRead` works here.
    let mut reader: &[u8] = &encoded;
    let decoded: Entity = decode_async(config::standard(), &mut reader).await.unwrap();
    assert_eq!(entity, decoded);
}
```

---

## Performance Optimizations

Bincode-Next includes advanced optimizations for extreme performance:
- **SIMD Varint Scanning**: Accelerates decoding of collections (like `Vec<u64>`) by scanning for small values using SSE2 or NEON instructions.
- **Bulk Native Copy**: Automatically detects when data can be copied directly from memory (e.g., slices of primitives with matching endianness) to avoid element-wise processing.
- **Uninitialized Memory**: Utilizes `MaybeUninit` and `set_len` optimizations for `Vec` decoding to avoid redundant zero-initialization.

```shell
git clone https://github.com/Apich-Organization/bincode.git
cd bincode
cargo bench --bench extreme_perf
cargo bench --bench complex
```

TL;DR: Please visit [https://bincode-next.apich.org/](https://bincode-next.apich.org/) for more detailed information.

### **Performance Comparison: Decoding**

*Baseline: **bincode-next (traits, varint)** at 16.878 µs*

| Rank  | Implementation   | Interface | Int Encoding | Median Time   | Relative Speed |
| ----- | ---------------- | --------- | ------------ | ------------- | -------------- |
| **1** | **bincode-next** | traits    | varint       | **16.878 µs** | **1.00x**      |
| 2     | **bincode-next** | traits    | fixed        | 21.872 µs     | 1.30x          |
| 3     | **bincode-v2**   | serde     | fixed        | 21.973 µs     | 1.30x          |
| 4     | **bincode-v1**   | serde     | N/A          | 22.074 µs     | 1.31x          |
| 5     | **bincode-v2**   | serde     | varint       | 25.727 µs     | 1.52x          |

---

### **Performance Comparison: Encoding**

*Baseline: **bincode-next (traits, fixed)** at 2.9350 µs*

| Rank  | Implementation   | Interface | Int Encoding | Median Time   | Relative Speed |
| ----- | ---------------- | --------- | ------------ | ------------- | -------------- |
| **1** | **bincode-next** | traits    | fixed        | **2.9350 µs** | **1.00x**      |
| 2     | **bincode-v1**   | serde     | N/A          | 3.0767 µs     | 1.05x          |
| 3     | **bincode-v2**   | serde     | fixed        | 3.3295 µs     | 1.13x          |
| 4     | **bincode-next** | traits    | varint       | 3.3467 µs     | 1.14x          |
| 5     | **bincode-v2**   | serde     | varint       | 4.2489 µs     | 1.45x          |

---

### **Efficiency Score: Combined Round-Trip Performance**

*Sum of Median Decode + Median Encode (Normalized to the fastest = 1.00x)*

| Rank  | Implementation   | Interface  | Int Encoding | Total Time    | Efficiency Score |
| ----- | ---------------- | ---------- | ------------ | ------------- | ---------------- |
| **1** | **bincode-next** | **traits** | **varint**   | **20.225 µs** | **1.00x**        |
| 2     | **bincode-next** | traits     | fixed        | 24.807 µs     | 1.23x            |
| 3     | **bincode-v1**   | serde      | N/A          | 25.151 µs     | 1.24x            |
| 4     | **bincode-v2**   | serde      | fixed        | 25.303 µs     | 1.25x            |
| 5     | **bincode-v2**   | serde      | varint       | 29.976 µs     | 1.48x            |

---

### **Vector `u64` Decoding: Varint Performance**

*Contrasting small vs. large integer varint decoding.*

| Dataset          | Implementation             | Median Time   | Relative Speed |
| ---------------- | -------------------------- | ------------- | -------------- |
| **Small Varint** | **bincode-next (current)** | **2.8256 µs** | **1.00x**      |
| Small Varint     | bincode-v2 (original)      | 12.450 µs     | 4.41x          |
|                  |                            |               |                |
| **Large Varint** | **bincode-next (current)** | **13.062 µs** | **1.00x**      |
| Large Varint     | bincode-v2 (original)      | 17.635 µs     | 1.35x          |

---

### **Vector `u64` Decoding: Fixed Performance**

*Baseline: **bincode-next (current)** at 1.8373 µs*

| Rank  | Implementation             | Median Time   | Relative Speed |
| ----- | -------------------------- | ------------- | -------------- |
| **1** | **bincode-next (current)** | **1.8373 µs** | **1.00x**      |
| 2     | bincode-v1                 | 7.5378 µs     | 4.10x          |
| 3     | bincode-v2 (original)      | 10.129 µs     | 5.51x          |

---

### **Bulk `u8` Decoding: Throughput Performance**

*Baseline: **bincode-next (current)** at 160.44 ns*

| Rank  | Implementation             | Median Time   | Relative Speed |
| ----- | -------------------------- | ------------- | -------------- |
| **1** | **bincode-next (current)** | **160.44 ns** | **1.00x**      |
| 2     | bincode-v2 (original)      | 273.86 ns     | 1.71x          |
| 3     | bincode-v1                 | 6307.00 ns    | 39.31x         |

## About Security and Code Quality

For security issues, please visit [the Security Team Homepage](https://security.apich.org) for more details on reporting.

All code tests passed `miri` and all main crate source code passed `clippy` without errors.

```shell
MIRIFLAGS="-Zmiri-disable-isolation" cargo +nightly miri test --all-features --no-fail-fast
cargo clippy --all-features
```

We remain committed to code security and welcomed security reporting.

And please notice that contributors shall follow the community guide lines of `bincode-next`.

## Specification

The formal wire-format specification is available in [docs/spec.md](docs/spec.md).

## FAQ

### Why Bincode-Next?
Bincode-Next was created to continue the legacy of the original Bincode project while pushing the boundaries of what's possible with modern Rust performance techniques and AI-assisted development.

### Is it compatible with Bincode 1.x / 2.x?
Yes, Bincode-Next is designed to be wire-compatible with Bincode 2.x when using the same configurations. It also supports legacy 1.x formats via configuration.

## Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for more details.

## License

Bincode-Next is licensed under either of:

- The MIT License (MIT)
- The Apache License, Version 2.0

See [LICENSE.md](LICENSE.md) for details.
