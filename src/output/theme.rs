//! Theme system for rich output.
//!
//! This module defines semantic colors, icon sets, and layout styles used by
//! the rich output layer. The implementation is intentionally conservative
//! and favors safe defaults for machine-readable outputs.

use std::str::FromStr;

use rich_rust::color::{Color, ColorSystem, ColorTriplet, ColorType};
use rich_rust::style::Style;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, trace};

use crate::config::Config;

// ============================================================================
// Theme Errors
// ============================================================================

#[derive(Debug, Error)]
pub enum ThemeError {
    #[error("unknown color key: {0}")]
    UnknownColorKey(String),
    #[error("invalid style: {0}")]
    InvalidStyle(String),
    #[error("invalid preset: {0}")]
    InvalidPreset(String),
    #[error("invalid box style: {0}")]
    InvalidBoxStyle(String),
    #[error("invalid tree guide style: {0}")]
    InvalidTreeGuides(String),
    #[error("invalid progress style: {0}")]
    InvalidProgressStyle(String),
    #[error("missing color: {0}")]
    MissingColor(&'static str),
    #[error("spinner frames are empty")]
    EmptySpinnerFrames,
}

// ============================================================================
// Theme Core Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    #[serde(default)]
    pub colors: ThemeColors,
    #[serde(default)]
    pub icons: ThemeIcons,
    pub box_style: BoxStyle,
    pub tree_guides: TreeGuides,
    pub progress_style: ProgressStyle,
    pub is_light_mode: bool,
}

impl Theme {
    #[must_use]
    pub fn default() -> Self {
        ThemePreset::Default.to_theme()
    }

    #[must_use]
    pub fn from_preset(preset: ThemePreset) -> Self {
        preset.to_theme()
    }

    pub fn from_config(config: &Config) -> Result<Self, ThemeError> {
        let _ = config;

        let preset_str = std::env::var("MS_THEME").unwrap_or_else(|_| "auto".to_string());
        let preset = ThemePreset::from_str(&preset_str).unwrap_or(ThemePreset::Auto);

        let mut theme = match preset {
            ThemePreset::Auto => Theme::auto_detect(),
            _ => preset.to_theme(),
        };

        if let Ok(mode) = std::env::var("MS_THEME_MODE") {
            match mode.to_lowercase().as_str() {
                "light" => {
                    theme.is_light_mode = true;
                    theme.colors = theme.colors.for_light_background();
                }
                "dark" => {
                    theme.is_light_mode = false;
                    theme.colors = theme.colors.for_dark_background();
                }
                _ => {}
            }
        }

        let caps = detect_terminal_capabilities();
        let adapted = theme.adapted_for_terminal(&caps);

        debug!(
            preset = %preset_str,
            color_system = ?caps.color_system,
            unicode = caps.supports_unicode,
            "Theme loaded"
        );

        Ok(adapted)
    }

    #[must_use]
    pub fn auto_detect() -> Self {
        match detect_terminal_background() {
            TerminalBackground::Light => ThemePreset::Light.to_theme(),
            TerminalBackground::Dark | TerminalBackground::Unknown => {
                ThemePreset::Default.to_theme()
            }
        }
    }

    #[must_use]
    pub fn with_color_override(mut self, key: &str, style: Style) -> Self {
        let _ = self.colors.set(key, style);
        self
    }

    #[must_use]
    pub fn adapted_for_terminal(&self, caps: &TerminalCapabilities) -> Self {
        let mut theme = match caps.color_system {
            None => self.strip_colors(),
            Some(ColorSystem::TrueColor) => self.clone(),
            Some(ColorSystem::EightBit) => self.downgrade_to_256(),
            Some(ColorSystem::Standard) | Some(ColorSystem::Windows) => self.downgrade_to_16(),
        };

        if !caps.supports_unicode {
            theme = theme.with_ascii_fallback();
        }

        theme
    }

    #[must_use]
    pub fn with_ascii_fallback(&self) -> Self {
        let mut theme = self.clone();
        theme.icons = ThemeIcons::ascii();
        theme.box_style = BoxStyle::Ascii;
        theme.tree_guides = TreeGuides::Ascii;
        theme.progress_style = ProgressStyle::Ascii;
        theme
    }

