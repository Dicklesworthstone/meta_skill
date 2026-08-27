//! Streaming parser for terminal input bytes.
//!
//! [`Parser`] turns bytes from a terminal or PTY into [`Event`] values. It accepts partial input:
//! callers append bytes with [`Parser::parse`], then drain completed events with [`Parser::pop`].
//! This is the same parser used behind [`EventReader`], exposed for code that owns its input source
//! directly.
//!
//! # Implementation Notes
//!
//! The parser is adapted from [crossterm's Unix event parser], with additions for Termina-specific
//! escape sequences and Windows input modes. Crossterm comments call this style of parser a bit
//! scary and probably in need of a refactor. I like the approach though, because it is quite easy
//! to read and test. The main uncertainty is performance: `process_bytes` considers the bytes as an
//! increasing slice of the buffer until that slice becomes valid or invalid. WezTerm and Alacritty
//! use more formal parsers, [`vtparse`] and [`vte`] respectively, but those terminal-emulator
//! parsers may be larger or more complex than an application needs.
//!
//! [crossterm's Unix event parser]: https://docs.rs/crossterm/latest/crossterm/event/index.html
//! [`vtparse`]: https://docs.rs/vtparse/latest/vtparse/
//! [`vte`]: https://docs.rs/vte/latest/vte/

#[cfg(windows)]
pub mod windows;

#[cfg(all(windows, feature = "windows-legacy"))]
use windows::legacy;
#[cfg(windows)]
use windows::InputReaderMode;

use std::{collections::VecDeque, num::NonZeroU16, str};

#[cfg(doc)]
use crate::EventReader;

use crate::{
    escape::{
        self,
        csi::{self, Csi, KittyKeyboardFlags, ThemeMode},
        dcs, osc,
    },
    event::{
        KeyCode, KeyEvent, KeyEventKind, KeyEventState, MediaKeyCode, ModifierKeyCode, Modifiers,
        MouseButton, MouseEvent, MouseEventKind,
    },
    style, Event,
};

/// An incremental parser for terminal input.
///
/// The parser keeps incomplete escape sequences in an internal buffer. Pass `maybe_more = true`
/// when the current byte slice may end in the middle of a sequence. Completed events are queued
/// until [`Self::pop`] removes them.
///
/// # Examples
///
/// ```
/// use termina::{Event, Parser};
///
/// let mut parser = Parser::default();
/// parser.parse(b"\x1b[5~", false);
/// assert!(matches!(parser.pop(), Some(Event::Key(_))));
/// ```
#[derive(Debug)]
pub struct Parser {
    buffer: Vec<u8>,
    /// Events which have been parsed. Pop out with [`Self::pop`].
    events: VecDeque<Event>,
    #[cfg(windows)]
    mode: InputReaderMode,
    #[cfg(all(windows, feature = "windows-legacy"))]
    surrogate_buffer: Option<u16>,
    #[cfg(all(windows, feature = "windows-legacy"))]
    mouse_buttons_pressed: legacy::MouseButtonsPressed,
}

impl Default for Parser {
    fn default() -> Self {
        Self {
            buffer: Vec::with_capacity(256),
            events: VecDeque::with_capacity(32),
            #[cfg(windows)]
            mode: InputReaderMode::Vte,
            #[cfg(all(windows, feature = "windows-legacy"))]
            surrogate_buffer: None,
            #[cfg(all(windows, feature = "windows-legacy"))]
            mouse_buttons_pressed: legacy::MouseButtonsPressed::default(),
        }
    }
}

impl Parser {
    // The parser is publicly accessible, but we don't currently expose methods for parsing input records, just VTE.
    // So there's no need to make this public.
    #[cfg(windows)]
    pub(crate) fn with_mode(mode: InputReaderMode) -> Self {
        Self {
            mode,
            ..Default::default()
        }
    }

    /// Removes and returns the oldest completed event.
    pub fn pop(&mut self) -> Option<Event> {
        self.events.pop_front()
    }

    /// Adds bytes to the parser and queues any completed events.
    ///
    /// Set `maybe_more` to `true` when the input source may provide more bytes for the same
    /// escape sequence later. Set it to `false` when the buffer should be treated as complete for
    /// now; malformed or incomplete sequences can then be discarded instead of held indefinitely.
    pub fn parse(&mut self, bytes: &[u8], maybe_more: bool) {
        self.buffer.extend_from_slice(bytes);
        self.process_bytes(maybe_more);
    }

    fn process_bytes(&mut self, maybe_more: bool) {
        let mut start = 0;
        for n in 0..self.buffer.len() {
            let end = n + 1;
            match parse_event(
                &self.buffer[start..end],
                maybe_more || end < self.buffer.len(),
            ) {
                Ok(Some(event)) => {
                    self.events.push_back(event);
                    start = end;
                }
                Ok(None) => continue,
                Err(_) => start = end,
            }
        }
        self.advance(start);
    }

    fn advance(&mut self, len: usize) {
        if len == 0 {
            return;
        }
        let remain = self.buffer.len() - len;
        self.buffer.rotate_left(len);
        self.buffer.truncate(remain);
    }
}

#[derive(Debug)]
struct MalformedSequenceError;

// This is a bit hacky but cuts down on boilerplate conversions
impl From<str::Utf8Error> for MalformedSequenceError {
    fn from(_: str::Utf8Error) -> Self {
        Self
    }
}

type Result<T> = std::result::Result<T, MalformedSequenceError>;

macro_rules! bail {
    () => {
        return Err(MalformedSequenceError)
    };
}

