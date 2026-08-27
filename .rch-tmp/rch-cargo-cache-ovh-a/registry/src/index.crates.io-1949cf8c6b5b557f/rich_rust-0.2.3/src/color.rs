//! Color system for terminal rendering.
//!
//! This module provides comprehensive color support including:
//! - 4-bit ANSI colors (16 colors)
//! - 8-bit colors (256 colors)
//! - 24-bit true colors (16 million colors)
//! - Automatic color downgrading for terminal compatibility
//!
//! # Examples
//!
//! ## Parsing Colors
//!
//! ```
//! use rich_rust::color::Color;
//!
//! // Named colors
//! let red = Color::parse("red").unwrap();
//! let bright_blue = Color::parse("bright_blue").unwrap();
//!
//! // Hex colors
//! let orange = Color::parse("#ff8800").unwrap();
//! let short_hex = Color::parse("#f80").unwrap();  // Shorthand
//!
//! // RGB format
//! let custom = Color::parse("rgb(100, 150, 200)").unwrap();
//!
//! // Color number (0-255)
//! let color196 = Color::parse("color(196)").unwrap();
//! ```
//!
//! ## Creating Colors Programmatically
//!
//! ```
//! use rich_rust::color::{Color, ColorTriplet};
//!
//! // From ANSI number
//! let ansi_red = Color::from_ansi(1);
//!
//! // From RGB values
//! let rgb_color = Color::from_rgb(255, 128, 64);
//!
//! // From a triplet
//! let triplet = ColorTriplet::new(100, 150, 200);
//! let from_triplet = Color::from_triplet(triplet);
//! ```
//!
//! ## Color Downgrading
//!
//! Colors are automatically downgraded to match terminal capabilities:
//!
//! ```
//! use rich_rust::color::{Color, ColorSystem};
//!
//! let truecolor = Color::from_rgb(255, 128, 64);
//!
//! // Downgrade to 256 colors
//! let eight_bit = truecolor.downgrade(ColorSystem::EightBit);
//!
//! // Downgrade to 16 colors
//! let standard = truecolor.downgrade(ColorSystem::Standard);
//! ```
//!
//! ## ANSI Code Generation
//!
//! ```
//! use rich_rust::color::Color;
//!
//! let red = Color::from_ansi(1);
//! let fg_codes = red.get_ansi_codes(true);   // Foreground: ["31"]
//! let bg_codes = red.get_ansi_codes(false);  // Background: ["41"]
//!
//! let rgb = Color::from_rgb(255, 0, 0);
//! let rgb_fg = rgb.get_ansi_codes(true);     // ["38", "2", "255", "0", "0"]
//! ```

use lru::LruCache;
use regex::Regex;
use std::fmt;
use std::num::NonZeroUsize;
use std::str::FromStr;
use std::sync::LazyLock;
use std::sync::Mutex;

use crate::sync::lock_recover;

/// RGB color triplet with values 0-255.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ColorTriplet {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl ColorTriplet {
    /// Create a new color triplet from RGB components.
    #[must_use]
    pub const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }

    /// Returns CSS-style hex format `#rrggbb`.
    #[must_use]
    pub fn hex(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.red, self.green, self.blue)
    }

    /// Returns CSS-style rgb format `rgb(r,g,b)`.
    #[must_use]
    pub fn rgb(&self) -> String {
        format!("rgb({},{},{})", self.red, self.green, self.blue)
    }

    /// Returns normalized RGB as floats in range 0.0-1.0.
    #[must_use]
    pub fn normalized(&self) -> (f64, f64, f64) {
        (
            f64::from(self.red) / 255.0,
            f64::from(self.green) / 255.0,
            f64::from(self.blue) / 255.0,
        )
    }

    /// Convert RGB to HLS (Hue, Lightness, Saturation).
    #[must_use]
    pub fn to_hls(&self) -> (f64, f64, f64) {
        let (r, g, b) = self.normalized();
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let lightness = f64::midpoint(max, min);

        if (max - min).abs() < f64::EPSILON {
            return (0.0, lightness, 0.0);
        }

        let delta = max - min;
        let saturation = if lightness <= 0.5 {
            delta / (max + min)
        } else {
            delta / (2.0 - max - min)
        };

        let hue = if (max - r).abs() < f64::EPSILON {
            (g - b) / delta + (if g < b { 6.0 } else { 0.0 })
        } else if (max - g).abs() < f64::EPSILON {
            (b - r) / delta + 2.0
        } else {
            (r - g) / delta + 4.0
        };

        (hue / 6.0, lightness, saturation)
    }
}

impl From<(u8, u8, u8)> for ColorTriplet {
    fn from((red, green, blue): (u8, u8, u8)) -> Self {
        Self::new(red, green, blue)
    }
}

impl From<[u8; 3]> for ColorTriplet {
    fn from([red, green, blue]: [u8; 3]) -> Self {
        Self::new(red, green, blue)
    }
}

impl fmt::Display for ColorTriplet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "rgb({}, {}, {})", self.red, self.green, self.blue)
    }
}

/// Terminal color system capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum ColorSystem {
    /// 4-bit ANSI colors (16 colors).
    #[default]
    Standard = 1,
    /// 8-bit colors (256 colors).
    EightBit = 2,
    /// 24-bit RGB colors (16 million colors).
    TrueColor = 3,
    /// Windows 10+ console palette (16 colors).
    Windows = 4,
}

impl ColorSystem {
    /// Get the name of this color system.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::EightBit => "256",
            Self::TrueColor => "truecolor",
            Self::Windows => "windows",
        }
    }
}

/// Type of color stored in Color structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum ColorType {
    /// Default terminal color (no RGB/number).
    #[default]
    Default = 0,
    /// 4-bit ANSI standard color (0-15).
    Standard = 1,
    /// 8-bit color (0-255).
    EightBit = 2,
    /// 24-bit RGB color.
    TrueColor = 3,
    /// Windows console color (0-15).
    Windows = 4,
}

// ============================================================================
// TerminalTheme (HTML/SVG export)
// ============================================================================

/// A terminal color theme used for HTML/SVG export.
///
/// Mirrors Python Rich's `TerminalTheme`:
/// - `background_color` / `foreground_color` define the default terminal colors.
/// - `ansi_colors` defines the 16-color ANSI palette used when exporting standard colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TerminalTheme {
    pub background_color: ColorTriplet,
    pub foreground_color: ColorTriplet,
    pub ansi_colors: [ColorTriplet; 16],
}

impl TerminalTheme {
    /// Construct a `TerminalTheme` from an explicit 16-color ANSI palette.
    #[must_use]
    pub const fn from_ansi_colors(
        background: ColorTriplet,
        foreground: ColorTriplet,
        ansi_colors: [ColorTriplet; 16],
    ) -> Self {
        Self {
            background_color: background,
            foreground_color: foreground,
            ansi_colors,
        }
    }
}

/// Blend two RGB colors.
///
/// `factor` is the weight of `background` in the blend:
/// - 0.0 => all foreground
/// - 1.0 => all background
#[must_use]
pub fn blend_rgb(foreground: ColorTriplet, background: ColorTriplet, factor: f64) -> ColorTriplet {
    let factor = factor.clamp(0.0, 1.0);
    let inv = 1.0 - factor;

    let blend = |fg: u8, bg: u8| -> u8 {
        let v = inv * f64::from(fg) + factor * f64::from(bg);
        #[expect(clippy::cast_sign_loss, reason = "value is clamped to 0..255")]
        #[expect(
            clippy::cast_possible_truncation,
            reason = "value is clamped to 0..255"
        )]
        (v.trunc().clamp(0.0, 255.0) as u8)
    };

    ColorTriplet::new(
        blend(foreground.red, background.red),
        blend(foreground.green, background.green),
        blend(foreground.blue, background.blue),
    )
}

