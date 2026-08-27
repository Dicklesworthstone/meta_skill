//! Procedural macros for the bincode-next serialization library.
//!
//! This crate provides derive macros for `Encode`, `Decode`, `BorrowDecode`, `BitPacked`, `ZeroCopy`, `Fingerprint`, and `StaticSize`.

#![allow(dead_code)]
#![allow(unused_must_use)]

mod attribute;
mod derive_bit_packed;
mod derive_enum;
mod derive_fingerprint;
mod derive_static_size;
mod derive_struct;
mod derive_zerocopy;

use attribute::ContainerAttributes;
use virtue::prelude::AttributeAccess;
use virtue::prelude::Body;
#[allow(unused_imports)]
use virtue::prelude::Error;
use virtue::prelude::Parse;
use virtue::prelude::Result;
use virtue::prelude::TokenStream;

#[proc_macro_derive(Encode, attributes(bincode))]
pub fn derive_encode(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    derive_encode_inner(input).unwrap_or_else(|e| e.into_token_stream())
}

/// # Errors
///
/// Returns an error if the input cannot be parsed or if the code generation fails.
fn derive_encode_inner(input: TokenStream) -> Result<TokenStream> {
    let parse = Parse::new(input)?;
    let (mut generator, attributes, body) = parse.into_generator();
    let attributes = attributes
        .get_attribute::<ContainerAttributes>()?
        .unwrap_or_default();

    match body {
        | Body::Struct(body) => {
            derive_struct::DeriveStruct {
                fields: body.fields,
                attributes,
            }
            .generate_encode(&mut generator)?;
        },
        | Body::Enum(body) => {
            derive_enum::DeriveEnum {
                variants: body.variants,
                attributes,
            }
            .generate_encode(&mut generator)?;
        },
    }

    generator.export_to_file("bincode_next", "Encode");
    generator.finish()
}

#[proc_macro_derive(Decode, attributes(bincode))]
pub fn derive_decode(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    derive_decode_inner(input).unwrap_or_else(|e| e.into_token_stream())
}

/// # Errors
///
/// Returns an error if the input cannot be parsed or if the code generation fails.
fn derive_decode_inner(input: TokenStream) -> Result<TokenStream> {
    let parse = Parse::new(input)?;
    let (mut generator, attributes, body) = parse.into_generator();
    let attributes = attributes
        .get_attribute::<ContainerAttributes>()?
        .unwrap_or_default();

    match body {
        | Body::Struct(body) => {
            derive_struct::DeriveStruct {
                fields: body.fields,
                attributes,
            }
            .generate_decode(&mut generator)?;
        },
        | Body::Enum(body) => {
            derive_enum::DeriveEnum {
                variants: body.variants,
                attributes,
            }
            .generate_decode(&mut generator)?;
        },
    }

    generator.export_to_file("bincode_next", "Decode");
    generator.finish()
}

#[proc_macro_derive(BorrowDecode, attributes(bincode))]
pub fn derive_borrow_decode(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    derive_borrow_decode_inner(input).unwrap_or_else(|e| e.into_token_stream())
}

/// # Errors
///
/// Returns an error if the input cannot be parsed or if the code generation fails.
fn derive_borrow_decode_inner(input: TokenStream) -> Result<TokenStream> {
    let parse = Parse::new(input)?;
    let (mut generator, attributes, body) = parse.into_generator();
    let attributes = attributes
        .get_attribute::<ContainerAttributes>()?
        .unwrap_or_default();

    match body {
        | Body::Struct(body) => {
            derive_struct::DeriveStruct {
                fields: body.fields,
                attributes,
            }
            .generate_borrow_decode(&mut generator)?;
        },
        | Body::Enum(body) => {
            derive_enum::DeriveEnum {
                variants: body.variants,
                attributes,
            }
            .generate_borrow_decode(&mut generator)?;
        },
    }

    generator.export_to_file("bincode_next", "BorrowDecode");
    generator.finish()
}

#[proc_macro_derive(BitPacked, attributes(bincode))]
pub fn derive_bit_packed(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    derive_bit_packed_inner(input).unwrap_or_else(|e| e.into_token_stream())
}

/// # Errors
///
/// Returns an error if the input cannot be parsed or if the code generation fails.
fn derive_bit_packed_inner(input: TokenStream) -> Result<TokenStream> {
    let parse = Parse::new(input)?;
    let (mut generator, attributes, body) = parse.into_generator();
    let attributes = attributes
        .get_attribute::<ContainerAttributes>()?
        .unwrap_or_default();

    match body {
        | Body::Struct(body) => {
            derive_bit_packed::DeriveBitPacked {
                fields: body.fields,
                attributes,
            }
            .generate(&mut generator)?;
        },
        | Body::Enum(body) => {
            derive_bit_packed::DeriveBitPackedEnum {
                variants: body.variants,
                attributes,
            }
            .generate(&mut generator)?;
        },
    }

    generator.export_to_file("bincode_next", "BitPacked");
    generator.finish()
}

