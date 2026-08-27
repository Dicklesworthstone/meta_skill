#![cfg(feature = "serde")]
use serde::Deserialize;
use serde::Serialize;

extern crate bincode_next as bincode;

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum TestEnum {
    A,
    B,
}

#[test]
fn test_issue_695() {
    let data = bincode::serde::encode_to_vec(&TestEnum::B, bincode::config::standard()).unwrap();
    let (decoded, _): (TestEnum, usize) =
        bincode::serde::decode_from_slice(&data, bincode::config::standard()).unwrap();
    assert_eq!(decoded, TestEnum::B);

    let (decoded_borrowed, _): (TestEnum, usize) =
        bincode::serde::borrow_decode_from_slice(&data, bincode::config::standard()).unwrap();
    assert_eq!(decoded_borrowed, TestEnum::B);
}
