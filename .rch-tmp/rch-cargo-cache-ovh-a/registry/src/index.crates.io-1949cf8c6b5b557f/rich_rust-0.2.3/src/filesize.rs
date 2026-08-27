//! File size formatting for human-readable output.
//!
//! This module provides functions to format byte sizes into human-readable strings,
//! supporting both binary (1024-based: KiB, MiB, GiB) and decimal (1000-based: KB, MB, GB) units.
//!
//! # Examples
//!
//! ```
//! use rich_rust::filesize::{decimal, format_size, SizeUnit};
//!
//! // Decimal (1000-based) formatting
//! assert_eq!(decimal(1_500_000), "1.5 MB");
//! assert_eq!(decimal(1_000), "1.0 kB");
//!
//! // Binary (1024-based) formatting
//! use rich_rust::filesize::binary;
//! assert_eq!(binary(1_048_576), "1.0 MiB");
//! assert_eq!(binary(1_024), "1.0 KiB");
//!
//! // Custom precision
//! use rich_rust::filesize::decimal_with_precision;
//! assert_eq!(decimal_with_precision(1_536_000, 2), "1.54 MB");
//! ```

/// Units for binary (1024-based) file sizes.
const BINARY_UNITS: &[&str] = &[
    "bytes", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB", "ZiB", "YiB",
];

/// Units for decimal (1000-based) file sizes.
const DECIMAL_UNITS: &[&str] = &["bytes", "kB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB"];

/// Size unit system to use for formatting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SizeUnit {
    /// Binary units (1024-based): KiB, MiB, GiB, etc.
    #[default]
    Binary,
    /// Decimal units (1000-based): kB, MB, GB, etc.
    Decimal,
}

/// Format a size in bytes to a human-readable string.
///
/// # Arguments
///
/// * `size` - Size in bytes (can be negative for deltas)
/// * `unit` - Whether to use binary (1024) or decimal (1000) units
/// * `precision` - Number of decimal places
///
/// # Examples
///
/// ```
/// use rich_rust::filesize::{format_size, SizeUnit};
///
/// assert_eq!(format_size(1_500_000, SizeUnit::Decimal, 1), "1.5 MB");
/// assert_eq!(format_size(1_048_576, SizeUnit::Binary, 1), "1.0 MiB");
/// assert_eq!(format_size(-1_000, SizeUnit::Decimal, 1), "-1.0 kB");
/// ```
#[must_use]
pub fn format_size(size: i64, unit: SizeUnit, precision: usize) -> String {
    let (base, units): (f64, &[&str]) = match unit {
        SizeUnit::Binary => (1024.0, BINARY_UNITS),
        SizeUnit::Decimal => (1000.0, DECIMAL_UNITS),
    };

    let negative = size < 0;
    let abs_size = size.unsigned_abs();

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    if abs_size < base as u64 {
        // Special case: show as bytes without decimal
        let prefix = if negative { "-" } else { "" };
        return format!("{prefix}{abs_size} bytes");
    }

    #[allow(clippy::cast_precision_loss)]
    let mut value = abs_size as f64;
    let mut unit_idx = 0;

    while value >= base && unit_idx < units.len() - 1 {
        value /= base;
        unit_idx += 1;
    }

    let prefix = if negative { "-" } else { "" };
    format!("{prefix}{value:.precision$} {}", units[unit_idx])
}

/// Format a size in bytes to a human-readable string using decimal (1000-based) units.
///
/// Uses 1 decimal place by default.
///
/// # Examples
///
/// ```
/// use rich_rust::filesize::decimal;
///
/// assert_eq!(decimal(1_000), "1.0 kB");
/// assert_eq!(decimal(1_500_000), "1.5 MB");
/// assert_eq!(decimal(1_000_000_000), "1.0 GB");
/// ```
#[must_use]
pub fn decimal(size: u64) -> String {
    #[allow(clippy::cast_possible_wrap)]
    format_size(size as i64, SizeUnit::Decimal, 1)
}

/// Format a size in bytes to a human-readable string using decimal (1000-based) units
/// with custom precision.
///
/// # Examples
///
/// ```
/// use rich_rust::filesize::decimal_with_precision;
///
/// assert_eq!(decimal_with_precision(1_536_000, 2), "1.54 MB");
/// assert_eq!(decimal_with_precision(1_234_567_890, 3), "1.235 GB");
/// ```
#[must_use]
pub fn decimal_with_precision(size: u64, precision: usize) -> String {
    #[allow(clippy::cast_possible_wrap)]
    format_size(size as i64, SizeUnit::Decimal, precision)
}

