#![allow(unsafe_code)]
#![allow(clippy::cast_ptr_alignment)]
use super::SINGLE_BYTE_MAX;
use super::U16_BYTE;
use super::U32_BYTE;
use super::U64_BYTE;
use super::U128_BYTE;
use crate::config::Endianness;
use crate::de::read::Reader;
use crate::error::DecodeError;
use crate::error::IntegerType;

#[inline(never)]
#[cold]
fn deserialize_varint_cold_u16<R>(
    read: &mut R,
    endian: Endianness,
) -> Result<u16, DecodeError>
where
    R: Reader,
{
    let mut bytes = [0u8; 1];
    read.read(&mut bytes)?;
    match bytes[0] {
        | byte @ 0..=SINGLE_BYTE_MAX => Ok(u16::from(byte)),
        | U16_BYTE => {
            let mut bytes = [0u8; 2];
            read.read(&mut bytes)?;
            Ok(match endian {
                | Endianness::Big => u16::from_be_bytes(bytes),
                | Endianness::Little => u16::from_le_bytes(bytes),
            })
        },
        | U32_BYTE => invalid_varint_discriminant(IntegerType::U16, IntegerType::U32),
        | U64_BYTE => invalid_varint_discriminant(IntegerType::U16, IntegerType::U64),
        | U128_BYTE => invalid_varint_discriminant(IntegerType::U16, IntegerType::U128),
        | _ => invalid_varint_discriminant(IntegerType::U16, IntegerType::Reserved),
    }
}

#[inline(never)]
#[cold]
fn deserialize_varint_cold_u32<R>(
    read: &mut R,
    endian: Endianness,
) -> Result<u32, DecodeError>
where
    R: Reader,
{
    let mut bytes = [0u8; 1];
    read.read(&mut bytes)?;
    match bytes[0] {
        | byte @ 0..=SINGLE_BYTE_MAX => Ok(u32::from(byte)),
        | U16_BYTE => {
            let mut bytes = [0u8; 2];
            read.read(&mut bytes)?;
            Ok(match endian {
                | Endianness::Big => u32::from(u16::from_be_bytes(bytes)),
                | Endianness::Little => u32::from(u16::from_le_bytes(bytes)),
            })
        },
        | U32_BYTE => {
            let mut bytes = [0u8; 4];
            read.read(&mut bytes)?;
            Ok(match endian {
                | Endianness::Big => u32::from_be_bytes(bytes),
                | Endianness::Little => u32::from_le_bytes(bytes),
            })
        },
        | U64_BYTE => invalid_varint_discriminant(IntegerType::U32, IntegerType::U64),
        | U128_BYTE => invalid_varint_discriminant(IntegerType::U32, IntegerType::U128),
        | _ => invalid_varint_discriminant(IntegerType::U32, IntegerType::Reserved),
    }
}

#[inline(never)]
#[cold]
fn deserialize_varint_cold_u64<R>(
    read: &mut R,
    endian: Endianness,
) -> Result<u64, DecodeError>
where
    R: Reader,
{
    let mut bytes = [0u8; 1];
    read.read(&mut bytes)?;
    match bytes[0] {
        | byte @ 0..=SINGLE_BYTE_MAX => Ok(u64::from(byte)),
        | U16_BYTE => {
            let mut bytes = [0u8; 2];
            read.read(&mut bytes)?;
            Ok(match endian {
                | Endianness::Big => u64::from(u16::from_be_bytes(bytes)),
                | Endianness::Little => u64::from(u16::from_le_bytes(bytes)),
            })
        },
        | U32_BYTE => {
            let mut bytes = [0u8; 4];
            read.read(&mut bytes)?;
            Ok(match endian {
                | Endianness::Big => u64::from(u32::from_be_bytes(bytes)),
                | Endianness::Little => u64::from(u32::from_le_bytes(bytes)),
            })
        },
        | U64_BYTE => {
            let mut bytes = [0u8; 8];
            read.read(&mut bytes)?;
            Ok(match endian {
                | Endianness::Big => u64::from_be_bytes(bytes),
                | Endianness::Little => u64::from_le_bytes(bytes),
            })
        },
        | U128_BYTE => invalid_varint_discriminant(IntegerType::U64, IntegerType::U128),
        | _ => invalid_varint_discriminant(IntegerType::U64, IntegerType::Reserved),
    }
}

