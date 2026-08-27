#![allow(unsafe_code)]

use core::marker::PhantomData;

#[cfg(feature = "alloc")]
use alloc::string::String;
#[cfg(feature = "alloc")]
use alloc::vec::Vec;

/// Indicates that a type has a fixed size known at compile time.
/// This allows us to perform bounds checking efficiently.
pub trait StaticSize {
    /// The size of the type in bytes.
    const SIZE: usize;
}

macro_rules! impl_static_size {
    ($($t:ty),*) => {
        $(
            impl StaticSize for $t {
                const SIZE: usize = core::mem::size_of::<$t>();
            }
            unsafe impl ZeroCopy for $t {
                const ALIGN: usize = core::mem::align_of::<$t>();
            }
        )*
    };
}

impl_static_size!(
    u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize, f32, f64, char, bool
);

impl<T: StaticSize, const N: usize> StaticSize for [T; N] {
    const SIZE: usize = T::SIZE * N;
}

/// A marker trait indicating that a type has a fixed, predictable layout (e.g., `#[repr(C)]`)
/// and contains no padding bytes or invalid bit patterns allowing safe zero-copy casting.
///
/// # Safety
///
/// The type must have a fixed, predictable layout (e.g., `#[repr(C)]`) and contain no padding bytes or invalid bit patterns allowing safe zero-copy casting.
pub unsafe trait ZeroCopy: StaticSize {
    /// The required alignment for this type.
    const ALIGN: usize;
}

unsafe impl<T: ZeroCopy, const N: usize> ZeroCopy for [T; N] {
    const ALIGN: usize = T::ALIGN;
}

/// Trait for handling endianness in zero-copy types.
#[doc(hidden)]
pub trait Endian {
    /// Converts a u16 from native endianness to the relative pointer's endianness.
    fn from_native_u16(v: u16) -> u16;
    /// Converts a u16 from the relative pointer's endianness to native endianness.
    fn to_native_u16(v: u16) -> u16;
    /// Converts a u32 from native endianness to the relative pointer's endianness.
    fn from_native_u32(v: u32) -> u32;
    /// Converts a u32 from the relative pointer's endianness to native endianness.
    fn to_native_u32(v: u32) -> u32;
    /// Converts a u64 from native endianness to the relative pointer's endianness.
    fn from_native_u64(v: u64) -> u64;
    /// Converts a u64 from the relative pointer's endianness to native endianness.
    fn to_native_u64(v: u64) -> u64;
    /// Converts a u128 from native endianness to the relative pointer's endianness.
    fn from_native_u128(v: u128) -> u128;
    /// Converts a u128 from the relative pointer's endianness to native endianness.
    fn to_native_u128(v: u128) -> u128;

    /// Converts a i16 from native endianness to the relative pointer's endianness.
    fn from_native_i16(v: i16) -> i16;
    /// Converts a i16 from the relative pointer's endianness to native endianness.
    fn to_native_i16(v: i16) -> i16;
    /// Converts a i32 from native endianness to the relative pointer's endianness.
    fn from_native_i32(v: i32) -> i32;
    /// Converts a i32 from the relative pointer's endianness to native endianness.
    fn to_native_i32(v: i32) -> i32;
    /// Converts a i64 from native endianness to the relative pointer's endianness.
    fn from_native_i64(v: i64) -> i64;
    /// Converts a i64 from the relative pointer's endianness to native endianness.
    fn to_native_i64(v: i64) -> i64;
    /// Converts a i128 from native endianness to the relative pointer's endianness.
    fn from_native_i128(v: i128) -> i128;
    /// Converts a i128 from the relative pointer's endianness to native endianness.
    fn to_native_i128(v: i128) -> i128;

    #[inline(always)]
    /// Converts a f32 from native endianness to the relative pointer's endianness.
    #[must_use]
    fn from_native_f32(v: f32) -> f32 {
        f32::from_bits(Self::from_native_u32(v.to_bits()))
    }
    #[inline(always)]
    /// Converts a f32 from the relative pointer's endianness to native endianness.
    #[must_use]
    fn to_native_f32(v: f32) -> f32 {
        f32::from_bits(Self::to_native_u32(v.to_bits()))
    }
    #[inline(always)]
    /// Converts a f64 from native endianness to the relative pointer's endianness.
    #[must_use]
    fn from_native_f64(v: f64) -> f64 {
        f64::from_bits(Self::from_native_u64(v.to_bits()))
    }
    #[inline(always)]
    /// Converts a f64 from the relative pointer's endianness to native endianness.
    #[must_use]
    fn to_native_f64(v: f64) -> f64 {
        f64::from_bits(Self::to_native_u64(v.to_bits()))
    }
}

/// Little-endian marker.
#[doc(hidden)]
#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub struct LittleEndian;
impl Endian for LittleEndian {
    #[inline(always)]
    fn from_native_u16(v: u16) -> u16 {
        v.to_le()
    }

    #[inline(always)]
    fn to_native_u16(v: u16) -> u16 {
        u16::from_le(v)
    }

    #[inline(always)]
    fn from_native_u32(v: u32) -> u32 {
        v.to_le()
    }

    #[inline(always)]
    fn to_native_u32(v: u32) -> u32 {
        u32::from_le(v)
    }

    #[inline(always)]
    fn from_native_u64(v: u64) -> u64 {
        v.to_le()
    }

    #[inline(always)]
    fn to_native_u64(v: u64) -> u64 {
        u64::from_le(v)
    }

    #[inline(always)]
    fn from_native_u128(v: u128) -> u128 {
        v.to_le()
    }

    #[inline(always)]
    fn to_native_u128(v: u128) -> u128 {
        u128::from_le(v)
    }

    #[inline(always)]
    fn from_native_i16(v: i16) -> i16 {
        v.to_le()
    }

    #[inline(always)]
    fn to_native_i16(v: i16) -> i16 {
        i16::from_le(v)
    }

    #[inline(always)]
    fn from_native_i32(v: i32) -> i32 {
        v.to_le()
    }

    #[inline(always)]
    fn to_native_i32(v: i32) -> i32 {
        i32::from_le(v)
    }

