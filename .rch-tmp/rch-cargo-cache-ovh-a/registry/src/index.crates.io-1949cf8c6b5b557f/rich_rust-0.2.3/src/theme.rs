//! Theme support for named styles (Python Rich parity).
//!
//! Python Rich has a global style registry (`Theme`) containing many named styles
//! (e.g. `"rule.line"`, `"table.header"`). `Console.get_style()` will first consult
//! the active theme, and fall back to parsing a style definition if no named style
//! exists.
//!
//! This module ports `rich.theme` + `rich.default_styles` for `rich_rust`.

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use crate::style::{Style, StyleParseError};

static DEFAULT_STYLES: LazyLock<HashMap<String, Style>> = LazyLock::new(|| {
    let mut styles = HashMap::new();

    for (line_no, line) in include_str!("default_styles.tsv").lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let (name, definition) = line
            .split_once('\t')
            .expect("src/default_styles.tsv: expected TAB-separated name + style");

        let style = Style::parse(definition)
            .expect("src/default_styles.tsv: failed to parse style definition");

        let prior = styles.insert(name.to_string(), style);
        assert!(
            prior.is_none(),
            "src/default_styles.tsv:{}: duplicate style key {name:?}",
            line_no + 1
        );
    }

    styles
});

/// A container for style information used by [`crate::console::Console`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    styles: HashMap<String, Style>,
}

impl Theme {
    /// Create a theme from a map of named styles.
    ///
    /// If `inherit` is true, the theme starts with Python Rich's built-in
    /// `DEFAULT_STYLES` and the provided styles override / extend them.
    #[must_use]
    pub fn new(styles: Option<HashMap<String, Style>>, inherit: bool) -> Self {
        let mut merged = if inherit {
            DEFAULT_STYLES.clone()
        } else {
            HashMap::new()
        };

        if let Some(styles) = styles {
            merged.extend(styles);
        }

        Self { styles: merged }
    }

    /// Build a theme from string style definitions (`"bold red"`, `"rule.line"`, etc).
    pub fn from_style_definitions<I, K, V>(styles: I, inherit: bool) -> Result<Self, ThemeError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: AsRef<str>,
    {
        let mut parsed = HashMap::new();
        for (name, definition) in styles {
            let name = name.into();
            let style =
                Style::parse(definition.as_ref()).map_err(|err| ThemeError::InvalidStyle {
                    name: name.clone(),
                    err,
                })?;
            parsed.insert(name, style);
        }
        Ok(Self::new(Some(parsed), inherit))
    }

    /// Get a style by its theme name (exact match).
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Style> {
        self.styles.get(name)
    }

    /// Get all styles in this theme.
    #[must_use]
    pub fn styles(&self) -> &HashMap<String, Style> {
        &self.styles
    }

    /// Get the contents of a `.ini` theme file for this theme (Python Rich compatible).
    #[must_use]
    pub fn config(&self) -> String {
        let mut names: Vec<&str> = self.styles.keys().map(String::as_str).collect();
        names.sort_unstable();

        let mut out = String::from("[styles]\n");
        for name in names {
            let style = self.styles.get(name).expect("key exists");
            out.push_str(name);
            out.push_str(" = ");
            out.push_str(&style.to_string());
            out.push('\n');
        }
        out
    }

    /// Parse a `.ini` theme file string (supports a `[styles]` section).
    ///
    /// This is intentionally minimal but matches the common subset used by Rich.
    pub fn from_ini_str(contents: &str, inherit: bool) -> Result<Self, ThemeError> {
        let mut in_styles = false;
        let mut seen_styles_section = false;
        let mut styles: HashMap<String, Style> = HashMap::new();

        for (line_no, raw_line) in contents.lines().enumerate() {
            let line = raw_line.trim();

            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }

            if line.starts_with('[') && line.ends_with(']') {
                let section_name = line[1..line.len() - 1].trim();
                in_styles = section_name.eq_ignore_ascii_case("styles");
                if in_styles {
                    seen_styles_section = true;
                }
                continue;
            }

            if !in_styles {
                continue;
            }

            let (name, definition) = line
                .split_once('=')
                .or_else(|| line.split_once(':'))
                .ok_or_else(|| ThemeError::InvalidIniLine {
                    line_no: line_no + 1,
                    line: raw_line.to_string(),
                })?;

            // Match Python's configparser default behavior: option keys are lowercased.
            let name = name.trim().to_lowercase();
            if name.is_empty() {
                return Err(ThemeError::InvalidIniLine {
                    line_no: line_no + 1,
                    line: raw_line.to_string(),
                });
            }

            let definition = definition.trim();
            let style = Style::parse(definition).map_err(|err| ThemeError::InvalidStyle {
                name: name.clone(),
                err,
            })?;

            if styles.insert(name.clone(), style).is_some() {
                return Err(ThemeError::DuplicateIniKey {
                    line_no: line_no + 1,
                    name,
                });
            }
        }