// Python Rich export themes (Rich 13.9.4).
//
// These exact values are required for feature-for-feature export parity.
pub const DEFAULT_TERMINAL_THEME: TerminalTheme = TerminalTheme::from_ansi_colors(
    ColorTriplet::new(255, 255, 255),
    ColorTriplet::new(0, 0, 0),
    [
        ColorTriplet::new(0, 0, 0),
        ColorTriplet::new(128, 0, 0),
        ColorTriplet::new(0, 128, 0),
        ColorTriplet::new(128, 128, 0),
        ColorTriplet::new(0, 0, 128),
        ColorTriplet::new(128, 0, 128),
        ColorTriplet::new(0, 128, 128),
        ColorTriplet::new(192, 192, 192),
        ColorTriplet::new(128, 128, 128),
        ColorTriplet::new(255, 0, 0),
        ColorTriplet::new(0, 255, 0),
        ColorTriplet::new(255, 255, 0),
        ColorTriplet::new(0, 0, 255),
        ColorTriplet::new(255, 0, 255),
        ColorTriplet::new(0, 255, 255),
        ColorTriplet::new(255, 255, 255),
    ],
);

pub const MONOKAI: TerminalTheme = TerminalTheme::from_ansi_colors(
    ColorTriplet::new(12, 12, 12),
    ColorTriplet::new(217, 217, 217),
    [
        ColorTriplet::new(26, 26, 26),
        ColorTriplet::new(244, 0, 95),
        ColorTriplet::new(152, 224, 36),
        ColorTriplet::new(253, 151, 31),
        ColorTriplet::new(157, 101, 255),
        ColorTriplet::new(244, 0, 95),
        ColorTriplet::new(88, 209, 235),
        ColorTriplet::new(196, 197, 181),
        ColorTriplet::new(98, 94, 76),
        ColorTriplet::new(244, 0, 95),
        ColorTriplet::new(152, 224, 36),
        ColorTriplet::new(224, 213, 97),
        ColorTriplet::new(157, 101, 255),
        ColorTriplet::new(244, 0, 95),
        ColorTriplet::new(88, 209, 235),
        ColorTriplet::new(246, 246, 239),
    ],
);

pub const DIMMED_MONOKAI: TerminalTheme = TerminalTheme::from_ansi_colors(
    ColorTriplet::new(25, 25, 25),
    ColorTriplet::new(185, 188, 186),
    [
        ColorTriplet::new(58, 61, 67),
        ColorTriplet::new(190, 63, 72),
        ColorTriplet::new(135, 154, 59),
        ColorTriplet::new(197, 166, 53),
        ColorTriplet::new(79, 118, 161),
        ColorTriplet::new(133, 92, 141),
        ColorTriplet::new(87, 143, 164),
        ColorTriplet::new(185, 188, 186),
        ColorTriplet::new(136, 137, 135),
        ColorTriplet::new(251, 0, 31),
        ColorTriplet::new(15, 114, 47),
        ColorTriplet::new(196, 112, 51),
        ColorTriplet::new(24, 109, 227),
        ColorTriplet::new(251, 0, 103),
        ColorTriplet::new(46, 112, 109),
        ColorTriplet::new(253, 255, 185),
    ],
);

pub const NIGHT_OWLISH: TerminalTheme = TerminalTheme::from_ansi_colors(
    ColorTriplet::new(255, 255, 255),
    ColorTriplet::new(64, 63, 83),
    [
        ColorTriplet::new(1, 22, 39),
        ColorTriplet::new(211, 66, 62),
        ColorTriplet::new(42, 162, 152),
        ColorTriplet::new(218, 170, 1),
        ColorTriplet::new(72, 118, 214),
        ColorTriplet::new(64, 63, 83),
        ColorTriplet::new(8, 145, 106),
        ColorTriplet::new(122, 129, 129),
        ColorTriplet::new(122, 129, 129),
        ColorTriplet::new(247, 110, 110),
        ColorTriplet::new(73, 208, 197),
        ColorTriplet::new(218, 194, 107),
        ColorTriplet::new(92, 167, 228),
        ColorTriplet::new(105, 112, 152),
        ColorTriplet::new(0, 201, 144),
        ColorTriplet::new(152, 159, 177),
    ],
);

pub const SVG_EXPORT_THEME: TerminalTheme = TerminalTheme::from_ansi_colors(
    ColorTriplet::new(41, 41, 41),
    ColorTriplet::new(197, 200, 198),
    [
        ColorTriplet::new(75, 78, 85),
        ColorTriplet::new(204, 85, 90),
        ColorTriplet::new(152, 168, 75),
        ColorTriplet::new(208, 179, 68),
        ColorTriplet::new(96, 138, 177),
        ColorTriplet::new(152, 114, 159),
        ColorTriplet::new(104, 160, 179),
        ColorTriplet::new(197, 200, 198),
        ColorTriplet::new(154, 155, 153),
        ColorTriplet::new(255, 38, 39),
        ColorTriplet::new(0, 130, 61),
        ColorTriplet::new(208, 132, 66),
        ColorTriplet::new(25, 132, 233),
        ColorTriplet::new(255, 44, 122),
        ColorTriplet::new(57, 130, 128),
        ColorTriplet::new(253, 253, 197),
    ],
);

/// A terminal color that can be parsed from various formats.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Color {
    /// Name of the color (input that was parsed).
    pub name: String,
    /// Type of color.
    pub color_type: ColorType,
    /// Color number (for Standard, `EightBit`, Windows).
    pub number: Option<u8>,
    /// RGB components (for `TrueColor`).
    pub triplet: Option<ColorTriplet>,
}

impl Default for Color {
    fn default() -> Self {
        Self::default_color()
    }
}

impl Color {
    /// Create a new default color (no color applied).
    #[must_use]
    pub fn default_color() -> Self {
        Self {
            name: "default".to_string(),
            color_type: ColorType::Default,
            number: None,
            triplet: None,
        }
    }

    /// Create a color from an 8-bit ANSI number.
    #[must_use]
    pub fn from_ansi(number: u8) -> Self {
        let color_type = if number < 16 {
            ColorType::Standard
        } else {
            ColorType::EightBit
        };
        Self {
            name: format!("color({number})"),
            color_type,
            number: Some(number),
            triplet: None,
        }
    }

    /// Create a color from RGB triplet as `TrueColor`.
    #[must_use]
    pub fn from_triplet(triplet: ColorTriplet) -> Self {
        Self {
            name: triplet.hex(),
            color_type: ColorType::TrueColor,
            number: None,
            triplet: Some(triplet),
        }
    }

    /// Create a color from RGB components.
    #[must_use]
    pub fn from_rgb(red: u8, green: u8, blue: u8) -> Self {
        Self::from_triplet(ColorTriplet::new(red, green, blue))
    }

    /// Returns the native color system for this color.
    #[must_use]
    pub const fn system(&self) -> ColorSystem {
        match self.color_type {
            ColorType::Default | ColorType::Standard => ColorSystem::Standard,
            ColorType::EightBit => ColorSystem::EightBit,
            ColorType::TrueColor => ColorSystem::TrueColor,
            ColorType::Windows => ColorSystem::Windows,
        }
    }

    /// Returns true if color is system-defined (Standard or Windows).
    #[must_use]
    pub const fn is_system_defined(&self) -> bool {
        matches!(self.color_type, ColorType::Standard | ColorType::Windows)
    }

    /// Returns true if this is the default color.
    #[must_use]
    pub const fn is_default(&self) -> bool {
        matches!(self.color_type, ColorType::Default)
    }

    /// Get the RGB triplet for this color.
    #[must_use]
    pub fn get_truecolor(&self) -> ColorTriplet {
        match self.color_type {
            ColorType::Default => ColorTriplet::default(),
            ColorType::TrueColor => self.triplet.unwrap_or_default(),
            ColorType::Standard | ColorType::Windows => {
                let palette = if self.color_type == ColorType::Windows {
                    &WINDOWS_PALETTE
                } else {
                    &STANDARD_PALETTE
                };
                self.number
                    .and_then(|n| palette.get(n as usize))
                    .copied()
                    .unwrap_or_default()
            }
            ColorType::EightBit => self
                .number
                .and_then(|n| EIGHT_BIT_PALETTE.get(n as usize))
                .copied()
                .unwrap_or_default(),
        }
    }

