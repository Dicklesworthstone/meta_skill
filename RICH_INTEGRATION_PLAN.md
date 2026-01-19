# Rich Rust Integration Plan for Meta Skill

## Executive Summary

This plan details the comprehensive integration of `rich_rust` throughout the `meta_skill` codebase to provide premium, stylish console output for human observers while maintaining **zero interference** with AI agent/robot mode users.

**Core Principle**: Humans watching get a beautiful, polished experience. Agents get clean, parseable output unchanged.

---

## Table of Contents

1. [Agent-Safety Architecture](#1-agent-safety-architecture)
2. [Theme System Design](#2-theme-system-design)
3. [Integration Points by Module](#3-integration-points-by-module)
4. [Implementation Phases](#4-implementation-phases)
5. [Component Specifications](#5-component-specifications)
6. [Testing Strategy](#6-testing-strategy)
7. [Configuration](#7-configuration)

---

## 1. Agent-Safety Architecture

### 1.1 The Golden Rule

```
IF output_format == Robot OR !is_terminal() OR NO_COLOR is set
THEN use plain text output (current behavior)
ELSE use rich_rust styled output
```

### 1.2 Detection Hierarchy

```rust
/// Determines if rich output should be used
pub fn should_use_rich_output(config: &Config, output_format: &OutputFormat) -> bool {
    // 1. Robot mode always gets plain output
    if matches!(output_format, OutputFormat::Json | OutputFormat::Jsonl | OutputFormat::Tsv) {
        return false;
    }

    // 2. Explicit robot flag
    if config.robot_mode {
        return false;
    }

    // 3. Environment variable overrides
    if std::env::var("NO_COLOR").is_ok() {
        return false;
    }
    if std::env::var("MS_PLAIN_OUTPUT").is_ok() {
        return false;
    }

    // 4. Not a terminal (piped output)
    if !rich_rust::terminal::is_terminal() {
        return false;
    }

    // 5. Explicit force-rich override
    if std::env::var("MS_FORCE_RICH").is_ok() {
        return true;
    }

    // 6. Default: human format on terminal = rich
    matches!(output_format, OutputFormat::Human)
}
```

### 1.3 Output Abstraction Layer

Create a new module `src/output/rich_output.rs`:

```rust
pub struct RichOutput {
    console: Option<Console>,
    plain_mode: bool,
    theme: Theme,
}

impl RichOutput {
    pub fn new(config: &Config, format: &OutputFormat) -> Self {
        let use_rich = should_use_rich_output(config, format);
        Self {
            console: if use_rich { Some(Console::new()) } else { None },
            plain_mode: !use_rich,
            theme: Theme::from_config(config),
        }
    }

    /// Print with automatic mode detection
    pub fn print(&self, plain: &str, rich_fn: impl FnOnce(&Console, &Theme)) {
        match &self.console {
            Some(console) => rich_fn(console, &self.theme),
            None => println!("{}", plain),
        }
    }

    /// Print a styled message (degrades gracefully)
    pub fn styled(&self, text: &str, style: &str) {
        match &self.console {
            Some(console) => console.print(&format!("[{}]{}[/]", style, text)),
            None => println!("{}", text),
        }
    }
}
```

### 1.4 Ensuring Agent Compatibility

| Scenario | Detection | Output Mode |
|----------|-----------|-------------|
| `ms search --robot` | OutputFormat::Json | Plain JSON |
| `ms load skill \| jq` | !is_terminal() | Plain text |
| `NO_COLOR=1 ms search` | env var | Plain text |
| `ms search` (human on TTY) | Human + TTY | Rich styled |
| Agent calling via MCP | MCP protocol | JSON-RPC (unchanged) |
| Subprocess from agent | Often !TTY | Plain text |

---

## 2. Theme System Design

### 2.1 Semantic Color Palette

```rust
/// Semantic colors that convey meaning
pub struct ThemeColors {
    // Status colors
    pub success: Style,      // green - operations completed
    pub error: Style,        // red - failures
    pub warning: Style,      // yellow - cautions
    pub info: Style,         // blue - informational
    pub hint: Style,         // dim cyan - suggestions

    // Entity colors
    pub skill_name: Style,   // bold cyan - skill identifiers
    pub tag: Style,          // magenta - tags/labels
    pub path: Style,         // dim white - file paths
    pub url: Style,          // underline blue - links
    pub code: Style,         // green on dark - code snippets
    pub command: Style,      // bold white - shell commands

    // Data colors
    pub key: Style,          // blue - JSON/config keys
    pub value: Style,        // white - values
    pub number: Style,       // cyan - numeric values
    pub string: Style,       // green - string values
    pub boolean: Style,      // yellow - true/false
    pub null: Style,         // dim magenta - null/none

    // Structure colors
    pub header: Style,       // bold white - section headers
    pub subheader: Style,    // bold dim - subsection headers
    pub border: Style,       // dim white - table/panel borders
    pub separator: Style,    // dim - dividers
    pub emphasis: Style,     // bold - important text
    pub muted: Style,        // dim - less important text

    // Progress colors
    pub progress_done: Style,     // green - completed portion
    pub progress_remaining: Style, // dim - remaining portion
    pub spinner: Style,           // cyan - spinner animation
}
```

### 2.2 Default Theme (Dark Terminal Optimized)

```rust
impl Default for ThemeColors {
    fn default() -> Self {
        Self {
            // Status
            success: Style::parse("bold green").unwrap(),
            error: Style::parse("bold red").unwrap(),
            warning: Style::parse("bold yellow").unwrap(),
            info: Style::parse("bold blue").unwrap(),
            hint: Style::parse("dim cyan").unwrap(),

            // Entities
            skill_name: Style::parse("bold cyan").unwrap(),
            tag: Style::parse("magenta").unwrap(),
            path: Style::parse("dim").unwrap(),
            url: Style::parse("underline blue").unwrap(),
            code: Style::parse("green").unwrap(),
            command: Style::parse("bold").unwrap(),

            // Data
            key: Style::parse("blue").unwrap(),
            value: Style::new(),
            number: Style::parse("cyan").unwrap(),
            string: Style::parse("green").unwrap(),
            boolean: Style::parse("yellow").unwrap(),
            null: Style::parse("dim magenta italic").unwrap(),

            // Structure
            header: Style::parse("bold").unwrap(),
            subheader: Style::parse("bold dim").unwrap(),
            border: Style::parse("dim").unwrap(),
            separator: Style::parse("dim").unwrap(),
            emphasis: Style::parse("bold").unwrap(),
            muted: Style::parse("dim").unwrap(),

            // Progress
            progress_done: Style::parse("green").unwrap(),
            progress_remaining: Style::parse("dim").unwrap(),
            spinner: Style::parse("cyan").unwrap(),
        }
    }
}
```

### 2.3 Configuration Integration

Add to `config.toml`:

```toml
[theme]
# Preset: "default", "minimal", "vibrant", "monochrome"
preset = "default"

# Override individual colors (optional)
[theme.colors]
success = "bold green"
error = "bold red"
skill_name = "bold cyan"
# ... etc
```

---

## 3. Integration Points by Module

### 3.1 CLI Commands (`src/cli/commands/`)

#### 3.1.1 `search.rs` - Search Results

**Current**: Plain text list of results
**Enhanced**:

```rust
// Search results as styled table
let mut table = Table::new()
    .title("Search Results")
    .with_column(Column::new("Score").justify(JustifyMethod::Right))
    .with_column(Column::new("Skill").min_width(20))
    .with_column(Column::new("Tags"))
    .with_column(Column::new("Description").max_width(50));

for result in results {
    table.add_row_cells([
        &format!("{:.2}", result.score),
        &result.skill_id,
        &result.tags.join(", "),
        &truncate(&result.description, 50),
    ]);
}

output.print_renderable(&table);
```

**Visual Example**:
```
╭─────────────────────── Search Results ───────────────────────╮
│ Score │ Skill                │ Tags           │ Description  │
├───────┼──────────────────────┼────────────────┼──────────────┤
│  0.95 │ rust-error-handling  │ rust, errors   │ Best practi… │
│  0.87 │ debugging-patterns   │ debug, testing │ Common debu… │
│  0.72 │ logging-strategies   │ rust, logging  │ Structured … │
╰──────────────────────────────────────────────────────────────╯
```

#### 3.1.2 `load.rs` - Skill Loading

**Current**: Raw markdown/text dump
**Enhanced**:

```rust
// Skill header panel
let header_panel = Panel::from_text(&format!(
    "{}\n{}\nTags: {}",
    skill.name,
    skill.description,
    skill.tags.join(", ")
))
.title("Skill")
.subtitle(&format!("v{}", skill.version))
.border_style(theme.border);

output.print_renderable(&header_panel);

// Section rules
output.rule(Some("Overview"));

// Code blocks with syntax highlighting
if let Some(code) = &section.code {
    let syntax = Syntax::new(code, &section.language)
        .theme("base16-ocean.dark")
        .line_numbers(true);
    output.print_renderable(&syntax);
}
```

**Visual Example**:
```
╭─────────────────────── Skill ─────────────────────── v1.2.0 ─╮
│ Rust Error Handling                                          │
│ Best practices for error handling in Rust projects.          │
│ Tags: rust, errors, best-practices                           │
╰──────────────────────────────────────────────────────────────╯

─────────────────────────── Overview ───────────────────────────

Use `Result<T, E>` and propagate errors with `?`.

─────────────────────────── Examples ───────────────────────────

  1 │ fn read_config(path: &str) -> Result<Config, ConfigError> {
  2 │     let contents = std::fs::read_to_string(path)?;
  3 │     toml::from_str(&contents).map_err(ConfigError::Parse)
  4 │ }
```

#### 3.1.3 `suggest.rs` - Suggestions

**Current**: Plain list
**Enhanced**: Cards with confidence indicators

```rust
// Suggestion cards
for (i, suggestion) in suggestions.iter().enumerate() {
    let confidence_bar = ProgressBar::new()
        .completed(suggestion.score * 100.0)
        .total(100.0)
        .width(20)
        .style(theme.progress_done);

    let card = Panel::from_text(&format!(
        "{}\n{}\nConfidence: {}",
        suggestion.skill_id,
        suggestion.reason,
        render_progress_inline(suggestion.score)
    ))
    .title(&format!("#{}", i + 1))
    .rounded();

    output.print_renderable(&card);
}
```

#### 3.1.4 `index.rs` - Indexing Progress

**Current**: Print statements
**Enhanced**: Live progress with spinner

```rust
let spinner = Spinner::dots();
let progress = ProgressBar::new()
    .total(total_skills as f64)
    .width(40);

for (i, skill_path) in paths.iter().enumerate() {
    output.update_progress(&format!(
        "{} Indexing {} [{}/{}]",
        spinner.frame(),
        skill_path.display(),
        i + 1,
        total_skills
    ));

    // ... index skill ...

    output.set_progress(i as f64);
}

output.finish_progress(&format!(
    "{} Indexed {} skills",
    theme.success.render("✓"),
    total_skills
));
```

#### 3.1.5 `build.rs` - Skill Building (CASS Mining)

**Current**: Verbose text output
**Enhanced**: Step-by-step wizard UI

```rust
// Build wizard steps
let steps = vec![
    ("Searching CASS", "Finding relevant sessions..."),
    ("Analyzing Sessions", "Extracting patterns..."),
    ("Synthesizing Skill", "Generating structured content..."),
    ("Validating", "Checking skill quality..."),
];

for (i, (title, description)) in steps.iter().enumerate() {
    output.step(i + 1, steps.len(), title, description);
    // ... do work ...
    output.step_complete(i + 1);
}
```

**Visual**:
```
╭─ Building Skill from CASS ─────────────────────────────────╮
│                                                            │
│  [1/4] ✓ Searching CASS                                    │
│  [2/4] ✓ Analyzing Sessions                                │
│  [3/4] ⋯ Synthesizing Skill                                │
│  [4/4]   Validating                                        │
│                                                            │
│  Found 12 relevant sessions                                │
│  Extracted 8 patterns with high confidence                 │
│                                                            │
╰────────────────────────────────────────────────────────────╯
```

#### 3.1.6 `doctor.rs` - Health Checks

**Current**: Text status
**Enhanced**: Status tree with icons

```rust
let mut root = TreeNode::new("Health Check")
    .icon("🏥")
    .icon_style(theme.header);

// Database check
let db_status = if db_ok { "✓" } else { "✗" };
let db_style = if db_ok { theme.success } else { theme.error };
root.add_child(
    TreeNode::new(&format!("{} Database", db_status))
        .icon_style(db_style)
);

// Git archive check
// ... similar ...

let tree = Tree::new(root).guides(TreeGuides::Rounded);
output.print_renderable(&tree);
```

**Visual**:
```
🏥 Health Check
  ├── ✓ Database (ms.db: 2.3 MB, 847 skills)
  ├── ✓ Git Archive (128 commits, clean)
  ├── ✓ Search Index (last updated: 2m ago)
  ├── ✓ CASS Connection (healthy)
  └── ⚠ CM Integration (playbook empty)
```

#### 3.1.7 `graph.rs` - Dependency Graph

**Current**: Plain text or delegated to bv
**Enhanced**: Visual dependency trees

```rust
// Skill dependency tree
let mut tree = TreeNode::new(&skill_id)
    .icon("📦")
    .icon_style(theme.skill_name);

for dep in dependencies {
    let dep_node = TreeNode::new(&dep.skill_id)
        .icon(if dep.optional { "○" } else { "●" })
        .icon_style(if dep.satisfied { theme.success } else { theme.warning });
    tree.add_child(dep_node);
}

output.print_renderable(&Tree::new(tree));
```

#### 3.1.8 `evidence.rs` - Provenance Display

**Current**: Plain text evidence chain
**Enhanced**: Visual provenance trail

```rust
// Evidence timeline
for evidence in chain {
    let panel = Panel::from_text(&format!(
        "Source: {}\nSession: {}\nExtracted: {}",
        evidence.source_type,
        evidence.session_path,
        evidence.extracted_at
    ))
    .title(&format!("Evidence #{}", evidence.id))
    .subtitle(&evidence.rule_id)
    .border_style(theme.muted);

    output.print_renderable(&panel);
}
```

#### 3.1.9 `security.rs` - ACIP Status

**Current**: Text status
**Enhanced**: Security dashboard

```rust
// Security status panel
let status_table = Table::new()
    .with_column(Column::new("Component"))
    .with_column(Column::new("Status"))
    .with_column(Column::new("Details"));

status_table.add_row_cells([
    "ACIP",
    if acip_enabled { "✓ Enabled" } else { "✗ Disabled" },
    &format!("{} quarantined items", quarantine_count),
]);

status_table.add_row_cells([
    "DCG",
    if dcg_available { "✓ Available" } else { "✗ Not found" },
    &dcg_version,
]);

let panel = Panel::new(status_table.render(60))
    .title("Security Status")
    .border_style(if all_ok { theme.success } else { theme.warning });

output.print_renderable(&panel);
```

#### 3.1.10 `mcp.rs` - MCP Server

**NO CHANGES** - MCP server uses JSON-RPC protocol, not console output.
The MCP server must remain completely untouched.

### 3.2 Progress Indicators (`src/cli/progress.rs`)

**Current**: indicatif-based progress bars
**Enhanced**: rich_rust progress with graceful degradation

```rust
pub struct RichProgress {
    inner: Option<ProgressBar>,
    plain_mode: bool,
    last_message: String,
}

impl RichProgress {
    pub fn new(total: u64, message: &str) -> Self {
        if should_use_rich_output() {
            Self {
                inner: Some(ProgressBar::new()
                    .total(total as f64)
                    .width(40)),
                plain_mode: false,
                last_message: message.to_string(),
            }
        } else {
            eprintln!("{}", message);
            Self {
                inner: None,
                plain_mode: true,
                last_message: message.to_string(),
            }
        }
    }

    pub fn inc(&mut self, delta: u64) {
        if let Some(bar) = &mut self.inner {
            bar.completed += delta as f64;
            // Render to stderr so it doesn't interfere with stdout
            eprint!("\r{}", bar.render_inline());
        }
    }

    pub fn finish(&self, message: &str) {
        if self.plain_mode {
            eprintln!("{}", message);
        } else {
            eprintln!("\r{}", message);
        }
    }
}
```

### 3.3 Error Display (`src/error/`)

**Current**: StructuredError with plain text
**Enhanced**: Styled error panels with suggestions

```rust
impl RichErrorDisplay for MsError {
    fn display_rich(&self, output: &RichOutput) {
        let (icon, style) = match self.severity() {
            Severity::Error => ("✗", output.theme.error),
            Severity::Warning => ("⚠", output.theme.warning),
            Severity::Info => ("ℹ", output.theme.info),
        };

        let mut content = format!("{} {}\n", icon, self.message());

        if let Some(suggestion) = self.suggestion() {
            content.push_str(&format!("\n💡 {}", suggestion));
        }

        if let Some(context) = self.context() {
            content.push_str(&format!("\n\nContext:\n{}", context));
        }

        let panel = Panel::from_text(&content)
            .title(&format!("Error {}", self.code()))
            .border_style(style);

        output.print_renderable(&panel);
    }
}
```

**Visual**:
```
╭─ Error E404 ─────────────────────────────────────────────────╮
│ ✗ Skill not found: rust-error-handling                       │
│                                                              │
│ 💡 Did you mean: rust-errors, error-handling-patterns?       │
│                                                              │
│ Context:                                                     │
│   Searched in: ./skills, ~/.ms/skills                        │
│   Index last updated: 5 minutes ago                          │
╰──────────────────────────────────────────────────────────────╯
```

### 3.4 Output Formatting (`src/cli/output.rs`)

**Current**: OutputFormat enum with formatters
**Enhanced**: Add RichFormatter

```rust
pub enum OutputFormat {
    Human,    // Now uses RichOutput when appropriate
    Json,     // Unchanged - always plain JSON
    Jsonl,    // Unchanged - always plain JSONL
    Plain,    // Unchanged - always plain text
    Tsv,      // Unchanged - always plain TSV
}

pub struct OutputManager {
    format: OutputFormat,
    rich: Option<RichOutput>,
}

impl OutputManager {
    pub fn new(format: OutputFormat, config: &Config) -> Self {
        let rich = if matches!(format, OutputFormat::Human) {
            Some(RichOutput::new(config, &format))
        } else {
            None
        };
        Self { format, rich }
    }

    pub fn print_search_results(&self, results: &[SearchResult]) {
        match &self.rich {
            Some(rich) => rich.print_search_results(results),
            None => self.print_plain_search_results(results),
        }
    }
}
```

### 3.5 CASS Integration (`src/cass/`)

#### 3.5.1 `brenner.rs` - Wizard UI

**Enhanced**: Interactive wizard panels

```rust
// Wizard state display
pub fn display_wizard_state(&self, output: &RichOutput) {
    let state_panel = Panel::from_text(&format!(
        "Stage: {}\nSessions analyzed: {}\nPatterns found: {}\nConfidence: {:.0}%",
        self.stage.name(),
        self.sessions_analyzed,
        self.patterns.len(),
        self.confidence * 100.0
    ))
    .title("Brenner Wizard")
    .subtitle(&format!("Step {}/{}", self.step, self.total_steps));

    output.print_renderable(&state_panel);
}
```

#### 3.5.2 Session Quality Display

```rust
// Quality breakdown table
let mut table = Table::new()
    .title("Session Quality")
    .with_column(Column::new("Signal"))
    .with_column(Column::new("Status"))
    .with_column(Column::new("Weight"));

for signal in &quality.signals {
    let status = if signal.present { "✓" } else { "✗" };
    let style = if signal.present { theme.success } else { theme.muted };
    table.add_row_cells([
        &signal.name,
        status,
        &format!("{:.2}", signal.weight),
    ]);
}

output.print_renderable(&table);
```

### 3.6 Storage Layer (`src/storage/`)

#### Status Messages During Transactions

```rust
impl TxManager {
    pub fn with_progress<F, R>(&self, operation: &str, f: F) -> Result<R>
    where F: FnOnce() -> Result<R>
    {
        if self.rich_output.is_some() {
            self.rich_output.as_ref().unwrap().print(
                &format!("  {} {}", "⋯", operation),
                |console, theme| {
                    console.print(&format!(
                        "  [{}]⋯[/] {}",
                        theme.spinner.to_markup(),
                        operation
                    ));
                }
            );
        }

        let result = f();

        if self.rich_output.is_some() {
            let icon = if result.is_ok() { "✓" } else { "✗" };
            let style = if result.is_ok() { "green" } else { "red" };
            // Update line...
        }

        result
    }
}
```

### 3.7 Application Startup (`src/app.rs`)

**Enhanced**: Styled banner and initialization

```rust
pub fn print_banner(output: &RichOutput) {
    if !output.is_rich() { return; }

    let banner = r#"
    ┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
    ┃  ███╗   ███╗███████╗                          ┃
    ┃  ████╗ ████║██╔════╝   Meta Skill CLI         ┃
    ┃  ██╔████╔██║███████╗   v0.1.0                 ┃
    ┃  ██║╚██╔╝██║╚════██║                          ┃
    ┃  ██║ ╚═╝ ██║███████║   Local-first skills    ┃
    ┃  ╚═╝     ╚═╝╚══════╝                          ┃
    ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛
    "#;

    output.console.print(&format!("[bold cyan]{}[/]", banner));
}
```

---

## 4. Implementation Phases

### Phase 1: Foundation (Core Infrastructure)

**Duration**: ~2-3 days of focused work

1. **Add rich_rust dependency**
   ```toml
   # Cargo.toml
   [dependencies]
   rich_rust = { path = "../rich_rust", features = ["syntax", "json"] }
   ```

2. **Create output abstraction layer**
   - `src/output/mod.rs` - Module root
   - `src/output/rich_output.rs` - RichOutput struct
   - `src/output/theme.rs` - Theme system
   - `src/output/detection.rs` - Capability detection

3. **Integrate with existing OutputFormat**
   - Modify `src/cli/output.rs`
   - Add theme configuration to `src/config.rs`

4. **Add agent-safety guards**
   - Environment variable checks
   - Robot mode detection
   - Terminal detection

**Files to modify**:
- `Cargo.toml`
- `src/lib.rs` (add output module)
- `src/cli/output.rs`
- `src/config.rs`

**New files**:
- `src/output/mod.rs`
- `src/output/rich_output.rs`
- `src/output/theme.rs`
- `src/output/detection.rs`

### Phase 2: Core Commands (High-Impact)

**Duration**: ~3-4 days

1. **search.rs** - Search results table
2. **load.rs** - Skill display with panels and syntax
3. **show.rs** - Skill details panel
4. **list.rs** - Skills list table
5. **suggest.rs** - Suggestion cards

**Priority**: These are the most-used commands and will have the biggest visual impact.

### Phase 3: Progress & Feedback

**Duration**: ~2 days

1. **Progress bars** - All indexing/building operations
2. **Spinners** - Long-running operations
3. **Error display** - Styled error panels
4. **Success messages** - Completion confirmations

**Files to modify**:
- `src/cli/progress.rs`
- `src/error/mod.rs`
- Various command files

### Phase 4: Advanced Commands

**Duration**: ~3 days

1. **build.rs** - CASS mining wizard
2. **doctor.rs** - Health check tree
3. **graph.rs** - Dependency visualization
4. **evidence.rs** - Provenance display
5. **security.rs** - Security dashboard

### Phase 5: Polish & Refinement

**Duration**: ~2 days

1. **Consistency pass** - Ensure all output follows theme
2. **Edge cases** - Handle narrow terminals, no-color, etc.
3. **Performance** - Ensure no slowdown from rich output
4. **Documentation** - Update README with screenshots

---

## 5. Component Specifications

### 5.1 RichOutput API

```rust
pub struct RichOutput {
    console: Option<Console>,
    theme: Theme,
    is_rich: bool,
}

impl RichOutput {
    // Construction
    pub fn new(config: &Config, format: &OutputFormat) -> Self;
    pub fn plain() -> Self;  // Force plain mode

    // Query
    pub fn is_rich(&self) -> bool;
    pub fn theme(&self) -> &Theme;
    pub fn width(&self) -> usize;

    // Basic output
    pub fn print(&self, text: &str);
    pub fn println(&self, text: &str);
    pub fn print_styled(&self, text: &str, style: &str);
    pub fn print_markup(&self, markup: &str);

    // Renderables
    pub fn print_renderable<R: Renderable>(&self, renderable: &R);
    pub fn print_table(&self, table: &Table);
    pub fn print_panel(&self, panel: &Panel);
    pub fn print_tree(&self, tree: &Tree);

    // Semantic output
    pub fn success(&self, message: &str);
    pub fn error(&self, message: &str);
    pub fn warning(&self, message: &str);
    pub fn info(&self, message: &str);
    pub fn hint(&self, message: &str);

    // Structural
    pub fn rule(&self, title: Option<&str>);
    pub fn newline(&self);
    pub fn header(&self, text: &str);
    pub fn subheader(&self, text: &str);

    // Progress (writes to stderr)
    pub fn progress(&self, current: u64, total: u64, message: &str);
    pub fn spinner(&self, message: &str) -> SpinnerHandle;

    // Code/Data
    pub fn code(&self, code: &str, language: &str);
    pub fn json(&self, value: &serde_json::Value);
    pub fn key_value(&self, key: &str, value: &str);
}
```

### 5.2 Theme Configuration

```rust
pub struct Theme {
    pub colors: ThemeColors,
    pub box_style: BoxChars,
    pub tree_guides: TreeGuides,
    pub progress_style: BarStyle,
    pub icons: ThemeIcons,
}

pub struct ThemeIcons {
    pub success: &'static str,  // "✓"
    pub error: &'static str,    // "✗"
    pub warning: &'static str,  // "⚠"
    pub info: &'static str,     // "ℹ"
    pub hint: &'static str,     // "💡"
    pub skill: &'static str,    // "📦"
    pub tag: &'static str,      // "🏷"
    pub folder: &'static str,   // "📁"
    pub file: &'static str,     // "📄"
    pub search: &'static str,   // "🔍"
    pub spinner: Vec<&'static str>, // ["⠋", "⠙", "⠹", ...]
}

impl Theme {
    pub fn default() -> Self;
    pub fn minimal() -> Self;      // ASCII-only, no colors
    pub fn vibrant() -> Self;      // Extra colorful
    pub fn from_config(config: &Config) -> Self;
}
```

### 5.3 Builder Helpers

```rust
/// Convenience builders for common patterns
pub mod builders {
    /// Create a results table with standard columns
    pub fn search_results_table(results: &[SearchResult], theme: &Theme) -> Table;

    /// Create a skill display panel
    pub fn skill_panel(skill: &SkillSpec, theme: &Theme) -> Panel;

    /// Create a status tree
    pub fn status_tree(checks: &[HealthCheck], theme: &Theme) -> Tree;

    /// Create an error panel
    pub fn error_panel(error: &MsError, theme: &Theme) -> Panel;

    /// Create a progress display
    pub fn progress_line(current: u64, total: u64, message: &str, theme: &Theme) -> String;
}
```

---

## 6. Testing Strategy

### 6.1 Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_use_rich_output_robot_mode() {
        let config = Config { robot_mode: true, ..Default::default() };
        assert!(!should_use_rich_output(&config, &OutputFormat::Human));
    }

    #[test]
    fn test_should_use_rich_output_json_format() {
        let config = Config::default();
        assert!(!should_use_rich_output(&config, &OutputFormat::Json));
    }

    #[test]
    fn test_should_use_rich_output_no_color_env() {
        std::env::set_var("NO_COLOR", "1");
        let config = Config::default();
        assert!(!should_use_rich_output(&config, &OutputFormat::Human));
        std::env::remove_var("NO_COLOR");
    }

    #[test]
    fn test_theme_default_colors() {
        let theme = Theme::default();
        assert!(!theme.colors.success.is_null());
        assert!(!theme.colors.error.is_null());
    }
}
```

### 6.2 Integration Tests

```rust
#[test]
fn test_search_output_robot_mode() {
    let output = Command::new("ms")
        .args(["search", "error", "--robot"])
        .output()
        .unwrap();

    // Should be valid JSON with no ANSI codes
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("\x1b["));
    assert!(serde_json::from_str::<Value>(&stdout).is_ok());
}

#[test]
fn test_search_output_no_color() {
    let output = Command::new("ms")
        .args(["search", "error"])
        .env("NO_COLOR", "1")
        .output()
        .unwrap();

    // Should have no ANSI codes
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("\x1b["));
}
```

### 6.3 Visual Regression Tests

Use `insta` for snapshot testing of rendered output:

```rust
#[test]
fn test_search_results_table_snapshot() {
    let results = vec![
        SearchResult { skill_id: "test-skill".into(), score: 0.95, .. },
    ];
    let theme = Theme::default();
    let table = builders::search_results_table(&results, &theme);

    // Capture without ANSI codes for stable snapshots
    let rendered = table.render_plain(80);
    insta::assert_snapshot!(rendered);
}
```

---

## 7. Configuration

### 7.1 Config File Additions

```toml
# .ms/config.toml

[output]
# Force plain output even on TTY (for scripting)
force_plain = false

# Force rich output even when not TTY (for demos)
force_rich = false

[theme]
# Preset: "default", "minimal", "vibrant", "monochrome"
preset = "default"

# Box style: "rounded", "square", "heavy", "double", "ascii"
box_style = "rounded"

# Tree guides: "unicode", "ascii", "bold", "rounded"
tree_guides = "rounded"

# Progress bar style: "block", "ascii", "line", "dots"
progress_style = "block"

# Icon set: "emoji", "nerd", "ascii", "none"
icons = "emoji"

# Custom color overrides
[theme.colors]
success = "bold green"
error = "bold red"
warning = "bold yellow"
skill_name = "bold cyan"
```

### 7.2 Environment Variables

| Variable | Effect |
|----------|--------|
| `NO_COLOR` | Disable all colors (standard) |
| `FORCE_COLOR` | Force colors even when not TTY |
| `MS_PLAIN_OUTPUT` | Force plain text output |
| `MS_FORCE_RICH` | Force rich output |
| `MS_THEME` | Override theme preset |
| `TERM` | Terminal type (for capability detection) |
| `COLORTERM` | Color capability hint |

### 7.3 CLI Flags

```bash
# Global flags
ms --plain search "query"      # Force plain output
ms --color=always list         # Force colors
ms --color=never show skill    # Disable colors
ms --theme=minimal doctor      # Use minimal theme
```

---

## 8. Risk Mitigation

### 8.1 Agent Compatibility Risks

| Risk | Mitigation |
|------|------------|
| ANSI codes in JSON | Robot mode check first, before any output |
| Progress bars to stdout | All progress goes to stderr |
| Breaking existing scripts | Plain text mode preserved exactly |
| MCP protocol corruption | MCP server untouched |
| Piped output issues | is_terminal() check |

### 8.2 Performance Risks

| Risk | Mitigation |
|------|------------|
| Slow terminal detection | Cache result at startup |
| Expensive style rendering | LRU caches in rich_rust |
| Large table rendering | Lazy rendering, pagination |
| Memory from string allocation | Use Cow<str> where possible |

### 8.3 Compatibility Risks

| Risk | Mitigation |
|------|------------|
| Windows console issues | rich_rust handles this |
| Narrow terminals | Graceful width adaptation |
| Missing Unicode fonts | ASCII fallback via safe_box |
| CI/CD environments | NO_COLOR respected |

---

## 9. Success Metrics

### 9.1 Functional Requirements

- [ ] All `--robot` output unchanged
- [ ] All JSON/JSONL output unchanged
- [ ] `NO_COLOR` respected
- [ ] Piped output is plain text
- [ ] MCP server completely unaffected
- [ ] All existing tests pass

### 9.2 Visual Requirements

- [ ] Search results in styled table
- [ ] Skills in bordered panels
- [ ] Syntax highlighting for code
- [ ] Progress bars for long operations
- [ ] Error messages with suggestions
- [ ] Consistent color theme throughout

### 9.3 Performance Requirements

- [ ] No measurable slowdown in robot mode
- [ ] < 10ms overhead for rich output
- [ ] Memory usage increase < 5%

---

## 10. File Change Summary

### New Files

```
src/output/
├── mod.rs              # Module exports
├── rich_output.rs      # Main RichOutput struct
├── theme.rs            # Theme system
├── detection.rs        # Capability detection
└── builders.rs         # Convenience builders
```

### Modified Files

```
Cargo.toml              # Add rich_rust dependency
src/lib.rs              # Add output module
src/config.rs           # Add theme configuration
src/cli/mod.rs          # Integrate RichOutput
src/cli/output.rs       # OutputManager with rich support
src/cli/progress.rs     # Rich progress bars
src/cli/commands/
├── search.rs           # Search results table
├── load.rs             # Skill display panels
├── show.rs             # Skill details
├── list.rs             # Skills list table
├── suggest.rs          # Suggestion cards
├── index.rs            # Indexing progress
├── build.rs            # Building wizard
├── doctor.rs           # Health check tree
├── graph.rs            # Dependency visualization
├── evidence.rs         # Provenance display
├── security.rs         # Security dashboard
└── ...                 # Other commands as needed
src/error/mod.rs        # Styled error display
src/cass/brenner.rs     # Wizard UI
```

### Unchanged Files (Critical)

```
src/cli/commands/mcp.rs         # MCP server - DO NOT TOUCH
src/agent_mail/                 # Agent coordination - unchanged
All JSON serialization code     # Must remain pure JSON
```

---

## Appendix A: Visual Examples

### A.1 Search Results

```
╭──────────────────────────── Search Results ─────────────────────────────╮
│                                                                         │
│  Query: "error handling"                                                │
│  Found: 12 skills (showing top 5)                                       │
│                                                                         │
├─────────┬─────────────────────────┬────────────────┬────────────────────┤
│  Score  │  Skill                  │  Tags          │  Quality           │
├─────────┼─────────────────────────┼────────────────┼────────────────────┤
│   0.95  │  rust-error-handling    │  rust, errors  │  ████████░░  0.82  │
│   0.87  │  error-patterns         │  patterns      │  ███████░░░  0.71  │
│   0.82  │  debugging-strategies   │  debug, test   │  █████████░  0.88  │
│   0.76  │  logging-best-practices │  logging       │  ██████░░░░  0.65  │
│   0.71  │  exception-handling     │  java, errors  │  ████████░░  0.79  │
╰─────────┴─────────────────────────┴────────────────┴────────────────────╯

💡 Tip: Use 'ms load <skill>' to view full content
```

### A.2 Skill Display

```
╭─────────────────────────── rust-error-handling ──────────────── v1.2.0 ─╮
│                                                                         │
│  Best practices for error handling in Rust projects.                    │
│                                                                         │
│  Tags: rust, errors, best-practices, result, option                     │
│  Quality: ████████░░ 0.82                                               │
│  Last updated: 3 days ago                                               │
│                                                                         │
╰─────────────────────────────────────────────────────────────────────────╯

──────────────────────────────── Overview ─────────────────────────────────

Use `Result<T, E>` for recoverable errors and propagate with `?`. Define
custom error types for domain logic. Prefer `thiserror` for libraries and
`anyhow` for applications.

─────────────────────────────── Examples ──────────────────────────────────

  1 │ use thiserror::Error;
  2 │
  3 │ #[derive(Error, Debug)]
  4 │ pub enum ConfigError {
  5 │     #[error("failed to read config: {0}")]
  6 │     Io(#[from] std::io::Error),
  7 │     #[error("failed to parse config: {0}")]
  8 │     Parse(#[from] toml::de::Error),
  9 │ }
 10 │
 11 │ fn read_config(path: &str) -> Result<Config, ConfigError> {
 12 │     let contents = std::fs::read_to_string(path)?;
 13 │     Ok(toml::from_str(&contents)?)
 14 │ }

──────────────────────────────── Rules ────────────────────────────────────

  • Always include context when wrapping errors
  • Use `expect()` only when panic is the correct response
  • Implement `std::error::Error` for custom error types
  • Consider using `#[from]` for automatic conversions
```

### A.3 Doctor Output

```
╭─────────────────────────── Health Check ────────────────────────────────╮
│                                                                         │
│  🏥 Meta Skill Health Report                                            │
│                                                                         │
│  ├── ✓ Database                                                         │
│  │      ms.db: 2.3 MB, 847 skills indexed                              │
│  │      Last vacuum: 2 days ago                                         │
│  │                                                                      │
│  ├── ✓ Git Archive                                                      │
│  │      128 commits, working tree clean                                 │
│  │      Last commit: 3 hours ago                                        │
│  │                                                                      │
│  ├── ✓ Search Index                                                     │
│  │      Tantivy index: 4.1 MB                                          │
│  │      Last reindex: 5 minutes ago                                     │
│  │                                                                      │
│  ├── ✓ CASS Integration                                                 │
│  │      Connection: healthy                                             │
│  │      Sessions indexed: 1,247                                         │
│  │                                                                      │
│  └── ⚠ CM Integration                                                   │
│         Playbook: empty (run 'cm onboard' to populate)                  │
│                                                                         │
│  ─────────────────────────────────────────────────────────────────────  │
│  Overall: 4/5 checks passed                                             │
│                                                                         │
╰─────────────────────────────────────────────────────────────────────────╯
```

### A.4 Error Display

```
╭─ Error E404 ─────────────────────────────────────────────────────────────╮
│                                                                          │
│  ✗ Skill not found: rust-eror-handling                                   │
│                                                                          │
│  💡 Did you mean one of these?                                           │
│     • rust-error-handling (similarity: 0.94)                             │
│     • rust-errors (similarity: 0.71)                                     │
│                                                                          │
│  Context:                                                                │
│    Searched paths:                                                       │
│      • ./skills                                                          │
│      • ~/.ms/skills                                                      │
│    Index contains: 847 skills                                            │
│    Last indexed: 5 minutes ago                                           │
│                                                                          │
│  Try: ms search "rust error" to find related skills                      │
│                                                                          │
╰──────────────────────────────────────────────────────────────────────────╯
```

---

## Appendix B: Implementation Checklist

### Phase 1: Foundation
- [ ] Add rich_rust to Cargo.toml
- [ ] Create src/output/mod.rs
- [ ] Create src/output/rich_output.rs
- [ ] Create src/output/theme.rs
- [ ] Create src/output/detection.rs
- [ ] Add theme config to src/config.rs
- [ ] Write unit tests for detection logic
- [ ] Write integration tests for agent compatibility

### Phase 2: Core Commands
- [ ] Enhance search.rs
- [ ] Enhance load.rs
- [ ] Enhance show.rs
- [ ] Enhance list.rs
- [ ] Enhance suggest.rs
- [ ] Write visual regression tests

### Phase 3: Progress & Feedback
- [ ] Enhance progress.rs
- [ ] Enhance error display
- [ ] Add success/warning messages
- [ ] Test with long operations

### Phase 4: Advanced Commands
- [ ] Enhance build.rs
- [ ] Enhance doctor.rs
- [ ] Enhance graph.rs
- [ ] Enhance evidence.rs
- [ ] Enhance security.rs

### Phase 5: Polish
- [ ] Consistency review
- [ ] Edge case handling
- [ ] Performance testing
- [ ] Documentation update
- [ ] Screenshot gallery

---

*This plan was generated on 2026-01-19 for the meta_skill project.*