        if !seen_styles_section {
            return Err(ThemeError::MissingStylesSection);
        }

        Ok(Self::new(Some(styles), inherit))
    }

    /// Read a `.ini` theme file from disk.
    pub fn read(path: impl AsRef<Path>, inherit: bool) -> Result<Self, ThemeError> {
        let contents = fs::read_to_string(&path).map_err(|err| ThemeError::Io {
            path: path.as_ref().to_path_buf(),
            err,
        })?;
        Self::from_ini_str(&contents, inherit)
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::new(None, true)
    }
}

/// Errors returned by Theme / `ThemeStack` operations.
#[derive(Debug)]
pub enum ThemeError {
    Io {
        path: std::path::PathBuf,
        err: std::io::Error,
    },
    MissingStylesSection,
    InvalidIniLine {
        line_no: usize,
        line: String,
    },
    DuplicateIniKey {
        line_no: usize,
        name: String,
    },
    InvalidStyle {
        name: String,
        err: StyleParseError,
    },
}

impl fmt::Display for ThemeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, err } => {
                write!(f, "failed to read theme file {}: {err}", path.display())
            }
            Self::MissingStylesSection => write!(f, "theme ini is missing a [styles] section"),
            Self::InvalidIniLine { line_no, line } => {
                write!(f, "invalid theme ini line {line_no}: {line:?}")
            }
            Self::DuplicateIniKey { line_no, name } => {
                write!(f, "duplicate theme key {name:?} at line {line_no}")
            }
            Self::InvalidStyle { name, err } => {
                write!(f, "invalid style definition for theme key {name:?}: {err}")
            }
        }
    }
}

impl std::error::Error for ThemeError {}

/// Base exception for theme stack errors (Python Rich parity).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeStackError;

impl fmt::Display for ThemeStackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Unable to pop base theme")
    }
}

impl std::error::Error for ThemeStackError {}

/// A stack of themes (Python Rich parity).
#[derive(Debug, Clone)]
pub struct ThemeStack {
    entries: Vec<HashMap<String, Style>>,
}

impl ThemeStack {
    /// Create a theme stack with a base theme.
    #[must_use]
    pub fn new(theme: Theme) -> Self {
        Self {
            entries: vec![theme.styles],
        }
    }