fn parse_event(buffer: &[u8], maybe_more: bool) -> Result<Option<Event>> {
    if buffer.is_empty() {
        return Ok(None);
    }

    match buffer[0] {
        b'\x1B' => {
            if buffer.len() == 1 {
                if maybe_more {
                    // Possible Esc sequence
                    Ok(None)
                } else {
                    Ok(Some(Event::Key(KeyCode::Escape.into())))
                }
            } else {
                match buffer[1] {
                    b'O' => {
                        if buffer.len() == 2 {
                            Ok(None)
                        } else {
                            match buffer[2] {
                                b'D' => Ok(Some(Event::Key(KeyCode::Left.into()))),
                                b'C' => Ok(Some(Event::Key(KeyCode::Right.into()))),
                                b'A' => Ok(Some(Event::Key(KeyCode::Up.into()))),
                                b'B' => Ok(Some(Event::Key(KeyCode::Down.into()))),
                                b'H' => Ok(Some(Event::Key(KeyCode::Home.into()))),
                                b'F' => Ok(Some(Event::Key(KeyCode::End.into()))),
                                // F1-F4
                                val @ b'P'..=b'S' => {
                                    Ok(Some(Event::Key(KeyCode::Function(1 + val - b'P').into())))
                                }
                                _ => bail!(),
                            }
                        }
                    }
                    b'[' => parse_csi(buffer),
                    b']' => parse_osc(buffer),
                    b'P' => parse_dcs(buffer),
                    b'\x1B' => Ok(Some(Event::Key(KeyCode::Escape.into()))),
                    _ => parse_event(&buffer[1..], maybe_more).map(|event_option| {
                        event_option.map(|event| {
                            if let Event::Key(key_event) = event {
                                let mut alt_key_event = key_event;
                                alt_key_event.modifiers |= Modifiers::ALT;
                                Event::Key(alt_key_event)
                            } else {
                                event
                            }
                        })
                    }),
                }
            }
        }
        b'\r' => Ok(Some(Event::Key(KeyCode::Enter.into()))),
        b'\t' => Ok(Some(Event::Key(KeyCode::Tab.into()))),
        b'\x7F' => Ok(Some(Event::Key(KeyCode::Backspace.into()))),
        b'\0' => Ok(Some(Event::Key(KeyEvent::new(
            KeyCode::Char(' '),
            Modifiers::CONTROL,
        )))),
        c @ b'\x01'..=b'\x1A' => Ok(Some(Event::Key(KeyEvent::new(
            KeyCode::Char((c - 0x1 + b'a') as char),
            Modifiers::CONTROL,
        )))),
        c @ b'\x1C'..=b'\x1F' => Ok(Some(Event::Key(KeyEvent::new(
            KeyCode::Char((c - 0x1C + b'4') as char),
            Modifiers::CONTROL,
        )))),
        _ => parse_utf8_char(buffer).map(|maybe_char| {
            maybe_char.map(|ch| {
                let modifiers = if ch.is_uppercase() {
                    Modifiers::SHIFT
                } else {
                    Modifiers::NONE
                };
                Event::Key(KeyEvent::new(KeyCode::Char(ch), modifiers))
            })
        }),
    }
}

fn parse_utf8_char(buffer: &[u8]) -> Result<Option<char>> {
    assert!(!buffer.is_empty());
    match str::from_utf8(buffer) {
        Ok(s) => Ok(Some(s.chars().next().unwrap())),
        Err(_) => {
            // `from_utf8` failed but it could be because we don't have enough bytes to make a
            // valid UTF-8 codepoint. Check the validity of the bytes so far:
            let required_bytes = match buffer[0] {
                // https://en.wikipedia.org/wiki/UTF-8#Description
                (0x00..=0x7F) => 1, // 0xxxxxxx
                (0xC0..=0xDF) => 2, // 110xxxxx 10xxxxxx
                (0xE0..=0xEF) => 3, // 1110xxxx 10xxxxxx 10xxxxxx
                (0xF0..=0xF7) => 4, // 11110xxx 10xxxxxx 10xxxxxx 10xxxxxx
                (0x80..=0xBF) | (0xF8..=0xFF) => bail!(),
            };
            if required_bytes > 1 && buffer.len() > 1 {
                for byte in &buffer[1..] {
                    if byte & !0b0011_1111 != 0b1000_0000 {
                        bail!()
                    }
                }
            }
            if buffer.len() < required_bytes {
                Ok(None)
            } else {
                bail!()
            }
        }
    }
}

fn parse_csi(buffer: &[u8]) -> Result<Option<Event>> {
    assert!(buffer.starts_with(b"\x1B["));
    if buffer.len() == 2 {
        return Ok(None);
    }
    let maybe_event = match buffer[2] {
        b'[' => match buffer.get(3) {
            None => None,
            Some(b @ b'A'..=b'E') => Some(Event::Key(KeyCode::Function(1 + b - b'A').into())),
            Some(_) => bail!(),
        },
        b'D' => Some(Event::Key(KeyCode::Left.into())),
        b'C' => Some(Event::Key(KeyCode::Right.into())),
        b'A' => Some(Event::Key(KeyCode::Up.into())),
        b'B' => Some(Event::Key(KeyCode::Down.into())),
        b'H' => Some(Event::Key(KeyCode::Home.into())),
        b'F' => Some(Event::Key(KeyCode::End.into())),
        b'Z' => Some(Event::Key(KeyEvent {
            code: KeyCode::BackTab,
            modifiers: Modifiers::SHIFT,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })),
        b'M' => return parse_csi_normal_mouse(buffer),
        b'<' => return parse_csi_sgr_mouse(buffer),
        b'I' => Some(Event::FocusIn),
        b'O' => Some(Event::FocusOut),
        b';' => return parse_csi_modifier_key_code(buffer),
        // P, Q, and S for compatibility with Kitty keyboard protocol,
        // as the 1 in 'CSI 1 P' etc. must be omitted if there are no
        // modifiers pressed:
        // https://sw.kovidgoyal.net/kitty/keyboard-protocol/#legacy-functional-keys
        b'P' => Some(Event::Key(KeyCode::Function(1).into())),
        b'Q' => Some(Event::Key(KeyCode::Function(2).into())),
        b'S' => Some(Event::Key(KeyCode::Function(4).into())),
        b'?' => match buffer[buffer.len() - 1] {
            b'u' => return parse_csi_keyboard_enhancement_flags(buffer),
            b'c' => return parse_csi_primary_device_attributes(buffer),
            b'n' => return parse_csi_theme_mode(buffer),
            b'y' => return parse_csi_mode(buffer),
            _ => None,
        },
        b'>' => match buffer[buffer.len() - 2..buffer.len()] {
            [b' ', b'q'] => return parse_csi_cursor_shape_query_response(buffer),
            _ => None,
        },
        b'0'..=b'9' => {
            // Numbered escape code.
            if buffer.len() == 3 {
                None
            } else {
                // The final byte of a CSI sequence can be in the range 64-126, so
                // let's keep reading anything else.
                let last_byte = buffer[buffer.len() - 1];
                if !(64..=126).contains(&last_byte) {
                    None
                } else {
                    if buffer.starts_with(b"\x1B[200~") {
                        return parse_csi_bracketed_paste(buffer);
                    }
                    match last_byte {
                        b'M' => return parse_csi_rxvt_mouse(buffer),
                        b'~' => return parse_csi_special_key_code(buffer),
                        b'u' => return parse_csi_u_encoded_key_code(buffer),
                        b'R' => return parse_csi_cursor_position(buffer),
                        _ => return parse_csi_modifier_key_code(buffer),
                    }
                }
            }
        }
        _ => bail!(),
    };
    Ok(maybe_event)
}

