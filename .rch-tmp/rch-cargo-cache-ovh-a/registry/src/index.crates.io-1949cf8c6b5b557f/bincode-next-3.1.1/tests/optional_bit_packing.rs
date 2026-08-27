#![cfg(all(feature = "std", feature = "derive"))]

use bincode_next::BitPacked;
use bincode_next::config;

#[derive(BitPacked, Debug, PartialEq, Clone, Copy)]
enum TestEnum {
    #[bincode(bits = 2)]
    A(#[bincode(bits = 6)] u8),
    B,
}

#[test]
fn test_optional_bit_packing() {
    let val = TestEnum::A(15);

    // With bit packing
    // Note: we use with_fixed_int_encoding to avoid varint interference in length checks
    let config_packed = config::standard()
        .with_bit_packing()
        .with_fixed_int_encoding();
    let encoded_packed = bincode_next::encode_to_vec(val, config_packed).unwrap();
    // 2 bits for variant A, 6 bits for the value 15. Total 8 bits = 1 byte.
    assert_eq!(encoded_packed.len(), 1);
    let (decoded_packed, _): (TestEnum, usize) =
        bincode_next::decode_from_slice(&encoded_packed, config_packed).unwrap();
    assert_eq!(decoded_packed, val);

    // Without bit packing (explicitly disabled)
    let config_unpacked = config::standard()
        .with_no_bit_packing()
        .with_fixed_int_encoding();
    let encoded_unpacked = bincode_next::encode_to_vec(val, config_unpacked).unwrap();
    // In Fixint config, variant index is u32 (4 bytes) + u8 (1 byte) = 5 bytes.
    assert_eq!(encoded_unpacked.len(), 5);
    let (decoded_unpacked, _): (TestEnum, usize) =
        bincode_next::decode_from_slice(&encoded_unpacked, config_unpacked).unwrap();
    assert_eq!(decoded_unpacked, val);

    // Default configuration (should be unpacked to avoid breaking changes)
    let config_default = config::standard().with_fixed_int_encoding();
    let encoded_default = bincode_next::encode_to_vec(val, config_default).unwrap();
    assert_eq!(encoded_default.len(), 5);
}

#[derive(BitPacked, Debug, PartialEq, Clone, Copy)]
struct TestStruct {
    #[bincode(bits = 4)]
    a: u8,
    #[bincode(bits = 4)]
    b: u8,
}

#[test]
fn test_struct_optional_bit_packing() {
    let val = TestStruct { a: 10, b: 5 };

    // With bit packing
    let config_packed = config::standard()
        .with_bit_packing()
        .with_fixed_int_encoding();
    let encoded_packed = bincode_next::encode_to_vec(val, config_packed).unwrap();
    // 4+4 = 8 bits = 1 byte.
    assert_eq!(encoded_packed.len(), 1);

    // Without bit packing
    let config_unpacked = config::standard()
        .with_no_bit_packing()
        .with_fixed_int_encoding();
    let encoded_unpacked = bincode_next::encode_to_vec(val, config_unpacked).unwrap();
    // 1 byte + 1 byte = 2 bytes.
    assert_eq!(encoded_unpacked.len(), 2);
}