    #[must_use]
    pub fn validate(&self) -> Result<(), Vec<ThemeError>> {
        let mut errors = Vec::new();

        self.colors.validate_into(&mut errors);

        if self.icons.spinner_frames.is_empty() {
            errors.push(ThemeError::EmptySpinnerFrames);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn strip_colors(&self) -> Self {
        let mut theme = self.clone();
        theme.colors = theme.colors.strip_colors();
        theme
    }

    fn downgrade_to_256(&self) -> Self {
        let mut theme = self.clone();
        theme.colors = theme.colors.downgrade_to_256();
        theme
    }

    fn downgrade_to_16(&self) -> Self {
        let mut theme = self.clone();
        theme.colors = theme.colors.downgrade_to_16();
        theme
    }
}

impl Default for Theme {
    fn default() -> Self {
        Theme::default()
    }
}

// ============================================================================
// Theme Presets
// ============================================================================

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThemePreset {
    Default,
    Minimal,
    Vibrant,
    Monochrome,
    Light,
    Auto,
}

impl ThemePreset {
    #[must_use]
    pub fn to_theme(self) -> Theme {
        match self {
            ThemePreset::Default => Theme {
                name: "default".to_string(),
                colors: ThemeColors::default_dark(),
                icons: ThemeIcons::unicode(),
                box_style: BoxStyle::Rounded,
                tree_guides: TreeGuides::Unicode,
                progress_style: ProgressStyle::Block,
                is_light_mode: false,
            },
            ThemePreset::Minimal => Theme {
                name: "minimal".to_string(),
                colors: ThemeColors::minimal(),
                icons: ThemeIcons::ascii(),
                box_style: BoxStyle::Ascii,
                tree_guides: TreeGuides::Ascii,
                progress_style: ProgressStyle::Ascii,
                is_light_mode: false,
            },
            ThemePreset::Vibrant => Theme {
                name: "vibrant".to_string(),
                colors: ThemeColors::vibrant(),
                icons: ThemeIcons::unicode(),
                box_style: BoxStyle::Rounded,
                tree_guides: TreeGuides::Unicode,
                progress_style: ProgressStyle::Block,
                is_light_mode: false,
            },
            ThemePreset::Monochrome => Theme {
                name: "monochrome".to_string(),
                colors: ThemeColors::monochrome(),
                icons: ThemeIcons::ascii(),
                box_style: BoxStyle::Ascii,
                tree_guides: TreeGuides::Ascii,
                progress_style: ProgressStyle::Ascii,
                is_light_mode: false,
            },
            ThemePreset::Light => Theme {
                name: "light".to_string(),
                colors: ThemeColors::light(),
                icons: ThemeIcons::unicode(),
                box_style: BoxStyle::Rounded,
                tree_guides: TreeGuides::Unicode,
                progress_style: ProgressStyle::Block,
                is_light_mode: true,
            },
            ThemePreset::Auto => Theme::auto_detect(),
        }
    }
}

impl FromStr for ThemePreset {
    type Err = ThemeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let key = normalize_key(s);
        match key.as_str() {
            "default" => Ok(Self::Default),
            "minimal" => Ok(Self::Minimal),
            "vibrant" => Ok(Self::Vibrant),
            "monochrome" => Ok(Self::Monochrome),
            "light" => Ok(Self::Light),
            "auto" => Ok(Self::Auto),
            _ => Err(ThemeError::InvalidPreset(s.to_string())),
        }
    }
}

impl std::fmt::Display for ThemePreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            ThemePreset::Default => "default",
            ThemePreset::Minimal => "minimal",
            ThemePreset::Vibrant => "vibrant",
            ThemePreset::Monochrome => "monochrome",
            ThemePreset::Light => "light",
            ThemePreset::Auto => "auto",
        };
        write!(f, "{name}")
    }
}

// ============================================================================
// Theme Colors
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeColors {
    #[serde(with = "style_serde")]
    pub success: Style,
    #[serde(with = "style_serde")]
    pub error: Style,
    #[serde(with = "style_serde")]
    pub warning: Style,
    #[serde(with = "style_serde")]
    pub info: Style,
    #[serde(with = "style_serde")]
    pub hint: Style,
    #[serde(with = "style_serde")]
    pub debug: Style,

    #[serde(with = "style_serde")]
    pub skill_name: Style,
    #[serde(with = "style_serde")]
    pub tag: Style,
    #[serde(with = "style_serde")]
    pub path: Style,
    #[serde(with = "style_serde")]
    pub url: Style,
    #[serde(with = "style_serde")]
    pub code: Style,
    #[serde(with = "style_serde")]
    pub command: Style,
    #[serde(with = "style_serde")]
    pub version: Style,

    #[serde(with = "style_serde")]
    pub key: Style,
    #[serde(with = "style_serde")]
    pub value: Style,
    #[serde(with = "style_serde")]
    pub number: Style,
    #[serde(with = "style_serde")]
    pub string: Style,
    #[serde(with = "style_serde")]
    pub boolean: Style,
    #[serde(with = "style_serde")]
    pub null: Style,

    #[serde(with = "style_serde")]
    pub header: Style,
    #[serde(with = "style_serde")]
    pub subheader: Style,
    #[serde(with = "style_serde")]
    pub border: Style,
    #[serde(with = "style_serde")]
    pub separator: Style,
    #[serde(with = "style_serde")]
    pub emphasis: Style,
    #[serde(with = "style_serde")]
    pub muted: Style,
    #[serde(with = "style_serde")]
    pub highlight: Style,

    #[serde(with = "style_serde")]
    pub progress_done: Style,
    #[serde(with = "style_serde")]
    pub progress_remaining: Style,
    #[serde(with = "style_serde")]
    pub progress_text: Style,
    #[serde(with = "style_serde")]
    pub spinner: Style,
}

