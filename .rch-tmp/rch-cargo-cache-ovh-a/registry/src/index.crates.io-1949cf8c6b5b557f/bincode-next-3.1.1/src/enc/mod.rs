//! Encoder-based structs and traits.

pub(crate) mod deterministic;
mod encoder;
mod impl_tuples;
mod impls;

use self::write::Writer;
use crate::config::Config;
use crate::error::EncodeError;
use crate::utils::Sealed;

/// Bit-level writer for space-optimized packing.
pub mod bit_writer;
pub(crate) mod cbor;
pub mod write;

pub use self::encoder::EncoderImpl;

/// Any source that can be encoded. This trait should be implemented for all types that you want to be able to use with any of the `encode_with` methods.
///
/// This trait will be automatically implemented if you enable the `derive` feature and add `#[derive(bincode::Encode)]` to your trait.
///
/// # Implementing this trait manually
///
/// If you want to implement this trait for your type, the easiest way is to add a `#[derive(bincode::Encode)]`, build and check your `target/generated/bincode/` folder. This should generate a `<Struct name>_Encode.rs` file.
///
/// For this struct:
///
/// ```
/// struct Entity {
///     pub x: f32,
///     pub y: f32,
/// }
/// ```
/// It will look something like:
///
/// ```
/// # struct Entity {
/// #     pub x: f32,
/// #     pub y: f32,
/// # }
/// impl bincode_next::Encode for Entity {
///     fn encode<E: bincode_next::enc::Encoder>(
///         &self,
///         encoder: &mut E,
///     ) -> core::result::Result<(), bincode_next::error::EncodeError> {
///         bincode_next::Encode::encode(&self.x, encoder)?;
///         bincode_next::Encode::encode(&self.y, encoder)?;
///         Ok(())
///     }
/// }
/// ```
///
/// From here you can add/remove fields, or add custom logic.
pub trait Encode {
    /// Encode a given type.
    ///
    /// # Errors
    ///
    /// Returns any error encountered during encoding.
    fn encode<E: Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), EncodeError>;
}

/// Helper trait to encode basic types into.
pub trait Encoder: Sealed + crate::error_path::BincodeErrorPathCovered<1> {
    /// The concrete [Writer] type
    type W: Writer;

    /// The concrete [Config] type
    type C: Config;

    /// Returns a mutable reference to the writer
    fn writer(&mut self) -> &mut Self::W;

    /// Returns a reference to the config
    ///
    /// # Errors
    ///
    /// Returns `EncodeError` if the encoding fails.
    fn config(&self) -> &Self::C;

