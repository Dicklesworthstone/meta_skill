#![cfg(all(feature = "std", feature = "derive"))]

use bincode::Decode;
use bincode::Encode;
use bincode::config::{
    self,
};
use bincode::decode_from_slice;
use bincode::encode_to_vec;
use bincode_next as bincode;
use proptest::prelude::*;

#[derive(Encode, Decode, Debug, PartialEq, Clone)]
struct TestDerive {
    a: u32,
    b: String,
}

#[test]
fn test_cbor_derive() {
    let config = config::standard().with_cbor_format();
    let val = TestDerive {
        a: 42,
        b: "hello".to_string(),
    };
    let encoded = encode_to_vec(&val, config).unwrap();
    // 0x82 (array len 2), 0x18, 0x2a (u32 42), 0x65 (string len 5), 'h','e','l','l','o'
    assert_eq!(
        encoded,
        [0x82, 0x18, 0x2a, 0x65, b'h', b'e', b'l', b'l', b'o']
    );
    let (decoded, _): (TestDerive, _) = decode_from_slice(&encoded, config).unwrap();
    assert_eq!(decoded, val);
}

#[test]
fn test_rfc8949_appendix_a_integers() {
    let config = config::standard().with_cbor_format();

    let cases: &[(u64, Vec<u8>)] = &[
        (0u64, vec![0x00]),
        (1u64, vec![0x01]),
        (10u64, vec![0x0a]),
        (23u64, vec![0x17]),
        (24u64, vec![0x18, 0x18]),
        (25u64, vec![0x18, 0x19]),
        (100u64, vec![0x18, 0x64]),
        (1000u64, vec![0x19, 0x03, 0xe8]),
        (1000000u64, vec![0x1a, 0x00, 0x0f, 0x42, 0x40]),
    ];

    for (val, expected) in cases {
        assert_eq!(&encode_to_vec(val, config).unwrap(), expected);
    }

    assert_eq!(encode_to_vec(-1i8, config).unwrap(), vec![0x20]);
    assert_eq!(encode_to_vec(-10i8, config).unwrap(), vec![0x29]);
    assert_eq!(encode_to_vec(-100i8, config).unwrap(), vec![0x38, 0x63]);
}

#[test]
fn test_rfc8949_appendix_a_floats() {
    let config = config::standard().with_cbor_format();

    // Preferred serialization enabled by default in my implementation
    assert_eq!(encode_to_vec(0.0f32, config).unwrap(), [0xf9, 0x00, 0x00]);
    assert_eq!(encode_to_vec(-0.0f32, config).unwrap(), [0xf9, 0x80, 0x00]);
    assert_eq!(encode_to_vec(1.0f32, config).unwrap(), [0xf9, 0x3c, 0x00]);
    assert_eq!(encode_to_vec(1.5f32, config).unwrap(), [0xf9, 0x3e, 0x00]);
    assert_eq!(
        encode_to_vec(65504.0f32, config).unwrap(),
        [0xf9, 0x7b, 0xff]
    );

    // fa47c35000 is 100000.0 as f32
    assert_eq!(
        encode_to_vec(100000.0f32, config).unwrap(),
        [0xfa, 0x47, 0xc3, 0x50, 0x00]
    );

    assert_eq!(
        encode_to_vec(f32::INFINITY, config).unwrap(),
        [0xf9, 0x7c, 0x00]
    );
    assert_eq!(encode_to_vec(f32::NAN, config).unwrap(), [0xf9, 0x7e, 0x00]);
}

#[test]
fn test_rfc8949_appendix_a_collections() {
    let config = config::standard().with_cbor_format();

    // [] -> 0x80
    assert_eq!(encode_to_vec(Vec::<u8>::new(), config).unwrap(), [0x80]);

    // [1, 2, 3] -> 0x83010203
    assert_eq!(
        encode_to_vec(vec![1u8, 2, 3], config).unwrap(),
        [0x83, 0x01, 0x02, 0x03]
    );

    // {} -> 0xa0
    use std::collections::HashMap;
    assert_eq!(
        encode_to_vec(HashMap::<u8, u8>::new(), config).unwrap(),
        [0xa0]
    );
}

