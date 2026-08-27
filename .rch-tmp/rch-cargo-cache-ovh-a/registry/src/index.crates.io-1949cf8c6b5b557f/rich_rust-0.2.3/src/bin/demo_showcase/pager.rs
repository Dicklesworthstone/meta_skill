//! Pager integration for demo_showcase.
//!
//! Provides graceful pager support for long content, falling back to inline
//! output when pager is unavailable or non-interactive mode is active.

// Module prepared for future scene implementations
#![allow(dead_code)]

use std::io::{IsTerminal, Write};
use std::process::{Command, Stdio};
use std::sync::Arc;

use rich_rust::console::Console;

/// Result of attempting to page content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PagerResult {
    /// Content was successfully paged.
    Paged,
    /// Content was printed inline (pager unavailable or disabled).
    Inline,
}

/// Configuration for pager behavior.
pub struct PagerConfig {
    /// Whether interactive features are allowed (from --no-interactive flag).
    pub interactive_allowed: bool,
    /// Whether to force pager even for short content.
    pub force_pager: bool,
}

/// Attempt to page content, falling back to inline output.
///
/// This function tries to pipe content through `less` with appropriate flags.
/// If `less` is unavailable or fails, content is printed inline via the console.
///
/// # Arguments
/// * `content` - The rendered content to display (already contains ANSI codes)
/// * `console` - Console for fallback inline output
/// * `cfg` - Pager configuration
///
/// # Returns
/// `PagerResult::Paged` if content was shown in pager, `PagerResult::Inline` otherwise.
pub fn page_content(content: &str, console: &Arc<Console>, cfg: &PagerConfig) -> PagerResult {
    // Skip pager if interactive mode is disabled
    if !cfg.interactive_allowed {
        console.print(content);
        return PagerResult::Inline;
    }

    // Skip pager if stdout is not a terminal
    if !std::io::stdout().is_terminal() {
        console.print(content);
        return PagerResult::Inline;
    }

    // Try to spawn less with ANSI support
    // -R: interpret ANSI colors, -X: don't clear screen on exit
    // -F: quit if content fits in one screen (skip if force_pager is true)
    let args = if cfg.force_pager {
        vec!["-R", "-X"]
    } else {
        vec!["-R", "-F", "-X"]
    };
    let pager_result = Command::new("less")
        .args(&args)
        .stdin(Stdio::piped())
        .spawn();

    match pager_result {
        Ok(mut child) => {
            if let Some(mut stdin) = child.stdin.take() {
                // Write content to pager stdin
                if stdin.write_all(content.as_bytes()).is_ok() {
                    drop(stdin); // Close stdin to signal EOF
                    if child.wait().is_ok() {
                        return PagerResult::Paged;
                    }
                }
            }
            // Pager failed, fall back to inline
            console.print(content);
            PagerResult::Inline
        }
        Err(_) => {
            // less not available, fall back to inline
            console.print(content);
            PagerResult::Inline
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pager_result_variants() {
        assert_ne!(PagerResult::Paged, PagerResult::Inline);
    }

    #[test]
    fn non_interactive_falls_back_to_inline() {
        let console = Console::builder().force_terminal(false).build().shared();
        let cfg = PagerConfig {
            interactive_allowed: false,
            force_pager: false,
        };

        let result = page_content("test content", &console, &cfg);
        assert_eq!(result, PagerResult::Inline);
    }
}
