#![cfg(all(feature = "std", feature = "derive"))]

//! Comprehensive property-based round-trip tests for bincode.
//!
//! Tests encode→decode round-trips across all config combinations:
//! - Bincode: Variable LE, Variable BE, Fixed LE, Fixed BE
//! - CBOR, CBOR Deterministic
//!
//! Covers: primitives, containers, enums, nested types, edge cases.

extern crate bincode_next as bincode;

use bincode::Decode;
use bincode::Encode;
use bincode::config::Config;
use bincode::error::DecodeError;
use proptest::prelude::*;
use std::collections::BTreeMap;
use std::collections::BinaryHeap;
use std::collections::VecDeque;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Round-trip encode then decode, asserting equality and byte consumption.
fn roundtrip<T, C>(
    val: &T,
    config: C,
) where
    T: Encode + Decode<()> + PartialEq + core::fmt::Debug + 'static,
    C: Config,
    for<'a> <C as bincode::config::InternalFingerprintConfigExt>::Mode:
        bincode::config::InternalFingerprintGuard<&'a T, C>,
    <C as bincode::config::InternalFingerprintConfigExt>::Mode:
        bincode::config::InternalFingerprintGuard<T, C>,
{
    let encoded = bincode::encode_to_vec(val, config).expect("encode failed");
    let (decoded, len): (T, usize) =
        bincode::decode_from_slice(&encoded, config).expect("decode failed");
    assert_eq!(
        val,
        &decoded,
        "round-trip mismatch\nbytes: {:?}",
        &encoded[..]
    );
    assert_eq!(
        encoded.len(),
        len,
        "did not consume all bytes\nbytes: {:?}",
        &encoded[..]
    );
}

/// Round-trip with a custom comparator (for f32/f64 which need epsilon or bitwise).
fn roundtrip_cmp<T, C>(
    val: &T,
    config: C,
    cmp: impl Fn(&T, &T) -> bool,
) where
    T: Encode + Decode<()> + core::fmt::Debug + 'static,
    C: Config,
    for<'a> <C as bincode::config::InternalFingerprintConfigExt>::Mode:
        bincode::config::InternalFingerprintGuard<&'a T, C>,
    <C as bincode::config::InternalFingerprintConfigExt>::Mode:
        bincode::config::InternalFingerprintGuard<T, C>,
{
    let encoded = bincode::encode_to_vec(val, config).expect("encode failed");
    let (decoded, len): (T, usize) =
        bincode::decode_from_slice(&encoded, config).expect("decode failed");
    assert!(
        cmp(val, &decoded),
        "round-trip mismatch\noriginal: {:?}\ndecoded:  {:?}\nbytes: {:?}",
        val,
        decoded,
        &encoded[..]
    );
    assert_eq!(encoded.len(), len);
}

/// Run round-trip across all **Bincode** config permutations.
fn roundtrip_bincode<T>(val: &T)
where
    T: Encode + Decode<()> + PartialEq + core::fmt::Debug + 'static,
{
    roundtrip(
        val,
        bincode::config::standard()
            .with_little_endian()
            .with_variable_int_encoding(),
    );
    roundtrip(
        val,
        bincode::config::standard()
            .with_big_endian()
            .with_variable_int_encoding(),
    );
    roundtrip(
        val,
        bincode::config::standard()
            .with_little_endian()
            .with_fixed_int_encoding(),
    );
    roundtrip(
        val,
        bincode::config::standard()
            .with_big_endian()
            .with_fixed_int_encoding(),
    );
}

/// Run round-trip across all **CBOR** config permutations.
/// Note: CBOR doesn't use endianness or int-encoding options.
fn roundtrip_cbor<T>(val: &T)
where
    T: Encode + Decode<()> + PartialEq + core::fmt::Debug + 'static,
{
    roundtrip(val, bincode::config::standard().with_cbor_format());
    roundtrip(val, bincode::config::standard().with_deterministic_cbor());
}