#[test]
fn test_rfc8949_appendix_a_strings_and_bytes() {
    let config = config::standard().with_cbor_format();

    // "" -> 0x60
    assert_eq!(encode_to_vec("", config).unwrap(), [0x60]);
    // "a" -> 0x6161
    assert_eq!(encode_to_vec("a", config).unwrap(), [0x61, 0x61]);
    // "IETF" -> 0x6449455446
    assert_eq!(
        encode_to_vec("IETF", config).unwrap(),
        [0x64, 0x49, 0x45, 0x54, 0x46]
    );
    // "\u00fc" -> 0x62c3bc
    assert_eq!(
        encode_to_vec("\u{00fc}", config).unwrap(),
        [0x62, 0xc3, 0xbc]
    );

    // h'' -> 0x40
    assert_eq!(encode_to_vec(b"".as_slice(), config).unwrap(), [0x40]);
    // h'01020304' -> 0x4401020304
    assert_eq!(
        encode_to_vec(b"\x01\x02\x03\x04".as_slice(), config).unwrap(),
        [0x44, 0x01, 0x02, 0x03, 0x04]
    );
}

#[test]
fn test_cbor_indefinite_length() {
    let config = config::standard().with_cbor_format();

    // Hand-crafted indefinite length array: [_ 1, 2, 3] -> 0x9f010203ff
    let input = [0x9f, 0x01, 0x02, 0x03, 0xff];
    let (decoded, _): (Vec<u8>, _) = decode_from_slice(&input, config).unwrap();
    assert_eq!(decoded, vec![1, 2, 3]);

    // Indefinite length map: {_ 1: 2, 3: 4} -> 0xbf01020304ff
    let input = [0xbf, 0x01, 0x02, 0x03, 0x04, 0xff];
    let (decoded, _): (std::collections::BTreeMap<u8, u8>, _) =
        decode_from_slice(&input, config).unwrap();
    assert_eq!(decoded.get(&1), Some(&2));
    assert_eq!(decoded.get(&3), Some(&4));
}

#[test]
fn test_cbor_deterministic_map_sorting() {
    let config = config::standard().with_deterministic_cbor();
    let mut map = std::collections::BTreeMap::new();
    // 10 (0x0a) and -1 (0x20)
    // RFC 8949 G.3: 0x0a < 0x20.
    // So 10 must come before -1 regardless of Rust's BTreeMap order.
    map.insert(10i32, "a");
    map.insert(-1i32, "b");

    let encoded = encode_to_vec(&map, config).unwrap();
    // Expect: 0xa2 (map 2), 0x0a (key 10), 0x61, 0x61 ("a"), 0x20 (key -1), 0x61, 0x62 ("b")
    assert_eq!(encoded[1], 0x0a);
    assert_eq!(encoded[4], 0x20);
}

#[test]
fn test_cbor_booleans() {
    let config = config::standard().with_cbor_format();
    assert_eq!(encode_to_vec(false, config).unwrap(), [0xf4]);
    assert_eq!(encode_to_vec(true, config).unwrap(), [0xf5]);
}

#[test]
fn test_cbor_bignums() {
    let config = config::standard().with_cbor_format();

    // 2^64 as i128 (positive bignum)
    // 18446744073709551616
    let val: i128 = 18446744073709551616;
    let encoded = encode_to_vec(val, config).unwrap();
    // 0xc2 (tag 2), 0x49 (byte string len 9), 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
    assert_eq!(encoded[0], 0xc2);
    assert_eq!(encoded[1], 0x49);

    let (decoded, _): (i128, _) = decode_from_slice(&encoded, config).unwrap();
    assert_eq!(decoded, val);

    // Negative bignum: -2^64 - 1
    let val: i128 = -18446744073709551617;
    let encoded = encode_to_vec(val, config).unwrap();
    // 0xc3 (tag 3), 0x49 (byte string len 9), 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
    assert_eq!(encoded[0], 0xc3);

    let (decoded, _): (i128, _) = decode_from_slice(&encoded, config).unwrap();
    assert_eq!(decoded, val);
}

