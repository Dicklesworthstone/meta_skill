//! Fingerprinting implementation for bincode.
//!
//! This module provides the [`Fingerprint`] trait and derive macro to enable
//! schema verification during encoding and decoding. The fingerprint is a 64-bit
//! hash that validates that the encoder and decoder are using the same schema
//! and configuration.
//!
//! The fingerprint covers:
//! - Type names
//! - Struct field names, types, and their order
//! - Enum variant names, field names, types, and their order
//! - Relevant configuration settings (endianness, integer encoding, etc.)

use crate::config::Config;
#[cfg(feature = "alloc")]
use alloc::borrow::ToOwned;

/// Fingerprint trait for types that can be hashed for schema verification.
///
/// The fingerprint covers:
/// - The type name
/// - For structs: field names, types, and their order
/// - For enums: variant names, field names, types, and their order
/// - Relevant configuration settings (endianness, integer encoding, etc.)
pub trait Fingerprint<C: Config> {
    /// Unique 64-bit compile-time hash of the type's schema AND the Configuration used.
    const SCHEMA_HASH: u64;
}

macro_rules! impl_fingerprint {
    ($($t:ty => $name:expr),*) => {
        $(
            impl<C: Config> Fingerprint<C> for $t {
                const SCHEMA_HASH: u64 = rapidhash::v3::rapidhash_v3_seeded($name, &rapidhash::v3::RapidSecrets::seed_cpp(C::CONFIG_HASH));
            }
        )*
    };
}

impl_fingerprint! {
    u8 => b"u8",
    u16 => b"u16",
    u32 => b"u32",
    u64 => b"u64",
    u128 => b"u128",
    usize => b"usize",
    i8 => b"i8",
    i16 => b"i16",
    i32 => b"i32",
    i64 => b"i64",
    i128 => b"i128",
    isize => b"isize",
    bool => b"bool",
    f32 => b"f32",
    f64 => b"f64",
    char => b"char",
    str => b"str"
}

impl<C: Config, T: Fingerprint<C>, const N: usize> Fingerprint<C> for [T; N] {
    const SCHEMA_HASH: u64 = rapidhash::v3::rapidhash_v3_seeded(
        &N.to_le_bytes(),
        &rapidhash::v3::RapidSecrets::seed_cpp(T::SCHEMA_HASH),
    );
}

#[cfg(feature = "alloc")]
impl<C: Config, T: Fingerprint<C>> Fingerprint<C> for alloc::vec::Vec<T> {
    const SCHEMA_HASH: u64 = rapidhash::v3::rapidhash_v3_seeded(
        b"Vec",
        &rapidhash::v3::RapidSecrets::seed_cpp(T::SCHEMA_HASH),
    );
}

#[cfg(feature = "alloc")]
impl<C: Config> Fingerprint<C> for alloc::string::String {
    const SCHEMA_HASH: u64 = rapidhash::v3::rapidhash_v3_seeded(
        b"str",
        &rapidhash::v3::RapidSecrets::seed_cpp(C::CONFIG_HASH),
    );
}

impl<C: Config> Fingerprint<C> for () {
    const SCHEMA_HASH: u64 = rapidhash::v3::rapidhash_v3_seeded(
        b"()",
        &rapidhash::v3::RapidSecrets::seed_cpp(C::CONFIG_HASH),
    );
}

impl<C: Config, T1: Fingerprint<C>> Fingerprint<C> for (T1,) {
    const SCHEMA_HASH: u64 = T1::SCHEMA_HASH;
}

impl<C: Config, T1: Fingerprint<C>, T2: Fingerprint<C>> Fingerprint<C> for (T1, T2) {
    const SCHEMA_HASH: u64 = rapidhash::v3::rapidhash_v3_seeded(
        &T2::SCHEMA_HASH.to_le_bytes(),
        &rapidhash::v3::RapidSecrets::seed_cpp(T1::SCHEMA_HASH),
    );
}

impl<C: Config, T: Fingerprint<C>> Fingerprint<C> for Option<T> {
    const SCHEMA_HASH: u64 = rapidhash::v3::rapidhash_v3_seeded(
        b"Option",
        &rapidhash::v3::RapidSecrets::seed_cpp(T::SCHEMA_HASH),
    );
}

impl<C: Config, T: Fingerprint<C>, E: Fingerprint<C>> Fingerprint<C> for Result<T, E> {
    const SCHEMA_HASH: u64 = rapidhash::v3::rapidhash_v3_seeded(
        &E::SCHEMA_HASH.to_le_bytes(),
        &rapidhash::v3::RapidSecrets::seed_cpp(rapidhash::v3::rapidhash_v3_seeded(
            b"Result",
            &rapidhash::v3::RapidSecrets::seed_cpp(T::SCHEMA_HASH),
        )),
    );
}

impl<C: Config, T: ?Sized> Fingerprint<C> for core::marker::PhantomData<T> {
    const SCHEMA_HASH: u64 = rapidhash::v3::rapidhash_v3_seeded(
        b"PhantomData",
        &rapidhash::v3::RapidSecrets::seed_cpp(C::CONFIG_HASH),
    );
}

