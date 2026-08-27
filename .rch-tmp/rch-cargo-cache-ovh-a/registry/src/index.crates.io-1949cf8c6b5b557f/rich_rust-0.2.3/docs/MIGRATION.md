# Migration Guide: Python Rich to rich_rust

This guide helps Python Rich users migrate to rich_rust. Both libraries share similar concepts and markup syntax, but there are Rust-specific patterns you'll need to learn.

## Quick Comparison

| Aspect | Python Rich | rich_rust |
|--------|-------------|-----------|
| Language | Python | Rust |
| Install | `pip install rich` | `cargo add rich_rust` |
| Markup | `[bold red]text[/]` | `[bold red]text[/]` (same!) |
| Console | `Console()` | `Console::new()` |
| Style | `Style(bold=True)` | `Style::new().bold()` |
| Async | Native | Not built-in (sync) |

## Feature Mapping

### Core Features

| Python Rich | rich_rust | Status |
|-------------|-----------|--------|
| `Console` | `Console` | Full |
| `Text` | `Text` | Full |
| `Style` | `Style` | Full |
| `Table` | `Table` | Full |
| `Panel` | `Panel` | Full |
| `Rule` | `Rule` | Full |
| `Columns` | `Columns` | Full |
| `Tree` | `Tree` | Full |
| `Padding` | `Padding` | Full |
| `Align` | `Align` | Full |

### Optional Features (require feature flags)

| Python Rich | rich_rust | Feature Flag |
|-------------|-----------|--------------|
| `Syntax` | `Syntax` | `syntax` |
| `Markdown` | `Markdown` | `markdown` |
| `JSON` | `Json` | `json` |
| `Traceback` capture | `Traceback::capture` | `backtrace` |
| `tracing` integration | `RichTracingLayer` | `tracing` |

### Additional Systems

| Python Rich | rich_rust | Notes |
|-------------|-----------|-------|
| `Live` | `Live` | Dynamic refresh + optional process-wide stdout/stderr redirection in interactive terminals |
| `Console.status(...)` | `Status` | Spinner + message helper built on `Live` |
| `Prompt` / `Confirm` / `IntPrompt` | `Prompt` / `Confirm` / `Select` | Output-focused interactive helpers (degrade cleanly when non-interactive) |
| Logging handler (`RichHandler`) | `RichLogger` | Implements the `log` crate; prints Rich-style log lines |
| Tracebacks (`rich.traceback`) | `Traceback` | Deterministic explicit frames; optional runtime backtrace capture behind `backtrace` |

### Explicit Exclusions (Out of Scope)

- Jupyter/IPython integration
- Legacy Windows cmd.exe (use modern terminals with VT support)

## API Differences

### Console Creation

**Python:**
```python
from rich.console import Console

console = Console()
console = Console(width=80, force_terminal=True)
```

**Rust:**
```rust
use rich_rust::prelude::*;

let console = Console::new();
let console = Console::builder()
    .width(80)
    .force_terminal(true)
    .build();
```

### Printing with Markup

**Python:**
```python
console.print("[bold red]Error:[/] Something went wrong")
console.print("[green]Success![/]")
```

**Rust:**
```rust
console.print("[bold red]Error:[/] Something went wrong");
console.print("[green]Success![/]");
```

The markup syntax is identical!

### Printing without Markup

**Python:**
```python
console.print("[literal brackets]", markup=False)
```

**Rust:**
```rust
console.print_plain("[literal brackets]");
```

### Creating Styles

**Python:**
```python
from rich.style import Style

style = Style(bold=True, color="red")
style = Style.parse("bold red on white")
```

**Rust:**
```rust
use rich_rust::style::Style;
use rich_rust::color::Color;

let style = Style::new().bold().color(Color::parse("red").unwrap());
let style = Style::parse("bold red on white").unwrap();
```

### Creating Text

**Python:**
```python
from rich.text import Text

text = Text("Hello World")
text.stylize("bold", 0, 5)
```

**Rust:**
```rust
use rich_rust::text::Text;
use rich_rust::style::Style;

let mut text = Text::new("Hello World");
text.stylize(0, 5, Style::new().bold());
```

### Creating Tables

**Python:**
```python
from rich.table import Table

table = Table(title="Users")
table.add_column("Name", style="cyan")
table.add_column("Age", justify="right")
table.add_row("Alice", "30")
table.add_row("Bob", "25")
console.print(table)
```

**Rust:**
```rust
use rich_rust::prelude::*;

let mut table = Table::new()
    .title("Users")
    .with_column(Column::new("Name").style(Style::parse("cyan").unwrap()))
    .with_column(Column::new("Age").justify(JustifyMethod::Right));

table.add_row_cells(["Alice", "30"]);
table.add_row_cells(["Bob", "25"]);

for seg in table.render(80) {
    print!("{}", seg.text);
}
```

### Creating Panels

**Python:**
```python
from rich.panel import Panel

panel = Panel("Hello World", title="Greeting")
console.print(panel)
```

**Rust:**
```rust
use rich_rust::prelude::*;

let panel = Panel::from_text("Hello World")
    .title("Greeting");

for seg in panel.render(80) {
    print!("{}", seg.text);
}
```