#[test]
fn test_cbor_deterministic_set_sorting() {
    let config = config::standard().with_deterministic_cbor();
    let mut set = std::collections::BTreeSet::new();
    // "b" (0x61, 0x62), "a" (0x61, 0x61)
    // Deterministic: "a" then "b"
    set.insert("b".to_string());
    set.insert("a".to_string());

    let encoded = encode_to_vec(&set, config).unwrap();
    // Expect: 0x82 (array 2), [0x61, 0x61], [0x61, 0x62]
    assert_eq!(encoded[1], 0x61);
    assert_eq!(encoded[2], 0x61);
    assert_eq!(encoded[3], 0x61);
    assert_eq!(encoded[4], 0x62);
}

#[derive(Encode, Decode, Debug, PartialEq, Clone)]
struct ComplexStruct {
    m: std::collections::BTreeMap<String, Vec<i32>>,
    o: Option<Box<ComplexStruct>>,
    r: Result<u8, String>,
}

// Arbitrary for ComplexStruct
impl Arbitrary for ComplexStruct {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        let leaf = (
            prop::collection::btree_map(
                any::<String>(),
                prop::collection::vec(any::<i32>(), 0..5),
                0..5,
            ),
            any::<Result<u8, String>>(),
        )
            .prop_map(|(m, r)| ComplexStruct { m, o: None, r });

        leaf.prop_recursive(3, 16, 5, |inner| {
            (
                prop::collection::btree_map(
                    any::<String>(),
                    prop::collection::vec(any::<i32>(), 0..5),
                    0..5,
                ),
                prop::option::weighted(0.5, inner.prop_map(Box::new)),
                any::<Result<u8, String>>(),
            )
                .prop_map(|(m, o, r)| ComplexStruct { m, o, r })
        })
        .boxed()
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: if std::env::var("MIRIFLAGS").is_ok() { 2 } else { 1000 }, .. ProptestConfig::default() })]

    #[test]
    fn test_cbor_roundtrip_u64(a in any::<u64>()) {
        let config = config::standard().with_cbor_format();
        let encoded = encode_to_vec(a, config).unwrap();
        let (decoded, _): (u64, _) = decode_from_slice(&encoded, config).unwrap();
        assert_eq!(a, decoded);
    }

    #[test]
    fn test_cbor_roundtrip_i128(a in any::<i128>()) {
        let config = config::standard().with_cbor_format();
        let encoded = encode_to_vec(a, config).unwrap();
        let (decoded, _): (i128, _) = decode_from_slice(&encoded, config).unwrap();
        assert_eq!(a, decoded);
    }

    #[test]
    fn test_cbor_roundtrip_f64(a in any::<f64>()) {
        let config = config::standard().with_cbor_format();
        let encoded = encode_to_vec(a, config).unwrap();
        let (decoded, _): (f64, _) = decode_from_slice(&encoded, config).unwrap();
        if a.is_nan() {
            assert!(decoded.is_nan());
        } else {
            assert_eq!(a, decoded);
        }
    }

    #[test]
    fn test_cbor_roundtrip_vec_u8(a in prop::collection::vec(any::<u8>(), 0..100)) {
        let config = config::standard().with_cbor_format();
        let encoded = encode_to_vec(&a, config).unwrap();
        let (decoded, _): (Vec<u8>, _) = decode_from_slice(&encoded, config).unwrap();
        assert_eq!(a, decoded);
    }

    #[test]
    fn test_cbor_roundtrip_btree_map(a in prop::collection::btree_map(any::<i32>(), any::<String>(), 0..20)) {
        let config = config::standard().with_cbor_format();
        let encoded = encode_to_vec(&a, config).unwrap();
        let (decoded, _): (std::collections::BTreeMap<i32, String>, _) = decode_from_slice(&encoded, config).unwrap();
        assert_eq!(a, decoded);
    }

    #[test]
    fn test_cbor_roundtrip_hash_set(a in prop::collection::hash_set(any::<u32>(), 0..20)) {
        let config = config::standard().with_cbor_format();
        let encoded = encode_to_vec(&a, config).unwrap();
        let (decoded, _): (std::collections::HashSet<u32>, _) = decode_from_slice(&encoded, config).unwrap();
        assert_eq!(a, decoded);
    }

    #[test]
    fn test_cbor_roundtrip_complex(a in any::<ComplexStruct>()) {
        let config = config::standard().with_cbor_format();
        let encoded = encode_to_vec(&a, config).unwrap();
        let (decoded, _): (ComplexStruct, _) = decode_from_slice(&encoded, config).unwrap();
        assert_eq!(a, decoded);
    }
}
