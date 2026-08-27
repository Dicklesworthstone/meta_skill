#![allow(unsafe_code, clippy::cast_possible_truncation)]
#![allow(clippy::redundant_else)]
use super::BorrowDecode;
use super::BorrowDecoder;
use super::Decode;
use super::Decoder;
use super::read::BorrowReader;
use super::read::Reader;
use crate::config::Endianness;
use crate::config::IntEncoding;
use crate::config::InternalEndianConfig;
use crate::config::InternalIntEncodingConfig;
use crate::error::DecodeError;
use crate::error::IntegerType;
use crate::impl_borrow_decode;
use core::cell::Cell;
use core::cell::RefCell;
use core::cmp::Reverse;
use core::num::NonZeroI8;
use core::num::NonZeroI16;
use core::num::NonZeroI32;
use core::num::NonZeroI64;
use core::num::NonZeroI128;
use core::num::NonZeroIsize;
use core::num::NonZeroU8;
use core::num::NonZeroU16;
use core::num::NonZeroU32;
use core::num::NonZeroU64;
use core::num::NonZeroU128;
use core::num::NonZeroUsize;
use core::num::Wrapping;
use core::ops::Bound;
use core::ops::Range;
use core::ops::RangeInclusive;
use core::time::Duration;

impl<Context> Decode<Context> for bool {
    #[inline(always)]
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        decoder.decode_bool()
    }
}
impl_borrow_decode!(bool);

impl<Context> Decode<Context> for u8 {
    #[inline(always)]
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        decoder.decode_u8()
    }
}
impl_borrow_decode!(u8);

impl<Context> Decode<Context> for NonZeroU8 {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        Self::new(u8::decode(decoder)?).ok_or_else(|| {
            crate::error::cold_decode_error_non_zero_type_is_zero::<()>(IntegerType::U8)
                .unwrap_err()
        })
    }
}
impl_borrow_decode!(NonZeroU8);

impl<Context> Decode<Context> for u16 {
    #[inline(always)]
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        decoder.decode_u16()
    }
}
impl_borrow_decode!(u16);

impl<Context> Decode<Context> for NonZeroU16 {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        Self::new(u16::decode(decoder)?).ok_or_else(|| {
            crate::error::cold_decode_error_non_zero_type_is_zero::<()>(IntegerType::U16)
                .unwrap_err()
        })
    }
}
impl_borrow_decode!(NonZeroU16);

impl<Context> Decode<Context> for u32 {
    #[inline(always)]
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        decoder.decode_u32()
    }
}
impl_borrow_decode!(u32);

impl<Context> Decode<Context> for NonZeroU32 {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        Self::new(u32::decode(decoder)?).ok_or_else(|| {
            crate::error::cold_decode_error_non_zero_type_is_zero::<()>(IntegerType::U32)
                .unwrap_err()
        })
    }
}
impl_borrow_decode!(NonZeroU32);

impl<Context> Decode<Context> for u64 {
    #[inline(always)]
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        decoder.decode_u64()
    }
}
impl_borrow_decode!(u64);

impl<Context> Decode<Context> for NonZeroU64 {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        Self::new(u64::decode(decoder)?).ok_or_else(|| {
            crate::error::cold_decode_error_non_zero_type_is_zero::<()>(IntegerType::U64)
                .unwrap_err()
        })
    }
}
impl_borrow_decode!(NonZeroU64);

impl<Context> Decode<Context> for u128 {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        decoder.decode_u128()
    }
}
impl_borrow_decode!(u128);

impl<Context> Decode<Context> for NonZeroU128 {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        Self::new(u128::decode(decoder)?).ok_or_else(|| {
            crate::error::cold_decode_error_non_zero_type_is_zero::<()>(IntegerType::U128)
                .unwrap_err()
        })
    }
}
impl_borrow_decode!(NonZeroU128);