impl ThemeColors {
    #[must_use]
    pub fn default_dark() -> Self {
        Self {
            success: style("bold green"),
            error: style("bold red"),
            warning: style("bold yellow"),
            info: style("bold blue"),
            hint: style("dim cyan"),
            debug: style("dim bright_black"),

            skill_name: style("bold cyan"),
            tag: style("magenta"),
            path: style("dim bright_black"),
            url: style("underline blue"),
            code: style("green"),
            command: style("bold"),
            version: style("cyan"),

            key: style("blue"),
            value: style("white"),
            number: style("cyan"),
            string: style("green"),
            boolean: style("yellow"),
            null: style("dim magenta italic"),

            header: style("bold"),
            subheader: style("bold dim"),
            border: style("dim bright_black"),
            separator: style("dim bright_black"),
            emphasis: style("bold"),
            muted: style("dim"),
            highlight: style("reverse"),

            progress_done: style("green"),
            progress_remaining: style("dim bright_black"),
            progress_text: style("white"),
            spinner: style("cyan"),
        }
    }

    #[must_use]
    pub fn light() -> Self {
        Self::default_dark().for_light_background()
    }

    #[must_use]
    pub fn minimal() -> Self {
        let base = Style::new();
        Self {
            success: base.clone().bold(),
            error: base.clone().bold(),
            warning: base.clone().bold(),
            info: base.clone(),
            hint: base.clone().dim(),
            debug: base.clone().dim(),

            skill_name: base.clone().bold(),
            tag: base.clone(),
            path: base.clone().dim(),
            url: base.clone().underline(),
            code: base.clone(),
            command: base.clone().bold(),
            version: base.clone(),

            key: base.clone(),
            value: base.clone(),
            number: base.clone(),
            string: base.clone(),
            boolean: base.clone(),
            null: base.clone().dim(),

            header: base.clone().bold(),
            subheader: base.clone().bold(),
            border: base.clone(),
            separator: base.clone(),
            emphasis: base.clone().bold(),
            muted: base.clone().dim(),
            highlight: base.clone().reverse(),

            progress_done: base.clone(),
            progress_remaining: base.clone().dim(),
            progress_text: base.clone(),
            spinner: base.clone(),
        }
    }

    #[must_use]
    pub fn vibrant() -> Self {
        Self {
            success: style("bold bright_green"),
            error: style("bold bright_red"),
            warning: style("bold bright_yellow"),
            info: style("bold bright_blue"),
            hint: style("bright_cyan"),
            debug: style("bright_black"),

            skill_name: style("bold bright_cyan"),
            tag: style("bright_magenta"),
            path: style("bright_black"),
            url: style("underline bright_blue"),
            code: style("bright_green"),
            command: style("bold"),
            version: style("bright_cyan"),

            key: style("bright_blue"),
            value: style("bright_white"),
            number: style("bright_cyan"),
            string: style("bright_green"),
            boolean: style("bright_yellow"),
            null: style("dim bright_magenta"),

            header: style("bold"),
            subheader: style("bold dim"),
            border: style("bright_black"),
            separator: style("bright_black"),
            emphasis: style("bold"),
            muted: style("dim"),
            highlight: style("reverse"),

            progress_done: style("bright_green"),
            progress_remaining: style("bright_black"),
            progress_text: style("bright_white"),
            spinner: style("bright_cyan"),
        }
    }

    #[must_use]
    pub fn monochrome() -> Self {
        let base = Style::new();
        Self {
            success: base.clone().bold(),
            error: base.clone().bold(),
            warning: base.clone().bold(),
            info: base.clone(),
            hint: base.clone().dim(),
            debug: base.clone().dim(),

            skill_name: base.clone().bold(),
            tag: base.clone(),
            path: base.clone().dim(),
            url: base.clone().underline(),
            code: base.clone(),
            command: base.clone().bold(),
            version: base.clone(),

            key: base.clone(),
            value: base.clone(),
            number: base.clone(),
            string: base.clone(),
            boolean: base.clone(),
            null: base.clone().dim(),

            header: base.clone().bold(),
            subheader: base.clone().bold(),
            border: base.clone(),
            separator: base.clone(),
            emphasis: base.clone().bold(),
            muted: base.clone().dim(),
            highlight: base.clone().reverse(),

            progress_done: base.clone(),
            progress_remaining: base.clone().dim(),
            progress_text: base.clone(),
            spinner: base.clone(),
        }
    }

