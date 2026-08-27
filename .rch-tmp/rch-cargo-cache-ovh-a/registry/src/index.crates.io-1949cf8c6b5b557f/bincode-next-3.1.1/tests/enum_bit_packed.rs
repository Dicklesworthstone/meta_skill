#![cfg(feature = "derive")]

use bincode_next::BitPacked;

#[derive(BitPacked, Debug, PartialEq)]
enum SimpleEnum {
    A,
    B,
    C,
}

#[derive(BitPacked, Debug, PartialEq)]
enum ComplexEnum {
    // 0
    Empty,
    // 1
    Single(u8),
    // 2
    Tuple(#[bincode(bits = 2)] u8, #[bincode(bits = 4)] u8),
    // 3
    StructLike {
        #[bincode(bits = 1)]
        flag: bool,
        normal: u16,
        #[bincode(bits = 3)]
        tiny: u8,
    },
}

// SimpleEnum has 3 variants, so it needs (3 - 1).ilog2() + 1 = 2 bits for the tag!
// It should encode into exactly 1 byte.
#[test]
fn test_simple_enum() {
    let config = bincode_next::config::legacy().with_bit_packing();
    let encoded = bincode_next::encode_to_vec(SimpleEnum::C, config).unwrap();
    assert_eq!(encoded.len(), 1);

    let decoded: SimpleEnum = bincode_next::decode_from_slice(&encoded, config).unwrap().0;
    assert_eq!(decoded, SimpleEnum::C);
}

// ComplexEnum has 4 variants, so it needs (4 - 1).ilog2() + 1 = 2 bits for the tag!
// The Empty variant has 0 bits of data. So just the tag -> 2 bits. Fits in 1 byte.
// The Tuple variant has 2 + 4 = 6 bits of data. Plus tag 2 bits -> 8 bits. Fits exactly in 1 byte!
// The StructLike variant has 1 + (16 un-packed) + 3 bits. The tag is 2 bits, flag is 1 bit -> packed 3 bits total (first block).
// Then normal un-packed u16 (2 bytes). Then short tiny 3 bits (packed block).
#[test]
fn test_complex_enum_empty() {
    let config = bincode_next::config::legacy().with_bit_packing();
    let encoded = bincode_next::encode_to_vec(ComplexEnum::Empty, config).unwrap();
    assert_eq!(encoded.len(), 1);

    let decoded: ComplexEnum = bincode_next::decode_from_slice(&encoded, config).unwrap().0;
    assert_eq!(decoded, ComplexEnum::Empty);
}

#[test]
fn test_complex_enum_tuple() {
    let config = bincode_next::config::legacy().with_bit_packing();
    let val = ComplexEnum::Tuple(3, 15);
    let encoded = bincode_next::encode_to_vec(&val, config).unwrap();

    // Tag is 2 bits, fields are 2 and 4 bits = 8 bits. Exactly 1 byte.
    assert_eq!(
        encoded.len(),
        1,
        "Tuple should be tightly combined with discriminant and be 1 byte long"
    );

    let decoded: ComplexEnum = bincode_next::decode_from_slice(&encoded, config).unwrap().0;
    assert_eq!(decoded, val);
}

#[test]
fn test_complex_enum_struct() {
    let config = bincode_next::config::legacy().with_bit_packing();
    let val = ComplexEnum::StructLike {
        flag: true,
        normal: 65000,
        tiny: 7,
    };
    let encoded = bincode_next::encode_to_vec(&val, config).unwrap();

    // Tag: 2 bits
    // flag: 1 bit
    // FLUSH
    // normal: 16 bits (2 bytes)
    // tiny: 3 bits
    // FLUSH
    // Output: 4 bytes total
    assert_eq!(
        encoded.len(),
        4,
        "StructLike variant should be 4 bytes long"
    );

    let decoded: ComplexEnum = bincode_next::decode_from_slice(&encoded, config).unwrap().0;
    assert_eq!(decoded, val);
}