fn parse_osc(buffer: &[u8]) -> Result<Option<Event>> {
    assert!(buffer.starts_with(b"\x1B]"));
    // > In addition to the ECMA-48 string terminator (ST), xterm(1) accepts a BEL to
    // > terminate an OSC string.
    // https://www.man7.org/linux/man-pages/man4/console_codes.4.html
    let Some(buffer) = buffer
        .strip_suffix(escape::ST.as_bytes())
        .or_else(|| buffer.strip_suffix(escape::BEL.as_bytes()))
    else {
        return Ok(None);
    };
    let s = str::from_utf8(&buffer[2..buffer.len()])?;
    let mut split = s.split(';');
    let index = next_parsed::<u8>(&mut split)?;
    let Some(color_number) = osc::DynamicColorNumber::from_index(index) else {
        bail!()
    };
    let Some(color_or_query) = split.next() else {
        bail!()
    };
    let response = match color_or_query {
        "?" => osc::ColorOrQuery::Query,
        _ => osc::ColorOrQuery::Color(color_or_query.parse().map_err(|_| MalformedSequenceError)?),
    };
    // This parsing could be expanded, see <https://terminalguide.namepad.de/seq/osc-4/>.
    Ok(Some(Event::Osc(osc::Osc::ChangeDynamicColors(
        color_number,
        vec![response],
    ))))
}

fn next_parsed<T>(iter: &mut dyn Iterator<Item = &str>) -> Result<T>
where
    T: str::FromStr,
{
    iter.next()
        .ok_or(MalformedSequenceError)?
        .parse::<T>()
        .map_err(|_| MalformedSequenceError)
}

fn modifier_and_kind_parsed(iter: &mut dyn Iterator<Item = &str>) -> Result<(u8, u8)> {
    let mut sub_split = iter.next().ok_or(MalformedSequenceError)?.split(':');

    let modifier_mask = next_parsed::<u8>(&mut sub_split)?;

    if let Ok(kind_code) = next_parsed::<u8>(&mut sub_split) {
        Ok((modifier_mask, kind_code))
    } else {
        Ok((modifier_mask, 1))
    }
}

fn parse_csi_u_encoded_key_code(buffer: &[u8]) -> Result<Option<Event>> {
    assert!(buffer.starts_with(b"\x1B")); // CSI
    assert!(buffer.ends_with(b"u"));

    // This function parses `CSI … u` sequences. These are sequences defined in either
    // the `CSI u` (a.k.a. "Fix Keyboard Input on Terminals - Please", https://www.leonerd.org.uk/hacks/fixterms/)
    // or Kitty Keyboard Protocol (https://sw.kovidgoyal.net/kitty/keyboard-protocol/) specifications.
    // This CSI sequence is a tuple of semicolon-separated numbers.
    let s = str::from_utf8(&buffer[2..buffer.len() - 1])?;
    let mut split = s.split(';');

    // In `CSI u`, this is parsed as:
    //
    //     CSI codepoint ; modifiers u
    //     codepoint: ASCII Dec value
    //
    // The Kitty Keyboard Protocol extends this with optional components that can be
    // enabled progressively. The full sequence is parsed as:
    //
    //     CSI unicode-key-code:alternate-key-codes ; modifiers:event-type ; text-as-codepoints u
    let mut codepoints = split.next().ok_or(MalformedSequenceError)?.split(':');

    let codepoint = codepoints
        .next()
        .ok_or(MalformedSequenceError)?
        .parse::<u32>()
        .map_err(|_| MalformedSequenceError)?;

    let (mut modifiers, kind, state_from_modifiers) =
        if let Ok((modifier_mask, kind_code)) = modifier_and_kind_parsed(&mut split) {
            (
                parse_modifiers(modifier_mask),
                parse_key_event_kind(kind_code),
                parse_modifiers_to_state(modifier_mask),
            )
        } else {
            (Modifiers::NONE, KeyEventKind::Press, KeyEventState::NONE)
        };

    let (mut code, state_from_keycode) = {
        if let Some((special_key_code, state)) = translate_functional_key_code(codepoint) {
            (special_key_code, state)
        } else if let Some(c) = char::from_u32(codepoint) {
            (
                match c {
                    '\x1B' => KeyCode::Escape,
                    '\r' => KeyCode::Enter,
                    /*
                    // Issue #371: \n = 0xA, which is also the keycode for Ctrl+J. The only reason we get
                    // newlines as input is because the terminal converts \r into \n for us. When we
                    // enter raw mode, we disable that, so \n no longer has any meaning - it's better to
                    // use Ctrl+J. Waiting to handle it here means it gets picked up later
                    '\n' if !crate::terminal::sys::is_raw_mode_enabled() => KeyCode::Enter,
                    */
                    '\t' => {
                        if modifiers.contains(Modifiers::SHIFT) {
                            KeyCode::BackTab
                        } else {
                            KeyCode::Tab
                        }
                    }
                    '\x7F' => KeyCode::Backspace,
                    _ => KeyCode::Char(c),
                },
                KeyEventState::empty(),
            )
        } else {
            bail!();
        }
    };

    if let KeyCode::Modifier(modifier_keycode) = code {
        match modifier_keycode {
            ModifierKeyCode::LeftAlt | ModifierKeyCode::RightAlt => {
                modifiers.set(Modifiers::ALT, true)
            }
            ModifierKeyCode::LeftControl | ModifierKeyCode::RightControl => {
                modifiers.set(Modifiers::CONTROL, true)
            }
            ModifierKeyCode::LeftShift | ModifierKeyCode::RightShift => {
                modifiers.set(Modifiers::SHIFT, true)
            }
            ModifierKeyCode::LeftSuper | ModifierKeyCode::RightSuper => {
                modifiers.set(Modifiers::SUPER, true)
            }
            ModifierKeyCode::LeftHyper | ModifierKeyCode::RightHyper => {
                modifiers.set(Modifiers::HYPER, true)
            }
            ModifierKeyCode::LeftMeta | ModifierKeyCode::RightMeta => {
                modifiers.set(Modifiers::META, true)
            }
            _ => {}
        }
    }

    // When the "report alternate keys" flag is enabled in the Kitty Keyboard Protocol
    // and the terminal sends a keyboard event containing shift, the sequence will
    // contain an additional codepoint separated by a ':' character which contains
    // the shifted character according to the keyboard layout.
    if modifiers.contains(Modifiers::SHIFT) {
        if let Some(shifted_c) = codepoints
            .next()
            .and_then(|codepoint| codepoint.parse::<u32>().ok())
            .and_then(char::from_u32)
        {
            code = KeyCode::Char(shifted_c);
            modifiers.set(Modifiers::SHIFT, false);
        }
    }

    let event = Event::Key(KeyEvent {
        code,
        modifiers,
        kind,
        state: state_from_keycode | state_from_modifiers,
    });

    Ok(Some(event))
}

fn parse_modifiers(mask: u8) -> Modifiers {
    let modifier_mask = mask.saturating_sub(1);
    let mut modifiers = Modifiers::empty();
    if modifier_mask & 1 != 0 {
        modifiers |= Modifiers::SHIFT;
    }
    if modifier_mask & 2 != 0 {
        modifiers |= Modifiers::ALT;
    }
    if modifier_mask & 4 != 0 {
        modifiers |= Modifiers::CONTROL;
    }
    if modifier_mask & 8 != 0 {
        modifiers |= Modifiers::SUPER;
    }
    if modifier_mask & 16 != 0 {
        modifiers |= Modifiers::HYPER;
    }
    if modifier_mask & 32 != 0 {
        modifiers |= Modifiers::META;
    }
    modifiers
}