/// Format a size in bytes to a human-readable string using binary (1024-based) units.
///
/// Uses 1 decimal place by default.
///
/// # Examples
///
/// ```
/// use rich_rust::filesize::binary;
///
/// assert_eq!(binary(1_024), "1.0 KiB");
/// assert_eq!(binary(1_048_576), "1.0 MiB");
/// assert_eq!(binary(1_073_741_824), "1.0 GiB");
/// ```
#[must_use]
pub fn binary(size: u64) -> String {
    #[allow(clippy::cast_possible_wrap)]
    format_size(size as i64, SizeUnit::Binary, 1)
}

/// Format a size in bytes to a human-readable string using binary (1024-based) units
/// with custom precision.
///
/// # Examples
///
/// ```
/// use rich_rust::filesize::binary_with_precision;
///
/// assert_eq!(binary_with_precision(1_572_864, 2), "1.50 MiB");
/// assert_eq!(binary_with_precision(1_073_741_824, 3), "1.000 GiB");
/// ```
#[must_use]
pub fn binary_with_precision(size: u64, precision: usize) -> String {
    #[allow(clippy::cast_possible_wrap)]
    format_size(size as i64, SizeUnit::Binary, precision)
}

/// Format a transfer speed in bytes per second to a human-readable string.
///
/// # Arguments
///
/// * `bytes_per_second` - Transfer speed in bytes per second
/// * `unit` - Whether to use binary (1024) or decimal (1000) units
/// * `precision` - Number of decimal places
///
/// # Examples
///
/// ```
/// use rich_rust::filesize::{format_speed, SizeUnit};
///
/// assert_eq!(format_speed(1_500_000.0, SizeUnit::Decimal, 1), "1.5 MB/s");
/// assert_eq!(format_speed(1_048_576.0, SizeUnit::Binary, 1), "1.0 MiB/s");
/// ```
#[must_use]
pub fn format_speed(bytes_per_second: f64, unit: SizeUnit, precision: usize) -> String {
    // Handle NaN and Infinity gracefully
    if bytes_per_second.is_nan() {
        return "NaN".to_string();
    }
    if bytes_per_second.is_infinite() {
        let prefix = if bytes_per_second.is_sign_negative() {
            "-"
        } else {
            ""
        };
        return format!("{prefix}∞");
    }

    let (base, units): (f64, &[&str]) = match unit {
        SizeUnit::Binary => (1024.0, BINARY_UNITS),
        SizeUnit::Decimal => (1000.0, DECIMAL_UNITS),
    };

    let negative = bytes_per_second < 0.0;
    let mut value = bytes_per_second.abs();

    if value < base {
        // Show as bytes/s
        let prefix = if negative { "-" } else { "" };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let int_value = value as u64;
        return format!("{prefix}{int_value} bytes/s");
    }

    let mut unit_idx = 0;
    while value >= base && unit_idx < units.len() - 1 {
        value /= base;
        unit_idx += 1;
    }

    let unit_str = units[unit_idx];
    // Replace "bytes" with just the unit letter for speed display
    let speed_unit = if unit_str == "bytes" {
        "bytes/s"
    } else {
        // Append /s to unit
        &format!("{unit_str}/s")
    };

    let prefix = if negative { "-" } else { "" };
    format!("{prefix}{value:.precision$} {speed_unit}")
}

/// Format a transfer speed using decimal (1000-based) units.
///
/// # Examples
///
/// ```
/// use rich_rust::filesize::decimal_speed;
///
/// assert_eq!(decimal_speed(1_500_000.0), "1.5 MB/s");
/// assert_eq!(decimal_speed(500.0), "500 bytes/s");
/// ```
#[must_use]
pub fn decimal_speed(bytes_per_second: f64) -> String {
    format_speed(bytes_per_second, SizeUnit::Decimal, 1)
}

