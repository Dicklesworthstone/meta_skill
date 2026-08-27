//! This module contains reader-based structs and traits.
//!
//! Because `std::io::Read` is only limited to `std` and not `core`, we provide 2 alternative readers.
//!
//! [`Reader`\] is a reader for sources that do not own their data. It is assumed that the reader's data is dropped after the `read` method is called. This reader is incapable of reading borrowed data, like `&str` and `&[u8]`.
//!
//! [`BorrowReader`\] is an extension of `Reader` that also allows returning borrowed data. A `BorrowReader` allows reading `&str` and `&[u8]`.
//!
//! Specifically the `Reader` trait is used by [`Decode`\] and the `BorrowReader` trait is used by [`BorrowDecode`\].
//!
//! [Decode]: ../trait.Decode.html
//! [BorrowDecode]: ../trait.BorrowDecode.html
#![allow(unsafe_code)]

use crate::error::DecodeError;

/// A reader for owned data. See the module documentation for more information.
pub trait Reader {
    /// Fill the given `bytes` argument with values. Exactly the length of the given slice must be filled, or else an error must be returned.
    ///
    /// # Errors
    ///
    /// Returns `DecodeError::UnexpectedEnd` if the reader does not have enough bytes.
    fn read(
        &mut self,
        bytes: &mut [u8],
    ) -> Result<(), DecodeError>;

    /// Read a single byte from the reader.
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails.
    #[inline(always)]
    fn read_u8(&mut self) -> Result<u8, DecodeError> {
        let mut byte = [0u8; 1];
        self.read(&mut byte)?;
        Ok(byte[0])
    }

    /// Read a `u16` from the reader.
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails.
    #[inline(always)]
    fn read_u16(&mut self) -> Result<u16, DecodeError> {
        let mut bytes = [0u8; 2];
        self.read(&mut bytes)?;
        Ok(u16::from_ne_bytes(bytes))
    }

    /// Read a `u32` from the reader.
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails.
    #[inline(always)]
    fn read_u32(&mut self) -> Result<u32, DecodeError> {
        let mut bytes = [0u8; 4];
        self.read(&mut bytes)?;
        Ok(u32::from_ne_bytes(bytes))
    }

    /// Read a `u64` from the reader.
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails.
    #[inline(always)]
    fn read_u64(&mut self) -> Result<u64, DecodeError> {
        let mut bytes = [0u8; 8];
        self.read(&mut bytes)?;
        Ok(u64::from_ne_bytes(bytes))
    }

    /// Read a `u128` from the reader.
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails.
    #[inline(always)]
    fn read_u128(&mut self) -> Result<u128, DecodeError> {
        let mut bytes = [0u8; 16];
        self.read(&mut bytes)?;
        Ok(u128::from_ne_bytes(bytes))
    }

    /// If this reader wraps a buffer of any kind, this function lets callers access contents of
    /// the buffer without passing data through a buffer first.
    #[inline(always)]
    fn peek_read(
        &mut self,
        _: usize,
    ) -> Option<&[u8]> {
        None
    }

    /// If an implementation of `peek_read` is provided, an implementation of this function
    /// must be provided so that subsequent reads or peek-reads do not return the same bytes
    #[inline(always)]
    fn consume(
        &mut self,
        _: usize,
    ) {
    }

    /// Returns the next byte without consuming it.
    #[inline(always)]
    fn peek_u8(&mut self) -> Option<u8> {
        self.peek_read(1).map(|b| b[0])
    }
}

impl<T> Reader for &mut T
where
    T: Reader,
{
    #[inline(always)]
    fn read(
        &mut self,
        bytes: &mut [u8],
    ) -> Result<(), DecodeError> {
        (**self).read(bytes)
    }

    #[inline(always)]
    fn peek_read(
        &mut self,
        n: usize,
    ) -> Option<&[u8]> {
        (**self).peek_read(n)
    }

    #[inline(always)]
    fn consume(
        &mut self,
        n: usize,
    ) {
        (*self).consume(n);
    }

    #[inline(always)]
    fn read_u16(&mut self) -> Result<u16, DecodeError> {
        (**self).read_u16()
    }

    #[inline(always)]
    fn read_u32(&mut self) -> Result<u32, DecodeError> {
        (**self).read_u32()
    }

    #[inline(always)]
    fn read_u64(&mut self) -> Result<u64, DecodeError> {
        (**self).read_u64()
    }

    #[inline(always)]
    fn read_u128(&mut self) -> Result<u128, DecodeError> {
        (**self).read_u128()
    }
}

/// A reader for borrowed data. Implementers of this must also implement the [`Reader`\] trait. See the module documentation for more information.
pub trait BorrowReader<'storage>: Reader {
    /// Read exactly `length` bytes and return a slice to this data. If not enough bytes could be read, an error should be returned.
    ///
    /// *note*: Exactly `length` bytes must be returned. If less bytes are returned, bincode may panic. If more bytes are returned, the excess bytes may be discarded.
    ///
    /// # Errors
    ///
    /// Returns `DecodeError::UnexpectedEnd` if the reader does not have enough bytes.
    fn take_bytes(
        &mut self,
        length: usize,
    ) -> Result<&'storage [u8], DecodeError>;
}