    /// Encode a `u8` value.
    ///
    /// # Errors
    ///
    /// Returns `EncodeError` if the encoding fails.
    #[inline(always)]
    fn encode_u8(
        &mut self,
        val: u8,
    ) -> Result<(), EncodeError> {
        use crate::config::Format;
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => self.writer().write_u8(val),
            | Format::Cbor | Format::CborDeterministic => {
                cbor::encode_u8::<_, Self::C>(self.writer(), val)
            },
        }
    }

    /// Encode a `u16` value.
    ///
    /// # Errors
    ///
    /// Returns `EncodeError` if the encoding fails.
    #[inline(always)]
    fn encode_u16(
        &mut self,
        val: u16,
    ) -> Result<(), EncodeError> {
        use crate::config::Endianness;
        use crate::config::Format;
        use crate::config::IntEncoding;
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => {
                match <Self::C as crate::config::InternalIntEncodingConfig>::INT_ENCODING {
                    | IntEncoding::Variable => {
                        crate::varint::varint_encode_u16(
                            self.writer(),
                            <Self::C as crate::config::InternalEndianConfig>::ENDIAN,
                            val,
                        )
                    },
                    | IntEncoding::Fixed => {
                        match <Self::C as crate::config::InternalEndianConfig>::ENDIAN {
                            | Endianness::Big => self.writer().write_u16(val.to_be()),
                            | Endianness::Little => self.writer().write_u16(val.to_le()),
                        }
                    },
                }
            },
            | Format::Cbor | Format::CborDeterministic => {
                cbor::encode_u16::<_, Self::C>(self.writer(), val)
            },
        }
    }

    /// Encode a `u32` value.
    ///
    /// # Errors
    ///
    /// Returns `EncodeError` if the encoding fails.
    #[inline(always)]
    fn encode_u32(
        &mut self,
        val: u32,
    ) -> Result<(), EncodeError> {
        use crate::config::Endianness;
        use crate::config::Format;
        use crate::config::IntEncoding;
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => {
                match <Self::C as crate::config::InternalIntEncodingConfig>::INT_ENCODING {
                    | IntEncoding::Variable => {
                        crate::varint::varint_encode_u32(
                            self.writer(),
                            <Self::C as crate::config::InternalEndianConfig>::ENDIAN,
                            val,
                        )
                    },
                    | IntEncoding::Fixed => {
                        match <Self::C as crate::config::InternalEndianConfig>::ENDIAN {
                            | Endianness::Big => self.writer().write_u32(val.to_be()),
                            | Endianness::Little => self.writer().write_u32(val.to_le()),
                        }
                    },
                }
            },
            | Format::Cbor | Format::CborDeterministic => {
                cbor::encode_u32::<_, Self::C>(self.writer(), val)
            },
        }
    }

    /// Encode a `u64` value.
    ///
    /// # Errors
    ///
    /// Returns `EncodeError` if the encoding fails.
    #[inline(always)]
    fn encode_u64(
        &mut self,
        val: u64,
    ) -> Result<(), EncodeError> {
        use crate::config::Endianness;
        use crate::config::Format;
        use crate::config::IntEncoding;
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => {
                match <Self::C as crate::config::InternalIntEncodingConfig>::INT_ENCODING {
                    | IntEncoding::Variable => {
                        crate::varint::varint_encode_u64(
                            self.writer(),
                            <Self::C as crate::config::InternalEndianConfig>::ENDIAN,
                            val,
                        )
                    },
                    | IntEncoding::Fixed => {
                        match <Self::C as crate::config::InternalEndianConfig>::ENDIAN {
                            | Endianness::Big => self.writer().write_u64(val.to_be()),
                            | Endianness::Little => self.writer().write_u64(val.to_le()),
                        }
                    },
                }
            },
            | Format::Cbor | Format::CborDeterministic => {
                cbor::encode_u64::<_, Self::C>(self.writer(), val)
            },
        }
    }

    /// Encode a `u128` value.
    ///
    /// # Errors
    ///
    /// Returns `EncodeError` if the encoding fails.
    #[inline(always)]
    fn encode_u128(
        &mut self,
        val: u128,
    ) -> Result<(), EncodeError> {
        use crate::config::Endianness;
        use crate::config::Format;
        use crate::config::IntEncoding;
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => {
                match <Self::C as crate::config::InternalIntEncodingConfig>::INT_ENCODING {
                    | IntEncoding::Variable => {
                        crate::varint::varint_encode_u128(
                            self.writer(),
                            <Self::C as crate::config::InternalEndianConfig>::ENDIAN,
                            val,
                        )
                    },
                    | IntEncoding::Fixed => {
                        match <Self::C as crate::config::InternalEndianConfig>::ENDIAN {
                            | Endianness::Big => self.writer().write_u128(val.to_be()),
                            | Endianness::Little => self.writer().write_u128(val.to_le()),
                        }
                    },
                }
            },
            | Format::Cbor | Format::CborDeterministic => {
                cbor::encode_u128::<_, Self::C>(self.writer(), val)
            },
        }
    }

    /// Encode a `usize` value.
    ///
    /// # Errors
    ///
    /// Returns `EncodeError` if the encoding fails.
    #[inline(always)]
    fn encode_usize(
        &mut self,
        val: usize,
    ) -> Result<(), EncodeError> {
        use crate::config::Format;
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => self.encode_u64(val as u64),
            | Format::Cbor | Format::CborDeterministic => {
                cbor::encode_u64::<_, Self::C>(self.writer(), val as u64)
            },
        }
    }

    /// Encode an `i8` value.
    ///
    /// # Errors
    ///
    /// Returns `EncodeError` if the encoding fails.
    #[inline(always)]
    fn encode_i8(
        &mut self,
        val: i8,
    ) -> Result<(), EncodeError> {
        use crate::config::Format;
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => self.writer().write_u8(val as u8),
            | Format::Cbor | Format::CborDeterministic => {
                cbor::encode_i8::<_, Self::C>(self.writer(), val)
            },
        }
    }

    /// Encode an `i16` value.
    ///
    /// # Errors
    ///
    /// Returns `EncodeError` if the encoding fails.
    #[inline(always)]
    fn encode_i16(
        &mut self,
        val: i16,
    ) -> Result<(), EncodeError> {
        use crate::config::Endianness;
        use crate::config::Format;
        use crate::config::IntEncoding;
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => {
                match <Self::C as crate::config::InternalIntEncodingConfig>::INT_ENCODING {
                    | IntEncoding::Variable => {
                        crate::varint::varint_encode_i16(
                            self.writer(),
                            <Self::C as crate::config::InternalEndianConfig>::ENDIAN,
                            val,
                        )
                    },
                    | IntEncoding::Fixed => {
                        match <Self::C as crate::config::InternalEndianConfig>::ENDIAN {
                            | Endianness::Big => self.writer().write_u16(val.to_be() as u16),
                            | Endianness::Little => self.writer().write_u16(val.to_le() as u16),
                        }
                    },
                }
            },
            | Format::Cbor | Format::CborDeterministic => {
                cbor::encode_i16::<_, Self::C>(self.writer(), val)
            },
        }
    }

    /// Encode an `i32` value.
    ///
    /// # Errors
    ///
    /// Returns `EncodeError` if the encoding fails.
    #[inline(always)]
    fn encode_i32(
        &mut self,
        val: i32,
    ) -> Result<(), EncodeError> {
        use crate::config::Endianness;
        use crate::config::Format;
        use crate::config::IntEncoding;
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => {
                match <Self::C as crate::config::InternalIntEncodingConfig>::INT_ENCODING {
                    | IntEncoding::Variable => {
                        crate::varint::varint_encode_i32(
                            self.writer(),
                            <Self::C as crate::config::InternalEndianConfig>::ENDIAN,
                            val,
                        )
                    },
                    | IntEncoding::Fixed => {
                        match <Self::C as crate::config::InternalEndianConfig>::ENDIAN {
                            | Endianness::Big => self.writer().write_u32(val.to_be() as u32),
                            | Endianness::Little => self.writer().write_u32(val.to_le() as u32),
                        }
                    },
                }
            },
            | Format::Cbor | Format::CborDeterministic => {
                cbor::encode_i32::<_, Self::C>(self.writer(), val)
            },
        }
    }

    /// Encode an `i64` value.
    ///
    /// # Errors
    ///
    /// Returns `EncodeError` if the encoding fails.
    #[inline(always)]
    fn encode_i64(
        &mut self,
        val: i64,
    ) -> Result<(), EncodeError> {
        use crate::config::Endianness;
        use crate::config::Format;
        use crate::config::IntEncoding;
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => {
                match <Self::C as crate::config::InternalIntEncodingConfig>::INT_ENCODING {
                    | IntEncoding::Variable => {
                        crate::varint::varint_encode_i64(
                            self.writer(),
                            <Self::C as crate::config::InternalEndianConfig>::ENDIAN,
                            val,
                        )
                    },
                    | IntEncoding::Fixed => {
                        match <Self::C as crate::config::InternalEndianConfig>::ENDIAN {
                            | Endianness::Big => self.writer().write_u64(val.to_be() as u64),
                            | Endianness::Little => self.writer().write_u64(val.to_le() as u64),
                        }
                    },
                }
            },
            | Format::Cbor | Format::CborDeterministic => {
                cbor::encode_i64::<_, Self::C>(self.writer(), val)
            },
        }
    }

    /// Encode an `i128` value.
    ///
    /// # Errors
    ///
    /// Returns `EncodeError` if the encoding fails.
    #[inline(always)]
    fn encode_i128(
        &mut self,
        val: i128,
    ) -> Result<(), EncodeError> {
        use crate::config::Endianness;
        use crate::config::Format;
        use crate::config::IntEncoding;
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => {
                match <Self::C as crate::config::InternalIntEncodingConfig>::INT_ENCODING {
                    | IntEncoding::Variable => {
                        crate::varint::varint_encode_i128(
                            self.writer(),
                            <Self::C as crate::config::InternalEndianConfig>::ENDIAN,
                            val,
                        )
                    },
                    | IntEncoding::Fixed => {
                        match <Self::C as crate::config::InternalEndianConfig>::ENDIAN {
                            | Endianness::Big => self.writer().write_u128(val.to_be() as u128),
                            | Endianness::Little => self.writer().write_u128(val.to_le() as u128),
                        }
                    },
                }
            },
            | Format::Cbor | Format::CborDeterministic => {
                cbor::encode_i128::<_, Self::C>(self.writer(), val)
            },
        }
    }

    /// Encode an `isize` value.
    ///
    /// # Errors
    ///
    /// Returns `EncodeError` if the encoding fails.
    #[inline(always)]
    fn encode_isize(
        &mut self,
        val: isize,
    ) -> Result<(), EncodeError> {
        use crate::config::Format;
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => self.encode_i64(val as i64),
            | Format::Cbor | Format::CborDeterministic => {
                cbor::encode_i64::<_, Self::C>(self.writer(), val as i64)
            },
        }
    }

    /// Encode an `f32` value.
    ///
    /// # Errors
    ///
    /// Returns `EncodeError` if the encoding fails.
    #[inline(always)]
    fn encode_f32(
        &mut self,
        val: f32,
    ) -> Result<(), EncodeError> {
        use crate::config::Endianness;
        use crate::config::Format;
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => {
                match <Self::C as crate::config::InternalEndianConfig>::ENDIAN {
                    | Endianness::Big => self.writer().write_u32(val.to_bits().to_be()),
                    | Endianness::Little => self.writer().write_u32(val.to_bits().to_le()),
                }
            },
            | Format::Cbor | Format::CborDeterministic => {
                cbor::encode_f32::<_, Self::C>(self.writer(), val)
            },
        }
    }

    /// Encode an `f64` value.
    ///
    /// # Errors
    ///
    /// Returns `EncodeError` if the encoding fails.
    #[inline(always)]
    fn encode_f64(
        &mut self,
        val: f64,
    ) -> Result<(), EncodeError> {
        use crate::config::Endianness;
        use crate::config::Format;
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => {
                match <Self::C as crate::config::InternalEndianConfig>::ENDIAN {
                    | Endianness::Big => self.writer().write_u64(val.to_bits().to_be()),
                    | Endianness::Little => self.writer().write_u64(val.to_bits().to_le()),
                }
            },
            | Format::Cbor | Format::CborDeterministic => {
                cbor::encode_f64::<_, Self::C>(self.writer(), val)
            },
        }
    }

    /// Encode a `bool` value.
    ///
    /// # Errors
    ///
    /// Returns `EncodeError` if the encoding fails.
    #[inline(always)]
    fn encode_bool(
        &mut self,
        val: bool,
    ) -> Result<(), EncodeError> {
        use crate::config::Format;
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => self.encode_u8(u8::from(val)),
            | Format::Cbor | Format::CborDeterministic => {
                cbor::encode_bool::<_, Self::C>(self.writer(), val)
            },
        }
    }

    /// Encode a string.
    ///
    /// # Errors
    ///
    /// Returns `EncodeError` if the encoding fails.
    #[inline(always)]
    fn encode_str(
        &mut self,
        val: &str,
    ) -> Result<(), EncodeError> {
        use crate::config::Format;
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => {
                self.encode_slice_len(val.len())?;
                self.writer().write(val.as_bytes())
            },
            | Format::Cbor | Format::CborDeterministic => {
                cbor::encode_str::<_, Self::C>(self.writer(), val)
            },
        }
    }

    /// Encode the length of a slice.
    ///
    /// # Errors
    ///
    /// Returns `EncodeError` if the encoding fails.
    #[inline(always)]
    fn encode_slice_len(
        &mut self,
        len: usize,
    ) -> Result<(), EncodeError> {
        use crate::config::Format;
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => self.encode_u64(len as u64),
            | Format::Cbor | Format::CborDeterministic => {
                cbor::encode_slice_len::<_, Self::C>(self.writer(), len)
            },
        }
    }

    /// Encode the length of an array.
    ///
    /// # Errors
    ///
    /// Returns `EncodeError` if the encoding fails.
    #[inline(always)]
    fn encode_array_len(
        &mut self,
        len: usize,
    ) -> Result<(), EncodeError> {
        self.encode_slice_len(len)
    }

    /// Encode the length of a map.
    ///
    /// # Errors
    ///
    /// Returns `EncodeError` if the encoding fails.
    #[inline(always)]
    fn encode_map_len(
        &mut self,
        len: usize,
    ) -> Result<(), EncodeError> {
        use crate::config::Format;
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => self.encode_u64(len as u64),
            | Format::Cbor | Format::CborDeterministic => {
                cbor::encode_map_len::<_, Self::C>(self.writer(), len)
            },
        }
    }

    /// Encode a byte slice.
    ///
    /// # Errors
    ///
    /// Returns `EncodeError` if the encoding fails.
    #[inline(always)]
    fn encode_byte_slice(
        &mut self,
        val: &[u8],
    ) -> Result<(), EncodeError> {
        use crate::config::Format;
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => {
                self.encode_slice_len(val.len())?;
                self.writer().write(val)
            },
            | Format::Cbor | Format::CborDeterministic => {
                cbor::encode_byte_slice::<_, Self::C>(self.writer(), val)
            },
        }
    }

    /// Encode a struct header.
    ///
    /// # Errors
    ///
    /// Returns `EncodeError` if the encoding fails.
    #[inline(always)]
    fn encode_struct_header(
        &mut self,
        len: usize,
    ) -> Result<(), EncodeError> {
        use crate::config::Format;
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => Ok(()),
            | Format::Cbor | Format::CborDeterministic => self.encode_array_len(len),
        }
    }

    /// Encode the length of a byte slice.
    ///
    /// # Errors
    ///
    /// Returns `EncodeError` if the encoding fails.
    #[inline(always)]
    fn encode_byte_slice_len(
        &mut self,
        len: usize,
    ) -> Result<(), EncodeError> {
        use crate::config::Format;
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => self.encode_usize(len),
            | Format::Cbor | Format::CborDeterministic => {
                cbor::encode_byte_slice_len::<_, Self::C>(self.writer(), len as u64)
            },
        }
    }

    /// Encode an enum variant index.
    ///
    /// Variant indices are always encoded as a single `u8` for Bincode format,
    /// matching the decode side which uses `u8::decode()`.
    ///
    /// # Errors
    ///
    /// Returns `EncodeError` if the encoding fails.
    #[inline(always)]
    fn encode_variant_index(
        &mut self,
        idx: u32,
    ) -> Result<(), EncodeError> {
        use crate::config::Format;
        match <Self::C as crate::config::InternalFormatConfig>::FORMAT {
            | Format::Bincode | Format::BincodeDeterministic => self.encode_u8(idx as u8),
            | Format::Cbor | Format::CborDeterministic => {
                cbor::encode_u32::<_, Self::C>(self.writer(), idx)
            },
        }
    }
}