/// Run round-trip across **all** config permutations (Bincode + CBOR).
fn roundtrip_all<T>(val: &T)
where
    T: Encode + Decode<()> + PartialEq + core::fmt::Debug + 'static,
{
    roundtrip_bincode(val);
    roundtrip_cbor(val);
}

/// Float-aware round-trip across Bincode configs.
fn roundtrip_float_bincode<T>(
    val: &T,
    cmp: impl Fn(&T, &T) -> bool + Copy,
) where
    T: Encode + Decode<()> + core::fmt::Debug + 'static,
{
    roundtrip_cmp(
        val,
        bincode::config::standard()
            .with_little_endian()
            .with_variable_int_encoding(),
        cmp,
    );
    roundtrip_cmp(
        val,
        bincode::config::standard()
            .with_big_endian()
            .with_variable_int_encoding(),
        cmp,
    );
    roundtrip_cmp(
        val,
        bincode::config::standard()
            .with_little_endian()
            .with_fixed_int_encoding(),
        cmp,
    );
    roundtrip_cmp(
        val,
        bincode::config::standard()
            .with_big_endian()
            .with_fixed_int_encoding(),
        cmp,
    );
}

/// Float-aware round-trip across CBOR configs.
fn roundtrip_float_cbor<T>(
    val: &T,
    cmp: impl Fn(&T, &T) -> bool + Copy,
) where
    T: Encode + Decode<()> + core::fmt::Debug + 'static,
{
    roundtrip_cmp(val, bincode::config::standard().with_cbor_format(), cmp);
    roundtrip_cmp(
        val,
        bincode::config::standard().with_deterministic_cbor(),
        cmp,
    );
}

/// Float-aware round-trip across all configs.
#[allow(dead_code)]
fn roundtrip_float_all<T>(
    val: &T,
    cmp: impl Fn(&T, &T) -> bool + Copy,
) where
    T: Encode + Decode<()> + core::fmt::Debug + 'static,
{
    roundtrip_float_bincode(val, cmp);
    roundtrip_float_cbor(val, cmp);
}

#[allow(dead_code)]
fn proptest_cases() -> u32 {
    if std::env::var("MIRIFLAGS").is_ok() {
        2
    } else {
        512
    }
}

// ---------------------------------------------------------------------------
// Proptest: Primitives — Bincode
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig { cases: if std::env::var("MIRIFLAGS").is_ok() { 2 } else { 1000 }, .. ProptestConfig::default() })]

    // --- Unsigned integers ---
    #[test]
    fn bincode_u8(v in any::<u8>()) { roundtrip_bincode(&v); }
    #[test]
    fn bincode_u16(v in any::<u16>()) { roundtrip_bincode(&v); }
    #[test]
    fn bincode_u32(v in any::<u32>()) { roundtrip_bincode(&v); }
    #[test]
    fn bincode_u64(v in any::<u64>()) { roundtrip_bincode(&v); }
    #[test]
    fn bincode_u128(v in any::<u128>()) { roundtrip_bincode(&v); }
    #[test]
    fn bincode_usize(v in any::<u64>()) {
        // usize encoded as u64; constrain so it fits
        let v = v as usize;
        roundtrip_bincode(&v);
    }

    // --- Signed integers ---
    #[test]
    fn bincode_i8(v in any::<i8>()) { roundtrip_bincode(&v); }
    #[test]
    fn bincode_i16(v in any::<i16>()) { roundtrip_bincode(&v); }
    #[test]
    fn bincode_i32(v in any::<i32>()) { roundtrip_bincode(&v); }
    #[test]
    fn bincode_i64(v in any::<i64>()) { roundtrip_bincode(&v); }
    #[test]
    fn bincode_i128(v in any::<i128>()) { roundtrip_bincode(&v); }
    #[test]
    fn bincode_isize(v in any::<i64>()) {
        let v = v as isize;
        roundtrip_bincode(&v);
    }

    // --- Floats ---
    #[test]
    fn bincode_f32(v in any::<f32>()) {
        roundtrip_float_bincode(&v, |a, b| a.to_bits() == b.to_bits());
    }
    #[test]
    fn bincode_f64(v in any::<f64>()) {
        roundtrip_float_bincode(&v, |a, b| a.to_bits() == b.to_bits());
    }

    // --- Bool / Char ---
    #[test]
    fn bincode_bool(v in any::<bool>()) { roundtrip_bincode(&v); }
    #[test]
    fn bincode_char(v in any::<char>()) { roundtrip_bincode(&v); }

    // --- String ---
    #[test]
    fn bincode_string(v in ".*") { roundtrip_bincode(&v); }
}

