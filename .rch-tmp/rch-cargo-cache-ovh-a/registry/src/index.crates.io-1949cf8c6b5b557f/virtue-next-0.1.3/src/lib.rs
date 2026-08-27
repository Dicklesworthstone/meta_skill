//! # Virtue, a sinless derive macro helper
//!
//! ## Goals
//!
//! - Zero dependencies, so fast compile times
//! - No other dependencies needed
//! - Declarative code generation
//! - As much typesystem checking as possible
//! - Build for modern rust: 1.57 and up
//! - Build with popular crates in mind:
//!   - [bincode](https://docs.rs/bincode)
//! - Will always respect semver. Minor releases will never have:
//!   - Breaking API changes
//!   - MSRV changes
//!
//! ## Example
//!
//! First, add this to your Cargo.toml:
//! ```toml
//! [lib]
//! proc-macro = true
//! ```
//!
//! Then instantiate your project with:
//!
//! ```ignore
//! use virtue::prelude::*;
//!
//! #[proc_macro_derive(RetHi)] // change this to change your #[derive(...)] name
//! pub fn derive_ret_hi(input: TokenStream) -> TokenStream {
//!     derive_ret_hi_inner(input).unwrap_or_else(|error| error.into_token_stream())
//! }
//!
//! fn derive_ret_hi_inner(input: TokenStream) -> Result<TokenStream> {
//!     let parse = Parse::new(input)?;
//!     let (mut generator, _attributes, _body) = parse.into_generator();
//!     generator
//!         .generate_impl()
//!         .generate_fn("hi")
//!         .with_self_arg(FnSelfArg::RefSelf)
//!         .with_return_type("&'static str")
//!         .body(|body| {
//!             body.lit_str("hi");
//!             Ok(())
//!         })?;
//!     generator.finish()
//! }
//! ```
//!
//! You can invoke this with
//!
//! ```ignore
//! #[derive(RetHi)]
//! struct Foo;
//!
//! fn main() {
//!     println!("{}", Foo.hi());
//! }
//! ```
//!
//! The generated code is:
//!
//! ```ignore
//! impl Foo {
//!     fn hi(&self) -> &'static str {
//!         "hi"
//!     }
//! }
//! ```
#![warn(missing_docs)]
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
#![warn(
    unsafe_code,
    clippy::dbg_macro,
    clippy::todo,
    clippy::unnecessary_safety_comment
)]
// -------------------------------------------------------------------------
// LEVEL 3: ALLOW/IGNORABLE (Allow)
// -------------------------------------------------------------------------
#![allow(
    clippy::restriction,
    clippy::inline_always,
    unused_doc_comments,
    clippy::empty_line_after_doc_comments
)]

mod error;

pub mod generate;
pub mod parse;
pub mod utils;

/// Result alias for virtue's errors
pub type Result<T = ()> = std::result::Result<T, Error>;

pub use self::error::Error;

/// Useful includes
pub mod prelude {
    pub use crate::Error;
    pub use crate::Result;
    pub use crate::generate::FnSelfArg;
    pub use crate::generate::Generator;
    pub use crate::generate::StreamBuilder;
    pub use crate::parse::AttributeAccess;
    pub use crate::parse::Body;
    pub use crate::parse::EnumVariant;
    pub use crate::parse::Fields;
    pub use crate::parse::FromAttribute;
    pub use crate::parse::Parse;
    pub use crate::parse::UnnamedField;

    #[cfg(any(test, feature = "proc-macro2"))]
    pub use proc_macro2::*;

    #[cfg(not(any(test, feature = "proc-macro2")))]
    extern crate proc_macro;
    #[cfg(not(any(test, feature = "proc-macro2")))]
    pub use proc_macro::*;
}

#[cfg(test)]
pub(crate) fn token_stream(
    s: &str
) -> std::iter::Peekable<impl Iterator<Item = proc_macro2::TokenTree> + use<>> {
    use std::str::FromStr;

    let stream = proc_macro2::TokenStream::from_str(s)
        .unwrap_or_else(|e| panic!("Could not parse code: {:?}\n{:?}", s, e));
    stream.into_iter().peekable()
}