    #[inline(always)]
    fn from_native_i64(v: i64) -> i64 {
        v.to_le()
    }

    #[inline(always)]
    fn to_native_i64(v: i64) -> i64 {
        i64::from_le(v)
    }

    #[inline(always)]
    fn from_native_i128(v: i128) -> i128 {
        v.to_le()
    }

    #[inline(always)]
    fn to_native_i128(v: i128) -> i128 {
        i128::from_le(v)
    }
}

/// Big-endian marker.
#[doc(hidden)]
#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub struct BigEndian;
impl Endian for BigEndian {
    #[inline(always)]
    fn from_native_u16(v: u16) -> u16 {
        v.to_be()
    }

    #[inline(always)]
    fn to_native_u16(v: u16) -> u16 {
        u16::from_be(v)
    }

    #[inline(always)]
    fn from_native_u32(v: u32) -> u32 {
        v.to_be()
    }

    #[inline(always)]
    fn to_native_u32(v: u32) -> u32 {
        u32::from_be(v)
    }

    #[inline(always)]
    fn from_native_u64(v: u64) -> u64 {
        v.to_be()
    }

    #[inline(always)]
    fn to_native_u64(v: u64) -> u64 {
        u64::from_be(v)
    }

    #[inline(always)]
    fn from_native_u128(v: u128) -> u128 {
        v.to_be()
    }

    #[inline(always)]
    fn to_native_u128(v: u128) -> u128 {
        u128::from_be(v)
    }

    #[inline(always)]
    fn from_native_i16(v: i16) -> i16 {
        v.to_be()
    }

    #[inline(always)]
    fn to_native_i16(v: i16) -> i16 {
        i16::from_be(v)
    }

    #[inline(always)]
    fn from_native_i32(v: i32) -> i32 {
        v.to_be()
    }

    #[inline(always)]
    fn to_native_i32(v: i32) -> i32 {
        i32::from_be(v)
    }

    #[inline(always)]
    fn from_native_i64(v: i64) -> i64 {
        v.to_be()
    }

    #[inline(always)]
    fn to_native_i64(v: i64) -> i64 {
        i64::from_be(v)
    }

    #[inline(always)]
    fn from_native_i128(v: i128) -> i128 {
        v.to_be()
    }

    #[inline(always)]
    fn to_native_i128(v: i128) -> i128 {
        i128::from_be(v)
    }
}

/// Native-endian marker.
#[doc(hidden)]
#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub struct NativeEndian;
impl Endian for NativeEndian {
    #[inline(always)]
    fn from_native_u16(v: u16) -> u16 {
        v
    }

    #[inline(always)]
    fn to_native_u16(v: u16) -> u16 {
        v
    }

    #[inline(always)]
    fn from_native_u32(v: u32) -> u32 {
        v
    }

    #[inline(always)]
    fn to_native_u32(v: u32) -> u32 {
        v
    }

    #[inline(always)]
    fn from_native_u64(v: u64) -> u64 {
        v
    }

    #[inline(always)]
    fn to_native_u64(v: u64) -> u64 {
        v
    }

    #[inline(always)]
    fn from_native_u128(v: u128) -> u128 {
        v
    }

    #[inline(always)]
    fn to_native_u128(v: u128) -> u128 {
        v
    }

    #[inline(always)]
    fn from_native_i16(v: i16) -> i16 {
        v
    }

    #[inline(always)]
    fn to_native_i16(v: i16) -> i16 {
        v
    }

    #[inline(always)]
    fn from_native_i32(v: i32) -> i32 {
        v
    }

    #[inline(always)]
    fn to_native_i32(v: i32) -> i32 {
        v
    }

    #[inline(always)]
    fn from_native_i64(v: i64) -> i64 {
        v
    }

    #[inline(always)]
    fn to_native_i64(v: i64) -> i64 {
        v
    }

    #[inline(always)]
    fn from_native_i128(v: i128) -> i128 {
        v
    }

    #[inline(always)]
    fn to_native_i128(v: i128) -> i128 {
        v
    }
}

/// A relative pointer that stores the offset from its own address to the target data.
/// This allows zero-copy deserialization without runtime allocations.
#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct RelativePtr<T, const ALIGN: usize, E: Endian = NativeEndian> {
    offset: i32,
    _marker: PhantomData<(T, E)>,
}

impl<T, const ALIGN: usize, E: Endian> RelativePtr<T, ALIGN, E> {
    /// Creates a new `RelativePtr` with the given native offset.
    #[must_use]
    pub fn new(offset: i32) -> Self {
        Self {
            offset: E::from_native_i32(offset),
            _marker: PhantomData,
        }
    }

    /// Resolves the pointer within the given buffer.
    /// Returns `Some(&T)` if the computed pointer is within the bounds of `buffer`
    /// and correctly aligned. Otherwise, returns `None`.
    #[must_use]
    pub fn get<'a>(
        &self,
        buffer: &'a [u8],
    ) -> Option<&'a T>
    where
        T: ZeroCopy,
    {
        // Compile-time check: alignment must be a power of two, and sufficiently large for T
        const {
            let effective_align = if ALIGN == 0 {
                T::ALIGN
            } else {
                ALIGN
            };
            assert!(
                effective_align > 0 && effective_align.is_power_of_two(),
                "Alignment must be a power of two"
            );
            assert!(
                effective_align >= core::mem::align_of::<T>(),
                "ALIGN must be at least the natural alignment of T"
            );
        };

        let self_ptr = core::ptr::from_ref(self) as usize;
        let buffer_start = buffer.as_ptr() as usize;
        let buffer_end = buffer_start + buffer.len();

        let self_end = self_ptr + core::mem::size_of::<Self>();

        if self_ptr < buffer_start || self_end > buffer_end {
            return None;
        }

        let offset = E::to_native_i32(unsafe { core::ptr::addr_of!(self.offset).read_unaligned() });

        // Here we use wrapping arithmetic because the offset is relative to the pointer's address
        // and could potentially wrap around the address space. But in the CPU, the address space
        // is a linear ring, so we can use wrapping arithmetic to avoid overflow.
        let target_addr = if offset >= 0 {
            self_ptr.wrapping_add(offset as usize)
        } else {
            self_ptr.wrapping_sub(offset.unsigned_abs() as usize)
        };

        let target_end = target_addr.wrapping_add(T::SIZE);

        if target_addr < buffer_start || target_end > buffer_end {
            return None;
        }

        let effective_align = if ALIGN == 0 {
            T::ALIGN
        } else {
            ALIGN
        };
        // Runtime alignment check
        if target_addr % effective_align != 0 {
            return None;
        }

        // Derive target_ptr from buffer.as_ptr() to maintain provenance over the whole buffer
        let target_offset_in_buffer = target_addr - buffer_start;
        let target_ptr = unsafe { buffer.as_ptr().add(target_offset_in_buffer) };

        // Safe because we bounds checked against the buffer, alignment checked,
        // and we derive the lifetime from the buffer.
        Some(unsafe { &*target_ptr.cast::<T>() })
    }
}

