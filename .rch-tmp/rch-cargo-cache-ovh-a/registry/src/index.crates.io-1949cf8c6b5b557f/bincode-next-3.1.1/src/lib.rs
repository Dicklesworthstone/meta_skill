#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![allow(internal_features)]
#![cfg_attr(is_nightly, feature(core_intrinsics))]

//! Bincode-next is a crate for encoding and decoding using a tiny binary
//! serialization strategy.  Using it, you can easily go from having
//! an object in memory, quickly serialize it to bytes, and then
//! deserialize it back just as fast!
//!
//! If you're coming from bincode 1, check out our [migration guide](migration_guide/index.html)
//!
//! # Serde
//!
//! Starting from bincode 2, serde is now an optional dependency. If you want to use serde, please enable the `serde` feature. See [Features](#features) for more information.
//!
//! # Features
//!
//! |Name  |Default?|Affects MSRV?|Supported types for Encode/Decode|Enabled methods                                                  |Other|
//! |------|--------|-------------|-----------------------------------------|-----------------------------------------------------------------|-----|
//! |std   | Yes    | No          |`HashMap` and `HashSet`|`decode_from_std_read` and `encode_into_std_write`|
//! |alloc | Yes    | No          |All common containers in alloc, like `Vec`, `String`, `Box`|`encode_to_vec`|
//! |derive| Yes    | No          |||Enables the `BorrowDecode`, `Decode`, `Encode`, `Fingerprint` and `BitPacked` derive macros|
//! |serde | No     | Yes (MSRV reliant on serde)|`Compat` and `BorrowCompat`, which will work for all types that implement serde's traits|serde-specific encode/decode functions in the [`serde`\] module|Note: There are several [known issues](serde/index.html#known-issues) when using serde and bincode|
//! |zero-copy| No    | No          |`RelativePtr`, `ZeroArray`, `ZeroSlice`, `ZeroStr`, `ZeroString`|Enables the `relative_ptr` module and the `ZeroCopy` derive macro|Zero-copy nested structures using offsets|
//! |static-size| No    | No          |||Enables the `static_size` module, the `bounded` module and the `StaticSize` derive macro|Compile-time size verification|
//! |async-fiber| No    | No          |||Enables the `async_fiber` module and async decoding|Async fiber-based encoding/decoding|
//!
//! # Which functions to use
//!
//! Bincode-next has a couple of pairs of functions that are used in different situations.
//!
//! |Situation|Encode|Decode|
//! |---|---|---
//! |You're working with [`fs::File`\] or [`net::TcpStream`\]|[`encode_into_std_write`\]|[`decode_from_std_read`\]|
//! |you're working with in-memory buffers|[`encode_to_vec`\]|[`decode_from_slice`\]|
//! |You want to use a custom [Reader] and [Writer]|[`encode_into_writer`\]|[`decode_from_reader`\]|
//! |You're working with pre-allocated buffers or on embedded targets|[`encode_into_slice`\]|[`decode_from_slice`\]|
//! |You're working with tokio| - |[`decode_async_tokio_with_context`\][`decode_async_tokio`\]|
//! |You're working with futures-io| - |[`decode_async_with_context`\][`decode_async`\]|
//!
//! **Note:** If you're using `serde`, use `bincode_next::serde::...` instead of `bincode_next::...`
//!
//! ## Getting Started
//!
//! Add `bincode-next` to your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! bincode-next = "3.1.1"
//! ```
//!
//! ### Basic Encode / Decode
//!
//! ```rust
//! let mut slice = [0u8; 100];
//!
//! // You can encode any type that implements `Encode`.
//! // You can automatically implement this trait on custom types with the `derive` feature.
//! let input = (
//!     0u8,
//!     10u32,
//!     10000i128,
//!     'a',
//!     [0u8, 1u8, 2u8, 3u8]
//! );
//!
//! let length = bincode_next::encode_into_slice(
//!     input,
//!     &mut slice,
//!     bincode_next::config::standard()
//! ).unwrap();
//!
//! let slice = &slice[..length];
//! println!("Bytes written: {:?}", slice);
//!
//! // Decoding works the same as encoding.
//! // The trait used is `Decode`, and can also be automatically implemented with the `derive` feature.
//! let decoded: (u8, u32, i128, char, [u8; 4]) = bincode_next::decode_from_slice(slice, bincode_next::config::standard()).unwrap().0;
//!
//! assert_eq!(decoded, input);
//! ```
//!
//! ```rust
//! use bincode_next::Decode;
//! use bincode_next::Encode;
//! use bincode_next::config;
//!
//! #[derive(Encode, Decode, PartialEq, Debug)]
//! struct Entity {
//!     x: f32,
//!     y: f32,
//! }
//!
//! #[derive(Encode, Decode, PartialEq, Debug)]
//! struct World(Vec<Entity>);
//!
//! fn main() {
//!     let config = config::standard();
//!     let world = World(vec![Entity { x: 0.0, y: 4.0 }, Entity { x: 10.0, y: 20.5 }]);
//!
//!     let encoded: Vec<u8> = bincode_next::encode_to_vec(&world, config).unwrap();
//!     let (decoded, len): (World, usize) =
//!         bincode_next::decode_from_slice(&encoded[..], config).unwrap();
//!
//!     assert_eq!(world, decoded);
//!     assert_eq!(len, encoded.len());
//! }
//! ```
//!
//! ---
//!
//! ### Serde Compatibility
//!
//! Bincode-Next works with any type that already derives `serde::Serialize` /
//! `serde::Deserialize` — no need to re-derive `Encode`/`Decode` at all. Enable the
//! `serde` feature and use the `bincode_next::serde::*` entry points.
//!
//! ```toml
//! [dependencies]
//! bincode-next = { version = "3.1.1", features = ["serde"] }
//! serde = { version = "1", features = ["derive"] }
//! ```
//!
//! ```rust
//! # #[cfg(feature = "serde")] {
//! use serde::Deserialize;
//! use serde::Serialize;
//!
//! // Only serde derives — no Encode/Decode needed.
//! #[derive(Serialize, Deserialize, PartialEq, Debug)]
//! struct Config {
//!     host: String,
//!     port: u16,
//!     #[serde(default)]
//!     retries: u8,
//! }
//!
//! fn main() {
//!     let cfg = Config {
//!         host: "localhost".into(),
//!         port: 8080,
//!         retries: 3,
//!     };
//!
//!     // Encode via serde — honours all #[serde(...)] attributes
//!     let bytes =
//!         bincode_next::serde::encode_to_vec(&cfg, bincode_next::config::standard()).unwrap();
//!
//!     let (decoded, _): (Config, usize) =
//!         bincode_next::serde::decode_from_slice(&bytes, bincode_next::config::standard())
//!             .unwrap();
//!     assert_eq!(cfg, decoded);
//! }
//! # }
//! ```
//!
//! You can also mix: derive both `Serialize` and `Encode` on the same type, then use
//! `#[bincode(with_serde)]` on individual fields to route specific fields through their
//! serde impl (useful for types that only implement `Serialize`, not `Encode`).
//!
//! ---
//!
//! ### Bit-Packing
//!
//! Enable bit-packing in your configuration to pack fields at bit granularity. Consecutive
//! `#[bincode(bits = N)]` fields share bytes — 3 bits + 5 bits = exactly 1 byte on the wire.
//!
//! ```rust
//! use bincode_next::BitPacked;
//! use bincode_next::config;
//!
//! #[derive(BitPacked, PartialEq, Debug)]
//! struct Telemetry {
//!     #[bincode(bits = 1)]
//!     is_active: bool,
//!     #[bincode(bits = 1)]
//!     has_error: bool,
//!     #[bincode(bits = 3)]
//!     mode: u8,
//!     // ↑ 5 bits total → 1 byte on the wire when bit-packing is enabled
//! }
//!
//! fn main() {
//!     let config = config::standard().with_bit_packing();
//!     let t = Telemetry {
//!         is_active: true,
//!         has_error: false,
//!         mode: 5,
//!     };
//!
//!     let encoded = bincode_next::encode_to_vec(&t, config).unwrap();
//!     assert_eq!(encoded.len(), 1); // 5 bits packed into 1 byte
//!
//!     let (decoded, _): (Telemetry, usize) =
//!         bincode_next::decode_from_slice(&encoded, config).unwrap();
//!     assert_eq!(decoded, t);
//! }
//! ```
//!
//! ---
//!
//! ### Zero-Copy Structures
//!
//! The `zero-copy` feature lets you build flat byte blobs that can be accessed as typed
//! Rust references **without any deserialization step** — ideal for memory-mapped files,
//! shared memory, and IPC.
//!
//! `#[derive(ZeroCopy)]` on a `#[repr(C, u8)]` enum generates a companion `*Builder`
//! type that mirrors every variant. Use `ZeroBuilder` to accumulate bytes, `reserve::<T>()`
//! to claim space, and `build_to_target()` to write and get back a live typed reference
//! directly into the buffer.
//!
//! ```rust
//! #[cfg(all(feature = "zero-copy", feature = "alloc"))]
//! use bincode_next::DeepValidator;
//! #[cfg(all(feature = "zero-copy", feature = "alloc"))]
//! use bincode_next::ZeroBuilder;
//! #[cfg(all(feature = "zero-copy", feature = "alloc"))]
//! use bincode_next::ZeroCopyBuilder;
//!
//! /// Packet layout stored verbatim in the byte blob.
//! #[derive(bincode_derive_next::ZeroCopy, Debug, PartialEq, Eq)]
//! #[repr(C, u8)]
//! enum Packet {
//!     Ping,
//!     Data { seq: u32, value: u64 },
//!     Error(u32),
//! }
//!
//! #[cfg(all(feature = "zero-copy", feature = "alloc"))]
//! fn main() {
//!     let mut builder = ZeroBuilder::new();
//!
//!     // — Ping ----------------------------------------------------------------
//!     let ping_offset = builder.reserve::<Packet>();
//!     let ping_view = PacketBuilder::Ping.build_to_target(&mut builder, ping_offset);
//!     assert_eq!(ping_view, Packet::Ping);
//!
//!     // — Data ----------------------------------------------------------------
//!     let data_offset = builder.reserve::<Packet>();
//!     let data_view = PacketBuilder::Data {
//!         seq: 7,
//!         value: 0xDEAD_BEEF,
//!     }
//!     .build_to_target(&mut builder, data_offset);
//!
//!     match data_view {
//!         | Packet::Data { seq, value } => {
//!             assert_eq!(seq, 7);
//!             assert_eq!(value, 0xDEAD_BEEF);
//!         },
//!         | _ => unreachable!(),
//!     }
//!
//!     // — Error ---------------------------------------------------------------
//!     let err_offset = builder.reserve::<Packet>();
//!     let err_view = PacketBuilder::Error(404).build_to_target(&mut builder, err_offset);
//!
//!     match err_view {
//!         | Packet::Error(code) => assert_eq!(code, 404),
//!         | _ => unreachable!(),
//!     }
//!
//!     // All three packets live in one contiguous allocation — no heap per variant.
//!     let _bytes = builder.finish();
//! }
//! ```
//!
//! For lower-level use, `RelativePtr<T, OFFSET_SIZE>` lets you embed self-relative
//! pointers inside any `#[repr(C)]` struct:
//!
//! ```rust
//! #[cfg(feature = "zero-copy")]
//! use bincode_next::DeepValidator;
//! #[cfg(feature = "zero-copy")]
//! use bincode_next::RelativePtr;
//!
//! #[repr(align(8))]
//! struct AlignedBuf<const N: usize>(pub [u8; N]);
//!
//! #[cfg(feature = "zero-copy")]
//! fn relative_ptr_example() {
//!     let mut buf = AlignedBuf([0u8; 12]);
//!     let b = &mut buf.0;
//!
//!     b[0..4].copy_from_slice(&8i32.to_ne_bytes()); // 4-byte signed offset stored at position 0
//!     b[8..12].copy_from_slice(&42u32.to_ne_bytes()); // target value at position 8
//!
//!     let ptr = unsafe { &*(b.as_ptr() as *const RelativePtr<u32, 4>) };
//!     // is_valid_deep also validates any nested relative pointers recursively
//!     assert!(ptr.is_valid_deep(b));
//!     assert_eq!(*ptr.get(b).unwrap(), 42);
//! }
//! ```
//!
//! ---
//!
//! ### Compile-time Memory Bounds (`StaticSize`)
//!
//! `StaticSize` gives a compile-time upper bound on encoded size — useful for stack
//! allocation and `no_std` fixed-size buffers. Enable with the `static-size` feature.
//!
//! `MAX_SIZE` assumes worst-case varint encoding; `PACKED_MAX_SIZE` is tighter when
//! bit-packing is active (consecutive `#[bincode(bits = N)]` fields share bytes).
//!
//! ```rust
//! #[cfg(feature = "static-size")]
//! use bincode_next::BitPacked;
//! #[cfg(feature = "static-size")]
//! use bincode_next::StaticSize;
//!
//! #[cfg(feature = "static-size")]
//! #[derive(bincode_next::Encode, bincode_next::Decode, StaticSize, PartialEq, Debug)]
//! struct Packet {
//!     seq: u32,  // varint: up to 5 bytes
//!     data: u64, // varint: up to 9 bytes
//! }
//!
//! #[cfg(feature = "static-size")]
//! #[derive(BitPacked, StaticSize, PartialEq, Debug)]
//! struct Flags {
//!     #[bincode(bits = 4)]
//!     kind: u8,
//!     #[bincode(bits = 4)]
//!     priority: u8,
//! }
//!
//! #[cfg(feature = "static-size")]
//! fn main() {
//!     // Packet: 5 (u32) + 9 (u64) = 14 bytes worst-case
//!     assert_eq!(Packet::MAX_SIZE, 14);
//!
//!     // Flags without packing: two full u8s = 2 bytes
//!     assert_eq!(Flags::MAX_SIZE, 2);
//!     // Flags with packing: 4+4 bits = 1 byte
//!     assert_eq!(Flags::PACKED_MAX_SIZE, 1);
//!
//!     // Use MAX_SIZE for a guaranteed-large-enough stack buffer
//!     let val = Packet { seq: 1, data: 42 };
//!     let mut buf = [0u8; Packet::MAX_SIZE];
//!     let _ = bincode_next::encode_into_slice(&val, &mut buf, bincode_next::config::standard())
//!         .unwrap();
//!
//!     // decode_from_slice_static takes &[u8; N] — pass the whole fixed-size array
//!     let decoded: Packet =
//!         bincode_next::decode_from_slice_static(&buf, bincode_next::config::standard()).unwrap();
//!     assert_eq!(val, decoded);
//! }
//! ```
//!
//! ---
//!
//! ### Schema Fingerprinting
//!
//! Fingerprinting embeds a 64-bit schema hash into each encoded message. The hash covers
//! field names, types, ordering, **and the full configuration** — including format
//! (Bincode vs CBOR), endianness, integer encoding, and all CBOR options. Any mismatch
//! between encoder and decoder returns a `DecodeError::SchemaHashMismatch`.
//!
//! ```rust
//! use bincode_next::Decode;
//! use bincode_next::Encode;
//! use bincode_next::Fingerprint;
//! use bincode_next::config;
//!
//! #[derive(Encode, Decode, Fingerprint, PartialEq, Debug, Clone)]
//! struct PlayerV1 {
//!     id: u32,
//!     score: u64,
//! }
//!
//! // Adding a field changes the schema hash → decode_from_slice returns an error
//! #[derive(Encode, Decode, Fingerprint, PartialEq, Debug, Clone)]
//! struct PlayerV2 {
//!     id: u32,
//!     score: u64,
//!     level: u32, // new field
//! }
//!
//! fn main() {
//!     let config = config::standard().with_fingerprint();
//!     let player = PlayerV1 { id: 1, score: 9001 };
//!
//!     let encoded = bincode_next::encode_to_vec(&player, config).unwrap();
//!
//!     // Decoding as V1 succeeds
//!     let (decoded, _): (PlayerV1, usize) =
//!         bincode_next::decode_from_slice(&encoded, config).unwrap();
//!     assert_eq!(decoded, player);
//!
//!     // Decoding as V2 fails — schema hashes differ
//!     let result = bincode_next::decode_from_slice::<PlayerV2, _>(&encoded, config);
//!     assert!(result.is_err());
//!
//!     // Switching formats also changes the hash; cross-format decoding is caught too
//!     let cbor_config = config::standard().with_fingerprint().with_cbor_format();
//!     let result = bincode_next::decode_from_slice::<PlayerV1, _>(&encoded, cbor_config);
//!     assert!(result.is_err());
//! }
//! ```
//!
//! ---
//!
//! ### CBOR Format
//!
//! Bincode-Next implements full RFC 8949 CBOR encoding. Switch formats with a single
//! config call; all existing derives work unchanged.
//!
//! ```rust
//! use bincode_next::Decode;
//! use bincode_next::Encode;
//! use bincode_next::config;
//!
//! #[derive(Encode, Decode, PartialEq, Debug)]
//! struct Event {
//!     timestamp: u64,
//!     value: f32,
//! }
//!
//! fn main() {
//!     let config = config::standard().with_cbor_format();
//!     let event = Event {
//!         timestamp: 1_700_000_000,
//!         value: 3.14,
//!     };
//!
//!     let encoded = bincode_next::encode_to_vec(&event, config).unwrap();
//!     let (decoded, _): (Event, usize) =
//!         bincode_next::decode_from_slice(&encoded, config).unwrap();
//!     assert_eq!(event, decoded);
//!
//!     // Deterministic (canonical) CBOR for hashing or signing
//!     let det_config = config::standard().with_deterministic_cbor();
//!     let det_encoded = bincode_next::encode_to_vec(&event, det_config).unwrap();
//!     let (det_decoded, _): (Event, usize) =
//!         bincode_next::decode_from_slice(&det_encoded, det_config).unwrap();
//!     assert_eq!(event, det_decoded);
//! }
//! ```
//!
//! ---
//!
//! ### Async Fiber Decoding
//!
//! Bincode-Next supports true zero-cost asynchronous decoding using **Unified Fiber-backed
//! Async (UFA)**. Synchronous `Decode` traits run on a dedicated lightweight fiber stack,
//! avoiding state-machine code generation overhead entirely.
//!
//! ```rust
//! use bincode_next::Decode;
//! use bincode_next::Encode;
//! use bincode_next::config;
//! use bincode_next::decode_async;
//! use bincode_next::encode_to_vec;
//!
//! #[derive(Encode, Decode, PartialEq, Debug)]
//! struct Entity {
//!     x: f32,
//!     y: f32,
//! }
//!
//! #[tokio::main]
//! #[cfg_attr(miri, ignore)]
//! async fn main() {
//!     if cfg!(miri) {
//!         return;
//!     }
//!
//!     let entity = Entity { x: 1.0, y: 2.0 };
//!     let encoded = encode_to_vec(&entity, config::standard()).unwrap();
//!
//!     // Any type implementing `futures_io::AsyncRead` works here.
//!     let mut reader: &[u8] = &encoded;
//!     let decoded: Entity = decode_async(config::standard(), &mut reader).await.unwrap();
//!     assert_eq!(entity, decoded);
//! }
//! ```