#[inline(never)]
#[cold]
fn deserialize_varint_cold_usize<R>(
    read: &mut R,
    endian: Endianness,
) -> Result<usize, DecodeError>
where
    R: Reader,
{
    let mut bytes = [0u8; 1];
    read.read(&mut bytes)?;
    match bytes[0] {
        | byte @ 0..=SINGLE_BYTE_MAX => Ok(byte as usize),
        | U16_BYTE => {
            let mut bytes = [0u8; 2];
            read.read(&mut bytes)?;
            Ok(match endian {
                | Endianness::Big => u16::from_be_bytes(bytes) as usize,
                | Endianness::Little => u16::from_le_bytes(bytes) as usize,
            })
        },
        | U32_BYTE => {
            let mut bytes = [0u8; 4];
            read.read(&mut bytes)?;
            Ok(match endian {
                | Endianness::Big => u32::from_be_bytes(bytes) as usize,
                | Endianness::Little => u32::from_le_bytes(bytes) as usize,
            })
        },
        | U64_BYTE => {
            let mut bytes = [0u8; 8];
            read.read(&mut bytes)?;
            Ok(match endian {
                | Endianness::Big => {
                    usize::try_from(u64::from_be_bytes(bytes)).map_err(|_| {
                        crate::error::cold_decode_error_outside_usize_range::<()>(
                            u64::from_be_bytes(bytes),
                        )
                        .unwrap_err()
                    })?
                },
                | Endianness::Little => {
                    usize::try_from(u64::from_le_bytes(bytes)).map_err(|_| {
                        crate::error::cold_decode_error_outside_usize_range::<()>(
                            u64::from_le_bytes(bytes),
                        )
                        .unwrap_err()
                    })?
                },
            })
        },
        | U128_BYTE => invalid_varint_discriminant(IntegerType::Usize, IntegerType::U128),
        | _ => invalid_varint_discriminant(IntegerType::Usize, IntegerType::Reserved),
    }
}

#[inline(never)]
#[cold]
fn deserialize_varint_cold_u128<R>(
    read: &mut R,
    endian: Endianness,
) -> Result<u128, DecodeError>
where
    R: Reader,
{
    let mut bytes = [0u8; 1];
    read.read(&mut bytes)?;
    match bytes[0] {
        | byte @ 0..=SINGLE_BYTE_MAX => Ok(u128::from(byte)),
        | U16_BYTE => {
            let mut bytes = [0u8; 2];
            read.read(&mut bytes)?;
            Ok(match endian {
                | Endianness::Big => u128::from(u16::from_be_bytes(bytes)),
                | Endianness::Little => u128::from(u16::from_le_bytes(bytes)),
            })
        },
        | U32_BYTE => {
            let mut bytes = [0u8; 4];
            read.read(&mut bytes)?;
            Ok(match endian {
                | Endianness::Big => u128::from(u32::from_be_bytes(bytes)),
                | Endianness::Little => u128::from(u32::from_le_bytes(bytes)),
            })
        },
        | U64_BYTE => {
            let mut bytes = [0u8; 8];
            read.read(&mut bytes)?;
            Ok(match endian {
                | Endianness::Big => u128::from(u64::from_be_bytes(bytes)),
                | Endianness::Little => u128::from(u64::from_le_bytes(bytes)),
            })
        },
        | U128_BYTE => {
            let mut bytes = [0u8; 16];
            read.read(&mut bytes)?;
            Ok(match endian {
                | Endianness::Big => u128::from_be_bytes(bytes),
                | Endianness::Little => u128::from_le_bytes(bytes),
            })
        },
        | _ => invalid_varint_discriminant(IntegerType::U128, IntegerType::Reserved),
    }
}

#[cold]
#[inline(never)]
const fn invalid_varint_discriminant<T>(
    expected: IntegerType,
    found: IntegerType,
) -> Result<T, DecodeError> {
    crate::error::cold_decode_error_invalid_integer_type(expected, found)
}

