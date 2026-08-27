//! CBOR (RFC 8949) encoding implementation.
#![allow(clippy::branches_sharing_code)]
#![allow(clippy::extra_unused_type_parameters)]
#![allow(clippy::redundant_pub_crate)]

use crate::config::CborDeterministicMode;
use crate::config::Config;
use crate::enc::write::Writer;
use crate::error::EncodeError;

/// Helper to encode a CBOR header (major type + value/argument).
///
/// # Errors
///
/// Returns `EncodeError` if the encoding fails.
#[inline(always)]
fn encode_header<W: Writer>(
    writer: &mut W,
    major: u8,
    val: u64,
) -> Result<(), EncodeError> {
    let major = major << 5;
    if crate::utils::is_likely!(val <= 23) {
        writer.write_u8(major | val as u8)
    } else if val <= 0xFF {
        writer.write_u8(major | 0x0018)?;
        writer.write_u8(val as u8)
    } else if val <= 0xFFFF {
        writer.write_u8(major | 0x0019)?;
        writer.write_u16((val as u16).to_be())
    } else if val <= 0xFFFF_FFFF {
        writer.write_u8(major | 0x001a)?;
        writer.write_u32((val as u32).to_be())
    } else {
        writer.write_u8(major | 0x001b)?;
        writer.write_u64(val.to_be())
    }
}

/// Helper to encode a CBOR indefinite length header.
///
/// # Errors
///
/// Returns `EncodeError` if the encoding fails.
#[inline(always)]
fn encode_indefinite_header<W: Writer>(
    writer: &mut W,
    major: u8,
) -> Result<(), EncodeError> {
    writer.write_u8((major << 5) | 31)
}

/// Encode a `u8` value into CBOR.
///
/// # Errors
///
/// Returns `EncodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn encode_u8<W: Writer, C: Config>(
    writer: &mut W,
    val: u8,
) -> Result<(), EncodeError> {
    encode_header(writer, 0, u64::from(val))
}

/// Encode a `u16` value into CBOR.
///
/// # Errors
///
/// Returns `EncodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn encode_u16<W: Writer, C: Config>(
    writer: &mut W,
    val: u16,
) -> Result<(), EncodeError> {
    encode_header(writer, 0, u64::from(val))
}

/// Encode a `u32` value into CBOR.
///
/// # Errors
///
/// Returns `EncodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn encode_u32<W: Writer, C: Config>(
    writer: &mut W,
    val: u32,
) -> Result<(), EncodeError> {
    encode_header(writer, 0, u64::from(val))
}

/// Encode a `u64` value into CBOR.
///
/// # Errors
///
/// Returns `EncodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn encode_u64<W: Writer, C: Config>(
    writer: &mut W,
    val: u64,
) -> Result<(), EncodeError> {
    encode_header(writer, 0, val)
}

/// Encode an `i8` value into CBOR.
///
/// # Errors
///
/// Returns `EncodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn encode_i8<W: Writer, C: Config>(
    writer: &mut W,
    val: i8,
) -> Result<(), EncodeError> {
    if val >= 0 {
        encode_header(writer, 0, val as u64)
    } else {
        encode_header(writer, 1, (-1 - val) as u64)
    }
}

/// Encode an `i16` value into CBOR.
///
/// # Errors
///
/// Returns `EncodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn encode_i16<W: Writer, C: Config>(
    writer: &mut W,
    val: i16,
) -> Result<(), EncodeError> {
    if val >= 0 {
        encode_header(writer, 0, val as u64)
    } else {
        encode_header(writer, 1, (-1 - val) as u64)
    }
}

/// Encode an `i32` value into CBOR.
///
/// # Errors
///
/// Returns `EncodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn encode_i32<W: Writer, C: Config>(
    writer: &mut W,
    val: i32,
) -> Result<(), EncodeError> {
    if val >= 0 {
        encode_header(writer, 0, val as u64)
    } else {
        encode_header(writer, 1, (-1 - val) as u64)
    }
}

/// Encode an `i64` value into CBOR.
///
/// # Errors
///
/// Returns `EncodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn encode_i64<W: Writer, C: Config>(
    writer: &mut W,
    val: i64,
) -> Result<(), EncodeError> {
    if val >= 0 {
        encode_header(writer, 0, val as u64)
    } else {
        encode_header(writer, 1, (-1 - val) as u64)
    }
}

/// Encode a `u128` value into CBOR.
///
/// # Errors
///
/// Returns `EncodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn encode_u128<W: Writer, C: Config>(
    writer: &mut W,
    val: u128,
) -> Result<(), EncodeError> {
    if val <= u128::from(u64::MAX) {
        return encode_header(writer, 0, val as u64);
    }
    // Tag 2 = positive bignum
    encode_header(writer, 6, 2)?;
    let bytes = val.to_be_bytes();
    let start = bytes.iter().position(|&b| b != 0).unwrap_or(15);
    encode_header(writer, 2, (16 - start) as u64)?;
    writer.write(&bytes[start..])
}