// ---------------------------------------------------------------------------
// Proptest: Primitives — CBOR
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig { cases: if std::env::var("MIRIFLAGS").is_ok() { 2 } else { 1000 }, .. ProptestConfig::default() })]

    // --- Unsigned integers ---
    #[test]
    fn cbor_u8(v in any::<u8>()) { roundtrip_cbor(&v); }
    #[test]
    fn cbor_u16(v in any::<u16>()) { roundtrip_cbor(&v); }
    #[test]
    fn cbor_u32(v in any::<u32>()) { roundtrip_cbor(&v); }
    #[test]
    fn cbor_u64(v in any::<u64>()) { roundtrip_cbor(&v); }
    #[test]
    fn cbor_u128(v in any::<u128>()) { roundtrip_cbor(&v); }

    // --- Signed integers ---
    #[test]
    fn cbor_i8(v in any::<i8>()) { roundtrip_cbor(&v); }
    #[test]
    fn cbor_i16(v in any::<i16>()) { roundtrip_cbor(&v); }
    #[test]
    fn cbor_i32(v in any::<i32>()) { roundtrip_cbor(&v); }
    #[test]
    fn cbor_i64(v in any::<i64>()) { roundtrip_cbor(&v); }
    #[test]
    fn cbor_i128(v in any::<i128>()) { roundtrip_cbor(&v); }

    // --- Floats ---
    #[test]
    fn cbor_f32(v in any::<f32>()) {
        roundtrip_float_cbor(&v, |a, b| a.to_bits() == b.to_bits());
    }
    #[test]
    fn cbor_f64(v in any::<f64>()) {
        roundtrip_float_cbor(&v, |a, b| a.to_bits() == b.to_bits());
    }

    // --- Bool / Char ---
    #[test]
    fn cbor_bool(v in any::<bool>()) { roundtrip_cbor(&v); }
    #[test]
    fn cbor_char(v in any::<char>()) { roundtrip_cbor(&v); }

    // --- String ---
    #[test]
    fn cbor_string(v in ".*") { roundtrip_cbor(&v); }
}

