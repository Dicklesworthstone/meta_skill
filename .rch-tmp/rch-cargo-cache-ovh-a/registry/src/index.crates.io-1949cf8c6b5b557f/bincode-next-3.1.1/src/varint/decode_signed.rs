#![allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
use crate::config::Endianness;
use crate::de::read::Reader;
use crate::error::DecodeError;
use crate::error::IntegerType;

#[inline(always)]
pub fn varint_decode_i16<R: Reader>(
    read: &mut R,
    endian: Endianness,
) -> Result<i16, DecodeError> {
    let n = super::varint_decode_u16(read, endian)
        .map_err(DecodeError::change_integer_type_to_signed)?;
    Ok(if n % 2 == 0 {
        // positive number
        (n / 2) as _
    } else {
        // negative number
        // !m * 2 + 1 = n
        // !m * 2 = n - 1
        // !m = (n - 1) / 2
        // m = !((n - 1) / 2)
        // since we have n is odd, we have floor(n / 2) = floor((n - 1) / 2)
        !(n / 2) as _
    })
}

#[inline(always)]
pub fn varint_decode_i32<R: Reader>(
    read: &mut R,
    endian: Endianness,
) -> Result<i32, DecodeError> {
    let n = super::varint_decode_u32(read, endian)
        .map_err(DecodeError::change_integer_type_to_signed)?;
    Ok(if n % 2 == 0 {
        // positive number
        (n / 2) as _
    } else {
        // negative number
        // !m * 2 + 1 = n
        // !m * 2 = n - 1
        // !m = (n - 1) / 2
        // m = !((n - 1) / 2)
        // since we have n is odd, we have floor(n / 2) = floor((n - 1) / 2)
        !(n / 2) as _
    })
}

#[inline(always)]
pub fn varint_decode_i64<R: Reader>(
    read: &mut R,
    endian: Endianness,
) -> Result<i64, DecodeError> {
    let n = super::varint_decode_u64(read, endian)
        .map_err(DecodeError::change_integer_type_to_signed)?;
    Ok(if n % 2 == 0 {
        // positive number
        (n / 2) as _
    } else {
        // negative number
        // !m * 2 + 1 = n
        // !m * 2 = n - 1
        // !m = (n - 1) / 2
        // m = !((n - 1) / 2)
        // since we have n is odd, we have floor(n / 2) = floor((n - 1) / 2)
        !(n / 2) as _
    })
}

#[inline(always)]
pub fn varint_decode_i128<R: Reader>(
    read: &mut R,
    endian: Endianness,
) -> Result<i128, DecodeError> {
    let n = super::varint_decode_u128(read, endian)
        .map_err(DecodeError::change_integer_type_to_signed)?;
    Ok(if n % 2 == 0 {
        // positive number
        (n / 2) as _
    } else {
        // negative number
        // !m * 2 + 1 = n
        // !m * 2 = n - 1
        // !m = (n - 1) / 2
        // m = !((n - 1) / 2)
        // since we have n is odd, we have floor(n / 2) = floor((n - 1) / 2)
        !(n / 2) as _
    })
}

#[inline(always)]
pub fn varint_decode_isize<R: Reader>(
    read: &mut R,
    endian: Endianness,
) -> Result<isize, DecodeError> {
    match varint_decode_i64(read, endian) {
        | Ok(val) => {
            val.try_into().map_err(|_| {
                crate::error::cold_decode_error_outside_isize_range::<()>(val).unwrap_err()
            })
        },
        | Err(DecodeError::InvalidIntegerType { found, .. }) => {
            crate::error::cold_decode_error_invalid_integer_type(
                IntegerType::Isize,
                found.into_signed(),
            )
        },
        | Err(e) => Err(e),
    }
}