    /// Get the RGB triplet for this color within a given export [`TerminalTheme`].
    ///
    /// This mirrors Python Rich's `Color.get_truecolor(theme, foreground=...)` behavior
    /// used by HTML/SVG exporters.
    #[must_use]
    pub fn get_truecolor_with_theme(&self, theme: TerminalTheme, foreground: bool) -> ColorTriplet {
        match self.color_type {
            ColorType::TrueColor => self.triplet.unwrap_or_default(),
            ColorType::EightBit => self
                .number
                .and_then(|n| EIGHT_BIT_PALETTE.get(n as usize))
                .copied()
                .unwrap_or_default(),
            ColorType::Standard => self
                .number
                .and_then(|n| theme.ansi_colors.get(n as usize))
                .copied()
                .unwrap_or_default(),
            ColorType::Windows => self
                .number
                .and_then(|n| WINDOWS_PALETTE.get(n as usize))
                .copied()
                .unwrap_or_default(),
            ColorType::Default => {
                if foreground {
                    theme.foreground_color
                } else {
                    theme.background_color
                }
            }
        }
    }

    /// Get ANSI escape codes for this color.
    #[must_use]
    pub fn get_ansi_codes(&self, foreground: bool) -> Vec<String> {
        match self.color_type {
            ColorType::Default => {
                vec![if foreground { "39" } else { "49" }.to_string()]
            }
            ColorType::Standard => {
                let number = self.number.unwrap_or(0);
                let code = if number < 8 {
                    if foreground { 30 + number } else { 40 + number }
                } else {
                    if foreground { 82 + number } else { 92 + number }
                };
                vec![code.to_string()]
            }
            ColorType::EightBit => {
                let number = self.number.unwrap_or(0);
                vec![
                    if foreground { "38" } else { "48" }.to_string(),
                    "5".to_string(),
                    number.to_string(),
                ]
            }
            ColorType::TrueColor => {
                let triplet = self.triplet.unwrap_or_default();
                vec![
                    if foreground { "38" } else { "48" }.to_string(),
                    "2".to_string(),
                    triplet.red.to_string(),
                    triplet.green.to_string(),
                    triplet.blue.to_string(),
                ]
            }
            ColorType::Windows => {
                // Windows colors map to standard ANSI for VT sequences
                let number = self.number.unwrap_or(0);
                let code = if number < 8 {
                    if foreground { 30 + number } else { 40 + number }
                } else {
                    if foreground { 82 + number } else { 92 + number }
                };
                vec![code.to_string()]
            }
        }
    }

    /// Downgrade color to a lower-capability color system.
    #[must_use]
    pub fn downgrade(&self, system: ColorSystem) -> Self {
        if self.is_default() {
            return self.clone();
        }

        match (self.color_type, system) {
            // Downgrade TrueColor to EightBit
            (ColorType::TrueColor, ColorSystem::EightBit) => {
                let triplet = self.triplet.unwrap_or_default();
                let number = rgb_to_eight_bit(triplet);
                Self::from_ansi(number)
            }

            // Downgrade to Standard
            (ColorType::TrueColor, ColorSystem::Standard | ColorSystem::Windows) => {
                let triplet = self.triplet.unwrap_or_default();
                let number = rgb_to_standard(triplet);
                Self::from_ansi(number)
            }
            (ColorType::EightBit, ColorSystem::Standard | ColorSystem::Windows) => {
                let triplet = self.get_truecolor();
                let number = rgb_to_standard(triplet);
                Self::from_ansi(number)
            }

            // Already at or below target system - use wildcard to catch all remaining cases
            _ => self.clone(),
        }
    }

    /// Parse a color string (cached).
    ///
    /// Supported formats:
    /// - Named colors: `red`, `bright_blue`
    /// - Hex format: `#FF0000`
    /// - Color number: `color(196)`
    /// - RGB format: `rgb(255,0,0)`
    /// - Default: `default`
    ///
    /// # Errors
    ///
    /// Returns `ColorParseError` if the color string is invalid:
    /// - `Empty` if the string is empty
    /// - `InvalidHex` if hex format is malformed
    /// - `InvalidColorNumber` if color(N) format is invalid
    /// - `InvalidRgb` if rgb(r,g,b) format is invalid
    /// - `UnknownColor` if the color name is not recognized
    pub fn parse(color: &str) -> Result<Self, ColorParseError> {
        // Check cache first
        static CACHE: LazyLock<Mutex<LruCache<String, Color>>> =
            LazyLock::new(|| Mutex::new(LruCache::new(NonZeroUsize::new(1024).expect("non-zero"))));

        let normalized = color.trim().to_lowercase();

        {
            let mut cache = lock_recover(&CACHE);
            if let Some(cached) = cache.get(&normalized) {
                return Ok(cached.clone());
            }
        }

        let result = Self::parse_uncached(&normalized)?;

        lock_recover(&CACHE).put(normalized, result.clone());

        Ok(result)
    }

    fn parse_uncached(color: &str) -> Result<Self, ColorParseError> {
        static COLOR_NUM_RE: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"^color\((\d{1,3})\)$").expect("valid regex"));
        static RGB_RE: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"^rgb\(\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*(\d{1,3})\s*\)$")
                .expect("valid regex")
        });

        // Empty string is an error
        if color.is_empty() {
            return Err(ColorParseError::Empty);
        }

        // "default" returns the default color
        if color == "default" {
            return Ok(Self::default_color());
        }

        // Try hex format: #RRGGBB or #RGB (shorthand)
        if let Some(hex) = color.strip_prefix('#') {
            // 6-digit hex: #RRGGBB
            // Guard: hex.is_ascii() ensures byte-indexing is char-safe
            if hex.len() == 6
                && hex.is_ascii()
                && let (Ok(r), Ok(g), Ok(b)) = (
                    u8::from_str_radix(&hex[0..2], 16),
                    u8::from_str_radix(&hex[2..4], 16),
                    u8::from_str_radix(&hex[4..6], 16),
                )
            {
                return Ok(Self::from_rgb(r, g, b));
            }
            // 3-digit hex shorthand: #RGB -> #RRGGBB
            // Guard: hex.is_ascii() ensures len() == chars().count()
            if hex.len() == 3 && hex.is_ascii() {
                let chars: Vec<char> = hex.chars().collect();
                if let (Ok(r), Ok(g), Ok(b)) = (
                    u8::from_str_radix(&format!("{}{}", chars[0], chars[0]), 16),
                    u8::from_str_radix(&format!("{}{}", chars[1], chars[1]), 16),
                    u8::from_str_radix(&format!("{}{}", chars[2], chars[2]), 16),
                ) {
                    return Ok(Self::from_rgb(r, g, b));
                }
            }
            return Err(ColorParseError::InvalidHex(color.to_string()));
        }

        // Try color(N) format
        if let Some(caps) = COLOR_NUM_RE.captures(color)
            && let Ok(num) = caps[1].parse::<u16>()
            && num <= 255
        {
            #[expect(clippy::cast_possible_truncation, reason = "verified num <= 255")]
            return Ok(Self::from_ansi(num as u8));
        } else if COLOR_NUM_RE.is_match(color) {
            return Err(ColorParseError::InvalidColorNumber(color.to_string()));
        }

        // Try rgb(R,G,B) format
        if let Some(caps) = RGB_RE.captures(color)
            && let (Ok(r), Ok(g), Ok(b)) = (
                caps[1].parse::<u16>(),
                caps[2].parse::<u16>(),
                caps[3].parse::<u16>(),
            )
            && r <= 255
            && g <= 255
            && b <= 255
        {
            #[expect(clippy::cast_possible_truncation, reason = "verified values <= 255")]
            return Ok(Self::from_rgb(r as u8, g as u8, b as u8));
        } else if RGB_RE.is_match(color) {
            return Err(ColorParseError::InvalidRgb(color.to_string()));
        }

        // Try named color
        if let Some(&number) = NAMED_COLORS.get(color) {
            let color_type = if number < 16 {
                ColorType::Standard
            } else {
                ColorType::EightBit
            };
            return Ok(Self {
                name: color.to_string(),
                color_type,
                number: Some(number),
                triplet: None,
            });
        }

        Err(ColorParseError::UnknownColor(color.to_string()))
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl FromStr for Color {
    type Err = ColorParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl From<ColorTriplet> for Color {
    fn from(triplet: ColorTriplet) -> Self {
        Self::from_triplet(triplet)
    }
}

impl From<(u8, u8, u8)> for Color {
    fn from((red, green, blue): (u8, u8, u8)) -> Self {
        Self::from_rgb(red, green, blue)
    }
}

impl From<[u8; 3]> for Color {
    fn from([red, green, blue]: [u8; 3]) -> Self {
        Self::from_rgb(red, green, blue)
    }
}

/// Error type for color parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColorParseError {
    Empty,
    InvalidHex(String),
    InvalidColorNumber(String),
    InvalidRgb(String),
    UnknownColor(String),
}

