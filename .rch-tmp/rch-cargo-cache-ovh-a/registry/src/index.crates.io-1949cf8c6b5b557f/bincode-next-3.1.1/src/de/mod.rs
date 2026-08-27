//! Decoder-based structs and traits.
#![allow(clippy::used_underscore_binding)]

mod decoder;
#[doc(hidden)]
pub(crate) mod deterministic;
mod impl_core;
mod impl_tuples;
mod impls;

use self::decoder::WithContext;
use self::read::BorrowReader;
use self::read::Reader;
use crate::config::Config;
use crate::config::Endianness;
use crate::config::Format;
use crate::config::IntEncoding;
use crate::config::InternalLimitConfig;
use crate::error::DecodeError;
use crate::utils::Sealed;

/// Fiber-backed abstraction for zero-cost async decoding.
#[cfg(feature = "async-fiber")]
pub mod async_fiber;
/// Bit-level reader for space-optimized packing.
pub mod bit_reader;
pub(crate) mod cbor;
pub mod read;

pub use self::decoder::DecoderImpl;

/// Trait that makes a type able to be decoded, akin to serde's `DeserializeOwned` trait.
///
/// Some types may require specific contexts. For example, to decode arena-based collections, an arena allocator must be provided as a context. In these cases, the context type `Context` should be specified or bounded.
///
/// This trait should be implemented for types which do not have references to data in the reader. For types that contain e.g. `&str` and `&[u8]`, implement [`BorrowDecode`\] instead.
///
/// Whenever you derive `Decode` for your type, the base trait `BorrowDecode` is automatically implemented.
///
/// This trait will be automatically implemented with unbounded `Context` if you enable the `derive` feature and add `#[derive(bincode::Decode)]` to your type. Note that if the type contains any lifetimes, `BorrowDecode` will be implemented instead.
///
/// # Implementing this trait manually
///
/// If you want to implement this trait for your type, the easiest way is to add a `#[derive(bincode::Decode)]`, build and check your `target/generated/bincode/` folder. This should generate a `<Struct name>_Decode.rs` file.
///
/// For this struct:
///
/// ```
/// struct Entity {
///     pub x: f32,
///     pub y: f32,
/// }
/// ```
///
/// It will look something like:
///
/// ```
/// # struct Entity {
/// #     pub x: f32,
/// #     pub y: f32,
/// # }
/// impl<Context> bincode_next::Decode<Context> for Entity {
///     fn decode<D: bincode_next::de::Decoder<Context = Context>>(
///         decoder: &mut D
///     ) -> core::result::Result<Self, bincode_next::error::DecodeError> {
///         Ok(Self {
///             x: bincode_next::Decode::decode(decoder)?,
///             y: bincode_next::Decode::decode(decoder)?,
///         })
///     }
/// }
/// impl<'de, Context> bincode_next::BorrowDecode<'de, Context> for Entity {
///     fn borrow_decode<D: bincode_next::de::BorrowDecoder<'de, Context = Context>>(
///         decoder: &mut D
///     ) -> core::result::Result<Self, bincode_next::error::DecodeError> {
///         Ok(Self {
///             x: bincode_next::BorrowDecode::borrow_decode(decoder)?,
///             y: bincode_next::BorrowDecode::borrow_decode(decoder)?,
///         })
///     }
/// }
/// ```
///
/// From here you can add/remove fields, or add custom logic.
///
/// To get specific integer types, you can use:
/// ```
/// # struct Foo;
/// # impl<Context> bincode_next::Decode<Context> for Foo {
/// #     fn decode<D: bincode_next::de::Decoder<Context = Context>>(
/// #         decoder: &mut D,
/// #     ) -> core::result::Result<Self, bincode_next::error::DecodeError> {
/// let x: u8 = bincode_next::Decode::<Context>::decode(decoder)?;
/// let x = <u8 as bincode_next::Decode<Context>>::decode(decoder)?;
/// #         Ok(Foo)
/// #     }
/// # }
/// # bincode_next::impl_borrow_decode!(Foo);
/// ```
///
/// You can use `Context` to require contexts for decoding a type:
/// ```
/// # /// # use bumpalo::Bump;
/// use bincode_next::de::Decoder;
/// use bincode_next::error::DecodeError;
/// struct BytesInArena<'a>(bumpalo::collections::Vec<'a, u8>);
/// impl<'a> bincode_next::Decode<&'a bumpalo::Bump> for BytesInArena<'a> {
///
/// fn decode<D: Decoder>(decoder: &mut D) -> Result<Self, DecodeError> {
///         todo!()
///     }
/// # }
/// ```
pub trait Decode<Context>: Sized {
    /// Attempt to decode this type with the given [`Decode`\].
    ///
    /// # Errors
    ///
    /// Returns any error encountered during decoding.
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError>;
}

