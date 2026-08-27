//! Fake terminal implementation for headless testing.
//!
//! This module provides a `FakeTerminal` struct that simulates a terminal
//! for testing purposes without requiring an actual TTY.
//!
//! # Features
//!
//! - Configurable dimensions (width, height)
//! - Color system simulation (4-bit, 8-bit, 24-bit)
//! - ANSI sequence capture and parsing
//! - Cursor position tracking
//! - Screen buffer with cell storage
//! - Input injection for interactive testing
//!
//! # Example
//!
//! ```rust,ignore
//! use common::fake_terminal::FakeTerminal;
//!
//! let mut term = FakeTerminal::new(80, 24);
//! term.write_str("\x1b[1mBold Text\x1b[0m");
//!
//! assert_eq!(term.cursor_position(), (9, 0));
//! assert!(term.has_style_at(0, 0, StyleAttribute::Bold));
//! ```

#![allow(dead_code)]

use std::collections::VecDeque;

// =============================================================================
// Types
// =============================================================================

/// A cell in the terminal screen buffer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TerminalCell {
    /// The character at this cell (empty for ' ').
    pub char: char,
    /// Active SGR codes when this cell was written.
    pub sgr_codes: Vec<u8>,
}

impl TerminalCell {
    /// Create an empty cell.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            char: ' ',
            sgr_codes: Vec::new(),
        }
    }

    /// Create a cell with a character.
    #[must_use]
    pub fn new(char: char) -> Self {
        Self {
            char,
            sgr_codes: Vec::new(),
        }
    }

    /// Create a cell with a character and styles.
    #[must_use]
    pub fn with_styles(char: char, sgr_codes: Vec<u8>) -> Self {
        Self { char, sgr_codes }
    }

    /// Check if this cell has bold styling.
    #[must_use]
    pub fn is_bold(&self) -> bool {
        self.sgr_codes.contains(&1)
    }

    /// Check if this cell has italic styling.
    #[must_use]
    pub fn is_italic(&self) -> bool {
        self.sgr_codes.contains(&3)
    }

    /// Check if this cell has underline styling.
    #[must_use]
    pub fn is_underline(&self) -> bool {
        self.sgr_codes.contains(&4)
    }

    /// Check if this cell has a foreground color.
    #[must_use]
    pub fn has_foreground_color(&self) -> bool {
        self.sgr_codes
            .iter()
            .any(|&c| (30..=37).contains(&c) || (90..=97).contains(&c) || c == 38)
    }
}

/// Style attributes that can be checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleAttribute {
    Bold,
    Italic,
    Underline,
    Dim,
    Blink,
    Reverse,
    Strikethrough,
    ForegroundColor,
    BackgroundColor,
}

/// Color system simulation level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TerminalColorSystem {
    /// No colors (strip all color codes).
    NoColor,
    /// 4-bit ANSI colors (16 colors).
    #[default]
    Standard,
    /// 8-bit colors (256 colors).
    EightBit,
    /// 24-bit RGB colors.
    TrueColor,
}

// =============================================================================
// FakeTerminal
// =============================================================================

/// A simulated terminal for testing.
///
/// Provides a complete terminal emulation including:
/// - Screen buffer with character cells
/// - Cursor position tracking
/// - ANSI escape sequence parsing
/// - Style state management
/// - Input queue for interactive testing
#[derive(Debug)]
pub struct FakeTerminal {
    /// Terminal width in columns.
    width: usize,
    /// Terminal height in rows.
    height: usize,
    /// Screen buffer (rows x cols).
    screen: Vec<Vec<TerminalCell>>,
    /// Current cursor column (0-indexed).
    cursor_col: usize,
    /// Current cursor row (0-indexed).
    cursor_row: usize,
    /// Current active SGR codes.
    active_sgr: Vec<u8>,
    /// Color system to simulate.
    color_system: TerminalColorSystem,
    /// Whether terminal mode is forced (always reports as TTY).
    force_terminal: bool,
    /// Input queue for simulating user input.
    input_queue: VecDeque<String>,
    /// All captured raw output.
    raw_output: String,
    /// Parsed ANSI sequences.
    sequences: Vec<CapturedSequence>,
    /// Saved cursor positions (for \x1b[s / \x1b[u).
    saved_cursor: Option<(usize, usize)>,
}