impl<Context> Decode<Context> for usize {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        decoder.decode_usize()
    }
}
impl_borrow_decode!(usize);

impl<Context> Decode<Context> for NonZeroUsize {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        Self::new(usize::decode(decoder)?).ok_or_else(|| {
            crate::error::cold_decode_error_non_zero_type_is_zero::<()>(IntegerType::Usize)
                .unwrap_err()
        })
    }
}
impl_borrow_decode!(NonZeroUsize);

impl<Context> Decode<Context> for i8 {
    #[inline(always)]
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        decoder.decode_i8()
    }
}
impl_borrow_decode!(i8);

impl<Context> Decode<Context> for NonZeroI8 {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        Self::new(i8::decode(decoder)?).ok_or_else(|| {
            crate::error::cold_decode_error_non_zero_type_is_zero::<()>(IntegerType::I8)
                .unwrap_err()
        })
    }
}
impl_borrow_decode!(NonZeroI8);

impl<Context> Decode<Context> for i16 {
    #[inline(always)]
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        decoder.decode_i16()
    }
}
impl_borrow_decode!(i16);

impl<Context> Decode<Context> for NonZeroI16 {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        Self::new(i16::decode(decoder)?).ok_or_else(|| {
            crate::error::cold_decode_error_non_zero_type_is_zero::<()>(IntegerType::I16)
                .unwrap_err()
        })
    }
}
impl_borrow_decode!(NonZeroI16);

impl<Context> Decode<Context> for i32 {
    #[inline(always)]
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        decoder.decode_i32()
    }
}
impl_borrow_decode!(i32);

impl<Context> Decode<Context> for NonZeroI32 {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        Self::new(i32::decode(decoder)?).ok_or_else(|| {
            crate::error::cold_decode_error_non_zero_type_is_zero::<()>(IntegerType::I32)
                .unwrap_err()
        })
    }
}
impl_borrow_decode!(NonZeroI32);

impl<Context> Decode<Context> for i64 {
    #[inline(always)]
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        decoder.decode_i64()
    }
}
impl_borrow_decode!(i64);

impl<Context> Decode<Context> for NonZeroI64 {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        Self::new(i64::decode(decoder)?).ok_or_else(|| {
            crate::error::cold_decode_error_non_zero_type_is_zero::<()>(IntegerType::I64)
                .unwrap_err()
        })
    }
}
impl_borrow_decode!(NonZeroI64);

impl<Context> Decode<Context> for i128 {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        decoder.decode_i128()
    }
}
impl_borrow_decode!(i128);

impl<Context> Decode<Context> for NonZeroI128 {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        Self::new(i128::decode(decoder)?).ok_or_else(|| {
            crate::error::cold_decode_error_non_zero_type_is_zero::<()>(IntegerType::I128)
                .unwrap_err()
        })
    }
}
impl_borrow_decode!(NonZeroI128);

impl<Context> Decode<Context> for isize {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        decoder.decode_isize()
    }
}
impl_borrow_decode!(isize);

impl<Context> Decode<Context> for NonZeroIsize {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        Self::new(isize::decode(decoder)?).ok_or_else(|| {
            crate::error::cold_decode_error_non_zero_type_is_zero::<()>(IntegerType::Isize)
                .unwrap_err()
        })
    }
}
impl_borrow_decode!(NonZeroIsize);

impl<Context> Decode<Context> for f32 {
    #[inline(always)]
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        decoder.decode_f32()
    }
}
impl_borrow_decode!(f32);

impl<Context> Decode<Context> for f64 {
    #[inline(always)]
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        decoder.decode_f64()
    }
}
impl_borrow_decode!(f64);

impl<Context, T: Decode<Context>> Decode<Context> for Wrapping<T> {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        Ok(Self(T::decode(decoder)?))
    }
}
impl<'de, Context, T: BorrowDecode<'de, Context>> BorrowDecode<'de, Context> for Wrapping<T> {
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D
    ) -> Result<Self, DecodeError> {
        Ok(Self(T::borrow_decode(decoder)?))
    }
}

