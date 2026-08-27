//! CBOR (RFC 8949) decoding implementation.
#![allow(clippy::extra_unused_type_parameters)]
#![allow(clippy::redundant_pub_crate)]

use crate::de::read::Reader;
use crate::error::DecodeError;
use crate::error::cold_decode_error_invalid_boolean_value;
use crate::error::cold_decode_error_invalid_cbor_info;
use crate::error::cold_decode_error_outside_isize_range;
use crate::error::cold_decode_error_outside_usize_range;
use crate::error::cold_decode_error_unexpected_end;

/// Decodes a CBOR header (major type + value/argument + indefinite flag).
///
/// # Errors
///
/// Returns `DecodeError` if the encoding fails.
#[inline(always)]
#[allow(unsafe_code)]
fn decode_header<R: Reader>(reader: &mut R) -> Result<(u8, u64, bool), DecodeError> {
    let first = reader.read_u8()?;
    let major = first >> 5;
    let info = first & 0x1F;

    if crate::utils::is_likely!(info < 24) {
        Ok((major, u64::from(info), false))
    } else if crate::utils::is_likely!(info < 28) {
        if major == 7 {
            // Special case for Major Type 7: simple values and floats.
            // Don't consume bytes for Info 24-27 here, let the caller handle it.
            return Ok((major, u64::from(info), false));
        }
        let val = match info {
            | 24 => u64::from(reader.read_u8()?),
            | 25 => u64::from(u16::from_be(reader.read_u16()?)),
            | 26 => u64::from(u32::from_be(reader.read_u32()?)),
            | 27 => u64::from_be(reader.read_u64()?),
            | _ => unsafe { core::hint::unreachable_unchecked() },
        };
        Ok((major, val, false))
    } else if info == 31 {
        // Indefinite length (Major Types 2-5 and 7)
        if major == 0 || major == 1 || major == 6 {
            return cold_decode_error_invalid_cbor_info(info);
        }
        Ok((major, 0, true))
    } else {
        cold_decode_error_invalid_cbor_info(info)
    }
}

/// Decode a `u8` value from CBOR.
///
/// # Errors
///
/// Returns `DecodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn decode_u8<R: Reader>(reader: &mut R) -> Result<u8, DecodeError> {
    let (major, val, indefinite) = decode_header(reader)?;
    if major != 0 || indefinite {
        return cold_decode_error_unexpected_end(0);
    }
    val.try_into()
        .map_err(|_| cold_decode_error_outside_usize_range::<u8>(val).unwrap_err())
}

/// Decode a `u16` value from CBOR.
///
/// # Errors
///
/// Returns `DecodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn decode_u16<R: Reader>(reader: &mut R) -> Result<u16, DecodeError> {
    let (major, val, indefinite) = decode_header(reader)?;
    if major != 0 || indefinite {
        return cold_decode_error_unexpected_end(0);
    }
    val.try_into()
        .map_err(|_| cold_decode_error_outside_usize_range::<u16>(val).unwrap_err())
}

/// Decode a `u32` value from CBOR.
///
/// # Errors
///
/// Returns `DecodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn decode_u32<R: Reader>(reader: &mut R) -> Result<u32, DecodeError> {
    let (major, val, indefinite) = decode_header(reader)?;
    if major != 0 || indefinite {
        return cold_decode_error_unexpected_end(0);
    }
    val.try_into()
        .map_err(|_| cold_decode_error_outside_usize_range::<u32>(val).unwrap_err())
}

/// Decode a `u64` value from CBOR.
///
/// # Errors
///
/// Returns `DecodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn decode_u64<R: Reader>(reader: &mut R) -> Result<u64, DecodeError> {
    let (major, val, indefinite) = decode_header(reader)?;
    if major != 0 || indefinite {
        return cold_decode_error_unexpected_end(0);
    }
    Ok(val)
}

/// Decode a `u128` value from CBOR.
///
/// # Errors
///
/// Returns `DecodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn decode_u128<R: Reader>(reader: &mut R) -> Result<u128, DecodeError> {
    let (major, val, indefinite) = decode_header(reader)?;
    match major {
        | 0 if !indefinite => Ok(u128::from(val)),
        | 6 if !indefinite => {
            if val != 2 {
                return cold_decode_error_unexpected_end(0);
            }
            decode_bignum_bytes(reader)
        },
        | _ => cold_decode_error_unexpected_end(0),
    }
}

/// Decode an i8 value from CBOR.
///
/// # Errors
///
/// Returns `DecodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn decode_i8<R: Reader>(reader: &mut R) -> Result<i8, DecodeError> {
    let (major, val, indefinite) = decode_header(reader)?;
    if indefinite {
        return cold_decode_error_unexpected_end(0);
    }
    match major {
        | 0 => {
            val.try_into()
                .map_err(|_| cold_decode_error_outside_isize_range::<i8>(val as i64).unwrap_err())
        },
        | 1 => {
            let res = -1 - (val as i64);
            res.try_into()
                .map_err(|_| cold_decode_error_outside_isize_range::<i8>(res).unwrap_err())
        },
        | _ => cold_decode_error_unexpected_end(0),
    }
}

