//! Implements the Cohen-Sutherland line clipping algorithm.
//!
//! Returns the clipped line if the original line intersects the clipping window, or `None` if the
//! original line is completely outside the clipping window.
//!
//! Reference: [Cohen-Sutherland algorithm](https://en.wikipedia.org/wiki/Cohen%E2%80%93Sutherland_algorithm)
//!
//! The Cohen-Sutherland algorithm is a line clipping algorithm that divides the 2D plane into 9
//! regions and then determines the region in which the line lies. If the line lies completely
//! outside the clipping window, it is rejected. If the line lies completely inside the clipping
//! window, it is accepted. If the line lies partially inside the clipping window, it is clipped.
//!
//! The regions are defined as follows:
//!
//! ```plain
//! 1001 | 1000 | 1010
//! -----|------|-----
//! 0001 | 0000 | 0010
//! -----|------|-----
//! 0101 | 0100 | 0110
//! ```
//!
//! The algorithm works as follows:
//!
//! 1. Determine the region in which the line's starting point lies.
//! 2. Determine the region in which the line's ending point lies.
//! 3. If both points lie in region 0000, the line is completely inside the clipping window and
//!    should be accepted.
//! 4. If both points lie in the same region that is not 0000, the line is completely outside the
//!    clipping window and should be rejected.
//! 5. If the points lie in different regions, the line is partially inside the clipping window and
//!    should be clipped.
//! 6. Clip the line using the Cohen-Sutherland algorithm.
//! 7. Repeat the process for the clipped line.
//!
//! The Cohen-Sutherland algorithm is commonly used in computer graphics to clip lines against a
//! rectangular window.
//!
//! # Examples
//!
//! ```
//! use line_clipping::cohen_sutherland::clip_line;
//! use line_clipping::{LineSegment, Point, Window};
//!
//! let line = LineSegment::new(Point::new(-10.0, -10.0), Point::new(20.0, 20.0));
//! let window = Window::new(0.0, 10.0, 0.0, 10.0);
//! let clipped_line = clip_line(line, window);
//! ```
use bitflags::bitflags;

use crate::{LineSegment, Point, Window};

/// Clips a line segment against a rectangular window using the Cohen-Sutherland algorithm.
///
/// See the [module-level documentation](crate::cohen_sutherland) for more details on the algorithm.
///
/// # Examples
///
/// ```
/// use line_clipping::cohen_sutherland::clip_line;
/// use line_clipping::{LineSegment, Point, Window};
///
/// let line = LineSegment::new(Point::new(-10.0, -10.0), Point::new(20.0, 20.0));
/// let window = Window::new(0.0, 10.0, 0.0, 10.0);
/// let clipped_line = clip_line(line, window);
/// ```
#[must_use]
pub fn clip_line(mut line: LineSegment, window: Window) -> Option<LineSegment> {
    let mut region_1 = Region::from_point(line.p1, window);
    let mut region_2 = Region::from_point(line.p2, window);

    loop {
        if region_1.intersects(region_2) {
            // The line is completely outside the clipping window.
            return None;
        } else if region_1.is_outside() {
            line.p1 = calculate_intersection(line.p1, line.p2, region_1, window);
            region_1 = Region::from_point(line.p1, window);
        } else if region_2.is_outside() {
            line.p2 = calculate_intersection(line.p2, line.p1, region_2, window);
            region_2 = Region::from_point(line.p2, window);
        } else {
            return Some(line);
        }
    }
}

/// Calculates the intersection between the line segment and the boundary identified by `region`.
///
/// This helper must only be called for regions outside the clipping window. Callers are expected
/// to guard that precondition before selecting which endpoint to clip.
fn calculate_intersection(p1: Point, p2: Point, region: Region, window: Window) -> Point {
    let dx = p2.x - p1.x;
    let dy = p2.y - p1.y;
    if region.contains(Region::LEFT) {
        let y = p1.y + (window.x_min - p1.x) * dy / dx;
        return Point::new(window.x_min, y);
    }
    if region.contains(Region::RIGHT) {
        let y = p1.y + (window.x_max - p1.x) * dy / dx;
        return Point::new(window.x_max, y);
    }
    if region.contains(Region::BOTTOM) {
        let x = p1.x + (window.y_min - p1.y) * dx / dy;
        return Point::new(x, window.y_min);
    }

    debug_assert!(region.contains(Region::TOP));
    let x = p1.x + (window.y_max - p1.y) * dx / dy;
    Point::new(x, window.y_max)
}

