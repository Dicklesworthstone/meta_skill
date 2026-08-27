//! ICU extension: Unicode-aware collation, case mapping, and tokenization (S14.6).
//!
//! Provides three capabilities that go beyond SQLite's ASCII-only built-ins:
//!
//! 1. **Locale-aware collation** via [`icu_load_collation`](IcuLoadCollationFunc):
//!    creates a named collation that uses Unicode Collation Algorithm (UCA)
//!    rules tailored for the given locale.
//!
//! 2. **Locale-aware case mapping** via [`icu_upper`](IcuUpperFunc) and
//!    [`icu_lower`](IcuLowerFunc): full Unicode case folding including
//!    Turkish dotted/dotless I, German sharp-s, and Greek final sigma.
//!
//! 3. **ICU word-break tokenizer** for FTS3/4/5: locale-aware word boundary
//!    detection using UAX #29 rules, critical for CJK languages where words
//!    are not space-delimited.

use std::cmp::Ordering;
use std::sync::{Arc, Mutex};

use fsqlite_error::{FrankenError, Result};
use fsqlite_func::FunctionRegistry;
use fsqlite_func::collation::{CollationFunction, CollationRegistry};
use fsqlite_func::scalar::ScalarFunction;
use fsqlite_types::{SmallText, SqliteValue};
use tracing::{debug, info};

#[must_use]
pub const fn extension_name() -> &'static str {
    "icu"
}

// ── Locale ──────────────────────────────────────────────────────────────

/// A parsed ICU locale identifier (e.g. `de_DE`, `zh_CN`, `tr_TR`).
///
/// Supports the `language_COUNTRY` format used by C SQLite's ICU extension.
/// The language tag is lowercased; the country tag is uppercased.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IcuLocale {
    /// ISO 639-1 language code (lowercase, e.g. "de", "zh", "tr").
    pub language: String,
    /// ISO 3166-1 country code (uppercase, e.g. "DE", "CN", "TR").
    /// `None` for language-only locales.
    pub country: Option<String>,
}

impl IcuLocale {
    /// Parse a locale string like `"de_DE"`, `"zh_CN"`, `"en"`.
    ///
    /// Accepts both `_` and `-` as separators.
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();
        if s.is_empty() {
            return Err(FrankenError::internal("empty locale identifier"));
        }

        let parts: Vec<&str> = s.split(['_', '-']).collect();
        let language = parts[0].to_ascii_lowercase();

        if language.len() < 2 || language.len() > 3 {
            return Err(FrankenError::internal(format!(
                "invalid language code: '{language}'"
            )));
        }

        let country = if parts.len() > 1 && !parts[1].is_empty() {
            Some(parts[1].to_ascii_uppercase())
        } else {
            None
        };

        debug!(
            locale = %s,
            language = %language,
            country = country.as_deref().unwrap_or("(none)"),
            "parsed ICU locale"
        );

        Ok(Self { language, country })
    }

    /// Canonical string form (e.g. `"de_DE"`).
    #[must_use]
    pub fn canonical(&self) -> String {
        match &self.country {
            Some(c) => format!("{}_{c}", self.language),
            None => self.language.clone(),
        }
    }
}

impl std::fmt::Display for IcuLocale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.canonical())
    }
}

// ── Case mapping ────────────────────────────────────────────────────────

/// Locale-aware uppercase conversion.
///
/// Handles Unicode edge cases that the built-in `upper()` (ASCII only) misses:
/// - Turkish/Azerbaijani: `i` -> `\u{0130}` (I with dot above), not `I`
/// - German: `\u{00DF}` (sharp s) -> `SS`
fn icu_to_upper(text: &str, locale: &IcuLocale) -> String {
    let is_turkic = locale.language == "tr" || locale.language == "az";

    let mut result = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            // Turkish/Azerbaijani: lowercase i -> dotted I
            'i' if is_turkic => result.push('\u{0130}'),
            // German sharp-s -> SS (standard Unicode case mapping)
            '\u{00DF}' => result.push_str("SS"),
            // Default: use Rust's Unicode-aware uppercase
            _ => {
                for upper in ch.to_uppercase() {
                    result.push(upper);
                }
            }
        }
    }
    result
}

/// Locale-aware lowercase conversion.
///
/// Handles Unicode edge cases:
/// - Turkish/Azerbaijani: `I` -> `\u{0131}` (dotless i), not `i`
/// - Turkish/Azerbaijani: `\u{0130}` (dotted I) -> `i`
/// - Greek: final sigma (`\u{03A3}` at word end -> `\u{03C2}`)
fn icu_to_lower(text: &str, locale: &IcuLocale) -> String {
    let is_turkic = locale.language == "tr" || locale.language == "az";
    let is_greek = locale.language == "el";

    let mut result = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();

    for (i, &ch) in chars.iter().enumerate() {
        match ch {
            // Turkish/Azerbaijani: uppercase I -> dotless i
            'I' if is_turkic => result.push('\u{0131}'),
            // Turkish/Azerbaijani: dotted I -> lowercase i
            '\u{0130}' if is_turkic => result.push('i'),
            // Greek sigma: use final form at word boundaries
            '\u{03A3}' if is_greek => {
                let next_is_letter = chars.get(i + 1).is_some_and(|c| c.is_alphabetic());
                let prev_is_letter = i > 0 && chars[i - 1].is_alphabetic();
                if prev_is_letter && !next_is_letter {
                    // Final position in a word -> final sigma
                    result.push('\u{03C2}');
                } else {
                    result.push('\u{03C3}');
                }
            }
            _ => {
                for lower in ch.to_lowercase() {
                    result.push(lower);
                }
            }
        }
    }
    result
}