### Creating Trees

**Python:**
```python
from rich.tree import Tree

tree = Tree("Root")
tree.add("Child 1")
branch = tree.add("Child 2")
branch.add("Grandchild")
console.print(tree)
```

**Rust:**
```rust
use rich_rust::prelude::*;

let mut root = TreeNode::new("Root");
root.add_child(TreeNode::new("Child 1"));
let mut child2 = TreeNode::new("Child 2");
child2.add_child(TreeNode::new("Grandchild"));
root.add_child(child2);

let tree = Tree::new(root);
for seg in tree.render(80) {
    print!("{}", seg.text);
}
```

### Horizontal Rules

**Python:**
```python
from rich.rule import Rule

console.rule("Section Title")
console.print(Rule(style="cyan"))
```

**Rust:**
```rust
use rich_rust::prelude::*;

console.rule(Some("Section Title"));

// Or with custom style:
let rule = Rule::with_title("Section")
    .style(Style::parse("cyan").unwrap());
```

## Markup Syntax Reference

The markup syntax is identical between Python Rich and rich_rust:

| Markup | Effect |
|--------|--------|
| `[bold]text[/]` | Bold |
| `[italic]text[/]` | Italic |
| `[underline]text[/]` | Underline |
| `[strike]text[/]` | Strikethrough |
| `[red]text[/]` | Red foreground |
| `[on blue]text[/]` | Blue background |
| `[bold red on white]text[/]` | Combined |
| `[#ff0000]text[/]` | Hex color |
| `[rgb(255,0,0)]text[/]` | RGB color |
| `[color(196)]text[/]` | 256-color palette |
| `[link=https://...]text[/]` | Hyperlink |

### Escaping Brackets

**Python:**
```python
console.print(r"\[not markup\]")
```

**Rust:**
```rust
console.print(r"\[not markup\]");
```

## Feature Flags

Enable optional features in your `Cargo.toml`:

```toml
[dependencies]
rich_rust = { version = "0.1", features = ["syntax", "markdown", "json"] }

# Or enable all:
rich_rust = { version = "0.1", features = ["full"] }
```

### Syntax Highlighting

**Python:**
```python
from rich.syntax import Syntax

syntax = Syntax(code, "python", line_numbers=True)
console.print(syntax)
```

**Rust (requires `syntax` feature):**
```rust
use rich_rust::prelude::*;

let syntax = Syntax::new(code, "python")
    .line_numbers(true);

for seg in syntax.render(80) {
    print!("{}", seg.text);
}
```

### Markdown

**Python:**
```python
from rich.markdown import Markdown

md = Markdown("# Hello\n\nWorld")
console.print(md)
```

**Rust (requires `markdown` feature):**
```rust
use rich_rust::prelude::*;

let md = Markdown::new("# Hello\n\nWorld");

for seg in md.render(80) {
    print!("{}", seg.text);
}
```

## Key Differences Summary

1. **Builder Pattern**: Rust uses builder methods (`Style::new().bold()`) instead of keyword arguments
2. **Explicit Rendering**: Call `.render(width)` to get segments, then iterate
3. **Error Handling**: Methods that can fail return `Result`, use `.unwrap()` or proper error handling
4. **Ownership**: Rust's ownership model means some methods take `&self`, others `&mut self`
5. **Interactive helpers**: rich_rust includes Live/Status/Prompt helpers, but it is not a full TUI widget framework
6. **Feature Flags**: Optional features (syntax, markdown, json, tracing, backtrace) require explicit Cargo.toml flags

## Common Migration Patterns

### Python idiom: Style chaining
```python
style = Style(bold=True) + Style(color="red")
```

**Rust equivalent:**
```rust
let style = Style::new().bold() + Style::new().color(Color::parse("red").unwrap());
// Or:
let style = Style::new().bold().color(Color::parse("red").unwrap());
```

### Python idiom: Console recording
```python
console = Console(record=True)
console.print("Hello")
html = console.export_html()
```

**Rust equivalent:**
```rust
let console = Console::new();
console.begin_capture();
console.print("Hello");
let html = console.export_html(false);
let svg = console.export_svg(true);
```

### Python idiom: Render to string
```python
from io import StringIO
console = Console(file=StringIO())
console.print("Hello")
output = console.file.getvalue()
```

**Rust equivalent:**
```rust
use std::io::Write;

let buffer = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

// Create a wrapper that implements Write + Send
struct SharedBuffer(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
impl Write for SharedBuffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.lock().unwrap().flush()
    }
}

let console = Console::builder()
    .file(Box::new(SharedBuffer(buffer.clone())))
    .build();

console.print("Hello");
let output = String::from_utf8_lossy(&buffer.lock().unwrap()).to_string();
```

## Getting Help

- [rich_rust Documentation](https://docs.rs/rich_rust)
- [RICH_SPEC.md](../RICH_SPEC.md) - Detailed behavioral specification
- [Examples](../examples/) - Working code examples
- [GitHub Issues](https://github.com/Dicklesworthstone/rich_rust/issues)