// ---------------------------------------------------------------------------
// Proptest: Containers — All configs
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig { cases: if std::env::var("MIRIFLAGS").is_ok() { 2 } else { 1000 }, .. ProptestConfig::default() })]

    // --- Vec ---
    #[test]
    fn all_vec_u8(v in proptest::collection::vec(any::<u8>(), 0..64)) {
        roundtrip_all(&v);
    }
    #[test]
    fn all_vec_u32(v in proptest::collection::vec(any::<u32>(), 0..32)) {
        roundtrip_all(&v);
    }
    #[test]
    fn all_vec_i64(v in proptest::collection::vec(any::<i64>(), 0..16)) {
        roundtrip_all(&v);
    }
    #[test]
    fn all_vec_string(v in proptest::collection::vec(".*", 0..8)) {
        roundtrip_all(&v);
    }

    // --- VecDeque ---
    #[test]
    fn all_vecdeque_u32(v in proptest::collection::vec(any::<u32>(), 0..32)) {
        let vd: VecDeque<u32> = v.into_iter().collect();
        roundtrip_all(&vd);
    }

    // --- BTreeSet ---
    #[test]
    fn all_btreeset_i32(v in proptest::collection::btree_set(any::<i32>(), 0..32)) {
        roundtrip_all(&v);
    }

    // --- BTreeMap ---
    #[test]
    fn all_btreemap_string_u32(
        v in proptest::collection::btree_map("[a-z]{0,8}", any::<u32>(), 0..16)
    ) {
        roundtrip_all(&v);
    }
    #[test]
    fn all_btreemap_i32_i32(
        v in proptest::collection::btree_map(any::<i32>(), any::<i32>(), 0..16)
    ) {
        roundtrip_all(&v);
    }

    // --- BinaryHeap (encode only uses as_slice; decode reconstructs) ---
    // BinaryHeap does not have PartialEq, so compare sorted vecs.
    #[test]
    fn all_binaryheap_u32(v in proptest::collection::vec(any::<u32>(), 0..32)) {
        let heap: BinaryHeap<u32> = v.into_iter().collect();

        macro_rules! test_config {
            ($config:expr) => {
                let config = $config;
                let encoded = bincode::encode_to_vec(&heap, config).expect("encode failed");
                let (decoded, len): (BinaryHeap<u32>, usize) =
                    bincode::decode_from_slice(&encoded, config).expect("decode failed");
                let mut orig: Vec<u32> = heap.iter().copied().collect();
                let mut dec: Vec<u32> = decoded.into_iter().collect();
                orig.sort();
                dec.sort();
                assert_eq!(orig, dec);
                assert_eq!(encoded.len(), len);
            };
        }

        test_config!(bincode::config::standard().with_little_endian().with_variable_int_encoding());
        test_config!(bincode::config::standard().with_big_endian().with_variable_int_encoding());
        test_config!(bincode::config::standard().with_little_endian().with_fixed_int_encoding());
        test_config!(bincode::config::standard().with_big_endian().with_fixed_int_encoding());
    }
}

// ---------------------------------------------------------------------------
// Proptest: Option / Result — All configs
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig { cases: if std::env::var("MIRIFLAGS").is_ok() { 2 } else { 1000 }, .. ProptestConfig::default() })]

    #[test]
    fn all_option_u32(v in proptest::option::of(any::<u32>())) {
        roundtrip_all(&v);
    }
    #[test]
    fn all_option_string(v in proptest::option::of(".*")) {
        roundtrip_all(&v);
    }
    #[test]
    fn all_option_none(_dummy in 0u8..1) {
        let v: Option<u64> = None;
        roundtrip_all(&v);
    }

    #[test]
    fn all_result_ok(v in any::<u32>()) {
        let r: Result<u32, u8> = Ok(v);
        roundtrip_all(&r);
    }
    #[test]
    fn all_result_err(v in any::<u8>()) {
        let r: Result<u32, u8> = Err(v);
        roundtrip_all(&r);
    }
    #[test]
    fn all_result_nested(v in any::<i32>()) {
        let r: Result<Option<i32>, String> = Ok(Some(v));
        roundtrip_all(&r);
    }
}

// ---------------------------------------------------------------------------
// Proptest: Tuples — All configs
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig { cases: if std::env::var("MIRIFLAGS").is_ok() { 2 } else { 1000 }, .. ProptestConfig::default() })]

    #[test]
    fn all_tuple2(a in any::<u8>(), b in any::<u32>()) {
        roundtrip_all(&(a, b));
    }
    #[test]
    fn all_tuple3(a in any::<bool>(), b in any::<i16>(), c in any::<u64>()) {
        roundtrip_all(&(a, b, c));
    }
    #[test]
    fn all_tuple4(a in any::<u8>(), b in any::<u8>(), c in any::<u8>(), d in any::<u8>()) {
        roundtrip_all(&(a, b, c, d));
    }
    #[test]
    fn all_tuple_mixed(
        a in any::<bool>(),
        b in any::<u32>(),
        c in ".*",
        d in any::<i64>()
    ) {
        roundtrip_all(&(a, b, c, d));
    }
}