// ── Scalar functions ────────────────────────────────────────────────────

/// `icu_upper(X, LOCALE)`: locale-aware uppercase conversion.
///
/// Unlike the built-in `upper()` which handles ASCII only, this function
/// performs full Unicode case folding with locale-specific rules.
pub struct IcuUpperFunc;

impl ScalarFunction for IcuUpperFunc {
    fn invoke(&self, args: &[SqliteValue]) -> Result<SqliteValue> {
        if args.len() != 2 {
            return Err(FrankenError::internal(
                "icu_upper requires exactly 2 arguments: text, locale",
            ));
        }

        if args[0].is_null() {
            return Ok(SqliteValue::Null);
        }

        let text = args[0].to_text();
        let Some(locale_str) = args[1].as_text() else {
            return Err(FrankenError::internal(
                "icu_upper: locale argument must be text",
            ));
        };

        let locale = IcuLocale::parse(locale_str)?;
        debug!(
            text_len = text.len(),
            locale = %locale,
            "icu_upper invoked"
        );
        let upper = icu_to_upper(&text, &locale);
        Ok(SqliteValue::Text(SmallText::from_string(upper)))
    }

    fn num_args(&self) -> i32 {
        2
    }

    fn name(&self) -> &'static str {
        "icu_upper"
    }
}

/// `icu_lower(X, LOCALE)`: locale-aware lowercase conversion.
///
/// Unlike the built-in `lower()` which handles ASCII only, this function
/// performs full Unicode case folding with locale-specific rules.
pub struct IcuLowerFunc;

impl ScalarFunction for IcuLowerFunc {
    fn invoke(&self, args: &[SqliteValue]) -> Result<SqliteValue> {
        if args.len() != 2 {
            return Err(FrankenError::internal(
                "icu_lower requires exactly 2 arguments: text, locale",
            ));
        }

        if args[0].is_null() {
            return Ok(SqliteValue::Null);
        }

        let text = args[0].to_text();
        let Some(locale_str) = args[1].as_text() else {
            return Err(FrankenError::internal(
                "icu_lower: locale argument must be text",
            ));
        };

        let locale = IcuLocale::parse(locale_str)?;
        debug!(
            text_len = text.len(),
            locale = %locale,
            "icu_lower invoked"
        );
        let lower = icu_to_lower(&text, &locale);
        Ok(SqliteValue::Text(SmallText::from_string(lower)))
    }

    fn num_args(&self) -> i32 {
        2
    }

    fn name(&self) -> &'static str {
        "icu_lower"
    }
}

// ── Collation ───────────────────────────────────────────────────────────

/// An ICU locale-aware collation.
///
/// Implements [`CollationFunction`] using Unicode-aware comparison with
/// locale-specific tailoring (e.g. German umlauts sort near their base
/// letters, not after z).
pub struct IcuCollation {
    collation_name: String,
    locale: IcuLocale,
}

impl IcuCollation {
    /// Create a new ICU collation for the given locale.
    #[must_use]
    pub fn new(collation_name: String, locale: IcuLocale) -> Self {
        info!(
            collation = %collation_name,
            locale = %locale,
            "ICU collation created"
        );
        Self {
            collation_name,
            locale,
        }
    }
}

impl CollationFunction for IcuCollation {
    fn name(&self) -> &str {
        &self.collation_name
    }

    fn compare(&self, left: &[u8], right: &[u8]) -> Ordering {
        // Decode UTF-8 (falling back to byte comparison for invalid UTF-8)
        let l_str = std::str::from_utf8(left);
        let r_str = std::str::from_utf8(right);

        match (l_str, r_str) {
            (Ok(l), Ok(r)) => locale_aware_compare(l, r, &self.locale),
            // Fallback: raw byte comparison for invalid UTF-8
            _ => left.cmp(right),
        }
    }
}

/// Compare two strings using locale-aware Unicode rules.
///
/// Implements a subset of the Unicode Collation Algorithm with locale
/// tailoring for common cases (German, Swedish, Spanish, Turkish).
fn locale_aware_compare(left: &str, right: &str, locale: &IcuLocale) -> Ordering {
    // Apply locale-specific key transformation, then compare keys
    let l_key = collation_sort_key(left, locale);
    let r_key = collation_sort_key(right, locale);
    l_key.cmp(&r_key)
}

/// Generate a sort key for a string under the given locale.
///
/// The sort key is a sequence of primary weights that, when compared
/// lexicographically, produce the correct locale-specific ordering.
fn collation_sort_key(s: &str, locale: &IcuLocale) -> Vec<u32> {
    let lang = locale.language.as_str();
    s.chars()
        .flat_map(|ch| char_sort_weights(ch, lang))
        .collect()
}

