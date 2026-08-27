//! Python Rich conformance tests.
//!
//! Enable with: `cargo test --features conformance_test`
#![allow(unexpected_cfgs)]
#![cfg(feature = "conformance_test")]

use std::fs;

use rich_rust::color::ColorSystem;
use rich_rust::console::{Console, PrintOptions};
use rich_rust::prelude::*;
use rich_rust::renderables::{
    Align, Columns, Padding, Panel, Rule, Table, Traceback, TracebackFrame, Tree, TreeNode,
};
use rich_rust::segment::Segment;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColorSystemMode {
    Auto,
    Fixed(ColorSystem),
    None,
}

#[derive(Debug, Clone)]
struct RenderOptions {
    width: Option<usize>,
    color_system: ColorSystemMode,
    force_terminal: Option<bool>,
}

fn normalize_line_endings(text: &str) -> String {
    text.replace("\r\n", "\n").replace("\r", "\n")
}

fn normalize_hyperlink_ids(text: &str) -> String {
    let re = regex::Regex::new(r"\x1b]8;id=[^;]*;").expect("regex");
    re.replace_all(text, "\x1b]8;;").to_string()
}

fn normalize_ansi(text: &str) -> String {
    normalize_line_endings(&normalize_hyperlink_ids(text))
}

fn parse_color_system_mode(value: &str) -> ColorSystemMode {
    match value {
        "auto" => ColorSystemMode::Auto,
        "truecolor" => ColorSystemMode::Fixed(ColorSystem::TrueColor),
        "256" | "eight_bit" => ColorSystemMode::Fixed(ColorSystem::EightBit),
        "standard" => ColorSystemMode::Fixed(ColorSystem::Standard),
        "none" | "" => ColorSystemMode::None,
        _ => ColorSystemMode::None,
    }
}

fn parse_render_options(defaults: &Value, overrides: Option<&Value>) -> RenderOptions {
    let default_width = defaults
        .get("width")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);
    let default_color = defaults
        .get("color_system")
        .and_then(|v| v.as_str())
        .map(parse_color_system_mode)
        .unwrap_or(ColorSystemMode::Auto);
    let default_force = defaults.get("force_terminal").and_then(|v| v.as_bool());

    let mut width = default_width;
    let mut color_system = default_color;
    let mut force_terminal = default_force;

    if let Some(overrides) = overrides {
        if let Some(w) = overrides.get("width").and_then(|v| v.as_u64()) {
            width = Some(w as usize);
        }
        if let Some(cs_value) = overrides.get("color_system") {
            color_system = cs_value
                .as_str()
                .map(parse_color_system_mode)
                .unwrap_or(ColorSystemMode::None);
        }
        if let Some(force_value) = overrides.get("force_terminal") {
            force_terminal = force_value.as_bool();
        }
    }

    RenderOptions {
        width,
        color_system,
        force_terminal,
    }
}

fn build_console(case: &Value, options: &RenderOptions, theme: Option<Theme>) -> Console {
    let mut builder = Console::builder();
    if let Some(width) = options.width {
        builder = builder.width(width);
    }
    if let Some(force_terminal) = options.force_terminal {
        builder = builder.force_terminal(force_terminal);
    }
    match options.color_system {
        ColorSystemMode::Auto => {
            let is_tty = match options.force_terminal {
                Some(value) => value,
                None => force_color_forces_terminal(env_override(case, "FORCE_COLOR").as_deref()),
            };
            match detect_color_system_for_case(case, is_tty) {
                Some(color_system) => builder = builder.color_system(color_system),
                None => builder = builder.no_color(),
            }
        }
        ColorSystemMode::Fixed(color_system) => builder = builder.color_system(color_system),
        ColorSystemMode::None => builder = builder.no_color(),
    };
    if let Some(theme) = theme {
        builder = builder.theme(theme);
    }
    builder.build()
}