// =========================================================================
// RUST LINT CONFIGURATION: bincode-next
// =========================================================================

// -------------------------------------------------------------------------
// LEVEL 1: CRITICAL ERRORS (Deny)
// -------------------------------------------------------------------------
#![deny(
    // Rust Compiler Errors
    unreachable_code,
    improper_ctypes_definitions,
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms,
    clippy::perf,
    clippy::correctness,
    clippy::suspicious,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::missing_safety_doc,
    clippy::same_item_push,
    clippy::implicit_clone,
    clippy::all,
    clippy::pedantic,
    missing_docs,
    clippy::nursery,
    clippy::single_call_fn,
)]
// -------------------------------------------------------------------------
// LEVEL 2: STYLE WARNINGS (Warn)
// -------------------------------------------------------------------------
#![warn(
    // For `no-std` Situation Issues
    dead_code,
    warnings,
    unsafe_code,
    clippy::dbg_macro,
    clippy::todo,
    clippy::unnecessary_safety_comment
)]
// -------------------------------------------------------------------------
// LEVEL 3: ALLOW/IGNORABLE (Allow)
// -------------------------------------------------------------------------
#![allow(
    clippy::restriction,
    clippy::inline_always,
    unused_doc_comments,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::empty_line_after_doc_comments
)]
#![crate_name = "bincode_next"]
#![crate_type = "rlib"]

