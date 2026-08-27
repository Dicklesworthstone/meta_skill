#![no_std]
//! A crate that allows you to untype your types.
//!
//! This provides 2 functions:
//!
//! [`type_equal`] allows you to check if two types are the same.
//!
//! [`unty`] allows you to downcast a generic type into a concrete type.
//!
//! This is mostly useful for generic functions, e.g.
//!
//! ```
//! # use unty::*;
//! pub fn foo<S>(s: S) {
//!     if let Ok(a) = unsafe { unty::<S, u8>(s) } {
//!         println!("It is an u8 with value {a}");
//!     } else {
//!         println!("it is not an u8");
//!     }
//! }
//! foo(10u8); // will print "it is an u8"
//! foo("test"); // will print "it is not an u8"
//! ```
//!
//! Note that both of these functions may give false positives if both types have lifetimes. There currently is not a way to prevent this. See [`type_equal`] for more information.
//!
//! ```no_run
//! # fn foo<'a>(input: &'a str) {
//! # use unty::*;
//! assert!(type_equal::<&'a str, &'static str>()); // these are not actually the same
//! if let Ok(str) = unsafe { unty::<&'a str, &'static str>(input) } {
//!     // this will extend the &'a str lifetime to be &'static, which is not allowed.
//!     // the compiler may now light your PC on fire.
//! }
//! # }
//! ```

// =========================================================================
// RUST LINT CONFIGURATION: bincode-next -- virtue
// =========================================================================

// -------------------------------------------------------------------------
// LEVEL 1: CRITICAL ERRORS (Deny)
// -------------------------------------------------------------------------
#![deny(
    // Rust Compiler Errors
    dead_code,
    unreachable_code,
    improper_ctypes_definitions,
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms,
    clippy::perf,
    clippy::correctness,
    clippy::suspicious,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::missing_safety_doc,
    clippy::same_item_push,
    clippy::implicit_clone,
    clippy::all,
    clippy::pedantic,
    warnings,
    missing_docs,
    clippy::nursery,
    clippy::single_call_fn,
)]
// -------------------------------------------------------------------------
// LEVEL 2: STYLE WARNINGS (Warn)
// -------------------------------------------------------------------------
#![warn(clippy::dbg_macro, clippy::todo, clippy::unnecessary_safety_comment)]
// -------------------------------------------------------------------------
// LEVEL 3: ALLOW/IGNORABLE (Allow)
// -------------------------------------------------------------------------
#![allow(
    unsafe_code,
    clippy::restriction,
    clippy::inline_always,
    unused_doc_comments,
    clippy::empty_line_after_doc_comments
)]

use core::{any::TypeId, marker::PhantomData, mem};

/// Untypes your types. For documentation see the root of this crate.
///
/// # Safety
///
/// This should not be used with types with lifetimes.
///
/// # Errors
///
/// Returns an error with Result type if the types are not equal.
pub unsafe fn unty<Src, Target: 'static>(x: Src) -> Result<Target, Src> {
    unsafe {
        if type_equal::<Src, Target>() {
            let x = mem::ManuallyDrop::new(x);
            Ok(mem::transmute_copy::<Src, Target>(&x))
        } else {
            Err(x)
        }
    }
}

/// Checks to see if the two types are equal.
///
/// ```
/// # use unty::type_equal;
/// assert!(type_equal::<u8, u8>());
/// assert!(!type_equal::<u8, u16>());
///
/// fn is_string<T>(_t: T) -> bool {
///     type_equal::<T, String>()
/// }
///
/// assert!(is_string(String::new()));
/// assert!(!is_string("")); // `&'static str`, not `String`
/// ```
///
/// Note that this may give false positives if both of the types have lifetimes. Currently it is not possible to determine the difference between `&'a str` and `&'b str`.
///
/// ```
/// # use unty::type_equal;
/// # fn foo<'a, 'b>(a: &'a str, b: &'b str) {
/// assert!(type_equal::<&'a str, &'b str>()); // actual different types, this is not safe to transmute
/// # }
/// # foo("", "");
/// ```
#[must_use]
pub fn type_equal<Src: ?Sized, Target: ?Sized>() -> bool {
    non_static_type_id::<Src>() == non_static_type_id::<Target>()
}

// Code by dtolnay in a bincode issue:
// https://github.com/bincode-org/bincode/issues/665#issue-1903241159
fn non_static_type_id<T: ?Sized>() -> TypeId {
    trait NonStaticAny {
        fn get_type_id(&self) -> TypeId
        where
            Self: 'static;
    }

    impl<T: ?Sized> NonStaticAny for PhantomData<T> {
        fn get_type_id(&self) -> TypeId
        where
            Self: 'static,
        {
            TypeId::of::<T>()
        }
    }

    let phantom_data = PhantomData::<T>;
    NonStaticAny::get_type_id(unsafe {
        mem::transmute::<&dyn NonStaticAny, &(dyn NonStaticAny + 'static)>(&phantom_data)
    })
}

#[test]
fn test_double_drop() {
    use core::sync::atomic::{AtomicUsize, Ordering};
    #[derive(Debug)]
    struct Ty;
    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    impl Drop for Ty {
        fn drop(&mut self) {
            COUNTER.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn foo<T: core::fmt::Debug>(t: T) {
        unsafe { unty::<T, Ty>(t) }.unwrap();
    }

    foo(Ty);
    assert_eq!(COUNTER.load(Ordering::Relaxed), 1);
}
