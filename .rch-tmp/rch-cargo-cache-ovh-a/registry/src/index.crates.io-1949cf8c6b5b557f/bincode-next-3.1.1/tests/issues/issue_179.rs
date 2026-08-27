#![cfg(feature = "serde")]
extern crate alloc;
use alloc::string::String;
use alloc::string::ToString;
use serde::Deserialize;
use serde::Serialize;

extern crate bincode_next as bincode;

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct V1 {
    pub a: u32,
    pub b: String,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct V2 {
    pub a: u32,
    pub b: String,
    #[serde(default)]
    pub c: u32, // Added later
    #[serde(default)]
    pub d: Option<u64>, // Added later
}

#[test]
fn test_issue_179_backward_compatibility() {
    let v1 = V1 {
        a: 42,
        b: "hello".to_string(),
    };

    let encoded_v1 = bincode::serde::encode_to_vec(&v1, bincode::config::standard()).unwrap();

    let (decoded_v2, _): (V2, usize) =
        bincode::serde::decode_from_slice(&encoded_v1, bincode::config::standard()).unwrap();

    assert_eq!(decoded_v2.a, 42);
    assert_eq!(decoded_v2.b, "hello");
    assert_eq!(decoded_v2.c, 0); // Default value
    assert_eq!(decoded_v2.d, None); // Default value
}