/// Trait that makes a type able to be decoded, akin to serde's `Deserialize` trait.
///
/// This trait should be implemented for types that contain borrowed data, like `&str` and `&[u8]`. If your type does not have borrowed data, consider implementing [`Decode`\] instead.
///
/// This trait will be automatically implemented if you enable the `derive` feature and add `#[derive(bincode::Decode)]` to a type with a lifetime.
pub trait BorrowDecode<'de, Context>: Sized {
    /// Attempt to decode this type with the given [`BorrowDecode`\].
    ///
    /// # Errors
    ///
    /// Returns any error encountered during decoding.
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D
    ) -> Result<Self, DecodeError>;
}

/// Helper macro to implement `BorrowDecode` for any type that implements `Decode`.
#[macro_export]
#[doc(hidden)]
macro_rules! impl_borrow_decode {
    ($ty:ty $(, $param:tt)*) => {
        impl<'de $(, $param)*, __Context> $crate::BorrowDecode<'de, __Context> for $ty {
            #[inline(always)]
            fn borrow_decode<D: $crate::de::BorrowDecoder<'de, Context = __Context>>(
                decoder: &mut D,
            ) -> core::result::Result<Self, $crate::error::DecodeError> {
                $crate::Decode::decode(decoder)
            }
        }
    };
}

/// Helper macro to implement `BorrowDecode` for any type that implements `Decode`.
#[macro_export]
#[doc(hidden)]
macro_rules! impl_borrow_decode_with_context {
    ($ty:ty, $context:ty $(, $param:tt)*) => {
        impl<'de $(, $param)*> $crate::BorrowDecode<'de, $context> for $ty {
            #[inline(always)]
            fn borrow_decode<D: $crate::de::BorrowDecoder<'de, Context = $context>>(
                decoder: &mut D,
            ) -> core::result::Result<Self, $crate::error::DecodeError> {
                $crate::Decode::decode(decoder)
            }
        }
    };
}

/// Any source that can decode basic types. This type is most notably implemented for [Decoder].
pub trait Decoder: Sealed + crate::error_path::BincodeErrorPathCovered<0> {
    /// The concrete [Reader] type
    type R: Reader;

    /// The concrete [Config] type
    type C: Config;

    /// The decoding context type
    type Context;

    /// Returns the decoding context
    fn context(&mut self) -> &mut Self::Context;