#[cfg(feature = "alloc")]
extern crate alloc;
#[cfg(any(feature = "std", test))]
extern crate std;

mod atomic;
#[doc(hidden)]
pub mod error_path;
mod features;
#[doc(hidden)]
pub mod utils;
pub(crate) mod varint;

use de::Decoder;
use de::read::Reader;
use enc::write::Writer;

#[cfg(any(
    feature = "alloc",
    feature = "std",
    feature = "derive",
    feature = "serde",
    feature = "zero-copy",
    feature = "static-size"
))]
pub use features::*;

/// The major version of the bincode library.
pub const BINCODE_MAJOR_VERSION: u64 = 3;

#[doc(hidden)]
pub use rapidhash;

pub mod config;
/// Fingerprinting support for schema verification.
pub mod fingerprint;

#[macro_use]
pub mod de;
pub mod enc;
#[macro_use]
pub mod error;

#[cfg(feature = "static-size")]
pub use static_size::StaticSize;

pub use de::BorrowDecode;
pub use de::Decode;
pub use enc::Encode;
pub use fingerprint::Fingerprint;
/// Relative pointer system for zero-copy nested structures
#[cfg(feature = "zero-copy")]
pub mod zero_copy {
    pub use crate::relative_ptr::*;
}
#[cfg(feature = "zero-copy")]
#[doc(hidden)]
pub use crate::relative_ptr::*;