#[cfg(feature = "fuzzing")]
impl<'a, T, const ALIGN: usize, E: Endian> arbitrary::Arbitrary<'a> for RelativePtr<T, ALIGN, E> {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(Self::new(i32::arbitrary(u)?))
    }
}

impl<T, const ALIGN: usize, E: Endian> StaticSize for RelativePtr<T, ALIGN, E> {
    const SIZE: usize = core::mem::size_of::<Self>();
}

unsafe impl<T, const ALIGN: usize, E: Endian> ZeroCopy for RelativePtr<T, ALIGN, E> {
    const ALIGN: usize = core::mem::align_of::<i32>();
}

/// A zero-copy array collection equivalent to `[T; N]`.
#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct ZeroArray<T, const N: usize, const ALIGN: usize, E: Endian = NativeEndian> {
    ptr: RelativePtr<[T; N], ALIGN, E>,
}

impl<T: ZeroCopy, const N: usize, const ALIGN: usize, E: Endian> ZeroArray<T, N, ALIGN, E> {
    /// Resolves the array within the given buffer.
    #[must_use]
    pub fn get<'a>(
        &self,
        buffer: &'a [u8],
    ) -> Option<&'a [T; N]> {
        self.ptr.get(buffer)
    }
}

#[cfg(feature = "fuzzing")]
impl<'a, T: ZeroCopy, const N: usize, const ALIGN: usize, E: Endian> arbitrary::Arbitrary<'a>
    for ZeroArray<T, N, ALIGN, E>
{
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(Self {
            ptr: arbitrary::Arbitrary::arbitrary(u)?,
        })
    }
}

impl<T, const N: usize, const ALIGN: usize, E: Endian> StaticSize for ZeroArray<T, N, ALIGN, E> {
    const SIZE: usize = core::mem::size_of::<Self>();
}

// Implement ZeroCopy for zero-copy containers since their layouts are guaranteed
unsafe impl<T: ZeroCopy, const N: usize, const ALIGN: usize, E: Endian> ZeroCopy
    for ZeroArray<T, N, ALIGN, E>
{
    const ALIGN: usize = core::mem::align_of::<i32>();
}

/// A zero-copy string type with compile-time known max capacity, stored inline.
#[repr(C)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct ZeroString<const CAP: usize, E: Endian = NativeEndian> {
    len: u32,
    data: [u8; CAP],
    _marker: PhantomData<E>,
}

impl<const CAP: usize, E: Endian> ZeroString<CAP, E> {
    /// Resolves the string. Inline strings don't necessarily need the buffer.
    #[must_use]
    pub fn get(&self) -> Option<&str> {
        let len =
            E::to_native_u32(unsafe { core::ptr::addr_of!(self.len).read_unaligned() }) as usize;
        let len = len.min(CAP);
        core::str::from_utf8(&self.data[..len]).ok()
    }
}

#[cfg(feature = "fuzzing")]
impl<'a, const CAP: usize, E: Endian> arbitrary::Arbitrary<'a> for ZeroString<CAP, E> {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let len = u32::arbitrary(u)? % (CAP as u32 + 1);
        let mut data = [0u8; CAP];
        u.fill_buffer(&mut data[..len as usize])?;
        Ok(Self {
            len: E::from_native_u32(len),
            data,
            _marker: PhantomData,
        })
    }
}

impl<const CAP: usize, E: Endian> StaticSize for ZeroString<CAP, E> {
    const SIZE: usize = core::mem::size_of::<Self>();
}

unsafe impl<const CAP: usize, E: Endian> ZeroCopy for ZeroString<CAP, E> {
    const ALIGN: usize = core::mem::align_of::<u32>();
}

/// A zero-copy slice collection conceptually equivalent to `&[T]` or `Vec<T>`.
#[repr(C)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct ZeroSlice<T, const ALIGN: usize, E: Endian = NativeEndian> {
    len: u32,
    ptr: RelativePtr<T, ALIGN, E>,
}

impl<T: ZeroCopy, const ALIGN: usize, E: Endian> ZeroSlice<T, ALIGN, E> {
    /// Creates a new `ZeroSlice` with the given native length and relative offset.
    #[must_use]
    pub fn new(
        len: u32,
        offset: i32,
    ) -> Self {
        Self {
            len: E::from_native_u32(len),
            ptr: RelativePtr::new(offset),
        }
    }

    /// Resolves the slice within the given buffer.
    #[must_use]
    pub fn get<'a>(
        &self,
        buffer: &'a [u8],
    ) -> Option<&'a [T]> {
        let len = E::to_native_u32(unsafe { core::ptr::addr_of!(self.len).read_unaligned() });

        if len == 0 {
            // For zero-length slices, we can just return an empty slice,
            // bypassing the pointer lookup (which might be invalid or dummy).
            return Some(&[]);
        }

        // Get the first element's reference to validate base bounds, alignment, and offset
        let first_ref = self.ptr.get(buffer)?;

        let slice_len = len as usize;

        // Calculate the total size required for the full slice.
        let total_size = T::SIZE.checked_mul(slice_len)?;

        let first_addr = core::ptr::from_ref::<T>(first_ref) as usize;
        let buffer_start = buffer.as_ptr() as usize;
        let target_offset = first_addr - buffer_start;

        let target_end = target_offset.checked_add(total_size)?;
        if target_end > buffer.len() {
            return None;
        }

        // Derive target_ptr from buffer.as_ptr() to maintain provenance over the whole slice
        let target_ptr = unsafe { buffer.as_ptr().add(target_offset).cast::<T>() };

        Some(unsafe { core::slice::from_raw_parts(target_ptr, slice_len) })
    }
}

