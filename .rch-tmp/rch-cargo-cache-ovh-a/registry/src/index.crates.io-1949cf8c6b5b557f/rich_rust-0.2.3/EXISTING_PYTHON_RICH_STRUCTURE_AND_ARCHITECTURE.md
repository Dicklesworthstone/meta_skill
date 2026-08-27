# Existing Rich Structure and Architecture

> Comprehensive specification of the Legacy Rich codebase for porting to Rust.
> This document serves as the complete reference - consult this instead of source files.

## Table of Contents
1. [Project Overview](#1-project-overview)
2. [Directory Structure](#2-directory-structure)
3. [Data Types and Models](#3-data-types-and-models)
4. [Rendering Pipeline](#4-rendering-pipeline)
5. [Core Components](#5-core-components)
6. [Terminal Handling](#6-terminal-handling)
7. [Key Architectural Patterns](#7-key-architectural-patterns)
8. [Exclusions and Simplifications](#8-exclusions-and-simplifications)
9. [Porting Considerations](#9-porting-considerations)

---

## 1. Project Overview

**Rich** is a Python library for rich text and beautiful formatting in the terminal. It provides an abstraction over ANSI escape codes to render styled text, tables, markdown, syntax highlighted code, and more.

**Key Design Principles:**
- **Console-Centric:** The `Console` object is the god-object that manages global state, I/O, and the rendering loop.
- **Renderable Protocol:** Any object implementing `__rich_console__` (or `__rich__`) can be rendered. This corresponds to a Rust trait (e.g., `RichDisplay`).
- **Segment-Based Rendering:** High-level objects (Tables, Panels) break down into lists of `Segment` objects (text + style) before being written to the stream.
- **Auto-Detection:** Rich automatically detects terminal capabilities (color support, dimensions, legacy Windows) and degrades gracefully.

---

## 2. Directory Structure

The core logic resides in `rich/`.

```
rich/
├── console.py       # THE CORE. Manages I/O, state, and the main render loop.
├── text.py          # Fundamental rich text object. Handles spans and styles.
├── style.py         # Style definitions (color + attributes).
├── segment.py       # Low-level rendering primitive (text chunk + style).
├── color.py         # Color parsing and conversion logic.
├── theme.py         # Registry of style aliases.
├── box.py           # Box drawing characters.
├── table.py         # Complex table layout engine.
├── padding.py       # Padding helper.
├── measure.py       # Logic for calculating object widths.
├── protocol.py      # Protocol definitions (RichCast, ConsoleRenderable).
├── ansi.py          # ANSI decoding logic.
└── ... (many specific renderables: markdown, syntax, tree, bar, etc.)
```

---

## 3. Data Types and Models

### 3.1 `Console` (The Context)
The `Console` holds the configuration for a rendering session.

**Core Fields:**
- `file`: The I/O stream (stdout/stderr).
- `width` / `height`: Terminal dimensions.
- `color_system`: Enum (`standard`, `256`, `truecolor`, `windows`).
- `theme`: A `Theme` object mapping names to styles.
- `record`: Boolean, whether to buffer output for export.
- `safe_box`: Boolean, whether to use ASCII fallback for boxes.

**Rust Mapping:**
```rust
pub struct Console {
    pub options: ConsoleOptions,
    pub buffer: Vec<Segment>,
    // ... writer, lock, etc.
}
```

### 3.2 `Style` (The Visuals)
Represents the visual attributes of text. It is immutable and composable.

**Core Fields:**
- `color`: Foreground color (`Color` object).
- `bgcolor`: Background color (`Color` object).
- `attributes`: Bitmask of booleans (bold, dim, italic, underline, blink, reverse, hidden, strike).
- `link`: Optional URL string for hyperlinks.

**Rust Mapping:**
```rust
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Style {
    pub color: Option<Color>,
    pub bgcolor: Option<Color>,
    pub attributes: Attributes, // Bitflags
    pub link: Option<String>,
}
```

### 3.3 `Text` (The Content)
A string with applied styles. Internally manages a list of `Span` objects.

**Core Fields:**
- `_text`: The raw plain text list.
- `_spans`: List of `Span(start, end, style)`.
- `style`: Base style for the whole text.
- `justify`: Alignment (`left`, `center`, `right`, `full`).
- `overflow`: Overflow handling (`fold`, `crop`, `ellipsis`).

**Rust Mapping:**
```rust
pub struct Text {
    pub plain: String,
    pub spans: Vec<Span>,
    pub style: Style,
    // ... layout options
}

pub struct Span {
    pub start: usize,
    pub end: usize,
    pub style: Style,
}
```

### 3.4 `Segment` (The Atom)
The atomic unit of rendering. A piece of text with a *single* style applied. The render pipeline produces an iterable of these.

**Core Fields:**
- `text`: String content.
- `style`: `Style` object.
- `control`: Optional control codes (e.g., cursor movement).

**Rust Mapping:**
```rust
pub struct Segment {
    pub text: String,
    pub style: Style,
}
```

---

## 4. Rendering Pipeline

The rendering process converts high-level objects into a stream of ANSI characters.

1.  **Input:** User passes an object to `console.print(obj)`.
2.  **Protocol Check:**
    - If `obj` has `__rich_console__`, call it. It returns `Iterable[Segment | Renderable]`.
    - If `obj` has `__rich__`, call it. It returns a renderable (usually `Text`).
    - If `str`, convert to `Text`.
3.  **Flattening:** Recursively expand all renderables until only `Segment` objects remain.
4.  **Buffering/Output:**
    - If `record=True`, store segments.
    - Otherwise, encode segments to ANSI string immediately.
5.  **ANSI Generation:**
    - Iterate segments.
    - Calculate diff between `current_style` and `segment.style`.
    - Emit ANSI SGR codes to transition style.
    - Emit segment text.

**Invariants:**
- The pipeline MUST handle recursive objects (e.g., a Table containing a Panel containing Text).
- It MUST respect `ConsoleOptions` (width/height constraints) passed down the stack.

---

## 5. Core Components

### 5.1 `Table`
A complex layout engine.

**Key Logic:**
- **Column Calculation:**
    - Calculates min/max width for every cell.
    - Aggregates to find min/max width for columns.
    - Distributes available width (from `ConsoleOptions`) to columns based on `ratio` or content.
- **Rendering:**
    - Renders cells line-by-line.
    - Wraps cell content using `render_lines`.
    - Vertically aligns cells in a row (top/middle/bottom).
    - Draws box borders around cells.

### 5.2 `Box`
Defines the characters for borders (ASCII, UTF-8).

**Structure:**
- 8 lines defining: top, head, head_row, mid, row, foot_row, foot, bottom.
- Methods to generate rows based on column widths.

### 5.3 `Padding`
Adds whitespace around a renderable.

**Logic:**
- Accepts `(top, right, bottom, left)` tuple.
- `__rich_console__` yields blank lines for top/bottom.
- Wraps inner renderable lines with space segments for left/right.

---

## 6. Terminal Handling

### 6.1 Color Systems
Rich downgrades colors based on terminal capabilities.

- **TRUECOLOR (24-bit):** RGB values (8, 8, 8). Used directly.
- **EIGHT_BIT (256 colors):** Maps RGB to nearest index in the standard 256-color palette.
- **STANDARD (16 colors):** Maps to nearest standard ANSI color (0-15).
- **WINDOWS:** Legacy Windows console colors.

**Algorithm:**
- Rich uses a cached lookup or formula (like `rgb_to_hls`) to find the nearest match when downgrading.

### 6.2 Width Measurement
- Uses `wcwidth` (or equivalent logic) to determine the cell width of unicode characters (e.g., emojis are often 2 cells).
- **Critical:** Rust port must use `unicode-width` crate to match this behavior.

---

## 7. Key Architectural Patterns

### 7.1 The "Console Options" Pattern
Every `__rich_console__` method receives `ConsoleOptions`.
- This struct is **immutable** (conceptually) during a render pass.
- It contains `min_width`, `max_width`, `is_terminal`, etc.
- Renderables derive new options for their children (e.g., a Panel reduces `max_width` by 2 for its borders before rendering content).

### 7.2 The "Measurement" Protocol
Before rendering, Rich may need to know how big an object *wants* to be.
- Method: `__rich_measure__(console, options) -> Measurement`
- Returns: `Measurement(min, max)`
- Used by `Table` to auto-size columns.

---

## 8. Exclusions and Simplifications

**NOT Porting (Initial Phase):**
- **Jupyter/IPython Integration:** Python-specific.
- **`inspect` module:** Python runtime introspection.
- **`traceback` module:** Python exception handling.
- **`logging` integration:** Python `logging` handler.
- **Legacy Windows Support:** Focus on VT sequences (modern Windows Terminal supports this). Use `crossterm` abstraction in Rust.
- **Emoji codes:** The massive database of `:smile:` aliases. Use native unicode characters or a smaller subset initially.

---

## 9. Porting Considerations

### 9.1 Rust Traits
- Python `__rich_console__` -> Rust `trait ConsoleRenderable`
- Python `__rich_measure__` -> Rust `trait Measurable`

### 9.2 String Handling
- Python strings are immutable Unicode.
- Rust `String` is UTF-8.
- Rich relies heavily on string slicing. In Rust, be careful with char indices vs byte indices. Use `unicode-segmentation`.

### 9.3 Reference Counting
- Rich style objects are often shared. Rust `Arc<Style>` or small `Copy` structs (if using bitflags) are appropriate. `Style` in Python is essentially a bag of optional attributes; in Rust, bitflags are much more efficient.

### 9.4 Global State
- Python has a module-level default `Console`.
- Rust should avoid global mutable state. The user should instantiate a `Console` and pass it around, or use a thread-local if absolutely necessary for a `print!` macro equivalent.

---