use config::Config;
use config::internal::InternalFingerprintGuard;

/// Encode the given value into the given slice. Returns the amount of bytes that have been written.
///
/// See the [config] module for more information on configurations.
///
/// # Errors
///
/// Returns an `EncodeError` if the slice is too small or the value cannot be encoded.
///
/// [config]: config/index.html
#[inline]
pub fn encode_into_slice<E: enc::Encode, C: Config>(
    val: E,
    dst: &mut [u8],
    config: C,
) -> Result<usize, error::EncodeError>
where
    C::Mode: config::InternalFingerprintGuard<E, C>,
{
    let mut writer = enc::write::SliceWriter::new(dst);
    C::Mode::encode_check(&config, &mut writer)?;
    let mut encoder = enc::EncoderImpl::<_, C>::new(writer, config);
    val.encode(&mut encoder)?;
    Ok(encoder.into_writer().bytes_written())
}

/// Encode the given value into a custom [`Writer`\].
///
/// See the [config] module for more information on configurations.
///
/// # Errors
///
/// Returns an `EncodeError` if the writer fails or the value cannot be encoded.
///
/// [config]: config/index.html
#[inline]
pub fn encode_into_writer<E: enc::Encode, W: Writer, C: Config>(
    val: E,
    mut writer: W,
    config: C,
) -> Result<(), error::EncodeError>
where
    C::Mode: config::InternalFingerprintGuard<E, C>,
{
    C::Mode::encode_check(&config, &mut writer)?;
    let mut encoder = enc::EncoderImpl::<_, C>::new(writer, config);
    val.encode(&mut encoder)?;
    Ok(())
}