#[cfg(feature = "fuzzing")]
impl<'a, T: ZeroCopy, const ALIGN: usize, E: Endian> arbitrary::Arbitrary<'a>
    for ZeroSlice<T, ALIGN, E>
{
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(Self::new(u32::arbitrary(u)?, i32::arbitrary(u)?))
    }
}

impl<T, const ALIGN: usize, E: Endian> StaticSize for ZeroSlice<T, ALIGN, E> {
    const SIZE: usize = core::mem::size_of::<Self>();
}

unsafe impl<T: ZeroCopy, const ALIGN: usize, E: Endian> ZeroCopy for ZeroSlice<T, ALIGN, E> {
    const ALIGN: usize = core::mem::align_of::<u32>();
}

/// A dynamically sized zero-copy string conceptually equivalent to `&str` or `String`.
#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct ZeroStr<E: Endian = NativeEndian> {
    slice: ZeroSlice<u8, 0, E>,
}

impl<E: Endian> ZeroStr<E> {
    /// Creates a new instance.
    #[must_use]
    pub fn new(
        len: u32,
        offset: i32,
    ) -> Self {
        Self {
            slice: ZeroSlice::new(len, offset),
        }
    }

    /// Returns a reference to the underlying data if valid.
    #[must_use]
    pub fn get<'a>(
        &self,
        buffer: &'a [u8],
    ) -> Option<&'a str> {
        let bytes = self.slice.get(buffer)?;
        core::str::from_utf8(bytes).ok()
    }
}

#[cfg(feature = "fuzzing")]
impl<'a, E: Endian> arbitrary::Arbitrary<'a> for ZeroStr<E> {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(Self {
            slice: arbitrary::Arbitrary::arbitrary(u)?,
        })
    }
}

impl<E: Endian> StaticSize for ZeroStr<E> {
    const SIZE: usize = core::mem::size_of::<Self>();
}

unsafe impl<E: Endian> ZeroCopy for ZeroStr<E> {
    const ALIGN: usize = core::mem::align_of::<u32>();
}

/// A trait for validating zero-copy types.
pub trait Validator {
    /// Checks if the relative pointer is valid within the given buffer.
    fn is_valid(
        &self,
        buffer: &[u8],
    ) -> bool;
}

impl<T: ZeroCopy, const ALIGN: usize, E: Endian> Validator for RelativePtr<T, ALIGN, E> {
    /// Checks if the relative pointer is valid within the given buffer.
    fn is_valid(
        &self,
        buffer: &[u8],
    ) -> bool {
        self.get(buffer).is_some()
    }
}

impl<const CAP: usize, E: Endian> Validator for ZeroString<CAP, E> {
    /// Checks if the relative pointer is valid within the given buffer.
    fn is_valid(
        &self,
        _buffer: &[u8],
    ) -> bool {
        self.get().is_some()
    }
}

impl<T: ZeroCopy, const ALIGN: usize, E: Endian> Validator for ZeroSlice<T, ALIGN, E> {
    /// Checks if the relative pointer is valid within the given buffer.
    fn is_valid(
        &self,
        buffer: &[u8],
    ) -> bool {
        self.get(buffer).is_some()
    }
}

impl<E: Endian> Validator for ZeroStr<E> {
    /// Checks if the relative pointer is valid within the given buffer.
    fn is_valid(
        &self,
        buffer: &[u8],
    ) -> bool {
        self.get(buffer).is_some()
    }
}

macro_rules! impl_validator_primitive {
    ($($t:ty),*) => {
        $(
            impl Validator for $t {
                /// Checks if the relative pointer is valid within the given buffer.
                fn is_valid(&self, _buffer: &[u8]) -> bool {
                    true
                }
            }
        )*
    };
}

impl_validator_primitive!(
    u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize, f32, f64, char, bool
);

impl<T: ZeroCopy, const N: usize, const ALIGN: usize, E: Endian> Validator
    for ZeroArray<T, N, ALIGN, E>
{
    /// Checks if the relative pointer is valid within the given buffer.
    fn is_valid(
        &self,
        buffer: &[u8],
    ) -> bool {
        self.get(buffer).is_some()
    }
}

impl<T: ZeroCopy + Validator, const N: usize> Validator for [T; N] {
    /// Checks if the relative pointer is valid within the given buffer.
    fn is_valid(
        &self,
        buffer: &[u8],
    ) -> bool {
        for item in self {
            if !item.is_valid(buffer) {
                return false;
            }
        }
        true
    }
}

/// A trait for deep validation of zero-copy structures, recursively checking all pointers.
pub trait DeepValidator: Validator {
    /// Performs a deep validation check on the relative pointer.
    fn is_valid_deep(
        &self,
        buffer: &[u8],
    ) -> bool;
}

macro_rules! impl_deep_validator_primitive {
    ($($t:ty),*) => {
        $(
            impl DeepValidator for $t {
                /// Performs a deep validation check on the relative pointer.
                fn is_valid_deep(&self, _buffer: &[u8]) -> bool {
                    true
                }
            }
        )*
    };
}

impl_deep_validator_primitive!(
    u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize, f32, f64, char, bool
);

impl<T: ZeroCopy + DeepValidator, const N: usize> DeepValidator for [T; N] {
    /// Performs a deep validation check on the relative pointer.
    fn is_valid_deep(
        &self,
        buffer: &[u8],
    ) -> bool {
        for item in self {
            if !item.is_valid_deep(buffer) {
                return false;
            }
        }
        true
    }
}

