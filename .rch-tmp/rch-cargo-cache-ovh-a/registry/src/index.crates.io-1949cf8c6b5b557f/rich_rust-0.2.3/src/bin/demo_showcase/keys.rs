//! Keyboard input handling for demo_showcase.
//!
//! Provides non-blocking key polling for interactive mode.
//! Only active when running in a TTY; no-ops in non-interactive contexts.

use std::io::IsTerminal;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};

/// Key actions that can be handled during demo execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    /// Quit the demo early.
    Quit,
    /// Pause/resume the demo (toggle).
    Pause,
    /// No action (key not recognized or no key pressed).
    None,
}

/// Poll for a key press with a short timeout.
///
/// Returns `KeyAction::None` if:
/// - No key is pressed within the timeout
/// - Not running in a TTY (non-interactive)
/// - An error occurs during polling
///
/// This function is non-blocking with a very short timeout (10ms)
/// to avoid slowing down the main loop.
pub fn poll_key_action() -> KeyAction {
    // Only poll if we're in a real terminal
    if !std::io::stdin().is_terminal() {
        return KeyAction::None;
    }

    // Very short timeout to avoid blocking
    let timeout = Duration::from_millis(10);

    match event::poll(timeout) {
        Ok(true) => match event::read() {
            Ok(Event::Key(key_event)) => map_key_event(key_event),
            _ => KeyAction::None,
        },
        _ => KeyAction::None,
    }
}

/// Map a key event to an action.
fn map_key_event(event: KeyEvent) -> KeyAction {
    match event.code {
        // 'q' or 'Q' to quit
        KeyCode::Char('q') | KeyCode::Char('Q') => KeyAction::Quit,
        // Ctrl+C to quit
        KeyCode::Char('c') if event.modifiers.contains(KeyModifiers::CONTROL) => KeyAction::Quit,
        // Escape to quit
        KeyCode::Esc => KeyAction::Quit,
        // 'p' or 'P' or Space to pause
        KeyCode::Char('p') | KeyCode::Char('P') | KeyCode::Char(' ') => KeyAction::Pause,
        // Anything else is ignored
        _ => KeyAction::None,
    }
}

/// Check if the user wants to quit (convenience function).
///
/// Polls for input and returns true if a quit key was pressed.
pub fn should_quit() -> bool {
    poll_key_action() == KeyAction::Quit
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_action_variants() {
        assert_ne!(KeyAction::Quit, KeyAction::None);
        assert_ne!(KeyAction::Pause, KeyAction::None);
        assert_ne!(KeyAction::Quit, KeyAction::Pause);
    }

    #[test]
    fn poll_returns_none_in_non_tty() {
        // In test environment (not a TTY), should return None
        // This test validates the safety check works
        let action = poll_key_action();
        assert_eq!(action, KeyAction::None);
    }

    #[test]
    fn map_quit_keys() {
        use crossterm::event::KeyEventKind;

        let q_event = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        assert_eq!(map_key_event(q_event), KeyAction::Quit);

        let esc_event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(map_key_event(esc_event), KeyAction::Quit);

        let ctrl_c = KeyEvent::new_with_kind(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
            KeyEventKind::Press,
        );
        assert_eq!(map_key_event(ctrl_c), KeyAction::Quit);
    }

    #[test]
    fn map_pause_keys() {
        let p_event = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE);
        assert_eq!(map_key_event(p_event), KeyAction::Pause);

        let space_event = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE);
        assert_eq!(map_key_event(space_event), KeyAction::Pause);
    }

    #[test]
    fn map_unknown_keys() {
        let a_event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        assert_eq!(map_key_event(a_event), KeyAction::None);
    }
}