#[inline(always)]
pub fn varint_decode_u16<R: Reader>(
    read: &mut R,
    endian: Endianness,
) -> Result<u16, DecodeError> {
    if let Some(bytes) = read.peek_read(3) {
        let b = unsafe { *bytes.as_ptr() };
        if crate::utils::is_likely!(b <= SINGLE_BYTE_MAX) {
            read.consume(1);
            return Ok(u16::from(b));
        }
        if crate::utils::is_unlikely!(b == U16_BYTE) {
            let v = unsafe {
                let ptr = bytes.as_ptr().add(1).cast::<u16>();
                let val = ptr.read_unaligned();
                match endian {
                    | Endianness::Little => u16::from_le(val),
                    | Endianness::Big => u16::from_be(val),
                }
            };
            read.consume(3);
            return Ok(v);
        }
    }
    deserialize_varint_cold_u16(read, endian)
}

#[inline(always)]
pub fn varint_decode_u32<R: Reader>(
    read: &mut R,
    endian: Endianness,
) -> Result<u32, DecodeError> {
    if let Some(bytes) = read.peek_read(5) {
        let b = unsafe { *bytes.as_ptr() };
        if crate::utils::is_likely!(b <= SINGLE_BYTE_MAX) {
            read.consume(1);
            return Ok(u32::from(b));
        }
        if crate::utils::is_unlikely!(b == U32_BYTE) {
            let v = unsafe {
                let ptr = bytes.as_ptr().add(1).cast::<u32>();
                let val = ptr.read_unaligned();
                match endian {
                    | Endianness::Little => u32::from_le(val),
                    | Endianness::Big => u32::from_be(val),
                }
            };
            read.consume(5);
            return Ok(v);
        }
    }
    deserialize_varint_cold_u32(read, endian)
}

#[inline(always)]
pub fn varint_decode_u64<R: Reader>(
    read: &mut R,
    endian: Endianness,
) -> Result<u64, DecodeError> {
    if let Some(bytes) = read.peek_read(9) {
        let b = unsafe { *bytes.as_ptr() };
        if crate::utils::is_likely!(b <= SINGLE_BYTE_MAX) {
            read.consume(1);
            return Ok(u64::from(b));
        }
        if crate::utils::is_unlikely!(b == U64_BYTE) {
            let v = unsafe {
                let ptr = bytes.as_ptr().add(1).cast::<u64>();
                let val = ptr.read_unaligned();
                match endian {
                    | Endianness::Little => u64::from_le(val),
                    | Endianness::Big => u64::from_be(val),
                }
            };
            read.consume(9);
            return Ok(v);
        }
    }
    deserialize_varint_cold_u64(read, endian)
}

#[inline(always)]
pub fn varint_decode_usize<R: Reader>(
    read: &mut R,
    endian: Endianness,
) -> Result<usize, DecodeError> {
    if let Some(bytes) = read.peek_read(9) {
        let b = unsafe { *bytes.as_ptr() };
        if crate::utils::is_likely!(b <= SINGLE_BYTE_MAX) {
            read.consume(1);
            return Ok(b as usize);
        }
        if crate::utils::is_unlikely!(b == U64_BYTE) {
            let v = unsafe {
                let ptr = bytes.as_ptr().add(1).cast::<u64>();
                let val = ptr.read_unaligned();
                match endian {
                    | Endianness::Little => u64::from_le(val),
                    | Endianness::Big => u64::from_be(val),
                }
            };
            let res = usize::try_from(v).map_err(|_| {
                crate::error::cold_decode_error_outside_usize_range::<()>(v).unwrap_err()
            })?;
            read.consume(9);
            return Ok(res);
        }
    }
    deserialize_varint_cold_usize(read, endian)
}

#[inline(always)]
pub fn varint_decode_u128<R: Reader>(
    read: &mut R,
    endian: Endianness,
) -> Result<u128, DecodeError> {
    if let Some(bytes) = read.peek_read(17) {
        let b = unsafe { *bytes.as_ptr() };
        if crate::utils::is_likely!(b <= SINGLE_BYTE_MAX) {
            read.consume(1);
            return Ok(u128::from(b));
        }
        if crate::utils::is_unlikely!(b == U128_BYTE) {
            let v = unsafe {
                let ptr = bytes.as_ptr().add(1).cast::<u128>();
                let val = ptr.read_unaligned();
                match endian {
                    | Endianness::Little => u128::from_le(val),
                    | Endianness::Big => u128::from_be(val),
                }
            };
            read.consume(17);
            return Ok(v);
        }
    }
    deserialize_varint_cold_u128(read, endian)
}

