# Feature Parity (Python Rich -> rich_rust)

> **Purpose:** Authoritative, self-contained parity matrix for the Rust port.
> **Scope:** Compare against core Python Rich behavior and document what is implemented, partial, missing, or explicitly out of scope.
> **Status categories:**
> - **Implemented**: feature exists in rich_rust and is covered by tests.
> - **Partial**: feature exists but is incomplete or has known gaps.
> - **Missing (planned)**: not implemented yet, but in scope.
> - **Out of scope**: intentionally not planned for this project.

This document is the source of truth. If code or docs change, update this file, `RICH_SPEC.md`, README, and `tests/conformance/README.md`.

---

## Status Legend

- **Implemented**: shipping in current codebase.
- **Partial**: some functionality present, but parity gaps remain.
- **Missing (planned)**: explicitly intended but not implemented yet.
- **Out of scope**: excluded by design; not planned.

---

## Related Docs

- `README.md` (public-facing feature overview + examples)
- `RICH_SPEC.md` (detailed behavioral spec)
- `tests/conformance/README.md` (fixture schema + update workflow)

---

## Core Systems

| Area | Python Rich Feature | Status | Rust Location / Flags | Evidence / Notes |
|---|---|---|---|---|
| Console | Console print/render pipeline | Implemented | `src/console.rs` | Extensive unit/e2e tests in `tests/` |
| Console | Capture output segments | Implemented | `Console::begin_capture/end_capture` | `src/console.rs` tests |
| Console | Plain text export | Implemented | `Console::export_text*` | `src/console.rs` tests |
| Style | Style attributes, combine | Implemented | `src/style.rs` | Unit tests + regression tests |
| Color | Named/ANSI/8-bit/truecolor | Implemented | `src/color.rs` | Unit tests + property tests |
| Color | Auto downgrade by terminal | Implemented | `src/color.rs`, `src/terminal.rs` | Regression tests |
| Markup | Rich markup parsing | Implemented | `src/markup/mod.rs` | E2E + regression tests |
| Console | Highlighters (`rich.highlighter`) | Implemented | `src/highlighter.rs`, `src/console.rs` | Unit tests cover `NullHighlighter`, `RegexHighlighter`, `ReprHighlighter`, and Console integration; Python fixture conformance includes `text/highlighter_repr`. |
| Protocol | Protocol hooks (`rich.protocol` / `rich_cast`) | Implemented | `src/protocol.rs`, `src/console.rs`, `src/renderables/mod.rs` | Conformance fixtures: `protocol/rich_cast` (casts via `__rich__` to a string and highlights it), `protocol/measure` (uses `__rich_measure__` via `Console.measure`, output rendered through standard string pipeline). |
| ANSI | ANSI decode (`rich.ansi` / `Text.from_ansi`) | Implemented | `src/ansi.rs`, `src/text.rs`, `src/live.rs` | Conformance fixtures: `text/from_ansi_basic`, `text/from_ansi_osc8_link`. Live proxy writer buffers by line and decodes ANSI (FileProxy parity). |
| Emoji | Emoji code replacement (`:smile:`) | Implemented | `src/emoji.rs`, `src/console.rs` | Unit tests + Python fixture conformance |
| Theme | Theme + named styles (`rule.line`, `[warning]`) | Implemented | `src/theme.rs`, `src/console.rs` | Unit tests + Python fixture conformance (`text/theme_named_style`) |
| Text | Styled spans, wrapping | Implemented | `src/text.rs` | E2E + property tests |
| Measurement | Min/max width protocol | Implemented | `src/measure.rs` | Unit + property tests |
| Unicode width | CJK/emoji width | Implemented | `src/cells.rs` | Unit + regression tests |
| Terminal detection | Color + width detection | Implemented | `src/terminal.rs` | Regression tests |

---

## Renderables (Structured Output)

