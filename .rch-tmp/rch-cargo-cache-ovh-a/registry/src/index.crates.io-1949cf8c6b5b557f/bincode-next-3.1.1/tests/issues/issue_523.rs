#![cfg(all(feature = "derive", feature = "std"))]
#![allow(dead_code)]

extern crate bincode_next as bincode;
extern crate std;

use bincode::Decode;
use bincode::Encode;
use std::borrow::Cow;

#[derive(Clone, Encode, Decode)]
pub struct Foo<'a>(Cow<'a, str>);
