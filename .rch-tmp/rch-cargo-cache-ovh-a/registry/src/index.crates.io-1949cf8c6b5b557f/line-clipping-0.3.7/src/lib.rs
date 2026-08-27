#![no_std]
//! A rust crate to implement several line clipping algorithms. See the
//! [documentation](https://docs.rs/line_clipping) for more information. The choice of algorithms is
//! based on the following article which contains a good summary of the options:
//!
//! Matthes D, Drakopoulos V. [Line Clipping in 2D: Overview, Techniques and
//! Algorithms](https://pmc.ncbi.nlm.nih.gov/articles/PMC9605407/). J Imaging. 2022 Oct
//! 17;8(10):286. doi: 10.3390/jimaging8100286. PMID: 36286380; PMCID: PMC9605407.
//!
//! Supports:
//!
//! - [x] [Cohen-Sutherland](crate::cohen_sutherland)
//!
//! TODO
//!
//! - [ ] Cyrus-Beck
//! - [ ] Liang-Barsky
//! - [ ] Nicholl-Lee-Nicholl
//! - [ ] More comprehensive testing
//!
//! # Installation
//!
//! ```shell
//! cargo add line-clipping
//! ```
//!
//! # Minimum supported Rust version
//!
//! The crate is built with Rust 1.85 to match the 2024 edition. The MSRV may increase in a
//! future minor release, but will be noted in the changelog.
//!
//! # Usage
//!
//! ```rust
//! use line_clipping::cohen_sutherland::clip_line;
//! use line_clipping::{LineSegment, Point, Window};
//!
//! let line = LineSegment::new(Point::new(-10.0, -10.0), Point::new(20.0, 20.0));
//! let window = Window::new(0.0, 10.0, 0.0, 10.0);
//! let clipped_line = clip_line(line, window);
//! ```
//!
//! # License
//!
//! Copyright (c) Josh McKinney
//!
//! This project is licensed under either of
//!
//! - MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)
//! - Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
//!
//! at your option.
//!
//! # Contribution
//!
//! Contributions are welcome! Please open an issue or submit a pull request.
//!
//! Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
//! the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without
//! any additional terms or conditions.
pub mod cohen_sutherland;

/// A point in 2D space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    /// The x coordinate of the point.
    pub x: f64,

    /// The y coordinate of the point.
    pub y: f64,
}

impl Point {
    /// A point at the origin (0.0, 0.0).
    pub const ORIGIN: Self = Self { x: 0.0, y: 0.0 };

    /// Creates a new point.
    #[must_use]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// A line segment in 2D space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineSegment {
    /// The first point of the line segment.
    pub p1: Point,

    /// The second point of the line segment.
    pub p2: Point,
}

impl LineSegment {
    /// Creates a new line segment.
    #[must_use]
    pub const fn new(p1: Point, p2: Point) -> Self {
        Self { p1, p2 }
    }
}

/// A rectangular region to clip lines against.
#[derive(Debug, Clone, Copy)]
pub struct Window {
    /// The minimum x coordinate of the window.
    pub x_min: f64,

    /// The maximum x coordinate of the window.
    pub x_max: f64,

    /// The minimum y coordinate of the window.
    pub y_min: f64,

    /// The maximum y coordinate of the window.
    pub y_max: f64,
}

impl Window {
    /// Creates a new window.
    #[must_use]
    pub const fn new(x_min: f64, x_max: f64, y_min: f64, y_max: f64) -> Self {
        Self {
            x_min,
            x_max,
            y_min,
            y_max,
        }
    }
}