#[test]
fn test_decode_u16() {
    let cases: &[(&[u8], u16, u16)] = &[
        (&[0], 0, 0),
        (&[10], 10, 10),
        (&[U16_BYTE, 0, 10], 2560, 10),
    ];
    for &(slice, expected_le, expected_be) in cases {
        let mut reader = crate::de::read::SliceReader::new(slice);
        let found = varint_decode_u16(&mut reader, Endianness::Little).unwrap();
        assert_eq!(expected_le, found);

        let mut reader = crate::de::read::SliceReader::new(slice);
        let found = varint_decode_u16(&mut reader, Endianness::Big).unwrap();
        assert_eq!(expected_be, found);
    }

    let errors: &[(&[u8], DecodeError)] = &[
        (
            &[U32_BYTE],
            DecodeError::InvalidIntegerType {
                expected: IntegerType::U16,
                found: IntegerType::U32,
            },
        ),
        (
            &[U64_BYTE],
            DecodeError::InvalidIntegerType {
                expected: IntegerType::U16,
                found: IntegerType::U64,
            },
        ),
        (
            &[U128_BYTE],
            DecodeError::InvalidIntegerType {
                expected: IntegerType::U16,
                found: IntegerType::U128,
            },
        ),
        (&[U16_BYTE], DecodeError::UnexpectedEnd { additional: 2 }),
        (&[U16_BYTE, 0], DecodeError::UnexpectedEnd { additional: 1 }),
    ];

    for (slice, expected) in errors {
        let mut reader = crate::de::read::SliceReader::new(slice);
        let found = varint_decode_u16(&mut reader, Endianness::Little).unwrap_err();
        assert_eq!(std::format!("{expected:?}"), std::format!("{found:?}"));
    }
}

#[test]
fn test_decode_u32() {
    let cases: &[(&[u8], u32, u32)] = &[
        (&[0], 0, 0),
        (&[10], 10, 10),
        (&[U16_BYTE, 0, 10], 2560, 10),
        (&[U32_BYTE, 0, 0, 0, 10], 167_772_160, 10),
    ];
    for &(slice, expected_le, expected_be) in cases {
        let mut reader = crate::de::read::SliceReader::new(slice);
        let found = varint_decode_u32(&mut reader, Endianness::Little).unwrap();
        assert_eq!(expected_le, found);

        let mut reader = crate::de::read::SliceReader::new(slice);
        let found = varint_decode_u32(&mut reader, Endianness::Big).unwrap();
        assert_eq!(expected_be, found);
    }

    let errors: &[(&[u8], DecodeError)] = &[
        (
            &[U64_BYTE],
            DecodeError::InvalidIntegerType {
                expected: IntegerType::U32,
                found: IntegerType::U64,
            },
        ),
        (
            &[U128_BYTE],
            DecodeError::InvalidIntegerType {
                expected: IntegerType::U32,
                found: IntegerType::U128,
            },
        ),
        (&[U16_BYTE], DecodeError::UnexpectedEnd { additional: 2 }),
        (&[U16_BYTE, 0], DecodeError::UnexpectedEnd { additional: 1 }),
        (&[U32_BYTE], DecodeError::UnexpectedEnd { additional: 4 }),
        (&[U32_BYTE, 0], DecodeError::UnexpectedEnd { additional: 3 }),
        (
            &[U32_BYTE, 0, 0],
            DecodeError::UnexpectedEnd { additional: 2 },
        ),
        (
            &[U32_BYTE, 0, 0, 0],
            DecodeError::UnexpectedEnd { additional: 1 },
        ),
    ];

    for (slice, expected) in errors {
        let mut reader = crate::de::read::SliceReader::new(slice);
        let found = varint_decode_u32(&mut reader, Endianness::Little).unwrap_err();
        assert_eq!(std::format!("{expected:?}"), std::format!("{found:?}"));
    }
}

