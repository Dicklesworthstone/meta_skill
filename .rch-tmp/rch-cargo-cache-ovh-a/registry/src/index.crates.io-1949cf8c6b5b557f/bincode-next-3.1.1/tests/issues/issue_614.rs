#![cfg(feature = "derive")]
#![allow(dead_code)]

extern crate bincode_next as bincode;

use bincode::Decode;
use bincode::Encode;

#[derive(Encode, Decode, Clone)]
pub struct A;
#[derive(Encode, Decode, Clone)]
pub struct B<T>
where
    T: Clone + Encode + Decode<()>,
{
    pub t: T,
}

#[derive(Encode, Decode)]
pub struct MyStruct<T>
where
    T: Clone + Encode + Decode<()>,
{
    pub a: A,
    pub b: B<T>,
}