/// A reader type for `&[u8]` slices. Implements both [`Reader`\] and [`BorrowReader`\], and thus can be used for borrowed data.
pub struct SliceReader<'storage> {
    pub(crate) slice: &'storage [u8],
}

impl<'storage> SliceReader<'storage> {
    /// Constructs a slice reader
    #[must_use]
    pub const fn new(bytes: &'storage [u8]) -> Self {
        Self { slice: bytes }
    }
}

impl<'storage> Reader for SliceReader<'storage> {
    #[inline(always)]
    fn read(
        &mut self,
        bytes: &mut [u8],
    ) -> Result<(), DecodeError> {
        if bytes.len() > self.slice.len() {
            return crate::error::cold_decode_error_unexpected_end(bytes.len() - self.slice.len());
        }
        // SAFETY: `bytes.len() <= self.slice.len()` is checked above.
        // `copy_nonoverlapping` is safe because pointers do not overlap and the lengths match.
        // `get_unchecked` is safe because `bytes.len()` is <= the length of `self.slice`.
        unsafe {
            core::ptr::copy_nonoverlapping(self.slice.as_ptr(), bytes.as_mut_ptr(), bytes.len());
            self.slice = self.slice.get_unchecked(bytes.len()..);
        }

        Ok(())
    }

    #[inline(always)]
    fn peek_read(
        &mut self,
        n: usize,
    ) -> Option<&'storage [u8]> {
        self.slice.get(..n)
    }

    #[inline(always)]
    fn consume(
        &mut self,
        n: usize,
    ) {
        if n >= self.slice.len() {
            self.slice = &[];
        } else {
            self.slice = unsafe { self.slice.get_unchecked(n..) };
        }
    }

    #[inline(always)]
    fn peek_u8(&mut self) -> Option<u8> {
        self.slice.first().copied()
    }

    #[inline(always)]
    fn read_u8(&mut self) -> Result<u8, DecodeError> {
        if self.slice.is_empty() {
            return crate::error::cold_decode_error_unexpected_end(1);
        }
        let byte = unsafe { *self.slice.get_unchecked(0) };
        self.slice = unsafe { self.slice.get_unchecked(1..) };
        Ok(byte)
    }

    #[inline(always)]
    fn read_u16(&mut self) -> Result<u16, DecodeError> {
        if self.slice.len() < 2 {
            return crate::error::cold_decode_error_unexpected_end(2 - self.slice.len());
        }
        let val = unsafe { core::ptr::read_unaligned(self.slice.as_ptr().cast::<u16>()) };
        self.slice = unsafe { self.slice.get_unchecked(2..) };
        Ok(val)
    }

    #[inline(always)]
    fn read_u32(&mut self) -> Result<u32, DecodeError> {
        if self.slice.len() < 4 {
            return crate::error::cold_decode_error_unexpected_end(4 - self.slice.len());
        }
        let val = unsafe { core::ptr::read_unaligned(self.slice.as_ptr().cast::<u32>()) };
        self.slice = unsafe { self.slice.get_unchecked(4..) };
        Ok(val)
    }

    #[inline(always)]
    fn read_u64(&mut self) -> Result<u64, DecodeError> {
        if self.slice.len() < 8 {
            return crate::error::cold_decode_error_unexpected_end(8 - self.slice.len());
        }
        let val = unsafe { core::ptr::read_unaligned(self.slice.as_ptr().cast::<u64>()) };
        self.slice = unsafe { self.slice.get_unchecked(8..) };
        Ok(val)
    }

    #[inline(always)]
    fn read_u128(&mut self) -> Result<u128, DecodeError> {
        if self.slice.len() < 16 {
            return crate::error::cold_decode_error_unexpected_end(16 - self.slice.len());
        }
        let val = unsafe { core::ptr::read_unaligned(self.slice.as_ptr().cast::<u128>()) };
        self.slice = unsafe { self.slice.get_unchecked(16..) };
        Ok(val)
    }
}

impl<'storage> BorrowReader<'storage> for SliceReader<'storage> {
    #[inline(always)]
    fn take_bytes(
        &mut self,
        length: usize,
    ) -> Result<&'storage [u8], DecodeError> {
        if length > self.slice.len() {
            return crate::error::cold_decode_error_unexpected_end(length - self.slice.len());
        }
        // SAFETY: `length <= self.slice.len()` is checked above.
        // `get_unchecked` is therefore safe for both the read portion and the remainder.
        unsafe {
            let read_slice = self.slice.get_unchecked(..length);
            self.slice = self.slice.get_unchecked(length..);
            Ok(read_slice)
        }
    }
}