/// A captured ANSI sequence with metadata.
#[derive(Debug, Clone)]
pub struct CapturedSequence {
    /// The raw sequence string.
    pub raw: String,
    /// The position in the output where this occurred.
    pub position: usize,
    /// The cursor position when this was received.
    pub cursor: (usize, usize),
    /// Parsed sequence type.
    pub kind: SequenceKind,
}

/// Types of ANSI sequences.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SequenceKind {
    /// SGR (style) sequence.
    Sgr(Vec<u8>),
    /// Cursor movement.
    CursorMove {
        col: Option<usize>,
        row: Option<usize>,
    },
    /// Cursor save.
    CursorSave,
    /// Cursor restore.
    CursorRestore,
    /// Clear screen.
    ClearScreen(ClearMode),
    /// Clear line.
    ClearLine(ClearMode),
    /// OSC (hyperlink, title, etc.).
    Osc(String),
    /// Unknown sequence.
    Unknown(String),
}

/// Clear mode for erase operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClearMode {
    /// Clear from cursor to end.
    ToEnd,
    /// Clear from start to cursor.
    ToStart,
    /// Clear entire line/screen.
    All,
}

impl Default for FakeTerminal {
    fn default() -> Self {
        Self::new(80, 24)
    }
}

impl FakeTerminal {
    /// Create a new fake terminal with given dimensions.
    #[must_use]
    pub fn new(width: usize, height: usize) -> Self {
        let screen = vec![vec![TerminalCell::empty(); width]; height];
        Self {
            width,
            height,
            screen,
            cursor_col: 0,
            cursor_row: 0,
            active_sgr: Vec::new(),
            color_system: TerminalColorSystem::default(),
            force_terminal: true,
            input_queue: VecDeque::new(),
            raw_output: String::new(),
            sequences: Vec::new(),
            saved_cursor: None,
        }
    }

    /// Create a terminal with a specific color system.
    #[must_use]
    pub fn with_color_system(mut self, system: TerminalColorSystem) -> Self {
        self.color_system = system;
        self
    }

    /// Set whether this terminal is forced to act as a TTY.
    #[must_use]
    pub fn force_terminal(mut self, force: bool) -> Self {
        self.force_terminal = force;
        self
    }

    // =========================================================================
    // Dimensions
    // =========================================================================

    /// Get terminal width.
    #[must_use]
    pub fn width(&self) -> usize {
        self.width
    }

    /// Get terminal height.
    #[must_use]
    pub fn height(&self) -> usize {
        self.height
    }

    /// Resize the terminal.
    pub fn resize(&mut self, width: usize, height: usize) {
        self.width = width;
        self.height = height;

        // Resize screen buffer
        self.screen
            .resize(height, vec![TerminalCell::empty(); width]);
        for row in &mut self.screen {
            row.resize(width, TerminalCell::empty());
        }

        // Clamp cursor
        self.cursor_col = self.cursor_col.min(width.saturating_sub(1));
        self.cursor_row = self.cursor_row.min(height.saturating_sub(1));
    }

    // =========================================================================
    // Cursor
    // =========================================================================

    /// Get current cursor position (col, row).
    #[must_use]
    pub fn cursor_position(&self) -> (usize, usize) {
        (self.cursor_col, self.cursor_row)
    }

    /// Set cursor position.
    pub fn set_cursor(&mut self, col: usize, row: usize) {
        self.cursor_col = col.min(self.width.saturating_sub(1));
        self.cursor_row = row.min(self.height.saturating_sub(1));
    }

    /// Move cursor by relative amount.
    pub fn move_cursor(&mut self, col_delta: isize, row_delta: isize) {
        let new_col = (self.cursor_col as isize + col_delta).max(0) as usize;
        let new_row = (self.cursor_row as isize + row_delta).max(0) as usize;
        self.set_cursor(new_col, new_row);
    }