// ---------------------------------------------------------------------------
// Proptest: Duration / Ranges — All configs
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig { cases: if std::env::var("MIRIFLAGS").is_ok() { 2 } else { 1000 }, .. ProptestConfig::default() })]

    #[test]
    fn all_duration(secs in 0u64..u64::MAX, nanos in 0u32..1_000_000_000) {
        let d = Duration::new(secs, nanos);
        roundtrip_all(&d);
    }

    #[test]
    fn all_range_u8(a in any::<u8>(), b in any::<u8>()) {
        let lo = a.min(b);
        let hi = a.max(b);
        roundtrip_all(&(lo..hi));
    }

    #[test]
    fn all_range_inclusive_u8(a in any::<u8>(), b in any::<u8>()) {
        let lo = a.min(b);
        let hi = a.max(b);
        roundtrip_all(&(lo..=hi));
    }
}

// ---------------------------------------------------------------------------
// Proptest: Arrays — All configs
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig { cases: if std::env::var("MIRIFLAGS").is_ok() { 2 } else { 1000 }, .. ProptestConfig::default() })]

    #[test]
    fn all_array_u8_16(v in any::<[u8; 16]>()) {
        roundtrip_all(&v);
    }
    #[test]
    fn all_array_u32_4(v in any::<[u32; 4]>()) {
        roundtrip_all(&v);
    }
    #[test]
    fn all_array_i64_2(v in any::<[i64; 2]>()) {
        roundtrip_all(&v);
    }
}

// ---------------------------------------------------------------------------
// Proptest: Nested / Composite types — All configs
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig { cases: if std::env::var("MIRIFLAGS").is_ok() { 2 } else { 1000 }, .. ProptestConfig::default() })]

    #[test]
    fn all_vec_option_u32(
        v in proptest::collection::vec(proptest::option::of(any::<u32>()), 0..16)
    ) {
        roundtrip_all(&v);
    }

    #[test]
    fn all_vec_vec_u8(
        v in proptest::collection::vec(
            proptest::collection::vec(any::<u8>(), 0..16),
            0..8,
        )
    ) {
        roundtrip_all(&v);
    }

    #[test]
    fn all_btreemap_string_vec_u32(
        v in proptest::collection::btree_map(
            "[a-z]{0,6}",
            proptest::collection::vec(any::<u32>(), 0..8),
            0..8,
        )
    ) {
        roundtrip_all(&v);
    }

    #[test]
    fn all_option_vec_string(
        v in proptest::option::of(
            proptest::collection::vec("[a-z]{0,8}", 0..4),
        )
    ) {
        roundtrip_all(&v);
    }

    #[test]
    fn all_result_vec_btreemap(
        ok in proptest::collection::vec(any::<u8>(), 0..16),
        err in proptest::collection::btree_map(any::<i32>(), any::<i32>(), 0..4),
        is_ok in any::<bool>(),
    ) {
        let v: Result<Vec<u8>, BTreeMap<i32, i32>> = if is_ok { Ok(ok) } else { Err(err) };
        roundtrip_all(&v);
    }
}