/// Attempt to decode a given type `D` from the given slice. Returns the decoded output and the amount of bytes read.
///
/// Note that this does not work with borrowed types like `&str` or `&[u8]`. For that use [`borrow_decode_from_slice`\].
///
/// See the [config] module for more information on configurations.
///
/// # Errors
///
/// Returns a `DecodeError` if the slice is too small or the data is invalid.
///
/// [config]: config/index.html
#[inline(always)]
pub fn decode_from_slice<D: de::Decode<()>, C: Config>(
    src: &[u8],
    config: C,
) -> Result<(D, usize), error::DecodeError>
where
    C::Mode: config::InternalFingerprintGuard<D, C>,
{
    decode_from_slice_with_context(src, config, ())
}

/// Attempt to decode a given type `D` from the given slice with `Context`. Returns the decoded output and the amount of bytes read.
///
/// Note that this does not work with borrowed types like `&str` or `&[u8]`. For that use [`borrow_decode_from_slice`\].
///
/// See the [config] module for more information on configurations.
///
/// # Errors
///
/// Returns a `DecodeError` if the slice is too small or the data is invalid.
///
/// [config]: config/index.html
#[inline]
pub fn decode_from_slice_with_context<Context, D: de::Decode<Context>, C: Config>(
    src: &[u8],
    config: C,
    context: Context,
) -> Result<(D, usize), error::DecodeError>
where
    C::Mode: config::InternalFingerprintGuard<D, C>,
{
    let mut reader = de::read::SliceReader::new(src);
    C::Mode::decode_check(&config, &mut reader)?;
    let mut decoder = de::DecoderImpl::<_, C, Context>::new(reader, config, context);
    let result = D::decode(&mut decoder)?;
    let bytes_read = src.len() - decoder.reader().slice.len();
    Ok((result, bytes_read))
}