    /// Wraps decoder with a context
    #[inline(always)]
    fn with_context<C>(
        &mut self,
        context: C,
    ) -> WithContext<'_, Self, C> {
        WithContext {
            decoder: self,
            context,
        }
    }

    /// Returns a mutable reference to the reader
    fn reader(&mut self) -> &mut Self::R;

    /// Returns a reference to the config
    fn config(&self) -> &Self::C;

    /// Claim that `n` bytes are going to be read from the decoder.
    /// This can be used to validate `Configuration::Limit<N>()`.
    ///
    /// # Errors
    ///
    /// Returns `DecodeError::LimitExceeded` if the limit is exceeded.
    fn claim_bytes_read(
        &mut self,
        n: usize,
    ) -> Result<(), DecodeError>;

    /// Claim that we're going to read a container which contains `len` entries of `T`.
    /// This will correctly handle overflowing if `len * size_of::<T>() > usize::max_value`
    ///
    /// # Errors
    ///
    /// Returns `DecodeError::LimitExceeded` if the limit is exceeded or if `len * size_of::<T>()` overflows.
    #[inline(always)]
    fn claim_container_read<T>(
        &mut self,
        len: usize,
    ) -> Result<(), DecodeError> {
        Self::assert_covered();
        if <Self::C as InternalLimitConfig>::LIMIT.is_some() {
            len.checked_mul(core::mem::size_of::<T>()).map_or_else(
                || crate::error::cold_decode_error_limit_exceeded(),
                |val| self.claim_bytes_read(val),
            )
        } else {
            Ok(())
        }
    }

    /// Notify the decoder that `n` bytes are being reclaimed.
    ///
    /// When decoding container types, a typical implementation would claim to read `len * size_of::<T>()` bytes.
    /// This is to ensure that bincode won't allocate several GB of memory while constructing the container.
    ///
    /// Because the implementation claims `len * size_of::<T>()`, but then has to decode each `T`, this would be marked
    /// as double. This function allows us to un-claim each `T` that gets decoded.
    ///
    /// We cannot check if `len * size_of::<T>()` is valid without claiming it, because this would mean that if you have
    /// a nested container (e.g. `Vec<Vec<T>>`), it does not know how much memory is already claimed, and could easily
    /// allocate much more than the user intends.
    /// ```
    /// # use bincode_next::de::{Decode, Decoder};
    /// # use bincode_next::error::DecodeError;
    /// # struct Container<T>(Vec<T>);
    /// # impl<T> Container<T> {
    /// #     fn with_capacity(cap: usize) -> Self {
    /// #         Self(Vec::with_capacity(cap))
    /// #     }
    /// #
    ///
    /// #     fn push(&mut self, t: T) {
    /// #         self.0.push(t);
    /// #     }
    /// # }
    /// impl<Context, T: Decode<Context>> Decode<Context> for Container<T> {
    ///     fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
    ///         let len = u64::decode(decoder)?;
    ///         let len: usize = len
    ///             .try_into()
    ///             .map_err(|_| DecodeError::OutsideUsizeRange(len))?;
    ///         // Make sure we don't allocate too much memory
    ///         decoder.claim_bytes_read(len * core::mem::size_of::<T>());
    ///
    ///         let mut result = Container::with_capacity(len);
    ///         for _ in 0..len {
    ///             // un-claim the memory
    ///             decoder.unclaim_bytes_read(core::mem::size_of::<T>());
    ///             result.push(T::decode(decoder)?)
    ///         }
    ///         Ok(result)
    ///     }
    /// }
    /// impl<'de, Context, T: bincode_next::BorrowDecode<'de, Context>>
    ///     bincode_next::BorrowDecode<'de, Context> for Container<T>
    /// {
    ///     fn borrow_decode<D: bincode_next::de::BorrowDecoder<'de, Context = Context>>(
    ///         decoder: &mut D
    ///     ) -> core::result::Result<Self, bincode_next::error::DecodeError> {
    ///         let len = u64::borrow_decode(decoder)?;
    ///         let len: usize = len
    ///             .try_into()
    ///             .map_err(|_| DecodeError::OutsideUsizeRange(len))?;
    ///         // Make sure we don't allocate too much memory
    ///         decoder.claim_bytes_read(len * core::mem::size_of::<T>());
    ///
    ///         let mut result = Container::with_capacity(len);
    ///         for _ in 0..len {
    ///             // un-claim the memory
    ///             decoder.unclaim_bytes_read(core::mem::size_of::<T>());
    ///             result.push(T::borrow_decode(decoder)?)
    ///         }
    ///         Ok(result)
    ///     }
    /// }
    /// ```
    fn unclaim_bytes_read(
        &mut self,
        n: usize,
    );

    /// Decode a `u8` value.
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails.
    #[inline(always)]
    fn decode_u8(&mut self) -> Result<u8, DecodeError> {
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => self.reader().read_u8(),
            | Format::Cbor | Format::CborDeterministic => cbor::decode_u8(self.reader()),
        }
    }

    /// Decode a `u16` value.
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails.
    #[inline(always)]
    fn decode_u16(&mut self) -> Result<u16, DecodeError> {
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => {
                match <Self::C as crate::config::InternalIntEncodingConfig>::INT_ENCODING {
                    | IntEncoding::Variable => {
                        crate::varint::varint_decode_u16(
                            self.reader(),
                            <Self::C as crate::config::InternalEndianConfig>::ENDIAN,
                        )
                    },
                    | IntEncoding::Fixed => {
                        let val = self.reader().read_u16()?;
                        match <Self::C as crate::config::InternalEndianConfig>::ENDIAN {
                            | Endianness::Big => Ok(u16::from_be(val)),
                            | Endianness::Little => Ok(u16::from_le(val)),
                        }
                    },
                }
            },
            | Format::Cbor | Format::CborDeterministic => cbor::decode_u16(self.reader()),
        }
    }

    /// Decode a `u32` value.
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails.
    #[inline(always)]
    fn decode_u32(&mut self) -> Result<u32, DecodeError> {
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => {
                match <Self::C as crate::config::InternalIntEncodingConfig>::INT_ENCODING {
                    | IntEncoding::Variable => {
                        crate::varint::varint_decode_u32(
                            self.reader(),
                            <Self::C as crate::config::InternalEndianConfig>::ENDIAN,
                        )
                    },
                    | IntEncoding::Fixed => {
                        let val = self.reader().read_u32()?;
                        match <Self::C as crate::config::InternalEndianConfig>::ENDIAN {
                            | Endianness::Big => Ok(u32::from_be(val)),
                            | Endianness::Little => Ok(u32::from_le(val)),
                        }
                    },
                }
            },
            | Format::Cbor | Format::CborDeterministic => cbor::decode_u32(self.reader()),
        }
    }

    /// Decode a `u64` value.
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails.
    #[inline(always)]
    fn decode_u64(&mut self) -> Result<u64, DecodeError> {
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => {
                match <Self::C as crate::config::InternalIntEncodingConfig>::INT_ENCODING {
                    | IntEncoding::Variable => {
                        crate::varint::varint_decode_u64(
                            self.reader(),
                            <Self::C as crate::config::InternalEndianConfig>::ENDIAN,
                        )
                    },
                    | IntEncoding::Fixed => {
                        let val = self.reader().read_u64()?;
                        match <Self::C as crate::config::InternalEndianConfig>::ENDIAN {
                            | Endianness::Big => Ok(u64::from_be(val)),
                            | Endianness::Little => Ok(u64::from_le(val)),
                        }
                    },
                }
            },
            | Format::Cbor | Format::CborDeterministic => cbor::decode_u64(self.reader()),
        }
    }

    /// Decode a `u128` value.
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails.
    #[inline(always)]
    fn decode_u128(&mut self) -> Result<u128, DecodeError> {
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => {
                match <Self::C as crate::config::InternalIntEncodingConfig>::INT_ENCODING {
                    | IntEncoding::Variable => {
                        crate::varint::varint_decode_u128(
                            self.reader(),
                            <Self::C as crate::config::InternalEndianConfig>::ENDIAN,
                        )
                    },
                    | IntEncoding::Fixed => {
                        let val = self.reader().read_u128()?;
                        match <Self::C as crate::config::InternalEndianConfig>::ENDIAN {
                            | Endianness::Big => Ok(u128::from_be(val)),
                            | Endianness::Little => Ok(u128::from_le(val)),
                        }
                    },
                }
            },
            | Format::Cbor | Format::CborDeterministic => cbor::decode_u128(self.reader()),
        }
    }

    /// Decode a `usize` value.
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails.
    #[inline(always)]
    fn decode_usize(&mut self) -> Result<usize, DecodeError> {
        self.claim_bytes_read(8)?;
        let v = self.decode_u64()?;
        v.try_into()
            .map_err(|_| crate::error::cold_decode_error_outside_usize_range::<()>(v).unwrap_err())
    }

    /// Decode an `i8` value.
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails.
    #[inline(always)]
    fn decode_i8(&mut self) -> Result<i8, DecodeError> {
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => {
                self.reader().read_u8().map(|v| v as i8)
            },
            | Format::Cbor | Format::CborDeterministic => cbor::decode_i8(self.reader()),
        }
    }

    /// Decode an `i16` value.
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails.
    #[inline(always)]
    fn decode_i16(&mut self) -> Result<i16, DecodeError> {
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => {
                match <Self::C as crate::config::InternalIntEncodingConfig>::INT_ENCODING {
                    | IntEncoding::Variable => {
                        crate::varint::varint_decode_i16(
                            self.reader(),
                            <Self::C as crate::config::InternalEndianConfig>::ENDIAN,
                        )
                    },
                    | IntEncoding::Fixed => {
                        let val = self.reader().read_u16()?;
                        match <Self::C as crate::config::InternalEndianConfig>::ENDIAN {
                            | Endianness::Big => Ok(u16::from_be(val) as i16),
                            | Endianness::Little => Ok(u16::from_le(val) as i16),
                        }
                    },
                }
            },
            | Format::Cbor | Format::CborDeterministic => cbor::decode_i16(self.reader()),
        }
    }

    /// Decode an `i32` value.
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails.
    #[inline(always)]
    fn decode_i32(&mut self) -> Result<i32, DecodeError> {
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => {
                match <Self::C as crate::config::InternalIntEncodingConfig>::INT_ENCODING {
                    | IntEncoding::Variable => {
                        crate::varint::varint_decode_i32(
                            self.reader(),
                            <Self::C as crate::config::InternalEndianConfig>::ENDIAN,
                        )
                    },
                    | IntEncoding::Fixed => {
                        let val = self.reader().read_u32()?;
                        match <Self::C as crate::config::InternalEndianConfig>::ENDIAN {
                            | Endianness::Big => Ok(u32::from_be(val) as i32),
                            | Endianness::Little => Ok(u32::from_le(val) as i32),
                        }
                    },
                }
            },
            | Format::Cbor | Format::CborDeterministic => cbor::decode_i32(self.reader()),
        }
    }

    /// Decode an `i64` value.
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails.
    #[inline(always)]
    fn decode_i64(&mut self) -> Result<i64, DecodeError> {
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => {
                match <Self::C as crate::config::InternalIntEncodingConfig>::INT_ENCODING {
                    | IntEncoding::Variable => {
                        crate::varint::varint_decode_i64(
                            self.reader(),
                            <Self::C as crate::config::InternalEndianConfig>::ENDIAN,
                        )
                    },
                    | IntEncoding::Fixed => {
                        let val = self.reader().read_u64()?;
                        match <Self::C as crate::config::InternalEndianConfig>::ENDIAN {
                            | Endianness::Big => Ok(u64::from_be(val) as i64),
                            | Endianness::Little => Ok(u64::from_le(val) as i64),
                        }
                    },
                }
            },
            | Format::Cbor | Format::CborDeterministic => cbor::decode_i64(self.reader()),
        }
    }

    /// Decode an `i128` value.
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails.
    #[inline(always)]
    fn decode_i128(&mut self) -> Result<i128, DecodeError> {
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => {
                match <Self::C as crate::config::InternalIntEncodingConfig>::INT_ENCODING {
                    | IntEncoding::Variable => {
                        crate::varint::varint_decode_i128(
                            self.reader(),
                            <Self::C as crate::config::InternalEndianConfig>::ENDIAN,
                        )
                    },
                    | IntEncoding::Fixed => {
                        let val = self.reader().read_u128()?;
                        match <Self::C as crate::config::InternalEndianConfig>::ENDIAN {
                            | Endianness::Big => Ok(u128::from_be(val) as i128),
                            | Endianness::Little => Ok(u128::from_le(val) as i128),
                        }
                    },
                }
            },
            | Format::Cbor | Format::CborDeterministic => cbor::decode_i128(self.reader()),
        }
    }

    /// Decode an `isize` value.
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails.
    #[inline(always)]
    fn decode_isize(&mut self) -> Result<isize, DecodeError> {
        self.claim_bytes_read(8)?;
        let v = self.decode_i64()?;
        v.try_into()
            .map_err(|_| crate::error::cold_decode_error_outside_isize_range::<()>(v).unwrap_err())
    }

    /// Decode an `f32` value.
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails.
    #[inline(always)]
    fn decode_f32(&mut self) -> Result<f32, DecodeError> {
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => {
                let val = self.reader().read_u32()?;
                Ok(f32::from_bits(
                    match <Self::C as crate::config::InternalEndianConfig>::ENDIAN {
                        | Endianness::Big => u32::from_be(val),
                        | Endianness::Little => u32::from_le(val),
                    },
                ))
            },
            | Format::Cbor | Format::CborDeterministic => cbor::decode_f32(self.reader()),
        }
    }

    /// Decode an `f64` value.
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails.
    #[inline(always)]
    fn decode_f64(&mut self) -> Result<f64, DecodeError> {
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => {
                let val = self.reader().read_u64()?;
                Ok(f64::from_bits(
                    match <Self::C as crate::config::InternalEndianConfig>::ENDIAN {
                        | Endianness::Big => u64::from_be(val),
                        | Endianness::Little => u64::from_le(val),
                    },
                ))
            },
            | Format::Cbor | Format::CborDeterministic => cbor::decode_f64(self.reader()),
        }
    }

    /// Decode a `bool` value.
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails.
    #[inline(always)]
    fn decode_bool(&mut self) -> Result<bool, DecodeError> {
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => {
                match self.reader().read_u8()? {
                    | 0 => Ok(false),
                    | 1 => Ok(true),
                    | x => crate::error::cold_decode_error_invalid_boolean_value(x),
                }
            },
            | Format::Cbor | Format::CborDeterministic => cbor::decode_bool(self.reader()),
        }
    }

    /// Decode the length of a slice.
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails.
    #[inline(always)]
    fn decode_slice_len(&mut self) -> Result<usize, DecodeError> {
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => {
                self.claim_bytes_read(8)?;
                let v = self.decode_u64()?;
                v.try_into().map_err(|_| {
                    crate::error::cold_decode_error_outside_usize_range::<()>(v).unwrap_err()
                })
            },
            | Format::Cbor | Format::CborDeterministic => {
                self.claim_bytes_read(9)?;
                cbor::decode_slice_len(self.reader())
            },
        }
    }

    /// Decode the length of an array.
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails.
    #[inline(always)]
    fn decode_array_len(&mut self) -> Result<usize, DecodeError> {
        self.decode_slice_len()
    }

    /// Decode the length of a map.
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails.
    #[inline(always)]
    fn decode_map_len(&mut self) -> Result<usize, DecodeError> {
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => self.decode_slice_len(),
            | Format::Cbor | Format::CborDeterministic => {
                self.claim_bytes_read(9)?;
                cbor::decode_map_len(self.reader())
            },
        }
    }

    /// Decode an enum variant index.
    ///
    /// # Errors
    ///
    /// Returns `DecodeError` if the encoding fails.
    #[inline(always)]
    fn decode_variant_index(&mut self) -> Result<u32, DecodeError> {
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => {
                self.claim_bytes_read(1)?;
                self.decode_u8().map(u32::from)
            },
            | Format::Cbor | Format::CborDeterministic => {
                self.claim_bytes_read(5)?;
                cbor::decode_u32(self.reader())
            },
        }
    }

    /// Decode the length of a byte slice (Major Type 2 in CBOR).
    ///
    /// # Errors
    ///
    /// Returns `DecodeError` if the encoding fails.
    #[inline(always)]
    fn decode_byte_slice_len(&mut self) -> Result<usize, DecodeError> {
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => self.decode_slice_len(),
            | Format::Cbor | Format::CborDeterministic => {
                self.claim_bytes_read(9)?;
                cbor::decode_byte_slice_len(self.reader())
            },
        }
    }

    /// Decode the length of a byte slice or an array (Major Type 2 or 4 in CBOR).
    ///
    /// # Errors
    ///
    /// Returns `DecodeError` if the encoding fails.
    #[inline(always)]
    fn decode_byte_slice_or_array_len(&mut self) -> Result<(u8, usize), DecodeError> {
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => {
                self.decode_slice_len().map(|len| (0, len))
            },
            | Format::Cbor | Format::CborDeterministic => {
                self.claim_bytes_read(9)?;
                cbor::decode_byte_slice_or_array_len(self.reader())
            },
        }
    }

    /// Decode the length of a string (Major Type 3 in CBOR).
    ///
    /// # Errors
    ///
    /// Returns `DecodeError` if the encoding fails.
    #[inline(always)]
    fn decode_str_len(&mut self) -> Result<usize, DecodeError> {
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => self.decode_slice_len(),
            | Format::Cbor | Format::CborDeterministic => {
                self.claim_bytes_read(9)?;
                cbor::decode_str_len(self.reader())
            },
        }
    }

    /// Decode the header for a struct.
    ///
    /// # Errors
    ///
    /// Returns `DecodeError` if the encoding fails.
    #[inline(always)]
    fn decode_struct_header(
        &mut self,
        _len: usize,
    ) -> Result<(), DecodeError> {
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => Ok(()),
            | Format::Cbor | Format::CborDeterministic => {
                let actual_len = cbor::decode_slice_len(self.reader())?;
                if actual_len != _len && actual_len != usize::MAX {
                    return crate::error::cold_decode_error_other("struct length mismatch");
                }
                Ok(())
            },
        }
    }
}