fn parse_theme(case: &Value) -> Option<Theme> {
    let config = case.get("theme")?.as_object()?;
    let inherit = config
        .get("inherit")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let styles = config.get("styles")?.as_object()?;

    let mut entries: Vec<(String, String)> = Vec::with_capacity(styles.len());
    for (name, definition) in styles {
        entries.push((name.clone(), definition.as_str()?.to_string()));
    }

    Theme::from_style_definitions(entries, inherit).ok()
}

fn env_value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn env_override(case: &Value, key: &str) -> Option<String> {
    case.get("env")
        .and_then(|value| value.as_object())
        .and_then(|env| env.get(key))
        .and_then(env_value_to_string)
}

fn force_color_forces_terminal(force_color: Option<&str>) -> bool {
    let Some(force_color) = force_color else {
        return false;
    };
    let force_color = force_color.trim();
    !force_color.is_empty() && force_color != "0"
}

fn detect_color_system_for_case(case: &Value, is_tty: bool) -> Option<ColorSystem> {
    // Match `src/terminal.rs` behavior for determinism in conformance tests.
    if env_override(case, "NO_COLOR")
        .as_deref()
        .is_some_and(|value| !value.is_empty())
    {
        return None;
    }

    if let Some(colorterm) = env_override(case, "COLORTERM") {
        let colorterm = colorterm.trim().to_lowercase();
        if colorterm == "truecolor" || colorterm == "24bit" {
            return Some(ColorSystem::TrueColor);
        }
    }

    let term = env_override(case, "TERM")
        .map(|value| value.trim().to_lowercase())
        .unwrap_or_default();
    if term == "dumb" || term == "unknown" {
        return None;
    }

    let colors = term.rsplit('-').next().unwrap_or("");
    match colors {
        "kitty" | "256color" => return Some(ColorSystem::EightBit),
        "16color" => return Some(ColorSystem::Standard),
        _ => {}
    }

    if is_tty {
        Some(ColorSystem::Standard)
    } else {
        None
    }
}

fn render_text(console: &Console, markup: &str, width: Option<usize>) -> (String, String) {
    let mut options = PrintOptions::new().with_markup(true);
    if let Some(width) = width {
        options = options.with_width(width);
    }

    let plain = console.export_text_with_options(markup, &options);
    let mut buf = Vec::new();
    console
        .print_to(&mut buf, markup, &options)
        .expect("print_to failed");

    (
        normalize_line_endings(&plain),
        normalize_ansi(&String::from_utf8(buf).expect("utf8 output")),
    )
}

fn render_protocol_rich_cast(
    console: &Console,
    markup: &str,
    width: Option<usize>,
) -> (String, String) {
    #[derive(Debug)]
    struct ProtocolCast {
        markup: String,
    }

    impl RichCast for ProtocolCast {
        fn rich_cast(&self) -> rich_rust::protocol::RichCastOutput {
            rich_rust::protocol::RichCastOutput::Str(self.markup.clone())
        }
    }

    let mut options = PrintOptions::new().with_markup(true);
    if let Some(width) = width {
        options = options.with_width(width);
    }

    let value = ProtocolCast {
        markup: markup.to_string(),
    };
    let plain = console.export_cast_text_with_options(&value, &options);

    let mut buf = Vec::new();
    console
        .print_cast_to(&mut buf, &value, &options)
        .expect("print_cast_to failed");

    (
        normalize_line_endings(&plain),
        normalize_ansi(&String::from_utf8(buf).expect("utf8 output")),
    )
}

fn render_protocol_measure(
    console: &Console,
    minimum: usize,
    maximum: usize,
    width: Option<usize>,
) -> (String, String) {
    struct ProtocolMeasure {
        minimum: usize,
        maximum: usize,
    }

    impl rich_rust::measure::RichMeasure for ProtocolMeasure {
        fn rich_measure(
            &self,
            _console: &Console,
            _options: &rich_rust::console::ConsoleOptions,
        ) -> Measurement {
            Measurement::new(self.minimum, self.maximum)
        }
    }

    let measurement = console.measure(&ProtocolMeasure { minimum, maximum }, None);
    let display = format!("{}:{}", measurement.minimum, measurement.maximum);
    render_text(console, &display, width)
}