impl<T> crate::error_path::BincodeErrorPathCovered<1> for &mut T where
    T: crate::error_path::BincodeErrorPathCovered<1>
{
}

impl<T> Encoder for &mut T
where
    T: Encoder,
{
    type C = T::C;
    type W = T::W;

    #[inline(always)]
    fn writer(&mut self) -> &mut Self::W {
        T::writer(self)
    }

    #[inline(always)]
    fn config(&self) -> &Self::C {
        T::config(self)
    }

    #[inline(always)]
    fn encode_u8(
        &mut self,
        val: u8,
    ) -> Result<(), EncodeError> {
        T::encode_u8(self, val)
    }

    #[inline(always)]
    fn encode_u16(
        &mut self,
        val: u16,
    ) -> Result<(), EncodeError> {
        T::encode_u16(self, val)
    }

    #[inline(always)]
    fn encode_u32(
        &mut self,
        val: u32,
    ) -> Result<(), EncodeError> {
        T::encode_u32(self, val)
    }

    #[inline(always)]
    fn encode_u64(
        &mut self,
        val: u64,
    ) -> Result<(), EncodeError> {
        T::encode_u64(self, val)
    }

    #[inline(always)]
    fn encode_u128(
        &mut self,
        val: u128,
    ) -> Result<(), EncodeError> {
        T::encode_u128(self, val)
    }

    #[inline(always)]
    fn encode_usize(
        &mut self,
        val: usize,
    ) -> Result<(), EncodeError> {
        T::encode_usize(self, val)
    }

    #[inline(always)]
    fn encode_i8(
        &mut self,
        val: i8,
    ) -> Result<(), EncodeError> {
        self.encode_u8(val as u8)
    }

    #[inline(always)]
    fn encode_i16(
        &mut self,
        val: i16,
    ) -> Result<(), EncodeError> {
        T::encode_i16(self, val)
    }

    #[inline(always)]
    fn encode_i32(
        &mut self,
        val: i32,
    ) -> Result<(), EncodeError> {
        T::encode_i32(self, val)
    }

    #[inline(always)]
    fn encode_i64(
        &mut self,
        val: i64,
    ) -> Result<(), EncodeError> {
        T::encode_i64(self, val)
    }

    #[inline(always)]
    fn encode_i128(
        &mut self,
        val: i128,
    ) -> Result<(), EncodeError> {
        T::encode_i128(self, val)
    }

    #[inline(always)]
    fn encode_isize(
        &mut self,
        val: isize,
    ) -> Result<(), EncodeError> {
        T::encode_isize(self, val)
    }

    #[inline(always)]
    fn encode_f32(
        &mut self,
        val: f32,
    ) -> Result<(), EncodeError> {
        T::encode_f32(self, val)
    }

    #[inline(always)]
    fn encode_f64(
        &mut self,
        val: f64,
    ) -> Result<(), EncodeError> {
        T::encode_f64(self, val)
    }

    #[inline(always)]
    fn encode_bool(
        &mut self,
        val: bool,
    ) -> Result<(), EncodeError> {
        T::encode_bool(self, val)
    }

    #[inline(always)]
    fn encode_str(
        &mut self,
        val: &str,
    ) -> Result<(), EncodeError> {
        T::encode_str(self, val)
    }

    #[inline(always)]
    fn encode_slice_len(
        &mut self,
        len: usize,
    ) -> Result<(), EncodeError> {
        T::encode_slice_len(self, len)
    }

    #[inline(always)]
    fn encode_array_len(
        &mut self,
        len: usize,
    ) -> Result<(), EncodeError> {
        T::encode_array_len(self, len)
    }

    #[inline(always)]
    fn encode_map_len(
        &mut self,
        len: usize,
    ) -> Result<(), EncodeError> {
        T::encode_map_len(self, len)
    }

    #[inline(always)]
    fn encode_variant_index(
        &mut self,
        idx: u32,
    ) -> Result<(), EncodeError> {
        T::encode_variant_index(self, idx)
    }

    #[inline(always)]
    fn encode_byte_slice(
        &mut self,
        val: &[u8],
    ) -> Result<(), EncodeError> {
        T::encode_byte_slice(self, val)
    }

    #[inline(always)]
    fn encode_struct_header(
        &mut self,
        len: usize,
    ) -> Result<(), EncodeError> {
        T::encode_struct_header(self, len)
    }
}

/// Encode the variant of the given option. Will not encode the option itself.
///
/// # Errors
///
/// Returns `EncodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn encode_option_variant<E: Encoder, T>(
    encoder: &mut E,
    value: Option<&T>,
) -> Result<(), EncodeError> {
    E::assert_covered();
    match value {
        | None => 0u8.encode(encoder),
        | Some(_) => 1u8.encode(encoder),
    }
}

/// Encodes the length of any slice, container, etc into the given encoder
///
/// # Errors
///
/// Returns `EncodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn encode_slice_len<E: Encoder>(
    encoder: &mut E,
    len: usize,
) -> Result<(), EncodeError> {
    E::assert_covered();
    encoder.encode_slice_len(len)
}
