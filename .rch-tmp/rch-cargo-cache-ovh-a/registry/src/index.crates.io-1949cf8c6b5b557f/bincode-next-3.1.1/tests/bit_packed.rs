#![cfg(all(feature = "std", feature = "derive"))]

use bincode_next::BitPacked;

#[derive(BitPacked, PartialEq, Debug)]
struct Telemetry {
    #[bincode(bits = 1)]
    is_active: bool,
    #[bincode(bits = 1)]
    has_error: bool,
    #[bincode(bits = 3)]
    mode: u8,
}

#[test]
fn test_bitpacked_density() {
    let t = Telemetry {
        is_active: true,
        has_error: false,
        mode: 5,
    };

    // We expect 5 bits of data, so it should encode into exactly 1 byte.
    let config = bincode_next::config::standard().with_bit_packing();
    let encoded = bincode_next::encode_to_vec(&t, config).unwrap();

    assert_eq!(encoded.len(), 1, "Should be packed perfectly into one byte");

    let (decoded, len): (Telemetry, usize) =
        bincode_next::decode_from_slice(&encoded, config).unwrap();
    assert_eq!(len, 1);
    assert_eq!(decoded, t);
}

#[derive(BitPacked, PartialEq, Debug)]
struct CrossByte {
    #[bincode(bits = 6)]
    a: u8,
    #[bincode(bits = 4)]
    b: u8,
}

#[test]
fn test_bitpacked_cross_byte() {
    let cb = CrossByte {
        a: 0b111111,
        b: 0b1111,
    };

    let config = bincode_next::config::standard().with_bit_packing();
    let encoded = bincode_next::encode_to_vec(&cb, config).unwrap();

    assert_eq!(encoded.len(), 2, "10 bits take exactly 2 bytes");

    let (decoded, len): (CrossByte, usize) =
        bincode_next::decode_from_slice(&encoded, config).unwrap();
    assert_eq!(len, 2);
    assert_eq!(decoded, cb);
}
