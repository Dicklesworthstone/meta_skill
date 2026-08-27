# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.6.0] — 2026-02-10
- Bump to MSRV 1.85.0 and Edition 2024 in line with `rand` ([#28])
- Update `rand` to version 0.10.0 ([#31], [#48])

### Additions
- `MultiDistribution` trait to sample more efficiently from multi-dimensional distributions ([#18])
- Add `WeightedAliasIndex::weights()` to reconstruct the original weights in O(n) ([#25])
- `ConstMultiDistribution` trait as support for fixed-dimension distributions ([#29])

### Changes
- Moved `Dirichlet` into the new `multi` module and implement `MultiDistribution` for it ([#18])
- `Dirichlet` no longer uses `const` generics, which means that its size is not required at compile time. Essentially a revert of [rand#1292]. ([#30])

### Fixes
- Fix `Geometric::new` for small `p > 0` where `1 - p` rounds to 1 ([#36])
- Use `direct-minimal-versions` ([#38])
- Fix panic in `FisherF::new` on almost zero parameters ([#39])
- Fix panic in `NormalInverseGaussian::new` with very large `alpha`; this is a Value-breaking change ([#40])
- Fix hang and debug assertion in `Zipf::new` on invalid parameters ([#41])
- Fix panic in `Binomial::sample` with `n ≥ 2^63`; this is a Value-breaking change ([#43])
- Error instead of producing `-inf` output for `Exp` when `lambda` is `-0.0` ([#44])
- Avoid returning NaN from `Gamma::sample`; this is a Value-breaking change and also affects `ChiSquared` and `Dirichlet` ([#46])

## [0.5.1]

### Testing
- Added building the crate to CI

### Fixes
- Fix missing import for `no_std` builds

## [0.5.0] - 2025-01-27

### Dependencies and features
- Bump the MSRV to 1.61.0 ([rand#1207], [rand#1246], [rand#1269], [rand#1341], [rand#1416]); note that 1.60.0 may work for dependents when using `--ignore-rust-version`
- Update to `rand` v0.9.0 ([rand#1558])
- Rename feature `serde1` to `serde` ([rand#1477])

### API changes
- Make distributions comparable with `PartialEq` ([rand#1218])
- `Dirichlet` now uses `const` generics, which means that its size is required at compile time ([rand#1292])
- The `Dirichlet::new_with_size` constructor was removed ([rand#1292])
- Add `WeightedIndexTree` ([rand#1372], [rand#1444])
- Add `PertBuilder` to allow specification of `mean` or `mode` ([rand#1452])
- Rename `Zeta`'s parameter `a` to `s` ([rand#1466])
- Mark `WeightError`, `PoissonError`, `BinomialError` as `#[non_exhaustive]` ([rand#1480])
- Remove support for usage of `isize` as a `WeightedAliasIndex` weight ([rand#1487])
- Change parameter type of `Zipf::new`: `n` is now floating-point ([rand#1518])

### API changes: renames
- Move `Slice` -> `slice::Choose`, `EmptySlice` -> `slice::Empty` ([rand#1548])
- Rename trait `DistString` -> `SampleString` ([rand#1548])
- Rename `DistIter` -> `Iter`, `DistMap` -> `Map` ([rand#1548])
- Move `{Weight, WeightError, WeightedIndex}` -> `weighted::{Weight, Error, WeightedIndex}` ([rand#1548])
- Move `weighted_alias::{AliasableWeight, WeightedAliasIndex}` -> `weighted::{..}` ([rand#1548])
- Move `weighted_tree::WeightedTreeIndex` -> `weighted::WeightedTreeIndex` ([rand#1548])

### Testing
- Add Kolmogorov Smirnov tests for distributions ([rand#1494], [rand#1504], [rand#1525], [rand#1530])

### Fixes
- Fix Knuth's method so `Poisson` doesn't return -1.0 for small lambda ([rand#1284])
- Fix `Poisson` distribution instantiation so it return an error if lambda is infinite ([rand#1291])
- Fix Dirichlet sample for small alpha values to avoid NaN samples ([rand#1209])
- Fix infinite loop in `Binomial` distribution ([rand#1325])
- Fix `Pert` distribution where `mode` is close to `(min + max) / 2` ([rand#1452])
- Fix panic in Binomial ([rand#1484])
- Limit the maximal acceptable lambda for `Poisson` to solve ([rand#1312]) ([rand#1498])
- Fix bug in `Hypergeometric`, this is a Value-breaking change ([rand#1510])

### Other changes
- Remove unused fields from `Gamma`, `NormalInverseGaussian` and `Zipf` distributions ([rand#1184])
  This breaks serialization compatibility with older versions.
- Add plots for `rand_distr` distributions to documentation ([rand#1434])
- Move some of the computations in Binomial from `sample` to `new` ([rand#1484])
- Reimplement `Poisson`'s rejection method to improve performance and correct sampling inaccuracies for large lambda values, this is a Value-breaking change ([rand#1560])

## [0.4.3] - 2021-12-30
- Fix `no_std` build ([rand#1208])

## [0.4.2] - 2021-09-18
- New `Zeta` and `Zipf` distributions ([rand#1136])
- New `SkewNormal` distribution ([rand#1149])
- New `Gumbel` and `Frechet` distributions ([rand#1168], [rand#1171])

## [0.4.1] - 2021-06-15
- Empirically test PDF of normal distribution ([rand#1121])
- Correctly document `no_std` support ([rand#1100])
- Add `std_math` feature to prefer `std` over `libm` for floating point math ([rand#1100])
- Add mean and std_dev accessors to Normal ([rand#1114])
- Make sure all distributions and their error types implement `Error`, `Display`, `Clone`, `Copy`, `PartialEq` and `Eq` as appropriate ([rand#1126])
- Port benchmarks to use Criterion crate ([rand#1116])
- Support serde for distributions ([rand#1141])

## [0.4.0] - 2020-12-18
- Bump `rand` to v0.8.0
- New `Geometric`, `StandardGeometric` and `Hypergeometric` distributions ([rand#1062])
- New `Beta` sampling algorithm for improved performance and accuracy ([rand#1000])
- `Normal` and `LogNormal` now support `from_mean_cv` and `from_zscore` ([rand#1044])
- Variants of `NormalError` changed ([rand#1044])

## [0.3.0] - 2020-08-25
- Move alias method for `WeightedIndex` from `rand` ([rand#945])
- Rename `WeightedIndex` to `WeightedAliasIndex` ([rand#1008])
- Replace custom `Float` trait with `num-traits::Float` ([rand#987])
- Enable `no_std` support via `num-traits` math functions ([rand#987])
- Remove `Distribution<u64>` impl for `Poisson` ([rand#987])
- Tweak `Dirichlet` and `alias_method` to use boxed slice instead of `Vec` ([rand#987])
- Use whitelist for package contents, reducing size by 5kb ([rand#983])
- Add case `lambda = 0` in the parametrization of `Exp` ([rand#972])
- Implement inverse Gaussian distribution ([rand#954])
- Reformatting and use of `rustfmt::skip` ([rand#926])
- All error types now implement `std::error::Error` ([rand#919])
- Re-exported `rand::distributions::BernoulliError` ([rand#919])
- Add value stability tests for distributions ([rand#891])

## [0.2.2] - 2019-09-10
- Fix version requirement on rand lib ([rand#847])
- Clippy fixes & suppression ([rand#840])

## [0.2.1] - 2019-06-29
- Update dependency to support Rand 0.7
- Doc link fixes

## [0.2.0] - 2019-06-06
- Remove `new` constructors for zero-sized types
- Add Pert distribution
- Fix undefined behavior in `Poisson`
- Make all distributions return `Result`s instead of panicking
- Implement `f32` support for most distributions
- Rename `UnitSphereSurface` to `UnitSphere`
- Implement `UnitBall` and `UnitDisc`

## [0.1.0] - 2019-06-06
Initial release. This is equivalent to the code in `rand` 0.6.5.

[#15]: https://github.com/rust-random/rand_distr/pull/15
[#18]: https://github.com/rust-random/rand_distr/pull/18
[#25]: https://github.com/rust-random/rand_distr/pull/25
[#28]: https://github.com/rust-random/rand_distr/pull/28
[#29]: https://github.com/rust-random/rand_distr/pull/29
[#30]: https://github.com/rust-random/rand_distr/pull/30
[#31]: https://github.com/rust-random/rand_distr/pull/31
[#36]: https://github.com/rust-random/rand_distr/pull/36
[#38]: https://github.com/rust-random/rand_distr/pull/38
[#39]: https://github.com/rust-random/rand_distr/pull/39
[#40]: https://github.com/rust-random/rand_distr/pull/40
[#41]: https://github.com/rust-random/rand_distr/pull/41
[#43]: https://github.com/rust-random/rand_distr/pull/43
[#44]: https://github.com/rust-random/rand_distr/pull/44
[#46]: https://github.com/rust-random/rand_distr/pull/46
[#48]: https://github.com/rust-random/rand_distr/pull/48
[rand#840]: https://github.com/rust-random/rand/pull/840
[rand#847]: https://github.com/rust-random/rand/pull/847
[rand#891]: https://github.com/rust-random/rand/pull/891
[rand#919]: https://github.com/rust-random/rand/pull/919
[rand#926]: https://github.com/rust-random/rand/pull/926
[rand#945]: https://github.com/rust-random/rand/pull/945
[rand#954]: https://github.com/rust-random/rand/pull/954
[rand#972]: https://github.com/rust-random/rand/pull/972
[rand#983]: https://github.com/rust-random/rand/pull/983
[rand#987]: https://github.com/rust-random/rand/pull/987
[rand#1000]: https://github.com/rust-random/rand/pull/1000
[rand#1008]: https://github.com/rust-random/rand/pull/1008
[rand#1044]: https://github.com/rust-random/rand/pull/1044
[rand#1062]: https://github.com/rust-random/rand/pull/1062
[rand#1100]: https://github.com/rust-random/rand/pull/1100
[rand#1114]: https://github.com/rust-random/rand/pull/1114
[rand#1116]: https://github.com/rust-random/rand/pull/1116
[rand#1121]: https://github.com/rust-random/rand/pull/1121
[rand#1126]: https://github.com/rust-random/rand/pull/1126
[rand#1136]: https://github.com/rust-random/rand/pull/1136
[rand#1141]: https://github.com/rust-random/rand/pull/1141
[rand#1149]: https://github.com/rust-random/rand/pull/1149
[rand#1168]: https://github.com/rust-random/rand/pull/1168
[rand#1171]: https://github.com/rust-random/rand/pull/1171
[rand#1184]: https://github.com/rust-random/rand/pull/1184
[rand#1207]: https://github.com/rust-random/rand/pull/1207
[rand#1208]: https://github.com/rust-random/rand/pull/1208
[rand#1209]: https://github.com/rust-random/rand/pull/1209
[rand#1218]: https://github.com/rust-random/rand/pull/1218
[rand#1246]: https://github.com/rust-random/rand/pull/1246
[rand#1269]: https://github.com/rust-random/rand/pull/1269
[rand#1284]: https://github.com/rust-random/rand/pull/1284
[rand#1291]: https://github.com/rust-random/rand/pull/1291
[rand#1292]: https://github.com/rust-random/rand/pull/1292
[rand#1312]: https://github.com/rust-random/rand/pull/1312
[rand#1325]: https://github.com/rust-random/rand/pull/1325
[rand#1341]: https://github.com/rust-random/rand/pull/1341
[rand#1372]: https://github.com/rust-random/rand/pull/1372
[rand#1416]: https://github.com/rust-random/rand/pull/1416
[rand#1434]: https://github.com/rust-random/rand/pull/1434
[rand#1444]: https://github.com/rust-random/rand/pull/1444
[rand#1452]: https://github.com/rust-random/rand/pull/1452
[rand#1466]: https://github.com/rust-random/rand/pull/1466
[rand#1477]: https://github.com/rust-random/rand/pull/1477
[rand#1480]: https://github.com/rust-random/rand/pull/1480
[rand#1484]: https://github.com/rust-random/rand/pull/1484
[rand#1487]: https://github.com/rust-random/rand/pull/1487
[rand#1494]: https://github.com/rust-random/rand/pull/1494
[rand#1498]: https://github.com/rust-random/rand/pull/1498
[rand#1504]: https://github.com/rust-random/rand/pull/1504
[rand#1510]: https://github.com/rust-random/rand/pull/1510
[rand#1518]: https://github.com/rust-random/rand/pull/1518
[rand#1525]: https://github.com/rust-random/rand/pull/1525
[rand#1530]: https://github.com/rust-random/rand/pull/1530
[rand#1548]: https://github.com/rust-random/rand/pull/1548
[rand#1558]: https://github.com/rust-random/rand/pull/1558
[rand#1560]: https://github.com/rust-random/rand/pull/1560