bitflags! {
    /// Represents the regions in the Cohen-Sutherland algorithm.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Region: u8 {
        const LEFT = 0b0001;
        const RIGHT = 0b0010;
        const BOTTOM = 0b0100;
        const TOP = 0b1000;
    }
}

impl Region {
    const fn is_outside(self) -> bool {
        !self.is_empty()
    }

    /// Determines the region in which a point lies.
    fn from_point(point: Point, window: Window) -> Self {
        let mut region = Self::empty();
        if point.x < window.x_min {
            region |= Self::LEFT;
        } else if point.x > window.x_max {
            region |= Self::RIGHT;
        }
        if point.y < window.y_min {
            region |= Self::BOTTOM;
        } else if point.y > window.y_max {
            region |= Self::TOP;
        }
        region
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::left(Point::new(-2.0, 0.0), Region::LEFT)]
    #[case::right(Point::new(2.0, 0.0), Region::RIGHT)]
    #[case::top(Point::new(0.0, 2.0), Region::TOP)]
    #[case::bottom(Point::new(0.0, -2.0), Region::BOTTOM)]
    #[case::top_left(Point::new(-2.0, 2.0), Region::LEFT | Region::TOP)]
    #[case::top_right(Point::new(2.0, 2.0), Region::RIGHT | Region::TOP)]
    #[case::bottom_left(Point::new(-2.0, -2.0), Region::LEFT | Region::BOTTOM)]
    #[case::bottom_right(Point::new(2.0, -2.0), Region::RIGHT | Region::BOTTOM)]
    #[case::inside(Point::new(0.0, 0.0), Region::empty())]
    #[case::inside_left(Point::new(-1.0, 0.0), Region::empty())]
    #[case::inside_right(Point::new(1.0, 0.0), Region::empty())]
    #[case::inside_top(Point::new(0.0, 1.0), Region::empty())]
    #[case::inside_bottom(Point::new(0.0, -1.0), Region::empty())]
    #[case::inside_top_left(Point::new(-1.0, 1.0), Region::empty())]
    #[case::inside_top_right(Point::new(1.0, 1.0), Region::empty())]
    #[case::inside_bottom_left(Point::new(-1.0, -1.0), Region::empty())]
    #[case::inside_bottom_right(Point::new(1.0, -1.0), Region::empty())]
    fn region_from_point(#[case] point: Point, #[case] expected: Region) {
        let window = Window::new(-1.0, 1.0, -1.0, 1.0);
        assert_eq!(Region::from_point(point, window), expected);
    }

    #[rstest]
    #[case::top_left(Point::new(-2.0, 2.0), Point::new(-3.0, 3.0))]
    #[case::top_right(Point::new(2.0, 2.0), Point::new(3.0, 3.0))]
    #[case::bottom_left(Point::new(-2.0, -2.0), Point::new(-3.0, -3.0))]
    #[case::bottom_right(Point::new(2.0, -2.0), Point::new(3.0, -3.0))]
    #[case::left(Point::new(-2.0, 0.0), Point::new(-3.0, 0.0))]
    #[case::right(Point::new(2.0, 0.0), Point::new(3.0, 0.0))]
    #[case::top(Point::new(0.0, 2.0), Point::new(0.0, 2.0))]
    #[case::bottom(Point::new(0.0, -2.0), Point::new(0.0, -3.0))]
    fn outside(#[case] p1: Point, #[case] p2: Point) {
        let line = LineSegment::new(p1, p2);
        let window = Window::new(-1.0, 1.0, -1.0, 1.0);
        assert_eq!(clip_line(line, window), None);
    }