| Renderable | Python Rich Feature | Status | Rust Location / Flags | Evidence / Notes |
|---|---|---|---|---|
| Table | Auto-sizing tables | Implemented | `src/renderables/table.rs` | E2E + golden tests |
| Panel | Panels with titles/subtitles | Implemented | `src/renderables/panel.rs` | E2E + golden tests |
| Rule | Horizontal rules | Implemented | `src/renderables/rule.rs` | E2E + golden tests |
| Tree | Hierarchical trees | Implemented | `src/renderables/tree.rs` | E2E + golden tests |
| Progress | Progress bars + spinners | Implemented | `src/renderables/progress.rs` | E2E + golden tests |
| Columns | Multi-column layout | Implemented | `src/renderables/columns.rs` | Golden tests |
| Emoji | Emoji renderable | Implemented | `src/renderables/emoji.rs` | Unit tests |
| Padding | Padding | Implemented | `src/renderables/padding.rs` | Golden tests |
| Align | Alignment | Implemented | `src/renderables/align.rs` | Golden tests |
| Constrain | Constrain renderable width (`rich.constrain`) | Implemented | `src/renderables/constrain.rs` | Python fixture conformance: `constrain/rule_width_10`, `constrain/none_passthrough`. |
| Control | Terminal control renderable + control-code helpers (`rich.control`) | Implemented | `src/renderables/control.rs`, `src/segment.rs`, `src/console.rs` | Python fixture conformance: `control/clear`, `control/move_to_column_offset`, `control/title`. Includes `strip_control_codes` and `escape_control_codes`. |
| Pretty / Inspect | Debug-based pretty printing + type inspection | Implemented | `src/renderables/pretty.rs` | Unit tests + snapshots in `src/renderables/snapshots/` |
| Traceback | Traceback rendering + `Console::print_exception` | Implemented | `src/renderables/traceback.rs` | Deterministic explicit frames + Python fixture conformance (`traceback/basic`). Code context via `extra_lines` + `source_context` (from file or embedded). Automatic Rust backtrace capture via `Traceback::capture()` (requires `backtrace` feature). Locals rendering supported when provided explicitly (`TracebackFrame::locals`, `Traceback::show_locals`). |
| Syntax | Syntax highlighting | Partial | `src/renderables/syntax.rs` (feature `syntax`) | Python fixture conformance (`syntax/basic`, `syntax/python_assign`, `syntax/no_terminal`) now checks ANSI directly and passes for covered Rust + non-Rust defaults via `python-rich-default` compatibility behavior. Custom syntect theme/syntax loading is implemented (`Syntax::theme_set`, `Syntax::syntax_set`, `load_themes_from_folder`, `load_syntaxes_from_folder`) and covered by `tests/e2e_syntax.rs`. |
| Markdown | Markdown rendering | Partial | `src/renderables/markdown.rs` (feature `markdown`) | Python fixture conformance covers `markdown/plain`, `markdown/emphasis_no_terminal`, `markdown/link`, `markdown/link_hyperlinks_false`, `markdown/image`, `markdown/fenced_code_rust`, `markdown/fenced_code_wrap` (including OSC8 link behavior, image alt-text rendering, and fenced code blocks rendered via `Syntax(word_wrap=True, padding=1)` semantics). Fenced-code ANSI is now compared for covered Rust fixtures and passes. |
| JSON | JSON pretty-print | Implemented | `src/renderables/json.rs` (feature `json`) | Python fixture conformance covers `json/basic`, `json/nested`, `json/compact_indent_none`, `json/indent_tab`, `json/bools_null`, `json/ensure_ascii`. Supports Python Rich compatible options: `indent: None|int|str`, `sort_keys`, `ensure_ascii`, `highlight`, plus `from_data` constructor parity. |

---

## Output / Export

| Area | Python Rich Feature | Status | Rust Location / Flags | Evidence / Notes |
|---|---|---|---|---|
| ANSI output | Styled ANSI rendering | Implemented | `Style::render_ansi` + `Console::write_segments` | Unit/regression tests |
| HTML export | Console export to HTML | Implemented | `Console::export_html`, `Console::export_html_with_options` | Mirrors Python Rich's HTML export templates (stylesheet mode by default; optional inline styles). |
| SVG export | Console export to SVG | Implemented | `Console::export_svg`, `Console::export_svg_with_options` | Mirrors Python Rich's SVG export template (SVG primitives: `<text>`, `<rect>`, clip paths) with optional terminal-window chrome. |

---

## Live / Layout / Logging

| Feature | Python Rich Feature | Status | Rust Location / Flags | Evidence / Notes |
|---|---|---|---|---|
| Live | Dynamic refresh (`Live`) | Implemented | `src/live.rs` | Nested Live supported; stdout/stderr can be redirected process-wide in interactive terminals (see `LiveOptions.redirect_stdout` / `redirect_stderr`). |
| Layout | Layout engine | Implemented | `src/renderables/layout.rs` | Ratio-based row/column splits with named lookups. |
| Logging | Rich logging handler | Implemented | `src/logging.rs` | `RichLogger` implements the `log` crate; optional `RichTracingLayer` via `tracing` feature. |

---

## Conformance Testing Status

| Topic | Status | Notes |
|---|---|---|
| Internal correctness tests | Implemented | Extensive unit/e2e/golden/regression/property tests in `tests/` |
| Python Rich fixture comparison | Implemented | `tests/conformance/python_reference` + fixtures; gated behind `conformance_test` |

---

## Explicit Exclusions (Out of Scope)

These are intentionally not planned for the Rust port:

- Jupyter/IPython integration
- Legacy Windows cmd.exe (use VT sequences via modern terminals)

---

## Maintenance Notes

- Keep this file aligned with real code paths and tests, not intended scope.
- If fixtures are updated, record the Python Rich version and generator timestamp in the JSON.
- For any new feature, add at least one test reference or conformance note in the Evidence column.

---

## Update Checklist (Keep Docs in Sync)

1. Update **this file** (`FEATURE_PARITY.md`) status and notes.
2. Update `RICH_SPEC.md` exclusions and phase notes.
3. Update README **Feature Parity** section.
4. If conformance fixtures change, update conformance docs and fixture version notes.