/// Attempt to decode a given type `D` from the given slice. Returns the decoded output and the amount of bytes read.
///
/// See the [config] module for more information on configurations.
///
/// # Errors
///
/// Returns a `DecodeError` if the slice is too small or the data is invalid.
///
/// [config]: config/index.html
#[inline(always)]
pub fn borrow_decode_from_slice<'a, D: de::BorrowDecode<'a, ()>, C: Config>(
    src: &'a [u8],
    config: C,
) -> Result<(D, usize), error::DecodeError>
where
    C::Mode: config::InternalFingerprintGuard<D, C>,
{
    borrow_decode_from_slice_with_context(src, config, ())
}

/// Attempt to decode a given type `D` from the given slice with `Context`. Returns the decoded output and the amount of bytes read.
///
/// See the [config] module for more information on configurations.
///
/// # Errors
///
/// Returns a `DecodeError` if the slice is too small or the data is invalid.
///
/// [config]: config/index.html
#[inline]
pub fn borrow_decode_from_slice_with_context<
    'a,
    Context,
    D: de::BorrowDecode<'a, Context>,
    C: Config,
>(
    src: &'a [u8],
    config: C,
    context: Context,
) -> Result<(D, usize), error::DecodeError>
where
    C::Mode: config::InternalFingerprintGuard<D, C>,
{
    let mut reader = de::read::SliceReader::new(src);
    C::Mode::decode_check(&config, &mut reader)?;
    let mut decoder = de::DecoderImpl::<_, C, Context>::new(reader, config, context);
    let result = D::borrow_decode(&mut decoder)?;
    let bytes_read = src.len() - decoder.reader().slice.len();
    Ok((result, bytes_read))
}

/// Attempt to decode a given type `D` from the given slice with a compile-time bound check.
///
/// This function ensures that the target type `D` cannot exceed the provided buffer capacity `CAP` at compile-time.
///
/// # Errors
///
/// Returns a `DecodeError` if the slice contains invalid data.
#[cfg(feature = "static-size")]
#[inline(always)]
pub fn decode_from_slice_static<D, const CAP: usize, C>(
    src: &[u8; CAP],
    config: C,
) -> Result<D, error::DecodeError>
where
    D: de::Decode<()> + static_size::StaticSize,
    C: Config,
    C::Mode: config::InternalFingerprintGuard<D, C>,
{
    const {
        assert!(D::MAX_SIZE <= CAP, "Buffer too small for target type");
    }
    let (val, _) = decode_from_slice(src, config)?;
    Ok(val)
}

/// Attempt to decode a given type `D` from the given slice with a compile-time bound check and a
/// decoding context.
///
/// This function ensures that the target type `D` cannot exceed the provided buffer capacity `CAP`
/// at compile-time.
///
/// # Errors
///
/// Returns a `DecodeError` if the slice contains invalid data.
#[cfg(feature = "static-size")]
#[inline(always)]
pub fn decode_from_slice_static_with_context<Context, D, const CAP: usize, C>(
    src: &[u8; CAP],
    config: C,
    context: Context,
) -> Result<D, error::DecodeError>
where
    D: de::Decode<Context> + static_size::StaticSize,
    C: Config,
    C::Mode: config::InternalFingerprintGuard<D, C>,
{
    const {
        assert!(D::MAX_SIZE <= CAP, "Buffer too small for target type");
    }
    let (val, _) = decode_from_slice_with_context(src, config, context)?;
    Ok(val)
}

/// Attempt to decode a given type `D` from the given slice with a compile-time bound check.
///
/// This function ensures that the target type `D` cannot exceed the provided buffer capacity `CAP`
/// at compile-time.
///
/// # Errors
///
/// Returns a `DecodeError` if the slice contains invalid data.
#[cfg(feature = "static-size")]
#[inline(always)]
pub fn borrow_decode_from_slice_static<'a, D, const CAP: usize, C>(
    src: &'a [u8; CAP],
    config: C,
) -> Result<D, error::DecodeError>
where
    D: de::BorrowDecode<'a, ()> + static_size::StaticSize,
    C: Config,
    C::Mode: config::InternalFingerprintGuard<D, C>,
{
    const {
        assert!(D::MAX_SIZE <= CAP, "Buffer too small for target type");
    }
    let (val, _) = borrow_decode_from_slice(src, config)?;
    Ok(val)
}

