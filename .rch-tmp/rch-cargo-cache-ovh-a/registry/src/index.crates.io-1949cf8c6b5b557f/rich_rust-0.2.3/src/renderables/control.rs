//! Control renderable (Python Rich `rich.control` parity).
//!
//! This renderable emits non-printable terminal control codes such as cursor
//! movements, clear-screen, alt-screen toggles, and window title updates.

use smallvec::smallvec;
use std::borrow::Cow;

use crate::console::{Console, ConsoleOptions};
use crate::renderables::Renderable;
use crate::segment::{ControlCode, ControlType, Segment};

/// A renderable that inserts one or more terminal control codes.
#[derive(Debug, Clone)]
pub struct Control {
    codes: Vec<ControlCode>,
    title: Option<String>,
}

impl Control {
    /// Construct a Control renderable from explicit control codes.
    #[must_use]
    pub fn new(codes: Vec<ControlCode>) -> Self {
        Self { codes, title: None }
    }

    #[must_use]
    pub fn bell() -> Self {
        Self::new(vec![ControlCode::new(ControlType::Bell)])
    }

    #[must_use]
    pub fn home() -> Self {
        Self::new(vec![ControlCode::new(ControlType::Home)])
    }

    /// Move cursor relative to current position.
    #[must_use]
    pub fn r#move(x: i32, y: i32) -> Self {
        let mut codes: Vec<ControlCode> = Vec::new();
        if x != 0 {
            let control_type = if x > 0 {
                ControlType::CursorForward
            } else {
                ControlType::CursorBackward
            };
            codes.push(ControlCode::with_params(
                control_type,
                smallvec![x.saturating_abs()],
            ));
        }
        if y != 0 {
            let control_type = if y > 0 {
                ControlType::CursorDown
            } else {
                ControlType::CursorUp
            };
            codes.push(ControlCode::with_params(
                control_type,
                smallvec![y.saturating_abs()],
            ));
        }
        Self::new(codes)
    }

    /// Move cursor relative to current position.
    #[must_use]
    pub fn move_cursor(x: i32, y: i32) -> Self {
        Self::r#move(x, y)
    }

    /// Move to the given 0-based column, optionally add a relative row offset.
    #[must_use]
    pub fn move_to_column(x: i32, y: i32) -> Self {
        let mut codes: Vec<ControlCode> = vec![ControlCode::with_params(
            ControlType::CursorMoveToColumn,
            smallvec![x],
        )];
        if y != 0 {
            let control_type = if y > 0 {
                ControlType::CursorDown
            } else {
                ControlType::CursorUp
            };
            codes.push(ControlCode::with_params(
                control_type,
                smallvec![y.saturating_abs()],
            ));
        }
        Self::new(codes)
    }

    /// Move cursor to an absolute 0-based position (x, y).
    #[must_use]
    pub fn move_to(x: i32, y: i32) -> Self {
        Self::new(vec![ControlCode::with_params(
            ControlType::CursorMoveTo,
            smallvec![x, y],
        )])
    }

    #[must_use]
    pub fn clear() -> Self {
        Self::new(vec![ControlCode::new(ControlType::Clear)])
    }

    #[must_use]
    pub fn show_cursor(show: bool) -> Self {
        Self::new(vec![ControlCode::new(if show {
            ControlType::ShowCursor
        } else {
            ControlType::HideCursor
        })])
    }

    #[must_use]
    pub fn alt_screen(enable: bool) -> Self {
        if enable {
            Self::new(vec![
                ControlCode::new(ControlType::EnableAltScreen),
                ControlCode::new(ControlType::Home),
            ])
        } else {
            Self::new(vec![ControlCode::new(ControlType::DisableAltScreen)])
        }
    }

    #[must_use]
    pub fn title(title: impl Into<String>) -> Self {
        let mut control = Self::new(vec![ControlCode::new(ControlType::SetWindowTitle)]);
        control.title = Some(title.into());
        control
    }
}

impl Renderable for Control {
    fn render<'a>(&'a self, _console: &Console, _options: &ConsoleOptions) -> Vec<Segment<'a>> {
        let text = match self.title.as_deref() {
            Some(title) => Cow::Borrowed(title),
            None => Cow::Borrowed(""),
        };
        vec![Segment {
            text,
            style: None,
            control: Some(self.codes.clone()),
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::console::Console;

    #[test]
    fn move_alias_matches_move_cursor() {
        let via_keyword = Control::r#move(3, -2);
        let via_alias = Control::move_cursor(3, -2);

        let console = Console::new();
        let options = console.options();
        let kw_segments = via_keyword.render(&console, &options);
        let alias_segments = via_alias.render(&console, &options);
        assert_eq!(kw_segments, alias_segments);
    }

    #[test]
    fn move_to_column_includes_optional_vertical_offset() {
        let control = Control::move_to_column(0, 2);
        let console = Console::new();
        let options = console.options();
        let segments = control.render(&console, &options);
        assert_eq!(segments.len(), 1);

        let codes = segments[0]
            .control
            .as_ref()
            .expect("control codes should be present");
        assert_eq!(codes.len(), 2);
        assert_eq!(codes[0].control_type, ControlType::CursorMoveToColumn);
        assert_eq!(codes[0].params.as_slice(), &[0]);
        assert_eq!(codes[1].control_type, ControlType::CursorDown);
        assert_eq!(codes[1].params.as_slice(), &[2]);
    }

    #[test]
    fn title_renders_title_text_for_window_title_control() {
        let control = Control::title("Example");
        let console = Console::new();
        let options = console.options();
        let segments = control.render(&console, &options);

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text.as_ref(), "Example");
        let codes = segments[0]
            .control
            .as_ref()
            .expect("control codes should be present");
        assert_eq!(codes.len(), 1);
        assert_eq!(codes[0].control_type, ControlType::SetWindowTitle);
    }
}
