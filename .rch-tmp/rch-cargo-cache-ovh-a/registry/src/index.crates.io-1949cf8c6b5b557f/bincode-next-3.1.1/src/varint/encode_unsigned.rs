#![allow(clippy::cast_lossless, clippy::cast_possible_truncation)]
use super::SINGLE_BYTE_MAX;
use super::U16_BYTE;
use super::U32_BYTE;
use super::U64_BYTE;
use super::U128_BYTE;
use crate::config::Endianness;
use crate::enc::write::Writer;
use crate::error::EncodeError;

#[inline(always)]
pub fn varint_encode_u16<W: Writer>(
    writer: &mut W,
    endian: Endianness,
    val: u16,
) -> Result<(), EncodeError> {
    if crate::utils::is_likely!(val <= SINGLE_BYTE_MAX as _) {
        writer.write_u8(val as u8)
    } else {
        varint_encode_u16_cold(writer, endian, val)
    }
}

#[inline(never)]
#[cold]
fn varint_encode_u16_cold<W: Writer>(
    writer: &mut W,
    endian: Endianness,
    val: u16,
) -> Result<(), EncodeError> {
    let mut buf = [0u8; 3];
    buf[0] = U16_BYTE;
    let bytes = match endian {
        | Endianness::Big => val.to_be_bytes(),
        | Endianness::Little => val.to_le_bytes(),
    };
    buf[1..3].copy_from_slice(&bytes);
    writer.write(&buf)
}

#[inline(always)]
pub fn varint_encode_u32<W: Writer>(
    writer: &mut W,
    endian: Endianness,
    val: u32,
) -> Result<(), EncodeError> {
    if crate::utils::is_likely!(val <= SINGLE_BYTE_MAX as _) {
        writer.write_u8(val as u8)
    } else {
        varint_encode_u32_cold(writer, endian, val)
    }
}

#[inline(never)]
#[cold]
fn varint_encode_u32_cold<W: Writer>(
    writer: &mut W,
    endian: Endianness,
    val: u32,
) -> Result<(), EncodeError> {
    if crate::utils::is_likely!(val <= u16::MAX as _) {
        let mut buf = [0u8; 3];
        buf[0] = U16_BYTE;
        let bytes = match endian {
            | Endianness::Big => (val as u16).to_be_bytes(),
            | Endianness::Little => (val as u16).to_le_bytes(),
        };
        buf[1..3].copy_from_slice(&bytes);
        writer.write(&buf)
    } else {
        let mut buf = [0u8; 5];
        buf[0] = U32_BYTE;
        let bytes = match endian {
            | Endianness::Big => val.to_be_bytes(),
            | Endianness::Little => val.to_le_bytes(),
        };
        buf[1..5].copy_from_slice(&bytes);
        writer.write(&buf)
    }
}

#[inline(always)]
pub fn varint_encode_u64<W: Writer>(
    writer: &mut W,
    endian: Endianness,
    val: u64,
) -> Result<(), EncodeError> {
    if crate::utils::is_likely!(val <= SINGLE_BYTE_MAX as _) {
        writer.write_u8(val as u8)
    } else {
        varint_encode_u64_cold(writer, endian, val)
    }
}

#[inline(never)]
#[cold]
fn varint_encode_u64_cold<W: Writer>(
    writer: &mut W,
    endian: Endianness,
    val: u64,
) -> Result<(), EncodeError> {
    if crate::utils::is_likely!(val <= u16::MAX as _) {
        let mut buf = [0u8; 3];
        buf[0] = U16_BYTE;
        let bytes = match endian {
            | Endianness::Big => (val as u16).to_be_bytes(),
            | Endianness::Little => (val as u16).to_le_bytes(),
        };
        buf[1..3].copy_from_slice(&bytes);
        writer.write(&buf)
    } else if crate::utils::is_likely!(val <= u32::MAX as _) {
        let mut buf = [0u8; 5];
        buf[0] = U32_BYTE;
        let bytes = match endian {
            | Endianness::Big => (val as u32).to_be_bytes(),
            | Endianness::Little => (val as u32).to_le_bytes(),
        };
        buf[1..5].copy_from_slice(&bytes);
        writer.write(&buf)
    } else {
        let mut buf = [0u8; 9];
        buf[0] = U64_BYTE;
        let bytes = match endian {
            | Endianness::Big => val.to_be_bytes(),
            | Endianness::Little => val.to_le_bytes(),
        };
        buf[1..9].copy_from_slice(&bytes);
        writer.write(&buf)
    }
}

