//! Support for serde integration. Enable this with the `serde` feature.
//!
//! To encode/decode type that implement serde's trait, you can use:
//! - [`borrow_decode_from_slice`\]
//! - [`decode_from_slice`\]
//! - [`encode_into_slice`\]
//! - [`encode_to_vec`\]
//!
//! For interop with bincode's [Decode]/[Encode], you can use:
//! - [`Compat`\]
//! - [`BorrowCompat`\]
//!
//! For interop with bincode's `derive` feature, you can use the `#[bincode(with_serde)]` attribute on each field that implements serde's traits.
//!
//! ```
//! # #[cfg(feature = "derive")]
//! # mod foo {
//! # use bincode_next::{Decode, Encode};
//! # use serde_derive::{Deserialize, Serialize};
//! #[derive(Serialize, Deserialize)]
//! pub struct SerdeType {
//!     // ...
//! }
//!
//! #[derive(Decode, Encode)]
//! pub struct StructWithSerde {
//!     #[bincode(with_serde)]
//!     pub serde: SerdeType,
//! }
//!
//! #[derive(Decode, Encode)]
//! pub enum EnumWithSerde {
//!     Unit(#[bincode(with_serde)] SerdeType),
//!     Struct {
//!         #[bincode(with_serde)]
//!         serde: SerdeType,
//!     },
//! }
//! # }
//! ```
//!
//! # Known issues
//!
//! Because bincode is a format without meta data, there are several known issues with serde's attributes. Please do not use any of the following attributes if you plan on using bincode, or use bincode's own `derive` macros.
//! - `#[serde(flatten)]`
//! - `#[serde(skip)]`
//! - `#[serde(skip_deserializing)]`
//! - `#[serde(skip_serializing)]`
//! - `#[serde(skip_serializing_if = "path")]`
//! - `#[serde(tag = "...")]`
//! - `#[serde(untagged)]`
//!
//! **Using any of the above attributes can and will cause issues with bincode and will result in lost data**. Consider using bincode's own derive macro instead.
//!
//! ### `deserialize_any` and Self-Describing Formats
//!
//! Bincode is not a self-describing format (it does not store type metadata with the data map like JSON does). Because of this, it **does not support `deserialize_any`**.
//!
//! This means that types like `serde_json::Value` or `toml::Value` cannot be deserialized directly from a bincode stream. Attempting to do so will result in an `AnyNotSupported` error. To work around this, you must deserialize into strongly-typed data structures representing your schema, rather than dynamic or nested value enums.
//!
//! # Why move away from serde?
//!
//! Serde is a great library, but it has some issues that makes us want to be decoupled from serde:
//! - The issues documented above with attributes.
//! - Serde has chosen to not have a MSRV ([source](https://github.com/serde-rs/serde/pull/2257)). We think MSRV is important, bincode 1 still compiles with rust 1.18.
//! - Before serde we had rustc-serializer. Serde has more than replaced rustc-serializer, but we can imagine a future where serde is replaced by something else.
//! - We believe that less dependencies is better, and that you should be able to choose your own dependencies. If you disable all features, bincode 2 only has 1 dependency. ([`unty`\], a micro crate we manage ourselves)
//!
//! **note:** just because we're making serde an optional dependency, it does not mean we're dropping support for serde. Serde will still be fully supported, we're just giving you the option to not use it.
//!
//! [Decode]: ../de/trait.Decode.html
//! [Encode]: ../enc/trait.Encode.html
//! [`unty`\]: <https://crates.io/crates/unty>

mod de_borrowed;
mod de_owned;
mod ser;

pub use self::de_borrowed::*;
pub use self::de_owned::*;
pub use self::ser::*;

use serde::de::Visitor;

#[cfg(feature = "async-fiber")]
#[doc(alias = "decode_serde_async")]
#[doc(inline)]
pub use crate::decode_serde_async as decode_async;
#[cfg(feature = "async-fiber")]
#[doc(alias = "decode_serde_async_with_context")]
#[doc(inline)]
pub use crate::decode_serde_async_with_context as decode_async_with_context;
#[cfg(all(feature = "tokio", feature = "async-fiber", feature = "serde"))]
#[doc(alias = "decode_serde_async_tokio")]
#[doc(inline)]
pub use crate::decode_serde_tokio_async as decode_async_tokio;
#[cfg(all(feature = "tokio", feature = "async-fiber", feature = "serde"))]
#[doc(alias = "decode_serde_async_tokio_with_context")]
#[doc(inline)]
pub use crate::decode_serde_tokio_async_with_context as decode_async_tokio_with_context;

