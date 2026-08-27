# bincode\_derive-next

Procedural macros for [bincode-next](https://crates.io/crates/bincode-next). You normally
do not depend on this crate directly — it is re-exported through the `derive` feature of
`bincode-next`.

```toml
[dependencies]
bincode-next = { version = "3.1.1", features = ["derive"] }
```

---

## Derive macros

### `Encode` / `Decode`

The core traits. Derive them on any struct or enum whose fields also implement `Encode`/`Decode`.

```rust
use bincode_next::{Encode, Decode, config};

#[derive(Encode, Decode, PartialEq, Debug)]
struct Point { x: f32, y: f32 }

let p = Point { x: 1.0, y: 2.0 };
let bytes = bincode_next::encode_to_vec(&p, config::standard()).unwrap();
let (decoded, _): (Point, usize) = bincode_next::decode_from_slice(&bytes, config::standard()).unwrap();
assert_eq!(p, decoded);
```

**Field attributes**

| Attribute | Effect |
|-----------|--------|
| `#[bincode(with_serde)]` | Encode/decode this field via its `serde::Serialize`/`Deserialize` impl |
| `#[bincode(bits = N)]` | Bit-pack this field into `N` bits (requires `BitPacked` derive) |

**Container attributes**

| Attribute | Effect |
|-----------|--------|
| `#[bincode(crate = "path")]` | Override the crate path (useful when re-exporting) |
| `#[bincode(bounds = "T: Trait")]` | Replace the auto-generated where-clause |
| `#[bincode(encode_bounds = "…")]` | Override bounds for `Encode` only |
| `#[bincode(decode_bounds = "…")]` | Override bounds for `Decode` only |
| `#[bincode(decode_context = "MyCtx")]` | Use a custom decode context instead of the default `()` |

---

### `BitPacked`

Generates `Encode`, `Decode`, and `BorrowDecode` implementations that pack annotated
fields at bit granularity when the config has `.with_bit_packing()` enabled. Falls back
to normal byte-aligned encoding when bit-packing is off.

```rust
use bincode_next::{BitPacked, config};

#[derive(BitPacked, PartialEq, Debug)]
struct Flags {
    #[bincode(bits = 4)]
    kind: u8,
    #[bincode(bits = 4)]
    priority: u8,
    // 4 + 4 = 8 bits → 1 byte when bit-packing is enabled
}

let config = config::standard().with_bit_packing();
let f = Flags { kind: 3, priority: 7 };
let bytes = bincode_next::encode_to_vec(&f, config).unwrap();
assert_eq!(bytes.len(), 1);
let (decoded, _): (Flags, usize) = bincode_next::decode_from_slice(&bytes, config).unwrap();
assert_eq!(f, decoded);
```

Consecutive `#[bincode(bits = N)]` fields are flushed to a byte boundary as a group.
Fields without a `bits` attribute are always byte-aligned.

**Enum support**: `#[derive(BitPacked)]` on an enum packs the variant discriminant
into the minimum number of bits needed, followed by the variant's fields.

---

### `Fingerprint`

Derives `Fingerprint<C: Config>`, contributing this type's field names, types, and
order to the schema hash. Used together with `config.with_fingerprint()` to detect
encoder/decoder schema mismatches at runtime.

```rust
use bincode_next::{Fingerprint, Encode, Decode, config};

#[derive(Fingerprint, Encode, Decode, PartialEq, Debug)]
struct Record { id: u32, value: String }

let cfg = config::standard().with_fingerprint();
let r = Record { id: 1, value: "hello".into() };
let bytes = bincode_next::encode_to_vec(&r, cfg).unwrap();
// The first 8 bytes of `bytes` contain Record's SCHEMA_HASH.
let (decoded, _): (Record, usize) = bincode_next::decode_from_slice(&bytes, cfg).unwrap();
assert_eq!(r, decoded);
```

The hash covers field names, types, field order, and the **full config** (format,
endianness, integer encoding, CBOR options). Switching between Bincode and CBOR,
or adding/renaming a field, produces a different hash and triggers a decode error.

---

### `StaticSize`

Derives `StaticSize`, providing two compile-time constants:

- `MAX_SIZE` — worst-case encoded size in bytes (varint upper bounds, no bit-packing).
- `PACKED_MAX_SIZE` — tighter bound when bit-packing is active; consecutive
  `#[bincode(bits = N)]` field groups are counted as `ceil(N_total / 8)` bytes.

Requires the `static-size` feature on `bincode-next`.

```rust
use bincode_next::StaticSize;

#[derive(bincode_next::Encode, bincode_next::Decode, StaticSize, PartialEq, Debug)]
struct Header { version: u32, flags: u16 }

// u32 varint ≤ 5 bytes, u16 varint ≤ 3 bytes
assert_eq!(Header::MAX_SIZE, 8);
```

For types with `#[bincode(bits = N)]` fields the derive also generates a correct
`PACKED_MAX_SIZE` distinct from `MAX_SIZE`.

---

### `ZeroCopy`

Derives zero-copy support for `#[repr(C)]` structs and `#[repr(C, u8)]` enums.
Generates a companion `*Builder` type with matching variants/fields that writes directly
into a `ZeroBuilder` byte blob. The result can be accessed as a typed reference into
the buffer with zero allocation.

Requires the `zero-copy` feature on `bincode-next`.

```rust
// See the main bincode-next README for a full ZeroCopy example.
#[derive(bincode_derive_next::ZeroCopy, Debug, PartialEq, Eq)]
#[repr(C, u8)]
enum Message {
    Ping,
    Data { seq: u32, payload: u64 },
}
// #[derive(ZeroCopy)] generates `MessageBuilder` automatically.
```

---

## Internal architecture

The crate is split into two layers:

**Parsing** (`src/attribute.rs`, `src/lib.rs`)  
Reads `#[bincode(...)]` attributes off container and field items and populates
`ContainerAttributes` / `FieldAttributes`.

**Code generation** (one file per derive)

| File | Derive |
|------|--------|
| `src/derive_struct.rs` / `src/derive_enum.rs` | `Encode`, `Decode`, `BorrowDecode` |
| `src/derive_bit_packed.rs` | `BitPacked` |
| `src/derive_fingerprint.rs` | `Fingerprint` |
| `src/derive_static_size.rs` | `StaticSize` |
| `src/derive_zerocopy.rs` | `ZeroCopy` |

All generation uses the [virtue](https://crates.io/crates/virtue) proc-macro helper.
For debugging, generated token streams are written to
`target/generated/bincode/<TypeName>_Encode.rs` etc.

For additional integration tests see `../tests/`.