impl<T: ZeroCopy + DeepValidator, const ALIGN: usize, E: Endian> DeepValidator
    for RelativePtr<T, ALIGN, E>
{
    /// Performs a deep validation check on the relative pointer.
    fn is_valid_deep(
        &self,
        buffer: &[u8],
    ) -> bool {
        self.get(buffer)
            .is_some_and(|target| target.is_valid_deep(buffer))
    }
}

impl<const CAP: usize, E: Endian> DeepValidator for ZeroString<CAP, E> {
    /// Performs a deep validation check on the relative pointer.
    fn is_valid_deep(
        &self,
        _buffer: &[u8],
    ) -> bool {
        self.get().is_some()
    }
}

impl<T: ZeroCopy + DeepValidator, const ALIGN: usize, E: Endian> DeepValidator
    for ZeroSlice<T, ALIGN, E>
{
    /// Performs a deep validation check on the relative pointer.
    fn is_valid_deep(
        &self,
        buffer: &[u8],
    ) -> bool {
        if let Some(slice) = self.get(buffer) {
            for item in slice {
                if !item.is_valid_deep(buffer) {
                    return false;
                }
            }
            true
        } else {
            false
        }
    }
}

impl<E: Endian> DeepValidator for ZeroStr<E> {
    /// Performs a deep validation check on the relative pointer.
    fn is_valid_deep(
        &self,
        buffer: &[u8],
    ) -> bool {
        self.get(buffer).is_some()
    }
}

impl<T: ZeroCopy + DeepValidator, const N: usize, const ALIGN: usize, E: Endian> DeepValidator
    for ZeroArray<T, N, ALIGN, E>
{
    /// Performs a deep validation check on the relative pointer.
    fn is_valid_deep(
        &self,
        buffer: &[u8],
    ) -> bool {
        if let Some(arr) = self.get(buffer) {
            for item in arr {
                if !item.is_valid_deep(buffer) {
                    return false;
                }
            }
            true
        } else {
            false
        }
    }
}

// Bincode integration
use crate::de::BorrowDecode;
use crate::de::BorrowDecoder;
use crate::de::Decode;
use crate::de::Decoder;
use crate::enc::Encode;
use crate::enc::Encoder;
use crate::error::DecodeError;
use crate::error::EncodeError;

impl<T: StaticSize, const ALIGN: usize, E: Endian, Context> Decode<Context>
    for RelativePtr<T, ALIGN, E>
{
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        Ok(Self {
            offset: i32::decode(decoder)?,
            _marker: PhantomData,
        })
    }
}
impl<'de, T: StaticSize, const ALIGN: usize, E: Endian, Context> BorrowDecode<'de, Context>
    for RelativePtr<T, ALIGN, E>
{
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D
    ) -> Result<Self, DecodeError> {
        Ok(Self {
            offset: i32::borrow_decode(decoder)?,
            _marker: PhantomData,
        })
    }
}
impl<T: StaticSize, const ALIGN: usize, E: Endian> Encode for RelativePtr<T, ALIGN, E> {
    fn encode<EN: Encoder>(
        &self,
        encoder: &mut EN,
    ) -> Result<(), EncodeError> {
        self.offset.encode(encoder)
    }
}

impl<const CAP: usize, E: Endian, Context> Decode<Context> for ZeroString<CAP, E> {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        Ok(Self {
            len: u32::decode(decoder)?,
            data: Decode::decode(decoder)?,
            _marker: PhantomData,
        })
    }
}
impl<'de, const CAP: usize, E: Endian, Context> BorrowDecode<'de, Context> for ZeroString<CAP, E> {
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D
    ) -> Result<Self, DecodeError> {
        Ok(Self {
            len: u32::borrow_decode(decoder)?,
            data: BorrowDecode::borrow_decode(decoder)?,
            _marker: PhantomData,
        })
    }
}
impl<const CAP: usize, E: Endian> Encode for ZeroString<CAP, E> {
    fn encode<EN: Encoder>(
        &self,
        encoder: &mut EN,
    ) -> Result<(), EncodeError> {
        self.len.encode(encoder)?;
        self.data.encode(encoder)
    }
}

impl<T: StaticSize, const ALIGN: usize, E: Endian, Context> Decode<Context>
    for ZeroSlice<T, ALIGN, E>
{
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        Ok(Self {
            len: u32::decode(decoder)?,
            ptr: RelativePtr::decode(decoder)?,
        })
    }
}
impl<'de, T: StaticSize, const ALIGN: usize, E: Endian, Context> BorrowDecode<'de, Context>
    for ZeroSlice<T, ALIGN, E>
{
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D
    ) -> Result<Self, DecodeError> {
        Ok(Self {
            len: u32::borrow_decode(decoder)?,
            ptr: RelativePtr::borrow_decode(decoder)?,
        })
    }
}
impl<T: StaticSize, const ALIGN: usize, E: Endian> Encode for ZeroSlice<T, ALIGN, E> {
    fn encode<EN: Encoder>(
        &self,
        encoder: &mut EN,
    ) -> Result<(), EncodeError> {
        self.len.encode(encoder)?;
        self.ptr.encode(encoder)
    }
}

impl<E: Endian, Context> Decode<Context> for ZeroStr<E> {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        Ok(Self {
            slice: ZeroSlice::decode(decoder)?,
        })
    }
}
impl<'de, E: Endian, Context> BorrowDecode<'de, Context> for ZeroStr<E> {
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D
    ) -> Result<Self, DecodeError> {
        Ok(Self {
            slice: ZeroSlice::borrow_decode(decoder)?,
        })
    }
}
impl<E: Endian> Encode for ZeroStr<E> {
    fn encode<EN: Encoder>(
        &self,
        encoder: &mut EN,
    ) -> Result<(), EncodeError> {
        self.slice.encode(encoder)
    }
}

/// An aligned byte buffer that correctly deallocates with the alignment it was allocated with.
///
/// This is necessary because `Vec<u8>` always deallocates with alignment 1, which is UB
/// if the memory was allocated with a higher alignment.
#[cfg(feature = "alloc")]
#[doc(hidden)]
pub struct AlignedBuffer {
    ptr: *mut u8,
    len: usize,
    align: usize,
}

