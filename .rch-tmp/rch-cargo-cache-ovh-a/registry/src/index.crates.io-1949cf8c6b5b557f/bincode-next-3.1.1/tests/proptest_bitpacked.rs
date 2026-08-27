#![cfg(all(feature = "std", feature = "derive"))]

use bincode_next::BitPacked;
use proptest::prelude::*;

#[derive(BitPacked, Debug, PartialEq, Clone)]
struct MixedFields {
    #[bincode(bits = 1)]
    flag_a: bool,

    #[bincode(bits = 3)]
    small_int: u8,

    #[bincode(bits = 5)]
    medium_int: u16,

    normal_u32: u32,

    #[bincode(bits = 1)]
    flag_b: bool,

    #[bincode(bits = 12)]
    large_int: u32,

    normal_string: String,
}

// Generate valid random ranges constrained by the bits manually.
// 3 bits = 0..=7
// 5 bits = 0..=31
// 12 bits = 0..=4095
prop_compose! {
    fn arb_mixed_fields()(
        flag_a in any::<bool>(),
        small_int in 0u8..=7,
        medium_int in 0u16..=31,
        normal_u32 in any::<u32>(),
        flag_b in any::<bool>(),
        large_int in 0u32..=4095,
        normal_string in ".*"
    ) -> MixedFields {
        MixedFields {
            flag_a,
            small_int,
            medium_int,
            normal_u32,
            flag_b,
            large_int,
            normal_string,
        }
    }
}

proptest! {
    // #![proptest_config(ProptestConfig::with_cases(1000))]
    #![proptest_config(ProptestConfig {cases: if std::env::var("MIRIFLAGS").is_ok() { 2 } else { 1000 },.. ProptestConfig::default()})]
    #[test]
    fn round_trip_mixed_fields(item in arb_mixed_fields()) {
        let config = bincode_next::config::standard();

        // Encode
        let encoded = bincode_next::encode_to_vec(&item, config).expect("Encoding should succeed");

        // Decode
        let (decoded, amount_read): (MixedFields, usize) = bincode_next::decode_from_slice(&encoded, config).expect("Decoding should succeed");

        assert_eq!(item, decoded, "Decoded item does not match original");
        assert_eq!(encoded.len(), amount_read, "Did not consume all bytes");
    }
}