/// Any source that can decode basic types. This type is most notably implemented for [Decoder].
///
/// This is an extension of [Decode] that can also return borrowed data.
pub trait BorrowDecoder<'de>: Decoder {
    /// The concrete [`BorrowReader`\] type
    type BR: BorrowReader<'de>;

    /// Returns a mutable reference to the borrow reader
    fn borrow_reader(&mut self) -> &mut Self::BR;
}

impl<T> crate::error_path::BincodeErrorPathCovered<0> for &mut T where
    T: crate::error_path::BincodeErrorPathCovered<0>
{
}

impl<T> Decoder for &mut T
where
    T: Decoder,
{
    type C = T::C;
    type Context = T::Context;
    type R = T::R;

    #[inline(always)]
    fn reader(&mut self) -> &mut Self::R {
        T::reader(self)
    }

    #[inline(always)]
    fn config(&self) -> &Self::C {
        T::config(self)
    }

    #[inline(always)]
    fn claim_bytes_read(
        &mut self,
        n: usize,
    ) -> Result<(), DecodeError> {
        T::claim_bytes_read(self, n)
    }

    #[inline(always)]
    fn unclaim_bytes_read(
        &mut self,
        n: usize,
    ) {
        T::unclaim_bytes_read(self, n);
    }

    #[inline(always)]
    fn context(&mut self) -> &mut Self::Context {
        T::context(self)
    }

    #[inline(always)]
    fn decode_u8(&mut self) -> Result<u8, DecodeError> {
        T::decode_u8(self)
    }

    #[inline(always)]
    fn decode_u16(&mut self) -> Result<u16, DecodeError> {
        T::decode_u16(self)
    }

    #[inline(always)]
    fn decode_u32(&mut self) -> Result<u32, DecodeError> {
        T::decode_u32(self)
    }

    #[inline(always)]
    fn decode_u64(&mut self) -> Result<u64, DecodeError> {
        T::decode_u64(self)
    }

    #[inline(always)]
    fn decode_u128(&mut self) -> Result<u128, DecodeError> {
        T::decode_u128(self)
    }

    #[inline(always)]
    fn decode_usize(&mut self) -> Result<usize, DecodeError> {
        T::decode_usize(self)
    }

    #[inline(always)]
    fn decode_i8(&mut self) -> Result<i8, DecodeError> {
        T::decode_i8(self)
    }

    #[inline(always)]
    fn decode_i16(&mut self) -> Result<i16, DecodeError> {
        T::decode_i16(self)
    }

    #[inline(always)]
    fn decode_i32(&mut self) -> Result<i32, DecodeError> {
        T::decode_i32(self)
    }

    #[inline(always)]
    fn decode_i64(&mut self) -> Result<i64, DecodeError> {
        T::decode_i64(self)
    }

    #[inline(always)]
    fn decode_i128(&mut self) -> Result<i128, DecodeError> {
        T::decode_i128(self)
    }

    #[inline(always)]
    fn decode_isize(&mut self) -> Result<isize, DecodeError> {
        T::decode_isize(self)
    }

    #[inline(always)]
    fn decode_f32(&mut self) -> Result<f32, DecodeError> {
        T::decode_f32(self)
    }

    #[inline(always)]
    fn decode_f64(&mut self) -> Result<f64, DecodeError> {
        T::decode_f64(self)
    }

    #[inline(always)]
    fn decode_bool(&mut self) -> Result<bool, DecodeError> {
        T::decode_bool(self)
    }

    #[inline(always)]
    fn decode_slice_len(&mut self) -> Result<usize, DecodeError> {
        T::decode_slice_len(self)
    }

    #[inline(always)]
    fn decode_array_len(&mut self) -> Result<usize, DecodeError> {
        T::decode_array_len(self)
    }

    #[inline(always)]
    fn decode_map_len(&mut self) -> Result<usize, DecodeError> {
        T::decode_map_len(self)
    }

    #[inline(always)]
    fn decode_variant_index(&mut self) -> Result<u32, DecodeError> {
        T::decode_variant_index(self)
    }

    #[inline(always)]
    fn decode_byte_slice_len(&mut self) -> Result<usize, DecodeError> {
        T::decode_byte_slice_len(self)
    }

    #[inline(always)]
    fn decode_str_len(&mut self) -> Result<usize, DecodeError> {
        T::decode_str_len(self)
    }

    #[inline(always)]
    fn decode_struct_header(
        &mut self,
        len: usize,
    ) -> Result<(), DecodeError> {
        T::decode_struct_header(self, len)
    }
}