/// Attempt to borrow-decode a given type `D` from the given slice with a compile-time bound check
/// and a decoding context.
///
/// This function ensures that the target type `D` cannot exceed the provided buffer capacity `CAP`
/// at compile-time.
///
/// # Errors
///
/// Returns a `DecodeError` if the slice contains invalid data.
#[cfg(feature = "static-size")]
#[inline(always)]
pub fn borrow_decode_from_slice_static_with_context<'a, Context, D, const CAP: usize, C>(
    src: &'a [u8; CAP],
    config: C,
    context: Context,
) -> Result<D, error::DecodeError>
where
    D: de::BorrowDecode<'a, Context> + static_size::StaticSize,
    C: Config,
    C::Mode: config::InternalFingerprintGuard<D, C>,
{
    const {
        assert!(D::MAX_SIZE <= CAP, "Buffer too small for target type");
    }
    let (val, _) = borrow_decode_from_slice_with_context(src, config, context)?;
    Ok(val)
}

/// Attempt to decode a given type `D` from the given [`Reader`\].
///
/// See the [config] module for more information on configurations.
///
/// # Errors
///
/// Returns a `DecodeError` if the reader fails or the data is invalid.
///
/// [config]: config/index.html
#[inline]
pub fn decode_from_reader<D: de::Decode<()>, R: Reader, C: Config>(
    mut reader: R,
    config: C,
) -> Result<D, error::DecodeError>
where
    C::Mode: config::InternalFingerprintGuard<D, C>,
{
    C::Mode::decode_check(&config, &mut reader)?;
    let mut decoder = de::DecoderImpl::<_, C, ()>::new(reader, config, ());
    D::decode(&mut decoder)
}

/// Attempt to decode a given type `T` from the given async reader safely using a non-blocking fiber.
///
/// Requires the `async-fiber` feature.
///
/// # Errors
///
/// Returns a `DecodeError` if the reader fails or the data is invalid.
///
/// [config]: config/index.html
#[cfg(feature = "async-fiber")]
#[inline(always)]
pub async fn decode_async<T, R, C>(
    config: C,
    reader: R,
) -> Result<T, crate::error::DecodeError>
where
    T: crate::Decode<()>,
    R: futures_io::AsyncRead + std::marker::Unpin,
    C: crate::config::Config,
    C::Mode: crate::config::InternalFingerprintGuard<T, C>,
{
    decode_async_with_context::<T, R, C, ()>(config, reader, ()).await
}

/// Attempt to decode a given type `T` from the given async reader using a non-blocking fiber and a context.
///
/// This is the primary implementation for runtimes that use the `futures-io` traits (e.g. `async-std`, `smol`).
///
/// Requires the `async-fiber` feature.
///
/// # Errors
///
/// Returns a `DecodeError` if the reader fails or the data is invalid.
///
/// [config]: config/index.html
#[cfg(feature = "async-fiber")]
#[inline]
pub async fn decode_async_with_context<T, R, C, Context>(
    config: C,
    reader: R,
    context: Context,
) -> Result<T, crate::error::DecodeError>
where
    T: crate::Decode<Context>,
    R: futures_io::AsyncRead + std::marker::Unpin,
    C: crate::config::Config,
    C::Mode: crate::config::InternalFingerprintGuard<T, C>,
{
    let bridge = crate::de::async_fiber::AsyncFiberBridge::new(reader);
    bridge
        .run(move |fiber_reader| {
            C::Mode::decode_check(&config, fiber_reader)?;
            let mut decoder =
                crate::de::DecoderImpl::<_, C, Context>::new(fiber_reader, config, context);
            T::decode(&mut decoder)
        })
        .await
}

/// Attempt to decode a given type `T` from the given tokio async reader safely using a non-blocking fiber.
///
/// Requires the `tokio` and `async-fiber` features.
///
/// # Errors
///
/// Returns a `DecodeError` if the reader fails or the data is invalid.
///
/// [config]: config/index.html
#[cfg(all(feature = "tokio", feature = "async-fiber"))]
#[inline(always)]
pub async fn decode_async_tokio<T, R, C>(
    config: C,
    reader: R,
) -> Result<T, crate::error::DecodeError>
where
    T: crate::Decode<()>,
    R: tokio::io::AsyncRead + std::marker::Unpin,
    C: crate::config::Config,
    C::Mode: crate::config::InternalFingerprintGuard<T, C>,
{
    decode_async_tokio_with_context::<T, R, C, ()>(config, reader, ()).await
}