#[cfg(feature = "alloc")]
impl AlignedBuffer {
    fn from_vec(
        data: &[u8],
        align: usize,
    ) -> Self {
        let len = data.len();
        if len == 0 {
            return Self {
                ptr: core::ptr::NonNull::dangling().as_ptr(),
                len: 0,
                align,
            };
        }
        let layout = alloc::alloc::Layout::from_size_align(len, align).expect("Invalid layout");
        unsafe {
            let ptr = alloc::alloc::alloc(layout);
            if ptr.is_null() {
                alloc::alloc::handle_alloc_error(layout);
            }
            core::ptr::copy_nonoverlapping(data.as_ptr(), ptr, len);
            Self { ptr, len, align }
        }
    }
}

#[cfg(feature = "alloc")]
impl core::ops::Deref for AlignedBuffer {
    /// The target type that this builds into.
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        if self.len == 0 {
            return &[];
        }
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }
}

#[cfg(feature = "alloc")]
impl AsRef<[u8]> for AlignedBuffer {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

#[cfg(feature = "alloc")]
impl Drop for AlignedBuffer {
    fn drop(&mut self) {
        if self.len > 0 {
            let layout = alloc::alloc::Layout::from_size_align(self.len, self.align)
                .expect("Invalid layout");
            unsafe {
                alloc::alloc::dealloc(self.ptr, layout);
            }
        }
    }
}

/// Computes a relative offset and checks that it fits in an `i32`.
/// Panics if the distance between `from` and `to` exceeds the i32 range (~2GB).
#[cfg(feature = "alloc")]
fn checked_relative_offset(
    to: usize,
    from: usize,
) -> i32 {
    let diff = to as isize - from as isize;
    i32::try_from(diff).unwrap_or_else(|_| {
        panic!(
            "Relative offset overflow: distance {diff} between positions {from} and {to} exceeds i32 range"
        )
    })
}

/// A builder for zero-copy structures.
#[cfg(feature = "alloc")]
pub struct ZeroBuilder {
    data: Vec<u8>,
    max_align: usize,
}

#[cfg(feature = "alloc")]
impl Default for ZeroBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "alloc")]
impl ZeroBuilder {
    /// Creates a new instance.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            data: Vec::new(),
            max_align: 1,
        }
    }

    /// Aligns the builder to the specified boundary.
    pub fn align(
        &mut self,
        align: usize,
    ) -> usize {
        if align > self.max_align {
            self.max_align = align;
        }
        if align > 1 {
            let padding = (align - (self.data.len() % align)) % align;
            if padding > 0 {
                self.data.resize(self.data.len() + padding, 0);
            }
        }
        self.data.len()
    }

    /// Reserves space for a value of type T.
    pub fn reserve<T: ZeroCopy>(&mut self) -> usize {
        let offset = self.align(T::ALIGN);
        self.data.resize(offset + T::SIZE, 0);
        offset
    }

    /// Reserves a specific number of bytes with alignment.
    pub fn reserve_bytes(
        &mut self,
        size: usize,
        align: usize,
    ) -> usize {
        let offset = self.align(align);
        self.data.resize(offset + size, 0);
        offset
    }

    /// Writes a value to the specified offset.
    pub fn write<T: ZeroCopy>(
        &mut self,
        offset: usize,
        val: &T,
    ) {
        let size = T::SIZE;
        let bytes =
            unsafe { core::slice::from_raw_parts(core::ptr::from_ref(val).cast::<u8>(), size) };
        self.data[offset..offset + size].copy_from_slice(bytes);
    }

    /// Pushes a value onto the builder and returns its offset.
    pub fn push<T: ZeroCopy>(
        &mut self,
        val: &T,
    ) -> usize {
        let offset = self.reserve::<T>();
        self.write(offset, val);
        offset
    }

    /// Pushes raw bytes onto the builder.
    pub fn push_bytes(
        &mut self,
        bytes: &[u8],
        align: usize,
    ) -> usize {
        let offset = self.align(align);
        self.data.extend_from_slice(bytes);
        offset
    }

    /// Finalizes the builder and returns the `AlignedBuffer`.
    #[must_use]
    pub fn finish(self) -> AlignedBuffer {
        AlignedBuffer::from_vec(&self.data, self.max_align)
    }

    /// Returns the current length of the buffer.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.data.len()
    }

    /// Returns true if the buffer is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

/// A trait for zero-copy types that defines their preferred builder type.
pub trait ZeroCopyType<E: Endian = NativeEndian>: ZeroCopy {
    /// The builder type associated with this trait.
    type Builder;
}

/// A trait for types that can be built into a zero-copy structure.
pub trait ZeroCopyBuilder<E: Endian = NativeEndian, const ALIGN: usize = 0> {
    /// The target type that this builds into.
    type Target: ZeroCopy;

    #[cfg(feature = "alloc")]
    /// Build the zero-copy type into the builder.
    fn build_to_target(
        self,
        builder: &mut ZeroBuilder,
        offset: usize,
    ) -> Self::Target;

    #[cfg(feature = "alloc")]
    /// Build the zero-copy type into the builder and return its offset.
    fn build(
        self,
        builder: &mut ZeroBuilder,
    ) -> usize
    where
        Self: Sized,
    {
        // Use the target's natural alignment if ALIGN is 0, otherwise use ALIGN
        let alignment = if ALIGN == 0 {
            <Self::Target as ZeroCopy>::ALIGN
        } else {
            ALIGN
        };
        let offset = builder.align(alignment);
        builder
            .data
            .resize(offset + <Self::Target as StaticSize>::SIZE, 0);
        let target = self.build_to_target(builder, offset);
        builder.write(offset, &target);
        offset
    }
}

/// A wrapper for builders that produce a `RelativePtr`.
#[cfg(feature = "alloc")]
#[doc(hidden)]
pub struct RelativeBuilder<B, const ALIGN: usize>(pub B);