    #[rstest]
    #[case::left_border(Point::new(-1.0, -1.0), Point::new(-1.0, 1.0))]
    #[case::right_border(Point::new(1.0, -1.0), Point::new(1.0, 1.0))]
    #[case::top_border(Point::new(-1.0, 1.0), Point::new(1.0, 1.0))]
    #[case::bottom_border(Point::new(-1.0, -1.0), Point::new(1.0, -1.0))]
    #[case::corners_up(Point::new(-1.0, -1.0), Point::new(1.0, 1.0))]
    #[case::corners_down(Point::new(-1.0, 1.0), Point::new(1.0, -1.0))]
    #[case::horizontal(Point::new(-0.5, 0.0), Point::new(0.5, 0.0))]
    #[case::vertical(Point::new(0.0, -0.5), Point::new(0.0, 0.5))]
    #[case::diagonal_up(Point::new(-0.5, -0.5), Point::new(0.5, 0.5))]
    #[case::diagonal_down(Point::new(-0.5, 0.5), Point::new(0.5, -0.5))]
    fn inside(#[case] p1: Point, #[case] p2: Point) {
        let line = LineSegment::new(p1, p2);
        let window = Window::new(-1.0, 1.0, -1.0, 1.0);
        assert_eq!(clip_line(line, window), Some(line));
    }

    /// Test cases for lines that point to the origin and intersect the window. The cases move
    /// clockwise around the window. This makes sure that we test the intersection of the line with
    /// the window from all regions.
    ///
    /// ```
    /// 1 2 3 4 5 6 7 8 1
    /// 8               2
    /// 7   ┌───────┐   3
    /// 6   │       │   4
    /// 5   │   .   │   5
    /// 4   │       │   6
    /// 3   └───────┘   7
    /// 2               8
    /// 1 8 7 6 5 4 3 2 1
    /// ```
    #[rstest]
    #[case::top_1(Point::new(-2.0, 2.0), Point::new(-1.0, 1.0))]
    #[case::top_2(Point::new(-1.5, 2.0), Point::new(-0.75, 1.0))]
    #[case::top_3(Point::new(-1.0, 2.0), Point::new(-0.5, 1.0))]
    #[case::top_4(Point::new(-0.5, 2.0), Point::new(-0.25, 1.0))]
    #[case::top_5(Point::new(0.0, 2.0), Point::new(0.0, 1.0))]
    #[case::top_6(Point::new(0.5, 2.0), Point::new(0.25, 1.0))]
    #[case::top_7(Point::new(1.0, 2.0), Point::new(0.5, 1.0))]
    #[case::top_8(Point::new(1.5, 2.0), Point::new(0.75, 1.0))]
    #[case::right_1(Point::new(2.0, 2.0), Point::new(1.0, 1.0))]
    #[case::right_2(Point::new(2.0, 1.5), Point::new(1.0, 0.75))]
    #[case::right_3(Point::new(2.0, 1.0), Point::new(1.0, 0.5))]
    #[case::right_4(Point::new(2.0, 0.5), Point::new(1.0, 0.25))]
    #[case::right_5(Point::new(2.0, 0.0), Point::new(1.0, 0.0))]
    #[case::right_6(Point::new(2.0, -0.5), Point::new(1.0, -0.25))]
    #[case::right_7(Point::new(2.0, -1.0), Point::new(1.0, -0.5))]
    #[case::right_8(Point::new(2.0, -1.5), Point::new(1.0, -0.75))]
    #[case::bottom_1(Point::new(2.0, -2.0), Point::new(1.0, -1.0))]
    #[case::bottom_2(Point::new(1.5, -2.0), Point::new(0.75, -1.0))]
    #[case::bottom_3(Point::new(1.0, -2.0), Point::new(0.5, -1.0))]
    #[case::bottom_4(Point::new(0.5, -2.0), Point::new(0.25, -1.0))]
    #[case::bottom_5(Point::new(0.0, -2.0), Point::new(0.0, -1.0))]
    #[case::bottom_6(Point::new(-0.5, -2.0), Point::new(-0.25, -1.0))]
    #[case::bottom_7(Point::new(-1.0, -2.0), Point::new(-0.5, -1.0))]
    #[case::bottom_8(Point::new(-1.5, -2.0), Point::new(-0.75, -1.0))]
    #[case::left_1(Point::new(-2.0, -2.0), Point::new(-1.0, -1.0))]
    #[case::left_2(Point::new(-2.0, -1.5), Point::new(-1.0, -0.75))]
    #[case::left_3(Point::new(-2.0, -1.0), Point::new(-1.0, -0.5))]
    #[case::left_4(Point::new(-2.0, -0.5), Point::new(-1.0, -0.25))]
    #[case::left_5(Point::new(-2.0, 0.0), Point::new(-1.0, 0.0))]
    #[case::left_6(Point::new(-2.0, 0.5), Point::new(-1.0, 0.25))]
    #[case::left_7(Point::new(-2.0, 1.0), Point::new(-1.0, 0.5))]
    #[case::left_8(Point::new(-2.0, 1.5), Point::new(-1.0, 0.75))]
    fn one_intersection(#[case] p1: Point, #[case] expected: Point) {
        let line = LineSegment::new(p1, Point::ORIGIN);
        let window = Window::new(-1.0, 1.0, -1.0, 1.0);
        let expected = LineSegment::new(expected, Point::ORIGIN);
        assert_eq!(clip_line(line, window), Some(expected));
    }