fn render_segments_to_ansi(console: &Console, segments: &[Segment<'_>]) -> String {
    let mut buf = Vec::new();
    console
        .print_segments_to(&mut buf, segments)
        .expect("print_segments_to failed");
    normalize_ansi(&String::from_utf8(buf).expect("utf8 output"))
}

fn render_renderable(
    console: &Console,
    renderable: &dyn rich_rust::renderables::Renderable,
) -> (String, String) {
    let options = console.options();
    let segments = renderable.render(console, &options);
    let plain: String = segments
        .iter()
        .filter(|segment| !segment.is_control())
        .map(|segment| segment.text.as_ref())
        .collect();
    let ansi = render_segments_to_ansi(console, &segments);
    (normalize_line_endings(&plain), ansi)
}

fn render_prepared_text(console: &Console, text: &Text) -> (String, String) {
    let segments = text.render(&text.end);
    let plain: String = segments
        .iter()
        .filter(|segment| !segment.is_control())
        .map(|segment| segment.text.as_ref())
        .collect();
    let ansi = render_segments_to_ansi(console, &segments);
    (normalize_line_endings(&plain), ansi)
}

fn value_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn value_bool(value: &Value, key: &str, default: bool) -> bool {
    value.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
}

fn value_usize(value: &Value, key: &str) -> Option<usize> {
    value.get(key).and_then(|v| v.as_u64()).map(|v| v as usize)
}

fn value_i32(value: &Value, key: &str, default: i32) -> i32 {
    value
        .get(key)
        .and_then(|v| v.as_i64())
        .and_then(|v| i32::try_from(v).ok())
        .unwrap_or(default)
}

fn build_table(input: &Value) -> Table {
    let show_header = value_bool(input, "show_header", true);
    let show_lines = value_bool(input, "show_lines", false);
    let mut table = Table::new().show_header(show_header).show_lines(show_lines);

    if let Some(title) = value_string(input, "title") {
        table = table.title(title);
    }
    if let Some(caption) = value_string(input, "caption") {
        table = table.caption(caption);
    }

    let columns = input
        .get("columns")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let justifies = input
        .get("column_justifies")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    for (idx, col) in columns.iter().enumerate() {
        let name = col.as_str().unwrap_or("");
        let mut column = Column::new(name);
        if let Some(justify) = justifies
            .get(idx)
            .and_then(|v| v.as_str())
            .map(|s| s.to_lowercase())
        {
            column = match justify.as_str() {
                "right" => column.justify(JustifyMethod::Right),
                "center" => column.justify(JustifyMethod::Center),
                _ => column.justify(JustifyMethod::Left),
            };
        }
        table.add_column(column);
    }

    if let Some(rows) = input.get("rows").and_then(|v| v.as_array()) {
        for row in rows {
            if let Some(cells) = row.as_array() {
                let row_cells: Vec<Cell> = cells
                    .iter()
                    .map(|cell| Cell::new(cell.as_str().unwrap_or("")))
                    .collect();
                table.add_row(Row::new(row_cells));
            }
        }
    }

    table
}

fn build_tree_node(node: &Value) -> TreeNode {
    let label = node.get("label").and_then(|v| v.as_str()).unwrap_or("");
    let mut tree = TreeNode::new(label);
    if let Some(children) = node.get("children").and_then(|v| v.as_array()) {
        for child in children {
            tree = tree.child(build_tree_node(child));
        }
    }
    tree
}

fn build_renderable(
    kind: &str,
    input: &Value,
    options: &RenderOptions,
) -> Box<dyn rich_rust::renderables::Renderable + 'static> {
    match kind {
        "rule" => {
            let title = value_string(input, "title");
            let align = value_string(input, "align").unwrap_or_else(|| "center".to_string());
            let character = value_string(input, "character").unwrap_or_else(|| "─".to_string());
            let mut rule = if let Some(title) = title {
                Rule::with_title(title)
            } else {
                Rule::new()
            };
            rule = rule.character(character);
            rule = match align.as_str() {
                "left" => rule.align_left(),
                "right" => rule.align_right(),
                _ => rule.align_center(),
            };
            Box::new(rule)
        }
        "panel" => {
            let text = value_string(input, "text").unwrap_or_default();
            let content_lines: Vec<Vec<Segment<'static>>> = text
                .lines()
                .map(|line| vec![Segment::new(line.to_string(), None)])
                .collect();
            let mut panel = Panel::new(content_lines);
            if let Some(title) = value_string(input, "title") {
                panel = panel.title(title);
            }
            if let Some(subtitle) = value_string(input, "subtitle") {
                panel = panel.subtitle(subtitle);
            }
            if let Some(width) = value_usize(input, "width") {
                panel = panel.width(width);
            }
            if let Some(box_style) = value_string(input, "box") {
                panel = match box_style.as_str() {
                    "ASCII" => panel.ascii(),
                    "SQUARE" | "DOUBLE" => panel.square(), // DOUBLE uses square as fallback
                    _ => panel.rounded(),
                };
            }
            Box::new(panel)
        }
        "table" => Box::new(build_table(input)),
        "tree" => {
            let node = build_tree_node(input);
            Box::new(Tree::new(node))
        }
        "progress" => {
            let total = input.get("total").and_then(|v| v.as_u64()).unwrap_or(100);
            let completed = input.get("completed").and_then(|v| v.as_u64()).unwrap_or(0);
            let width = value_usize(input, "width").unwrap_or(0);
            let ratio = if total == 0 {
                0.0
            } else {
                (completed as f64) / (total as f64)
            };
            let mut bar = ProgressBar::new()
                .width(width)
                .bar_style(BarStyle::Line)
                .show_brackets(false)
                .show_percentage(false)
                .completed_style(Style::new().color(Color::from_rgb(249, 38, 114)))
                .remaining_style(Style::new().color(Color::from_ansi(237)))
                .pulse_style(Style::new().color(Color::from_ansi(237)));
            bar.set_progress(ratio);
            Box::new(bar)
        }
        "columns" => {
            let items = input
                .get("items")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .map(|v| v.as_str().unwrap_or("").to_string())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let segments: Vec<Vec<Segment<'static>>> = items
                .iter()
                .map(|item| vec![Segment::new(item.to_string(), None)])
                .collect();
            Box::new(Columns::new(segments).expand(false).gutter(1).padding(0))
        }
        "padding" => {
            let text = value_string(input, "text").unwrap_or_default();
            let pad = input
                .get("pad")
                .and_then(|v| v.as_array())
                .map(|values| {
                    let mut nums = [0usize; 4];
                    for (idx, value) in values.iter().enumerate().take(4) {
                        nums[idx] = value.as_u64().unwrap_or(0) as usize;
                    }
                    nums
                })
                .unwrap_or([0, 0, 0, 0]);
            let width = value_usize(input, "width").or(options.width).unwrap_or(0);
            let text = Text::new(text);
            let content: Vec<Vec<Segment<'static>>> = vec![
                text.render("")
                    .into_iter()
                    .map(Segment::into_owned)
                    .collect(),
            ];
            Box::new(Padding::new(content, pad, width))
        }
        "align" => {
            let text = value_string(input, "text").unwrap_or_default();
            let width = options.width.or(value_usize(input, "width")).unwrap_or(0);
            let align = value_string(input, "align").unwrap_or_else(|| "left".to_string());
            let content = vec![Segment::new(text, None)];
            let align = match align.as_str() {
                "center" => Align::new(content, width).center(),
                "right" => Align::new(content, width).right(),
                _ => Align::new(content, width).left(),
            };
            Box::new(align)
        }
        "constrain" => {
            let child_kind =
                value_string(input, "child_kind").unwrap_or_else(|| "rule".to_string());
            let child_input = input.get("child_input").unwrap_or(&Value::Null);
            let width = input
                .get("width")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);

            let child = build_renderable(&child_kind, child_input, options);
            Box::new(rich_rust::renderables::Constrain::new_boxed(child, width))
        }
        "control" => {
            let operation = value_string(input, "operation").unwrap_or_else(|| "clear".to_string());
            let control = match operation.as_str() {
                "bell" => Control::bell(),
                "home" => Control::home(),
                "move" => Control::r#move(value_i32(input, "x", 0), value_i32(input, "y", 0)),
                "move_to_column" => {
                    Control::move_to_column(value_i32(input, "x", 0), value_i32(input, "y", 0))
                }
                "move_to" => Control::move_to(value_i32(input, "x", 0), value_i32(input, "y", 0)),
                "clear" => Control::clear(),
                "show_cursor" => {
                    let show = input.get("show").and_then(|v| v.as_bool()).unwrap_or(true);
                    Control::show_cursor(show)
                }
                "alt_screen" => {
                    let enable = input
                        .get("enable")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true);
                    Control::alt_screen(enable)
                }
                "title" => Control::title(value_string(input, "title").unwrap_or_default()),
                other => {
                    panic!("unsupported control operation: {other}");
                }
            };
            Box::new(control)
        }
        "markdown" => {
            #[cfg(feature = "markdown")]
            {
                let source = value_string(input, "text").unwrap_or_default();
                let mut md = rich_rust::renderables::Markdown::new(source);
                if let Some(hyperlinks) = input.get("hyperlinks").and_then(|v| v.as_bool()) {
                    md = md.hyperlinks(hyperlinks);
                }
                Box::new(md)
            }
            #[cfg(not(feature = "markdown"))]
            {
                assert!(false, "markdown conformance requires the markdown feature");
                Box::new(Text::new(""))
            }
        }
        "json" => {
            #[cfg(feature = "json")]
            {
                let source = value_string(input, "json").unwrap_or_else(|| "{}".to_string());
                let mut json = rich_rust::renderables::Json::from_str(&source)
                    .unwrap_or_else(|_| rich_rust::renderables::Json::new(serde_json::Value::Null));
                if let Some(sort_keys) = input.get("sort_keys").and_then(|v| v.as_bool()) {
                    json = json.sort_keys(sort_keys);
                }
                if let Some(ensure_ascii) = input.get("ensure_ascii").and_then(|v| v.as_bool()) {
                    json = json.ensure_ascii(ensure_ascii);
                }
                if let Some(highlight) = input.get("highlight").and_then(|v| v.as_bool()) {
                    json = json.highlight(highlight);
                }
                if let Some(indent_value) = input.get("indent") {
                    if indent_value.is_null() {
                        json = json.compact();
                    } else if let Some(spaces) = indent_value.as_u64() {
                        json = json.indent(spaces as usize);
                    } else if let Some(unit) = indent_value.as_str() {
                        json = json.indent_str(unit);
                    }
                }
                Box::new(json)
            }
            #[cfg(not(feature = "json"))]
            {
                assert!(false, "json conformance requires the json feature");
                Box::new(Text::new(""))
            }
        }
        "syntax" => {
            #[cfg(feature = "syntax")]
            {
                let code = value_string(input, "code").unwrap_or_default();
                let language =
                    value_string(input, "language").unwrap_or_else(|| "rust".to_string());
                let syntax = rich_rust::renderables::Syntax::new(code, language);
                Box::new(syntax)
            }
            #[cfg(not(feature = "syntax"))]
            {
                assert!(false, "syntax conformance requires the syntax feature");
                Box::new(Text::new(""))
            }
        }
        "traceback" => {
            let mut frames: Vec<TracebackFrame> = Vec::new();
            if let Some(frame_values) = input.get("frames").and_then(|v| v.as_array()) {
                for frame in frame_values {
                    let name = frame.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let line = frame.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    if !name.is_empty() && line > 0 {
                        let mut tf = TracebackFrame::new(name, line);
                        if let Some(filename) = frame.get("filename").and_then(|v| v.as_str()) {
                            tf = tf.filename(filename);
                        }
                        if let Some(source) = frame.get("source_context").and_then(|v| v.as_str()) {
                            let first_line = frame
                                .get("source_first_line")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(1) as usize;
                            tf = tf.source_context(source, first_line);
                        }
                        frames.push(tf);
                    }
                }
            }

            let exception_type =
                value_string(input, "exception_type").unwrap_or_else(|| "Error".to_string());
            let exception_message = value_string(input, "exception_message").unwrap_or_default();
            let extra_lines = input
                .get("extra_lines")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            Box::new(
                Traceback::new(frames, exception_type, exception_message).extra_lines(extra_lines),
            )
        }
        other => {
            panic!("unsupported kind: {other}");
        }
    }
}