    /// Get a style by name from the top-most theme.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Style> {
        self.entries.last().and_then(|styles| styles.get(name))
    }

    /// Push a theme on top of the stack.
    pub fn push_theme(&mut self, theme: Theme, inherit: bool) {
        let styles = if inherit {
            let mut merged = self.entries.last().cloned().unwrap_or_else(HashMap::new);
            merged.extend(theme.styles);
            merged
        } else {
            theme.styles
        };
        self.entries.push(styles);
    }

    /// Pop (and discard) the top-most theme.
    pub fn pop_theme(&mut self) -> Result<(), ThemeStackError> {
        if self.entries.len() == 1 {
            return Err(ThemeStackError);
        }
        self.entries.pop();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // DEFAULT_STYLES Tests
    // =========================================================================

    #[test]
    fn test_default_styles_loaded() {
        // DEFAULT_STYLES should be non-empty
        assert!(!DEFAULT_STYLES.is_empty());
    }

    #[test]
    fn test_default_styles_contains_common_keys() {
        // Check for common style keys from Python Rich
        assert!(DEFAULT_STYLES.contains_key("rule.line"));
        assert!(DEFAULT_STYLES.contains_key("table.header"));
    }

    // =========================================================================
    // Theme Creation Tests
    // =========================================================================

    #[test]
    fn test_theme_new_empty_no_inherit() {
        let theme = Theme::new(None, false);
        assert!(theme.styles.is_empty());
    }

    #[test]
    fn test_theme_new_empty_with_inherit() {
        let theme = Theme::new(None, true);
        // Should have all default styles
        assert!(!theme.styles.is_empty());
        assert!(theme.get("rule.line").is_some());
    }

    #[test]
    fn test_theme_new_with_styles_no_inherit() {
        let mut styles = HashMap::new();
        styles.insert("custom".to_string(), Style::new().bold());
        let theme = Theme::new(Some(styles), false);

        assert!(theme.get("custom").is_some());
        assert!(theme.get("rule.line").is_none()); // No default styles
    }

    #[test]
    fn test_theme_new_with_styles_and_inherit() {
        let mut styles = HashMap::new();
        styles.insert("custom".to_string(), Style::new().bold());
        let theme = Theme::new(Some(styles), true);

        assert!(theme.get("custom").is_some());
        assert!(theme.get("rule.line").is_some()); // Has default styles
    }

    #[test]
    fn test_theme_default() {
        let theme = Theme::default();
        // Default theme inherits default styles
        assert!(!theme.styles.is_empty());
    }

    #[test]
    fn default_theme_contains_rule_line() {
        let theme = Theme::default();
        assert!(theme.get("rule.line").is_some());
        assert_eq!(theme.get("rule.line").unwrap().to_string(), "bright_green");
    }

    // =========================================================================
    // Theme from_style_definitions Tests
    // =========================================================================

    #[test]
    fn theme_from_style_definitions_overrides_defaults() {
        let theme =
            Theme::from_style_definitions([("rule.line", "bold red")], true).expect("theme");
        assert_eq!(theme.get("rule.line").unwrap().to_string(), "bold red");
    }

    #[test]
    fn test_from_style_definitions_no_inherit() {
        let theme = Theme::from_style_definitions([("custom", "italic")], false).expect("theme");
        assert!(theme.get("custom").is_some());
        assert!(theme.get("rule.line").is_none());
    }

    #[test]
    fn test_from_style_definitions_multiple() {
        let definitions = [
            ("warning", "bold yellow"),
            ("error", "bold red"),
            ("success", "bold green"),
        ];
        let theme = Theme::from_style_definitions(definitions, false).expect("theme");

        assert_eq!(theme.get("warning").unwrap().to_string(), "bold yellow");
        assert_eq!(theme.get("error").unwrap().to_string(), "bold red");
        assert_eq!(theme.get("success").unwrap().to_string(), "bold green");
    }

    #[test]
    fn test_from_style_definitions_invalid_style() {
        let result = Theme::from_style_definitions([("bad", "not-a-valid-style-xxx")], false);
        assert!(result.is_err());
        if let Err(ThemeError::InvalidStyle { name, .. }) = result {
            assert_eq!(name, "bad");
        } else {
            panic!("Expected InvalidStyle error");
        }
    }

    // =========================================================================
    // Theme Style Lookup Tests
    // =========================================================================

    #[test]
    fn test_theme_get_existing() {
        let theme = Theme::default();
        let style = theme.get("rule.line");
        assert!(style.is_some());
    }

    #[test]
    fn test_theme_get_missing() {
        let theme = Theme::default();
        let style = theme.get("nonexistent.style.name");
        assert!(style.is_none());
    }

    #[test]
    fn test_theme_styles() {
        let theme =
            Theme::from_style_definitions([("a", "bold"), ("b", "italic")], false).expect("theme");
        let styles = theme.styles();
        assert_eq!(styles.len(), 2);
        assert!(styles.contains_key("a"));
        assert!(styles.contains_key("b"));
    }

    // =========================================================================
    // Theme INI Parsing Tests
    // =========================================================================

    #[test]
    fn test_from_ini_str_basic() {
        let ini = "[styles]\nwarning = bold red\n";
        let theme = Theme::from_ini_str(ini, false).expect("theme");
        assert_eq!(theme.get("warning").unwrap().to_string(), "bold red");
    }

    #[test]
    fn theme_from_ini_str_parses_styles_section() {
        let ini = "[styles]\nwarning = bold red\n";
        let theme = Theme::from_ini_str(ini, false).expect("theme");
        assert_eq!(theme.get("warning").unwrap().to_string(), "bold red");
    }

    #[test]
    fn test_from_ini_str_with_comments() {
        let ini = "# Comment line\n[styles]\n; Another comment\ninfo = blue\n";
        let theme = Theme::from_ini_str(ini, false).expect("theme");
        assert_eq!(theme.get("info").unwrap().to_string(), "blue");
    }

    #[test]
    fn test_from_ini_str_colon_separator() {
        let ini = "[styles]\nwarning: bold yellow\n";
        let theme = Theme::from_ini_str(ini, false).expect("theme");
        assert_eq!(theme.get("warning").unwrap().to_string(), "bold yellow");
    }

    #[test]
    fn test_from_ini_str_lowercases_keys() {
        let ini = "[styles]\nWARNING = bold red\n";
        let theme = Theme::from_ini_str(ini, false).expect("theme");
        // Key should be lowercased
        assert!(theme.get("warning").is_some());
        assert!(theme.get("WARNING").is_none());
    }

    #[test]
    fn test_from_ini_str_missing_styles_section() {
        let ini = "warning = bold red\n";
        let result = Theme::from_ini_str(ini, false);
        assert!(matches!(result, Err(ThemeError::MissingStylesSection)));
    }

    #[test]
    fn test_from_ini_str_duplicate_key() {
        let ini = "[styles]\nwarning = bold red\nwarning = italic\n";
        let result = Theme::from_ini_str(ini, false);
        assert!(matches!(result, Err(ThemeError::DuplicateIniKey { .. })));
    }

    #[test]
    fn test_from_ini_str_invalid_line() {
        let ini = "[styles]\nthis is not valid\n";
        let result = Theme::from_ini_str(ini, false);
        assert!(matches!(result, Err(ThemeError::InvalidIniLine { .. })));
    }

    #[test]
    fn test_from_ini_str_empty_name() {
        let ini = "[styles]\n = bold red\n";
        let result = Theme::from_ini_str(ini, false);
        assert!(matches!(result, Err(ThemeError::InvalidIniLine { .. })));
    }

    #[test]
    fn test_from_ini_str_other_sections_ignored() {
        let ini = "[metadata]\nauthor = test\n[styles]\nwarning = bold red\n[other]\nfoo = bar\n";
        let theme = Theme::from_ini_str(ini, false).expect("theme");
        assert!(theme.get("warning").is_some());
        assert!(theme.get("author").is_none());
        assert!(theme.get("foo").is_none());
    }

    #[test]
    fn test_from_ini_str_with_inherit() {
        let ini = "[styles]\ncustom = italic\n";
        let theme = Theme::from_ini_str(ini, true).expect("theme");
        assert!(theme.get("custom").is_some());
        assert!(theme.get("rule.line").is_some()); // Inherited
    }

    // =========================================================================
    // Theme Config Export Tests
    // =========================================================================

    #[test]
    fn theme_config_roundtrip_has_styles_section() {
        let theme = Theme::from_style_definitions([("warning", "bold red")], false).expect("theme");
        let config = theme.config();
        assert!(config.starts_with("[styles]\n"));
        assert!(config.contains("warning = bold red\n"));
    }

    #[test]
    fn test_config_sorted_keys() {
        let theme = Theme::from_style_definitions([("zebra", "bold"), ("alpha", "italic")], false)
            .expect("theme");
        let config = theme.config();
        let alpha_pos = config.find("alpha").expect("alpha");
        let zebra_pos = config.find("zebra").expect("zebra");
        assert!(
            alpha_pos < zebra_pos,
            "Keys should be sorted alphabetically"
        );
    }

    #[test]
    fn test_config_roundtrip() {
        let original =
            Theme::from_style_definitions([("warning", "bold yellow"), ("error", "red")], false)
                .expect("theme");
        let config = original.config();
        let parsed = Theme::from_ini_str(&config, false).expect("parsed");

        assert_eq!(
            parsed.get("warning").unwrap().to_string(),
            original.get("warning").unwrap().to_string()
        );
        assert_eq!(
            parsed.get("error").unwrap().to_string(),
            original.get("error").unwrap().to_string()
        );
    }

    // =========================================================================
    // Theme File Read Tests
    // =========================================================================

    #[test]
    fn test_read_from_file() {
        use std::fs;

        // Create a temp file for testing
        let temp_path = std::env::temp_dir().join("rich_rust_theme_test.ini");
        fs::write(&temp_path, "[styles]\ncustom = bold underline\n").expect("write temp file");

        let theme = Theme::read(&temp_path, false).expect("theme");
        assert_eq!(theme.get("custom").unwrap().to_string(), "bold underline");

        // Cleanup
        let _ = fs::remove_file(&temp_path);
    }

    #[test]
    fn test_read_nonexistent_file() {
        let result = Theme::read("/nonexistent/path/to/theme.ini", false);
        assert!(matches!(result, Err(ThemeError::Io { .. })));
    }

    // =========================================================================
    // ThemeError Tests
    // =========================================================================

    #[test]
    fn test_theme_error_display_io() {
        let err = ThemeError::Io {
            path: std::path::PathBuf::from("/test/path"),
            err: std::io::Error::new(std::io::ErrorKind::NotFound, "not found"),
        };
        let msg = err.to_string();
        assert!(msg.contains("/test/path"));
        assert!(msg.contains("not found"));
    }

    #[test]
    fn test_theme_error_display_missing_section() {
        let err = ThemeError::MissingStylesSection;
        assert!(err.to_string().contains("[styles]"));
    }

    #[test]
    fn test_theme_error_display_invalid_line() {
        let err = ThemeError::InvalidIniLine {
            line_no: 5,
            line: "bad line".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains('5'));
        assert!(msg.contains("bad line"));
    }

    #[test]
    fn test_theme_error_display_duplicate_key() {
        let err = ThemeError::DuplicateIniKey {
            line_no: 10,
            name: "warning".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("10"));
        assert!(msg.contains("warning"));
    }

    #[test]
    fn test_theme_error_display_invalid_style() {
        let err = ThemeError::InvalidStyle {
            name: "test".to_string(),
            err: StyleParseError::UnknownToken("bad".to_string()),
        };
        let msg = err.to_string();
        assert!(msg.contains("test"));
    }

    // =========================================================================
    // ThemeStack Tests
    // =========================================================================

    #[test]
    fn test_theme_stack_new() {
        let stack = ThemeStack::new(Theme::default());
        assert!(stack.get("rule.line").is_some());
    }

    #[test]
    fn test_theme_stack_get_from_base() {
        let base = Theme::from_style_definitions([("custom", "bold")], false).expect("theme");
        let stack = ThemeStack::new(base);
        assert!(stack.get("custom").is_some());
    }

    #[test]
    fn test_theme_stack_get_missing() {
        let stack = ThemeStack::new(Theme::new(None, false));
        assert!(stack.get("anything").is_none());
    }

    #[test]
    fn theme_stack_pop_base_errors() {
        let mut stack = ThemeStack::new(Theme::default());
        let err = stack.pop_theme().expect_err("expected error");
        assert_eq!(err.to_string(), "Unable to pop base theme");
    }

    #[test]
    fn theme_stack_push_and_pop() {
        let mut stack = ThemeStack::new(Theme::default());
        let theme = Theme::from_style_definitions([("warning", "bold red")], false).expect("theme");
        stack.push_theme(theme, true);
        assert_eq!(stack.get("warning").unwrap().to_string(), "bold red");
        stack.pop_theme().expect("pop theme");
    }

    #[test]
    fn test_theme_stack_push_with_inherit() {
        let base = Theme::from_style_definitions([("base_style", "bold")], false).expect("base");
        let mut stack = ThemeStack::new(base);

        let overlay =
            Theme::from_style_definitions([("overlay_style", "italic")], false).expect("overlay");
        stack.push_theme(overlay, true);

        // Both should be accessible
        assert!(stack.get("base_style").is_some());
        assert!(stack.get("overlay_style").is_some());
    }

    #[test]
    fn test_theme_stack_push_without_inherit() {
        let base = Theme::from_style_definitions([("base_style", "bold")], false).expect("base");
        let mut stack = ThemeStack::new(base);

        let overlay =
            Theme::from_style_definitions([("overlay_style", "italic")], false).expect("overlay");
        stack.push_theme(overlay, false);

        // Only overlay should be accessible
        assert!(stack.get("base_style").is_none());
        assert!(stack.get("overlay_style").is_some());
    }

    #[test]
    fn test_theme_stack_push_overrides() {
        let base = Theme::from_style_definitions([("shared", "bold")], false).expect("base");
        let mut stack = ThemeStack::new(base);

        let overlay =
            Theme::from_style_definitions([("shared", "italic")], false).expect("overlay");
        stack.push_theme(overlay, true);

        // Overlay should override
        assert_eq!(stack.get("shared").unwrap().to_string(), "italic");

        stack.pop_theme().expect("pop");
        // Back to base
        assert_eq!(stack.get("shared").unwrap().to_string(), "bold");
    }

    #[test]
    fn test_theme_stack_multiple_push_pop() {
        let base = Theme::from_style_definitions([("level", "dim")], false).expect("base");
        let mut stack = ThemeStack::new(base);

        let layer1 = Theme::from_style_definitions([("level", "italic")], false).expect("layer1");
        let layer2 = Theme::from_style_definitions([("level", "bold")], false).expect("layer2");

        stack.push_theme(layer1, false);
        assert_eq!(stack.get("level").unwrap().to_string(), "italic");

        stack.push_theme(layer2, false);
        assert_eq!(stack.get("level").unwrap().to_string(), "bold");

        stack.pop_theme().expect("pop");
        stack.pop_theme().expect("pop");

        assert_eq!(stack.get("level").unwrap().to_string(), "dim");
    }

    // =========================================================================
    // ThemeStackError Tests
    // =========================================================================

    #[test]
    fn test_theme_stack_error_display() {
        let err = ThemeStackError;
        assert_eq!(err.to_string(), "Unable to pop base theme");
    }

    #[test]
    fn test_theme_stack_error_eq() {
        let err1 = ThemeStackError;
        let err2 = ThemeStackError;
        assert_eq!(err1, err2);
    }

    // =========================================================================
    // Theme Clone/Eq Tests
    // =========================================================================

    #[test]
    fn test_theme_clone() {
        let theme = Theme::from_style_definitions([("test", "bold")], false).expect("theme");
        let cloned = theme.clone();
        assert_eq!(theme, cloned);
    }

    #[test]
    fn test_theme_eq() {
        let theme1 = Theme::from_style_definitions([("test", "bold")], false).expect("theme1");
        let theme2 = Theme::from_style_definitions([("test", "bold")], false).expect("theme2");
        assert_eq!(theme1, theme2);
    }

    #[test]
    fn test_theme_ne() {
        let theme1 = Theme::from_style_definitions([("test", "bold")], false).expect("theme1");
        let theme2 = Theme::from_style_definitions([("test", "italic")], false).expect("theme2");
        assert_ne!(theme1, theme2);
    }

    // =========================================================================
    // std::error::Error Trait Tests
    // =========================================================================

    #[test]
    fn test_theme_error_is_std_error() {
        let err: Box<dyn std::error::Error> = Box::new(ThemeError::MissingStylesSection);
        // ThemeError implements std::error::Error with no source
        assert!(err.source().is_none());
        assert_ne!(err.to_string(), "");
    }

    #[test]
    fn test_theme_error_io_as_std_error() {
        let err: Box<dyn std::error::Error> = Box::new(ThemeError::Io {
            path: std::path::PathBuf::from("/tmp/test"),
            err: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"),
        });
        assert!(err.source().is_none());
        assert!(err.to_string().contains("denied"));
    }

    #[test]
    fn test_theme_stack_error_is_std_error() {
        let err: Box<dyn std::error::Error> = Box::new(ThemeStackError);
        assert!(err.source().is_none());
        assert_eq!(err.to_string(), "Unable to pop base theme");
    }

    // =========================================================================
    // Theme Edge Case Tests
    // =========================================================================

    #[test]
    fn test_theme_new_some_empty_map() {
        // Some(empty HashMap) should behave like None
        let theme_none = Theme::new(None, false);
        let theme_empty = Theme::new(Some(HashMap::new()), false);
        assert_eq!(theme_none, theme_empty);
    }

    #[test]
    fn test_theme_new_some_empty_map_with_inherit() {
        let theme_none = Theme::new(None, true);
        let theme_empty = Theme::new(Some(HashMap::new()), true);
        assert_eq!(theme_none, theme_empty);
    }

    #[test]
    fn test_from_style_definitions_empty_with_inherit() {
        let theme =
            Theme::from_style_definitions(std::iter::empty::<(&str, &str)>(), true).expect("theme");
        // Should contain default styles with no extras
        assert!(theme.get("rule.line").is_some());
        assert_eq!(theme, Theme::default());
    }

    #[test]
    fn test_from_style_definitions_empty_no_inherit() {
        let theme = Theme::from_style_definitions(std::iter::empty::<(&str, &str)>(), false)
            .expect("theme");
        assert!(theme.styles().is_empty());
    }

    // =========================================================================
    // Config Export Edge Cases
    // =========================================================================

    #[test]
    fn test_config_empty_theme() {
        let theme = Theme::new(None, false);
        let config = theme.config();
        assert_eq!(config, "[styles]\n");
    }

    #[test]
    fn test_config_roundtrip_many_styles() {
        let defs = [
            ("alpha", "bold"),
            ("beta", "italic"),
            ("gamma", "underline"),
            ("delta", "dim"),
            ("epsilon", "red"),
        ];
        let original = Theme::from_style_definitions(defs, false).expect("theme");
        let config = original.config();
        let parsed = Theme::from_ini_str(&config, false).expect("parsed");
        assert_eq!(original.styles().len(), parsed.styles().len());
        for (name, style) in original.styles() {
            assert_eq!(
                parsed.get(name).unwrap().to_string(),
                style.to_string(),
                "style mismatch for key {name:?}"
            );
        }
    }

    // =========================================================================
    // INI Parsing Edge Cases
    // =========================================================================

    #[test]
    fn test_from_ini_str_case_insensitive_section() {
        let ini = "[STYLES]\nwarning = bold red\n";
        let theme = Theme::from_ini_str(ini, false).expect("theme");
        assert_eq!(theme.get("warning").unwrap().to_string(), "bold red");
    }

    #[test]
    fn test_from_ini_str_mixed_case_section() {
        let ini = "[Styles]\nerror = italic\n";
        let theme = Theme::from_ini_str(ini, false).expect("theme");
        assert_eq!(theme.get("error").unwrap().to_string(), "italic");
    }

    #[test]
    fn test_from_ini_str_empty_styles_section() {
        let ini = "[styles]\n";
        let theme = Theme::from_ini_str(ini, false).expect("theme");
        assert!(theme.styles().is_empty());
    }

    #[test]
    fn test_from_ini_str_only_comments_in_styles() {
        let ini = "[styles]\n# Just a comment\n; Another comment\n\n";
        let theme = Theme::from_ini_str(ini, false).expect("theme");
        assert!(theme.styles().is_empty());
    }

    #[test]
    fn test_from_ini_str_whitespace_around_values() {
        let ini = "[styles]\n  warning  =   bold red   \n";
        let theme = Theme::from_ini_str(ini, false).expect("theme");
        assert_eq!(theme.get("warning").unwrap().to_string(), "bold red");
    }

    #[test]
    fn test_from_ini_str_invalid_style_value() {
        let ini = "[styles]\nbad = not-a-valid-style-xxxyyy\n";
        let result = Theme::from_ini_str(ini, false);
        assert!(matches!(result, Err(ThemeError::InvalidStyle { .. })));
        if let Err(ThemeError::InvalidStyle { name, .. }) = result {
            assert_eq!(name, "bad");
        }
    }

    #[test]
    fn test_from_ini_str_duplicate_key_line_no() {
        // Line 1: [styles]
        // Line 2: first = bold
        // Line 3: first = italic (duplicate)
        let ini = "[styles]\nfirst = bold\nfirst = italic\n";
        let result = Theme::from_ini_str(ini, false);
        if let Err(ThemeError::DuplicateIniKey { line_no, name }) = result {
            assert_eq!(name, "first");
            assert_eq!(line_no, 3); // 1-indexed, line 3
        } else {
            panic!("Expected DuplicateIniKey error");
        }
    }

    #[test]
    fn test_from_ini_str_styles_section_after_other() {
        // Styles section is not first
        let ini = "[other]\nfoo = bar\n[styles]\ncustom = bold\n";
        let theme = Theme::from_ini_str(ini, false).expect("theme");
        assert!(theme.get("custom").is_some());
        assert_eq!(theme.styles().len(), 1);
    }

    #[test]
    fn test_from_ini_str_styles_then_other_section() {
        // Entries after leaving [styles] section should not be parsed
        let ini = "[styles]\ncustom = bold\n[other]\nignored = italic\n";
        let theme = Theme::from_ini_str(ini, false).expect("theme");
        assert!(theme.get("custom").is_some());
        assert!(theme.get("ignored").is_none());
        assert_eq!(theme.styles().len(), 1);
    }

    #[test]
    fn test_from_ini_str_section_name_with_whitespace() {
        // Trimmed section name
        let ini = "[  styles  ]\ncustom = bold\n";
        let theme = Theme::from_ini_str(ini, false).expect("theme");
        assert!(theme.get("custom").is_some());
    }

    // =========================================================================
    // ThemeStack Advanced Tests
    // =========================================================================

    #[test]
    fn test_theme_stack_deep_nesting() {
        let base =
            Theme::from_style_definitions([("a", "bold"), ("b", "italic")], false).expect("base");
        let mut stack = ThemeStack::new(base);

        let layer1 =
            Theme::from_style_definitions([("b", "underline"), ("c", "dim")], false).expect("l1");
        stack.push_theme(layer1, true);

        // layer1 inherits: a=bold, b=underline (overridden), c=dim
        assert_eq!(stack.get("a").unwrap().to_string(), "bold");
        assert_eq!(stack.get("b").unwrap().to_string(), "underline");
        assert_eq!(stack.get("c").unwrap().to_string(), "dim");

        let layer2 = Theme::from_style_definitions([("a", "red")], false).expect("l2");
        stack.push_theme(layer2, true);

        // layer2 inherits layer1: a=red (overridden), b=underline, c=dim
        assert_eq!(stack.get("a").unwrap().to_string(), "red");
        assert_eq!(stack.get("b").unwrap().to_string(), "underline");
        assert_eq!(stack.get("c").unwrap().to_string(), "dim");

        let layer3 = Theme::from_style_definitions([("d", "green")], false).expect("l3");
        stack.push_theme(layer3, false); // no inherit

        // layer3: only d=green
        assert!(stack.get("a").is_none());
        assert!(stack.get("b").is_none());
        assert!(stack.get("c").is_none());
        assert_eq!(stack.get("d").unwrap().to_string(), "green");

        // Pop back through
        stack.pop_theme().expect("pop l3");
        assert_eq!(stack.get("a").unwrap().to_string(), "red");
        assert!(stack.get("d").is_none());

        stack.pop_theme().expect("pop l2");
        assert_eq!(stack.get("a").unwrap().to_string(), "bold");
        assert_eq!(stack.get("b").unwrap().to_string(), "underline");

        stack.pop_theme().expect("pop l1");
        assert_eq!(stack.get("a").unwrap().to_string(), "bold");
        assert_eq!(stack.get("b").unwrap().to_string(), "italic");
        assert!(stack.get("c").is_none());

        // Can't pop base
        assert!(stack.pop_theme().is_err());
    }

    #[test]
    fn test_theme_stack_clone() {
        let base = Theme::from_style_definitions([("x", "bold")], false).expect("base");
        let mut stack = ThemeStack::new(base);

        let overlay = Theme::from_style_definitions([("y", "italic")], false).expect("overlay");
        stack.push_theme(overlay, true);

        let cloned = stack.clone();
        assert_eq!(
            cloned.get("x").unwrap().to_string(),
            stack.get("x").unwrap().to_string()
        );
        assert_eq!(
            cloned.get("y").unwrap().to_string(),
            stack.get("y").unwrap().to_string()
        );
    }

    #[test]
    fn test_theme_stack_debug() {
        let base = Theme::from_style_definitions([("a", "bold")], false).expect("base");
        let stack = ThemeStack::new(base);
        let debug_str = format!("{stack:?}");
        assert!(debug_str.contains("ThemeStack"));
        assert!(debug_str.contains("entries"));
    }

    #[test]
    fn test_theme_debug() {
        let theme = Theme::from_style_definitions([("test", "bold")], false).expect("theme");
        let debug_str = format!("{theme:?}");
        assert!(debug_str.contains("Theme"));
        assert!(debug_str.contains("styles"));
    }

    #[test]
    fn test_theme_stack_error_clone_copy() {
        let err = ThemeStackError;
        let cloned = err; // Copy (clippy: don't use .clone() on Copy types)
        let copied = err; // Copy
        assert_eq!(err, cloned);
        assert_eq!(err, copied);
    }

    #[test]
    fn test_theme_stack_error_debug() {
        let err = ThemeStackError;
        let debug_str = format!("{err:?}");
        assert!(debug_str.contains("ThemeStackError"));
    }

    #[test]
    fn test_theme_error_debug() {
        let err = ThemeError::MissingStylesSection;
        let debug_str = format!("{err:?}");
        assert!(debug_str.contains("MissingStylesSection"));
    }

    #[test]
    fn test_theme_override_default_style() {
        // Custom styles should override defaults when inherit=true
        let default_rule = Theme::default().get("rule.line").unwrap().to_string();
        let theme =
            Theme::from_style_definitions([("rule.line", "bold magenta")], true).expect("theme");
        let custom_rule = theme.get("rule.line").unwrap().to_string();
        assert_ne!(default_rule, custom_rule);
        assert_eq!(custom_rule, "bold magenta");
    }

    #[test]
    fn test_theme_ne_different_keys() {
        let theme1 = Theme::from_style_definitions([("a", "bold")], false).expect("t1");
        let theme2 = Theme::from_style_definitions([("b", "bold")], false).expect("t2");
        assert_ne!(theme1, theme2);
    }

    #[test]
    fn test_theme_ne_different_count() {
        let theme1 = Theme::from_style_definitions([("a", "bold")], false).expect("t1");
        let theme2 =
            Theme::from_style_definitions([("a", "bold"), ("b", "italic")], false).expect("t2");
        assert_ne!(theme1, theme2);
    }
}
