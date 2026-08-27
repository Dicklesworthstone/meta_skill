#![cfg(all(feature = "std", feature = "derive"))]

use bincode_next::BitPacked;

#[derive(BitPacked, Debug, PartialEq, Clone, Copy)]
struct Valid {
    #[bincode(bits = 7)]
    val: u8,
}

#[test]
fn test_valid_bit_width() {
    let _ = Valid { val: 100 };
}

// The following code will fail to compile if uncommented
// #[test]
// fn test_invalid_bit_width() {
// let val = Invalid { val: 100 };
// let mut buf = [0u8; 10];
// let _ = bincode_next::encode_into_slice(val, &mut buf, bincode_next::config::standard());
// }
//
// #[derive(BitPacked)]
// struct Invalid  {
// #[bincode(bits = 9)]
// val: u8,
// }

#[derive(BitPacked, Debug, PartialEq, Clone, Copy)]
enum ValidEnum {
    A {
        #[bincode(bits = 7)]
        val: u8,
    },
}

#[test]
fn test_valid_enum_bit_width() {
    let val = ValidEnum::A { val: 100 };
    let mut buf = [0u8; 10];
    let _ = bincode_next::encode_into_slice(val, &mut buf, bincode_next::config::standard());
}
