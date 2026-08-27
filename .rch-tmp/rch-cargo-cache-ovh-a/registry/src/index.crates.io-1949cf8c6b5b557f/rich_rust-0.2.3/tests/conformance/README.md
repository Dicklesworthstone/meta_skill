# Conformance Fixtures (Python Rich)

This directory contains **fixture-based conformance tests** that compare `rich_rust` output against Python Rich output.

## Fixture Schema

Fixtures are stored in `tests/conformance/fixtures/python_rich.json` with this structure:

```json
{
  "rich_version": "<python rich version or legacy snapshot>",
  "generated_at": "<ISO-8601>",
  "defaults": {
    "width": 40,
    "color_system": "truecolor",
    "force_terminal": true
  },
  "cases": [
    {
      "id": "table/basic",
      "kind": "table",
      "compare_ansi": true,
      "render_options": {
        "width": 40,
        "color_system": "truecolor",
        "force_terminal": true
      },
      "env": {
        "NO_COLOR": null,
        "FORCE_COLOR": "1",
        "TERM": "xterm-256color",
        "COLORTERM": null
      },
      "theme": {
        "styles": { "warning": "bold red" },
        "inherit": true
      },
      "input": {
        "columns": ["Name", "Age"],
        "rows": [["Alice", "30"], ["Bob", "25"]],
        "show_header": true,
        "show_lines": false,
        "title": null,
        "caption": null,
        "column_justifies": ["left", "right"]
      },
      "expected": {
        "plain": "...",
        "ansi": "..."
      },
      "notes": "optional"
    }
  ]
}
```

### Environment Overrides

Cases may specify an `env` object with per-case environment overrides. Values are:

- string: set the environment variable to that string
- null: remove the variable for the duration of the case

The conformance runner clears the standard Rich-related variables (`NO_COLOR`,
`FORCE_COLOR`, `TERM`, `COLORTERM`) before applying per-case overrides to avoid
leaking host environment into fixtures.

### Theme Overrides

Cases may specify a `theme` object to initialize the console theme:

- `styles`: mapping of style name → style definition (e.g. `"bold red"`)
- `inherit`: if true, inherit Python Rich default styles and overlay `styles`

### Normalization Rules

To ensure deterministic comparisons:

- Normalize line endings to `\n` in both Python and Rust outputs.
- Do **not** strip trailing spaces (tables and panels rely on them).
- Preserve ANSI sequences as emitted; only normalize line endings.
- Strip OSC 8 hyperlink IDs (`id=...`) to keep fixtures deterministic.
- `compare_ansi: false` skips ANSI comparison for cases where styling sources differ
  (e.g., syntax highlighting between Pygments and syntect).
- `color_system: "auto"` in `render_options` will use terminal detection rather than
  forcing a specific color system.

## Generating Fixtures

Fixtures are produced by the Python reference runner:

```bash
python tests/conformance/python_reference/generate_fixtures.py
```

The script prefers the bundled `legacy_rich/` snapshot if present; otherwise it falls back to an installed `rich` package. The fixture file records the detected Rich version.

## Running Conformance Tests

```bash
cargo test --test conformance_python --features conformance_test
```

To include feature-flagged renderables:

```bash
cargo test --test conformance_python --features "conformance_test,syntax,markdown,json"
```

## CI Integration

- CI runs fixture-based conformance via:
  `cargo test --test conformance_python --features "conformance_test,syntax,markdown,json"`
- Regenerate fixtures only when behavior changes, and commit the updated JSON.

This test loads fixtures and compares rich_rust output to Python Rich output for each case.

## Update Checklist

When parity changes:

1. Update fixture cases in `generate_fixtures.py`.
2. Re-run the generator.
3. Run `cargo test --test conformance_python`.
4. Update `FEATURE_PARITY.md`, `RICH_SPEC.md`, and README if needed.

## Export Fixtures (HTML/SVG)

HTML/SVG export output is intended to match Python Rich's export templates (including optional
window chrome). For a small, human-readable reference sample, see:

- `tests/conformance/fixtures/python_rich_export.md`