/// Decode an i16 value from CBOR.
///
/// # Errors
///
/// Returns `DecodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn decode_i16<R: Reader>(reader: &mut R) -> Result<i16, DecodeError> {
    let (major, val, indefinite) = decode_header(reader)?;
    if indefinite {
        return cold_decode_error_unexpected_end(0);
    }
    match major {
        | 0 => {
            val.try_into()
                .map_err(|_| cold_decode_error_outside_isize_range::<i16>(val as i64).unwrap_err())
        },
        | 1 => {
            let res = -1 - (val as i64);
            res.try_into()
                .map_err(|_| cold_decode_error_outside_isize_range::<i16>(res).unwrap_err())
        },
        | _ => cold_decode_error_unexpected_end(0),
    }
}

/// Decode an i32 value from CBOR.
///
/// # Errors
///
/// Returns `DecodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn decode_i32<R: Reader>(reader: &mut R) -> Result<i32, DecodeError> {
    let (major, val, indefinite) = decode_header(reader)?;
    if indefinite {
        return cold_decode_error_unexpected_end(0);
    }
    match major {
        | 0 => {
            val.try_into()
                .map_err(|_| cold_decode_error_outside_isize_range::<i32>(val as i64).unwrap_err())
        },
        | 1 => {
            let res = -1 - (val as i64);
            res.try_into()
                .map_err(|_| cold_decode_error_outside_isize_range::<i32>(res).unwrap_err())
        },
        | _ => cold_decode_error_unexpected_end(0),
    }
}

/// Decode an i64 value from CBOR.
///
/// # Errors
///
/// Returns `DecodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn decode_i64<R: Reader>(reader: &mut R) -> Result<i64, DecodeError> {
    let (major, val, indefinite) = decode_header(reader)?;
    if indefinite {
        return cold_decode_error_unexpected_end(0);
    }
    match major {
        | 0 => {
            val.try_into()
                .map_err(|_| cold_decode_error_outside_isize_range::<i64>(val as i64).unwrap_err())
        },
        | 1 => {
            if val > (i64::MIN.unsigned_abs() - 1) {
                return cold_decode_error_outside_isize_range(-1);
            }
            Ok(-1 - (val as i64))
        },
        | _ => cold_decode_error_unexpected_end(0),
    }
}

/// Decode an i128 value from CBOR.
///
/// # Errors
///
/// Returns `DecodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn decode_i128<R: Reader>(reader: &mut R) -> Result<i128, DecodeError> {
    let (major, val, indefinite) = decode_header(reader)?;
    if indefinite {
        return cold_decode_error_unexpected_end(0);
    }
    match major {
        | 0 => Ok(i128::from(val)),
        | 1 => Ok(-1 - i128::from(val)),
        | 6 => {
            match val {
                | 2 => Ok(decode_bignum_bytes(reader)? as i128),
                | 3 => {
                    let magnitude = decode_bignum_bytes(reader)?;
                    if magnitude > (i128::MAX as u128) + 1 {
                        return cold_decode_error_outside_isize_range(-1);
                    }
                    if magnitude == (i128::MAX as u128) + 1 {
                        return Ok(i128::MIN);
                    }
                    Ok(-1 - (magnitude as i128))
                },
                | _ => cold_decode_error_unexpected_end(0),
            }
        },
        | _ => cold_decode_error_unexpected_end(0),
    }
}

/// Decode a CBOR byte string into a u128 (big-endian bignum).
///
/// # Errors
///
/// Returns `DecodeError` if the encoding fails.
#[inline(always)]
fn decode_bignum_bytes<R: Reader>(reader: &mut R) -> Result<u128, DecodeError> {
    let (major, len, indefinite) = decode_header(reader)?;
    if major != 2 || indefinite {
        return cold_decode_error_unexpected_end(0);
    }
    if len > 16 {
        return cold_decode_error_outside_usize_range::<u128>(len);
    }
    let len = len as usize;
    let mut buf = [0u8; 16];
    reader.read(&mut buf[16 - len..])?;
    Ok(u128::from_be_bytes(buf))
}

/// Decode a `bool` value from CBOR.
///
/// # Errors
///
/// Returns `DecodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn decode_bool<R: Reader>(reader: &mut R) -> Result<bool, DecodeError> {
    let (major, val, indefinite) = decode_header(reader)?;
    if major != 7 || indefinite {
        return cold_decode_error_invalid_boolean_value(0);
    }
    match val {
        | 20 => Ok(false),
        | 21 => Ok(true),
        | _ => cold_decode_error_invalid_boolean_value(val as u8),
    }
}

