//! Bounded types for compile-time size guarantees.
//!
//! These types enforce capacity limits both at construction time and during
//! decoding, preventing Denial-of-Service attacks via unbounded allocations.

#![cfg(feature = "alloc")]

use crate::de::BorrowDecode;
use crate::de::BorrowDecoder;
use crate::de::Decode;
use crate::de::Decoder;
use crate::de::read::Reader;
use crate::enc::Encode;
use crate::enc::Encoder;
use crate::error::DecodeError;
use crate::error::EncodeError;
use crate::static_size::StaticSize;
use crate::static_size::helpers::VARINT_MAX_64;
use alloc::string::String;
use alloc::vec::Vec;

/// A `Vec` wrapper with a compile-time capacity limit.
///
/// During decoding, the length is validated *before* allocation to prevent
/// OOM attacks from malicious length prefixes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BoundedVec<T, const CAP: usize>(pub Vec<T>);

impl<T, const CAP: usize> StaticSize for BoundedVec<T, CAP>
where
    T: StaticSize,
{
    const MAX_SIZE: usize = VARINT_MAX_64 + T::MAX_SIZE * CAP;
}

impl<T: Encode, const CAP: usize> Encode for BoundedVec<T, CAP> {
    #[inline]
    fn encode<E: Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), EncodeError> {
        if self.0.len() > CAP {
            return crate::error::cold_encode_error_other("BoundedVec exceeds capacity");
        }
        self.0.encode(encoder)
    }
}

impl<Context, T: Decode<Context>, const CAP: usize> Decode<Context> for BoundedVec<T, CAP> {
    #[inline]
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        // Decode length first and validate BEFORE allocating
        let len = crate::de::decode_slice_len(decoder)?;
        if len > CAP {
            return crate::error::cold_decode_error_other("BoundedVec length exceeds capacity");
        }

        decoder.claim_container_read::<T>(len)?;
        let mut vec = Vec::with_capacity(len);
        for _ in 0..len {
            decoder.unclaim_bytes_read(core::mem::size_of::<T>());
            vec.push(T::decode(decoder)?);
        }
        Ok(Self(vec))
    }
}

impl<'de, Context, T: BorrowDecode<'de, Context>, const CAP: usize> BorrowDecode<'de, Context>
    for BoundedVec<T, CAP>
{
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D
    ) -> Result<Self, DecodeError> {
        // Decode length first and validate BEFORE allocating
        let len = crate::de::decode_slice_len(decoder)?;
        if len > CAP {
            return crate::error::cold_decode_error_other("BoundedVec length exceeds capacity");
        }

        decoder.claim_container_read::<T>(len)?;
        let mut vec = Vec::with_capacity(len);
        for _ in 0..len {
            decoder.unclaim_bytes_read(core::mem::size_of::<T>());
            vec.push(T::borrow_decode(decoder)?);
        }
        Ok(Self(vec))
    }
}

/// A `String` wrapper with a compile-time byte-length capacity limit.
///
/// During decoding, the byte length is validated *before* allocation to
/// prevent OOM attacks from malicious length prefixes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BoundedString<const CAP: usize>(pub String);

impl<const CAP: usize> StaticSize for BoundedString<CAP> {
    const MAX_SIZE: usize = VARINT_MAX_64 + CAP;
}

impl<const CAP: usize> Encode for BoundedString<CAP> {
    #[inline]
    fn encode<E: Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), EncodeError> {
        if self.0.len() > CAP {
            return crate::error::cold_encode_error_other("BoundedString exceeds capacity");
        }
        self.0.encode(encoder)
    }
}

impl<Context, const CAP: usize> Decode<Context> for BoundedString<CAP> {
    #[inline]
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        // Decode byte length first and validate BEFORE allocating
        let len = crate::de::decode_slice_len(decoder)?;
        if len > CAP {
            return crate::error::cold_decode_error_other("BoundedString length exceeds capacity");
        }

        decoder.claim_container_read::<u8>(len)?;
        let mut bytes = alloc::vec![0u8; len];
        decoder.reader().read(&mut bytes)?;
        let s = String::from_utf8(bytes)
            .map_err(|e| crate::error::cold_decode_error_utf8::<()>(e.utf8_error()).unwrap_err())?;
        Ok(Self(s))
    }
}

impl<'de, Context, const CAP: usize> BorrowDecode<'de, Context> for BoundedString<CAP> {
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D
    ) -> Result<Self, DecodeError> {
        // Decode byte length first and validate BEFORE allocating
        let len = crate::de::decode_slice_len(decoder)?;
        if len > CAP {
            return crate::error::cold_decode_error_other("BoundedString length exceeds capacity");
        }

        decoder.claim_container_read::<u8>(len)?;
        let mut bytes = alloc::vec![0u8; len];
        decoder.reader().read(&mut bytes)?;
        let s = String::from_utf8(bytes)
            .map_err(|e| crate::error::cold_decode_error_utf8::<()>(e.utf8_error()).unwrap_err())?;
        Ok(Self(s))
    }
}

/// Error returned when a value exceeds a bounded type's capacity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundsExceeded;

impl core::fmt::Display for BoundsExceeded {
    #[inline]
    fn fmt(
        &self,
        f: &mut core::fmt::Formatter<'_>,
    ) -> core::fmt::Result {
        write!(f, "value exceeds bounded capacity")
    }
}

impl<T, const CAP: usize> TryFrom<Vec<T>> for BoundedVec<T, CAP> {
    type Error = BoundsExceeded;

    #[inline]
    fn try_from(v: Vec<T>) -> Result<Self, Self::Error> {
        if v.len() > CAP {
            Err(BoundsExceeded)
        } else {
            Ok(Self(v))
        }
    }
}

impl<const CAP: usize> TryFrom<String> for BoundedString<CAP> {
    type Error = BoundsExceeded;

    #[inline]
    fn try_from(s: String) -> Result<Self, Self::Error> {
        if s.len() > CAP {
            Err(BoundsExceeded)
        } else {
            Ok(Self(s))
        }
    }
}

impl<T, const CAP: usize> BoundedVec<T, CAP> {
    /// Create a new empty `BoundedVec`.
    #[must_use]
    #[inline(always)]
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    /// Try to push an element, returning an error if the capacity would be exceeded.
    ///
    /// # Errors
    ///
    /// Returns `BoundsExceeded` if the vector is already at its maximum capacity.
    #[inline]
    pub fn try_push(
        &mut self,
        value: T,
    ) -> Result<(), BoundsExceeded> {
        if self.0.len() >= CAP {
            Err(BoundsExceeded)
        } else {
            self.0.push(value);
            Ok(())
        }
    }

    /// Returns the inner `Vec<T>`.
    #[must_use]
    #[inline(always)]
    pub fn into_inner(self) -> Vec<T> {
        self.0
    }
}

impl<T, const CAP: usize> Default for BoundedVec<T, CAP> {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}

impl<const CAP: usize> BoundedString<CAP> {
    /// Create a new empty `BoundedString`.
    #[must_use]
    #[inline(always)]
    pub const fn new() -> Self {
        Self(String::new())
    }

    /// Returns the inner `String`.
    #[must_use]
    #[inline(always)]
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl<const CAP: usize> Default for BoundedString<CAP> {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const CAP: usize> core::ops::Deref for BoundedVec<T, CAP> {
    type Target = Vec<T>;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<const CAP: usize> core::ops::Deref for BoundedString<CAP> {
    type Target = String;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