fn parse_modifiers_to_state(mask: u8) -> KeyEventState {
    let modifier_mask = mask.saturating_sub(1);
    let mut state = KeyEventState::empty();
    if modifier_mask & 64 != 0 {
        state |= KeyEventState::CAPS_LOCK;
    }
    if modifier_mask & 128 != 0 {
        state |= KeyEventState::NUM_LOCK;
    }
    state
}

fn parse_key_event_kind(kind: u8) -> KeyEventKind {
    match kind {
        1 => KeyEventKind::Press,
        2 => KeyEventKind::Repeat,
        3 => KeyEventKind::Release,
        _ => KeyEventKind::Press,
    }
}

fn parse_csi_modifier_key_code(buffer: &[u8]) -> Result<Option<Event>> {
    assert!(buffer.starts_with(b"\x1B[")); // CSI
    let s = str::from_utf8(&buffer[2..buffer.len() - 1])?;
    let mut split = s.split(';');

    split.next();

    let (modifiers, kind) =
        if let Ok((modifier_mask, kind_code)) = modifier_and_kind_parsed(&mut split) {
            (
                parse_modifiers(modifier_mask),
                parse_key_event_kind(kind_code),
            )
        } else if buffer.len() > 3 {
            (
                parse_modifiers(
                    (buffer[buffer.len() - 2] as char)
                        .to_digit(10)
                        .ok_or(MalformedSequenceError)? as u8,
                ),
                KeyEventKind::Press,
            )
        } else {
            (Modifiers::NONE, KeyEventKind::Press)
        };
    let key = buffer[buffer.len() - 1];

    let code = match key {
        b'A' => KeyCode::Up,
        b'B' => KeyCode::Down,
        b'C' => KeyCode::Right,
        b'D' => KeyCode::Left,
        b'F' => KeyCode::End,
        b'H' => KeyCode::Home,
        b'P' => KeyCode::Function(1),
        b'Q' => KeyCode::Function(2),
        b'R' => KeyCode::Function(3),
        b'S' => KeyCode::Function(4),
        _ => bail!(),
    };

    let event = Event::Key(KeyEvent {
        code,
        modifiers,
        kind,
        state: KeyEventState::NONE,
    });

    Ok(Some(event))
}

fn parse_csi_special_key_code(buffer: &[u8]) -> Result<Option<Event>> {
    assert!(buffer.starts_with(b"\x1B[")); // CSI
    assert!(buffer.ends_with(b"~"));

    let s = str::from_utf8(&buffer[2..buffer.len() - 1])?;
    let mut split = s.split(';');

    // This CSI sequence can be a list of semicolon-separated numbers.
    let first = next_parsed::<u8>(&mut split)?;

    let (modifiers, kind, state) =
        if let Ok((modifier_mask, kind_code)) = modifier_and_kind_parsed(&mut split) {
            (
                parse_modifiers(modifier_mask),
                parse_key_event_kind(kind_code),
                parse_modifiers_to_state(modifier_mask),
            )
        } else {
            (Modifiers::NONE, KeyEventKind::Press, KeyEventState::NONE)
        };

    let code = match first {
        1 | 7 => KeyCode::Home,
        2 => KeyCode::Insert,
        3 => KeyCode::Delete,
        4 | 8 => KeyCode::End,
        5 => KeyCode::PageUp,
        6 => KeyCode::PageDown,
        v @ 11..=15 => KeyCode::Function(v - 10),
        v @ 17..=21 => KeyCode::Function(v - 11),
        v @ 23..=26 => KeyCode::Function(v - 12),
        v @ 28..=29 => KeyCode::Function(v - 15),
        v @ 31..=34 => KeyCode::Function(v - 17),
        _ => bail!(),
    };

    let event = Event::Key(KeyEvent {
        code,
        modifiers,
        kind,
        state,
    });

    Ok(Some(event))
}

fn translate_functional_key_code(codepoint: u32) -> Option<(KeyCode, KeyEventState)> {
    if let Some(keycode) = match codepoint {
        57399 => Some(KeyCode::Char('0')),
        57400 => Some(KeyCode::Char('1')),
        57401 => Some(KeyCode::Char('2')),
        57402 => Some(KeyCode::Char('3')),
        57403 => Some(KeyCode::Char('4')),
        57404 => Some(KeyCode::Char('5')),
        57405 => Some(KeyCode::Char('6')),
        57406 => Some(KeyCode::Char('7')),
        57407 => Some(KeyCode::Char('8')),
        57408 => Some(KeyCode::Char('9')),
        57409 => Some(KeyCode::Char('.')),
        57410 => Some(KeyCode::Char('/')),
        57411 => Some(KeyCode::Char('*')),
        57412 => Some(KeyCode::Char('-')),
        57413 => Some(KeyCode::Char('+')),
        57414 => Some(KeyCode::Enter),
        57415 => Some(KeyCode::Char('=')),
        57416 => Some(KeyCode::Char(',')),
        57417 => Some(KeyCode::Left),
        57418 => Some(KeyCode::Right),
        57419 => Some(KeyCode::Up),
        57420 => Some(KeyCode::Down),
        57421 => Some(KeyCode::PageUp),
        57422 => Some(KeyCode::PageDown),
        57423 => Some(KeyCode::Home),
        57424 => Some(KeyCode::End),
        57425 => Some(KeyCode::Insert),
        57426 => Some(KeyCode::Delete),
        57427 => Some(KeyCode::KeypadBegin),
        _ => None,
    } {
        return Some((keycode, KeyEventState::KEYPAD));
    }

    if let Some(keycode) = match codepoint {
        57358 => Some(KeyCode::CapsLock),
        57359 => Some(KeyCode::ScrollLock),
        57360 => Some(KeyCode::NumLock),
        57361 => Some(KeyCode::PrintScreen),
        57362 => Some(KeyCode::Pause),
        57363 => Some(KeyCode::Menu),
        57376 => Some(KeyCode::Function(13)),
        57377 => Some(KeyCode::Function(14)),
        57378 => Some(KeyCode::Function(15)),
        57379 => Some(KeyCode::Function(16)),
        57380 => Some(KeyCode::Function(17)),
        57381 => Some(KeyCode::Function(18)),
        57382 => Some(KeyCode::Function(19)),
        57383 => Some(KeyCode::Function(20)),
        57384 => Some(KeyCode::Function(21)),
        57385 => Some(KeyCode::Function(22)),
        57386 => Some(KeyCode::Function(23)),
        57387 => Some(KeyCode::Function(24)),
        57388 => Some(KeyCode::Function(25)),
        57389 => Some(KeyCode::Function(26)),
        57390 => Some(KeyCode::Function(27)),
        57391 => Some(KeyCode::Function(28)),
        57392 => Some(KeyCode::Function(29)),
        57393 => Some(KeyCode::Function(30)),
        57394 => Some(KeyCode::Function(31)),
        57395 => Some(KeyCode::Function(32)),
        57396 => Some(KeyCode::Function(33)),
        57397 => Some(KeyCode::Function(34)),
        57398 => Some(KeyCode::Function(35)),
        57428 => Some(KeyCode::Media(MediaKeyCode::Play)),
        57429 => Some(KeyCode::Media(MediaKeyCode::Pause)),
        57430 => Some(KeyCode::Media(MediaKeyCode::PlayPause)),
        57431 => Some(KeyCode::Media(MediaKeyCode::Reverse)),
        57432 => Some(KeyCode::Media(MediaKeyCode::Stop)),
        57433 => Some(KeyCode::Media(MediaKeyCode::FastForward)),
        57434 => Some(KeyCode::Media(MediaKeyCode::Rewind)),
        57435 => Some(KeyCode::Media(MediaKeyCode::TrackNext)),
        57436 => Some(KeyCode::Media(MediaKeyCode::TrackPrevious)),
        57437 => Some(KeyCode::Media(MediaKeyCode::Record)),
        57438 => Some(KeyCode::Media(MediaKeyCode::LowerVolume)),
        57439 => Some(KeyCode::Media(MediaKeyCode::RaiseVolume)),
        57440 => Some(KeyCode::Media(MediaKeyCode::MuteVolume)),
        57441 => Some(KeyCode::Modifier(ModifierKeyCode::LeftShift)),
        57442 => Some(KeyCode::Modifier(ModifierKeyCode::LeftControl)),
        57443 => Some(KeyCode::Modifier(ModifierKeyCode::LeftAlt)),
        57444 => Some(KeyCode::Modifier(ModifierKeyCode::LeftSuper)),
        57445 => Some(KeyCode::Modifier(ModifierKeyCode::LeftHyper)),
        57446 => Some(KeyCode::Modifier(ModifierKeyCode::LeftMeta)),
        57447 => Some(KeyCode::Modifier(ModifierKeyCode::RightShift)),
        57448 => Some(KeyCode::Modifier(ModifierKeyCode::RightControl)),
        57449 => Some(KeyCode::Modifier(ModifierKeyCode::RightAlt)),
        57450 => Some(KeyCode::Modifier(ModifierKeyCode::RightSuper)),
        57451 => Some(KeyCode::Modifier(ModifierKeyCode::RightHyper)),
        57452 => Some(KeyCode::Modifier(ModifierKeyCode::RightMeta)),
        57453 => Some(KeyCode::Modifier(ModifierKeyCode::IsoLevel3Shift)),
        57454 => Some(KeyCode::Modifier(ModifierKeyCode::IsoLevel5Shift)),
        _ => None,
    } {
        return Some((keycode, KeyEventState::empty()));
    }

    None
}