#[cfg(feature = "zero-copy")]
#[proc_macro_derive(ZeroCopy, attributes(bincode))]
pub fn derive_zerocopy(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    derive_zerocopy_inner(input).unwrap_or_else(|e| e.into_token_stream())
}

#[cfg(feature = "zero-copy")]
fn derive_zerocopy_inner(input: TokenStream) -> Result<TokenStream> {
    let parse = Parse::new(input)?;

    let visibility = match &parse {
        | Parse::Struct { visibility, .. } => visibility.clone(),
        | Parse::Enum { visibility, .. } => visibility.clone(),
        | _ => unreachable!(),
    };
    let (mut generator, attributes, body) = parse.into_generator();
    let repr = attributes
        .get_attribute::<attribute::ReprAttributes>()?
        .unwrap_or_default();

    let attributes = attributes
        .get_attribute::<ContainerAttributes>()?
        .unwrap_or_default();

    match body {
        | Body::Struct(body) => {
            if !repr.is_c && !repr.is_transparent {
                return Err(Error::custom(
                    "ZeroCopy structs must have #[repr(C)] or #[repr(transparent)]",
                ));
            }
            derive_zerocopy::DeriveZeroCopy {
                fields: body.fields,
                variants: None,
                attributes,
                repr,
                visibility,
            }
            .generate(&mut generator)?;
        },
        | Body::Enum(body) => {
            if !repr.is_c
                && !repr.is_u8
                && !repr.is_u16
                && !repr.is_u32
                && !repr.is_u64
                && !repr.is_i8
                && !repr.is_i16
                && !repr.is_i32
                && !repr.is_i64
            {
                return Err(Error::custom(
                    "ZeroCopy enums must have #[repr(C)] or a primitive repr like #[repr(u8)]",
                ));
            }
            derive_zerocopy::DeriveZeroCopy {
                fields: None,
                variants: Some(body.variants),
                attributes,
                repr,
                visibility,
            }
            .generate(&mut generator)?;
        },
    }

    generator.export_to_file("bincode_next", "ZeroCopy");
    generator.finish()
}

#[cfg(feature = "static-size")]
#[proc_macro_derive(StaticSize, attributes(bincode, static_size))]
pub fn derive_static_size(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    derive_static_size_inner(input).unwrap_or_else(|e| e.into_token_stream())
}

#[cfg(feature = "static-size")]
fn derive_static_size_inner(input: TokenStream) -> Result<TokenStream> {
    let parse = Parse::new(input)?;
    let (mut generator, attributes, body) = parse.into_generator();
    let attributes = attributes
        .get_attribute::<ContainerAttributes>()?
        .unwrap_or_default();

    match body {
        | Body::Struct(body) => {
            derive_static_size::DeriveStaticSize {
                fields: body.fields,
                variants: None,
                attributes,
            }
            .generate(&mut generator)?;
        },
        | Body::Enum(body) => {
            derive_static_size::DeriveStaticSize {
                fields: None,
                variants: Some(body.variants),
                attributes,
            }
            .generate(&mut generator)?;
        },
    }

    generator.export_to_file("bincode_next", "StaticSize");
    generator.finish()
}

#[proc_macro_derive(Fingerprint, attributes(bincode))]
pub fn derive_fingerprint(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    derive_fingerprint_inner(input).unwrap_or_else(|e| e.into_token_stream())
}

fn derive_fingerprint_inner(input: TokenStream) -> Result<TokenStream> {
    let parse = Parse::new(input)?;
    let type_name = match &parse {
        | Parse::Struct { name, .. } | Parse::Enum { name, .. } => name.to_string(),
        | _ => unreachable!(),
    };
    let (mut generator, attributes, body) = parse.into_generator();
    let attributes = attributes
        .get_attribute::<ContainerAttributes>()?
        .unwrap_or_default();

    match body {
        | Body::Struct(body) => {
            derive_fingerprint::DeriveFingerprint {
                fields: body.fields,
                variants: None,
                attributes,
            }
            .generate(&mut generator, &type_name)?;
        },
        | Body::Enum(body) => {
            derive_fingerprint::DeriveFingerprint {
                fields: None,
                variants: Some(body.variants),
                attributes,
            }
            .generate(&mut generator, &type_name)?;
        },
    }

    generator.export_to_file("bincode_next", "Fingerprint");
    generator.finish()
}