#[cfg(feature = "alloc")]
impl<E: Endian, T, B, const ALIGN: usize> ZeroCopyBuilder<E, ALIGN> for RelativeBuilder<B, ALIGN>
where
    B: ZeroCopyBuilder<E, 0, Target = T>,
    T: ZeroCopy,
{
    /// The target type that this builds into.
    type Target = RelativePtr<T, ALIGN, E>;

    /// Builds the object into the target location.
    fn build_to_target(
        self,
        builder: &mut ZeroBuilder,
        offset: usize,
    ) -> Self::Target {
        let target_offset = self.0.build(builder);
        RelativePtr::new(checked_relative_offset(target_offset, offset))
    }
}

/// A wrapper for builders that produce a `ZeroArray`.
#[cfg(feature = "alloc")]
#[doc(hidden)]
pub struct ArrayBuilder<B, const N: usize, const ALIGN: usize>(pub [B; N]);

#[cfg(feature = "alloc")]
impl<E: Endian, T, B, const N: usize, const ALIGN: usize> ZeroCopyBuilder<E, ALIGN>
    for ArrayBuilder<B, N, ALIGN>
where
    B: ZeroCopyBuilder<E, 0, Target = T>,
    T: ZeroCopy,
{
    /// The target type that this builds into.
    type Target = ZeroArray<T, N, ALIGN, E>;

    /// Builds the object into the target location.
    fn build_to_target(
        self,
        builder: &mut ZeroBuilder,
        offset: usize,
    ) -> Self::Target {
        let effective_align = if ALIGN == 0 {
            T::ALIGN
        } else {
            ALIGN
        };
        let data_offset = builder.reserve_bytes(N * T::SIZE, effective_align);
        for (i, item) in self.0.into_iter().enumerate() {
            let item_offset = data_offset + i * T::SIZE;
            let target = item.build_to_target(builder, item_offset);
            builder.write(item_offset, &target);
        }
        ZeroArray {
            ptr: RelativePtr::new(checked_relative_offset(data_offset, offset)),
        }
    }
}

/// A wrapper for builders that produce a `ZeroSlice`.
#[cfg(feature = "alloc")]
#[doc(hidden)]
pub struct SliceBuilder<B, const ALIGN: usize>(pub Vec<B>);

#[cfg(feature = "alloc")]
impl<E: Endian, T, B, const ALIGN: usize> ZeroCopyBuilder<E, ALIGN> for SliceBuilder<B, ALIGN>
where
    B: ZeroCopyBuilder<E, 0, Target = T>,
    T: ZeroCopy,
{
    /// The target type that this builds into.
    type Target = ZeroSlice<T, ALIGN, E>;

    /// Builds the object into the target location.
    fn build_to_target(
        self,
        builder: &mut ZeroBuilder,
        offset: usize,
    ) -> Self::Target {
        let len = self.0.len() as u32;
        let effective_align = if ALIGN == 0 {
            T::ALIGN
        } else {
            ALIGN
        };
        let data_offset = builder.reserve_bytes(self.0.len() * T::SIZE, effective_align);
        for (i, item) in self.0.into_iter().enumerate() {
            let item_offset = data_offset + i * T::SIZE;
            let target = item.build_to_target(builder, item_offset);
            builder.write(item_offset, &target);
        }
        // RelativePtr is at offset + size_of::<u32>() (after the `len` field)
        let ptr_field_offset = offset + core::mem::size_of::<u32>();
        ZeroSlice::new(len, checked_relative_offset(data_offset, ptr_field_offset))
    }
}

macro_rules! impl_zerocopy_primitive {
    ($($t:ty, $from_native:ident),*) => {
        $(
            impl<E: Endian> ZeroCopyType<E> for $t {
                /// The builder type associated with this trait.
                type Builder = $t;
            }
            impl<E: Endian> ZeroCopyBuilder<E, 0> for $t {
                /// The target type that this builds into.
                type Target = $t;
                #[cfg(feature = "alloc")]
                fn build_to_target(self, _builder: &mut ZeroBuilder, _offset: usize) -> Self::Target {
                    E::$from_native(self)
                }
            }
        )*
    };
}

impl_zerocopy_primitive!(
    u16,
    from_native_u16,
    u32,
    from_native_u32,
    u64,
    from_native_u64,
    u128,
    from_native_u128,
    i16,
    from_native_i16,
    i32,
    from_native_i32,
    i64,
    from_native_i64,
    i128,
    from_native_i128,
    f32,
    from_native_f32,
    f64,
    from_native_f64
);

impl<E: Endian> ZeroCopyType<E> for u8 {
    /// The builder type associated with this trait.
    type Builder = Self;
}
impl<E: Endian> ZeroCopyBuilder<E, 0> for u8 {
    /// The target type that this builds into.
    type Target = Self;

    #[cfg(feature = "alloc")]
    fn build_to_target(
        self,
        _builder: &mut ZeroBuilder,
        _offset: usize,
    ) -> Self::Target {
        self
    }
}
impl<E: Endian> ZeroCopyType<E> for i8 {
    /// The builder type associated with this trait.
    type Builder = Self;
}
impl<E: Endian> ZeroCopyBuilder<E, 0> for i8 {
    /// The target type that this builds into.
    type Target = Self;

    #[cfg(feature = "alloc")]
    fn build_to_target(
        self,
        _builder: &mut ZeroBuilder,
        _offset: usize,
    ) -> Self::Target {
        self
    }
}
impl<E: Endian> ZeroCopyType<E> for bool {
    /// The builder type associated with this trait.
    type Builder = Self;
}
impl<E: Endian> ZeroCopyBuilder<E, 0> for bool {
    /// The target type that this builds into.
    type Target = Self;

    #[cfg(feature = "alloc")]
    fn build_to_target(
        self,
        _builder: &mut ZeroBuilder,
        _offset: usize,
    ) -> Self::Target {
        self
    }
}
impl<E: Endian> ZeroCopyType<E> for char {
    /// The builder type associated with this trait.
    type Builder = Self;
}
impl<E: Endian> ZeroCopyBuilder<E, 0> for char {
    /// The target type that this builds into.
    type Target = Self;