#[inline(always)]
pub fn varint_encode_u128<W: Writer>(
    writer: &mut W,
    endian: Endianness,
    val: u128,
) -> Result<(), EncodeError> {
    if crate::utils::is_likely!(val <= SINGLE_BYTE_MAX as _) {
        writer.write_u8(val as u8)
    } else {
        varint_encode_u128_cold(writer, endian, val)
    }
}

#[inline(never)]
#[cold]
fn varint_encode_u128_cold<W: Writer>(
    writer: &mut W,
    endian: Endianness,
    val: u128,
) -> Result<(), EncodeError> {
    if crate::utils::is_likely!(val <= u16::MAX as _) {
        let mut buf = [0u8; 3];
        buf[0] = U16_BYTE;
        let bytes = match endian {
            | Endianness::Big => (val as u16).to_be_bytes(),
            | Endianness::Little => (val as u16).to_le_bytes(),
        };
        buf[1..3].copy_from_slice(&bytes);
        writer.write(&buf)
    } else if crate::utils::is_likely!(val <= u32::MAX as _) {
        let mut buf = [0u8; 5];
        buf[0] = U32_BYTE;
        let bytes = match endian {
            | Endianness::Big => (val as u32).to_be_bytes(),
            | Endianness::Little => (val as u32).to_le_bytes(),
        };
        buf[1..5].copy_from_slice(&bytes);
        writer.write(&buf)
    } else if crate::utils::is_likely!(val <= u64::MAX as _) {
        let mut buf = [0u8; 9];
        buf[0] = U64_BYTE;
        let bytes = match endian {
            | Endianness::Big => (val as u64).to_be_bytes(),
            | Endianness::Little => (val as u64).to_le_bytes(),
        };
        buf[1..9].copy_from_slice(&bytes);
        writer.write(&buf)
    } else {
        let mut buf = [0u8; 17];
        buf[0] = U128_BYTE;
        let bytes = match endian {
            | Endianness::Big => val.to_be_bytes(),
            | Endianness::Little => val.to_le_bytes(),
        };
        buf[1..17].copy_from_slice(&bytes);
        writer.write(&buf)
    }
}

#[test]
fn test_encode_u16() {
    use crate::enc::write::SliceWriter;
    let mut buffer = [0u8; 20];

    // these should all encode to a single byte
    for i in 0u16..=SINGLE_BYTE_MAX as u16 {
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u16(&mut writer, Endianness::Big, i).unwrap();
        assert_eq!(writer.bytes_written(), 1);
        assert_eq!(buffer[0] as u16, i);

        // Assert endianness doesn't matter
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u16(&mut writer, Endianness::Little, i).unwrap();
        assert_eq!(writer.bytes_written(), 1);
        assert_eq!(buffer[0] as u16, i);
    }

    // these values should encode in 3 bytes (leading byte + 2 bytes)
    // Values chosen at random, add new cases as needed
    for i in [
        SINGLE_BYTE_MAX as u16 + 1,
        300,
        500,
        700,
        888,
        1234,
        u16::MAX,
    ] {
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u16(&mut writer, Endianness::Big, i).unwrap();
        assert_eq!(writer.bytes_written(), 3);
        assert_eq!(buffer[0], U16_BYTE);
        assert_eq!(&buffer[1..3], &i.to_be_bytes());

        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u16(&mut writer, Endianness::Little, i).unwrap();
        assert_eq!(writer.bytes_written(), 3);
        assert_eq!(buffer[0], U16_BYTE);
        assert_eq!(&buffer[1..3], &i.to_le_bytes());
    }
}