fn parse_csi_rxvt_mouse(buffer: &[u8]) -> Result<Option<Event>> {
    // rxvt mouse encoding:
    // CSI Cb ; Cx ; Cy ; M

    assert!(buffer.starts_with(b"\x1B[")); // CSI
    assert!(buffer.ends_with(b"M"));

    let s = str::from_utf8(&buffer[2..buffer.len() - 1])?;
    let mut split = s.split(';');

    let cb = next_parsed::<u8>(&mut split)?
        .checked_sub(32)
        .ok_or(MalformedSequenceError)?;
    let (kind, modifiers) = parse_cb(cb)?;

    let cx = next_parsed::<u16>(&mut split)? - 1;
    let cy = next_parsed::<u16>(&mut split)? - 1;

    Ok(Some(Event::Mouse(MouseEvent {
        kind,
        column: cx,
        row: cy,
        modifiers,
    })))
}

fn parse_csi_normal_mouse(buffer: &[u8]) -> Result<Option<Event>> {
    // Normal mouse encoding: CSI M CB Cx Cy (6 characters only).

    assert!(buffer.starts_with(b"\x1B[M")); // CSI M

    if buffer.len() < 6 {
        return Ok(None);
    }

    let cb = buffer[3].checked_sub(32).ok_or(MalformedSequenceError)?;
    let (kind, modifiers) = parse_cb(cb)?;

    // See http://www.xfree86.org/current/ctlseqs.html#Mouse%20Tracking
    // Mouse positions are encoded as (value + 32), but the upper left
    // character position on the terminal is denoted as 1,1.
    // So, we need to subtract 32 + 1 (33) to keep it synced with the cursor.
    let cx = u16::from(buffer[4].saturating_sub(33));
    let cy = u16::from(buffer[5].saturating_sub(33));

    Ok(Some(Event::Mouse(MouseEvent {
        kind,
        column: cx,
        row: cy,
        modifiers,
    })))
}

fn parse_csi_sgr_mouse(buffer: &[u8]) -> Result<Option<Event>> {
    // CSI < Cb ; Cx ; Cy (;) (M or m)

    assert!(buffer.starts_with(b"\x1B[<")); // CSI <

    if !buffer.ends_with(b"m") && !buffer.ends_with(b"M") {
        return Ok(None);
    }

    let s = str::from_utf8(&buffer[3..buffer.len() - 1])?;
    let mut split = s.split(';');

    let cb = next_parsed::<u8>(&mut split)?;
    let (kind, modifiers) = parse_cb(cb)?;

    // See http://www.xfree86.org/current/ctlseqs.html#Mouse%20Tracking
    // The upper left character position on the terminal is denoted as 1,1.
    // Subtract 1 to keep it synced with cursor
    let cx = next_parsed::<u16>(&mut split)? - 1;
    let cy = next_parsed::<u16>(&mut split)? - 1;

    // When button 3 in Cb is used to represent mouse release, you can't tell which button was
    // released. SGR mode solves this by having the sequence end with a lowercase m if it's a
    // button release and an uppercase M if it's a button press.
    //
    // We've already checked that the last character is a lowercase or uppercase M at the start of
    // this function, so we just need one if.
    let kind = if buffer.last() == Some(&b'm') {
        match kind {
            MouseEventKind::Down(button) => MouseEventKind::Up(button),
            other => other,
        }
    } else {
        kind
    };

    Ok(Some(Event::Mouse(MouseEvent {
        kind,
        column: cx,
        row: cy,
        modifiers,
    })))
}