/// Encode an `i128` value into CBOR.
///
/// # Errors
///
/// Returns `EncodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn encode_i128<W: Writer, C: Config>(
    writer: &mut W,
    val: i128,
) -> Result<(), EncodeError> {
    if val >= 0 {
        return encode_u128::<W, C>(writer, val as u128);
    }
    let magnitude = (-1 - val) as u128;
    if magnitude <= u128::from(u64::MAX) {
        encode_header(writer, 1, magnitude as u64)
    } else {
        // Tag 3 = negative bignum
        encode_header(writer, 6, 3)?;
        let bytes = magnitude.to_be_bytes();
        let start = bytes.iter().position(|&b| b != 0).unwrap_or(15);
        encode_header(writer, 2, (16 - start) as u64)?;
        writer.write(&bytes[start..])
    }
}

/// Encode a `bool` value into CBOR.
///
/// # Errors
///
/// Returns `EncodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn encode_bool<W: Writer, C: Config>(
    writer: &mut W,
    val: bool,
) -> Result<(), EncodeError> {
    writer.write_u8(if val { 0xF5 } else { 0xF4 })
}

/// Losslessly convert f32 to f16 bit pattern.
#[inline(always)]
const fn f32_to_f16(val: f32) -> Option<u16> {
    let bits = val.to_bits();
    let sign = (bits >> 31) & 0x1;
    let exp = (bits >> 23) & 0xFF;
    let mat = bits & 0x7F_FFFF;

    if exp == 0xFF {
        if mat == 0 {
            return Some(((sign << 15) as u16) | 0x7C00); // Infinity
        } else if mat.trailing_zeros() >= 13 {
            return Some(((sign << 15) as u16) | 0x7C00 | (mat >> 13) as u16); // Precise NaN
        }
        return None;
    }

    if exp == 0 && mat == 0 {
        return Some((sign << 15) as u16); // Zero
    }

    let new_exp = (exp as i32) - 127 + 15;
    if new_exp > 30 {
        None
    } else if new_exp <= 0 {
        if new_exp < -10 {
            None
        } else {
            let mat = mat | 0x80_0000;
            let shift = 13 + (1 - new_exp);
            if mat & ((1 << shift) - 1) == 0 {
                Some(((sign << 15) as u16) | (mat >> shift) as u16)
            } else {
                None
            }
        }
    } else if mat.trailing_zeros() >= 13 {
        Some(((sign << 15) as u16) | ((new_exp as u16) << 10) | (mat >> 13) as u16)
    } else {
        None
    }
}

/// Losslessly convert f64 to f32.
#[inline(always)]
const fn f64_to_f32(val: f64) -> Option<f32> {
    let bits = val.to_bits();
    let sign = (bits >> 63) & 0x1;
    let exp = (bits >> 52) & 0x7FF;
    let mat = bits & 0xF_FFFF_FFFF_FFFF;

    if exp == 0x7FF {
        if mat == 0 {
            return Some(if sign == 0 {
                f32::INFINITY
            } else {
                f32::NEG_INFINITY
            });
        } else if mat.trailing_zeros() >= 37 {
            let f32_bits = ((sign << 31) as u32) | (0xFF << 23) | (mat >> 29) as u32;
            return Some(f32::from_bits(f32_bits));
        }
        return None;
    }

    let f = val as f32;
    #[allow(clippy::float_cmp)]
    if (f as f64) == val {
        Some(f)
    } else {
        None
    }
}

/// Encode an `f32` value into CBOR with preferred serialization.
///
/// # Errors
///
/// Returns `EncodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn encode_f32<W: Writer, C: Config>(
    writer: &mut W,
    mut val: f32,
) -> Result<(), EncodeError> {
    let opts = C::CBOR_OPTIONS;

    // Normalization
    if val == 0.0 && opts.normalize_neg_zero {
        val = 0.0;
    } else if val.is_nan() && opts.canonical_nan {
        return writer.write(&[0xf9, 0x7e, 0x00]); // Canonical NaN (f16)
    }

    if opts.preferred_float
        && let Some(f16_bits) = f32_to_f16(val)
    {
        writer.write_u8(0xF9)?;
        return writer.write(&f16_bits.to_be_bytes());
    }

    writer.write_u8(0xFA)?;
    writer.write(&val.to_be_bytes())
}

/// Encode an `f64` value into CBOR with preferred serialization.
///
/// # Errors
///
/// Returns `EncodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn encode_f64<W: Writer, C: Config>(
    writer: &mut W,
    mut val: f64,
) -> Result<(), EncodeError> {
    let opts = C::CBOR_OPTIONS;

    // Normalization
    if val == 0.0 && opts.normalize_neg_zero {
        val = 0.0;
    } else if val.is_nan() && opts.canonical_nan {
        return writer.write(&[0xf9, 0x7e, 0x00]); // Canonical NaN
    }

    if opts.preferred_float
        && let Some(f32_val) = f64_to_f32(val)
    {
        return encode_f32::<W, C>(writer, f32_val);
    }

    writer.write_u8(0xFB)?;
    writer.write(&val.to_be_bytes())
}

