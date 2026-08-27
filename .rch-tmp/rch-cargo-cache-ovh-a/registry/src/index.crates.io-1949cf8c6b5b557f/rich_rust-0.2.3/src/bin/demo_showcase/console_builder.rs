//! Console builder for demo_showcase.
//!
//! Builds a shared Console configured from CLI args with theme, dimensions,
//! color system, and feature toggles.

// Module will be used once scene runner is implemented
#![allow(dead_code)]

use std::io::IsTerminal;
use std::sync::Arc;

use rich_rust::color::ColorSystem;
use rich_rust::console::Console;
use rich_rust::terminal;

use crate::{ColorMode, Config};

/// Bundle returned by the demo_showcase console builder.
///
/// rich_rust does not currently offer a global "disable OSC8 hyperlinks" toggle,
/// so the demo keeps this as an explicit boolean alongside the shared Console.
pub struct DemoConsole {
    pub console: Arc<Console>,
    pub links_enabled: bool,
}

/// Build the shared Console for all demo_showcase scenes.
///
/// Responsibilities (bd-1gou):
/// - Apply demo Theme
/// - Respect width/height overrides for deterministic output
/// - Respect `--force-terminal` for TTY overrides
/// - Respect emoji/safe_box/color-system toggles
/// - Compute a `links_enabled` toggle for OSC8 emission (scene-level)
#[must_use]
pub fn build_demo_console(cfg: &Config) -> DemoConsole {
    let mut builder = Console::builder().theme(crate::theme::demo_theme());

    if let Some(width) = cfg.width {
        builder = builder.width(width);
    }
    if let Some(height) = cfg.height {
        builder = builder.height(height);
    }

    if cfg.force_terminal {
        builder = builder.force_terminal(true);
    }

    if let Some(emoji) = cfg.emoji {
        builder = builder.emoji(emoji);
    }

    if let Some(safe_box) = cfg.safe_box {
        builder = builder.safe_box(safe_box);
    }

    builder = apply_color_mode(builder, cfg.color_system, cfg.force_terminal);

    let console = builder.build().shared();
    let links_enabled = resolve_links_enabled(cfg);

    DemoConsole {
        console,
        links_enabled,
    }
}

fn apply_color_mode(
    builder: rich_rust::console::ConsoleBuilder,
    mode: ColorMode,
    force_terminal: bool,
) -> rich_rust::console::ConsoleBuilder {
    match mode {
        ColorMode::Auto => {
            // If the user explicitly asked to force terminal mode, apply it here.
            builder.pipe_if(force_terminal, |b| b.force_terminal(true))
        }
        ColorMode::None => {
            // rich_rust represents "no color" as `ColorSystem = None`.
            // We currently can't directly clear the detected color system without also forcing
            // `is_terminal=false`, so we do the conservative thing and treat output as non-TTY.
            //
            // This is acceptable for the demo_showcase safety model (no ANSI in pipes).
            // NOTE: `--color-system none` takes precedence over `--force-terminal`.
            builder.force_terminal(false)
        }
        ColorMode::Standard => builder
            .color_system(ColorSystem::Standard)
            .pipe_if(force_terminal, |b| b.force_terminal(true)),
        ColorMode::EightBit => builder
            .color_system(ColorSystem::EightBit)
            .pipe_if(force_terminal, |b| b.force_terminal(true)),
        ColorMode::TrueColor => builder
            .color_system(ColorSystem::TrueColor)
            .pipe_if(force_terminal, |b| b.force_terminal(true)),
    }
}

trait PipeIf: Sized {
    fn pipe_if(self, cond: bool, f: impl FnOnce(Self) -> Self) -> Self;
}

impl PipeIf for rich_rust::console::ConsoleBuilder {
    fn pipe_if(self, cond: bool, f: impl FnOnce(Self) -> Self) -> Self {
        if cond { f(self) } else { self }
    }
}

fn resolve_links_enabled(cfg: &Config) -> bool {
    if let Some(links) = cfg.links {
        return links;
    }

    // Default "auto": enable hyperlinks only when interactive.
    let is_tty = cfg.force_terminal || std::io::stdout().is_terminal();
    is_tty && !terminal::is_dumb_terminal() && cfg.interactive.unwrap_or(true)
}