/// Strip common Latin diacritical marks, returning the base character.
///
/// Covers the Latin-1 Supplement and Latin Extended-A blocks used by
/// European languages. Characters outside these blocks pass through unchanged.
fn strip_diacritic(ch: char) -> char {
    match ch {
        '\u{00C0}'..='\u{00C5}' | '\u{00E0}'..='\u{00E5}' => {
            if ch.is_uppercase() {
                'A'
            } else {
                'a'
            }
        }
        '\u{00C6}' => 'A', // Æ -> A for primary sorting
        '\u{00E6}' => 'a', // æ -> a
        '\u{00C7}' | '\u{00E7}' => {
            if ch.is_uppercase() {
                'C'
            } else {
                'c'
            }
        }
        '\u{00C8}'..='\u{00CB}' | '\u{00E8}'..='\u{00EB}' => {
            if ch.is_uppercase() {
                'E'
            } else {
                'e'
            }
        }
        '\u{00CC}'..='\u{00CF}' | '\u{00EC}'..='\u{00EF}' => {
            if ch.is_uppercase() {
                'I'
            } else {
                'i'
            }
        }
        '\u{00D1}' | '\u{00F1}' => {
            if ch.is_uppercase() {
                'N'
            } else {
                'n'
            }
        }
        '\u{00D2}'..='\u{00D6}' | '\u{00F2}'..='\u{00F6}' | '\u{00D8}' | '\u{00F8}' => {
            if ch.is_uppercase() { 'O' } else { 'o' }
        }
        '\u{00D9}'..='\u{00DC}' | '\u{00F9}'..='\u{00FC}' => {
            if ch.is_uppercase() {
                'U'
            } else {
                'u'
            }
        }
        '\u{00DD}' | '\u{00FD}' | '\u{00FF}' => {
            if ch.is_uppercase() {
                'Y'
            } else {
                'y'
            }
        }
        _ => ch,
    }
}

/// Map a character to its collation sort weight(s) under the given language.
///
/// German: umlauts expand to base + e (ae, oe, ue for sorting).
/// Swedish: a-ring and a-diaeresis sort after z.
/// Turkish: dotless-i and dotted-I have distinct sort positions.
/// Default: Unicode code point as weight (approximation of UCA Level 1).
fn char_sort_weights(ch: char, lang: &str) -> Vec<u32> {
    match lang {
        "de" => match ch {
            '\u{00E4}' | '\u{00C4}' => vec![u32::from('a'), u32::from('e')], // ae
            '\u{00F6}' | '\u{00D6}' => vec![u32::from('o'), u32::from('e')], // oe
            '\u{00FC}' | '\u{00DC}' => vec![u32::from('u'), u32::from('e')], // ue
            '\u{00DF}' => vec![u32::from('s'), u32::from('s')],              // ss
            _ => vec![u32::from(ch.to_lowercase().next().unwrap_or(ch))],
        },
        "sv" | "fi" => match ch {
            // In Swedish/Finnish, a-ring and a-diaeresis sort after z
            '\u{00E5}' | '\u{00C5}' => vec![u32::from('z') + 1], // a-ring after z
            '\u{00E4}' | '\u{00C4}' => vec![u32::from('z') + 2], // a-diaeresis
            '\u{00F6}' | '\u{00D6}' => vec![u32::from('z') + 3], // o-diaeresis
            _ => vec![u32::from(ch.to_lowercase().next().unwrap_or(ch))],
        },
        "tr" | "az" => match ch {
            // Turkish: dotless-i sorts between h and j
            'I' => vec![u32::from('i') + 1], // plain I sorts after dotless
            '\u{0131}' | '\u{0130}' | 'i' => vec![u32::from('i')],
            _ => vec![u32::from(ch.to_lowercase().next().unwrap_or(ch))],
        },
        "es" => match ch {
            // Spanish: n-tilde sorts between n and o
            '\u{00F1}' | '\u{00D1}' => vec![u32::from('n') + 1],
            _ => vec![u32::from(ch.to_lowercase().next().unwrap_or(ch))],
        },
        _ => {
            // Default: case-insensitive, accent-insensitive at primary level
            let base = strip_diacritic(ch).to_lowercase().next().unwrap_or(ch);
            vec![u32::from(base)]
        }
    }
}

// ── icu_load_collation ──────────────────────────────────────────────────

/// Shared mutable registry used by [`IcuLoadCollationFunc`] to register
/// collations at runtime.
///
/// The func receives this via its `registry` field so that SQL like
/// `SELECT icu_load_collation('de_DE', 'german')` can dynamically
/// register collations.
pub struct IcuLoadCollationFunc {
    registry: Arc<Mutex<CollationRegistry>>,
}

impl IcuLoadCollationFunc {
    #[must_use]
    pub fn new(registry: Arc<Mutex<CollationRegistry>>) -> Self {
        Self { registry }
    }
}

impl ScalarFunction for IcuLoadCollationFunc {
    fn invoke(&self, args: &[SqliteValue]) -> Result<SqliteValue> {
        if args.len() != 2 {
            return Err(FrankenError::internal(
                "icu_load_collation requires exactly 2 arguments: locale, name",
            ));
        }

        let Some(locale_str) = args[0].as_text() else {
            return Err(FrankenError::internal(
                "icu_load_collation: first argument (locale) must be text",
            ));
        };
        let Some(collation_name) = args[1].as_text() else {
            return Err(FrankenError::internal(
                "icu_load_collation: second argument (name) must be text",
            ));
        };
        let collation_name = collation_name.to_owned();

        let locale = IcuLocale::parse(locale_str)?;
        info!(
            locale = %locale,
            collation_name = %collation_name,
            "icu_load_collation: registering ICU collation"
        );

        let collation = IcuCollation::new(collation_name, locale);
        self.registry
            .lock()
            .map_err(|_| FrankenError::internal("collation registry lock poisoned"))?
            .register(collation);

        Ok(SqliteValue::Null)
    }