    #[cfg(feature = "alloc")]
    fn build_to_target(
        self,
        _builder: &mut ZeroBuilder,
        _offset: usize,
    ) -> Self::Target {
        Self::from_u32(E::from_native_u32(self as u32)).unwrap()
    }
}

#[cfg(feature = "alloc")]
impl<E: Endian> ZeroCopyType<E> for ZeroStr<E> {
    /// The builder type associated with this trait.
    type Builder = String;
}
#[cfg(feature = "alloc")]
impl<E: Endian> ZeroCopyBuilder<E, 0> for String {
    /// The target type that this builds into.
    type Target = ZeroStr<E>;

    /// Builds the object into the target location.
    fn build_to_target(
        self,
        builder: &mut ZeroBuilder,
        offset: usize,
    ) -> Self::Target {
        let alignment = 1;
        let data_offset = builder.push_bytes(self.as_bytes(), alignment);
        // The RelativePtr inside ZeroStr is at offset + size_of::<u32>() (after the `len` field).
        // RelativePtr::get() resolves relative to its own address, so we must adjust.
        let ptr_field_offset = offset + core::mem::size_of::<u32>();
        ZeroStr::new(
            self.len() as u32,
            checked_relative_offset(data_offset, ptr_field_offset),
        )
    }
}

/// A wrapper for `String` to build into an inline `ZeroString<CAP>`.
#[cfg(feature = "alloc")]
#[doc(hidden)]
pub struct FixedString<const CAP: usize>(pub String);

#[cfg(feature = "alloc")]
impl<E: Endian, const CAP: usize> ZeroCopyType<E> for ZeroString<CAP, E> {
    /// The builder type associated with this trait.
    type Builder = FixedString<CAP>;
}
#[cfg(feature = "alloc")]
impl<E: Endian, const CAP: usize> ZeroCopyBuilder<E, 0> for FixedString<CAP> {
    /// The target type that this builds into.
    type Target = ZeroString<CAP, E>;

    /// Builds the object into the target location.
    fn build_to_target(
        self,
        _builder: &mut ZeroBuilder,
        _offset: usize,
    ) -> Self::Target {
        let bytes = self.0.as_bytes();
        let mut len = bytes.len().min(CAP);
        // Ensure truncation happens at a valid UTF-8 boundary
        while len > 0 && !self.0.is_char_boundary(len) {
            len -= 1;
        }
        let mut data = [0u8; CAP];
        data[..len].copy_from_slice(&bytes[..len]);
        ZeroString {
            len: E::from_native_u32(len as u32),
            data,
            _marker: PhantomData,
        }
    }
}

#[cfg(feature = "alloc")]
impl<E: Endian, T, const ALIGN: usize> ZeroCopyType<E> for ZeroSlice<T, ALIGN, E>
where
    T: ZeroCopy + ZeroCopyType<E>,
{
    /// The builder type associated with this trait.
    type Builder = SliceBuilder<T::Builder, ALIGN>;
}
#[cfg(feature = "alloc")]
impl<E: Endian, T, B, const ALIGN: usize> ZeroCopyBuilder<E, ALIGN> for Vec<B>
where
    B: ZeroCopyBuilder<E, 0, Target = T>,
    T: ZeroCopy,
{
    /// The target type that this builds into.
    type Target = ZeroSlice<T, ALIGN, E>;

    /// Builds the object into the target location.
    fn build_to_target(
        self,
        builder: &mut ZeroBuilder,
        offset: usize,
    ) -> Self::Target {
        let len = self.len() as u32;
        let effective_align = if ALIGN == 0 {
            T::ALIGN
        } else {
            ALIGN
        };
        let data_offset = builder.reserve_bytes(self.len() * T::SIZE, effective_align);
        for (i, item) in self.into_iter().enumerate() {
            let item_offset = data_offset + i * T::SIZE;
            let target = item.build_to_target(builder, item_offset);
            builder.write(item_offset, &target);
        }
        // RelativePtr is at offset + size_of::<u32>() (after the `len` field)
        let ptr_field_offset = offset + core::mem::size_of::<u32>();
        ZeroSlice::new(len, checked_relative_offset(data_offset, ptr_field_offset))
    }
}
#[cfg(feature = "alloc")]
impl<E: Endian, T, const ALIGN: usize> ZeroCopyType<E> for RelativePtr<T, ALIGN, E>
where
    T: ZeroCopy + ZeroCopyType<E>,
{
    /// The builder type associated with this trait.
    type Builder = RelativeBuilder<T::Builder, ALIGN>;
}

#[cfg(feature = "alloc")]
impl<E: Endian, T, const N: usize, const ALIGN: usize> ZeroCopyType<E> for ZeroArray<T, N, ALIGN, E>
where
    T: ZeroCopy + ZeroCopyType<E>,
{
    /// The builder type associated with this trait.
    type Builder = ArrayBuilder<T::Builder, N, ALIGN>;
}

impl<E: Endian, T, const N: usize> ZeroCopyType<E> for [T; N]
where
    T: ZeroCopy + ZeroCopyType<E>,
{
    /// The builder type associated with this trait.
    type Builder = [<T as ZeroCopyType<E>>::Builder; N];
}
impl<E: Endian, T, B, const N: usize> ZeroCopyBuilder<E, 0> for [B; N]
where
    B: ZeroCopyBuilder<E, 0, Target = T>,
    T: ZeroCopy,
{
    /// The target type that this builds into.
    type Target = [T; N];

    #[cfg(feature = "alloc")]
    fn build_to_target(
        self,
        builder: &mut ZeroBuilder,
        offset: usize,
    ) -> Self::Target {
        let mut target_array: [core::mem::MaybeUninit<T>; N] =
            unsafe { core::mem::MaybeUninit::uninit().assume_init() };
        for (i, item) in self.into_iter().enumerate() {
            let item_offset = offset + i * T::SIZE;
            target_array[i] =
                core::mem::MaybeUninit::new(item.build_to_target(builder, item_offset));
        }
        unsafe { core::mem::transmute_copy(&target_array) }
    }
}