/// Attempt to decode a given type `T` from the given tokio async reader using a non-blocking fiber and a context.
///
/// Requires the `tokio` and `async-fiber` features.
///
/// # Errors
///
/// Returns a `DecodeError` if the reader fails or the data is invalid.
///
/// [config]: config/index.html
#[cfg(all(feature = "tokio", feature = "async-fiber"))]
#[inline(always)]
pub async fn decode_async_tokio_with_context<T, R, C, Context>(
    config: C,
    reader: R,
    context: Context,
) -> Result<T, crate::error::DecodeError>
where
    T: crate::Decode<Context>,
    R: tokio::io::AsyncRead + std::marker::Unpin,
    C: crate::config::Config,
    C::Mode: crate::config::InternalFingerprintGuard<T, C>,
{
    let reader = crate::de::async_fiber::TokioReader(reader);
    decode_async_with_context::<T, _, C, Context>(config, reader, context).await
}

/// Attempt to decode a given serde-compatible type `T` from the given async reader safely using a non-blocking fiber.
///
/// Requires the `async-fiber` and `serde` features.
///
/// # Errors
///
/// Returns a `DecodeError` if the reader fails or the data is invalid.
///
/// [config]: config/index.html
#[cfg(all(feature = "async-fiber", feature = "serde"))]
#[inline(always)]
#[doc(hidden)]
pub async fn decode_serde_async<'de, T, R, C>(
    config: C,
    reader: R,
) -> Result<T, crate::error::DecodeError>
where
    T: ::serde::Deserialize<'de>,
    R: futures_io::AsyncRead + std::marker::Unpin,
    C: crate::config::Config,
{
    decode_serde_async_with_context::<'de, T, R, C, ()>(config, reader, ()).await
}

/// Attempt to decode a given serde-compatible type `T` from the given async reader using a non-blocking fiber and a context.
///
/// Requires the `async-fiber` and `serde` features.
///
/// # Errors
///
/// Returns a `DecodeError` if the reader fails or the data is invalid.
///
/// [config]: config/index.html
#[cfg(all(feature = "async-fiber", feature = "serde"))]
#[inline]
#[doc(hidden)]
pub async fn decode_serde_async_with_context<'de, T, R, C, Context>(
    config: C,
    reader: R,
    context: Context,
) -> Result<T, crate::error::DecodeError>
where
    T: ::serde::Deserialize<'de>,
    R: futures_io::AsyncRead + std::marker::Unpin,
    C: crate::config::Config,
{
    let bridge = crate::de::async_fiber::AsyncFiberBridge::new(reader);
    bridge
        .run(move |fiber_reader| {
            let decoder =
                crate::de::DecoderImpl::<_, C, Context>::new(fiber_reader, config, context);
            let mut serde_decoder = crate::features::serde::OwnedSerdeDecoder { de: decoder };
            T::deserialize(serde_decoder.as_deserializer())
        })
        .await
}

/// Attempt to decode a given serde-compatible type `T` from the given tokio async reader safely using a non-blocking fiber.
///
/// Requires the `tokio`, `async-fiber` and `serde` features.
///
/// # Errors
///
/// Returns a `DecodeError` if the reader fails or the data is invalid.
///
/// [config]: config/index.html
#[cfg(all(feature = "tokio", feature = "async-fiber", feature = "serde"))]
#[inline(always)]
#[doc(hidden)]
pub async fn decode_serde_tokio_async<'de, T, R, C>(
    config: C,
    reader: R,
) -> Result<T, crate::error::DecodeError>
where
    T: ::serde::Deserialize<'de>,
    R: tokio::io::AsyncRead + std::marker::Unpin,
    C: crate::config::Config,
{
    decode_serde_tokio_async_with_context::<'de, T, R, C, ()>(config, reader, ()).await
}

/// Attempt to decode a given serde-compatible type `T` from the given tokio async reader using a non-blocking fiber and a context.
///
/// Requires the `tokio`, `async-fiber` and `serde` features.
///
/// # Errors
///
/// Returns a `DecodeError` if the reader fails or the data is invalid.
///
/// [config]: config/index.html
#[cfg(all(feature = "tokio", feature = "async-fiber", feature = "serde"))]
#[inline(always)]
#[doc(hidden)]
pub async fn decode_serde_tokio_async_with_context<'de, T, R, C, Context>(
    config: C,
    reader: R,
    context: Context,
) -> Result<T, crate::error::DecodeError>
where
    T: ::serde::Deserialize<'de>,
    R: tokio::io::AsyncRead + std::marker::Unpin,
    C: crate::config::Config,
{
    let reader = crate::de::async_fiber::TokioReader(reader);
    decode_serde_async_with_context::<'de, T, _, C, Context>(config, reader, context).await
}

#[cfg(all(feature = "alloc", feature = "derive", doc))]
pub mod spec {
    #![doc = include_str!("../docs/spec.md")]
}

#[cfg(doc)]
pub mod migration_guide {
    #![doc = include_str!("../docs/migration_guide.md")]
}

// Test the examples in readme.md
#[cfg(all(
    feature = "std",
    feature = "derive",
    feature = "serde",
    feature = "async-fiber",
    feature = "zero-copy",
    feature = "static-size",
    doctest
))]
#[cfg_attr(miri, ignore)]
mod readme {
    #![doc = include_str!("../README.md")]
    #![doc = include_str!("../derive/readme.md")]
}
