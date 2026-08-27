//! ANSI decoding utilities (Python Rich `rich.ansi` parity).
//!
//! Python Rich supports decoding terminal output containing ANSI escape sequences into
//! structured [`Text`] with styles. This module implements the subset required for
//! parity with Rich 13.9.4:
//! - SGR (`ESC[...m`) for attributes and colors
//! - OSC 8 hyperlinks (`ESC]8;...;urlESC\\`) to set/clear links
//!
//! The implementation is intentionally lenient: invalid / incomplete sequences are ignored.

use crate::color::Color;
use crate::style::{Attributes, Style};
use crate::text::Text;

/// Stateful ANSI decoder (Python Rich `rich.ansi.AnsiDecoder` parity).
#[derive(Debug, Clone)]
pub struct AnsiDecoder {
    style: Style,
}

impl Default for AnsiDecoder {
    fn default() -> Self {
        Self {
            style: Style::null(),
        }
    }
}

impl AnsiDecoder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Decode a string containing ANSI escapes into a list of [`Text`] lines.
    ///
    /// Mirrors Python Rich's `AnsiDecoder.decode`, which splits with `splitlines()`.
    #[must_use]
    pub fn decode(&mut self, terminal_text: &str) -> Vec<Text> {
        terminal_text
            .lines()
            .map(|line| self.decode_line(line))
            .collect()
    }

    /// Decode a single line containing ANSI escapes into [`Text`].
    ///
    /// Mirrors Python Rich's `AnsiDecoder.decode_line` behavior:
    /// - Drops everything before the last `\r` (carriage return), if present.
    /// - Applies escape sequences to update the current style state.
    /// - Appends plain text spans with the current style (or unstyled when style is null).
    #[must_use]
    pub fn decode_line(&mut self, line: &str) -> Text {
        let line = match line.rsplit_once('\r') {
            Some((_, after)) => after,
            None => line,
        };

        let mut text = Text::new("");
        // Python's decoder uses `Style.null()` as the "no style" baseline.
        text.set_style(Style::null());

        let bytes = line.as_bytes();
        let mut i = 0usize;
        let mut plain_start = 0usize;

        while i < bytes.len() {
            if bytes[i] != 0x1b {
                i += 1;
                continue;
            }

            // Flush any plain text before the escape.
            if plain_start < i {
                self.append_plain(&mut text, &line[plain_start..i]);
            }

            if i + 1 >= bytes.len() {
                break;
            }

            match bytes[i + 1] {
                // Character set escape: ESC ( <char>. Python skips the selector byte too.
                b'(' => {
                    if i + 2 < bytes.len() {
                        i += 3;
                    } else {
                        i = bytes.len();
                    }
                    plain_start = i;
                }
                // CSI
                b'[' => {
                    let Some((next_i, final_byte, params)) = parse_csi(line, i) else {
                        // Incomplete sequence: treat as literal.
                        i += 1;
                        plain_start = i;
                        continue;
                    };
                    if final_byte == b'm' {
                        self.apply_sgr(params);
                    }
                    i = next_i;
                    plain_start = i;
                }
                // OSC (only `ESC]...ESC\\` terminator is supported, matching Python Rich's regex).
                b']' => {
                    let Some((next_i, content)) = parse_osc(line, i) else {
                        i += 1;
                        plain_start = i;
                        continue;
                    };
                    self.apply_osc(content);
                    i = next_i;
                    plain_start = i;
                }
                // Other escapes: skip ESC + one byte (lenient).
                _ => {
                    i += 2;
                    plain_start = i;
                }
            }
        }

        // Flush remainder.
        if plain_start < bytes.len() {
            self.append_plain(&mut text, &line[plain_start..]);
        }

        text
    }

    fn append_plain(&self, text: &mut Text, plain: &str) {
        if plain.is_empty() {
            return;
        }
        if self.style.is_null() {
            text.append(plain);
        } else {
            text.append_styled(plain, self.style.clone());
        }
    }

    fn apply_osc(&mut self, content: &str) {
        // OSC 8 hyperlink support: "8;<params>;<url>" where url may be empty to clear.
        let Some(rest) = content.strip_prefix("8;") else {
            return;
        };
        let Some((_params, link)) = rest.split_once(';') else {
            return;
        };

        if link.is_empty() {
            self.style.link = None;
            self.style.link_id = None;
            self.normalize_style_nullness();
        } else {
            // Python ignores params; we do as well.
            self.style = self.style.clone().link(link.to_string());
        }
    }

    fn apply_sgr(&mut self, params: &str) {
        let mut codes: Vec<u8> = Vec::new();
        if params.is_empty() {
            codes.push(0);
        } else {
            for part in params.split(';') {
                if part.is_empty() {
                    codes.push(0);
                    continue;
                }
                let Ok(value) = part.parse::<u16>() else {
                    continue;
                };
                codes.push(value.min(255) as u8);
            }
            if codes.is_empty() {
                return;
            }
        }

        let mut iter = codes.into_iter();
        while let Some(code) = iter.next() {
            match code {
                0 => self.style = Style::null(),
                1 => {
                    self.style = self
                        .style
                        .clone()
                        .combine(&style_with_attr(Attributes::BOLD));
                }
                2 => {
                    self.style = self
                        .style
                        .clone()
                        .combine(&style_with_attr(Attributes::DIM));
                }
                3 => {
                    self.style = self
                        .style
                        .clone()
                        .combine(&style_with_attr(Attributes::ITALIC));
                }
                4 => {
                    self.style = self
                        .style
                        .clone()
                        .combine(&style_with_attr(Attributes::UNDERLINE));
                }
                5 => {
                    self.style = self
                        .style
                        .clone()
                        .combine(&style_with_attr(Attributes::BLINK));
                }
                6 => {
                    self.style = self
                        .style
                        .clone()
                        .combine(&style_with_attr(Attributes::BLINK2));
                }
                7 => {
                    self.style = self
                        .style
                        .clone()
                        .combine(&style_with_attr(Attributes::REVERSE));
                }
                8 => {
                    self.style = self
                        .style
                        .clone()
                        .combine(&style_with_attr(Attributes::CONCEAL));
                }
                9 => {
                    self.style = self
                        .style
                        .clone()
                        .combine(&style_with_attr(Attributes::STRIKE));
                }
                21 => {
                    self.style = self
                        .style
                        .clone()
                        .combine(&style_with_attr(Attributes::UNDERLINE2));
                }
                22 => {
                    self.style = self
                        .style
                        .clone()
                        .not(Attributes::DIM)
                        .not(Attributes::BOLD);
                }
                23 => self.style = self.style.clone().not(Attributes::ITALIC),
                24 => {
                    self.style = self
                        .style
                        .clone()
                        .not(Attributes::UNDERLINE)
                        .not(Attributes::UNDERLINE2);
                }
                25 => self.style = self.style.clone().not(Attributes::BLINK),
                26 => self.style = self.style.clone().not(Attributes::BLINK2),
                27 => self.style = self.style.clone().not(Attributes::REVERSE),
                28 => self.style = self.style.clone().not(Attributes::CONCEAL),
                29 => self.style = self.style.clone().not(Attributes::STRIKE),
                30..=37 => {
                    let n = code - 30;
                    self.style = self
                        .style
                        .clone()
                        .combine(&style_with_fg(Color::from_ansi(n)));
                }
                39 => {
                    self.style = self.style.clone().combine(&style_with_fg(Color::default()));
                }
                40..=47 => {
                    let n = code - 40;
                    self.style = self
                        .style
                        .clone()
                        .combine(&style_with_bg(Color::from_ansi(n)));
                }
                49 => {
                    self.style = self.style.clone().combine(&style_with_bg(Color::default()));
                }
                51 => {
                    self.style = self
                        .style
                        .clone()
                        .combine(&style_with_attr(Attributes::FRAME));
                }
                52 => {
                    self.style = self
                        .style
                        .clone()
                        .combine(&style_with_attr(Attributes::ENCIRCLE));
                }
                53 => {
                    self.style = self
                        .style
                        .clone()
                        .combine(&style_with_attr(Attributes::OVERLINE));
                }
                54 => {
                    self.style = self
                        .style
                        .clone()
                        .not(Attributes::FRAME)
                        .not(Attributes::ENCIRCLE);
                }
                55 => self.style = self.style.clone().not(Attributes::OVERLINE),
                90..=97 => {
                    let n = code - 90 + 8;
                    self.style = self
                        .style
                        .clone()
                        .combine(&style_with_fg(Color::from_ansi(n)));
                }
                100..=107 => {
                    let n = code - 100 + 8;
                    self.style = self
                        .style
                        .clone()
                        .combine(&style_with_bg(Color::from_ansi(n)));
                }
                38 => {
                    // Foreground extended color.
                    let Some(color_type) = iter.next() else {
                        break;
                    };
                    match color_type {
                        5 => {
                            if let Some(n) = iter.next() {
                                self.style = self
                                    .style
                                    .clone()
                                    .combine(&style_with_fg(Color::from_ansi(n)));
                            }
                        }
                        2 => {
                            let (Some(r), Some(g), Some(b)) =
                                (iter.next(), iter.next(), iter.next())
                            else {
                                break;
                            };
                            self.style = self
                                .style
                                .clone()
                                .combine(&style_with_fg(Color::from_rgb(r, g, b)));
                        }
                        _ => {}
                    }
                }
                48 => {
                    // Background extended color.
                    let Some(color_type) = iter.next() else {
                        break;
                    };
                    match color_type {
                        5 => {
                            if let Some(n) = iter.next() {
                                self.style = self
                                    .style
                                    .clone()
                                    .combine(&style_with_bg(Color::from_ansi(n)));
                            }
                        }
                        2 => {
                            let (Some(r), Some(g), Some(b)) =
                                (iter.next(), iter.next(), iter.next())
                            else {
                                break;
                            };
                            self.style = self
                                .style
                                .clone()
                                .combine(&style_with_bg(Color::from_rgb(r, g, b)));
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }

    fn normalize_style_nullness(&mut self) {
        if self.style.attributes.is_empty()
            && self.style.set_attributes.is_empty()
            && self.style.color.is_none()
            && self.style.bgcolor.is_none()
            && self.style.link.is_none()
            && self.style.link_id.is_none()
            && self.style.meta.is_none()
        {
            self.style = Style::null();
        }
    }
}

fn style_with_attr(attr: Attributes) -> Style {
    let mut style = Style::new();
    style.attributes.insert(attr);
    style.set_attributes.insert(attr);
    style
}

fn style_with_fg(color: Color) -> Style {
    let mut style = Style::new();
    style.color = Some(color);
    style
}

fn style_with_bg(color: Color) -> Style {
    let mut style = Style::new();
    style.bgcolor = Some(color);
    style
}

fn parse_csi(line: &str, esc_pos: usize) -> Option<(usize, u8, &str)> {
    let bytes = line.as_bytes();
    debug_assert_eq!(bytes.get(esc_pos), Some(&0x1b));
    debug_assert_eq!(bytes.get(esc_pos + 1), Some(&b'['));

    let mut j = esc_pos + 2;
    while j < bytes.len() {
        let b = bytes[j];
        if (0x40..=0x7e).contains(&b) {
            // Final byte in CSI.
            let params = &line[esc_pos + 2..j];
            return Some((j + 1, b, params));
        }
        j += 1;
    }
    None
}

fn parse_osc(line: &str, esc_pos: usize) -> Option<(usize, &str)> {
    let bytes = line.as_bytes();
    debug_assert_eq!(bytes.get(esc_pos), Some(&0x1b));
    debug_assert_eq!(bytes.get(esc_pos + 1), Some(&b']'));

    let mut j = esc_pos + 2;
    while j + 1 < bytes.len() {
        if bytes[j] == 0x1b && bytes[j + 1] == b'\\' {
            let content = &line[esc_pos + 2..j];
            return Some((j + 2, content));
        }
        j += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_line_plain_text() {
        let mut decoder = AnsiDecoder::new();
        let text = decoder.decode_line("hello");
        assert_eq!(text.plain(), "hello");
    }

    #[test]
    fn decode_line_sgr_bold_red_reset() {
        let mut decoder = AnsiDecoder::new();
        let text = decoder.decode_line("\u{1b}[1;31mHi\u{1b}[0m!");
        let rendered = text.render("");
        // There should be at least one styled segment (the "Hi").
        assert!(rendered.iter().any(|seg| seg.text.as_ref().contains("Hi")));
        assert_eq!(text.plain(), "Hi!");
    }

    #[test]
    fn decode_line_osc8_link_set_and_clear() {
        let mut decoder = AnsiDecoder::new();
        let s = "\u{1b}]8;;https://example.com\u{1b}\\link\u{1b}]8;;\u{1b}\\";
        let text = decoder.decode_line(s);
        assert_eq!(text.plain(), "link");
        let segments = text.render("");
        assert!(
            segments
                .iter()
                .filter_map(|seg| seg.style.as_ref())
                .any(|style| style.link.as_deref() == Some("https://example.com")),
            "expected a segment style with OSC8 link"
        );
    }
}
