#![cfg(all(feature = "std", feature = "derive"))]
#![allow(dead_code)]

extern crate std;

extern crate bincode_next as bincode;
use std::collections::BTreeMap;

#[derive(bincode::Encode)]
struct AllTypes(BTreeMap<u8, AllTypes>);

#[test]
fn test_issue_459() {
    let _result = bincode::encode_to_vec(AllTypes(BTreeMap::new()), bincode::config::standard());
}
