//! Markdown deep-dive scene for demo_showcase.
//!
//! Demonstrates rich_rust's Markdown rendering capabilities including:
//! - Headings (H1-H6)
//! - Emphasis (bold, italic, strikethrough)
//! - Bullet and numbered lists
//! - Code blocks (inline and fenced)
//! - Blockquotes
//! - Links
//! - Graceful handling when markdown feature is disabled

use std::sync::Arc;

use rich_rust::console::Console;
#[cfg(not(feature = "markdown"))]
use rich_rust::markup::render_or_plain;
#[cfg(not(feature = "markdown"))]
use rich_rust::renderables::panel::Panel;
#[cfg(not(feature = "markdown"))]
use rich_rust::style::Style;

use crate::Config;
use crate::scenes::{Scene, SceneError};

/// Markdown deep-dive scene.
pub struct MarkdownScene;

impl MarkdownScene {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Scene for MarkdownScene {
    fn name(&self) -> &'static str {
        "markdown"
    }

    fn summary(&self) -> &'static str {
        "Markdown deep-dive: release notes, headings, lists, and code blocks."
    }

    fn run(&self, console: &Arc<Console>, cfg: &Config) -> Result<(), SceneError> {
        console.print("[section.title]Markdown: Documentation Rendering[/]");
        console.print("");

        #[cfg(feature = "markdown")]
        {
            run_markdown_demo(console, cfg);
        }

        #[cfg(not(feature = "markdown"))]
        {
            run_markdown_disabled_notice(console, cfg);
        }

        Ok(())
    }
}

/// Run the markdown demo when the feature is enabled.
#[cfg(feature = "markdown")]
fn run_markdown_demo(console: &Arc<Console>, cfg: &Config) {
    console
        .print("[dim]Markdown renderable converts CommonMark + GFM to styled terminal output.[/]");
    console.print("");

    // Demo 1: Release notes (always inline)
    render_release_notes(console);

    console.print("");

    // Demo 2: Runbook excerpt (use pager when interactive for long content)
    render_runbook(console, cfg);

    console.print("");
    console.print("[hint]Markdown supports headings, lists, code blocks, blockquotes, and inline formatting.[/]");
}

/// Render release notes example.
#[cfg(feature = "markdown")]
fn render_release_notes(console: &Console) {
    use rich_rust::renderables::markdown::Markdown;
    use rich_rust::segment::Segment;

    console.print("[brand.accent]Release Notes[/]");
    console.print("");

    let release_notes = r#"# Nebula API v2.4.1

## What's New

This release brings **performance improvements** and *stability fixes* for production deployments.

### Features

- **Rolling deployments**: Zero-downtime updates with configurable surge
- **Health check improvements**: Faster failure detection with exponential backoff
- **Metrics endpoint**: New `/metrics` endpoint for Prometheus integration

### Breaking Changes

> **Note:** The `--legacy-mode` flag has been removed. See migration guide.

### Bug Fixes

1. Fixed memory leak in connection pool under high load
2. Resolved race condition in deployment rollback
3. Corrected timezone handling in audit logs

### Upgrade Instructions

Update your deployment config:

```toml
[deployment]
version = "2.4.1"
strategy = "rolling"
```

For more details, see the [migration guide](https://docs.nebula.io/migrate).
"#;

    let md = Markdown::new(release_notes);
    let segments = md.render(76);
    let mut output = segments;
    output.push(Segment::plain("\n"));
    console.print_segments(&output);
}

/// Render a runbook excerpt.
///
/// When interactive mode is allowed and the console is a TTY, this uses
/// the Pager for the runbook content (best UX for long documentation).
/// Otherwise, it renders inline.
#[cfg(feature = "markdown")]
fn render_runbook(console: &Arc<Console>, cfg: &Config) {
    use crate::pager::{PagerConfig, page_content};
    use rich_rust::renderables::markdown::Markdown;
    use rich_rust::segment::Segment;

    console.print("[brand.accent]Deployment Runbook (Excerpt)[/]");
    console.print("");

    let runbook = r#"## Pre-Deployment Checklist

Before deploying to production, verify:

- [ ] All tests pass in CI
- [ ] Staging environment validated
- [ ] Database migrations reviewed
- [ ] Rollback plan documented

## Deployment Steps

### 1. Notify the Team

Post in `#deployments`:

> Starting production deployment of **nebula-api v2.4.1**

### 2. Execute Deployment

Run the deployment command:

```bash
kubectl apply -f k8s/production/
kubectl rollout status deployment/nebula-api
```

### 3. Verify Health

Check the health endpoint:

```bash
curl -s https://api.nebula.io/health | jq .
```

Expected response:
```json
{"status": "healthy", "version": "2.4.1"}
```

---

*Last updated: January 2026*
"#;

    let md = Markdown::new(runbook);
    let segments = md.render(76);
    let mut output = segments;
    output.push(Segment::plain("\n"));

    // Use pager for runbook when interactive (best UX for long docs)
    // Falls back gracefully when pager unavailable or non-interactive
    if cfg.is_interactive_allowed() && console.is_terminal() {
        let mut buffer = Vec::new();
        if console.print_segments_to(&mut buffer, &output).is_ok() {
            let rendered = String::from_utf8_lossy(&buffer);
            let pager_cfg = PagerConfig {
                interactive_allowed: cfg.is_interactive_allowed(),
                force_pager: false,
            };
            let _ = page_content(&rendered, console, &pager_cfg);
        }
    } else {
        console.print_segments(&output);
    }
}

/// Show notice when markdown feature is disabled.
#[cfg(not(feature = "markdown"))]
fn run_markdown_disabled_notice(console: &Arc<Console>, cfg: &Config) {
    let content = render_or_plain(
        "[bold]Markdown feature not enabled[/]\n\n\
         The Markdown renderable requires the [cyan]markdown[/] feature.\n\n\
         To enable Markdown rendering, build with:\n\n\
         [cyan]cargo build --features markdown[/]\n\n\
         Or enable all content features:\n\n\
         [cyan]cargo build --features full[/]\n\n\
         Or run the full showcase:\n\n\
         [cyan]cargo run --bin demo_showcase --features showcase[/]",
    );
    let title = render_or_plain("[yellow]Feature Required[/]");
    let notice = Panel::from_rich_text(&content, 56)
        .title(title)
        .border_style(Style::parse("yellow").unwrap_or_default())
        .padding((1, 2))
        .width(60)
        .safe_box(cfg.is_safe_box());

    console.print_renderable(&notice);

    console.print("");
    console.print("[dim]When enabled, Markdown renderable provides:[/]");
    console.print("[dim]  - CommonMark + GitHub Flavored Markdown support[/]");
    console.print("[dim]  - Styled headings (H1-H6)[/]");
    console.print("[dim]  - Lists, blockquotes, and code blocks[/]");
    console.print("[dim]  - Inline emphasis and links[/]");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_scene_has_correct_name() {
        let scene = MarkdownScene::new();
        assert_eq!(scene.name(), "markdown");
    }

    #[test]
    fn markdown_scene_runs_without_error() {
        let scene = MarkdownScene::new();
        let console = Console::builder()
            .force_terminal(false)
            .markup(true)
            .build()
            .shared();
        let cfg = Config::with_defaults();

        let result = scene.run(&console, &cfg);
        assert!(result.is_ok());
    }
}