// ---------------------------------------------------------------------------
// Proptest: Edge cases for 128-bit integers in CBOR
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig { cases: if std::env::var("MIRIFLAGS").is_ok() { 2 } else { 1000 }, .. ProptestConfig::default() })]

    /// Test u128 values specifically in the bignum range (> u64::MAX)
    #[test]
    fn cbor_u128_bignum(hi in 1u64.., lo in any::<u64>()) {
        let val = ((hi as u128) << 64) | (lo as u128);
        roundtrip_cbor(&val);
    }

    /// Test i128 values in the negative bignum range (< i64::MIN)
    #[test]
    fn cbor_i128_neg_bignum(hi in 1u64.., lo in any::<u64>()) {
        // Construct a large negative: -(big magnitude) - 1
        let magnitude = ((hi as u128) << 64) | (lo as u128);
        // Clamp to valid i128 range
        if magnitude <= i128::MAX as u128 {
            let val = -1i128 - (magnitude as i128);
            roundtrip_cbor(&val);
        }
    }

    /// Test i128 boundary values
    #[test]
    fn cbor_i128_boundaries(_dummy in 0u8..1) {
        roundtrip_cbor(&0i128);
        roundtrip_cbor(&1i128);
        roundtrip_cbor(&(-1i128));
        roundtrip_cbor(&i128::MAX);
        roundtrip_cbor(&i128::MIN);
        roundtrip_cbor(&(i64::MAX as i128));
        roundtrip_cbor(&(i64::MIN as i128));
        roundtrip_cbor(&(i64::MAX as i128 + 1));
        roundtrip_cbor(&(i64::MIN as i128 - 1));
    }

    /// Test u128 boundary values
    #[test]
    fn cbor_u128_boundaries(_dummy in 0u8..1) {
        roundtrip_cbor(&0u128);
        roundtrip_cbor(&23u128);
        roundtrip_cbor(&24u128);
        roundtrip_cbor(&255u128);
        roundtrip_cbor(&256u128);
        roundtrip_cbor(&(u16::MAX as u128));
        roundtrip_cbor(&(u32::MAX as u128));
        roundtrip_cbor(&(u64::MAX as u128));
        roundtrip_cbor(&(u64::MAX as u128 + 1));
        roundtrip_cbor(&u128::MAX);
    }
}

// ---------------------------------------------------------------------------
// Proptest: Deterministic CBOR produces identical encoding for same input
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig { cases: if std::env::var("MIRIFLAGS").is_ok() { 2 } else { 1000 }, .. ProptestConfig::default() })]

    #[test]
    fn cbor_deterministic_u64(v in any::<u64>()) {
        let config = bincode::config::standard().with_deterministic_cbor();
        let a = bincode::encode_to_vec(v, config).unwrap();
        let b = bincode::encode_to_vec(v, config).unwrap();
        assert_eq!(a, b, "deterministic CBOR should produce identical output");
    }

    #[test]
    fn cbor_deterministic_string(v in ".*") {
        let config = bincode::config::standard().with_deterministic_cbor();
        let a = bincode::encode_to_vec(&v, config).unwrap();
        let b = bincode::encode_to_vec(&v, config).unwrap();
        assert_eq!(a, b, "deterministic CBOR should produce identical output");
    }

    #[test]
    fn cbor_deterministic_btreemap(
        v in proptest::collection::btree_map("[a-z]{0,4}", any::<u32>(), 0..8)
    ) {
        let config = bincode::config::standard().with_deterministic_cbor();
        let a = bincode::encode_to_vec(&v, config).unwrap();
        let b = bincode::encode_to_vec(&v, config).unwrap();
        assert_eq!(a, b, "deterministic CBOR should produce identical output");
    }
}

