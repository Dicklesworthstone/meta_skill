#![cfg(feature = "derive")]
#![allow(dead_code)]

#[cfg(feature = "zero-copy")]
use bincode_next::ZeroCopy;

#[cfg(feature = "zero-copy")]
#[repr(C)]
#[derive(ZeroCopy)]
struct ShouldPassC {
    a: u32,
}

#[cfg(feature = "zero-copy")]
#[repr(transparent)]
#[derive(ZeroCopy)]
struct ShouldPassTransparent {
    a: u32,
}

#[test]
fn test_repr_checks() {
    // This just ensures the file compiles with correct reprs
}