/// Encode a `str` value into CBOR.
///
/// # Errors
///
/// Returns `EncodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn encode_str<W: Writer, C: Config>(
    writer: &mut W,
    val: &str,
) -> Result<(), EncodeError> {
    encode_header(writer, 3, val.len() as u64)?;
    writer.write(val.as_bytes())
}

/// Encode a byte slice into CBOR.
///
/// # Errors
///
/// Returns `EncodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn encode_byte_slice<W: Writer, C: Config>(
    writer: &mut W,
    val: &[u8],
) -> Result<(), EncodeError> {
    encode_header(writer, 2, val.len() as u64)?;
    writer.write(val)
}

/// Encode a slice length into CBOR.
///
/// # Errors
///
/// Returns `EncodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn encode_slice_len<W: Writer, C: Config>(
    writer: &mut W,
    len: usize,
) -> Result<(), EncodeError> {
    if len == usize::MAX {
        encode_indefinite_header(writer, 4)
    } else {
        encode_header(writer, 4, len as u64)
    }
}

/// Encode a byte slice length into CBOR.
///
/// # Errors
///
/// Returns `EncodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn encode_byte_slice_len<W: Writer, C: Config>(
    writer: &mut W,
    len: u64,
) -> Result<(), EncodeError> {
    if len == u64::MAX {
        encode_indefinite_header(writer, 2)
    } else {
        encode_header(writer, 2, len)
    }
}

/// Encode a map length into CBOR.
///
/// # Errors
///
/// Returns `EncodeError` if the encoding fails.
#[inline(always)]
pub(crate) fn encode_map_len<W: Writer, C: Config>(
    writer: &mut W,
    len: usize,
) -> Result<(), EncodeError> {
    if len == usize::MAX {
        encode_indefinite_header(writer, 5)
    } else {
        encode_header(writer, 5, len as u64)
    }
}

/// Encode a map with deterministic ordering.
///
/// RFC 8949 §4.2.1: keys must be sorted by the bytewise lexicographic order of their deterministic encodings.
///
/// # Errors
///
/// Returns `EncodeError` if the encoding fails.
#[inline]
#[cfg(feature = "alloc")]
pub(crate) fn encode_map_deterministic<E, K, V, I>(
    encoder: &mut E,
    iter: I,
) -> Result<(), EncodeError>
where
    E: crate::enc::Encoder,
    K: crate::enc::Encode,
    V: crate::enc::Encode,
    I: IntoIterator<Item = (K, V)>,
    I::IntoIter: ExactSizeIterator,
{
    let iter = iter.into_iter();
    let len = iter.len();
    encode_map_len::<_, E::C>(encoder.writer(), len)?;

    let mut entries = crate::alloc::vec::Vec::with_capacity(len);
    for (k, v) in iter {
        let mut key_bytes = crate::alloc::vec::Vec::new();
        let mut key_encoder =
            crate::enc::EncoderImpl::<_, E::C>::new(&mut key_bytes, *encoder.config());
        k.encode(&mut key_encoder)?;
        entries.push((key_bytes, v));
    }

    entries.sort_by(|(ka, _), (kb, _)| {
        match <E::C as crate::config::InternalFormatConfig>::CBOR_OPTIONS.deterministic_mode {
            | CborDeterministicMode::LengthFirst => ka.len().cmp(&kb.len()).then(ka.cmp(kb)),
            | _ => ka.cmp(kb),
        }
    });

    for (k_bytes, v) in entries {
        encoder.writer().write(&k_bytes)?;
        v.encode(encoder)?;
    }

    Ok(())
}

/// Encode a slice with deterministic ordering.
///
/// RFC 8949: for sets (arrays with deterministic requirements), elements must be sorted.
///
/// # Errors
///
/// Returns `EncodeError` if the encoding fails.
#[inline]
#[cfg(feature = "alloc")]
pub(crate) fn encode_slice_deterministic<E, T, I>(
    encoder: &mut E,
    iter: I,
) -> Result<(), EncodeError>
where
    E: crate::enc::Encoder,
    T: crate::enc::Encode,
    I: IntoIterator<Item = T>,
    I::IntoIter: ExactSizeIterator,
{
    let iter = iter.into_iter();
    let len = iter.len();
    encode_slice_len::<_, E::C>(encoder.writer(), len)?;

    let mut entries = crate::alloc::vec::Vec::with_capacity(len);
    for item in iter {
        let mut bytes = crate::alloc::vec::Vec::new();
        let mut key_encoder =
            crate::enc::EncoderImpl::<_, E::C>::new(&mut bytes, *encoder.config());
        item.encode(&mut key_encoder)?;
        entries.push(bytes);
    }

    entries.sort_by(|a, b| {
        match <E::C as crate::config::InternalFormatConfig>::CBOR_OPTIONS.deterministic_mode {
            | CborDeterministicMode::LengthFirst => a.len().cmp(&b.len()).then(a.cmp(b)),
            | _ => a.cmp(b),
        }
    });

    for bytes in entries {
        encoder.writer().write(&bytes)?;
    }

    Ok(())
}