// ---------------------------------------------------------------------------
// Proptest: Cross-format sanity (decode truncated data → error, not panic)
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig { cases: if std::env::var("MIRIFLAGS").is_ok() { 2 } else { 1000 }, .. ProptestConfig::default() })]

    /// Truncating encoded data should produce a DecodeError, never a panic.
    #[test]
    fn bincode_truncated_no_panic(v in any::<u64>(), trunc in 1usize..8) {
        let config = bincode::config::standard();
        let encoded = bincode::encode_to_vec(v, config).unwrap();
        if trunc < encoded.len() {
            let result: Result<(u64, usize), _> =
                bincode::decode_from_slice(&encoded[..encoded.len() - trunc], config);
            assert!(result.is_err());
        }
    }

    /// Same for CBOR.
    #[test]
    fn cbor_truncated_no_panic(v in any::<u64>(), trunc in 1usize..8) {
        let config = bincode::config::standard().with_cbor_format();
        let encoded = bincode::encode_to_vec(v, config).unwrap();
        if trunc < encoded.len() {
            let result: Result<(u64, usize), _> =
                bincode::decode_from_slice(&encoded[..encoded.len() - trunc], config);
            assert!(result.is_err());
        }
    }

    /// Decoding completely random bytes should not panic (may succeed or error).
    #[test]
    fn bincode_random_bytes_no_panic(bytes in proptest::collection::vec(any::<u8>(), 0..64)) {
        let config = bincode::config::standard();
        let _: Result<(u32, usize), _> = bincode::decode_from_slice(&bytes, config);
        // We just care that it doesn't panic
    }

    #[test]
    fn cbor_random_bytes_no_panic(bytes in proptest::collection::vec(any::<u8>(), 0..64)) {
        let config = bincode::config::standard().with_cbor_format();
        let _: Result<(u32, usize), _> = bincode::decode_from_slice(&bytes, config);
    }
}

// ---------------------------------------------------------------------------
// Proptest: Limit enforcement
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig { cases: if std::env::var("MIRIFLAGS").is_ok() { 2 } else { 1000 }, .. ProptestConfig::default() })]

    /// Encoding with a byte limit should fail gracefully for large data.
    #[test]
    fn bincode_limit_enforcement(
        v in proptest::collection::vec(any::<u32>(), 10..64),
    ) {
        let config = bincode::config::standard().with_limit::<16>();
        let encoded = bincode::encode_to_vec(&v, config);
        // Encoding always succeeds (limits are decode-side only)
        if let Ok(encoded) = encoded {
            let result: Result<(Vec<u32>, usize), _> =
                bincode::decode_from_slice(&encoded, config);
            // Decoding large vecs with a 16-byte limit should fail
            match result {
                | Err(DecodeError::LimitExceeded) => {},
                | Err(_) => {}, // any decode error is acceptable
                | Ok(_) => {
                    // Only ok if the data actually fits in 16 bytes
                    assert!(
                        encoded.len() <= 16,
                        "should have failed with limit exceeded"
                    );
                },
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Derived enum (covers derive macro variant encoding)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
enum TestEnum {
    Unit,
    Newtype(u32),
    Tuple(u8, String),
    Struct { x: i64, y: Vec<u8> },
}

fn arb_test_enum() -> impl Strategy<Value = TestEnum> {
    prop_oneof![
        Just(TestEnum::Unit),
        any::<u32>().prop_map(TestEnum::Newtype),
        (any::<u8>(), ".*").prop_map(|(a, b)| TestEnum::Tuple(a, b)),
        (any::<i64>(), proptest::collection::vec(any::<u8>(), 0..16))
            .prop_map(|(x, y)| TestEnum::Struct { x, y }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: if std::env::var("MIRIFLAGS").is_ok() { 2 } else { 1000 }, .. ProptestConfig::default() })]

    #[test]
    fn all_derived_enum(v in arb_test_enum()) {
        roundtrip_all(&v);
    }
}

// ---------------------------------------------------------------------------
// Derived struct (covers derive macro field encoding)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
struct TestStruct {
    a: u8,
    b: i32,
    c: String,
    d: Vec<u16>,
    e: Option<bool>,
    f: (u64, u64),
}

prop_compose! {
    fn arb_test_struct()(
        a in any::<u8>(),
        b in any::<i32>(),
        c in ".*",
        d in proptest::collection::vec(any::<u16>(), 0..8),
        e in proptest::option::of(any::<bool>()),
        f in (any::<u64>(), any::<u64>()),
    ) -> TestStruct {
        TestStruct { a, b, c, d, e, f }
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: if std::env::var("MIRIFLAGS").is_ok() { 2 } else { 1000 }, .. ProptestConfig::default() })]

    #[test]
    fn all_derived_struct(v in arb_test_struct()) {
        roundtrip_all(&v);
    }
}