/// Format a transfer speed using binary (1024-based) units.
///
/// # Examples
///
/// ```
/// use rich_rust::filesize::binary_speed;
///
/// assert_eq!(binary_speed(1_048_576.0), "1.0 MiB/s");
/// assert_eq!(binary_speed(512.0), "512 bytes/s");
/// ```
#[must_use]
pub fn binary_speed(bytes_per_second: f64) -> String {
    format_speed(bytes_per_second, SizeUnit::Binary, 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decimal_bytes() {
        assert_eq!(decimal(0), "0 bytes");
        assert_eq!(decimal(1), "1 bytes");
        assert_eq!(decimal(999), "999 bytes");
    }

    #[test]
    fn test_decimal_kilobytes() {
        assert_eq!(decimal(1_000), "1.0 kB");
        assert_eq!(decimal(1_500), "1.5 kB");
        assert_eq!(decimal(999_000), "999.0 kB");
    }

    #[test]
    fn test_decimal_megabytes() {
        assert_eq!(decimal(1_000_000), "1.0 MB");
        assert_eq!(decimal(1_500_000), "1.5 MB");
        assert_eq!(decimal(999_000_000), "999.0 MB");
    }

    #[test]
    fn test_decimal_gigabytes() {
        assert_eq!(decimal(1_000_000_000), "1.0 GB");
        assert_eq!(decimal(1_500_000_000), "1.5 GB");
    }

    #[test]
    fn test_decimal_terabytes() {
        assert_eq!(decimal(1_000_000_000_000), "1.0 TB");
        assert_eq!(decimal(1_500_000_000_000), "1.5 TB");
    }

    #[test]
    fn test_binary_bytes() {
        assert_eq!(binary(0), "0 bytes");
        assert_eq!(binary(1), "1 bytes");
        assert_eq!(binary(1023), "1023 bytes");
    }

    #[test]
    fn test_binary_kibibytes() {
        assert_eq!(binary(1_024), "1.0 KiB");
        assert_eq!(binary(1_536), "1.5 KiB");
    }

    #[test]
    fn test_binary_mebibytes() {
        assert_eq!(binary(1_048_576), "1.0 MiB");
        assert_eq!(binary(1_572_864), "1.5 MiB");
    }

    #[test]
    fn test_binary_gibibytes() {
        assert_eq!(binary(1_073_741_824), "1.0 GiB");
        assert_eq!(binary(1_610_612_736), "1.5 GiB");
    }

    #[test]
    fn test_precision() {
        assert_eq!(decimal_with_precision(1_234_567, 0), "1 MB");
        assert_eq!(decimal_with_precision(1_234_567, 1), "1.2 MB");
        assert_eq!(decimal_with_precision(1_234_567, 2), "1.23 MB");
        assert_eq!(decimal_with_precision(1_234_567, 3), "1.235 MB");
    }

    #[test]
    fn test_negative_size() {
        assert_eq!(format_size(-1_000, SizeUnit::Decimal, 1), "-1.0 kB");
        assert_eq!(format_size(-1_500_000, SizeUnit::Decimal, 1), "-1.5 MB");
    }

    #[test]
    fn test_decimal_speed() {
        assert_eq!(decimal_speed(500.0), "500 bytes/s");
        assert_eq!(decimal_speed(1_500_000.0), "1.5 MB/s");
        assert_eq!(decimal_speed(1_000_000_000.0), "1.0 GB/s");
    }

    #[test]
    fn test_binary_speed() {
        assert_eq!(binary_speed(512.0), "512 bytes/s");
        assert_eq!(binary_speed(1_048_576.0), "1.0 MiB/s");
        assert_eq!(binary_speed(1_073_741_824.0), "1.0 GiB/s");
    }

    #[test]
    fn test_speed_precision() {
        assert_eq!(format_speed(1_234_567.0, SizeUnit::Decimal, 2), "1.23 MB/s");
        assert_eq!(format_speed(1_234_567.0, SizeUnit::Binary, 2), "1.18 MiB/s");
    }

    #[test]
    fn test_large_sizes() {
        // Petabytes
        assert_eq!(decimal(1_000_000_000_000_000), "1.0 PB");
        assert_eq!(binary(1_125_899_906_842_624), "1.0 PiB");

        // Exabytes
        assert_eq!(decimal(1_000_000_000_000_000_000), "1.0 EB");
        assert_eq!(binary(1_152_921_504_606_846_976), "1.0 EiB");
    }

    #[test]
    fn test_speed_nan_handling() {
        assert_eq!(format_speed(f64::NAN, SizeUnit::Decimal, 1), "NaN");
    }

    #[test]
    fn test_speed_infinity_handling() {
        assert_eq!(format_speed(f64::INFINITY, SizeUnit::Decimal, 1), "∞");
        assert_eq!(format_speed(f64::NEG_INFINITY, SizeUnit::Decimal, 1), "-∞");
    }
}
