//! Static size trait for compile-time memory bound validation.

/// A trait that calculates the upper bound of a type's serialized size as a `const` value.
pub trait StaticSize {
    /// The maximum possible serialized size in bytes (no bit-packing).
    const MAX_SIZE: usize;
    /// The maximum possible serialized size in bytes when bit-packing is enabled.
    ///
    /// For types with `#[bincode(bits = N)]` fields this is typically smaller than
    /// `MAX_SIZE` because consecutive bit-packed fields share bytes.  Types that
    /// have no bit-packed fields default to `MAX_SIZE`.
    const PACKED_MAX_SIZE: usize = Self::MAX_SIZE;
}

// Primitives
impl StaticSize for bool {
    const MAX_SIZE: usize = 1;
}
impl StaticSize for u8 {
    const MAX_SIZE: usize = 1;
}
impl StaticSize for i8 {
    const MAX_SIZE: usize = 1;
}

impl StaticSize for u16 {
    const MAX_SIZE: usize = 3;
}
impl StaticSize for i16 {
    const MAX_SIZE: usize = 3;
}
impl StaticSize for u32 {
    const MAX_SIZE: usize = 5;
}
impl StaticSize for i32 {
    const MAX_SIZE: usize = 5;
}
impl StaticSize for u64 {
    const MAX_SIZE: usize = 9;
}
impl StaticSize for i64 {
    const MAX_SIZE: usize = 9;
}
impl StaticSize for u128 {
    const MAX_SIZE: usize = 17;
}
impl StaticSize for i128 {
    const MAX_SIZE: usize = 17;
}
impl StaticSize for usize {
    const MAX_SIZE: Self = 9;
}
impl StaticSize for isize {
    const MAX_SIZE: usize = 9;
}

impl StaticSize for f32 {
    const MAX_SIZE: usize = 4;
}
impl StaticSize for f64 {
    const MAX_SIZE: usize = 8;
}
impl StaticSize for char {
    const MAX_SIZE: usize = 4;
}
impl StaticSize for () {
    const MAX_SIZE: usize = 0;
}

impl<T> StaticSize for core::marker::PhantomData<T> {
    const MAX_SIZE: usize = 0;
}

// Arrays
impl<T: StaticSize, const N: usize> StaticSize for [T; N] {
    const MAX_SIZE: usize = T::MAX_SIZE * N;
}

// Option
impl<T: StaticSize> StaticSize for Option<T> {
    const MAX_SIZE: usize = 1 + T::MAX_SIZE;
}

// Result
impl<T: StaticSize, E: StaticSize> StaticSize for Result<T, E> {
    const MAX_SIZE: usize = 5
        + (if T::MAX_SIZE > E::MAX_SIZE {
            T::MAX_SIZE
        } else {
            E::MAX_SIZE
        });
}

// Tuples
macro_rules! impl_static_size_tuple {
    ($($name:ident),*) => {
        impl<$($name: StaticSize),*> StaticSize for ($($name,)*) {
            const MAX_SIZE: usize = 0 $(+ $name::MAX_SIZE)*;
        }
    }
}

impl_static_size_tuple!(A);
impl_static_size_tuple!(A, B);
impl_static_size_tuple!(A, B, C);
impl_static_size_tuple!(A, B, C, D);
impl_static_size_tuple!(A, B, C, D, E);
impl_static_size_tuple!(A, B, C, D, E, F);
impl_static_size_tuple!(A, B, C, D, E, F, G);
impl_static_size_tuple!(A, B, C, D, E, F, G, H);
impl_static_size_tuple!(A, B, C, D, E, F, G, H, I);
impl_static_size_tuple!(A, B, C, D, E, F, G, H, I, J);
impl_static_size_tuple!(A, B, C, D, E, F, G, H, I, J, K);
impl_static_size_tuple!(A, B, C, D, E, F, G, H, I, J, K, L);

/// Helper constants for common bincode structures
#[doc(hidden)]
pub mod helpers {
    /// The maximum bytes a `usize` (length) can take when encoded as a varint.
    pub const VARINT_MAX_64: usize = 9;
    /// The maximum bytes a `u32` can take when encoded as a varint.
    pub const VARINT_MAX_32: usize = 5;
}
