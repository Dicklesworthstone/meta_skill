pub trait Sealed {}

impl<T> Sealed for &mut T where T: Sealed {}

/// A helper trait to perform compile-time checks on bit-packed fields.
pub trait BitPackedCheck<const BITS: u8> {
    /// Performs the check.
    const CHECK: ();
}

impl<const BITS: u8, T> BitPackedCheck<BITS> for T {
    const CHECK: () = assert!(
        BITS > 0 && BITS <= 64 && BITS as usize <= core::mem::size_of::<Self>() * 8,
        "Bit width must be 1-64 and not exceed type size"
    );
}

/// A trait to indicate that a type is a bincode primitive.
/// This is used for bulk copy optimizations when the configuration allows it.
pub trait IsPrimitive: Copy + 'static {}

impl IsPrimitive for u8 {}
impl IsPrimitive for i8 {}
impl IsPrimitive for u16 {}
impl IsPrimitive for i16 {}
impl IsPrimitive for u32 {}
impl IsPrimitive for i32 {}
impl IsPrimitive for u64 {}
impl IsPrimitive for i64 {}
impl IsPrimitive for u128 {}
impl IsPrimitive for i128 {}
impl IsPrimitive for f32 {}
impl IsPrimitive for f64 {}

#[doc(hidden)]
#[macro_export]
macro_rules! assume {
    ($cond:expr) => {
        if !$cond {
            debug_assert!(false, "Assumption failed: {}", stringify!($cond));
            unsafe { core::hint::unreachable_unchecked() };
        }
    };
}

pub use assume;

#[doc(hidden)]
#[macro_export]
macro_rules! is_likely {
    ($e:expr) => {{
        #[cfg(all(is_nightly, feature = "std"))]
        {
            std::intrinsics::likely($e)
        }
        #[cfg(not(all(is_nightly, feature = "std")))]
        {
            // Fallback for stable
            $e
        }
    }};
}

pub use is_likely;

#[doc(hidden)]
#[macro_export]
macro_rules! is_unlikely {
    ($e:expr) => {{
        #[cfg(all(is_nightly, feature = "std"))]
        {
            std::intrinsics::unlikely($e)
        }
        #[cfg(not(all(is_nightly, feature = "std")))]
        {
            // Fallback for stable
            $e
        }
    }};
}

pub use is_unlikely;