impl<'de, T> BorrowDecoder<'de> for &mut T
where
    T: BorrowDecoder<'de>,
{
    type BR = T::BR;

    #[inline(always)]
    fn borrow_reader(&mut self) -> &mut Self::BR {
        T::borrow_reader(self)
    }
}

/// Decodes only the option variant from the decoder. Will not read any more data than that.
///
/// # Errors
///
/// Returns `DecodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn decode_option_variant<D: Decoder>(
    decoder: &mut D,
    type_name: &'static str,
) -> Result<Option<()>, DecodeError> {
    D::assert_covered();
    let is_some = u8::decode(decoder)?;
    match is_some {
        | 0 => Ok(None),
        | 1 => Ok(Some(())),
        | x => {
            crate::error::cold_decode_error_unexpected_variant(
                type_name,
                &crate::error::AllowedEnumVariants::Range { max: 1, min: 0 },
                u32::from(x),
            )
        },
    }
}

/// Decodes the length of any slice, container, etc from the decoder
///
/// # Errors
///
/// Returns an error if the operation fails.
#[inline(always)]
#[allow(dead_code)]
pub(crate) fn decode_slice_len<D: Decoder>(decoder: &mut D) -> Result<usize, DecodeError> {
    D::assert_covered();
    decoder.decode_slice_len()
}