#[test]
fn test_encode_u32() {
    use crate::enc::write::SliceWriter;
    let mut buffer = [0u8; 20];

    // these should all encode to a single byte
    for i in 0u32..=SINGLE_BYTE_MAX as u32 {
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u32(&mut writer, Endianness::Big, i).unwrap();
        assert_eq!(writer.bytes_written(), 1);
        assert_eq!(buffer[0] as u32, i);

        // Assert endianness doesn't matter
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u32(&mut writer, Endianness::Little, i).unwrap();
        assert_eq!(writer.bytes_written(), 1);
        assert_eq!(buffer[0] as u32, i);
    }

    // these values should encode in 3 bytes (leading byte + 2 bytes)
    // Values chosen at random, add new cases as needed
    for i in [
        SINGLE_BYTE_MAX as u32 + 1,
        300,
        500,
        700,
        888,
        1234,
        u16::MAX as u32,
    ] {
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u32(&mut writer, Endianness::Big, i).unwrap();
        assert_eq!(writer.bytes_written(), 3);
        assert_eq!(buffer[0], U16_BYTE);
        assert_eq!(&buffer[1..3], &(i as u16).to_be_bytes());

        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u32(&mut writer, Endianness::Little, i).unwrap();
        assert_eq!(writer.bytes_written(), 3);
        assert_eq!(buffer[0], U16_BYTE);
        assert_eq!(&buffer[1..3], &(i as u16).to_le_bytes());
    }

    // these values should encode in 5 bytes (leading byte + 4 bytes)
    // Values chosen at random, add new cases as needed
    for i in [u16::MAX as u32 + 1, 100_000, 1_000_000, u32::MAX] {
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u32(&mut writer, Endianness::Big, i).unwrap();
        assert_eq!(writer.bytes_written(), 5);
        assert_eq!(buffer[0], U32_BYTE);
        assert_eq!(&buffer[1..5], &i.to_be_bytes());

        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u32(&mut writer, Endianness::Little, i).unwrap();
        assert_eq!(writer.bytes_written(), 5);
        assert_eq!(buffer[0], U32_BYTE);
        assert_eq!(&buffer[1..5], &i.to_le_bytes());
    }
}

#[test]
fn test_encode_u64() {
    use crate::enc::write::SliceWriter;
    let mut buffer = [0u8; 20];

    // these should all encode to a single byte
    for i in 0u64..=SINGLE_BYTE_MAX as u64 {
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u64(&mut writer, Endianness::Big, i).unwrap();
        assert_eq!(writer.bytes_written(), 1);
        assert_eq!(buffer[0] as u64, i);

        // Assert endianness doesn't matter
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u64(&mut writer, Endianness::Little, i).unwrap();
        assert_eq!(writer.bytes_written(), 1);
        assert_eq!(buffer[0] as u64, i);
    }

    // these values should encode in 3 bytes (leading byte + 2 bytes)
    // Values chosen at random, add new cases as needed
    for i in [
        SINGLE_BYTE_MAX as u64 + 1,
        300,
        500,
        700,
        888,
        1234,
        u16::MAX as u64,
    ] {
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u64(&mut writer, Endianness::Big, i).unwrap();
        assert_eq!(writer.bytes_written(), 3);
        assert_eq!(buffer[0], U16_BYTE);
        assert_eq!(&buffer[1..3], &(i as u16).to_be_bytes());

        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u64(&mut writer, Endianness::Little, i).unwrap();
        assert_eq!(writer.bytes_written(), 3);
        assert_eq!(buffer[0], U16_BYTE);
        assert_eq!(&buffer[1..3], &(i as u16).to_le_bytes());
    }

    // these values should encode in 5 bytes (leading byte + 4 bytes)
    // Values chosen at random, add new cases as needed
    for i in [u16::MAX as u64 + 1, 100_000, 1_000_000, u32::MAX as u64] {
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u64(&mut writer, Endianness::Big, i).unwrap();
        assert_eq!(writer.bytes_written(), 5);
        assert_eq!(buffer[0], U32_BYTE);
        assert_eq!(&buffer[1..5], &(i as u32).to_be_bytes());

        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u64(&mut writer, Endianness::Little, i).unwrap();
        assert_eq!(writer.bytes_written(), 5);
        assert_eq!(buffer[0], U32_BYTE);
        assert_eq!(&buffer[1..5], &(i as u32).to_le_bytes());
    }

    // these values should encode in 9 bytes (leading byte + 8 bytes)
    // Values chosen at random, add new cases as needed
    for i in [u32::MAX as u64 + 1, 5_000_000_000, u64::MAX] {
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u64(&mut writer, Endianness::Big, i).unwrap();
        assert_eq!(writer.bytes_written(), 9);
        assert_eq!(buffer[0], U64_BYTE);
        assert_eq!(&buffer[1..9], &i.to_be_bytes());

        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u64(&mut writer, Endianness::Little, i).unwrap();
        assert_eq!(writer.bytes_written(), 9);
        assert_eq!(buffer[0], U64_BYTE);
        assert_eq!(&buffer[1..9], &i.to_le_bytes());
    }
}