/// Cb is the byte of a mouse input that contains the button being used, the key modifiers being
/// held and whether the mouse is dragging or not.
///
/// Bit layout of cb, from low to high:
///
/// - button number
/// - button number
/// - shift
/// - meta (alt)
/// - control
/// - mouse is dragging
/// - button number
/// - button number
fn parse_cb(cb: u8) -> Result<(MouseEventKind, Modifiers)> {
    let button_number = (cb & 0b0000_0011) | ((cb & 0b1100_0000) >> 4);
    let dragging = cb & 0b0010_0000 == 0b0010_0000;

    let kind = match (button_number, dragging) {
        (0, false) => MouseEventKind::Down(MouseButton::Left),
        (1, false) => MouseEventKind::Down(MouseButton::Middle),
        (2, false) => MouseEventKind::Down(MouseButton::Right),
        (0, true) => MouseEventKind::Drag(MouseButton::Left),
        (1, true) => MouseEventKind::Drag(MouseButton::Middle),
        (2, true) => MouseEventKind::Drag(MouseButton::Right),
        (3, false) => MouseEventKind::Up(MouseButton::Left),
        (3, true) | (4, true) | (5, true) => MouseEventKind::Moved,
        (4, false) => MouseEventKind::ScrollUp,
        (5, false) => MouseEventKind::ScrollDown,
        (6, false) => MouseEventKind::ScrollLeft,
        (7, false) => MouseEventKind::ScrollRight,
        // We do not support other buttons.
        _ => bail!(),
    };

    let mut modifiers = Modifiers::empty();

    if cb & 0b0000_0100 == 0b0000_0100 {
        modifiers |= Modifiers::SHIFT;
    }
    if cb & 0b0000_1000 == 0b0000_1000 {
        modifiers |= Modifiers::ALT;
    }
    if cb & 0b0001_0000 == 0b0001_0000 {
        modifiers |= Modifiers::CONTROL;
    }

    Ok((kind, modifiers))
}

fn parse_csi_bracketed_paste(buffer: &[u8]) -> Result<Option<Event>> {
    // CSI 2 0 0 ~ pasted text CSI 2 0 1 ~
    let buffer = buffer
        .strip_prefix(b"\x1b[200~")
        .expect("asserted by calling functions");

    if let Some(contents) = buffer.strip_suffix(b"\x1b[201~") {
        let paste = String::from_utf8_lossy(contents).to_string();
        Ok(Some(Event::Paste(paste)))
    } else {
        Ok(None)
    }
}

fn parse_csi_cursor_position(buffer: &[u8]) -> Result<Option<Event>> {
    // CSI Cy ; Cx R
    //   Cy - cursor row number (starting from 1)
    //   Cx - cursor column number (starting from 1)
    assert!(buffer.starts_with(b"\x1B[")); // CSI
    assert!(buffer.ends_with(b"R"));

    let s = str::from_utf8(&buffer[2..buffer.len() - 1])?;

    let mut split = s.split(';');

    let line = next_parsed::<NonZeroU16>(&mut split)?.into();
    let col = next_parsed::<NonZeroU16>(&mut split)?.into();

    Ok(Some(Event::Csi(Csi::Cursor(
        csi::Cursor::ActivePositionReport { line, col },
    ))))
}