impl<Context, T: Decode<Context>> Decode<Context> for Reverse<T> {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        Ok(Self(T::decode(decoder)?))
    }
}

impl<'de, Context, T: BorrowDecode<'de, Context>> BorrowDecode<'de, Context> for Reverse<T> {
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D
    ) -> Result<Self, DecodeError> {
        Ok(Self(T::borrow_decode(decoder)?))
    }
}

impl<Context> Decode<Context> for char {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        let mut array = [0u8; 4];

        // Look at the first byte to see how many bytes must be read
        decoder.reader().read(&mut array[..1])?;

        let width = utf8_char_width(array[0]);
        if width == 0 {
            return crate::error::cold_decode_error_invalid_char_encoding(array);
        }
        // Normally we have to `.claim_bytes_read` before reading, however in this
        // case the amount of bytes read from `char` can vary wildly, and it should
        // only read up to 4 bytes too much.
        decoder.claim_bytes_read(width)?;
        if width == 1 {
            return Ok(array[0] as Self);
        }

        // read the remaining pain
        decoder.reader().read(&mut array[1..width])?;
        let res = core::str::from_utf8(&array[..width])
            .ok()
            .and_then(|s| s.chars().next())
            .ok_or_else(|| {
                crate::error::cold_decode_error_invalid_char_encoding::<()>(array).unwrap_err()
            })?;
        Ok(res)
    }
}
impl_borrow_decode!(char);

impl<'a, 'de: 'a, Context> BorrowDecode<'de, Context> for &'a [u8] {
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D
    ) -> Result<Self, DecodeError> {
        let len = decoder.decode_byte_slice_len()?;
        decoder.claim_bytes_read(len)?;
        decoder.borrow_reader().take_bytes(len)
    }
}

impl<'a, 'de: 'a, Context> BorrowDecode<'de, Context> for &'a str {
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D
    ) -> Result<Self, DecodeError> {
        let len = decoder.decode_str_len()?;
        decoder.claim_bytes_read(len)?;
        let slice = decoder.borrow_reader().take_bytes(len)?;
        core::str::from_utf8(slice)
            .map_err(|inner| crate::error::cold_decode_error_utf8::<()>(inner).unwrap_err())
    }
}

impl<Context, T, const N: usize> Decode<Context> for [T; N]
where
    T: Decode<Context>,
{
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        let is_u8 = unty::type_equal::<T, u8>() || unty::type_equal::<T, i8>();
        let is_bincode = matches!(
            <D::C as crate::config::InternalFormatConfig>::FORMAT,
            crate::config::Format::Bincode | crate::config::Format::BincodeDeterministic
        );

        if !is_bincode {
            if is_u8 {
                let len = decoder.decode_byte_slice_len()?;
                if len != N {
                    return crate::error::cold_decode_error_array_length_mismatch(N, len);
                }
                decoder.claim_bytes_read(N)?;
                let mut res = core::mem::MaybeUninit::<[T; N]>::uninit();
                unsafe {
                    let slice_ptr = res.as_mut_ptr().cast::<u8>();
                    let slice = core::slice::from_raw_parts_mut(slice_ptr, N);
                    decoder.reader().read(slice)?;
                    return Ok(res.assume_init());
                }
            } else {
                let len = decoder.decode_array_len()?;
                if len != N && len != usize::MAX {
                    return crate::error::cold_decode_error_array_length_mismatch(N, len);
                }
            }
        }

        let is_fixed = matches!(D::C::INT_ENCODING, IntEncoding::Fixed);
        let is_native_endian = match D::C::ENDIAN {
            | Endianness::Little => cfg!(target_endian = "little"),
            | Endianness::Big => cfg!(target_endian = "big"),
        };

        if is_bincode
            && (is_u8
                || (is_fixed
                    && is_native_endian
                    && (unty::type_equal::<T, u16>()
                        || unty::type_equal::<T, i16>()
                        || unty::type_equal::<T, u32>()
                        || unty::type_equal::<T, i32>()
                        || unty::type_equal::<T, u64>()
                        || unty::type_equal::<T, i64>()
                        || unty::type_equal::<T, u128>()
                        || unty::type_equal::<T, i128>()
                        || unty::type_equal::<T, f32>()
                        || unty::type_equal::<T, f64>())))
        {
            decoder.claim_bytes_read(core::mem::size_of::<[T; N]>())?;
            // SAFETY: T is a primitive type (pod), so it's safe to read its bytes directly.
            // We've checked that the encoding is Fixed and Endianness matches,
            // or that it's a 1-byte type (u8/i8).
            let mut res = core::mem::MaybeUninit::<[T; N]>::uninit();
            unsafe {
                let slice_ptr = res.as_mut_ptr().cast::<u8>();
                let slice =
                    core::slice::from_raw_parts_mut(slice_ptr, core::mem::size_of::<[T; N]>());
                decoder.reader().read(slice)?;
                Ok(res.assume_init())
            }
        } else {
            let result =
                super::impl_core::collect_into_array(&mut (0..N).map(|_| T::decode(decoder)));

            // result is only None if N does not match the values of `(0..N)`, which it always should
            // So this unwrap should never occur
            result.unwrap()
        }
    }
}