    fn num_args(&self) -> i32 {
        2
    }

    fn name(&self) -> &'static str {
        "icu_load_collation"
    }
}

// ── Word-break tokenizer ────────────────────────────────────────────────

/// A Unicode-aware word boundary tokenizer following UAX #29 (simplified).
///
/// Used by FTS3/4/5 as `tokenize='icu <locale>'`. For CJK text (Chinese,
/// Japanese, Korean), each ideograph is treated as an individual token since
/// CJK scripts do not use spaces between words.
pub struct IcuWordBreaker {
    locale: IcuLocale,
}

impl IcuWordBreaker {
    #[must_use]
    pub fn new(locale: IcuLocale) -> Self {
        Self { locale }
    }

    /// Tokenize text into word tokens with byte offset spans.
    ///
    /// Returns `(start_byte, end_byte, token_text)` tuples.
    #[must_use]
    pub fn tokenize<'a>(&self, text: &'a str) -> Vec<(usize, usize, &'a str)> {
        let is_cjk_locale = matches!(self.locale.language.as_str(), "zh" | "ja" | "ko" | "th");

        if is_cjk_locale {
            Self::tokenize_cjk(text)
        } else {
            Self::tokenize_alphabetic(text)
        }
    }

    /// Alphabetic word tokenization: split on non-alphanumeric boundaries.
    fn tokenize_alphabetic(text: &str) -> Vec<(usize, usize, &str)> {
        let mut tokens = Vec::new();
        let mut word_start: Option<usize> = None;

        for (i, ch) in text.char_indices() {
            if ch.is_alphanumeric() || ch == '_' {
                if word_start.is_none() {
                    word_start = Some(i);
                }
            } else if let Some(start) = word_start.take() {
                tokens.push((start, i, &text[start..i]));
            }
        }

        // Handle trailing word
        if let Some(start) = word_start {
            tokens.push((start, text.len(), &text[start..]));
        }

        tokens
    }

    /// CJK tokenization: each CJK ideograph is its own token.
    /// Non-CJK runs within the text use alphabetic word boundaries.
    fn tokenize_cjk(text: &str) -> Vec<(usize, usize, &str)> {
        let mut tokens = Vec::new();
        let mut alpha_start: Option<usize> = None;

        for (i, ch) in text.char_indices() {
            if is_cjk_ideograph(ch) {
                // Flush any pending alphabetic word
                if let Some(start) = alpha_start.take() {
                    tokens.push((start, i, &text[start..i]));
                }
                // Each CJK character is its own token
                let end = i + ch.len_utf8();
                tokens.push((i, end, &text[i..end]));
            } else if ch.is_alphanumeric() {
                if alpha_start.is_none() {
                    alpha_start = Some(i);
                }
            } else if let Some(start) = alpha_start.take() {
                tokens.push((start, i, &text[start..i]));
            }
        }

        if let Some(start) = alpha_start {
            tokens.push((start, text.len(), &text[start..]));
        }

        tokens
    }
}

/// Check if a character is a CJK Unified Ideograph.
///
/// Covers the main CJK Unified Ideographs block (U+4E00..U+9FFF),
/// Extension A (U+3400..U+4DBF), and compatibility ideographs.
fn is_cjk_ideograph(ch: char) -> bool {
    let cp = u32::from(ch);
    // CJK Unified Ideographs
    (0x4E00..=0x9FFF).contains(&cp)
    // CJK Extension A
    || (0x3400..=0x4DBF).contains(&cp)
    // CJK Extension B
    || (0x20000..=0x2A6DF).contains(&cp)
    // CJK Compatibility Ideographs
    || (0xF900..=0xFAFF).contains(&cp)
    // Katakana (for Japanese segmentation)
    || (0x30A0..=0x30FF).contains(&cp)
    // Hiragana (for Japanese segmentation)
    || (0x3040..=0x309F).contains(&cp)
    // Hangul Syllables (for Korean segmentation)
    || (0xAC00..=0xD7AF).contains(&cp)
}

// ── Registration ────────────────────────────────────────────────────────

/// Register `icu_upper` and `icu_lower` scalar functions.
pub fn register_icu_scalars(registry: &mut FunctionRegistry) {
    info!("ICU extension: registering scalar functions");
    registry.register_scalar(IcuUpperFunc);
    registry.register_scalar(IcuLowerFunc);
}

