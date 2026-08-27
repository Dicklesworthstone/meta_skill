//! This module contains writer-based structs and traits.
//!
//! Because `std::io::Write` is only limited to `std` and not `core`, we provide our own [Writer].
#![allow(unsafe_code)]

use crate::error::EncodeError;

#[cfg(feature = "alloc")]
impl Writer for crate::alloc::vec::Vec<u8> {
    #[inline(always)]
    fn write(
        &mut self,
        bytes: &[u8],
    ) -> Result<(), EncodeError> {
        self.extend_from_slice(bytes);
        Ok(())
    }

    #[inline(always)]
    fn write_u8(
        &mut self,
        value: u8,
    ) -> Result<(), EncodeError> {
        self.push(value);
        Ok(())
    }
}

/// Trait that indicates that a struct can be used as a destination to encode data too. This is used by [Encode]
///
/// [Encode]: ../trait.Encode.html
pub trait Writer {
    /// Write `bytes` to the underlying writer. Exactly `bytes.len()` bytes must be written, or else an error should be returned.
    ///
    /// # Errors
    ///
    /// Returns `EncodeError::UnexpectedEnd` if the writer does not have enough space.
    fn write(
        &mut self,
        bytes: &[u8],
    ) -> Result<(), EncodeError>;

    /// Write a single byte to the underlying writer.
    ///
    /// # Errors
    ///
    /// Returns `EncodeError::UnexpectedEnd` if the writer does not have enough space.
    #[inline(always)]
    fn write_u8(
        &mut self,
        value: u8,
    ) -> Result<(), EncodeError> {
        self.write(&[value])
    }

    /// Write a `u16` to the underlying writer.
    ///
    /// # Errors
    ///
    /// Returns `EncodeError::UnexpectedEnd` if the writer does not have enough space.
    #[inline(always)]
    fn write_u16(
        &mut self,
        value: u16,
    ) -> Result<(), EncodeError> {
        self.write(&value.to_ne_bytes())
    }

    /// Write a `u32` to the underlying writer.
    ///
    /// # Errors
    ///
    /// Returns `EncodeError::UnexpectedEnd` if the writer does not have enough space.
    #[inline(always)]
    fn write_u32(
        &mut self,
        value: u32,
    ) -> Result<(), EncodeError> {
        self.write(&value.to_ne_bytes())
    }

    /// Write a `u64` to the underlying writer.
    ///
    /// # Errors
    ///
    /// Returns `EncodeError::UnexpectedEnd` if the writer does not have enough space.
    #[inline(always)]
    fn write_u64(
        &mut self,
        value: u64,
    ) -> Result<(), EncodeError> {
        self.write(&value.to_ne_bytes())
    }

    /// Write a `u128` to the underlying writer.
    ///
    /// # Errors
    ///
    /// Returns `EncodeError::UnexpectedEnd` if the writer does not have enough space.
    #[inline(always)]
    fn write_u128(
        &mut self,
        value: u128,
    ) -> Result<(), EncodeError> {
        self.write(&value.to_ne_bytes())
    }
}

impl<T: Writer> Writer for &mut T {
    #[inline(always)]
    fn write(
        &mut self,
        bytes: &[u8],
    ) -> Result<(), EncodeError> {
        (**self).write(bytes)
    }

    #[inline(always)]
    fn write_u8(
        &mut self,
        value: u8,
    ) -> Result<(), EncodeError> {
        (**self).write_u8(value)
    }

    #[inline(always)]
    fn write_u16(
        &mut self,
        value: u16,
    ) -> Result<(), EncodeError> {
        (**self).write_u16(value)
    }

    #[inline(always)]
    fn write_u32(
        &mut self,
        value: u32,
    ) -> Result<(), EncodeError> {
        (**self).write_u32(value)
    }

    #[inline(always)]
    fn write_u64(
        &mut self,
        value: u64,
    ) -> Result<(), EncodeError> {
        (**self).write_u64(value)
    }

    #[inline(always)]
    fn write_u128(
        &mut self,
        value: u128,
    ) -> Result<(), EncodeError> {
        (**self).write_u128(value)
    }
}

/// A helper struct that implements `Writer` for a `&[u8]` slice.
///
/// ```
/// use bincode_next::enc::write::SliceWriter;
/// use bincode_next::enc::write::Writer;
///
/// let destination = &mut [0u8; 100];
/// let mut writer = SliceWriter::new(destination);
/// writer.write(&[1, 2, 3, 4, 5]).unwrap();
///
/// assert_eq!(writer.bytes_written(), 5);
/// assert_eq!(destination[0..6], [1, 2, 3, 4, 5, 0]);
/// ```
pub struct SliceWriter<'storage> {
    slice: &'storage mut [u8],
    original_length: usize,
}

impl<'storage> SliceWriter<'storage> {
    /// Create a new instance of `SliceWriter` with the given byte array.
    pub const fn new(bytes: &'storage mut [u8]) -> Self {
        let original = bytes.len();
        Self {
            slice: bytes,
            original_length: original,
        }
    }

    /// Return the amount of bytes written so far.
    #[must_use]
    pub const fn bytes_written(&self) -> usize {
        self.original_length - self.slice.len()
    }
}

