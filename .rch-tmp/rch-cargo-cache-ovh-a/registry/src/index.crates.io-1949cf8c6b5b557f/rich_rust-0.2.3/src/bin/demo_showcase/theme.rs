use rich_rust::theme::Theme;

/// Demo-only theme for the `demo_showcase` binary.
///
/// The goal is to provide stable, descriptive style names so scenes can use markup like:
/// - `[brand.title]Nebula Deploy[/]`
/// - `[status.ok]OK[/]`
/// - `[status.err.badge]FAIL[/]`
pub fn demo_theme() -> Theme {
    Theme::from_style_definitions(DEMO_STYLE_DEFINITIONS.iter().copied(), true)
        .expect("demo_showcase theme definitions must parse")
}

/// Named style definitions used by the demo.
///
/// Keep names stable; scene code should reference these keys directly.
pub const DEMO_STYLE_DEFINITIONS: &[(&str, &str)] = &[
    // Brand / headline
    ("brand.title", "bold #a78bfa"),
    ("brand.subtitle", "dim #c4b5fd"),
    ("brand.accent", "bold #38bdf8"),
    ("brand.muted", "dim #94a3b8"),
    // Statuses
    ("status.ok", "bold green"),
    ("status.warn", "bold yellow"),
    ("status.err", "bold red"),
    ("status.info", "bold cyan"),
    ("status.ok.badge", "bold white on green"),
    ("status.warn.badge", "bold black on yellow"),
    ("status.err.badge", "bold white on red"),
    // Sections / structure
    ("section.rule", "dim #38bdf8"),
    ("section.title", "bold #38bdf8"),
    ("section.muted", "dim #94a3b8"),
    // Panels / tables
    ("panel.title", "bold #38bdf8"),
    ("panel.subtitle", "dim #94a3b8"),
    ("table.header", "bold #e2e8f0"),
    ("table.caption", "dim #94a3b8"),
    ("table.border", "dim #64748b"),
    // Log levels (for demo panes; RichLogger has its own styling)
    ("log.trace", "dim #64748b"),
    ("log.debug", "blue dim"),
    ("log.info", "green"),
    ("log.warn", "yellow"),
    ("log.error", "bold red"),
    // Misc helpers
    ("hint", "dim cyan"),
    ("dim", "dim"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_contains_expected_keys() {
        let theme = demo_theme();
        for key in [
            "brand.title",
            "brand.subtitle",
            "status.ok",
            "status.warn",
            "status.err",
            "section.rule",
            "panel.title",
            "table.header",
            "log.info",
        ] {
            assert!(theme.get(key).is_some(), "missing theme key {key:?}");
        }
    }
}