    // =========================================================================
    // Writing
    // =========================================================================

    /// Write a string to the terminal, parsing ANSI sequences.
    pub fn write_str(&mut self, s: &str) {
        self.raw_output.push_str(s);
        let mut chars = s.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '\x1b' {
                // Start of escape sequence
                let seq_start = self.raw_output.len() - 1;
                let mut seq = String::from('\x1b');

                if let Some(&next) = chars.peek() {
                    seq.push(chars.next().unwrap());

                    match next {
                        '[' => {
                            // CSI sequence
                            while let Some(&c) = chars.peek() {
                                seq.push(chars.next().unwrap());
                                // Final byte is 0x40-0x7E
                                if c.is_ascii() && (0x40..=0x7E).contains(&(c as u8)) {
                                    break;
                                }
                            }
                            self.handle_csi_sequence(&seq, seq_start);
                        }
                        ']' => {
                            // OSC sequence - read until ST or BEL
                            while let Some(&c) = chars.peek() {
                                seq.push(chars.next().unwrap());
                                if c == '\x07' || seq.ends_with("\x1b\\") {
                                    break;
                                }
                            }
                            self.handle_osc_sequence(&seq, seq_start);
                        }
                        _ => {
                            // Unknown escape
                            self.sequences.push(CapturedSequence {
                                raw: seq.clone(),
                                position: seq_start,
                                cursor: self.cursor_position(),
                                kind: SequenceKind::Unknown(seq),
                            });
                        }
                    }
                }
            } else if c == '\n' {
                self.cursor_col = 0;
                self.cursor_row += 1;
                if self.cursor_row >= self.height {
                    self.scroll_up();
                    self.cursor_row = self.height - 1;
                }
            } else if c == '\r' {
                self.cursor_col = 0;
            } else if c == '\t' {
                // Tab to next 8-column boundary
                let next_tab = ((self.cursor_col / 8) + 1) * 8;
                self.cursor_col = next_tab.min(self.width - 1);
            } else {
                // Regular character
                self.write_char(c);
            }
        }
    }

    /// Write a single character at the current cursor position.
    fn write_char(&mut self, c: char) {
        if self.cursor_col < self.width && self.cursor_row < self.height {
            self.screen[self.cursor_row][self.cursor_col] =
                TerminalCell::with_styles(c, self.active_sgr.clone());
            self.cursor_col += 1;

            // Handle line wrap
            if self.cursor_col >= self.width {
                self.cursor_col = 0;
                self.cursor_row += 1;
                if self.cursor_row >= self.height {
                    self.scroll_up();
                    self.cursor_row = self.height - 1;
                }
            }
        }
    }

    /// Handle a CSI sequence (Control Sequence Introducer).
    fn handle_csi_sequence(&mut self, seq: &str, position: usize) {
        let params = &seq[2..seq.len() - 1];
        let final_byte = seq.chars().last().unwrap_or('?');

        let kind = match final_byte {
            'm' => {
                // SGR sequence
                let codes = self.parse_sgr_params(params);
                self.apply_sgr(&codes);
                SequenceKind::Sgr(codes)
            }
            'H' | 'f' => {
                // Cursor position
                let parts: Vec<&str> = params.split(';').collect();
                let row: usize = parts.first().and_then(|s| s.parse().ok()).unwrap_or(1);
                let col: usize = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
                self.set_cursor(col.saturating_sub(1), row.saturating_sub(1));
                SequenceKind::CursorMove {
                    col: Some(col.saturating_sub(1)),
                    row: Some(row.saturating_sub(1)),
                }
            }
            'A' => {
                // Cursor up
                let n: isize = params.parse().unwrap_or(1);
                self.move_cursor(0, -n);
                SequenceKind::CursorMove {
                    col: None,
                    row: Some(self.cursor_row),
                }
            }
            'B' => {
                // Cursor down
                let n: isize = params.parse().unwrap_or(1);
                self.move_cursor(0, n);
                SequenceKind::CursorMove {
                    col: None,
                    row: Some(self.cursor_row),
                }
            }
            'C' => {
                // Cursor forward
                let n: isize = params.parse().unwrap_or(1);
                self.move_cursor(n, 0);
                SequenceKind::CursorMove {
                    col: Some(self.cursor_col),
                    row: None,
                }
            }
            'D' => {
                // Cursor back
                let n: isize = params.parse().unwrap_or(1);
                self.move_cursor(-n, 0);
                SequenceKind::CursorMove {
                    col: Some(self.cursor_col),
                    row: None,
                }
            }
            's' => {
                // Save cursor
                self.saved_cursor = Some((self.cursor_col, self.cursor_row));
                SequenceKind::CursorSave
            }
            'u' => {
                // Restore cursor
                if let Some((col, row)) = self.saved_cursor {
                    self.set_cursor(col, row);
                }
                SequenceKind::CursorRestore
            }
            'J' => {
                // Clear screen
                let mode = match params {
                    "0" | "" => ClearMode::ToEnd,
                    "1" => ClearMode::ToStart,
                    "2" | "3" => ClearMode::All,
                    _ => ClearMode::ToEnd,
                };
                self.clear_screen(mode);
                SequenceKind::ClearScreen(mode)
            }
            'K' => {
                // Clear line
                let mode = match params {
                    "0" | "" => ClearMode::ToEnd,
                    "1" => ClearMode::ToStart,
                    "2" => ClearMode::All,
                    _ => ClearMode::ToEnd,
                };
                self.clear_line(mode);
                SequenceKind::ClearLine(mode)
            }
            _ => SequenceKind::Unknown(seq.to_string()),
        };

        self.sequences.push(CapturedSequence {
            raw: seq.to_string(),
            position,
            cursor: self.cursor_position(),
            kind,
        });
    }

    /// Handle an OSC sequence (Operating System Command).
    fn handle_osc_sequence(&mut self, seq: &str, position: usize) {
        self.sequences.push(CapturedSequence {
            raw: seq.to_string(),
            position,
            cursor: self.cursor_position(),
            kind: SequenceKind::Osc(seq.to_string()),
        });
    }

    /// Parse SGR parameters string into codes.
    fn parse_sgr_params(&self, params: &str) -> Vec<u8> {
        if params.is_empty() {
            return vec![0];
        }
        params.split(';').filter_map(|s| s.parse().ok()).collect()
    }

    /// Apply SGR codes to the active style state.
    fn apply_sgr(&mut self, codes: &[u8]) {
        for &code in codes {
            match code {
                0 => self.active_sgr.clear(),
                _ => {
                    if !self.active_sgr.contains(&code) {
                        self.active_sgr.push(code);
                    }
                }
            }
        }
    }

    /// Scroll the screen up by one line.
    fn scroll_up(&mut self) {
        self.screen.remove(0);
        self.screen.push(vec![TerminalCell::empty(); self.width]);
    }

    /// Clear the screen according to mode.
    fn clear_screen(&mut self, mode: ClearMode) {
        match mode {
            ClearMode::All => {
                for row in &mut self.screen {
                    for cell in row {
                        *cell = TerminalCell::empty();
                    }
                }
            }
            ClearMode::ToEnd => {
                // Clear from cursor to end
                for col in self.cursor_col..self.width {
                    self.screen[self.cursor_row][col] = TerminalCell::empty();
                }
                for row in (self.cursor_row + 1)..self.height {
                    for cell in &mut self.screen[row] {
                        *cell = TerminalCell::empty();
                    }
                }
            }
            ClearMode::ToStart => {
                // Clear from start to cursor
                for row in 0..self.cursor_row {
                    for cell in &mut self.screen[row] {
                        *cell = TerminalCell::empty();
                    }
                }
                for col in 0..=self.cursor_col {
                    self.screen[self.cursor_row][col] = TerminalCell::empty();
                }
            }
        }
    }

    /// Clear the current line according to mode.
    fn clear_line(&mut self, mode: ClearMode) {
        match mode {
            ClearMode::All => {
                for cell in &mut self.screen[self.cursor_row] {
                    *cell = TerminalCell::empty();
                }
            }
            ClearMode::ToEnd => {
                for col in self.cursor_col..self.width {
                    self.screen[self.cursor_row][col] = TerminalCell::empty();
                }
            }
            ClearMode::ToStart => {
                for col in 0..=self.cursor_col {
                    self.screen[self.cursor_row][col] = TerminalCell::empty();
                }
            }
        }
    }

    // =========================================================================
    // Input injection
    // =========================================================================

    /// Queue input for later reading.
    pub fn inject_input(&mut self, input: impl Into<String>) {
        self.input_queue.push_back(input.into());
    }

    /// Read queued input.
    pub fn read_input(&mut self) -> Option<String> {
        self.input_queue.pop_front()
    }

    /// Check if there's queued input.
    #[must_use]
    pub fn has_input(&self) -> bool {
        !self.input_queue.is_empty()
    }

    // =========================================================================
    // Screen inspection
    // =========================================================================

    /// Get the character at a position.
    #[must_use]
    pub fn char_at(&self, col: usize, row: usize) -> Option<char> {
        self.screen
            .get(row)
            .and_then(|r| r.get(col))
            .map(|c| c.char)
    }

    /// Get the cell at a position.
    #[must_use]
    pub fn cell_at(&self, col: usize, row: usize) -> Option<&TerminalCell> {
        self.screen.get(row).and_then(|r| r.get(col))
    }

    /// Check if a position has a specific style attribute.
    #[must_use]
    pub fn has_style_at(&self, col: usize, row: usize, attr: StyleAttribute) -> bool {
        self.cell_at(col, row).is_some_and(|cell| match attr {
            StyleAttribute::Bold => cell.is_bold(),
            StyleAttribute::Italic => cell.is_italic(),
            StyleAttribute::Underline => cell.is_underline(),
            StyleAttribute::Dim => cell.sgr_codes.contains(&2),
            StyleAttribute::Blink => cell.sgr_codes.contains(&5),
            StyleAttribute::Reverse => cell.sgr_codes.contains(&7),
            StyleAttribute::Strikethrough => cell.sgr_codes.contains(&9),
            StyleAttribute::ForegroundColor => cell.has_foreground_color(),
            StyleAttribute::BackgroundColor => cell
                .sgr_codes
                .iter()
                .any(|&c| (40..=47).contains(&c) || (100..=107).contains(&c) || c == 48),
        })
    }

    /// Get a row as plain text.
    #[must_use]
    pub fn row_text(&self, row: usize) -> String {
        self.screen
            .get(row)
            .map(|r| r.iter().map(|c| c.char).collect::<String>())
            .unwrap_or_default()
            .trim_end()
            .to_string()
    }

    /// Get all rows as plain text.
    #[must_use]
    pub fn screen_text(&self) -> String {
        (0..self.height)
            .map(|row| self.row_text(row))
            .collect::<Vec<_>>()
            .join("\n")
            .trim_end()
            .to_string()
    }

    /// Get the raw captured output.
    #[must_use]
    pub fn raw_output(&self) -> &str {
        &self.raw_output
    }

    /// Get all captured sequences.
    #[must_use]
    pub fn sequences(&self) -> &[CapturedSequence] {
        &self.sequences
    }

    /// Count sequences of a specific kind.
    #[must_use]
    pub fn count_sgr_sequences(&self) -> usize {
        self.sequences
            .iter()
            .filter(|s| matches!(s.kind, SequenceKind::Sgr(_)))
            .count()
    }

    /// Clear all captured data.
    pub fn reset(&mut self) {
        self.raw_output.clear();
        self.sequences.clear();
        self.active_sgr.clear();
        self.cursor_col = 0;
        self.cursor_row = 0;
        for row in &mut self.screen {
            for cell in row {
                *cell = TerminalCell::empty();
            }
        }
    }

    /// Check if terminal is being forced.
    #[must_use]
    pub fn is_force_terminal(&self) -> bool {
        self.force_terminal
    }

    /// Get the simulated color system.
    #[must_use]
    pub fn color_system(&self) -> TerminalColorSystem {
        self.color_system
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_terminal() {
        let term = FakeTerminal::new(80, 24);
        assert_eq!(term.width(), 80);
        assert_eq!(term.height(), 24);
        assert_eq!(term.cursor_position(), (0, 0));
    }

    #[test]
    fn test_write_plain_text() {
        let mut term = FakeTerminal::new(80, 24);
        term.write_str("Hello");

        assert_eq!(term.cursor_position(), (5, 0));
        assert_eq!(term.row_text(0), "Hello");
    }

    #[test]
    fn test_write_newline() {
        let mut term = FakeTerminal::new(80, 24);
        term.write_str("Line 1\nLine 2");

        assert_eq!(term.row_text(0), "Line 1");
        assert_eq!(term.row_text(1), "Line 2");
        assert_eq!(term.cursor_position(), (6, 1));
    }

    #[test]
    fn test_sgr_bold() {
        let mut term = FakeTerminal::new(80, 24);
        term.write_str("\x1b[1mBold\x1b[0m Normal");

        assert!(term.has_style_at(0, 0, StyleAttribute::Bold));
        assert!(term.has_style_at(3, 0, StyleAttribute::Bold));
        assert!(!term.has_style_at(5, 0, StyleAttribute::Bold)); // Space after reset
    }

    #[test]
    fn test_sgr_color() {
        let mut term = FakeTerminal::new(80, 24);
        term.write_str("\x1b[31mRed\x1b[0m");

        assert!(term.has_style_at(0, 0, StyleAttribute::ForegroundColor));
    }

    #[test]
    fn test_cursor_movement() {
        let mut term = FakeTerminal::new(80, 24);
        term.write_str("\x1b[10;5H"); // Move to row 10, col 5 (1-indexed)

        assert_eq!(term.cursor_position(), (4, 9)); // 0-indexed
    }

    #[test]
    fn test_clear_screen() {
        let mut term = FakeTerminal::new(80, 24);
        term.write_str("Hello World");
        term.write_str("\x1b[2J"); // Clear all

        assert_eq!(term.row_text(0), "");
    }

    #[test]
    fn test_input_injection() {
        let mut term = FakeTerminal::new(80, 24);
        term.inject_input("y\n");

        assert!(term.has_input());
        assert_eq!(term.read_input(), Some("y\n".to_string()));
        assert!(!term.has_input());
    }

    #[test]
    fn test_screen_text() {
        let mut term = FakeTerminal::new(80, 24);
        term.write_str("Line 1\nLine 2\nLine 3");

        let screen = term.screen_text();
        assert!(screen.contains("Line 1"));
        assert!(screen.contains("Line 2"));
        assert!(screen.contains("Line 3"));
    }

    #[test]
    fn test_resize() {
        let mut term = FakeTerminal::new(80, 24);
        term.write_str("Hello");
        term.resize(40, 12);

        assert_eq!(term.width(), 40);
        assert_eq!(term.height(), 12);
        assert_eq!(term.row_text(0), "Hello");
    }

    #[test]
    fn test_captured_sequences() {
        let mut term = FakeTerminal::new(80, 24);
        term.write_str("\x1b[1mBold\x1b[0m and \x1b[32mGreen\x1b[0m");

        let sgr_count = term.count_sgr_sequences();
        assert!(sgr_count >= 4); // At least bold, reset, green, reset
    }

    #[test]
    fn test_reset() {
        let mut term = FakeTerminal::new(80, 24);
        term.write_str("\x1b[1mHello\x1b[0m");
        term.reset();

        assert_eq!(term.cursor_position(), (0, 0));
        assert_eq!(term.row_text(0), "");
        assert!(term.sequences().is_empty());
    }
}