impl<C: Config, T: ?Sized + Fingerprint<C>> Fingerprint<C> for &T {
    const SCHEMA_HASH: u64 = T::SCHEMA_HASH;
}

impl<C: Config, T: ?Sized + Fingerprint<C>> Fingerprint<C> for &mut T {
    const SCHEMA_HASH: u64 = T::SCHEMA_HASH;
}

#[cfg(feature = "alloc")]
impl<C: Config, T: ?Sized + Fingerprint<C>> Fingerprint<C> for alloc::boxed::Box<T> {
    const SCHEMA_HASH: u64 = T::SCHEMA_HASH;
}

#[cfg(feature = "alloc")]
impl<C: Config, T: Fingerprint<C> + ToOwned + ?Sized> Fingerprint<C> for alloc::borrow::Cow<'_, T> {
    const SCHEMA_HASH: u64 = T::SCHEMA_HASH;
}

#[cfg(feature = "alloc")]
impl<C: Config, T: ?Sized + Fingerprint<C>> Fingerprint<C> for alloc::rc::Rc<T> {
    const SCHEMA_HASH: u64 = T::SCHEMA_HASH;
}

#[cfg(feature = "alloc")]
impl<C: Config, T: ?Sized + Fingerprint<C>> Fingerprint<C> for alloc::sync::Arc<T> {
    const SCHEMA_HASH: u64 = T::SCHEMA_HASH;
}

#[cfg(feature = "alloc")]
impl<C: Config, T: Fingerprint<C>> Fingerprint<C> for alloc::collections::VecDeque<T> {
    const SCHEMA_HASH: u64 = rapidhash::v3::rapidhash_v3_seeded(
        b"VecDeque",
        &rapidhash::v3::RapidSecrets::seed_cpp(T::SCHEMA_HASH),
    );
}

#[cfg(feature = "alloc")]
impl<C: Config, T: Fingerprint<C>> Fingerprint<C> for alloc::collections::BinaryHeap<T> {
    const SCHEMA_HASH: u64 = rapidhash::v3::rapidhash_v3_seeded(
        b"BinaryHeap",
        &rapidhash::v3::RapidSecrets::seed_cpp(T::SCHEMA_HASH),
    );
}

#[cfg(feature = "alloc")]
impl<C: Config, K: Fingerprint<C>, V: Fingerprint<C>> Fingerprint<C>
    for alloc::collections::BTreeMap<K, V>
{
    const SCHEMA_HASH: u64 = rapidhash::v3::rapidhash_v3_seeded(
        b"BTreeMap",
        &rapidhash::v3::RapidSecrets::seed_cpp(rapidhash::v3::rapidhash_v3_seeded(
            &V::SCHEMA_HASH.to_le_bytes(),
            &rapidhash::v3::RapidSecrets::seed_cpp(K::SCHEMA_HASH),
        )),
    );
}

#[cfg(feature = "alloc")]
impl<C: Config, T: Fingerprint<C>> Fingerprint<C> for alloc::collections::BTreeSet<T> {
    const SCHEMA_HASH: u64 = rapidhash::v3::rapidhash_v3_seeded(
        b"BTreeSet",
        &rapidhash::v3::RapidSecrets::seed_cpp(T::SCHEMA_HASH),
    );
}

#[cfg(feature = "std")]
impl<C: Config, K: Fingerprint<C>, V: Fingerprint<C>, S> Fingerprint<C>
    for std::collections::HashMap<K, V, S>
{
    const SCHEMA_HASH: u64 = rapidhash::v3::rapidhash_v3_seeded(
        b"HashMap",
        &rapidhash::v3::RapidSecrets::seed_cpp(rapidhash::v3::rapidhash_v3_seeded(
            &V::SCHEMA_HASH.to_le_bytes(),
            &rapidhash::v3::RapidSecrets::seed_cpp(K::SCHEMA_HASH),
        )),
    );
}

#[cfg(feature = "std")]
impl<C: Config, T: Fingerprint<C>, S> Fingerprint<C> for std::collections::HashSet<T, S> {
    const SCHEMA_HASH: u64 = rapidhash::v3::rapidhash_v3_seeded(
        b"HashSet",
        &rapidhash::v3::RapidSecrets::seed_cpp(T::SCHEMA_HASH),
    );
}

impl<C: Config> Fingerprint<C> for core::time::Duration {
    const SCHEMA_HASH: u64 = rapidhash::v3::rapidhash_v3_seeded(
        b"Duration",
        &rapidhash::v3::RapidSecrets::seed_cpp(C::CONFIG_HASH),
    );
}

#[cfg(feature = "std")]
impl<C: Config> Fingerprint<C> for std::time::SystemTime {
    const SCHEMA_HASH: u64 = rapidhash::v3::rapidhash_v3_seeded(
        b"SystemTime",
        &rapidhash::v3::RapidSecrets::seed_cpp(C::CONFIG_HASH),
    );
}