#[test]
fn python_rich_fixtures() {
    let fixture_path = "tests/conformance/fixtures/python_rich.json";
    let raw = fs::read_to_string(fixture_path).expect("missing python rich conformance fixtures");
    let data: Value = serde_json::from_str(&raw).expect("invalid fixture JSON");

    let defaults = data.get("defaults").expect("defaults missing");
    let cases = data
        .get("cases")
        .and_then(|v| v.as_array())
        .expect("cases missing");

    for case in cases {
        let id = case
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("<unknown>");
        let kind = case
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("<unknown>");
        let input = case.get("input").expect("input missing");
        let expected = case.get("expected").expect("expected missing");
        let expected_plain = expected.get("plain").and_then(|v| v.as_str()).unwrap_or("");
        let expected_ansi = expected.get("ansi").and_then(|v| v.as_str()).unwrap_or("");
        let compare_ansi = case
            .get("compare_ansi")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let options = parse_render_options(defaults, case.get("render_options"));
        let theme = parse_theme(case);
        let console = build_console(case, &options, theme);

        let (mut actual_plain, mut actual_ansi) = if kind == "text" {
            let markup = input.get("markup").and_then(|v| v.as_str()).unwrap_or("");
            render_text(&console, markup, options.width)
        } else if kind == "text_from_ansi" {
            let ansi_text = input.get("ansi").and_then(|v| v.as_str()).unwrap_or("");
            let text = Text::from_ansi(ansi_text);
            render_prepared_text(&console, &text)
        } else if kind == "protocol_rich_cast" {
            let markup = input.get("markup").and_then(|v| v.as_str()).unwrap_or("");
            render_protocol_rich_cast(&console, markup, options.width)
        } else if kind == "protocol_measure" {
            let minimum = value_usize(input, "minimum").unwrap_or(0);
            let maximum = value_usize(input, "maximum").unwrap_or(0);
            render_protocol_measure(&console, minimum, maximum, options.width)
        } else {
            let renderable = build_renderable(kind, input, &options);
            render_renderable(&console, &*renderable)
        };
        if kind == "progress" {
            actual_plain = actual_plain.trim_end_matches('\n').to_string();
            actual_ansi = actual_ansi.trim_end_matches('\n').to_string();
        }
        if (kind == "columns"
            || kind == "padding"
            || kind == "align"
            || kind == "markdown"
            || kind == "json"
            || kind == "syntax")
            && !actual_plain.ends_with('\n')
        {
            actual_plain.push('\n');
            actual_ansi.push('\n');
        }

        assert_eq!(
            actual_plain, expected_plain,
            "plain mismatch for case {id} ({kind})"
        );
        if compare_ansi {
            assert_eq!(
                actual_ansi,
                normalize_ansi(expected_ansi),
                "ansi mismatch for case {id} ({kind})"
            );
        }
    }
}
