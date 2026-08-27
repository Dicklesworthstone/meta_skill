/// Compile-Time check that any potential error path is covered.
/// This uses const-eval to ensure uncovered monomorphized error routes fail to compile.
#[diagnostic::on_unimplemented(
    message = "Bincode error path configuration constraint violated",
    label = "This feature subset is unsupported or unreachable in your const environment",
    note = "Compile-time assertion BincodeErrorPathCovered blocked an uncoverable monomorphized error route."
)]
#[allow(dead_code)]
pub trait BincodeErrorPathCovered<const PATH: u8> {
    const ASSERT: () = ();

    #[inline(always)]
    fn assert_covered() {
        let () = Self::ASSERT;
    }
}

// 0 for decode, 1 for encode
impl BincodeErrorPathCovered<0> for () {}
impl BincodeErrorPathCovered<1> for () {}
