#![cfg(all(feature = "derive", feature = "std"))]
#![allow(dead_code)]

extern crate bincode_next as bincode;

use bincode::Decode;
use bincode::Encode;

#[derive(Encode, Decode)]
pub enum TypeOfFile {
    Unknown = -1,
}