impl<'de, T, const N: usize, Context> BorrowDecode<'de, Context> for [T; N]
where
    T: BorrowDecode<'de, Context>,
{
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D
    ) -> Result<Self, DecodeError> {
        let is_u8 = unty::type_equal::<T, u8>() || unty::type_equal::<T, i8>();
        let is_bincode = matches!(
            <D::C as crate::config::InternalFormatConfig>::FORMAT,
            crate::config::Format::Bincode | crate::config::Format::BincodeDeterministic
        );

        if !is_bincode {
            if is_u8 {
                let len = decoder.decode_byte_slice_len()?;
                if len != N {
                    return crate::error::cold_decode_error_array_length_mismatch(N, len);
                }
                decoder.claim_bytes_read(N)?;
                let mut res = core::mem::MaybeUninit::<[T; N]>::uninit();
                unsafe {
                    let slice_ptr = res.as_mut_ptr().cast::<u8>();
                    let slice = core::slice::from_raw_parts_mut(slice_ptr, N);
                    decoder.reader().read(slice)?;
                    return Ok(res.assume_init());
                }
            } else {
                let len = decoder.decode_array_len()?;
                if len != N && len != usize::MAX {
                    return crate::error::cold_decode_error_array_length_mismatch(N, len);
                }
            }
        }

        let is_fixed = matches!(D::C::INT_ENCODING, IntEncoding::Fixed);
        let is_native_endian = match D::C::ENDIAN {
            | Endianness::Little => cfg!(target_endian = "little"),
            | Endianness::Big => cfg!(target_endian = "big"),
        };

        if is_bincode
            && (is_u8
                || (is_fixed
                    && is_native_endian
                    && (unty::type_equal::<T, u16>()
                        || unty::type_equal::<T, i16>()
                        || unty::type_equal::<T, u32>()
                        || unty::type_equal::<T, i32>()
                        || unty::type_equal::<T, u64>()
                        || unty::type_equal::<T, i64>()
                        || unty::type_equal::<T, u128>()
                        || unty::type_equal::<T, i128>()
                        || unty::type_equal::<T, f32>()
                        || unty::type_equal::<T, f64>())))
        {
            decoder.claim_bytes_read(core::mem::size_of::<[T; N]>())?;
            // SAFETY: T is a primitive type (pod), so it's safe to read its bytes directly.
            // We've checked that the encoding is Fixed and Endianness matches,
            // or that it's a 1-byte type (u8/i8).
            let mut res = core::mem::MaybeUninit::<[T; N]>::uninit();
            unsafe {
                let slice_ptr = res.as_mut_ptr().cast::<u8>();
                let slice =
                    core::slice::from_raw_parts_mut(slice_ptr, core::mem::size_of::<[T; N]>());
                decoder.reader().read(slice)?;
                Ok(res.assume_init())
            }
        } else {
            let result = super::impl_core::collect_into_array(
                &mut (0..N).map(|_| T::borrow_decode(decoder)),
            );

            // result is only None if N does not match the values of `(0..N)`, which it always should
            // So this unwrap should never occur
            result.unwrap()
        }
    }
}