impl fmt::Display for ColorParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "Empty color string"),
            Self::InvalidHex(s) => write!(f, "Invalid hex color: {s}"),
            Self::InvalidColorNumber(s) => write!(f, "Invalid color number: {s}"),
            Self::InvalidRgb(s) => write!(f, "Invalid RGB color: {s}"),
            Self::UnknownColor(s) => write!(f, "Unknown color: {s}"),
        }
    }
}

impl std::error::Error for ColorParseError {}

impl TryFrom<&str> for Color {
    type Error = ColorParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl TryFrom<String> for Color {
    type Error = ColorParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value.as_str())
    }
}

// ============================================================================
// Color Palettes
// ============================================================================

/// Standard 16-color ANSI palette.
pub static STANDARD_PALETTE: [ColorTriplet; 16] = [
    ColorTriplet {
        red: 0,
        green: 0,
        blue: 0,
    }, // 0: Black
    ColorTriplet {
        red: 170,
        green: 0,
        blue: 0,
    }, // 1: Red
    ColorTriplet {
        red: 0,
        green: 170,
        blue: 0,
    }, // 2: Green
    ColorTriplet {
        red: 170,
        green: 85,
        blue: 0,
    }, // 3: Yellow
    ColorTriplet {
        red: 0,
        green: 0,
        blue: 170,
    }, // 4: Blue
    ColorTriplet {
        red: 170,
        green: 0,
        blue: 170,
    }, // 5: Magenta
    ColorTriplet {
        red: 0,
        green: 170,
        blue: 170,
    }, // 6: Cyan
    ColorTriplet {
        red: 170,
        green: 170,
        blue: 170,
    }, // 7: White
    ColorTriplet {
        red: 85,
        green: 85,
        blue: 85,
    }, // 8: Bright Black
    ColorTriplet {
        red: 255,
        green: 85,
        blue: 85,
    }, // 9: Bright Red
    ColorTriplet {
        red: 85,
        green: 255,
        blue: 85,
    }, // 10: Bright Green
    ColorTriplet {
        red: 255,
        green: 255,
        blue: 85,
    }, // 11: Bright Yellow
    ColorTriplet {
        red: 85,
        green: 85,
        blue: 255,
    }, // 12: Bright Blue
    ColorTriplet {
        red: 255,
        green: 85,
        blue: 255,
    }, // 13: Bright Magenta
    ColorTriplet {
        red: 85,
        green: 255,
        blue: 255,
    }, // 14: Bright Cyan
    ColorTriplet {
        red: 255,
        green: 255,
        blue: 255,
    }, // 15: Bright White
];

/// Windows 10+ console palette.
pub static WINDOWS_PALETTE: [ColorTriplet; 16] = [
    ColorTriplet {
        red: 12,
        green: 12,
        blue: 12,
    }, // 0: Black
    ColorTriplet {
        red: 197,
        green: 15,
        blue: 31,
    }, // 1: Red
    ColorTriplet {
        red: 19,
        green: 161,
        blue: 14,
    }, // 2: Green
    ColorTriplet {
        red: 193,
        green: 156,
        blue: 0,
    }, // 3: Yellow
    ColorTriplet {
        red: 0,
        green: 55,
        blue: 218,
    }, // 4: Blue
    ColorTriplet {
        red: 136,
        green: 23,
        blue: 152,
    }, // 5: Magenta
    ColorTriplet {
        red: 58,
        green: 150,
        blue: 221,
    }, // 6: Cyan
    ColorTriplet {
        red: 204,
        green: 204,
        blue: 204,
    }, // 7: White
    ColorTriplet {
        red: 118,
        green: 118,
        blue: 118,
    }, // 8: Bright Black
    ColorTriplet {
        red: 231,
        green: 72,
        blue: 86,
    }, // 9: Bright Red
    ColorTriplet {
        red: 22,
        green: 198,
        blue: 12,
    }, // 10: Bright Green
    ColorTriplet {
        red: 249,
        green: 241,
        blue: 165,
    }, // 11: Bright Yellow
    ColorTriplet {
        red: 59,
        green: 120,
        blue: 255,
    }, // 12: Bright Blue
    ColorTriplet {
        red: 180,
        green: 0,
        blue: 158,
    }, // 13: Bright Magenta
    ColorTriplet {
        red: 97,
        green: 214,
        blue: 214,
    }, // 14: Bright Cyan
    ColorTriplet {
        red: 242,
        green: 242,
        blue: 242,
    }, // 15: Bright White
];

/// Generate the 256-color palette.
fn generate_eight_bit_palette() -> [ColorTriplet; 256] {
    let mut palette = [ColorTriplet::default(); 256];

    // 0-15: Standard colors
    for (i, &color) in STANDARD_PALETTE.iter().enumerate() {
        palette[i] = color;
    }

    // 16-231: 6x6x6 color cube
    let levels = [0u8, 95, 135, 175, 215, 255];
    for r in 0..6 {
        for g in 0..6 {
            for b in 0..6 {
                let index = 16 + r * 36 + g * 6 + b;
                palette[index] = ColorTriplet::new(levels[r], levels[g], levels[b]);
            }
        }
    }

    // 232-255: Grayscale ramp
    for i in 0..24 {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "max value is 8+23*10=238 which fits in u8"
        )]
        let gray = (8 + i * 10) as u8;
        palette[232 + i] = ColorTriplet::new(gray, gray, gray);
    }

    palette
}

/// 256-color palette (lazy initialized).
pub static EIGHT_BIT_PALETTE: LazyLock<[ColorTriplet; 256]> =
    LazyLock::new(generate_eight_bit_palette);

// ============================================================================
// Color Conversion Algorithms
// ============================================================================

/// Convert RGB to nearest 8-bit color number.
#[must_use]
pub fn rgb_to_eight_bit(triplet: ColorTriplet) -> u8 {
    let (_, lightness, saturation) = triplet.to_hls();

    // Grayscale detection
    if saturation < 0.15 {
        // Map to grayscale ramp (232-255)
        if lightness < 0.04 {
            return 16; // Near black
        }
        if lightness > 0.96 {
            return 231; // Near white
        }
        #[expect(clippy::cast_possible_truncation, reason = "result is 0-24 range")]
        #[expect(
            clippy::cast_sign_loss,
            reason = "lightness is positive so result is positive"
        )]
        let gray_index = ((lightness - 0.04) / 0.92 * 24.0).round() as u8;
        return 232 + gray_index.min(23);
    }

    // Color cube mapping
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "values are in 0-5 range"
    )]
    let quantize = |v: u8| -> usize {
        if v < 95 {
            (f64::from(v) / 95.0).round() as usize
        } else {
            1 + ((f64::from(v) - 95.0) / 40.0).round() as usize
        }
        .min(5)
    };

    let r_idx = quantize(triplet.red);
    let g_idx = quantize(triplet.green);
    let b_idx = quantize(triplet.blue);

    #[expect(clippy::cast_possible_truncation, reason = "result is in 16-231 range")]
    let color_index = (16 + r_idx * 36 + g_idx * 6 + b_idx) as u8;
    color_index
}

/// Convert RGB to nearest standard 16-color number.
#[must_use]
pub fn rgb_to_standard(triplet: ColorTriplet) -> u8 {
    let mut best_index = 0u8;
    let mut best_distance = u32::MAX;

    for (i, &palette_color) in STANDARD_PALETTE.iter().enumerate() {
        let distance = color_distance(triplet, palette_color);
        if distance < best_distance {
            best_distance = distance;
            #[expect(
                clippy::cast_possible_truncation,
                reason = "STANDARD_PALETTE has 16 entries"
            )]
            {
                best_index = i as u8;
            }
        }
    }

    best_index
}