#[test]
fn test_encode_u128() {
    use crate::enc::write::SliceWriter;
    let mut buffer = [0u8; 20];

    // these should all encode to a single byte
    for i in 0u128..=SINGLE_BYTE_MAX as u128 {
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u128(&mut writer, Endianness::Big, i).unwrap();
        assert_eq!(writer.bytes_written(), 1);
        assert_eq!(buffer[0] as u128, i);

        // Assert endianness doesn't matter
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u128(&mut writer, Endianness::Little, i).unwrap();
        assert_eq!(writer.bytes_written(), 1);
        assert_eq!(buffer[0] as u128, i);
    }

    // these values should encode in 3 bytes (leading byte + 2 bytes)
    // Values chosen at random, add new cases as needed
    for i in [
        SINGLE_BYTE_MAX as u128 + 1,
        300,
        500,
        700,
        888,
        1234,
        u16::MAX as u128,
    ] {
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u128(&mut writer, Endianness::Big, i).unwrap();
        assert_eq!(writer.bytes_written(), 3);
        assert_eq!(buffer[0], U16_BYTE);
        assert_eq!(&buffer[1..3], &(i as u16).to_be_bytes());

        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u128(&mut writer, Endianness::Little, i).unwrap();
        assert_eq!(writer.bytes_written(), 3);
        assert_eq!(buffer[0], U16_BYTE);
        assert_eq!(&buffer[1..3], &(i as u16).to_le_bytes());
    }

    // these values should encode in 5 bytes (leading byte + 4 bytes)
    // Values chosen at random, add new cases as needed
    for i in [u16::MAX as u128 + 1, 100_000, 1_000_000, u32::MAX as u128] {
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u128(&mut writer, Endianness::Big, i).unwrap();
        assert_eq!(writer.bytes_written(), 5);
        assert_eq!(buffer[0], U32_BYTE);
        assert_eq!(&buffer[1..5], &(i as u32).to_be_bytes());

        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u128(&mut writer, Endianness::Little, i).unwrap();
        assert_eq!(writer.bytes_written(), 5);
        assert_eq!(buffer[0], U32_BYTE);
        assert_eq!(&buffer[1..5], &(i as u32).to_le_bytes());
    }

    // these values should encode in 9 bytes (leading byte + 8 bytes)
    // Values chosen at random, add new cases as needed
    for i in [u32::MAX as u128 + 1, 5_000_000_000, u64::MAX as u128] {
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u128(&mut writer, Endianness::Big, i).unwrap();
        assert_eq!(writer.bytes_written(), 9);
        assert_eq!(buffer[0], U64_BYTE);
        assert_eq!(&buffer[1..9], &(i as u64).to_be_bytes());

        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u128(&mut writer, Endianness::Little, i).unwrap();
        assert_eq!(writer.bytes_written(), 9);
        assert_eq!(buffer[0], U64_BYTE);
        assert_eq!(&buffer[1..9], &(i as u64).to_le_bytes());
    }

    // these values should encode in 17 bytes (leading byte + 16 bytes)
    // Values chosen at random, add new cases as needed
    for i in [u64::MAX as u128 + 1, u128::MAX] {
        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u128(&mut writer, Endianness::Big, i).unwrap();
        assert_eq!(writer.bytes_written(), 17);
        assert_eq!(buffer[0], U128_BYTE);
        assert_eq!(&buffer[1..17], &i.to_be_bytes());

        let mut writer = SliceWriter::new(&mut buffer);
        varint_encode_u128(&mut writer, Endianness::Little, i).unwrap();
        assert_eq!(writer.bytes_written(), 17);
        assert_eq!(buffer[0], U128_BYTE);
        assert_eq!(&buffer[1..17], &i.to_le_bytes());
    }
}