#[cfg(feature = "std")]
impl<C: Config> Fingerprint<C> for std::net::IpAddr {
    const SCHEMA_HASH: u64 = rapidhash::v3::rapidhash_v3_seeded(
        b"IpAddr",
        &rapidhash::v3::RapidSecrets::seed_cpp(C::CONFIG_HASH),
    );
}

#[cfg(feature = "std")]
impl<C: Config> Fingerprint<C> for std::net::Ipv4Addr {
    const SCHEMA_HASH: u64 = rapidhash::v3::rapidhash_v3_seeded(
        b"Ipv4Addr",
        &rapidhash::v3::RapidSecrets::seed_cpp(C::CONFIG_HASH),
    );
}

#[cfg(feature = "std")]
impl<C: Config> Fingerprint<C> for std::net::Ipv6Addr {
    const SCHEMA_HASH: u64 = rapidhash::v3::rapidhash_v3_seeded(
        b"Ipv6Addr",
        &rapidhash::v3::RapidSecrets::seed_cpp(C::CONFIG_HASH),
    );
}

#[cfg(feature = "std")]
impl<C: Config> Fingerprint<C> for std::net::SocketAddr {
    const SCHEMA_HASH: u64 = rapidhash::v3::rapidhash_v3_seeded(
        b"SocketAddr",
        &rapidhash::v3::RapidSecrets::seed_cpp(C::CONFIG_HASH),
    );
}

#[cfg(feature = "std")]
impl<C: Config> Fingerprint<C> for std::net::SocketAddrV4 {
    const SCHEMA_HASH: u64 = rapidhash::v3::rapidhash_v3_seeded(
        b"SocketAddrV4",
        &rapidhash::v3::RapidSecrets::seed_cpp(C::CONFIG_HASH),
    );
}

#[cfg(feature = "std")]
impl<C: Config> Fingerprint<C> for std::net::SocketAddrV6 {
    const SCHEMA_HASH: u64 = rapidhash::v3::rapidhash_v3_seeded(
        b"SocketAddrV6",
        &rapidhash::v3::RapidSecrets::seed_cpp(C::CONFIG_HASH),
    );
}

#[cfg(feature = "std")]
impl<C: Config> Fingerprint<C> for std::path::Path {
    const SCHEMA_HASH: u64 = rapidhash::v3::rapidhash_v3_seeded(
        b"Path",
        &rapidhash::v3::RapidSecrets::seed_cpp(C::CONFIG_HASH),
    );
}

#[cfg(feature = "std")]
impl<C: Config> Fingerprint<C> for std::path::PathBuf {
    const SCHEMA_HASH: u64 = rapidhash::v3::rapidhash_v3_seeded(
        b"PathBuf",
        &rapidhash::v3::RapidSecrets::seed_cpp(C::CONFIG_HASH),
    );
}

#[cfg(feature = "std")]
impl<C: Config> Fingerprint<C> for std::ffi::CStr {
    const SCHEMA_HASH: u64 = rapidhash::v3::rapidhash_v3_seeded(
        b"CStr",
        &rapidhash::v3::RapidSecrets::seed_cpp(C::CONFIG_HASH),
    );
}

#[cfg(feature = "std")]
impl<C: Config> Fingerprint<C> for std::ffi::CString {
    const SCHEMA_HASH: u64 = rapidhash::v3::rapidhash_v3_seeded(
        b"CString",
        &rapidhash::v3::RapidSecrets::seed_cpp(C::CONFIG_HASH),
    );
}

#[cfg(feature = "std")]
impl<C: Config, T: Fingerprint<C>> Fingerprint<C> for std::sync::Mutex<T> {
    const SCHEMA_HASH: u64 = T::SCHEMA_HASH;
}

#[cfg(feature = "std")]
impl<C: Config, T: Fingerprint<C>> Fingerprint<C> for std::sync::RwLock<T> {
    const SCHEMA_HASH: u64 = T::SCHEMA_HASH;
}

macro_rules! impl_tuple {
    ($($t:ident),*) => {
        impl<C: Config, $($t: Fingerprint<C>),*> Fingerprint<C> for ($($t,)*) {
            const SCHEMA_HASH: u64 = {
                let mut hash = C::CONFIG_HASH;
                $(
                    hash = rapidhash::v3::rapidhash_v3_seeded(&$t::SCHEMA_HASH.to_le_bytes(), &rapidhash::v3::RapidSecrets::seed_cpp(hash));
                )*
                hash
            };
        }
    };
}

impl_tuple!(T1, T2, T3);
impl_tuple!(T1, T2, T3, T4);
impl_tuple!(T1, T2, T3, T4, T5);
impl_tuple!(T1, T2, T3, T4, T5, T6);
impl_tuple!(T1, T2, T3, T4, T5, T6, T7);
impl_tuple!(T1, T2, T3, T4, T5, T6, T7, T8);
