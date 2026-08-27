# Plan: Port Rich to Rust

> **Project:** rich_rust
> **Author:** Port initiated by Jeffrey Emanuel
> **Status:** Parity expansion in progress (target: full Python Rich parity)

## Executive Summary

This project aims to port the core functionality of the Python `rich` library to idiomatic Rust. `rich` is the gold standard for beautiful terminal output in the Python ecosystem. By porting it to Rust, we aim to provide the same high-level, ergonomic API for terminal styling, tables, and progress bars, but with the zero-cost abstractions, type safety, and performance benefits of Rust.

The goal is **not** a line-by-line translation. The goal is to capture the *behavior* and *capabilities* of Rich (the "Specification") and implement them using Rust best practices (`rich_rust`).

## Why Port to Rust?

1.  **Performance:** Rich is fast enough for Python, but Rust can eliminate allocation overhead in hot paths (like rendering large tables or real-time logs) and prevent screen flicker entirely.
2.  **Type Safety:** Rich uses runtime checks and protocols (`__rich__`). Rust's trait system (`RichDisplay`) enforces these contracts at compile time.
3.  **Ecosystem Gap:** While crates like `crossterm`, `termion`, and `ratatui` exist, there isn't a high-level "print beautiful text/tables/json easily" library that matches Rich's ergonomics.
4.  **Binary Size:** A Rust CLI using `rich_rust` will be a single small binary, unlike Python apps requiring an interpreter.

## What We're Porting (The "Core")

We are targeting the "classic" Rich feature set that defines its character:

- **Console API:** The central entry point for printing.
- **Rich Text:** Styled text with spans, parsing from markup (e.g., `[bold red]Hello[/]`).
- **Styling:** Comprehensive style system (colors, attributes, links) with combination logic.
- **Renderables:**
    - **Tables:** Complex layout with auto-sizing columns, borders, headers/footers.
    - **Panel:** Boxed content with titles.
    - **Padding/Align:** Layout primitives.
    - **Syntax:** Syntax highlighting (using `syntect` probably, mapping from `pygments` logic).
    - **Markdown:** Rendering markdown to terminal.
- **Terminal Detection:** Auto-detecting color support (16/256/TrueColor) and dimensions.

## Parity Scope

The current goal is **full parity with upstream Python Rich** wherever it makes
sense in Rust.

- Remaining gaps are tracked in `FEATURE_PARITY.md` and Beads issues (`.beads/`).
- For Python-only integrations (e.g. Jupyter), we aim for Rust-idiomatic
  equivalents and document any irreducible differences explicitly.

## Reference Projects

We will leverage patterns from high-quality Rust CLI crates:

- **`ratatui`**: For underlying rendering concepts (buffers, areas) where applicable, though Rich is streaming (immediate mode) vs Ratatui's retained mode.
- **`nu-ansi-term` / `anstyle`**: For low-level ANSI code generation.
- **`syntect`**: For syntax highlighting (replacing Pygments).
- **`crossterm`**: For raw terminal capability detection.

## Architecture Overview

1.  **`Console`**: Holds state (writer, theme, options). Uses a `Buffer` or streams directly.
2.  **`Trait RichDisplay`**: The Rust equivalent of `__rich__`. Any type implementing this can be printed.
3.  **`Segment`**: The atomic rendering unit (`text: String`, `style: Style`).
4.  **`Measure` trait**: For calculating required width (min/max) before rendering (crucial for Tables).

## Implementation Phases

### Phase 1: Foundation (The Spine) - [x] Complete
- **Data Models:** `Style`, `Color`, `Text`, `Segment`.
- **Markup Parser:** Re-implement the `[style]text[/]` parsing logic.
- **Console:** Basic `print` that handles styles and auto-resets.

### Phase 2: Layout Engine - [x] Complete
- **Measurement:** Implement the width measurement protocol.
- **Table:** The most complex renderable. Requires column resizing logic.
- **Box:** Border rendering.

### Phase 3: Advanced Renderables - [x] Complete
- **Syntax:** Integration with `syntect`.
- **Markdown:** Integration with `pulldown-cmark`.
- **Panel/Rule:** Decorative elements.

### Phase 4: Polish & Performance - [x] Complete
- **Zero-allocation Rendering:** Optimize `Segment` output to avoiding string copies. (Implemented via `Cow<'a, str>` and byte slicing)
- **Concurrency:** Ensure `Console` is thread-safe. (Verified via tests)

## Success Criteria
- **Output Parity:** `rich_rust` output looks pixel-identical to `rich` for supported features.
- **Markup Compatibility:** Existing Rich markup strings work in Rust.
- **Ergonomics:** `console.print("Hello [b]World[/]")` works in Rust.

---
*Created by Gemini based on Deep Scan of Textualize/rich*