    #[must_use]
    pub fn for_light_background(&self) -> Self {
        let mut colors = self.clone();
        colors.value = style("black");
        colors.progress_text = style("black");
        colors.border = style("dim black");
        colors.separator = style("dim black");
        colors.muted = style("dim black");
        colors
    }

    #[must_use]
    pub fn for_dark_background(&self) -> Self {
        self.clone()
    }

    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Style> {
        match normalize_key(key).as_str() {
            "success" => Some(&self.success),
            "error" => Some(&self.error),
            "warning" => Some(&self.warning),
            "info" => Some(&self.info),
            "hint" => Some(&self.hint),
            "debug" => Some(&self.debug),
            "skill_name" => Some(&self.skill_name),
            "tag" => Some(&self.tag),
            "path" => Some(&self.path),
            "url" => Some(&self.url),
            "code" => Some(&self.code),
            "command" => Some(&self.command),
            "version" => Some(&self.version),
            "key" => Some(&self.key),
            "value" => Some(&self.value),
            "number" => Some(&self.number),
            "string" => Some(&self.string),
            "boolean" => Some(&self.boolean),
            "null" => Some(&self.null),
            "header" => Some(&self.header),
            "subheader" => Some(&self.subheader),
            "border" => Some(&self.border),
            "separator" => Some(&self.separator),
            "emphasis" => Some(&self.emphasis),
            "muted" => Some(&self.muted),
            "highlight" => Some(&self.highlight),
            "progress_done" => Some(&self.progress_done),
            "progress_remaining" => Some(&self.progress_remaining),
            "progress_text" => Some(&self.progress_text),
            "spinner" => Some(&self.spinner),
            _ => None,
        }
    }

    pub fn set(&mut self, key: &str, style: Style) -> Result<(), ThemeError> {
        match normalize_key(key).as_str() {
            "success" => self.success = style,
            "error" => self.error = style,
            "warning" => self.warning = style,
            "info" => self.info = style,
            "hint" => self.hint = style,
            "debug" => self.debug = style,
            "skill_name" => self.skill_name = style,
            "tag" => self.tag = style,
            "path" => self.path = style,
            "url" => self.url = style,
            "code" => self.code = style,
            "command" => self.command = style,
            "version" => self.version = style,
            "key" => self.key = style,
            "value" => self.value = style,
            "number" => self.number = style,
            "string" => self.string = style,
            "boolean" => self.boolean = style,
            "null" => self.null = style,
            "header" => self.header = style,
            "subheader" => self.subheader = style,
            "border" => self.border = style,
            "separator" => self.separator = style,
            "emphasis" => self.emphasis = style,
            "muted" => self.muted = style,
            "highlight" => self.highlight = style,
            "progress_done" => self.progress_done = style,
            "progress_remaining" => self.progress_remaining = style,
            "progress_text" => self.progress_text = style,
            "spinner" => self.spinner = style,
            _ => return Err(ThemeError::UnknownColorKey(key.to_string())),
        }
        Ok(())
    }

    pub fn set_str(&mut self, key: &str, style_str: &str) -> Result<(), ThemeError> {
        let style =
            Style::parse(style_str).map_err(|err| ThemeError::InvalidStyle(err.to_string()))?;
        self.set(key, style)
    }

    #[must_use]
    pub fn strip_colors(&self) -> Self {
        self.map_styles(strip_style_colors)
    }

    #[must_use]
    pub fn downgrade_to_256(&self) -> Self {
        self.map_styles(|style| downgrade_style(style, ColorSystem::EightBit))
    }

    #[must_use]
    pub fn downgrade_to_16(&self) -> Self {
        self.map_styles(|style| downgrade_style(style, ColorSystem::Standard))
    }

    fn map_styles<F>(&self, mut f: F) -> Self
    where
        F: FnMut(&Style) -> Style,
    {
        Self {
            success: f(&self.success),
            error: f(&self.error),
            warning: f(&self.warning),
            info: f(&self.info),
            hint: f(&self.hint),
            debug: f(&self.debug),

            skill_name: f(&self.skill_name),
            tag: f(&self.tag),
            path: f(&self.path),
            url: f(&self.url),
            code: f(&self.code),
            command: f(&self.command),
            version: f(&self.version),

            key: f(&self.key),
            value: f(&self.value),
            number: f(&self.number),
            string: f(&self.string),
            boolean: f(&self.boolean),
            null: f(&self.null),

            header: f(&self.header),
            subheader: f(&self.subheader),
            border: f(&self.border),
            separator: f(&self.separator),
            emphasis: f(&self.emphasis),
            muted: f(&self.muted),
            highlight: f(&self.highlight),

            progress_done: f(&self.progress_done),
            progress_remaining: f(&self.progress_remaining),
            progress_text: f(&self.progress_text),
            spinner: f(&self.spinner),
        }
    }