/// Convert f16 bits to f32.
#[inline(always)]
fn f16_to_f32(bits: u16) -> f32 {
    let sign = u32::from(bits & 0x8000);
    let exp = u32::from((bits & 0x7C00) >> 10);
    let mat = u32::from(bits & 0x03FF);

    if exp == 0x1F {
        f32::from_bits((sign << 16) | (0xFF << 23) | (mat << 13))
    } else if exp == 0 {
        if mat == 0 {
            f32::from_bits(sign << 16)
        } else {
            // Subnormal f16 to normal f32
            let mut m = mat;
            let mut e = 0i32;
            while (m & 0x400) == 0 {
                m <<= 1;
                e -= 1;
            }
            e += 1;
            m &= !0x400;
            let f32_exp = (127 - 15 + e) as u32;
            f32::from_bits((sign << 16) | (f32_exp << 23) | (m << 13))
        }
    } else {
        f32::from_bits((sign << 16) | ((exp + 127 - 15) << 23) | (mat << 13))
    }
}

/// Decode an `f32` value from CBOR. Supports f16, f32, f64 per RFC preference.
///
/// # Errors
///
/// Returns `DecodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn decode_f32<R: Reader>(reader: &mut R) -> Result<f32, DecodeError> {
    let (major, info, indefinite) = decode_header(reader)?;
    if major != 7 || indefinite {
        return cold_decode_error_unexpected_end(4);
    }
    match info {
        | 25 => Ok(f16_to_f32(u16::from_be(reader.read_u16()?))),
        | 26 => Ok(f32::from_bits(u32::from_be(reader.read_u32()?))),
        | 27 => Ok(f64::from_bits(u64::from_be(reader.read_u64()?)) as f32),
        | _ => {
            // Simple values < 24? RFC 8949 says 20: false, 21: true, 22: null, 23: undefined.
            // These aren't floats.
            cold_decode_error_unexpected_end(4)
        },
    }
}

/// Decode an `f64` value from CBOR. Supports f16, f32, f64.
///
/// # Errors
///
/// Returns `DecodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn decode_f64<R: Reader>(reader: &mut R) -> Result<f64, DecodeError> {
    let (major, info, indefinite) = decode_header(reader)?;
    if major != 7 || indefinite {
        return cold_decode_error_unexpected_end(8);
    }
    match info {
        | 25 => Ok(f64::from(f16_to_f32(u16::from_be(reader.read_u16()?)))),
        | 26 => Ok(f64::from(f32::from_bits(u32::from_be(reader.read_u32()?)))),
        | 27 => Ok(f64::from_bits(u64::from_be(reader.read_u64()?))),
        | _ => cold_decode_error_unexpected_end(8),
    }
}

/// Decode a slice length from CBOR.
///
/// # Errors
///
/// Returns `DecodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn decode_slice_len<R: Reader>(reader: &mut R) -> Result<usize, DecodeError> {
    let (major, val, indefinite) = decode_header(reader)?;
    if major != 4 {
        return cold_decode_error_unexpected_end(0);
    }
    if indefinite {
        // Special value to indicate indefinite length (Major Type 4)
        return Ok(usize::MAX);
    }
    val.try_into()
        .map_err(|_| cold_decode_error_outside_usize_range::<usize>(val).unwrap_err())
}

/// Decode a map length from CBOR.
///
/// # Errors
///
/// Returns `DecodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn decode_map_len<R: Reader>(reader: &mut R) -> Result<usize, DecodeError> {
    let (major, val, indefinite) = decode_header(reader)?;
    if major != 5 {
        return cold_decode_error_unexpected_end(0);
    }
    if indefinite {
        return Ok(usize::MAX);
    }
    val.try_into()
        .map_err(|_| cold_decode_error_outside_usize_range::<usize>(val).unwrap_err())
}

/// Decode a byte slice length from CBOR (Major Type 2).
///
/// # Errors
///
/// Returns `DecodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn decode_byte_slice_len<R: Reader>(reader: &mut R) -> Result<usize, DecodeError> {
    let (major, val, indefinite) = decode_header(reader)?;
    if major != 2 {
        return cold_decode_error_unexpected_end(0);
    }
    if indefinite {
        return Ok(usize::MAX);
    }
    val.try_into()
        .map_err(|_| cold_decode_error_outside_usize_range::<usize>(val).unwrap_err())
}

/// Decode a length that can be either a byte slice (Major Type 2) or an array (Major Type 4).
///
/// # Errors
///
/// Returns `DecodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn decode_byte_slice_or_array_len<R: Reader>(
    reader: &mut R
) -> Result<(u8, usize), DecodeError> {
    let (major, val, indefinite) = decode_header(reader)?;
    if major != 2 && major != 4 {
        return cold_decode_error_unexpected_end(0);
    }
    let len = if indefinite {
        usize::MAX
    } else {
        val.try_into()
            .map_err(|_| cold_decode_error_outside_usize_range::<usize>(val).unwrap_err())?
    };
    Ok((major, len))
}

/// Decode a string length from CBOR (Major Type 3).
///
/// # Errors
///
/// Returns `DecodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn decode_str_len<R: Reader>(reader: &mut R) -> Result<usize, DecodeError> {
    let (major, val, indefinite) = decode_header(reader)?;
    if major != 3 {
        return cold_decode_error_unexpected_end(0);
    }
    if indefinite {
        return Ok(usize::MAX);
    }
    val.try_into()
        .map_err(|_| cold_decode_error_outside_usize_range::<usize>(val).unwrap_err())
}