impl<Context> Decode<Context> for () {
    fn decode<D: Decoder<Context = Context>>(_: &mut D) -> Result<Self, DecodeError> {
        Ok(())
    }
}
impl_borrow_decode!(());

impl<Context, T> Decode<Context> for core::marker::PhantomData<T> {
    fn decode<D: Decoder<Context = Context>>(_: &mut D) -> Result<Self, DecodeError> {
        Ok(Self)
    }
}
impl_borrow_decode!(core::marker::PhantomData<T>, T);

impl<Context, T> Decode<Context> for Option<T>
where
    T: Decode<Context>,
{
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        match super::decode_option_variant(decoder, core::any::type_name::<Self>())? {
            | Some(()) => {
                let val = T::decode(decoder)?;
                Ok(Some(val))
            },
            | None => Ok(None),
        }
    }
}

impl<'de, T, Context> BorrowDecode<'de, Context> for Option<T>
where
    T: BorrowDecode<'de, Context>,
{
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D
    ) -> Result<Self, DecodeError> {
        match super::decode_option_variant(decoder, core::any::type_name::<Self>())? {
            | Some(()) => {
                let val = T::borrow_decode(decoder)?;
                Ok(Some(val))
            },
            | None => Ok(None),
        }
    }
}

impl<Context, T, U> Decode<Context> for Result<T, U>
where
    T: Decode<Context>,
    U: Decode<Context>,
{
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        let is_ok = decoder.decode_variant_index()?;
        match is_ok {
            | 0 => {
                let t = T::decode(decoder)?;
                Ok(Ok(t))
            },
            | 1 => {
                let u = U::decode(decoder)?;
                Ok(Err(u))
            },
            | x => {
                crate::error::cold_decode_error_unexpected_variant(
                    core::any::type_name::<Self>(),
                    &crate::error::AllowedEnumVariants::Range { max: 1, min: 0 },
                    x,
                )
            },
        }
    }
}

impl<'de, T, U, Context> BorrowDecode<'de, Context> for Result<T, U>
where
    T: BorrowDecode<'de, Context>,
    U: BorrowDecode<'de, Context>,
{
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D
    ) -> Result<Self, DecodeError> {
        let is_ok = decoder.decode_variant_index()?;
        match is_ok {
            | 0 => {
                let t = T::borrow_decode(decoder)?;
                Ok(Ok(t))
            },
            | 1 => {
                let u = U::borrow_decode(decoder)?;
                Ok(Err(u))
            },
            | x => {
                crate::error::cold_decode_error_unexpected_variant(
                    core::any::type_name::<Self>(),
                    &crate::error::AllowedEnumVariants::Range { max: 1, min: 0 },
                    x,
                )
            },
        }
    }
}

impl<Context, T> Decode<Context> for Cell<T>
where
    T: Decode<Context>,
{
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        let t = T::decode(decoder)?;
        Ok(Self::new(t))
    }
}

impl<'de, T, Context> BorrowDecode<'de, Context> for Cell<T>
where
    T: BorrowDecode<'de, Context>,
{
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D
    ) -> Result<Self, DecodeError> {
        let t = T::borrow_decode(decoder)?;
        Ok(Self::new(t))
    }
}