#[doc(hidden)]
#[macro_export]
#[rustfmt::skip]
macro_rules! bincode_error_serde {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $(
                $(#[$variant_meta:meta])*
                $variant:ident $( { $( $(#[$field_meta:meta])* $field:ident : $ftype:ty ),* $(,)? } )? $( ( $( $(#[$tuple_meta:meta])* $tname:ident : $ttype:ty ),* $(,)? ) )?
            ),* $(,)?
        }
    ) => {
        $(#[$meta])*
        $vis enum $name {
            $(
                $(#[$variant_meta])*
                $variant $( { $( $(#[$field_meta])* $field : $ftype ),* } )? $( ( $( $(#[$tuple_meta])* $ttype ),* ) )?,
            )*
        }

        pastey::paste! {
            $(
                $(#[$variant_meta])*
                #[doc(hidden)]
                #[cold]
                #[track_caller]
                #[inline(never)]
                pub const fn [<cold_ $name:snake _ $variant:snake>]<'de, V>(
                    $($($field : $ftype),*)?
                    $($($tname : $ttype),*)?
                ) -> core::result::Result<<V as Visitor<'de>>::Value, $name>
                where
                    V: Visitor<'de>,
                {
                    core::result::Result::Err($name::$variant $( { $($field),* } )? $( ( $( $tname ),* ) )?)
                }
            )*
        }
    };
}

bincode_error_serde! {
    /// A serde-specific error that occurred while decoding.
    #[derive(Debug)]
    #[non_exhaustive]
    pub enum DecodeError {
        /// Bincode does not support serde's `any` decoding feature.
        ///
        /// See the "known issues" list in the serde module for more information on this.
        AnyNotSupported,

        /// Bincode does not support serde identifiers
        IdentifierNotSupported,

        /// Bincode does not support serde's `ignored_any`.
        ///
        /// See the "known issues" list in the serde module for more information on this.
        IgnoredAnyNotSupported,

        /// Serde tried decoding a borrowed value from an owned reader. Use `serde_decode_borrowed_from_*` instead
        CannotBorrowOwnedData,

        /// Could not allocate data like `String` and `Vec<u8>`
        #[cfg(not(feature = "alloc"))]
        CannotAllocate,

        /// Custom serde error but bincode is unable to allocate a string. Set a breakpoint where this is thrown for more information.
        #[cfg(not(feature = "alloc"))]
        CustomError,
    }
}

#[cfg(feature = "alloc")]
impl serde::de::Error for crate::error::DecodeError {
    fn custom<T>(msg: T) -> Self
    where
        T: core::fmt::Display,
    {
        use alloc::string::ToString;
        crate::error::cold_decode_error_other_string::<()>(msg.to_string()).unwrap_err()
    }
}

#[cfg(not(feature = "alloc"))]
impl serde::de::Error for crate::error::DecodeError {
    fn custom<T>(_: T) -> Self
    where
        T: core::fmt::Display,
    {
        crate::error::cold_decode_error_serde::<()>(DecodeError::CustomError).unwrap_err()
    }
}

/// A serde-specific error that occurred while encoding.
#[derive(Debug)]
#[non_exhaustive]
pub enum EncodeError {
    /// Serde provided bincode with a sequence without a length, which is not supported in bincode
    SequenceMustHaveLength,

    /// [Serializer::collect_str] got called but bincode was unable to allocate memory.
    #[cfg(not(feature = "alloc"))]
    CannotCollectStr,

    /// Custom serde error but bincode is unable to allocate a string. Set a breakpoint where this is thrown for more information.
    #[cfg(not(feature = "alloc"))]
    CustomError,
}

#[allow(clippy::from_over_into)]
impl Into<crate::error::EncodeError> for EncodeError {
    fn into(self) -> crate::error::EncodeError {
        crate::error::cold_encode_error_serde::<()>(self).unwrap_err()
    }
}

#[cfg(feature = "alloc")]
impl serde::ser::Error for crate::error::EncodeError {
    fn custom<T>(msg: T) -> Self
    where
        T: core::fmt::Display,
    {
        use alloc::string::ToString;

        crate::error::cold_encode_error_other_string::<()>(msg.to_string()).unwrap_err()
    }
}

#[cfg(not(feature = "alloc"))]
impl serde::ser::Error for crate::error::EncodeError {
    fn custom<T>(_: T) -> Self
    where
        T: core::fmt::Display,
    {
        crate::error::cold_encode_error_serde::<()>(EncodeError::CustomError).unwrap_err()
    }
}

/// Wrapper struct that implements [`crate::Decode`\] and [`crate::Encode`\] on any type that implements serde's [`DeserializeOwned`\] and [`Serialize`\] respectively.
///
/// This works for most types, but if you're dealing with borrowed data consider using [`BorrowCompat`\] instead.
///
/// [Decode]: ../de/trait.Decode.html
/// [Encode]: ../enc/trait.Encode.html
/// [DeserializeOwned]: https://docs.rs/serde/1/serde/de/trait.DeserializeOwned.html
/// [Serialize]: https://docs.rs/serde/1/serde/trait.Serialize.html
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Compat<T>(pub T);

impl<Context, T> crate::Decode<Context> for Compat<T>
where
    T: serde::de::DeserializeOwned,
{
    fn decode<D: crate::de::Decoder>(decoder: &mut D) -> Result<Self, crate::error::DecodeError> {
        let serde_decoder = de_owned::SerdeDecoder { de: decoder };
        T::deserialize(serde_decoder).map(Compat)
    }
}
impl<'de, T, Context> crate::BorrowDecode<'de, Context> for Compat<T>
where
    T: serde::de::DeserializeOwned,
{
    fn borrow_decode<D: crate::de::BorrowDecoder<'de>>(
        decoder: &mut D
    ) -> Result<Self, crate::error::DecodeError> {
        let serde_decoder = de_owned::SerdeDecoder { de: decoder };
        T::deserialize(serde_decoder).map(Compat)
    }
}

impl<T> crate::Encode for Compat<T>
where
    T: serde::Serialize,
{
    fn encode<E: crate::enc::Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), crate::error::EncodeError> {
        let serializer = ser::SerdeEncoder { enc: encoder };
        self.0.serialize(serializer)?;
        Ok(())
    }
}

impl<T> core::fmt::Debug for Compat<T>
where
    T: core::fmt::Debug,
{
    fn fmt(
        &self,
        f: &mut core::fmt::Formatter<'_>,
    ) -> core::fmt::Result {
        f.debug_tuple("Compat").field(&self.0).finish()
    }
}

impl<T> core::fmt::Display for Compat<T>
where
    T: core::fmt::Display,
{
    fn fmt(
        &self,
        f: &mut core::fmt::Formatter<'_>,
    ) -> core::fmt::Result {
        self.0.fmt(f)
    }
}

/// Wrapper struct that implements [`crate::de::BorrowDecode`\] and [`crate::Encode`\] on any type that implements serde's [`Deserialize`\] and [`Serialize`\] respectively.
///
/// This is mostly used on `&[u8]` and `&str`, for other types consider using [`Compat`\] instead.
/// [`Deserialize`\]: <https://docs.rs/serde/1/serde/de/trait.Deserialize.html>
/// [`Serialize`\]: <https://docs.rs/serde/1/serde/trait.Serialize.html>
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct BorrowCompat<T>(pub T);

impl<'de, T, Context> crate::de::BorrowDecode<'de, Context> for BorrowCompat<T>
where
    T: serde::de::Deserialize<'de>,
{
    fn borrow_decode<D: crate::de::BorrowDecoder<'de>>(
        decoder: &mut D
    ) -> Result<Self, crate::error::DecodeError> {
        let serde_decoder = de_borrowed::SerdeDecoder {
            de: decoder,
            pd: core::marker::PhantomData,
        };
        T::deserialize(serde_decoder).map(BorrowCompat)
    }
}

impl<T> crate::Encode for BorrowCompat<T>
where
    T: serde::Serialize,
{
    fn encode<E: crate::enc::Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), crate::error::EncodeError> {
        let serializer = ser::SerdeEncoder { enc: encoder };
        self.0.serialize(serializer)?;
        Ok(())
    }
}

impl<T> core::fmt::Debug for BorrowCompat<T>
where
    T: core::fmt::Debug,
{
    fn fmt(
        &self,
        f: &mut core::fmt::Formatter<'_>,
    ) -> core::fmt::Result {
        f.debug_tuple("BorrowCompat").field(&self.0).finish()
    }
}

impl<T> core::fmt::Display for BorrowCompat<T>
where
    T: core::fmt::Display,
{
    fn fmt(
        &self,
        f: &mut core::fmt::Formatter<'_>,
    ) -> core::fmt::Result {
        self.0.fmt(f)
    }
}
