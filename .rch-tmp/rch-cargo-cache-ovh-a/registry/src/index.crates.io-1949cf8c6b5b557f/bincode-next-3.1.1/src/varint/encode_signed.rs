#![allow(clippy::cast_sign_loss)]
use super::varint_encode_u16;
use super::varint_encode_u32;
use super::varint_encode_u64;
use super::varint_encode_u128;
use crate::config::Endianness;

use crate::enc::write::Writer;
use crate::error::EncodeError;

#[cfg(test)]
use crate::enc::write::SliceWriter;

#[inline(always)]
pub fn varint_encode_i16<W: Writer>(
    writer: &mut W,
    endian: Endianness,
    val: i16,
) -> Result<(), EncodeError> {
    varint_encode_u16(writer, endian, ((val << 1) ^ (val >> 15)) as u16)
}

#[inline(always)]
pub fn varint_encode_i32<W: Writer>(
    writer: &mut W,
    endian: Endianness,
    val: i32,
) -> Result<(), EncodeError> {
    varint_encode_u32(writer, endian, ((val << 1) ^ (val >> 31)) as u32)
}

#[inline(always)]
pub fn varint_encode_i64<W: Writer>(
    writer: &mut W,
    endian: Endianness,
    val: i64,
) -> Result<(), EncodeError> {
    varint_encode_u64(writer, endian, ((val << 1) ^ (val >> 63)) as u64)
}

#[inline(always)]
pub fn varint_encode_i128<W: Writer>(
    writer: &mut W,
    endian: Endianness,
    val: i128,
) -> Result<(), EncodeError> {
    varint_encode_u128(writer, endian, ((val << 1) ^ (val >> 127)) as u128)
}

#[test]
fn test_encode_i16() {
    let cases: &[(i16, &[u8], &[u8])] = &[
        (0, &[0], &[0]),
        (2, &[4], &[4]),
        (256, &[super::U16_BYTE, 0, 2], &[super::U16_BYTE, 2, 0]),
        (
            16_000,
            &[super::U16_BYTE, 0, 125],
            &[super::U16_BYTE, 125, 0],
        ),
        (
            i16::MAX - 1,
            &[super::U16_BYTE, 252, 255],
            &[super::U16_BYTE, 255, 252],
        ),
        (
            i16::MAX,
            &[super::U16_BYTE, 254, 255],
            &[super::U16_BYTE, 255, 254],
        ),
    ];

    let mut buffer = [0u8; 20];
    for &(value, expected_le, expected_be) in cases {
        std::dbg!(value);

        // Little endian
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_i16(&mut writer, Endianness::Little, value).unwrap();

        assert_eq!(writer.bytes_written(), expected_le.len());
        assert_eq!(&buffer[..expected_le.len()], expected_le);

        // Big endian
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_i16(&mut writer, Endianness::Big, value).unwrap();

        assert_eq!(writer.bytes_written(), expected_be.len());
        assert_eq!(&buffer[..expected_be.len()], expected_be);
    }
}

#[test]
fn test_encode_i32() {
    let cases: &[(i32, &[u8], &[u8])] = &[
        (0, &[0], &[0]),
        (2, &[4], &[4]),
        (256, &[super::U16_BYTE, 0, 2], &[super::U16_BYTE, 2, 0]),
        (
            16_000,
            &[super::U16_BYTE, 0, 125],
            &[super::U16_BYTE, 125, 0],
        ),
        (
            40_000,
            &[super::U32_BYTE, 128, 56, 1, 0],
            &[super::U32_BYTE, 0, 1, 56, 128],
        ),
        (
            i32::MAX - 1,
            &[super::U32_BYTE, 252, 255, 255, 255],
            &[super::U32_BYTE, 255, 255, 255, 252],
        ),
        (
            i32::MAX,
            &[super::U32_BYTE, 254, 255, 255, 255],
            &[super::U32_BYTE, 255, 255, 255, 254],
        ),
    ];

    let mut buffer = [0u8; 20];
    for &(value, expected_le, expected_be) in cases {
        std::dbg!(value);

        // Little endian
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_i32(&mut writer, Endianness::Little, value).unwrap();

        assert_eq!(writer.bytes_written(), expected_le.len());
        assert_eq!(&buffer[..expected_le.len()], expected_le);

        // Big endian
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_i32(&mut writer, Endianness::Big, value).unwrap();

        assert_eq!(writer.bytes_written(), expected_be.len());
        assert_eq!(&buffer[..expected_be.len()], expected_be);
    }
}

