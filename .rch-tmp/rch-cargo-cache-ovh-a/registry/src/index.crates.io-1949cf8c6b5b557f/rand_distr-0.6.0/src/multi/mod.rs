// Copyright 2025 Developers of the Rand project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Contains Multi-dimensional distributions.
//!
//! The trait [`MultiDistribution`] supports multi-dimensional sampling without
//! allocating a [`Vec`](std::vec::Vec) for each sample.
//! [`ConstMultiDistribution`] is an extension for distributions with constant
//! dimension.
//!
//! Multi-dimensional distributions implement `MultiDistribution<T>` and (where
//! the dimension is fixed) `ConstMultiDistribution<T>` for some scalar type
//! `T`. They may also implement `Distribution<Vec<T>>` and (where the
//! dimension, `N`, is fixed) `Distribution<[T; N]>`.

use rand::Rng;

/// A standard abstraction for distributions with multi-dimensional results
///
/// Implementations may also implement `Distribution<Vec<T>>`.
pub trait MultiDistribution<T> {
    /// The length of a sample (dimension of the distribution)
    fn sample_len(&self) -> usize;

    /// Sample a multi-dimensional result from the distribution
    ///
    /// The result is written to `output`. Implementations should assert that
    /// `output.len()` equals the result of [`Self::sample_len`].
    fn sample_to_slice<R: Rng + ?Sized>(&self, rng: &mut R, output: &mut [T]);
}

/// An extension of [`MultiDistribution`] for multi-dimensional distributions of fixed dimension
///
/// Implementations may also implement `Distribution<[T; SAMPLE_LEN]>`.
pub trait ConstMultiDistribution<T>: MultiDistribution<T> {
    /// Constant sample length (dimension of the distribution)
    const SAMPLE_LEN: usize;
}

macro_rules! distribution_impl {
    ($scalar:ident) => {
        fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Vec<$scalar> {
            use crate::multi::MultiDistribution;
            let mut buf = vec![Default::default(); self.sample_len()];
            self.sample_to_slice(rng, &mut buf);
            buf
        }
    };
}

#[allow(unused)]
macro_rules! const_distribution_impl {
    ($scalar:ident) => {
        fn sample<R: Rng + ?Sized>(
            &self,
            rng: &mut R,
        ) -> [$scalar; <Self as crate::multi::MultiDistribution>::SAMPLE_LEN] {
            use crate::multi::MultiDistribution;
            let mut buf =
                [Default::default(); <Self as crate::multi::MultiDistribution>::SAMPLE_LEN];
            self.sample_to_slice(rng, &mut buf);
            buf
        }
    };
}

pub use dirichlet::Dirichlet;

mod dirichlet;