impl<Context, T> Decode<Context> for RefCell<T>
where
    T: Decode<Context>,
{
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        let t = T::decode(decoder)?;
        Ok(Self::new(t))
    }
}

impl<'de, T, Context> BorrowDecode<'de, Context> for RefCell<T>
where
    T: BorrowDecode<'de, Context>,
{
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D
    ) -> Result<Self, DecodeError> {
        let t = T::borrow_decode(decoder)?;
        Ok(Self::new(t))
    }
}

impl<Context> Decode<Context> for Duration {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        const NANOS_PER_SEC: u64 = 1_000_000_000;
        let secs: u64 = Decode::decode(decoder)?;
        let nanos: u32 = Decode::decode(decoder)?;
        if secs.checked_add(u64::from(nanos) / NANOS_PER_SEC).is_none() {
            return crate::error::cold_decode_error_invalid_duration(secs, nanos);
        }
        Ok(Self::new(secs, nanos))
    }
}
impl_borrow_decode!(Duration);

impl<Context, T> Decode<Context> for Range<T>
where
    T: Decode<Context>,
{
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        let min = T::decode(decoder)?;
        let max = T::decode(decoder)?;
        Ok(min..max)
    }
}
impl<'de, T, Context> BorrowDecode<'de, Context> for Range<T>
where
    T: BorrowDecode<'de, Context>,
{
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D
    ) -> Result<Self, DecodeError> {
        let min = T::borrow_decode(decoder)?;
        let max = T::borrow_decode(decoder)?;
        Ok(min..max)
    }
}

impl<Context, T> Decode<Context> for RangeInclusive<T>
where
    T: Decode<Context>,
{
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        let min = T::decode(decoder)?;
        let max = T::decode(decoder)?;
        Ok(Self::new(min, max))
    }
}

impl<'de, T, Context> BorrowDecode<'de, Context> for RangeInclusive<T>
where
    T: BorrowDecode<'de, Context>,
{
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D
    ) -> Result<Self, DecodeError> {
        let min = T::borrow_decode(decoder)?;
        let max = T::borrow_decode(decoder)?;
        Ok(Self::new(min, max))
    }
}

impl<T, Context> Decode<Context> for Bound<T>
where
    T: Decode<Context>,
{
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        match u32::decode(decoder)? {
            | 0 => Ok(Self::Unbounded),
            | 1 => Ok(Self::Included(T::decode(decoder)?)),
            | 2 => Ok(Self::Excluded(T::decode(decoder)?)),
            | x => {
                crate::error::cold_decode_error_unexpected_variant(
                    core::any::type_name::<Self>(),
                    &crate::error::AllowedEnumVariants::Range { max: 2, min: 0 },
                    x,
                )
            },
        }
    }
}

impl<'de, T, Context> BorrowDecode<'de, Context> for Bound<T>
where
    T: BorrowDecode<'de, Context>,
{
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D
    ) -> Result<Self, DecodeError> {
        match u32::decode(decoder)? {
            | 0 => Ok(Self::Unbounded),
            | 1 => Ok(Self::Included(T::borrow_decode(decoder)?)),
            | 2 => Ok(Self::Excluded(T::borrow_decode(decoder)?)),
            | x => {
                crate::error::cold_decode_error_unexpected_variant(
                    core::any::type_name::<Self>(),
                    &crate::error::AllowedEnumVariants::Range { max: 2, min: 0 },
                    x,
                )
            },
        }
    }
}

const UTF8_CHAR_WIDTH: [u8; 256] = [
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, // 0x1F
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, // 0x3F
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, // 0x5F
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, // 0x7F
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, // 0x9F
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, // 0xBF
    0, 0, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
    2, // 0xDF
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, // 0xEF
    4, 4, 4, 4, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0xFF
];

// This function is a copy of core::str::utf8_char_width
const fn utf8_char_width(b: u8) -> usize {
    UTF8_CHAR_WIDTH[b as usize] as usize
}