    fn validate_into(&self, errors: &mut Vec<ThemeError>) {
        for (name, style) in [
            ("success", &self.success),
            ("error", &self.error),
            ("warning", &self.warning),
            ("info", &self.info),
            ("hint", &self.hint),
            ("debug", &self.debug),
            ("skill_name", &self.skill_name),
            ("tag", &self.tag),
            ("path", &self.path),
            ("url", &self.url),
            ("code", &self.code),
            ("command", &self.command),
            ("version", &self.version),
            ("key", &self.key),
            ("value", &self.value),
            ("number", &self.number),
            ("string", &self.string),
            ("boolean", &self.boolean),
            ("null", &self.null),
            ("header", &self.header),
            ("subheader", &self.subheader),
            ("border", &self.border),
            ("separator", &self.separator),
            ("emphasis", &self.emphasis),
            ("muted", &self.muted),
            ("highlight", &self.highlight),
            ("progress_done", &self.progress_done),
            ("progress_remaining", &self.progress_remaining),
            ("progress_text", &self.progress_text),
            ("spinner", &self.spinner),
        ] {
            if style.is_null() {
                errors.push(ThemeError::MissingColor(name));
            }
        }
    }
}

impl Default for ThemeColors {
    fn default() -> Self {
        ThemeColors::default_dark()
    }
}

// ============================================================================
// Theme Icons
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IconSet {
    pub unicode: String,
    pub ascii: String,
}