#[test]
fn test_encode_i64() {
    let cases: &[(i64, &[u8], &[u8])] = &[
        (0, &[0], &[0]),
        (2, &[4], &[4]),
        (256, &[super::U16_BYTE, 0, 2], &[super::U16_BYTE, 2, 0]),
        (
            16_000,
            &[super::U16_BYTE, 0, 125],
            &[super::U16_BYTE, 125, 0],
        ),
        (
            40_000,
            &[super::U32_BYTE, 128, 56, 1, 0],
            &[super::U32_BYTE, 0, 1, 56, 128],
        ),
        (
            3_000_000_000,
            &[super::U64_BYTE, 0, 188, 160, 101, 1, 0, 0, 0],
            &[super::U64_BYTE, 0, 0, 0, 1, 101, 160, 188, 0],
        ),
        (
            i64::MAX - 1,
            &[super::U64_BYTE, 252, 255, 255, 255, 255, 255, 255, 255],
            &[super::U64_BYTE, 255, 255, 255, 255, 255, 255, 255, 252],
        ),
        (
            i64::MAX,
            &[super::U64_BYTE, 254, 255, 255, 255, 255, 255, 255, 255],
            &[super::U64_BYTE, 255, 255, 255, 255, 255, 255, 255, 254],
        ),
    ];

    let mut buffer = [0u8; 20];
    for &(value, expected_le, expected_be) in cases {
        std::dbg!(value);

        // Little endian
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_i64(&mut writer, Endianness::Little, value).unwrap();

        assert_eq!(writer.bytes_written(), expected_le.len());
        assert_eq!(&buffer[..expected_le.len()], expected_le);

        // Big endian
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_i64(&mut writer, Endianness::Big, value).unwrap();

        assert_eq!(writer.bytes_written(), expected_be.len());
        assert_eq!(&buffer[..expected_be.len()], expected_be);
    }
}

#[test]
fn test_encode_i128() {
    #[rustfmt::skip]
    let cases: &[(i128, &[u8], &[u8])] = &[
        (0, &[0], &[0]),
        (2, &[4], &[4]),
        (256, &[super::U16_BYTE, 0, 2], &[super::U16_BYTE, 2, 0]),
        (
            16_000,
            &[super::U16_BYTE, 0, 125],
            &[super::U16_BYTE, 125, 0],
        ),
        (
            40_000,
            &[super::U32_BYTE, 128, 56, 1, 0],
            &[super::U32_BYTE, 0, 1, 56, 128],
        ),
        (
            3_000_000_000,
            &[super::U64_BYTE, 0, 188, 160, 101, 1, 0, 0, 0],
            &[super::U64_BYTE, 0, 0, 0, 1, 101, 160, 188, 0],
        ),
        (
            11_000_000_000_000_000_000,
            &[
                super::U128_BYTE,
                0, 0, 152, 98, 112, 179, 79, 49,
                1, 0, 0, 0, 0, 0, 0, 0,
            ],
            &[
                super::U128_BYTE,
                0, 0, 0, 0, 0, 0, 0, 1,
                49, 79, 179, 112, 98, 152, 0, 0,
            ],
        ),
        (
            i128::MAX - 1,
            &[
                super::U128_BYTE,
                252, 255, 255, 255, 255, 255, 255, 255,
                255, 255, 255, 255, 255, 255, 255, 255,
            ],
            &[
                super::U128_BYTE,
                255, 255, 255, 255, 255, 255, 255, 255,
                255, 255, 255, 255, 255, 255, 255, 252,
            ],
        ),
        (
            i128::MAX,
            &[
                super::U128_BYTE,
                254, 255, 255, 255, 255, 255, 255, 255,
                255, 255, 255, 255, 255, 255, 255, 255,
            ],
            &[
                super::U128_BYTE,
                255, 255, 255, 255, 255, 255, 255, 255,
                255, 255, 255, 255, 255, 255, 255, 254,
            ],
        ),
    ];

    let mut buffer = [0u8; 20];
    for &(value, expected_le, expected_be) in cases {
        std::dbg!(value);

        // Little endian
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_i128(&mut writer, Endianness::Little, value).unwrap();

        assert_eq!(writer.bytes_written(), expected_le.len());
        assert_eq!(&buffer[..expected_le.len()], expected_le);

        // Big endian
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_i128(&mut writer, Endianness::Big, value).unwrap();

        assert_eq!(writer.bytes_written(), expected_be.len());
        assert_eq!(&buffer[..expected_be.len()], expected_be);
    }
}