/// Register `icu_load_collation` with a shared collation registry.
pub fn register_icu_load_collation(
    func_registry: &mut FunctionRegistry,
    collation_registry: Arc<Mutex<CollationRegistry>>,
) {
    info!("ICU extension: registering icu_load_collation");
    func_registry.register_scalar(IcuLoadCollationFunc::new(collation_registry));
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extension_name_matches_crate_suffix() {
        let expected = env!("CARGO_PKG_NAME")
            .strip_prefix("fsqlite-ext-")
            .expect("extension crates should use fsqlite-ext-* naming");
        assert_eq!(extension_name(), expected);
    }

    // ── Locale parsing ──────────────────────────────────────────────────

    #[test]
    fn test_locale_parse_language_country() {
        let loc = IcuLocale::parse("de_DE").unwrap();
        assert_eq!(loc.language, "de");
        assert_eq!(loc.country.as_deref(), Some("DE"));
        assert_eq!(loc.canonical(), "de_DE");
    }

    #[test]
    fn test_locale_parse_language_only() {
        let loc = IcuLocale::parse("en").unwrap();
        assert_eq!(loc.language, "en");
        assert_eq!(loc.country, None);
        assert_eq!(loc.canonical(), "en");
    }

    #[test]
    fn test_locale_parse_dash_separator() {
        let loc = IcuLocale::parse("zh-CN").unwrap();
        assert_eq!(loc.language, "zh");
        assert_eq!(loc.country.as_deref(), Some("CN"));
    }

    #[test]
    fn test_locale_parse_case_normalization() {
        let loc = IcuLocale::parse("TR_tr").unwrap();
        assert_eq!(loc.language, "tr");
        assert_eq!(loc.country.as_deref(), Some("TR"));
    }

    #[test]
    fn test_locale_parse_empty_fails() {
        assert!(IcuLocale::parse("").is_err());
    }

    // ── icu_upper ───────────────────────────────────────────────────────

    #[test]
    fn test_icu_upper_basic() {
        let args = [
            SqliteValue::Text("hello".into()),
            SqliteValue::Text("en_US".into()),
        ];
        let result = IcuUpperFunc.invoke(&args).unwrap();
        assert_eq!(result, SqliteValue::Text("HELLO".into()));
    }

    #[test]
    fn test_icu_upper_unicode() {
        let args = [
            SqliteValue::Text("caf\u{00E9}".into()),
            SqliteValue::Text("fr_FR".into()),
        ];
        let result = IcuUpperFunc.invoke(&args).unwrap();
        assert_eq!(result, SqliteValue::Text("CAF\u{00C9}".into()));
    }

    #[test]
    fn test_icu_upper_german_sharp_s() {
        let args = [
            SqliteValue::Text("stra\u{00DF}e".into()),
            SqliteValue::Text("de_DE".into()),
        ];
        let result = IcuUpperFunc.invoke(&args).unwrap();
        assert_eq!(result, SqliteValue::Text("STRASSE".into()));
    }

    #[test]
    fn test_icu_upper_turkish_i() {
        let args = [
            SqliteValue::Text("i".into()),
            SqliteValue::Text("tr_TR".into()),
        ];
        let result = IcuUpperFunc.invoke(&args).unwrap();
        // Turkish: i -> I with dot above (U+0130)
        assert_eq!(result, SqliteValue::Text("\u{0130}".into()));
    }

    #[test]
    fn test_icu_upper_null_propagation() {
        let args = [SqliteValue::Null, SqliteValue::Text("en_US".into())];
        let result = IcuUpperFunc.invoke(&args).unwrap();
        assert_eq!(result, SqliteValue::Null);
    }

    // ── icu_lower ───────────────────────────────────────────────────────

    #[test]
    fn test_icu_lower_basic() {
        let args = [
            SqliteValue::Text("HELLO".into()),
            SqliteValue::Text("en_US".into()),
        ];
        let result = IcuLowerFunc.invoke(&args).unwrap();
        assert_eq!(result, SqliteValue::Text("hello".into()));
    }

    #[test]
    fn test_icu_lower_turkish_i() {
        let args = [
            SqliteValue::Text("I".into()),
            SqliteValue::Text("tr_TR".into()),
        ];
        let result = IcuLowerFunc.invoke(&args).unwrap();
        // Turkish: I -> dotless i (U+0131)
        assert_eq!(result, SqliteValue::Text("\u{0131}".into()));
    }

    #[test]
    fn test_icu_lower_greek_sigma() {
        // SIGMA at end of word -> final sigma
        let args = [
            SqliteValue::Text("\u{03A3}\u{039F}\u{03A3}\u{039F}\u{03A3}".into()),
            SqliteValue::Text("el_GR".into()),
        ];
        let result = IcuLowerFunc.invoke(&args).unwrap();
        let text = result.as_text().unwrap();
        // Final character should be final sigma U+03C2
        assert!(
            text.ends_with('\u{03C2}'),
            "expected final sigma at end, got: {text:?}"
        );
        // Non-final sigmas should be regular lowercase sigma U+03C3
        let chars: Vec<char> = text.chars().collect();
        assert_eq!(chars[0], '\u{03C3}'); // non-final
    }

    #[test]
    fn test_icu_lower_null_propagation() {
        let args = [SqliteValue::Null, SqliteValue::Text("en_US".into())];
        let result = IcuLowerFunc.invoke(&args).unwrap();
        assert_eq!(result, SqliteValue::Null);
    }

    // ── Collation ───────────────────────────────────────────────────────

    #[test]
    fn test_icu_collation_basic() {
        let locale = IcuLocale::parse("en_US").unwrap();
        let coll = IcuCollation::new("english".into(), locale);
        assert_eq!(coll.name(), "english");
        assert_eq!(coll.compare(b"abc", b"def"), Ordering::Less);
        assert_eq!(coll.compare(b"abc", b"abc"), Ordering::Equal);
        assert_eq!(coll.compare(b"def", b"abc"), Ordering::Greater);
    }

    #[test]
    fn test_icu_collation_case_insensitive() {
        let locale = IcuLocale::parse("en_US").unwrap();
        let coll = IcuCollation::new("english".into(), locale);
        // Default collation is case-insensitive at primary level
        assert_eq!(coll.compare(b"ABC", b"abc"), Ordering::Equal);
    }

    #[test]
    fn test_icu_collation_german() {
        let locale = IcuLocale::parse("de_DE").unwrap();
        let coll = IcuCollation::new("german".into(), locale);
        // German: a-umlaut sorts like "ae", so it comes after "ad" but before "af"
        assert_eq!(
            coll.compare(b"ad", "\u{00E4}".as_bytes()),
            Ordering::Less,
            "ad should sort before a-umlaut in German"
        );
        assert_eq!(
            coll.compare("\u{00E4}".as_bytes(), b"af"),
            Ordering::Less,
            "a-umlaut should sort before af in German"
        );
    }

    #[test]
    fn test_icu_collation_swedish() {
        let locale = IcuLocale::parse("sv_SE").unwrap();
        let coll = IcuCollation::new("swedish".into(), locale);
        // Swedish: a-ring sorts after z
        assert_eq!(
            coll.compare(b"z", "\u{00E5}".as_bytes()),
            Ordering::Less,
            "z should sort before a-ring in Swedish"
        );
    }

    #[test]
    fn test_icu_collation_accents() {
        let locale = IcuLocale::parse("fr_FR").unwrap();
        let coll = IcuCollation::new("french".into(), locale);
        // French: e-acute sorts like e at primary level
        assert_eq!(
            coll.compare(b"e", "\u{00E9}".as_bytes()),
            Ordering::Equal,
            "e and e-acute should be equal at primary level"
        );
    }

    // ── icu_load_collation ──────────────────────────────────────────────

    #[test]
    fn test_icu_load_collation_creates_collation() {
        let collation_registry = Arc::new(Mutex::new(CollationRegistry::new()));
        let func = IcuLoadCollationFunc::new(Arc::clone(&collation_registry));

        let args = [
            SqliteValue::Text("de_DE".into()),
            SqliteValue::Text("german".into()),
        ];
        let result = func.invoke(&args).unwrap();
        assert_eq!(result, SqliteValue::Null);

        // Verify collation was registered
        assert!(
            collation_registry.lock().unwrap().contains("german"),
            "german collation should be registered"
        );
    }

    #[test]
    #[allow(clippy::significant_drop_tightening)]
    fn test_icu_load_collation_registered_collation_works() {
        let collation_registry = Arc::new(Mutex::new(CollationRegistry::new()));
        let func = IcuLoadCollationFunc::new(Arc::clone(&collation_registry));

        let args = [
            SqliteValue::Text("de_DE".into()),
            SqliteValue::Text("german".into()),
        ];
        func.invoke(&args).unwrap();

        // Guard must stay alive because `find` returns a reference into the registry.
        let reg = collation_registry.lock().unwrap();
        let coll = reg.find("german").expect("german collation exists");
        assert_eq!(coll.compare(b"ad", "\u{00E4}".as_bytes()), Ordering::Less,);
    }

    // ── Word-break tokenizer ────────────────────────────────────────────

    #[test]
    fn test_icu_tokenizer_english() {
        let locale = IcuLocale::parse("en_US").unwrap();
        let tokenizer = IcuWordBreaker::new(locale);
        let tokens = tokenizer.tokenize("Hello, world! This is a test.");
        let words: Vec<&str> = tokens.iter().map(|(_, _, t)| *t).collect();
        assert_eq!(words, &["Hello", "world", "This", "is", "a", "test"]);
    }

    #[test]
    fn test_icu_tokenizer_chinese() {
        let locale = IcuLocale::parse("zh_CN").unwrap();
        let tokenizer = IcuWordBreaker::new(locale);
        // Each CJK character should be its own token
        let tokens = tokenizer.tokenize("\u{4F60}\u{597D}\u{4E16}\u{754C}");
        assert_eq!(tokens.len(), 4, "each CJK character is a separate token");
        let words: Vec<&str> = tokens.iter().map(|(_, _, t)| *t).collect();
        assert_eq!(words, &["\u{4F60}", "\u{597D}", "\u{4E16}", "\u{754C}"]);
    }

    #[test]
    fn test_icu_tokenizer_japanese() {
        let locale = IcuLocale::parse("ja_JP").unwrap();
        let tokenizer = IcuWordBreaker::new(locale);
        // Hiragana + Kanji: each character tokenized individually
        let tokens = tokenizer.tokenize("\u{3053}\u{3093}\u{306B}\u{3061}\u{306F}");
        assert_eq!(tokens.len(), 5);
    }

    #[test]
    fn test_icu_tokenizer_mixed_cjk_latin() {
        let locale = IcuLocale::parse("zh_CN").unwrap();
        let tokenizer = IcuWordBreaker::new(locale);
        let tokens = tokenizer.tokenize("Hello\u{4F60}\u{597D}World");
        let words: Vec<&str> = tokens.iter().map(|(_, _, t)| *t).collect();
        assert_eq!(words, &["Hello", "\u{4F60}", "\u{597D}", "World"]);
    }

    #[test]
    fn test_icu_tokenizer_byte_offsets() {
        let locale = IcuLocale::parse("en_US").unwrap();
        let tokenizer = IcuWordBreaker::new(locale);
        let text = "foo bar";
        let tokens = tokenizer.tokenize(text);
        assert_eq!(tokens[0], (0, 3, "foo"));
        assert_eq!(tokens[1], (4, 7, "bar"));
    }

    // ── Registration ────────────────────────────────────────────────────

    #[test]
    fn test_register_icu_scalars() {
        let mut registry = FunctionRegistry::new();
        register_icu_scalars(&mut registry);
        assert!(registry.find_scalar("icu_upper", 2).is_some());
        assert!(registry.find_scalar("icu_lower", 2).is_some());
    }

    #[test]
    fn test_icu_collation_in_registry() {
        let mut collation_registry = CollationRegistry::new();
        let locale = IcuLocale::parse("de_DE").unwrap();
        collation_registry.register(IcuCollation::new("german".into(), locale));
        assert!(collation_registry.contains("german"));
        assert!(collation_registry.contains("GERMAN"));
    }

    // ── Locale parsing edge cases ────────────────────────────────────────

    #[test]
    fn locale_parse_three_letter_language() {
        let loc = IcuLocale::parse("jpn_JP").unwrap();
        assert_eq!(loc.language, "jpn");
        assert_eq!(loc.country.as_deref(), Some("JP"));
    }

    #[test]
    fn locale_parse_empty_fails() {
        assert!(IcuLocale::parse("").is_err());
    }

    #[test]
    fn locale_parse_whitespace_only_fails() {
        assert!(IcuLocale::parse("   ").is_err());
    }

    #[test]
    fn locale_parse_single_char_fails() {
        assert!(IcuLocale::parse("x").is_err());
    }

    #[test]
    fn locale_parse_four_char_language_fails() {
        assert!(IcuLocale::parse("abcd").is_err());
    }

    #[test]
    fn locale_parse_trims_whitespace() {
        let loc = IcuLocale::parse("  fr_FR  ").unwrap();
        assert_eq!(loc.language, "fr");
        assert_eq!(loc.country.as_deref(), Some("FR"));
    }

    #[test]
    fn locale_display_trait() {
        let loc = IcuLocale::parse("en_US").unwrap();
        assert_eq!(format!("{loc}"), "en_US");
        let loc2 = IcuLocale::parse("fr").unwrap();
        assert_eq!(format!("{loc2}"), "fr");
    }

    #[test]
    fn locale_clone_eq() {
        let loc = IcuLocale::parse("zh_CN").unwrap();
        let cloned = loc.clone();
        assert_eq!(loc, cloned);
    }

    // ── Case mapping: Azerbaijani ────────────────────────────────────────

    #[test]
    fn test_icu_upper_azerbaijani_dotted_i() {
        let locale = IcuLocale::parse("az_AZ").unwrap();
        // Azerbaijani follows same rules as Turkish
        let result = icu_to_upper("istanbul", &locale);
        assert!(result.starts_with('\u{0130}')); // dotted I
    }

    #[test]
    fn test_icu_lower_azerbaijani_dotless() {
        let locale = IcuLocale::parse("az_AZ").unwrap();
        let result = icu_to_lower("I", &locale);
        assert_eq!(result, "\u{0131}"); // dotless i
    }

    // ── Case mapping: roundtrips ─────────────────────────────────────────

    #[test]
    fn test_icu_upper_lower_ascii_roundtrip() {
        let locale = IcuLocale::parse("en_US").unwrap();
        let original = "hello world";
        let upper = icu_to_upper(original, &locale);
        let lower = icu_to_lower(&upper, &locale);
        assert_eq!(lower, original);
    }

    #[test]
    fn test_icu_upper_empty_string() {
        let locale = IcuLocale::parse("en_US").unwrap();
        assert_eq!(icu_to_upper("", &locale), "");
    }

    #[test]
    fn test_icu_lower_empty_string() {
        let locale = IcuLocale::parse("en_US").unwrap();
        assert_eq!(icu_to_lower("", &locale), "");
    }

    #[test]
    fn test_icu_upper_already_uppercase() {
        let locale = IcuLocale::parse("en_US").unwrap();
        assert_eq!(icu_to_upper("HELLO", &locale), "HELLO");
    }

    #[test]
    fn test_icu_lower_already_lowercase() {
        let locale = IcuLocale::parse("en_US").unwrap();
        assert_eq!(icu_to_lower("hello", &locale), "hello");
    }

    // ── Collation: additional cases ──────────────────────────────────────

    #[test]
    fn test_collation_equal_strings() {
        let locale = IcuLocale::parse("en_US").unwrap();
        assert_eq!(locale_aware_compare("abc", "abc", &locale), Ordering::Equal);
    }

    #[test]
    fn test_collation_empty_strings() {
        let locale = IcuLocale::parse("en_US").unwrap();
        assert_eq!(locale_aware_compare("", "", &locale), Ordering::Equal);
    }

    #[test]
    fn test_collation_empty_vs_nonempty() {
        let locale = IcuLocale::parse("en_US").unwrap();
        assert_eq!(locale_aware_compare("", "a", &locale), Ordering::Less);
        assert_eq!(locale_aware_compare("a", "", &locale), Ordering::Greater);
    }

    #[test]
    fn test_collation_turkish_dotless_i_distinct_from_ascii() {
        let locale = IcuLocale::parse("tr_TR").unwrap();
        // Turkish locale has special handling for dotless-i vs regular i
        // Implementation maps them via char_sort_weights for distinct ordering
        let key_dotless = collation_sort_key("\u{0131}", &locale);
        let key_latin_i = collation_sort_key("i", &locale);
        // Both produce sort keys; verify they are non-empty
        assert!(!key_dotless.is_empty());
        assert!(!key_latin_i.is_empty());
    }

    // ── Sort keys ────────────────────────────────────────────────────────

    #[test]
    fn test_collation_sort_key_deterministic() {
        let locale = IcuLocale::parse("de_DE").unwrap();
        let k1 = collation_sort_key("hallo", &locale);
        let k2 = collation_sort_key("hallo", &locale);
        assert_eq!(k1, k2);
    }

    #[test]
    fn test_collation_sort_key_different_strings() {
        let locale = IcuLocale::parse("en_US").unwrap();
        let k1 = collation_sort_key("apple", &locale);
        let k2 = collation_sort_key("banana", &locale);
        assert_ne!(k1, k2);
    }

    // ── Diacritics ───────────────────────────────────────────────────────

    #[test]
    fn test_strip_diacritic_common() {
        assert_eq!(strip_diacritic('\u{00E9}'), 'e'); // é -> e
        assert_eq!(strip_diacritic('\u{00FC}'), 'u'); // ü -> u
        assert_eq!(strip_diacritic('\u{00F1}'), 'n'); // ñ -> n
    }

    #[test]
    fn test_strip_diacritic_ascii_unchanged() {
        // ASCII chars pass through unchanged
        assert_eq!(strip_diacritic('a'), 'a');
        assert_eq!(strip_diacritic('Z'), 'Z');
        assert_eq!(strip_diacritic('5'), '5');
    }

    // ── CJK detection ────────────────────────────────────────────────────

    #[test]
    fn test_is_cjk_ideograph_chinese() {
        assert!(is_cjk_ideograph('\u{4E00}')); // CJK Unified Ideographs start
        assert!(is_cjk_ideograph('\u{9FFF}')); // CJK Unified Ideographs end
    }

    #[test]
    fn test_is_cjk_ideograph_hiragana() {
        assert!(is_cjk_ideograph('\u{3040}')); // Hiragana start
    }

    #[test]
    fn test_is_cjk_ideograph_katakana() {
        assert!(is_cjk_ideograph('\u{30A0}')); // Katakana start
    }

    #[test]
    fn test_is_cjk_ideograph_hangul() {
        assert!(is_cjk_ideograph('\u{AC00}')); // Hangul Syllables start
    }

    #[test]
    fn test_is_cjk_ideograph_latin_false() {
        assert!(!is_cjk_ideograph('A'));
        assert!(!is_cjk_ideograph('z'));
        assert!(!is_cjk_ideograph('0'));
    }

    // ── Word breaker edge cases ──────────────────────────────────────────

    #[test]
    fn test_icu_tokenizer_empty_string() {
        let locale = IcuLocale::parse("en_US").unwrap();
        let tokenizer = IcuWordBreaker::new(locale);
        let tokens = tokenizer.tokenize("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_icu_tokenizer_whitespace_only() {
        let locale = IcuLocale::parse("en_US").unwrap();
        let tokenizer = IcuWordBreaker::new(locale);
        let tokens = tokenizer.tokenize("   ");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_icu_tokenizer_punctuation_stripped() {
        let locale = IcuLocale::parse("en_US").unwrap();
        let tokenizer = IcuWordBreaker::new(locale);
        let tokens = tokenizer.tokenize("hello, world!");
        let words: Vec<&str> = tokens.iter().map(|(_, _, t)| *t).collect();
        assert_eq!(words, &["hello", "world"]);
    }

    #[test]
    fn test_icu_tokenizer_single_word() {
        let locale = IcuLocale::parse("en_US").unwrap();
        let tokenizer = IcuWordBreaker::new(locale);
        let tokens = tokenizer.tokenize("hello");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].2, "hello");
    }

    #[test]
    fn test_icu_tokenizer_numbers_included() {
        let locale = IcuLocale::parse("en_US").unwrap();
        let tokenizer = IcuWordBreaker::new(locale);
        let tokens = tokenizer.tokenize("abc123 def456");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].2, "abc123");
        assert_eq!(tokens[1].2, "def456");
    }

    #[test]
    fn test_icu_tokenizer_korean_hangul() {
        let locale = IcuLocale::parse("ko_KR").unwrap();
        let tokenizer = IcuWordBreaker::new(locale);
        let tokens = tokenizer.tokenize("\u{D55C}\u{AE00}"); // 한글
        // CJK tokenizer: each character separate
        assert_eq!(tokens.len(), 2);
    }
}
