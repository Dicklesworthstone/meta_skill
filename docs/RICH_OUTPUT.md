# Rich Output System

The `ms` CLI features a rich output system that automatically adapts between styled terminal output and plain text based on the environment. This document covers usage, configuration, development, and agent integration.

## User Guide

### Output Modes

| Mode | When | Description |
|------|------|-------------|
| **Rich** | Interactive terminal | Colors, Unicode box-drawing, styled panels |
| **Plain** | Pipes, redirects, `--plain` | Clean ASCII text, no escape sequences |
| **JSON** | `--robot`, `--format json` | Structured JSON for machine consumption |

### Controlling Output Mode

**CLI flags:**
```sh
ms list --plain          # Force plain text
ms list --robot          # Force JSON output
ms list --format json    # JSON output
ms list --format tsv     # Tab-separated values
```

**Environment variables:**
```sh
export NO_COLOR=1          # Disable all colors (respects no-color.org)
export MS_PLAIN_OUTPUT=1   # Force plain mode
export MS_FORCE_RICH=1     # Force rich mode (overrides detection)
export MS_NO_UNICODE=1     # ASCII-only (no box-drawing chars)
export MS_NO_HYPERLINKS=1  # Disable OSC 8 clickable links
export MS_THEME=dark       # Set theme (auto, dark, light, minimal)
export MS_THEME_MODE=light # Override background detection
export MS_DEBUG_OUTPUT=1   # Print detection diagnostics to stderr
```

### Terminal Requirements

Rich output works best with:
- A terminal supporting 256 colors or TrueColor (most modern terminals)
- UTF-8 locale for Unicode box-drawing and icons
- Terminals with OSC 8 support for clickable hyperlinks (iTerm2, Kitty, WezTerm, Ghostty, Windows Terminal, Konsole, GNOME Terminal 0.50+)

### Troubleshooting

**No colors showing:**
1. Check `echo $TERM` is not `dumb`
2. Check `NO_COLOR` is not set: `unset NO_COLOR`
3. Force rich mode: `MS_FORCE_RICH=1 ms list`
4. Run diagnostics: `MS_DEBUG_OUTPUT=1 ms list 2>&1 | head`

**Garbled box-drawing characters:**
1. Ensure UTF-8 locale: `locale | grep -i utf`
2. Force ASCII: `MS_NO_UNICODE=1 ms list`

**Links not clickable:**
1. Check terminal supports OSC 8 (see Terminal Requirements above)
2. Disable if causing issues: `MS_NO_HYPERLINKS=1`

## Developer Guide

### Architecture

```
src/output/
  mod.rs           -- Module root, re-exports
  detection.rs     -- Environment detection (rich vs plain)
  theme.rs         -- Colors, icons, box styles, terminal capabilities
  rich_output.rs   -- Main RichOutput abstraction
  safe.rs          -- SafeRichOutput with panic recovery
  fallback.rs      -- Minimal fallback renderers
  builders.rs      -- Pre-built renderables (panels, tables, bars)
  messages.rs      -- High-level message renderers
  progress.rs      -- Progress bars and spinners
  errors.rs        -- Error display formatting
  plain_format.rs  -- Machine-readable plain/JSON output
  test_utils.rs    -- Test helpers (EnvGuard, ANSI stripping)
```

### Detection Priority

The output detector checks conditions in this order (first match wins):

1. Machine-readable format (`--format json/jsonl/tsv`)
2. Explicit plain format (`--format plain`)
3. Robot mode (`--robot`)
4. AI agent environment (`CLAUDE_CODE`, `CURSOR_AI`, etc.)
5. CI environment (`CI`, `GITHUB_ACTIONS`, etc.)
6. `NO_COLOR` set
7. `MS_PLAIN_OUTPUT` set
8. stdout is not a terminal (piped/redirected)
9. `MS_FORCE_RICH` set -> rich
10. Default: rich (human on a terminal)

### Adding Rich Output to a Command

```rust
use crate::output::{RichOutput, SuccessRenderer, InfoRenderer};

fn run_my_command(output: &RichOutput) {
    // Semantic output (auto-adapts to mode)
    output.success("Operation completed");
    output.info("Found 42 items");
    output.warning("Deprecated flag used");

    // Structured output
    output.key_value("Layer", "project");
    output.header("Results");

    // High-level renderers
    SuccessRenderer::new(output, "Import complete")
        .detail("5 skills imported")
        .next_step("Run `ms list` to see them")
        .render();

    // Hyperlinks (auto-detects support)
    output.println_hyperlink("Documentation", "https://example.com/docs");
}
```

### Using Builders

```rust
use crate::output::builders;

// Panels
let panel = builders::success_panel_with_width("Created", "Skill saved", 80);
output.println(&panel);

// Search results table
let results = vec![("skill-a", 0.95, "project", "Description here")];
let table = builders::search_results_table(&results, output.width());
output.print_table(&table);

// Quality indicators
let bar = builders::quality_bar(0.85, 20);
output.println(&bar);
```

### Testing Rich Output

Use `RichOutput::plain()` in tests for deterministic output:

```rust
#[test]
fn test_my_output() {
    let output = RichOutput::plain();
    let formatted = output.format_success("done");
    assert!(formatted.contains("done"));
}
```

For environment-dependent tests, use `EnvGuard`:

