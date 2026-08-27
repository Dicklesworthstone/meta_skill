# Line clipping

[![crate-badge]][crate]
[![docs-badge]][docs]
[![license-badge]][license] \
[![github-badge]][github]
[![build-badge]][build]
[![codecov-badge]][codecov]

[crate-badge]: https://img.shields.io/crates/v/line-clipping?logo=rust
[docs-badge]: https://img.shields.io/badge/docs.rs-line_clipping-blue?logo=rust
[license-badge]: https://img.shields.io/crates/l/line-clipping?logo=apache
[github-badge]: https://img.shields.io/badge/github-ratatui%2Fline--clipping-blue?logo=github
[build-badge]: https://github.com/ratatui/line-clipping/actions/workflows/ci.yml/badge.svg?logo=github
[codecov-badge]: https://img.shields.io/codecov/c/github/ratatui/line-clipping?logo=codecov

[github]: https://github.com/ratatui/line-clipping
[crate]: https://crates.io/crates/line-clipping
[license]: #license
[docs]: https://docs.rs/line-clipping
[build]: https://github.com/ratatui/line-clipping/actions/workflows/ci.yml
[codecov]: https://codecov.io/gh/ratatui/line-clipping

<!-- cargo-rdme start -->

A rust crate to implement several line clipping algorithms. See the
[documentation](https://docs.rs/line_clipping) for more information. The choice of algorithms is
based on the following article which contains a good summary of the options:

Matthes D, Drakopoulos V. [Line Clipping in 2D: Overview, Techniques and
Algorithms](https://pmc.ncbi.nlm.nih.gov/articles/PMC9605407/). J Imaging. 2022 Oct
17;8(10):286. doi: 10.3390/jimaging8100286. PMID: 36286380; PMCID: PMC9605407.

Supports:

- [x] [Cohen-Sutherland](https://docs.rs/line-clipping/latest/line_clipping/cohen_sutherland/)

TODO

- [ ] Cyrus-Beck
- [ ] Liang-Barsky
- [ ] Nicholl-Lee-Nicholl
- [ ] More comprehensive testing

## Installation

```shell
cargo add line-clipping
```

## Minimum supported Rust version

The crate is built with Rust 1.85 to match the 2024 edition. The MSRV may increase in a
future minor release, but will be noted in the changelog.

## Usage

```rust
use line_clipping::cohen_sutherland::clip_line;
use line_clipping::{LineSegment, Point, Window};

let line = LineSegment::new(Point::new(-10.0, -10.0), Point::new(20.0, 20.0));
let window = Window::new(0.0, 10.0, 0.0, 10.0);
let clipped_line = clip_line(line, window);
```

## License

Copyright (c) Josh McKinney

This project is licensed under either of

- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)

at your option.

## Contribution

Contributions are welcome! Please open an issue or submit a pull request.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without
any additional terms or conditions.

<!-- cargo-rdme end -->