    const A: Point = Point::new(-2.0, 2.0);
    const B: Point = Point::new(0.0, 2.0);
    const C: Point = Point::new(2.0, 2.0);
    const D: Point = Point::new(2.0, 0.0);
    const E: Point = Point::new(2.0, -2.0);
    const F: Point = Point::new(0.0, -2.0);
    const G: Point = Point::new(-2.0, -2.0);
    const H: Point = Point::new(-2.0, 0.0);

    /// Test cases for lines that intersect the window twice. The cases move clockwise around the
    /// window. This makes sure that we test every region to each other region.
    /// ```
    /// A       B       C
    ///
    ///     ┌───────┐
    ///     │       │
    /// H   │   .   │   D
    ///     │       │
    ///     └───────┘
    ///
    /// G       F       E
    /// ```
    #[rstest]
    #[case::a_to_d(A, D, Point::new(0.0, 1.0), Point::new(1.0, 0.5))]
    #[case::a_to_e(A, E, Point::new(-1.0, 1.0), Point::new(1.0, -1.0))]
    #[case::a_to_k(A, F, Point::new(-1.0, 0.0), Point::new(-0.5, -1.0))]
    #[case::b_to_d(B, D, Point::new(1.0, 1.0), Point::new(1.0, 1.0))]
    #[case::b_to_e(B, E, Point::new(0.5, 1.0), Point::new(1.0, 0.0))]
    #[case::b_to_f(B, F, Point::new(0.0, 1.0), Point::new(0.0, -1.0))]
    #[case::b_to_g(B, G, Point::new(-0.5, 1.0), Point::new(-1.0, -0.0))]
    #[case::b_to_h(B, H, Point::new(-1.0, 1.0), Point::new(-1.0, 1.0))]
    #[case::c_to_f(C, F, Point::new(1.0, 0.0), Point::new(0.5, -1.0))]
    #[case::c_to_g(C, G, Point::new(1.0, 1.0), Point::new(-1.0, -1.0))]
    #[case::c_to_h(C, H, Point::new(0.0, 1.0), Point::new(-1.0, 0.5))]
    #[case::d_to_f(D, F, Point::new(1.0, -1.0), Point::new(1.0, -1.0))]
    #[case::d_to_g(D, G, Point::new(1.0, -0.5), Point::new(0.0, -1.0))]
    #[case::d_to_h(D, H, Point::new(1.0, 0.0), Point::new(-1.0, 0.0))]
    #[case::e_to_h(E, H, Point::new(0.0, -1.0), Point::new(-1.0, -0.5))]
    #[case::f_to_h(F, H, Point::new(-1.0, -1.0), Point::new(-1.0, -1.0))]
    fn two_intersections(
        #[case] p1: Point,
        #[case] p2: Point,
        #[case] expected_p1: Point,
        #[case] expected_p2: Point,
    ) {
        let line = LineSegment::new(p1, p2);
        let window = Window::new(-1.0, 1.0, -1.0, 1.0);
        let expected = LineSegment::new(expected_p1, expected_p2);
        assert_eq!(clip_line(line, window).unwrap(), expected);
    }
}