impl IconSet {
    #[must_use]
    pub fn select(&self, use_unicode: bool) -> &str {
        if use_unicode && !self.unicode.is_empty() {
            &self.unicode
        } else {
            &self.ascii
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeIcons {
    pub success: IconSet,
    pub error: IconSet,
    pub warning: IconSet,
    pub info: IconSet,
    pub hint: IconSet,

    pub skill: IconSet,
    pub tag: IconSet,
    pub folder: IconSet,
    pub file: IconSet,
    pub search: IconSet,

    pub loading: IconSet,
    pub done: IconSet,
    pub arrow: IconSet,
    pub bullet: IconSet,

    pub spinner_frames: Vec<String>,
}

impl ThemeIcons {
    #[must_use]
    pub fn get(&self, key: &str, use_unicode: bool) -> &str {
        match normalize_key(key).as_str() {
            "success" => self.success.select(use_unicode),
            "error" => self.error.select(use_unicode),
            "warning" => self.warning.select(use_unicode),
            "info" => self.info.select(use_unicode),
            "hint" => self.hint.select(use_unicode),
            "skill" => self.skill.select(use_unicode),
            "tag" => self.tag.select(use_unicode),
            "folder" => self.folder.select(use_unicode),
            "file" => self.file.select(use_unicode),
            "search" => self.search.select(use_unicode),
            "loading" => self.loading.select(use_unicode),
            "done" => self.done.select(use_unicode),
            "arrow" => self.arrow.select(use_unicode),
            "bullet" => self.bullet.select(use_unicode),
            _ => "",
        }
    }

    #[must_use]
    pub fn unicode() -> Self {
        Self {
            success: IconSet {
                unicode: "\u{2713}".to_string(),
                ascii: "OK".to_string(),
            },
            error: IconSet {
                unicode: "\u{2717}".to_string(),
                ascii: "ERR".to_string(),
            },
            warning: IconSet {
                unicode: "\u{26a0}".to_string(),
                ascii: "WARN".to_string(),
            },
            info: IconSet {
                unicode: "\u{2139}".to_string(),
                ascii: "INFO".to_string(),
            },
            hint: IconSet {
                unicode: "\u{1f4a1}".to_string(),
                ascii: "HINT".to_string(),
            },

            skill: IconSet {
                unicode: "\u{1f4e6}".to_string(),
                ascii: "SKL".to_string(),
            },
            tag: IconSet {
                unicode: "\u{1f3f7}".to_string(),
                ascii: "TAG".to_string(),
            },
            folder: IconSet {
                unicode: "\u{1f4c1}".to_string(),
                ascii: "DIR".to_string(),
            },
            file: IconSet {
                unicode: "\u{1f4c4}".to_string(),
                ascii: "FILE".to_string(),
            },
            search: IconSet {
                unicode: "\u{1f50d}".to_string(),
                ascii: "?".to_string(),
            },

            loading: IconSet {
                unicode: "\u{23f3}".to_string(),
                ascii: "...".to_string(),
            },
            done: IconSet {
                unicode: "\u{2705}".to_string(),
                ascii: "DONE".to_string(),
            },
            arrow: IconSet {
                unicode: "\u{2192}".to_string(),
                ascii: "->".to_string(),
            },
            bullet: IconSet {
                unicode: "\u{2022}".to_string(),
                ascii: "*".to_string(),
            },

            spinner_frames: vec![
                "\u{280b}".to_string(),
                "\u{2819}".to_string(),
                "\u{2839}".to_string(),
                "\u{2838}".to_string(),
                "\u{283c}".to_string(),
                "\u{2834}".to_string(),
                "\u{2826}".to_string(),
                "\u{2827}".to_string(),
                "\u{2807}".to_string(),
                "\u{280f}".to_string(),
            ],
        }
    }

    #[must_use]
    pub fn ascii() -> Self {
        Self {
            success: IconSet {
                unicode: "".to_string(),
                ascii: "OK".to_string(),
            },
            error: IconSet {
                unicode: "".to_string(),
                ascii: "ERR".to_string(),
            },
            warning: IconSet {
                unicode: "".to_string(),
                ascii: "WARN".to_string(),
            },
            info: IconSet {
                unicode: "".to_string(),
                ascii: "INFO".to_string(),
            },
            hint: IconSet {
                unicode: "".to_string(),
                ascii: "HINT".to_string(),
            },

            skill: IconSet {
                unicode: "".to_string(),
                ascii: "SKL".to_string(),
            },
            tag: IconSet {
                unicode: "".to_string(),
                ascii: "TAG".to_string(),
            },
            folder: IconSet {
                unicode: "".to_string(),
                ascii: "DIR".to_string(),
            },
            file: IconSet {
                unicode: "".to_string(),
                ascii: "FILE".to_string(),
            },
            search: IconSet {
                unicode: "".to_string(),
                ascii: "?".to_string(),
            },

            loading: IconSet {
                unicode: "".to_string(),
                ascii: "...".to_string(),
            },
            done: IconSet {
                unicode: "".to_string(),
                ascii: "DONE".to_string(),
            },
            arrow: IconSet {
                unicode: "".to_string(),
                ascii: "->".to_string(),
            },
            bullet: IconSet {
                unicode: "".to_string(),
                ascii: "*".to_string(),
            },

            spinner_frames: vec![
                "-".to_string(),
                "\\".to_string(),
                "|".to_string(),
                "/".to_string(),
            ],
        }
    }

    #[must_use]
    pub fn none() -> Self {
        Self {
            success: IconSet {
                unicode: "".to_string(),
                ascii: "".to_string(),
            },
            error: IconSet {
                unicode: "".to_string(),
                ascii: "".to_string(),
            },
            warning: IconSet {
                unicode: "".to_string(),
                ascii: "".to_string(),
            },
            info: IconSet {
                unicode: "".to_string(),
                ascii: "".to_string(),
            },
            hint: IconSet {
                unicode: "".to_string(),
                ascii: "".to_string(),
            },

            skill: IconSet {
                unicode: "".to_string(),
                ascii: "".to_string(),
            },
            tag: IconSet {
                unicode: "".to_string(),
                ascii: "".to_string(),
            },
            folder: IconSet {
                unicode: "".to_string(),
                ascii: "".to_string(),
            },
            file: IconSet {
                unicode: "".to_string(),
                ascii: "".to_string(),
            },
            search: IconSet {
                unicode: "".to_string(),
                ascii: "".to_string(),
            },

            loading: IconSet {
                unicode: "".to_string(),
                ascii: "".to_string(),
            },
            done: IconSet {
                unicode: "".to_string(),
                ascii: "".to_string(),
            },
            arrow: IconSet {
                unicode: "".to_string(),
                ascii: "".to_string(),
            },
            bullet: IconSet {
                unicode: "".to_string(),
                ascii: "".to_string(),
            },

            spinner_frames: vec!["".to_string()],
        }
    }
}

impl Default for ThemeIcons {
    fn default() -> Self {
        ThemeIcons::unicode()
    }
}

// ============================================================================
// Box, Tree, and Progress Styles
// ============================================================================

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BoxStyle {
    Rounded,
    Square,
    Heavy,
    Double,
    Ascii,
    None,
}

impl BoxStyle {
    #[must_use]
    pub const fn chars(&self) -> BoxChars {
        match self {
            BoxStyle::Rounded => BoxChars {
                top_left: "\u{256d}",
                top_right: "\u{256e}",
                bottom_left: "\u{2570}",
                bottom_right: "\u{256f}",
                horizontal: "\u{2500}",
                vertical: "\u{2502}",
            },
            BoxStyle::Square => BoxChars {
                top_left: "\u{250c}",
                top_right: "\u{2510}",
                bottom_left: "\u{2514}",
                bottom_right: "\u{2518}",
                horizontal: "\u{2500}",
                vertical: "\u{2502}",
            },
            BoxStyle::Heavy => BoxChars {
                top_left: "\u{250f}",
                top_right: "\u{2513}",
                bottom_left: "\u{2517}",
                bottom_right: "\u{251b}",
                horizontal: "\u{2501}",
                vertical: "\u{2503}",
            },
            BoxStyle::Double => BoxChars {
                top_left: "\u{2554}",
                top_right: "\u{2557}",
                bottom_left: "\u{255a}",
                bottom_right: "\u{255d}",
                horizontal: "\u{2550}",
                vertical: "\u{2551}",
            },
            BoxStyle::Ascii => BoxChars {
                top_left: "+",
                top_right: "+",
                bottom_left: "+",
                bottom_right: "+",
                horizontal: "-",
                vertical: "|",
            },
            BoxStyle::None => BoxChars {
                top_left: "",
                top_right: "",
                bottom_left: "",
                bottom_right: "",
                horizontal: "",
                vertical: "",
            },
        }
    }
}

impl Default for BoxStyle {
    fn default() -> Self {
        BoxStyle::Rounded
    }
}

impl FromStr for BoxStyle {
    type Err = ThemeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let key = normalize_key(s);
        match key.as_str() {
            "rounded" => Ok(BoxStyle::Rounded),
            "square" => Ok(BoxStyle::Square),
            "heavy" => Ok(BoxStyle::Heavy),
            "double" => Ok(BoxStyle::Double),
            "ascii" => Ok(BoxStyle::Ascii),
            "none" => Ok(BoxStyle::None),
            _ => Err(ThemeError::InvalidBoxStyle(s.to_string())),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TreeGuides {
    Unicode,
    Rounded,
    Ascii,
    Bold,
}

impl TreeGuides {
    #[must_use]
    pub const fn chars(&self) -> TreeChars {
        match self {
            TreeGuides::Unicode => TreeChars {
                vertical: "\u{2502}",
                branch: "\u{251c}",
                last: "\u{2514}",
                horizontal: "\u{2500}",
            },
            TreeGuides::Rounded => TreeChars {
                vertical: "\u{2502}",
                branch: "\u{251c}",
                last: "\u{2570}",
                horizontal: "\u{2500}",
            },
            TreeGuides::Ascii => TreeChars {
                vertical: "|",
                branch: "+",
                last: "`",
                horizontal: "-",
            },
            TreeGuides::Bold => TreeChars {
                vertical: "\u{2503}",
                branch: "\u{2523}",
                last: "\u{2517}",
                horizontal: "\u{2501}",
            },
        }
    }
}

impl Default for TreeGuides {
    fn default() -> Self {
        TreeGuides::Unicode
    }
}

impl FromStr for TreeGuides {
    type Err = ThemeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let key = normalize_key(s);
        match key.as_str() {
            "unicode" => Ok(TreeGuides::Unicode),
            "rounded" => Ok(TreeGuides::Rounded),
            "ascii" => Ok(TreeGuides::Ascii),
            "bold" => Ok(TreeGuides::Bold),
            _ => Err(ThemeError::InvalidTreeGuides(s.to_string())),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProgressStyle {
    Block,
    Ascii,
    Line,
    Dots,
}

impl ProgressStyle {
    #[must_use]
    pub const fn chars(&self) -> ProgressChars {
        match self {
            ProgressStyle::Block => ProgressChars {
                filled: "\u{2588}",
                empty: "\u{2591}",
            },
            ProgressStyle::Ascii => ProgressChars {
                filled: "#",
                empty: "-",
            },
            ProgressStyle::Line => ProgressChars {
                filled: "\u{2501}",
                empty: "\u{2500}",
            },
            ProgressStyle::Dots => ProgressChars {
                filled: "\u{25cf}",
                empty: "\u{25cb}",
            },
        }
    }
}

impl Default for ProgressStyle {
    fn default() -> Self {
        ProgressStyle::Block
    }
}

impl FromStr for ProgressStyle {
    type Err = ThemeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let key = normalize_key(s);
        match key.as_str() {
            "block" => Ok(ProgressStyle::Block),
            "ascii" => Ok(ProgressStyle::Ascii),
            "line" => Ok(ProgressStyle::Line),
            "dots" => Ok(ProgressStyle::Dots),
            _ => Err(ThemeError::InvalidProgressStyle(s.to_string())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoxChars {
    pub top_left: &'static str,
    pub top_right: &'static str,
    pub bottom_left: &'static str,
    pub bottom_right: &'static str,
    pub horizontal: &'static str,
    pub vertical: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TreeChars {
    pub vertical: &'static str,
    pub branch: &'static str,
    pub last: &'static str,
    pub horizontal: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgressChars {
    pub filled: &'static str,
    pub empty: &'static str,
}

// ============================================================================
// Terminal Background and Capability Detection
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalBackground {
    Light,
    Dark,
    Unknown,
}

#[must_use]
pub fn detect_terminal_background() -> TerminalBackground {
    if let Ok(val) = std::env::var("MS_THEME_MODE") {
        return match val.to_lowercase().as_str() {
            "light" => TerminalBackground::Light,
            "dark" => TerminalBackground::Dark,
            _ => TerminalBackground::Unknown,
        };
    }

    if let Ok(val) = std::env::var("COLORFGBG") {
        let bg = val.split(';').last().or_else(|| val.split(':').last());
        if let Some(code) = bg.and_then(|v| v.parse::<u8>().ok()) {
            if code == 7 || code == 15 || (248..=255).contains(&code) {
                return TerminalBackground::Light;
            }
            return TerminalBackground::Dark;
        }
    }

    if let Ok(profile) = std::env::var("ITERM_PROFILE") {
        if profile.to_lowercase().contains("light") {
            return TerminalBackground::Light;
        }
    }

    TerminalBackground::Dark
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalCapabilities {
    pub color_system: Option<ColorSystem>,
    pub supports_unicode: bool,
}

#[must_use]
pub fn detect_terminal_capabilities() -> TerminalCapabilities {
    let color_system = rich_rust::terminal::detect_color_system();
    let supports_unicode = detect_unicode_support();

    trace!(
        color_system = ?color_system,
        supports_unicode = supports_unicode,
        "Terminal capabilities detected"
    );

    TerminalCapabilities {
        color_system,
        supports_unicode,
    }
}

fn detect_unicode_support() -> bool {
    if env_flag("MS_NO_UNICODE") {
        return false;
    }

    if let Ok(term) = std::env::var("TERM") {
        if term == "dumb" {
            return false;
        }
    }

    for key in ["LC_ALL", "LC_CTYPE", "LANG"] {
        if let Ok(value) = std::env::var(key) {
            let value = value.to_lowercase();
            if value.contains("utf-8") || value.contains("utf8") {
                return true;
            }
        }
    }

    true
}

// ============================================================================
// Helpers
// ============================================================================

fn style(spec: &str) -> Style {
    Style::parse(spec).unwrap_or_else(|_| Style::new())
}

fn normalize_key(value: &str) -> String {
    value.trim().to_lowercase().replace('-', "_")
}

fn strip_style_colors(style: &Style) -> Style {
    let mut stripped = style.clone();
    stripped.color = None;
    stripped.bgcolor = None;

    if stripped.attributes.is_empty() && stripped.link.is_none() && stripped.link_id.is_none() {
        return Style::new();
    }

    stripped
}

fn downgrade_style(style: &Style, target: ColorSystem) -> Style {
    let mut downgraded = style.clone();

    if let Some(color) = &style.color {
        downgraded.color = Some(downgrade_color(color, target));
    }
    if let Some(bg) = &style.bgcolor {
        downgraded.bgcolor = Some(downgrade_color(bg, target));
    }

    downgraded
}

fn downgrade_color(color: &Color, target: ColorSystem) -> Color {
    if color.color_type == ColorType::Default {
        return color.clone();
    }

    match target {
        ColorSystem::TrueColor => color.clone(),
        ColorSystem::EightBit => match color.color_type {
            ColorType::TrueColor => to_standard_color(color),
            _ => color.clone(),
        },
        ColorSystem::Standard | ColorSystem::Windows => match color.color_type {
            ColorType::Standard | ColorType::Windows => color.clone(),
            _ => to_standard_color(color),
        },
    }
}

fn to_standard_color(color: &Color) -> Color {
    let rgb = color.get_truecolor();
    let idx = nearest_ansi_color(rgb);
    Color::from_ansi(idx)
}

fn nearest_ansi_color(color: ColorTriplet) -> u8 {
    const PALETTE: [ColorTriplet; 16] = [
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
    ];

    let mut best = 0;
    let mut best_dist = u32::MAX;
    for (idx, candidate) in PALETTE.iter().enumerate() {
        let dr = i32::from(color.red) - i32::from(candidate.red);
        let dg = i32::from(color.green) - i32::from(candidate.green);
        let db = i32::from(color.blue) - i32::from(candidate.blue);
        let dist = (dr * dr + dg * dg + db * db) as u32;
        if dist < best_dist {
            best_dist = dist;
            best = idx as u8;
        }
    }
    best
}

fn env_flag(key: &str) -> bool {
    std::env::var_os(key).is_some()
}

// ============================================================================
// Serde helpers
// ============================================================================

mod style_serde {
    use super::*;
    use serde::de::Error as _;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S>(style: &Style, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&style.to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Style, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Style::parse(&value).map_err(D::Error::custom)
    }
}