```rust
use ms::output::test_utils::EnvGuard;

#[test]
fn test_no_color() {
    let _guard = EnvGuard::new().set("NO_COLOR", "1");
    // Environment restored on drop
}
```

For snapshot testing, use `insta`:

```rust
use insta::assert_snapshot;

#[test]
fn visual_my_panel() {
    let panel = success_panel_with_width("OK", "Done", 60);
    assert_snapshot!("my_panel", panel);
}
```

## API Reference

### `RichOutput`

The main abstraction. All output methods adapt to the current mode.

| Method | Description |
|--------|-------------|
| `plain()` | Create plain-mode instance |
| `is_rich()` / `is_plain()` / `is_json()` | Query current mode |
| `width()` | Terminal width in columns |
| `use_unicode()` | Whether Unicode is supported |
| `supports_hyperlinks()` | Whether OSC 8 links are supported |
| `success(msg)` / `error(msg)` / `warning(msg)` / `info(msg)` | Semantic messages |
| `key_value(k, v)` | Key-value pair |
| `header(text)` / `subheader(text)` | Section headers |
| `print_table(table)` / `print_panel(content, title)` | Renderables |
| `print_markdown(md)` / `print_syntax(code, lang)` | Content rendering |
| `format_hyperlink(text, url)` | OSC 8 hyperlink (returns String) |
| `format_file_hyperlink(text, path)` | File path hyperlink |
| `format_styled(text, style)` | Apply style spec (returns String) |
| `progress(current, total, msg)` | Progress bar on stderr |
| `spinner(msg)` | Animated spinner (returns handle) |

### Message Renderers

| Renderer | Purpose |
|----------|---------|
| `SuccessRenderer` | Success with optional next steps and detail |
| `InfoRenderer` | Info messages with key-value context |
| `HintDisplay` | Tips and shortcuts (omitted in agent mode) |
| `StatusTracker` | Multi-step progress tracking |

### Builders

| Builder | Returns |
|---------|---------|
| `success_panel(title, msg)` | Green-bordered panel |
| `error_panel(title, msg)` | Red-bordered panel |
| `warning_panel(title, msg)` | Yellow-bordered panel |
| `search_results_table(results, width)` | Table renderable |
| `quality_bar(score, width)` | Colored progress bar string |
| `quality_indicator(score)` | Single-character indicator |
| `key_value_table(pairs)` | Aligned key-value table |

## Configuration Reference

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `NO_COLOR` | unset | Disable all colors (no-color.org) |
| `MS_PLAIN_OUTPUT` | unset | Force plain text mode |
| `MS_FORCE_RICH` | unset | Force rich mode |
| `MS_NO_UNICODE` | unset | ASCII-only, no Unicode |
| `MS_NO_HYPERLINKS` | unset | Disable OSC 8 hyperlinks |
| `MS_THEME` | `auto` | Theme preset: auto, dark, light, minimal |
| `MS_THEME_MODE` | auto-detect | Override: `light` or `dark` |
| `MS_DEBUG_OUTPUT` | unset | Print detection report to stderr |
| `TERM` | varies | `dumb` disables Unicode |
| `COLORTERM` | varies | Used for color depth detection |

### Agent Environment Variables

These trigger automatic plain mode:

`CLAUDE_CODE`, `CURSOR_AI`, `OPENAI_CODEX`, `AIDER_MODE`, `CODEIUM_ENABLED`,
`WINDSURF_AGENT`, `COPILOT_AGENT`, `COPILOT_WORKSPACE`, `AGENT_MODE`,
`IDE_AGENT`, `CONTINUE_DEV`, `SOURCEGRAPH_CODY`, `TABNINE_AGENT`,
`AMAZON_Q`, `GEMINI_CODE`

### Hyperlink Detection

OSC 8 hyperlinks are enabled when any of these are set:

| Variable | Terminal |
|----------|----------|
| `WT_SESSION` | Windows Terminal |
| `ITERM_SESSION_ID` | iTerm2 |
| `KITTY_WINDOW_ID` | Kitty |
| `KONSOLE_VERSION` | Konsole |
| `WEZTERM_EXECUTABLE` | WezTerm |
| `GHOSTTY_RESOURCES_DIR` | Ghostty |
| `VTE_VERSION` >= 5000 | GNOME Terminal (VTE 0.50+) |

## Agent Integration Guide

### How Agent Detection Works

When `ms` starts, it checks for known AI agent environment variables. If any are found, output automatically switches to plain mode. This ensures agents receive clean, parseable text.

The detection order ensures that explicit flags (`--robot`, `--format json`) always take precedence over environment detection.

### Ensuring Compatibility

For MCP server usage, `RichOutput::plain()` is always used regardless of environment. This guarantees no ANSI escape codes leak into structured responses.

For testing agent mode:
```rust
let output = RichOutput::plain();
// Simulates agent mode output
assert!(output.is_plain());
assert!(!output.supports_hyperlinks());
```

### Best Practices

1. Always use `RichOutput` methods instead of raw `println!`
2. Use `format_*` methods when building strings for structured output
3. Never assume rich mode - always provide plain fallbacks
4. Test with `RichOutput::plain()` for deterministic results
5. Use `HintDisplay` for tips - they're automatically omitted in agent mode
6. Prefer `InfoRenderer` over `output.info()` when context pairs are needed