fn parse_csi_cursor_shape_query_response(buffer: &[u8]) -> Result<Option<Event>> {
    assert!(buffer.starts_with(b"\x1B[>")); // CSI >
    assert!(buffer.ends_with(b" q"));

    if buffer.len() < 5 {
        // Minimum: ESC [ > SP q
        return Ok(None);
    }

    let s = str::from_utf8(&buffer[3..buffer.len() - 2])?;

    // An empty parameter string (CSI > SP q) is a query.
    if s.is_empty() {
        return Ok(Some(Event::Csi(Csi::Cursor(csi::Cursor::QueryCursorShape))));
    }

    let caps: Vec<csi::MultiCursorCapability> = s
        .split(';')
        .filter(|part| !part.is_empty())
        .map(|part| {
            part.parse::<u8>()
                .map_err(|_| MalformedSequenceError)
                .and_then(|v| {
                    csi::MultiCursorCapability::try_from(v).map_err(|_| MalformedSequenceError)
                })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(Some(Event::Csi(Csi::Cursor(
        csi::Cursor::CursorShapeQueryResponse(caps),
    ))))
}

fn parse_csi_keyboard_enhancement_flags(buffer: &[u8]) -> Result<Option<Event>> {
    // CSI ? flags u
    assert!(buffer.starts_with(b"\x1B[?")); // ESC [ ?
    assert!(buffer.ends_with(b"u"));

    if buffer.len() < 5 {
        return Ok(None);
    }

    let bits = buffer[3];
    let mut flags = KittyKeyboardFlags::empty();

    if bits & 1 != 0 {
        flags |= KittyKeyboardFlags::DISAMBIGUATE_ESCAPE_CODES;
    }
    if bits & 2 != 0 {
        flags |= KittyKeyboardFlags::REPORT_EVENT_TYPES;
    }
    if bits & 4 != 0 {
        flags |= KittyKeyboardFlags::REPORT_ALTERNATE_KEYS;
    }
    if bits & 8 != 0 {
        flags |= KittyKeyboardFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES;
    }
    // TODO: support this
    // if bits & 16 != 0 {
    //     flags |= KeyboardEnhancementFlags::REPORT_ASSOCIATED_TEXT;
    // }

    Ok(Some(Event::Csi(Csi::Keyboard(csi::Keyboard::ReportFlags(
        flags,
    )))))
}

fn parse_csi_primary_device_attributes(buffer: &[u8]) -> Result<Option<Event>> {
    // CSI 64 ; attr1 ; attr2 ; ... ; attrn ; c
    assert!(buffer.starts_with(b"\x1B[?"));
    assert!(buffer.ends_with(b"c"));

    // This is a stub for parsing the primary device attributes. This response is not
    // exposed in the crossterm API so we don't need to parse the individual attributes yet.
    // See <https://vt100.net/docs/vt510-rm/DA1.html>

    Ok(Some(Event::Csi(Csi::Device(
        csi::Device::DeviceAttributes(()),
    ))))
}

fn parse_csi_theme_mode(buffer: &[u8]) -> Result<Option<Event>> {
    // dark mode:  CSI ? 997 ; 1 n
    // light mode: CSI ? 997 ; 2 n
    assert!(buffer.starts_with(b"\x1B[?"));
    assert!(buffer.ends_with(b"n"));

    let s = str::from_utf8(&buffer[3..buffer.len() - 1])?;

    let mut split = s.split(';');

    if next_parsed::<u16>(&mut split)? != 997 {
        bail!();
    }

    let theme_mode = match next_parsed::<u8>(&mut split)? {
        1 => ThemeMode::Dark,
        2 => ThemeMode::Light,
        _ => bail!(),
    };

    Ok(Some(Event::Csi(Csi::Mode(csi::Mode::ReportTheme(
        theme_mode,
    )))))
}

fn parse_csi_mode(buffer: &[u8]) -> Result<Option<Event>> {
    // sync output mode:       CSI ? 2026 ; 0 $ y
    // grapheme clustering:    CSI ? 2027 ; 1 $ y
    assert!(buffer.starts_with(b"\x1B[?"));
    assert!(buffer.ends_with(b"y"));

    let s = str::from_utf8(&buffer[3..buffer.len() - 1])?;
    let s = match s.strip_suffix('$') {
        Some(s) => s,
        None => bail!(),
    };

    let mut split = s.split(';');

    let mode = match next_parsed::<u16>(&mut split)? {
        2026 => csi::DecPrivateMode::Code(csi::DecPrivateModeCode::SynchronizedOutput),
        2027 => csi::DecPrivateMode::Code(csi::DecPrivateModeCode::GraphemeClustering),
        _ => bail!(),
    };

    let setting = match next_parsed::<u8>(&mut split)? {
        // For synchronized output specifically, 3 is undefined and 0 and 4 are treated as "not
        // supported."
        0 | 4 if mode == csi::DecPrivateMode::Code(csi::DecPrivateModeCode::SynchronizedOutput) => {
            csi::DecModeSetting::NotRecognized
        }
        0 => csi::DecModeSetting::NotRecognized,
        1 => csi::DecModeSetting::Set,
        2 => csi::DecModeSetting::Reset,
        3 if mode == csi::DecPrivateMode::Code(csi::DecPrivateModeCode::GraphemeClustering) => {
            csi::DecModeSetting::PermanentlySet
        }
        4 => csi::DecModeSetting::PermanentlyReset,
        _ => bail!(),
    };

    Ok(Some(Event::Csi(Csi::Mode(
        csi::Mode::ReportDecPrivateMode { mode, setting },
    ))))
}

fn parse_dcs(buffer: &[u8]) -> Result<Option<Event>> {
    assert!(buffer.starts_with(escape::DCS.as_bytes()));
    if !buffer.ends_with(escape::ST.as_bytes()) {
        return Ok(None);
    }
    match buffer[buffer.len() - 3] {
        // SGR response: DCS Ps $ r SGR m ST
        b'm' => {
            if buffer.get(3..5) != Some(b"$r") {
                bail!();
            }
            // NOTE: <https://www.xfree86.org/current/ctlseqs.html> says that '1' is a valid
            // request and '0' is invalid while the vt100.net docs for DECRQSS say the opposite.
            // Kitty and WezTerm both follow the ctlseqs doc.
            let is_request_valid = match buffer[2] {
                b'1' => true,
                // TODO: don't parse attributes if the request isn't valid?
                b'0' => false,
                _ => bail!(),
            };
            let s = str::from_utf8(&buffer[5..buffer.len() - 3])?;
            let mut sgrs = Vec::new();
            // TODO: is this correct? What about terminals that use ';' for true colors?
            for sgr in s.split(';') {
                sgrs.push(parse_sgr(sgr)?);
            }
            Ok(Some(Event::Dcs(dcs::Dcs::Response {
                is_request_valid,
                value: dcs::DcsResponse::GraphicRendition(sgrs),
            })))
        }
        _ => bail!(),
    }
}

fn parse_sgr(buffer: &str) -> Result<csi::Sgr> {
    use csi::Sgr;
    use style::*;

    let sgr = match buffer {
        "0" => Sgr::Reset,
        "22" => Sgr::Intensity(Intensity::Normal),
        "1" => Sgr::Intensity(Intensity::Bold),
        "2" => Sgr::Intensity(Intensity::Dim),
        "24" => Sgr::Underline(Underline::None),
        "4" => Sgr::Underline(Underline::Single),
        "21" => Sgr::Underline(Underline::Double),
        "4:3 " => Sgr::Underline(Underline::Curly),
        "4:4" => Sgr::Underline(Underline::Dotted),
        "4:5" => Sgr::Underline(Underline::Dashed),
        "25" => Sgr::Blink(Blink::None),
        "5" => Sgr::Blink(Blink::Slow),
        "6" => Sgr::Blink(Blink::Rapid),
        "3" => Sgr::Italic(true),
        "23" => Sgr::Italic(false),
        "7" => Sgr::Reverse(true),
        "27" => Sgr::Reverse(false),
        "8" => Sgr::Invisible(true),
        "28" => Sgr::Invisible(false),
        "9" => Sgr::StrikeThrough(true),
        "29" => Sgr::StrikeThrough(false),
        "53" => Sgr::Overline(true),
        "55" => Sgr::Overline(false),
        "10" => Sgr::Font(Font::Default),
        "11" => Sgr::Font(Font::Alternate(1)),
        "12" => Sgr::Font(Font::Alternate(2)),
        "13" => Sgr::Font(Font::Alternate(3)),
        "14" => Sgr::Font(Font::Alternate(4)),
        "15" => Sgr::Font(Font::Alternate(5)),
        "16" => Sgr::Font(Font::Alternate(6)),
        "17" => Sgr::Font(Font::Alternate(7)),
        "18" => Sgr::Font(Font::Alternate(8)),
        "19" => Sgr::Font(Font::Alternate(9)),
        "75" => Sgr::VerticalAlign(VerticalAlign::BaseLine),
        "73" => Sgr::VerticalAlign(VerticalAlign::SuperScript),
        "74" => Sgr::VerticalAlign(VerticalAlign::SubScript),
        "39" => Sgr::Foreground(ColorSpec::Reset),
        "30" => Sgr::Foreground(ColorSpec::BLACK),
        "31" => Sgr::Foreground(ColorSpec::RED),
        "32" => Sgr::Foreground(ColorSpec::GREEN),
        "33" => Sgr::Foreground(ColorSpec::YELLOW),
        "34" => Sgr::Foreground(ColorSpec::BLUE),
        "35" => Sgr::Foreground(ColorSpec::MAGENTA),
        "36" => Sgr::Foreground(ColorSpec::CYAN),
        "37" => Sgr::Foreground(ColorSpec::WHITE),
        "90" => Sgr::Foreground(ColorSpec::BRIGHT_BLACK),
        "91" => Sgr::Foreground(ColorSpec::BRIGHT_RED),
        "92" => Sgr::Foreground(ColorSpec::BRIGHT_GREEN),
        "93" => Sgr::Foreground(ColorSpec::BRIGHT_YELLOW),
        "94" => Sgr::Foreground(ColorSpec::BRIGHT_BLUE),
        "95" => Sgr::Foreground(ColorSpec::BRIGHT_MAGENTA),
        "96" => Sgr::Foreground(ColorSpec::BRIGHT_CYAN),
        "97" => Sgr::Foreground(ColorSpec::BRIGHT_WHITE),
        "49" => Sgr::Background(ColorSpec::Reset),
        "40" => Sgr::Background(ColorSpec::BLACK),
        "41" => Sgr::Background(ColorSpec::RED),
        "42" => Sgr::Background(ColorSpec::GREEN),
        "43" => Sgr::Background(ColorSpec::YELLOW),
        "44" => Sgr::Background(ColorSpec::BLUE),
        "45" => Sgr::Background(ColorSpec::MAGENTA),
        "46" => Sgr::Background(ColorSpec::CYAN),
        "47" => Sgr::Background(ColorSpec::WHITE),
        "100" => Sgr::Background(ColorSpec::BRIGHT_BLACK),
        "101" => Sgr::Background(ColorSpec::BRIGHT_RED),
        "102" => Sgr::Background(ColorSpec::BRIGHT_GREEN),
        "103" => Sgr::Background(ColorSpec::BRIGHT_YELLOW),
        "104" => Sgr::Background(ColorSpec::BRIGHT_BLUE),
        "105" => Sgr::Background(ColorSpec::BRIGHT_MAGENTA),
        "106" => Sgr::Background(ColorSpec::BRIGHT_CYAN),
        "107" => Sgr::Background(ColorSpec::BRIGHT_WHITE),
        "59" => Sgr::UnderlineColor(ColorSpec::Reset),
        _ => {
            let mut split = buffer.split(':').filter(|s| !s.is_empty());
            let first = next_parsed::<u8>(&mut split)?;
            let color = match next_parsed::<u8>(&mut split)? {
                2 => RgbColor {
                    red: next_parsed::<u8>(&mut split)?,
                    green: next_parsed::<u8>(&mut split)?,
                    blue: next_parsed::<u8>(&mut split)?,
                }
                .into(),
                5 => ColorSpec::PaletteIndex(next_parsed::<u8>(&mut split)?),
                6 => RgbaColor {
                    red: next_parsed::<u8>(&mut split)?,
                    green: next_parsed::<u8>(&mut split)?,
                    blue: next_parsed::<u8>(&mut split)?,
                    alpha: next_parsed::<u8>(&mut split)?,
                }
                .into(),
                _ => bail!(),
            };
            match first {
                38 => Sgr::Foreground(color),
                48 => Sgr::Background(color),
                58 => Sgr::UnderlineColor(color),
                _ => bail!(),
            }
        }
    };
    Ok(sgr)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn parse_dcs_sgr_response() {
        // Example from <https://vt100.net/docs/vt510-rm/DECRPSS.html>
        // > If the current graphic rendition is underline, blinking, and reverse, then the
        // > terminal responds with the following DECRPSS sequence:
        // > DCS 0 $ r 0 ; 4 ; 5 ; 7 m ST
        // NOTE: The vt100.net docs have the Ps part of this reversed. 0 is invalid and 1 is
        // valid according to the xterm docs. See `parse_dcs`.
        let event = parse_event(b"\x1bP0$r0;4;5;7m\x1b\\", false)
            .unwrap()
            .unwrap();
        assert_eq!(
            event,
            Event::Dcs(dcs::Dcs::Response {
                is_request_valid: false,
                value: dcs::DcsResponse::GraphicRendition(vec![
                    csi::Sgr::Reset,
                    csi::Sgr::Underline(style::Underline::Single),
                    csi::Sgr::Blink(style::Blink::Slow),
                    csi::Sgr::Reverse(true),
                ])
            })
        );
    }

    #[test]
    fn parse_osc_dynamic_color_response() {
        assert_eq!(
            parse_event(b"\x1b]11;rgb:2828/2828/2828\x1b\\", false)
                .unwrap()
                .unwrap(),
            Event::Osc(osc::Osc::ChangeDynamicColors(
                osc::DynamicColorNumber::TextBackgroundColor,
                vec![style::RgbColor::new(40, 40, 40).into()]
            ))
        );
        // BEL ending instead of ST
        assert_eq!(
            parse_event(b"\x1b]11;rgb:2828/2828/2828\x07", false)
                .unwrap()
                .unwrap(),
            Event::Osc(osc::Osc::ChangeDynamicColors(
                osc::DynamicColorNumber::TextBackgroundColor,
                vec![style::RgbColor::new(40, 40, 40).into()]
            ))
        );
    }

    #[test]
    fn parse_cursor_shape_query() {
        // CSI > SP q with no parameters is a query.
        let event = parse_event(b"\x1b[> q", false).unwrap().unwrap();
        assert_eq!(
            event,
            Event::Csi(Csi::Cursor(csi::Cursor::QueryCursorShape))
        );
    }

    #[test]
    fn parse_cursor_shape_query_response() {
        // Kitty responds with the supported operation codes.
        let event = parse_event(b"\x1b[>1;2;29;100 q", false).unwrap().unwrap();
        assert_eq!(
            event,
            Event::Csi(Csi::Cursor(csi::Cursor::CursorShapeQueryResponse(vec![
                csi::MultiCursorCapability::BlockShape,
                csi::MultiCursorCapability::BeamShape,
                csi::MultiCursorCapability::FollowMainCursorShape,
                csi::MultiCursorCapability::QueryCurrentCursors,
            ])))
        );
    }

    #[test]
    fn parse_cursor_shape_query_response_invalid() {
        // Value 7 is not a valid MultiCursorCapability code.
        assert!(parse_event(b"\x1b[>7 q", false).is_err());
    }

    #[test]
    fn cursor_shape_query_response_round_trip() {
        let response = csi::Cursor::CursorShapeQueryResponse(vec![
            csi::MultiCursorCapability::BlockShape,
            csi::MultiCursorCapability::FollowMainCursorShape,
            csi::MultiCursorCapability::QueryCurrentCursors,
        ]);
        let encoded = Csi::Cursor(response.clone()).to_string();
        assert_eq!(encoded, "\x1b[>1;29;100 q");

        let parsed = parse_event(encoded.as_bytes(), false).unwrap().unwrap();
        assert_eq!(parsed, Event::Csi(Csi::Cursor(response)));
    }

    #[test]
    fn parse_synchronized_output_mode_set() {
        let event = parse_event(b"\x1b[?2026;1$y", false).unwrap().unwrap();
        assert_eq!(
            event,
            Event::Csi(Csi::Mode(csi::Mode::ReportDecPrivateMode {
                mode: csi::DecPrivateMode::Code(csi::DecPrivateModeCode::SynchronizedOutput),
                setting: csi::DecModeSetting::Set,
            }))
        );
    }

    #[test]
    fn parse_grapheme_clustering_mode_set() {
        let event = parse_event(b"\x1b[?2027;1$y", false).unwrap().unwrap();
        assert_eq!(
            event,
            Event::Csi(Csi::Mode(csi::Mode::ReportDecPrivateMode {
                mode: csi::DecPrivateMode::Code(csi::DecPrivateModeCode::GraphemeClustering),
                setting: csi::DecModeSetting::Set,
            }))
        );
    }

    #[test]
    fn parse_bracketed_paste() {
        // Incomplete input is not considered a paste.
        let event = parse_event(b"\x1b[200~", false).unwrap();
        assert_eq!(event, None);
        let event = parse_event(b"\x1b[200~Hello, world!\x1b[201~", false).unwrap();
        assert_eq!(event, Some(Event::Paste("Hello, world!".to_string())));
        let event = parse_event(b"\x1b[200~\x1b[201~", false).unwrap();
        assert_eq!(event, Some(Event::Paste("".to_string())));
    }
}
