#![cfg(feature = "derive")]
#![allow(dead_code)]

extern crate bincode_next as bincode;

#[derive(bincode::Encode, bincode::Decode)]
pub struct Eg<D, E> {
    data: (D, E),
}