#[test]
fn test_decode_u64() {
    let cases: &[(&[u8], u64, u64)] = &[
        (&[0], 0, 0),
        (&[10], 10, 10),
        (&[U16_BYTE, 0, 10], 2560, 10),
        (&[U32_BYTE, 0, 0, 0, 10], 167_772_160, 10),
        (
            &[U64_BYTE, 0, 0, 0, 0, 0, 0, 0, 10],
            720_575_940_379_279_360,
            10,
        ),
    ];
    for &(slice, expected_le, expected_be) in cases {
        let mut reader = crate::de::read::SliceReader::new(slice);
        let found = varint_decode_u64(&mut reader, Endianness::Little).unwrap();
        assert_eq!(expected_le, found);

        let mut reader = crate::de::read::SliceReader::new(slice);
        let found = varint_decode_u64(&mut reader, Endianness::Big).unwrap();
        assert_eq!(expected_be, found);
    }

    let errors: &[(&[u8], DecodeError)] = &[
        (
            &[U128_BYTE],
            DecodeError::InvalidIntegerType {
                expected: IntegerType::U64,
                found: IntegerType::U128,
            },
        ),
        (&[U16_BYTE], DecodeError::UnexpectedEnd { additional: 2 }),
        (&[U16_BYTE, 0], DecodeError::UnexpectedEnd { additional: 1 }),
        (&[U32_BYTE], DecodeError::UnexpectedEnd { additional: 4 }),
        (&[U32_BYTE, 0], DecodeError::UnexpectedEnd { additional: 3 }),
        (
            &[U32_BYTE, 0, 0],
            DecodeError::UnexpectedEnd { additional: 2 },
        ),
        (
            &[U32_BYTE, 0, 0, 0],
            DecodeError::UnexpectedEnd { additional: 1 },
        ),
        (&[U64_BYTE], DecodeError::UnexpectedEnd { additional: 8 }),
        (&[U64_BYTE, 0], DecodeError::UnexpectedEnd { additional: 7 }),
        (
            &[U64_BYTE, 0, 0],
            DecodeError::UnexpectedEnd { additional: 6 },
        ),
        (
            &[U64_BYTE, 0, 0, 0],
            DecodeError::UnexpectedEnd { additional: 5 },
        ),
        (
            &[U64_BYTE, 0, 0, 0, 0],
            DecodeError::UnexpectedEnd { additional: 4 },
        ),
        (
            &[U64_BYTE, 0, 0, 0, 0, 0],
            DecodeError::UnexpectedEnd { additional: 3 },
        ),
        (
            &[U64_BYTE, 0, 0, 0, 0, 0, 0],
            DecodeError::UnexpectedEnd { additional: 2 },
        ),
        (
            &[U64_BYTE, 0, 0, 0, 0, 0, 0, 0],
            DecodeError::UnexpectedEnd { additional: 1 },
        ),
    ];

    for (slice, expected) in errors {
        let mut reader = crate::de::read::SliceReader::new(slice);
        let found = varint_decode_u64(&mut reader, Endianness::Little).unwrap_err();
        assert_eq!(std::format!("{expected:?}"), std::format!("{found:?}"));
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn test_decode_u128() {
    let cases: &[(&[u8], u128, u128)] = &[
        (&[0], 0, 0),
        (&[10], 10, 10),
        (&[U16_BYTE, 0, 10], 2560, 10),
        (&[U32_BYTE, 0, 0, 0, 10], 167_772_160, 10),
        (
            &[U64_BYTE, 0, 0, 0, 0, 0, 0, 0, 10],
            720_575_940_379_279_360,
            10,
        ),
        (
            &[U128_BYTE, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 10],
            13_292_279_957_849_158_729_038_070_602_803_445_760,
            10,
        ),
    ];
    for &(slice, expected_le, expected_be) in cases {
        let mut reader = crate::de::read::SliceReader::new(slice);
        let found = varint_decode_u128(&mut reader, Endianness::Little).unwrap();
        assert_eq!(expected_le, found);

        let mut reader = crate::de::read::SliceReader::new(slice);
        let found = varint_decode_u128(&mut reader, Endianness::Big).unwrap();
        assert_eq!(expected_be, found);
    }

    let errors: &[(&[u8], DecodeError)] = &[
        (&[U16_BYTE], DecodeError::UnexpectedEnd { additional: 2 }),
        (&[U16_BYTE, 0], DecodeError::UnexpectedEnd { additional: 1 }),
        (&[U32_BYTE], DecodeError::UnexpectedEnd { additional: 4 }),
        (&[U32_BYTE, 0], DecodeError::UnexpectedEnd { additional: 3 }),
        (
            &[U32_BYTE, 0, 0],
            DecodeError::UnexpectedEnd { additional: 2 },
        ),
        (
            &[U32_BYTE, 0, 0, 0],
            DecodeError::UnexpectedEnd { additional: 1 },
        ),
        (&[U64_BYTE], DecodeError::UnexpectedEnd { additional: 8 }),
        (&[U64_BYTE, 0], DecodeError::UnexpectedEnd { additional: 7 }),
        (
            &[U64_BYTE, 0, 0],
            DecodeError::UnexpectedEnd { additional: 6 },
        ),
        (
            &[U64_BYTE, 0, 0, 0],
            DecodeError::UnexpectedEnd { additional: 5 },
        ),
        (
            &[U64_BYTE, 0, 0, 0, 0],
            DecodeError::UnexpectedEnd { additional: 4 },
        ),
        (
            &[U64_BYTE, 0, 0, 0, 0, 0],
            DecodeError::UnexpectedEnd { additional: 3 },
        ),
        (
            &[U64_BYTE, 0, 0, 0, 0, 0, 0],
            DecodeError::UnexpectedEnd { additional: 2 },
        ),
        (
            &[U64_BYTE, 0, 0, 0, 0, 0, 0, 0],
            DecodeError::UnexpectedEnd { additional: 1 },
        ),
        (&[U128_BYTE], DecodeError::UnexpectedEnd { additional: 16 }),
        (
            &[U128_BYTE, 0],
            DecodeError::UnexpectedEnd { additional: 15 },
        ),
        (
            &[U128_BYTE, 0, 0],
            DecodeError::UnexpectedEnd { additional: 14 },
        ),
        (
            &[U128_BYTE, 0, 0, 0],
            DecodeError::UnexpectedEnd { additional: 13 },
        ),
        (
            &[U128_BYTE, 0, 0, 0, 0],
            DecodeError::UnexpectedEnd { additional: 12 },
        ),
        (
            &[U128_BYTE, 0, 0, 0, 0, 0],
            DecodeError::UnexpectedEnd { additional: 11 },
        ),
        (
            &[U128_BYTE, 0, 0, 0, 0, 0, 0],
            DecodeError::UnexpectedEnd { additional: 10 },
        ),
        (
            &[U128_BYTE, 0, 0, 0, 0, 0, 0, 0],
            DecodeError::UnexpectedEnd { additional: 9 },
        ),
        (
            &[U128_BYTE, 0, 0, 0, 0, 0, 0, 0, 0],
            DecodeError::UnexpectedEnd { additional: 8 },
        ),
        (
            &[U128_BYTE, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            DecodeError::UnexpectedEnd { additional: 7 },
        ),
        (
            &[U128_BYTE, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            DecodeError::UnexpectedEnd { additional: 6 },
        ),
        (
            &[U128_BYTE, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            DecodeError::UnexpectedEnd { additional: 5 },
        ),
        (
            &[U128_BYTE, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            DecodeError::UnexpectedEnd { additional: 4 },
        ),
        (
            &[U128_BYTE, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            DecodeError::UnexpectedEnd { additional: 3 },
        ),
        (
            &[U128_BYTE, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            DecodeError::UnexpectedEnd { additional: 2 },
        ),
        (
            &[U128_BYTE, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            DecodeError::UnexpectedEnd { additional: 1 },
        ),
    ];

    for (slice, expected) in errors {
        let mut reader = crate::de::read::SliceReader::new(slice);
        let found = varint_decode_u128(&mut reader, Endianness::Little).unwrap_err();
        std::dbg!(slice);
        assert_eq!(std::format!("{expected:?}"), std::format!("{found:?}"));
    }
}
