//! Mostly safe no_std wrapper for `alloca` in Rust.
//!
//! This crate uses Rust lifetime system to ensure that stack allocated memory will not be used
//! after function return, but it does not make any guarantee about memory that is turned into
//! raw pointer and stored somewhere else.
//!
//! ### Example
//! ```rust
//! // allocate 128 bytes on the stack
//! alloca::with_alloca(128, |memory| {
//!     // memory: &mut [MaybeUninit<u8>]
//!     assert_eq!(memory.len(), 128);
//! });
//! ```

#![no_std]

use core::{
    ffi::c_void,
    mem::{self, ManuallyDrop, MaybeUninit},
    ptr, slice,
};

type Callback = unsafe extern "C-unwind" fn(ptr: *mut MaybeUninit<u8>, data: *mut c_void);

extern "C-unwind" {
    fn c_with_alloca(size: usize, callback: Callback, data: *mut c_void);
}

#[inline(always)]
fn get_trampoline<F: FnOnce(*mut MaybeUninit<u8>)>(_closure: &F) -> Callback {
    trampoline::<F>
}

unsafe extern "C-unwind" fn trampoline<F: FnOnce(*mut MaybeUninit<u8>)>(
    ptr: *mut MaybeUninit<u8>,
    data: *mut c_void,
) {
    let f = ManuallyDrop::take(&mut *(data as *mut ManuallyDrop<F>));
    f(ptr);
}

/// Allocates `[u8; size]` memory on stack and invokes `closure` with this slice as argument.
///
/// # Safety
/// This function is safe because `c_with_alloca` (which is internally used) will always returns non-null
/// pointer.
///
/// # Potential segfaults or UB
///
/// When using this function in wrong way your program might get UB or segfault "for free":
/// - Using memory allocated by `with_alloca` outside of it e.g closure is already returned but you somehow
/// managed to store pointer to memory and use it.
/// - Allocating more memory than thread stack size.
///
///   This will trigger segfault on stack overflow.
pub fn with_alloca<R>(size: usize, f: impl FnOnce(&mut [MaybeUninit<u8>]) -> R) -> R {
    let mut ret = MaybeUninit::uninit();

    let closure = |ptr| {
        let slice = unsafe { slice::from_raw_parts_mut(ptr, size) };
        ret.write(f(slice));
    };

    let trampoline = get_trampoline(&closure);
    let mut closure_data = ManuallyDrop::new(closure);

    unsafe {
        c_with_alloca(size, trampoline, &mut closure_data as *mut _ as *mut c_void);
        ret.assume_init()
    }
}

/// Same as `with_alloca` except it zeroes memory slice.
pub fn with_alloca_zeroed<R>(size: usize, f: impl FnOnce(&mut [u8]) -> R) -> R {
    with_alloca(size, |memory| unsafe {
        ptr::write_bytes(memory.as_mut_ptr(), 0, size);
        f(mem::transmute(memory))
    })
}

/// Allocates `T` on stack space.
pub fn alloca<T, R>(f: impl FnOnce(&mut MaybeUninit<T>) -> R) -> R {
    use mem::{align_of, size_of};

    with_alloca(size_of::<T>() + (align_of::<T>() - 1), |memory| unsafe {
        let mut raw_memory = memory.as_mut_ptr();
        if raw_memory as usize % align_of::<T>() != 0 {
            raw_memory = raw_memory.add(align_of::<T>() - raw_memory as usize % align_of::<T>());
        }
        f(&mut *raw_memory.cast::<MaybeUninit<T>>())
    })
}

#[cfg(test)]
mod tests;