impl Writer for SliceWriter<'_> {
    #[inline(always)]
    fn write(
        &mut self,
        bytes: &[u8],
    ) -> Result<(), EncodeError> {
        if bytes.len() > self.slice.len() {
            return crate::error::cold_encode_error_unexpected_end();
        }
        // SAFETY: `bytes.len() <= self.slice.len()` is checked above.
        // `copy_nonoverlapping` is safe since the pointers won't overlap and bounds are exact.
        // Advancing the pointer by `bytes.len()` is safe for the same reason.
        unsafe {
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), self.slice.as_mut_ptr(), bytes.len());
            let ptr = self.slice.as_mut_ptr().add(bytes.len());
            let len = self.slice.len() - bytes.len();
            self.slice = core::slice::from_raw_parts_mut(ptr, len);
        }

        Ok(())
    }

    #[inline(always)]
    fn write_u8(
        &mut self,
        value: u8,
    ) -> Result<(), EncodeError> {
        if self.slice.is_empty() {
            return crate::error::cold_encode_error_unexpected_end();
        }
        // SAFETY: `!self.slice.is_empty()` is checked above.
        unsafe {
            *self.slice.as_mut_ptr() = value;
            let ptr = self.slice.as_mut_ptr().add(1);
            let len = self.slice.len() - 1;
            self.slice = core::slice::from_raw_parts_mut(ptr, len);
        }

        Ok(())
    }

    #[inline(always)]
    fn write_u16(
        &mut self,
        value: u16,
    ) -> Result<(), EncodeError> {
        if self.slice.len() < 2 {
            return crate::error::cold_encode_error_unexpected_end();
        }
        // SAFETY: `self.slice.len() < 2` is checked above.
        unsafe {
            core::ptr::write_unaligned(self.slice.as_mut_ptr().cast::<u16>(), value);
            let ptr = self.slice.as_mut_ptr().add(2);
            let len = self.slice.len() - 2;
            self.slice = core::slice::from_raw_parts_mut(ptr, len);
        }

        Ok(())
    }

    #[inline(always)]
    fn write_u32(
        &mut self,
        value: u32,
    ) -> Result<(), EncodeError> {
        if self.slice.len() < 4 {
            return crate::error::cold_encode_error_unexpected_end();
        }
        // SAFETY: `self.slice.len() < 4` is checked above.
        unsafe {
            core::ptr::write_unaligned(self.slice.as_mut_ptr().cast::<u32>(), value);
            let ptr = self.slice.as_mut_ptr().add(4);
            let len = self.slice.len() - 4;
            self.slice = core::slice::from_raw_parts_mut(ptr, len);
        }

        Ok(())
    }

    #[inline(always)]
    fn write_u64(
        &mut self,
        value: u64,
    ) -> Result<(), EncodeError> {
        if self.slice.len() < 8 {
            return crate::error::cold_encode_error_unexpected_end();
        }
        // SAFETY: `self.slice.len() < 8` is checked above.
        unsafe {
            core::ptr::write_unaligned(self.slice.as_mut_ptr().cast::<u64>(), value);
            let ptr = self.slice.as_mut_ptr().add(8);
            let len = self.slice.len() - 8;
            self.slice = core::slice::from_raw_parts_mut(ptr, len);
        }

        Ok(())
    }

    #[inline(always)]
    fn write_u128(
        &mut self,
        value: u128,
    ) -> Result<(), EncodeError> {
        if self.slice.len() < 16 {
            return crate::error::cold_encode_error_unexpected_end();
        }
        // SAFETY: `self.slice.len() < 16` is checked above.
        unsafe {
            core::ptr::write_unaligned(self.slice.as_mut_ptr().cast::<u128>(), value);
            let ptr = self.slice.as_mut_ptr().add(16);
            let len = self.slice.len() - 16;
            self.slice = core::slice::from_raw_parts_mut(ptr, len);
        }

        Ok(())
    }
}

/// A writer that counts how many bytes were written. This is useful for e.g. pre-allocating buffers before writing to them.
#[derive(Default)]
pub struct SizeWriter {
    /// the amount of bytes that were written so far
    pub bytes_written: usize,
}
impl Writer for SizeWriter {
    #[inline(always)]
    fn write(
        &mut self,
        bytes: &[u8],
    ) -> Result<(), EncodeError> {
        self.bytes_written += bytes.len();

        Ok(())
    }

    #[inline(always)]
    fn write_u8(
        &mut self,
        _: u8,
    ) -> Result<(), EncodeError> {
        self.bytes_written += 1;

        Ok(())
    }

    #[inline(always)]
    fn write_u16(
        &mut self,
        _: u16,
    ) -> Result<(), EncodeError> {
        self.bytes_written += 2;

        Ok(())
    }

    #[inline(always)]
    fn write_u32(
        &mut self,
        _: u32,
    ) -> Result<(), EncodeError> {
        self.bytes_written += 4;

        Ok(())
    }

    #[inline(always)]
    fn write_u64(
        &mut self,
        _: u64,
    ) -> Result<(), EncodeError> {
        self.bytes_written += 8;

        Ok(())
    }

    #[inline(always)]
    fn write_u128(
        &mut self,
        _: u128,
    ) -> Result<(), EncodeError> {
        self.bytes_written += 16;

        Ok(())
    }
}