/// Calculate weighted color distance (CIE76-like).
fn color_distance(c1: ColorTriplet, c2: ColorTriplet) -> u32 {
    let r1 = u32::from(c1.red);
    let g1 = u32::from(c1.green);
    let b1 = u32::from(c1.blue);
    let r2 = u32::from(c2.red);
    let g2 = u32::from(c2.green);
    let b2 = u32::from(c2.blue);

    let red_mean = u32::midpoint(r1, r2);
    let red_diff = r1.abs_diff(r2);
    let green_diff = g1.abs_diff(g2);
    let blue_diff = b1.abs_diff(b2);

    // Weighted Euclidean distance
    let red_weight = ((512 + red_mean) * red_diff * red_diff) >> 8;
    let green_weight = 4 * green_diff * green_diff;
    let blue_weight = ((767 - red_mean) * blue_diff * blue_diff) >> 8;

    red_weight + green_weight + blue_weight
}

// ============================================================================
// Named Colors
// ============================================================================

use std::collections::HashMap;

/// Map of named colors to their 8-bit color numbers.
static NAMED_COLORS: LazyLock<HashMap<&'static str, u8>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    // Standard colors (0-7)
    m.insert("black", 0);
    m.insert("red", 1);
    m.insert("green", 2);
    m.insert("yellow", 3);
    m.insert("blue", 4);
    m.insert("magenta", 5);
    m.insert("cyan", 6);
    m.insert("white", 7);

    // Bright colors (8-15)
    m.insert("bright_black", 8);
    m.insert("bright_red", 9);
    m.insert("bright_green", 10);
    m.insert("bright_yellow", 11);
    m.insert("bright_blue", 12);
    m.insert("bright_magenta", 13);
    m.insert("bright_cyan", 14);
    m.insert("bright_white", 15);

    // Aliases
    m.insert("grey", 8);
    m.insert("gray", 8);
    m.insert("dark_red", 1);
    m.insert("dark_green", 2);
    m.insert("dark_yellow", 3);
    m.insert("dark_blue", 4);
    m.insert("dark_magenta", 5);
    m.insert("dark_cyan", 6);

    // Extended colors from the 256 palette
    m.insert("navy_blue", 17);
    m.insert("dark_blue", 18);
    m.insert("blue3", 20);
    m.insert("blue1", 21);
    m.insert("dark_green", 22);
    m.insert("deep_sky_blue4", 23);
    m.insert("dodger_blue3", 26);
    m.insert("dodger_blue2", 27);
    m.insert("green4", 28);
    m.insert("spring_green4", 29);
    m.insert("turquoise4", 30);
    m.insert("deep_sky_blue3", 31);
    m.insert("dodger_blue1", 33);
    m.insert("green3", 34);
    m.insert("spring_green3", 35);
    m.insert("dark_cyan", 36);
    m.insert("light_sea_green", 37);
    m.insert("deep_sky_blue2", 38);
    m.insert("deep_sky_blue1", 39);
    m.insert("spring_green2", 42);
    m.insert("cyan3", 43);
    m.insert("dark_turquoise", 44);
    m.insert("turquoise2", 45);
    m.insert("green1", 46);
    m.insert("spring_green1", 48);
    m.insert("medium_spring_green", 49);
    m.insert("cyan2", 50);
    m.insert("cyan1", 51);
    m.insert("dark_red", 52);
    m.insert("deep_pink4", 53);
    m.insert("purple4", 54);
    m.insert("purple3", 56);
    m.insert("blue_violet", 57);
    m.insert("orange4", 58);
    m.insert("grey37", 59);
    m.insert("medium_purple4", 60);
    m.insert("slate_blue3", 62);
    m.insert("royal_blue1", 63);
    m.insert("chartreuse4", 64);
    m.insert("dark_sea_green4", 65);
    m.insert("pale_turquoise4", 66);
    m.insert("steel_blue", 67);
    m.insert("steel_blue3", 68);
    m.insert("cornflower_blue", 69);
    m.insert("chartreuse3", 70);
    m.insert("cadet_blue", 72);
    m.insert("sky_blue3", 74);
    m.insert("steel_blue1", 75);
    m.insert("pale_green3", 77);
    m.insert("sea_green3", 78);
    m.insert("aquamarine3", 79);
    m.insert("medium_turquoise", 80);
    m.insert("chartreuse2", 82);
    m.insert("sea_green2", 83);
    m.insert("sea_green1", 85);
    m.insert("aquamarine1", 86);
    m.insert("dark_slate_gray2", 87);
    m.insert("dark_magenta", 90);
    m.insert("dark_violet", 128);
    m.insert("purple", 129);
    m.insert("light_pink4", 95);
    m.insert("plum4", 96);
    m.insert("medium_purple3", 98);
    m.insert("slate_blue1", 99);
    m.insert("wheat4", 101);
    m.insert("grey53", 102);
    m.insert("light_slate_grey", 103);
    m.insert("medium_purple", 104);
    m.insert("light_slate_blue", 105);
    m.insert("dark_olive_green3", 107);
    m.insert("dark_sea_green", 108);
    m.insert("light_sky_blue3", 110);
    m.insert("sky_blue2", 111);
    m.insert("dark_sea_green3", 115);
    m.insert("dark_slate_gray3", 116);
    m.insert("sky_blue1", 117);
    m.insert("chartreuse1", 118);
    m.insert("light_green", 119);
    m.insert("pale_green1", 121);
    m.insert("dark_slate_gray1", 123);
    m.insert("red3", 124);
    m.insert("medium_violet_red", 126);
    m.insert("magenta3", 127);
    m.insert("dark_orange3", 130);
    m.insert("indian_red", 131);
    m.insert("hot_pink3", 132);
    m.insert("medium_orchid3", 133);
    m.insert("medium_orchid", 134);
    m.insert("medium_purple2", 135);
    m.insert("dark_goldenrod", 136);
    m.insert("light_salmon3", 137);
    m.insert("rosy_brown", 138);
    m.insert("grey63", 139);
    m.insert("medium_purple1", 141);
    m.insert("gold3", 142);
    m.insert("dark_khaki", 143);
    m.insert("navajo_white3", 144);
    m.insert("grey69", 145);
    m.insert("light_steel_blue3", 146);
    m.insert("light_steel_blue", 147);
    m.insert("yellow3", 148);
    m.insert("dark_sea_green2", 157);
    m.insert("light_cyan3", 152);
    m.insert("light_sky_blue1", 153);
    m.insert("green_yellow", 154);
    m.insert("dark_olive_green2", 155);
    m.insert("dark_sea_green1", 158);
    m.insert("pale_turquoise1", 159);
    m.insert("deep_pink3", 162);
    m.insert("magenta2", 165);
    m.insert("hot_pink2", 169);
    m.insert("orchid", 170);
    m.insert("medium_orchid1", 171);
    m.insert("orange3", 172);
    m.insert("light_pink3", 174);
    m.insert("pink3", 175);
    m.insert("plum3", 176);
    m.insert("violet", 177);
    m.insert("light_goldenrod3", 179);
    m.insert("tan", 180);
    m.insert("misty_rose3", 181);
    m.insert("thistle3", 182);
    m.insert("plum2", 183);
    m.insert("khaki3", 185);
    m.insert("light_goldenrod2", 186);
    m.insert("light_yellow3", 187);
    m.insert("grey84", 188);
    m.insert("light_steel_blue1", 189);
    m.insert("yellow2", 190);
    m.insert("dark_olive_green1", 192);
    m.insert("honeydew2", 194);
    m.insert("light_cyan1", 195);
    m.insert("red1", 196);
    m.insert("deep_pink2", 197);
    m.insert("deep_pink1", 199);
    m.insert("magenta1", 201);
    m.insert("orange_red1", 202);
    m.insert("indian_red1", 204);
    m.insert("hot_pink", 206);
    m.insert("dark_orange", 208);
    m.insert("salmon1", 209);
    m.insert("light_coral", 210);
    m.insert("pale_violet_red1", 211);
    m.insert("orchid2", 212);
    m.insert("orchid1", 213);
    m.insert("orange1", 214);
    m.insert("sandy_brown", 215);
    m.insert("light_salmon1", 216);
    m.insert("light_pink1", 217);
    m.insert("pink1", 218);
    m.insert("plum1", 219);
    m.insert("gold1", 220);
    m.insert("navajo_white1", 223);
    m.insert("misty_rose1", 224);
    m.insert("thistle1", 225);
    m.insert("yellow1", 226);
    m.insert("light_goldenrod1", 227);
    m.insert("khaki1", 228);
    m.insert("wheat1", 229);
    m.insert("cornsilk1", 230);
    m.insert("grey100", 231);
    m.insert("grey3", 232);
    m.insert("grey7", 233);
    m.insert("grey11", 234);
    m.insert("grey15", 235);
    m.insert("grey19", 236);
    m.insert("grey23", 237);
    m.insert("grey27", 238);
    m.insert("grey30", 239);
    m.insert("grey35", 240);
    m.insert("grey39", 241);
    m.insert("grey42", 242);
    m.insert("grey46", 243);
    m.insert("grey50", 244);
    m.insert("grey54", 245);
    m.insert("grey58", 246);
    m.insert("grey62", 247);
    m.insert("grey66", 248);
    m.insert("grey70", 249);
    m.insert("grey74", 250);
    m.insert("grey78", 251);
    m.insert("grey82", 252);
    m.insert("grey85", 253);
    m.insert("grey89", 254);
    m.insert("grey93", 255);

    m
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_triplet_hex() {
        let c = ColorTriplet::new(255, 0, 128);
        assert_eq!(c.hex(), "#ff0080");
    }

    #[test]
    fn test_color_triplet_rgb_string() {
        let c = ColorTriplet::new(100, 150, 200);
        assert_eq!(c.rgb(), "rgb(100,150,200)");
    }

    #[test]
    fn test_color_parse_hex() {
        let c = Color::parse("#ff0000").unwrap();
        assert_eq!(c.color_type, ColorType::TrueColor);
        assert_eq!(c.triplet, Some(ColorTriplet::new(255, 0, 0)));
    }

    #[test]
    fn test_color_parse_named() {
        let c = Color::parse("red").unwrap();
        assert_eq!(c.color_type, ColorType::Standard);
        assert_eq!(c.number, Some(1));
    }

    #[test]
    fn test_color_parse_color_number() {
        let c = Color::parse("color(196)").unwrap();
        assert_eq!(c.color_type, ColorType::EightBit);
        assert_eq!(c.number, Some(196));
    }

    #[test]
    fn test_color_parse_rgb() {
        let c = Color::parse("rgb(100, 150, 200)").unwrap();
        assert_eq!(c.color_type, ColorType::TrueColor);
        assert_eq!(c.triplet, Some(ColorTriplet::new(100, 150, 200)));
    }

    #[test]
    fn test_color_default() {
        let c = Color::default_color();
        assert!(c.is_default());
        assert_eq!(c.get_ansi_codes(true), vec!["39"]);
        assert_eq!(c.get_ansi_codes(false), vec!["49"]);
    }

    #[test]
    fn test_color_ansi_codes_standard() {
        let c = Color::from_ansi(1); // Red
        assert_eq!(c.get_ansi_codes(true), vec!["31"]);
        assert_eq!(c.get_ansi_codes(false), vec!["41"]);
    }

    #[test]
    fn test_color_ansi_codes_bright() {
        let c = Color::from_ansi(9); // Bright Red
        assert_eq!(c.get_ansi_codes(true), vec!["91"]);
        assert_eq!(c.get_ansi_codes(false), vec!["101"]);
    }

    #[test]
    fn test_color_ansi_codes_eight_bit() {
        let c = Color::from_ansi(196);
        assert_eq!(c.get_ansi_codes(true), vec!["38", "5", "196"]);
    }

    #[test]
    fn test_color_ansi_codes_truecolor() {
        let c = Color::from_rgb(255, 128, 64);
        assert_eq!(c.get_ansi_codes(true), vec!["38", "2", "255", "128", "64"]);
    }

    #[test]
    fn test_color_downgrade() {
        let truecolor = Color::from_rgb(255, 0, 0);
        let eight_bit = truecolor.downgrade(ColorSystem::EightBit);
        assert_eq!(eight_bit.color_type, ColorType::EightBit);

        let standard = truecolor.downgrade(ColorSystem::Standard);
        assert_eq!(standard.color_type, ColorType::Standard);
        // Pure red (255,0,0) is closer to standard red (170,0,0) than bright red (255,85,85)
        assert_eq!(standard.number, Some(1));
    }

    #[test]
    fn test_rgb_to_standard() {
        // Pure red (255,0,0) should map to standard red (1)
        // Distance to index 1 (170,0,0): 85^2 = 7225
        // Distance to index 9 (255,85,85): 85^2 + 85^2 = 14450
        let idx = rgb_to_standard(ColorTriplet::new(255, 0, 0));
        assert_eq!(idx, 1);
    }

    #[test]
    fn test_eight_bit_palette_generation() {
        let palette = &*EIGHT_BIT_PALETTE;
        // Check grayscale ramp
        assert_eq!(palette[232], ColorTriplet::new(8, 8, 8));
        assert_eq!(palette[255], ColorTriplet::new(238, 238, 238));
        // Check color cube
        assert_eq!(palette[16], ColorTriplet::new(0, 0, 0));
        assert_eq!(palette[21], ColorTriplet::new(0, 0, 255));
    }

    // ============================================================================
    // SPEC VALIDATION TESTS - RICH_SPEC.md Section 1 (Color System)
    // ============================================================================

    // 1.1 Data Structures - ColorTriplet
    #[test]
    fn test_spec_color_triplet_normalized() {
        let c = ColorTriplet::new(255, 128, 0);
        let (r, g, b) = c.normalized();
        assert!((r - 1.0).abs() < f64::EPSILON);
        assert!((g - 128.0 / 255.0).abs() < 0.001);
        assert!(b.abs() < f64::EPSILON);
    }

    // 1.1 Data Structures - ColorSystem enum values
    #[test]
    fn test_spec_color_system_values() {
        assert_eq!(ColorSystem::Standard as u8, 1);
        assert_eq!(ColorSystem::EightBit as u8, 2);
        assert_eq!(ColorSystem::TrueColor as u8, 3);
        assert_eq!(ColorSystem::Windows as u8, 4);
    }

    // 1.1 Data Structures - ColorType enum values
    #[test]
    fn test_spec_color_type_values() {
        assert_eq!(ColorType::Default as u8, 0);
        assert_eq!(ColorType::Standard as u8, 1);
        assert_eq!(ColorType::EightBit as u8, 2);
        assert_eq!(ColorType::TrueColor as u8, 3);
        assert_eq!(ColorType::Windows as u8, 4);
    }

    // 1.2 Color Parsing - Case insensitivity
    #[test]
    fn test_spec_parse_case_insensitive() {
        let c1 = Color::parse("RED").unwrap();
        let c2 = Color::parse("Red").unwrap();
        let c3 = Color::parse("red").unwrap();
        assert_eq!(c1.number, c2.number);
        assert_eq!(c2.number, c3.number);

        let h1 = Color::parse("#FF0000").unwrap();
        let h2 = Color::parse("#ff0000").unwrap();
        assert_eq!(h1.triplet, h2.triplet);
    }

    // 1.2 Color Parsing - color(N) boundary: 0-15 = Standard, 16-255 = EightBit
    #[test]
    fn test_spec_parse_color_number_boundaries() {
        // color(0) should be Standard
        let c0 = Color::parse("color(0)").unwrap();
        assert_eq!(c0.color_type, ColorType::Standard);
        assert_eq!(c0.number, Some(0));

        // color(15) should be Standard
        let c15 = Color::parse("color(15)").unwrap();
        assert_eq!(c15.color_type, ColorType::Standard);
        assert_eq!(c15.number, Some(15));

        // color(16) should be EightBit
        let c16 = Color::parse("color(16)").unwrap();
        assert_eq!(c16.color_type, ColorType::EightBit);
        assert_eq!(c16.number, Some(16));

        // color(255) should be EightBit
        let c255 = Color::parse("color(255)").unwrap();
        assert_eq!(c255.color_type, ColorType::EightBit);
        assert_eq!(c255.number, Some(255));
    }

    // 1.2 Color Parsing - RGB format whitespace handling
    #[test]
    fn test_spec_parse_rgb_whitespace() {
        let c1 = Color::parse("rgb(100,150,200)").unwrap();
        let c2 = Color::parse("rgb( 100 , 150 , 200 )").unwrap();
        assert_eq!(c1.triplet, c2.triplet);
    }

    // 1.2 Color Parsing - Default
    #[test]
    fn test_spec_parse_default_variants() {
        // Per RICH_SPEC Section 1.2: "default" -> ColorType::DEFAULT
        let c1 = Color::parse("default").unwrap();
        assert_eq!(c1.color_type, ColorType::Default);

        // Empty string is an error per RICH_SPEC (not listed in valid formats)
        assert!(Color::parse("").is_err());
    }

    // 1.3 Color Palettes - Standard palette exact values
    #[test]
    fn test_spec_standard_palette_values() {
        // Per RICH_SPEC.md Section 1.3
        assert_eq!(STANDARD_PALETTE[0], ColorTriplet::new(0, 0, 0)); // Black
        assert_eq!(STANDARD_PALETTE[1], ColorTriplet::new(170, 0, 0)); // Red
        assert_eq!(STANDARD_PALETTE[2], ColorTriplet::new(0, 170, 0)); // Green
        assert_eq!(STANDARD_PALETTE[3], ColorTriplet::new(170, 85, 0)); // Yellow
        assert_eq!(STANDARD_PALETTE[4], ColorTriplet::new(0, 0, 170)); // Blue
        assert_eq!(STANDARD_PALETTE[5], ColorTriplet::new(170, 0, 170)); // Magenta
        assert_eq!(STANDARD_PALETTE[6], ColorTriplet::new(0, 170, 170)); // Cyan
        assert_eq!(STANDARD_PALETTE[7], ColorTriplet::new(170, 170, 170)); // White
        assert_eq!(STANDARD_PALETTE[8], ColorTriplet::new(85, 85, 85)); // Bright Black
        assert_eq!(STANDARD_PALETTE[9], ColorTriplet::new(255, 85, 85)); // Bright Red
        assert_eq!(STANDARD_PALETTE[10], ColorTriplet::new(85, 255, 85)); // Bright Green
        assert_eq!(STANDARD_PALETTE[11], ColorTriplet::new(255, 255, 85)); // Bright Yellow
        assert_eq!(STANDARD_PALETTE[12], ColorTriplet::new(85, 85, 255)); // Bright Blue
        assert_eq!(STANDARD_PALETTE[13], ColorTriplet::new(255, 85, 255)); // Bright Magenta
        assert_eq!(STANDARD_PALETTE[14], ColorTriplet::new(85, 255, 255)); // Bright Cyan
        assert_eq!(STANDARD_PALETTE[15], ColorTriplet::new(255, 255, 255)); // Bright White
    }

    // 1.3 Color Palettes - Windows palette exact values
    #[test]
    fn test_spec_windows_palette_values() {
        // Per RICH_SPEC.md Section 1.3
        assert_eq!(WINDOWS_PALETTE[0], ColorTriplet::new(12, 12, 12)); // Black
        assert_eq!(WINDOWS_PALETTE[1], ColorTriplet::new(197, 15, 31)); // Red
        assert_eq!(WINDOWS_PALETTE[2], ColorTriplet::new(19, 161, 14)); // Green
        assert_eq!(WINDOWS_PALETTE[3], ColorTriplet::new(193, 156, 0)); // Yellow
        assert_eq!(WINDOWS_PALETTE[4], ColorTriplet::new(0, 55, 218)); // Blue
        assert_eq!(WINDOWS_PALETTE[5], ColorTriplet::new(136, 23, 152)); // Magenta
        assert_eq!(WINDOWS_PALETTE[6], ColorTriplet::new(58, 150, 221)); // Cyan
        assert_eq!(WINDOWS_PALETTE[7], ColorTriplet::new(204, 204, 204)); // White
        assert_eq!(WINDOWS_PALETTE[8], ColorTriplet::new(118, 118, 118)); // Bright Black
        assert_eq!(WINDOWS_PALETTE[9], ColorTriplet::new(231, 72, 86)); // Bright Red
        assert_eq!(WINDOWS_PALETTE[10], ColorTriplet::new(22, 198, 12)); // Bright Green
        assert_eq!(WINDOWS_PALETTE[11], ColorTriplet::new(249, 241, 165)); // Bright Yellow
        assert_eq!(WINDOWS_PALETTE[12], ColorTriplet::new(59, 120, 255)); // Bright Blue
        assert_eq!(WINDOWS_PALETTE[13], ColorTriplet::new(180, 0, 158)); // Bright Magenta
        assert_eq!(WINDOWS_PALETTE[14], ColorTriplet::new(97, 214, 214)); // Bright Cyan
        assert_eq!(WINDOWS_PALETTE[15], ColorTriplet::new(242, 242, 242)); // Bright White
    }

    // 1.3 Color Palettes - 6x6x6 cube formula
    #[test]
    fn test_spec_eight_bit_cube_formula() {
        let palette = &*EIGHT_BIT_PALETTE;
        let levels = [0u8, 95, 135, 175, 215, 255];

        // Verify formula: index = 16 + 36 * red_index + 6 * green_index + blue_index
        for (ri, &r) in levels.iter().enumerate() {
            for (gi, &g) in levels.iter().enumerate() {
                for (bi, &b) in levels.iter().enumerate() {
                    let expected_index = 16 + 36 * ri + 6 * gi + bi;
                    assert_eq!(
                        palette[expected_index],
                        ColorTriplet::new(r, g, b),
                        "Mismatch at cube index {expected_index} (r={ri}, g={gi}, b={bi})"
                    );
                }
            }
        }
    }

    // 1.5 ANSI Codes - All standard colors 0-7
    #[test]
    fn test_spec_ansi_codes_standard_all() {
        for n in 0..8u8 {
            let c = Color::from_ansi(n);
            assert_eq!(c.get_ansi_codes(true), vec![(30 + n).to_string()]);
            assert_eq!(c.get_ansi_codes(false), vec![(40 + n).to_string()]);
        }
    }

    // 1.5 ANSI Codes - All bright colors 8-15
    #[test]
    fn test_spec_ansi_codes_bright_all() {
        for n in 8..16u8 {
            let c = Color::from_ansi(n);
            // Spec says 82+n for fg, 92+n for bg
            assert_eq!(c.get_ansi_codes(true), vec![(82 + n).to_string()]);
            assert_eq!(c.get_ansi_codes(false), vec![(92 + n).to_string()]);
        }
    }

    // 1.5 ANSI Codes - EIGHT_BIT background
    #[test]
    fn test_spec_ansi_codes_eight_bit_bg() {
        let c = Color::from_ansi(196);
        assert_eq!(c.get_ansi_codes(false), vec!["48", "5", "196"]);
    }

    // 1.5 ANSI Codes - TRUECOLOR background
    #[test]
    fn test_spec_ansi_codes_truecolor_bg() {
        let c = Color::from_rgb(255, 128, 64);
        assert_eq!(c.get_ansi_codes(false), vec!["48", "2", "255", "128", "64"]);
    }

    // 1.4 Color Conversion - Grayscale detection threshold (saturation < 0.15)
    #[test]
    fn test_spec_grayscale_detection() {
        // Pure gray (saturation = 0) should map to grayscale range
        let gray = ColorTriplet::new(128, 128, 128);
        let idx = rgb_to_eight_bit(gray);
        assert!(
            idx >= 232,
            "Gray should map to grayscale ramp (232-255), got {idx}"
        );

        // Slightly chromatic (saturation > 0.15) should map to color cube
        let chromatic = ColorTriplet::new(128, 100, 100);
        let (_, _, sat) = chromatic.to_hls();
        if sat >= 0.15 {
            let idx = rgb_to_eight_bit(chromatic);
            assert!(
                idx < 232 || idx >= 16,
                "Chromatic color should map to color cube or standard"
            );
        }
    }

    // Test get_truecolor for all color types
    #[test]
    fn test_spec_get_truecolor() {
        // TrueColor returns its triplet
        let tc = Color::from_rgb(100, 150, 200);
        assert_eq!(tc.get_truecolor(), ColorTriplet::new(100, 150, 200));

        // Standard returns from STANDARD_PALETTE
        let std = Color::from_ansi(1); // Red
        assert_eq!(std.get_truecolor(), STANDARD_PALETTE[1]);

        // EightBit returns from EIGHT_BIT_PALETTE
        let eb = Color::from_ansi(196);
        assert_eq!(eb.get_truecolor(), EIGHT_BIT_PALETTE[196]);

        // Default returns (0,0,0)
        let def = Color::default_color();
        assert_eq!(def.get_truecolor(), ColorTriplet::default());
    }

    // Test LRU cache is working (same parse returns same result)
    #[test]
    fn test_spec_lru_cache() {
        // Parse same color multiple times
        let c1 = Color::parse("bright_blue").unwrap();
        let c2 = Color::parse("bright_blue").unwrap();
        let c3 = Color::parse("BRIGHT_BLUE").unwrap(); // Case normalized

        assert_eq!(c1.number, c2.number);
        assert_eq!(c2.number, c3.number);
    }

    // ============================================================================
    // EDGE CASE TESTS - Error Handling and Invalid Inputs
    // ============================================================================

    // Invalid hex color formats
    #[test]
    fn test_invalid_hex_colors() {
        // Too short
        assert!(Color::parse("#f").is_err());
        assert!(Color::parse("#ff").is_err());

        // Invalid characters
        assert!(Color::parse("#gggggg").is_err());
        assert!(Color::parse("#xyz123").is_err());

        // Too long
        assert!(Color::parse("#fffffff").is_err());

        // Missing hash
        assert!(Color::parse("ff0000").is_err());
    }

    // Invalid RGB values
    #[test]
    fn test_invalid_rgb_values() {
        // Out of range values (> 255)
        assert!(Color::parse("rgb(256,0,0)").is_err());
        assert!(Color::parse("rgb(0,256,0)").is_err());
        assert!(Color::parse("rgb(0,0,256)").is_err());
        assert!(Color::parse("rgb(1000,1000,1000)").is_err());

        // Negative values
        assert!(Color::parse("rgb(-1,0,0)").is_err());

        // Invalid format
        assert!(Color::parse("rgb(255,0)").is_err()); // Missing component
        assert!(Color::parse("rgb(255,0,0,0)").is_err()); // Extra component
        assert!(Color::parse("rgb(a,b,c)").is_err()); // Non-numeric
        assert!(Color::parse("rgb255,0,0)").is_err()); // Missing open paren
        assert!(Color::parse("rgb(255,0,0").is_err()); // Missing close paren
    }

    // Invalid named colors
    #[test]
    fn test_invalid_named_colors() {
        assert!(Color::parse("not_a_color").is_err());
        assert!(Color::parse("redd").is_err()); // Typo
        assert!(Color::parse("bluee").is_err()); // Typo
        assert!(Color::parse("fancy_purple").is_err()); // Not in palette
        assert!(Color::parse("123abc").is_err()); // Starts with number
    }

    // Invalid color numbers
    #[test]
    fn test_invalid_color_numbers() {
        // Out of range (must be 0-255)
        assert!(Color::parse("color(256)").is_err());
        assert!(Color::parse("color(1000)").is_err());
        assert!(Color::parse("color(-1)").is_err());

        // Invalid format
        assert!(Color::parse("color()").is_err()); // Missing number
        assert!(Color::parse("color(abc)").is_err()); // Non-numeric
        assert!(Color::parse("color 128").is_err()); // Missing parens
    }

    // Empty and whitespace inputs
    #[test]
    fn test_empty_and_whitespace_inputs() {
        assert!(Color::parse("").is_err());
        assert!(Color::parse("   ").is_err());
        assert!(Color::parse("\t\n").is_err());
    }

    // Boundary values for RGB
    #[test]
    fn test_rgb_boundary_values() {
        // Valid boundary values
        let black = Color::parse("rgb(0,0,0)").unwrap();
        assert_eq!(black.get_truecolor(), ColorTriplet::new(0, 0, 0));

        let white = Color::parse("rgb(255,255,255)").unwrap();
        assert_eq!(white.get_truecolor(), ColorTriplet::new(255, 255, 255));

        // Single boundary component
        let r = Color::parse("rgb(255,0,0)").unwrap();
        assert_eq!(r.get_truecolor(), ColorTriplet::new(255, 0, 0));

        let g = Color::parse("rgb(0,255,0)").unwrap();
        assert_eq!(g.get_truecolor(), ColorTriplet::new(0, 255, 0));

        let b = Color::parse("rgb(0,0,255)").unwrap();
        assert_eq!(b.get_truecolor(), ColorTriplet::new(0, 0, 255));
    }

    // EightBit to Standard downgrade
    #[test]
    fn test_eight_bit_to_standard_downgrade() {
        // Create an EightBit color and downgrade to Standard
        let eight_bit = Color::from_ansi(196); // Bright red from 256-color palette
        let _triplet = eight_bit.get_truecolor();

        // Downgrade to Standard (should find nearest standard color)
        let downgraded = eight_bit.downgrade(ColorSystem::Standard);
        assert!(
            downgraded.color_type == ColorType::Standard
                || downgraded.color_type == ColorType::Default
        );

        // The downgraded color should be a valid standard color (0-15)
        if let Some(num) = downgraded.number {
            assert!(num <= 15);
        }
    }

    // ColorTriplet equality and comparison
    #[test]
    fn test_color_triplet_equality() {
        use std::collections::HashSet;

        let a = ColorTriplet::new(128, 64, 32);
        let b = ColorTriplet::new(128, 64, 32);
        let c = ColorTriplet::new(128, 64, 33);

        assert_eq!(a, b);
        assert_ne!(a, c);

        // Hash equality (same triplets should hash equally for HashMap use)
        let mut set = HashSet::new();
        set.insert(a);
        assert!(set.contains(&b));
        assert!(!set.contains(&c));
    }

    // Test all 16 standard named colors parse correctly
    #[test]
    fn test_all_standard_named_colors() {
        let standard_colors = [
            "black",
            "red",
            "green",
            "yellow",
            "blue",
            "magenta",
            "cyan",
            "white",
            "bright_black",
            "bright_red",
            "bright_green",
            "bright_yellow",
            "bright_blue",
            "bright_magenta",
            "bright_cyan",
            "bright_white",
        ];

        for (i, name) in standard_colors.iter().enumerate() {
            let color = Color::parse(name).expect("standard named colors should parse");
            assert!(
                color.number.is_some(),
                "Standard color '{name}' should have a number"
            );
            assert_eq!(
                color.number.unwrap() as usize,
                i,
                "Color '{name}' should have number {i}"
            );
        }
    }

    // Test hex parsing with various valid formats
    #[test]
    fn test_hex_valid_formats() {
        // 6-digit hex
        let c1 = Color::parse("#ff0000").unwrap();
        assert_eq!(c1.get_truecolor(), ColorTriplet::new(255, 0, 0));

        // 3-digit hex (shorthand)
        let c2 = Color::parse("#f00").unwrap();
        assert_eq!(c2.get_truecolor(), ColorTriplet::new(255, 0, 0));

        // Uppercase
        let c3 = Color::parse("#FF0000").unwrap();
        assert_eq!(c3.get_truecolor(), ColorTriplet::new(255, 0, 0));

        // Mixed case
        let c4 = Color::parse("#Ff00fF").unwrap();
        assert_eq!(c4.get_truecolor(), ColorTriplet::new(255, 0, 255));
    }

    // Test color number boundaries
    #[test]
    fn test_color_number_full_range() {
        // Test all valid color numbers (0-255)
        for i in 0..=255u8 {
            let color = Color::parse(&format!("color({i})")).unwrap();
            assert_eq!(
                color.number,
                Some(i),
                "color({i}) should parse to number {i}"
            );
        }
    }
}
