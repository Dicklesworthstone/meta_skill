use std::path::PathBuf;

#[path = "demo_showcase/console_builder.rs"]
mod console_builder;
#[path = "demo_showcase/dashboard_scene.rs"]
mod dashboard_scene;
#[path = "demo_showcase/debug_tools.rs"]
mod debug_tools;
#[path = "demo_showcase/emoji_links_scene.rs"]
mod emoji_links_scene;
#[path = "demo_showcase/export_scene.rs"]
mod export_scene;
#[path = "demo_showcase/hero.rs"]
mod hero;
#[path = "demo_showcase/json_scene.rs"]
mod json_scene;
#[path = "demo_showcase/keys.rs"]
mod keys;
#[path = "demo_showcase/layout_scene.rs"]
mod layout_scene;
#[path = "demo_showcase/log_pane.rs"]
mod log_pane;
#[path = "demo_showcase/markdown_scene.rs"]
mod markdown_scene;
#[path = "demo_showcase/outro_scene.rs"]
mod outro_scene;
#[path = "demo_showcase/pager.rs"]
mod pager;
#[path = "demo_showcase/panel_scene.rs"]
mod panel_scene;
#[path = "demo_showcase/scenes.rs"]
mod scenes;
#[path = "demo_showcase/simulation.rs"]
mod simulation;
#[path = "demo_showcase/state.rs"]
mod state;
#[path = "demo_showcase/syntax_scene.rs"]
mod syntax_scene;
#[path = "demo_showcase/table_scene.rs"]
mod table_scene;
#[path = "demo_showcase/theme.rs"]
mod theme;
#[path = "demo_showcase/timing.rs"]
mod timing;
#[path = "demo_showcase/traceback_scene.rs"]
mod traceback_scene;
#[path = "demo_showcase/tracing_scene.rs"]
mod tracing_scene;
#[path = "demo_showcase/tree_scene.rs"]
mod tree_scene;
#[path = "demo_showcase/typography.rs"]
mod typography;
#[path = "demo_showcase/wizard.rs"]
mod wizard;

/// Standalone rich_rust showcase binary (roadmap).
///
/// This file intentionally avoids heavy CLI dependencies (e.g. clap) and uses a
/// small hand-rolled parser per `bd-1o8x`.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cfg = match parse_args(args) {
        Ok(cfg) => cfg,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };

    if cfg.help {
        print!("{HELP_TEXT}");
        return;
    }

    if cfg.list_scenes {
        let demo_console = console_builder::build_demo_console(&cfg);
        scenes::print_scene_list(&demo_console.console);
        return;
    }

    // Build console early so logger can use it
    let demo_console = console_builder::build_demo_console(&cfg);

    // Initialize RichLogger if log level is not Off
    if cfg.log_level != LogLevel::Off {
        init_logger(&demo_console.console, cfg.log_level);
    }

    // Run interactive wizard if no scene specified and interactive is allowed
    let mut cfg = cfg;
    if cfg.scene.is_none() && !cfg.is_export() && cfg.is_interactive_allowed() {
        let registry = scenes::build_registry();
        let scene_names: Vec<_> = registry.all().map(|s| s.name()).collect();

        if let Some(choices) = wizard::run_wizard(
            &demo_console.console,
            cfg.is_interactive_allowed(),
            &scene_names,
        ) {
            // Apply wizard choices to config
            if let Some(scene) = choices.scene {
                cfg.scene = Some(scene);
            }
            if choices.quick {
                cfg.quick = true;
            }
            if choices.export {
                cfg.export = ExportMode::TempDir;
            }
        }
    }

    if let Some(scene_name) = cfg.scene.as_deref() {
        let registry = scenes::build_registry();
        if let Some(scene) = registry.get(scene_name) {
            // Start capture mode if exporting
            if cfg.is_export() {
                demo_console.console.begin_capture();
            }

            if let Err(err) = scene.run(&demo_console.console, &cfg) {
                eprintln!("Scene '{scene_name}' failed: {err}");
                std::process::exit(1);
            }

            // Handle export for single scene if requested
            if cfg.is_export() {
                write_export_files(&cfg, &demo_console.console);
            }
        } else {
            // Defensive: parse_args already validates scene names, but keep a clear error here too.
            let err = scenes::SceneError::Failed(format!("Unknown scene: {scene_name}"));
            eprintln!("{err}");
            std::process::exit(2);
        }
        return;
    }

    // Full demo run: execute all scenes in storyboard order
    // If export mode is enabled, capture output and write files
    if cfg.is_export() {
        run_export_with_console(&cfg, &demo_console);
    } else {
        run_full_demo_with_console(&cfg, &demo_console);
    }
}

/// Initialize RichLogger with the given console and log level.
fn init_logger(console: &std::sync::Arc<rich_rust::console::Console>, level: LogLevel) {
    use rich_rust::logging::RichLogger;

    let logger = RichLogger::new(console.clone())
        .level(level.to_level_filter())
        .markup(true)
        .show_path(false); // Cleaner output for demo

    if let Err(err) = logger.init() {
        eprintln!("Warning: Failed to initialize logger: {err}");
    }
}

/// Run the full demo, executing all scenes in order.
fn run_full_demo_with_console(cfg: &Config, demo_console: &console_builder::DemoConsole) {
    let console = &demo_console.console;
    let registry = scenes::build_registry();

    // Print opening
    console.print("");
    typography::scene_header(console, "Nebula Deploy", Some("rich_rust showcase"));
    typography::hint(console, "Running all scenes in storyboard order...");
    console.print("");

    let mut failed_scenes: Vec<(&str, String)> = Vec::new();
    let mut scene_count = 0;

    for scene in registry.all() {
        scene_count += 1;
        log::debug!("Starting scene: {}", scene.name());

        // Section framing between scenes
        typography::section_header(console, scene.name(), false);

        // Run the scene
        if let Err(err) = scene.run(&demo_console.console, cfg) {
            // Record failure but continue to next scene
            failed_scenes.push((scene.name(), err.to_string()));
            console.print(&format!(
                "[status.err]Scene '{}' failed:[/] {}",
                scene.name(),
                err
            ));
        }

        // Small spacing between scenes
        console.print("");
    }

    // Print summary
    typography::print_divider(console);
    console.print("");

    if failed_scenes.is_empty() {
        console.print(&format!(
            "[status.ok]All {} scenes completed successfully.[/]",
            scene_count
        ));
    } else {
        console.print(&format!(
            "[status.warn]{} of {} scenes completed with errors:[/]",
            failed_scenes.len(),
            scene_count
        ));
        for (name, _err) in &failed_scenes {
            console.print(&format!("  [dim]-[/] {name}"));
        }
    }

    console.print("");
    typography::hint(
        console,
        "Run with --scene <name> to run individual scenes, or --list-scenes to see all options.",
    );

    // Exit with error code if any scene failed
    if !failed_scenes.is_empty() {
        std::process::exit(1);
    }
}

/// Run the demo in export mode, capturing output to HTML and SVG files.
fn run_export_with_console(cfg: &Config, demo_console: &console_builder::DemoConsole) {
    use std::fs;
    use std::io::Write;

    let export_dir = cfg
        .export_dir()
        .expect("export_dir should be Some in export mode");

    // Create export directory
    if let Err(err) = fs::create_dir_all(&export_dir) {
        eprintln!("Failed to create export directory: {err}");
        std::process::exit(1);
    }

    let console = &demo_console.console;

    // Enable capture mode
    console.begin_capture();

    // Run all scenes (without the interactive hint at the end)
    let registry = scenes::build_registry();

    console.print("");
    typography::scene_header(console, "Nebula Deploy", Some("rich_rust showcase"));
    console.print("");

    for scene in registry.all() {
        typography::section_header(console, scene.name(), false);
        if let Err(err) = scene.run(console, cfg) {
            console.print(&format!(
                "[status.err]Scene '{}' failed:[/] {}",
                scene.name(),
                err
            ));
        }
        console.print("");
    }

    typography::print_divider(console);
    console.print("");

    // Export HTML (don't clear buffer yet - we need it for SVG too)
    let html_path = export_dir.join("demo_showcase.html");
    let html_content = console.export_html(false);
    match fs::File::create(&html_path).and_then(|mut f| f.write_all(html_content.as_bytes())) {
        Ok(()) => eprintln!("Exported HTML: {}", html_path.display()),
        Err(err) => eprintln!("Failed to write HTML: {err}"),
    }

    // Export SVG (clear buffer after this)
    let svg_path = export_dir.join("demo_showcase.svg");
    let svg_content = console.export_svg(true);
    match fs::File::create(&svg_path).and_then(|mut f| f.write_all(svg_content.as_bytes())) {
        Ok(()) => eprintln!("Exported SVG: {}", svg_path.display()),
        Err(err) => eprintln!("Failed to write SVG: {err}"),
    }

    eprintln!("\nExport complete: {}", export_dir.display());
}

/// Write export files from already-captured console output.
/// Used for single-scene export where begin_capture was called before the scene.
fn write_export_files(cfg: &Config, console: &std::sync::Arc<rich_rust::console::Console>) {
    use std::fs;
    use std::io::Write;

    let export_dir = cfg
        .export_dir()
        .expect("export_dir should be Some in export mode");

    // Create export directory
    if let Err(err) = fs::create_dir_all(&export_dir) {
        eprintln!("Failed to create export directory: {err}");
        std::process::exit(1);
    }

    // Export HTML (don't clear buffer yet - we need it for SVG too)
    let html_path = export_dir.join("demo_showcase.html");
    let html_content = console.export_html(false);
    match fs::File::create(&html_path).and_then(|mut f| f.write_all(html_content.as_bytes())) {
        Ok(()) => eprintln!("Exported HTML: {}", html_path.display()),
        Err(err) => eprintln!("Failed to write HTML: {err}"),
    }

    // Export SVG (clear buffer after this)
    let svg_path = export_dir.join("demo_showcase.svg");
    let svg_content = console.export_svg(true);
    match fs::File::create(&svg_path).and_then(|mut f| f.write_all(svg_content.as_bytes())) {
        Ok(()) => eprintln!("Exported SVG: {}", svg_path.display()),
        Err(err) => eprintln!("Failed to write SVG: {err}"),
    }

    eprintln!("\nExport complete: {}", export_dir.display());
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum ColorMode {
    #[default]
    Auto,
    None,
    Standard,
    EightBit,
    TrueColor,
}

impl ColorMode {
    fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "none" | "no" | "off" => Ok(Self::None),
            "standard" | "16" => Ok(Self::Standard),
            "eight_bit" | "eightbit" | "256" => Ok(Self::EightBit),
            "truecolor" | "true" | "24bit" => Ok(Self::TrueColor),
            _ => Err(format!(
                "Invalid --color-system value `{value}` (expected: auto|none|standard|eight_bit|truecolor)."
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
enum ExportMode {
    #[default]
    Off,
    TempDir,
    Dir(PathBuf),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum LogLevel {
    #[default]
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "off" | "none" => Ok(Self::Off),
            "error" => Ok(Self::Error),
            "warn" | "warning" => Ok(Self::Warn),
            "info" => Ok(Self::Info),
            "debug" => Ok(Self::Debug),
            "trace" => Ok(Self::Trace),
            _ => Err(format!(
                "Invalid --log-level value `{value}` (expected: off|error|warn|info|debug|trace)."
            )),
        }
    }

    fn to_level_filter(self) -> log::LevelFilter {
        match self {
            Self::Off => log::LevelFilter::Off,
            Self::Error => log::LevelFilter::Error,
            Self::Warn => log::LevelFilter::Warn,
            Self::Info => log::LevelFilter::Info,
            Self::Debug => log::LevelFilter::Debug,
            Self::Trace => log::LevelFilter::Trace,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct Config {
    help: bool,
    list_scenes: bool,
    scene: Option<String>,
    seed: u64,

    quick: bool,
    speed: f64,

    interactive: Option<bool>,
    live: Option<bool>,
    screen: Option<bool>,

    force_terminal: bool,
    width: Option<usize>,
    height: Option<usize>,
    color_system: ColorMode,
    emoji: Option<bool>,
    safe_box: Option<bool>,
    links: Option<bool>,

    log_level: LogLevel,

    export: ExportMode,
}

impl Config {
    fn with_defaults() -> Self {
        Self {
            speed: 1.0,
            ..Self::default()
        }
    }

    /// Check if export mode is enabled.
    fn is_export(&self) -> bool {
        !matches!(self.export, ExportMode::Off)
    }

    /// Check if interactive features (prompts, pager) are allowed.
    ///
    /// Returns `false` if `--no-interactive` was specified, otherwise `true`.
    /// Note: This only checks the CLI flag; actual interactivity also depends
    /// on whether the console is a TTY (checked separately by `Pager`).
    fn is_interactive_allowed(&self) -> bool {
        self.interactive.unwrap_or(true)
    }

    /// Check if ASCII-safe box characters should be used.
    ///
    /// Returns `true` if `--safe-box` was specified, `false` otherwise.
    fn is_safe_box(&self) -> bool {
        self.safe_box.unwrap_or(false)
    }

    /// Get the export directory, creating a temp dir if needed.
    fn export_dir(&self) -> Option<PathBuf> {
        match &self.export {
            ExportMode::Off => None,
            ExportMode::TempDir => {
                // Create a temp directory for export
                let temp = std::env::temp_dir().join("demo_showcase_export");
                Some(temp)
            }
            ExportMode::Dir(path) => Some(path.clone()),
        }
    }

    /// Get the run ID (uses seed as a stable identifier).
    fn run_id(&self) -> u64 {
        self.seed
    }

    /// Get the seed value.
    fn seed(&self) -> u64 {
        self.seed
    }

    /// Get the speed multiplier.
    fn speed(&self) -> f64 {
        self.speed
    }

    /// Check if quick mode is enabled.
    fn is_quick(&self) -> bool {
        self.quick
    }

    /// Check if interactive mode is enabled.
    ///
    /// Returns `false` if `--no-interactive` was specified, otherwise `true`.
    fn is_interactive(&self) -> bool {
        self.interactive.unwrap_or(true)
    }
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Config, String> {
    let mut iter = args.into_iter();
    // Drop binary name if present.
    let _ = iter.next();

    let mut cfg = Config::with_defaults();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => cfg.help = true,
            "--list-scenes" => cfg.list_scenes = true,
            "--scene" => {
                if cfg.scene.is_some() {
                    return Err("`--scene` provided more than once.".to_string());
                }
                let scene = next_value(&mut iter, "--scene")?;
                if !is_known_scene(&scene) {
                    return Err(format!(
                        "Unknown scene `{scene}`.\n\n{}",
                        available_scenes_help()
                    ));
                }
                cfg.scene = Some(scene);
            }
            "--seed" => {
                let raw = next_value(&mut iter, "--seed")?;
                cfg.seed = parse_u64_flag("--seed", &raw)?;
            }
            "--quick" => cfg.quick = true,
            "--speed" => {
                let raw = next_value(&mut iter, "--speed")?;
                cfg.speed = raw.parse::<f64>().map_err(|_| {
                    format!("Invalid --speed value `{raw}` (expected a number like 0.5, 1.0, 2.0).")
                })?;
                if !cfg.speed.is_finite() || cfg.speed <= 0.0 {
                    return Err(format!(
                        "Invalid --speed value `{raw}` (expected a finite number > 0)."
                    ));
                }
            }

            "--interactive" => cfg.interactive = Some(true),
            "--no-interactive" => cfg.interactive = Some(false),
            "--live" => cfg.live = Some(true),
            "--no-live" => cfg.live = Some(false),
            "--screen" => cfg.screen = Some(true),
            "--no-screen" => cfg.screen = Some(false),

            "--force-terminal" => cfg.force_terminal = true,
            "--width" => {
                let raw = next_value(&mut iter, "--width")?;
                cfg.width = Some(parse_usize_flag("--width", &raw)?);
            }
            "--height" => {
                let raw = next_value(&mut iter, "--height")?;
                cfg.height = Some(parse_usize_flag("--height", &raw)?);
            }
            "--color-system" => {
                let raw = next_value(&mut iter, "--color-system")?;
                cfg.color_system = ColorMode::parse(&raw)?;
            }
            "--emoji" => cfg.emoji = Some(true),
            "--no-emoji" => cfg.emoji = Some(false),
            "--safe-box" => cfg.safe_box = Some(true),
            "--no-safe-box" => cfg.safe_box = Some(false),
            "--links" => cfg.links = Some(true),
            "--no-links" => cfg.links = Some(false),

            "--export" => {
                if !matches!(cfg.export, ExportMode::Off) {
                    return Err("`--export`/`--export-dir` provided more than once.".to_string());
                }
                cfg.export = ExportMode::TempDir;
            }
            "--export-dir" => {
                if !matches!(cfg.export, ExportMode::Off) {
                    return Err("`--export`/`--export-dir` provided more than once.".to_string());
                }
                let raw = next_value(&mut iter, "--export-dir")?;
                cfg.export = ExportMode::Dir(PathBuf::from(raw));
            }

            "--log-level" => {
                let raw = next_value(&mut iter, "--log-level")?;
                cfg.log_level = LogLevel::parse(&raw)?;
            }

            "--" => {
                return Err(
                    "Unexpected positional arguments (this CLI has no positional args)."
                        .to_string(),
                );
            }

            _ => {
                return Err(format!(
                    "Unknown flag: {arg}\n\nRun with `--help` to see valid options."
                ));
            }
        }
    }

    Ok(cfg)
}

fn next_value(iter: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    iter.next()
        .ok_or_else(|| format!("Missing value for `{flag}`."))
}

fn parse_usize_flag(flag: &str, raw: &str) -> Result<usize, String> {
    let value = raw
        .parse::<usize>()
        .map_err(|_| format!("Invalid {flag} value `{raw}` (expected a positive integer)."))?;
    if value == 0 {
        return Err(format!("Invalid {flag} value `{raw}` (expected >= 1)."));
    }
    Ok(value)
}

fn parse_u64_flag(flag: &str, raw: &str) -> Result<u64, String> {
    raw.parse::<u64>()
        .map_err(|_| format!("Invalid {flag} value `{raw}` (expected a non-negative integer)."))
}

fn is_known_scene(name: &str) -> bool {
    scenes::build_registry().contains(name)
}

fn available_scenes_help() -> String {
    let registry = scenes::build_registry();
    let scenes: Vec<_> = registry.all().collect();

    let mut out = String::from("Available scenes:\n");
    let width = scenes.iter().map(|s| s.name().len()).max().unwrap_or(0);

    for scene in scenes {
        out.push_str(&format!(
            "  {:width$} - {}\n",
            scene.name(),
            scene.summary(),
            width = width
        ));
    }

    out.push_str("\nRun with `--list-scenes` to print this list and exit.");
    out
}

const HELP_TEXT: &str = r#"demo_showcase — Nebula Deploy (rich_rust showcase)

USAGE:
    demo_showcase [OPTIONS]

OPTIONS:
    --list-scenes               List available scenes and exit
    --scene <name>              Run a single scene (see --list-scenes)
    --seed <u64>                Seed deterministic demo data (default: 0)
    --quick                     Reduce sleeps/runtime (CI-friendly)
    --speed <multiplier>        Animation speed multiplier (default: 1.0)

    --interactive               Force interactive mode
    --no-interactive            Disable prompts/pager/etc
    --live                      Force live refresh
    --no-live                   Disable live refresh; print snapshots
    --screen                    Use alternate screen (requires live)
    --no-screen                 Disable alternate screen

    --force-terminal            Treat stdout as a TTY (even when piped)
    --width <cols>              Override console width
    --height <rows>             Override console height
    --color-system <mode>       auto|none|standard|eight_bit|truecolor
    --emoji                     Enable emoji (default)
    --no-emoji                  Disable emoji
    --safe-box                  Use ASCII-safe box characters
    --no-safe-box               Use Unicode box characters (default)
    --links                     Enable OSC8 hyperlinks
    --no-links                  Disable OSC8 hyperlinks

    --export                    Write an HTML/SVG bundle to a temp dir
    --export-dir <path>         Write an HTML/SVG bundle to a directory

    --log-level <level>         Enable RichLogger (off|error|warn|info|debug|trace)

    -h, --help                  Print help and exit

EXAMPLES:
    demo_showcase               Run the full demo (TTY-friendly defaults)
    demo_showcase --list-scenes List scenes
    demo_showcase --scene hero  Run a single scene
    demo_showcase --quick       Fast mode for CI/dev
    demo_showcase | cat         Non-interactive output (no live/prompt)
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(argv: &[&str]) -> Result<Config, String> {
        parse_args(argv.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn help_flag_sets_help() {
        let cfg = parse(&["demo_showcase", "--help"]).expect("parse");
        assert!(cfg.help);
    }

    #[test]
    fn list_scenes_parses() {
        let cfg = parse(&["demo_showcase", "--list-scenes"]).expect("parse");
        assert!(cfg.list_scenes);
    }

    #[test]
    fn scene_parses_once() {
        let cfg = parse(&["demo_showcase", "--scene", "hero"]).expect("parse");
        assert_eq!(cfg.scene.as_deref(), Some("hero"));
    }

    #[test]
    fn scene_rejects_unknown() {
        let err = parse(&["demo_showcase", "--scene", "wat"]).expect_err("error");
        assert!(err.contains("Unknown scene"));
        assert!(err.contains("Available scenes"));
    }

    #[test]
    fn scene_rejects_duplicates() {
        let err =
            parse(&["demo_showcase", "--scene", "hero", "--scene", "outro"]).expect_err("error");
        assert!(err.contains("more than once"));
    }

    #[test]
    fn boolean_no_forms_parse() {
        let cfg = parse(&[
            "demo_showcase",
            "--no-interactive",
            "--live",
            "--no-screen",
            "--no-emoji",
            "--safe-box",
            "--no-links",
        ])
        .expect("parse");

        assert_eq!(cfg.interactive, Some(false));
        assert_eq!(cfg.live, Some(true));
        assert_eq!(cfg.screen, Some(false));
        assert_eq!(cfg.emoji, Some(false));
        assert_eq!(cfg.safe_box, Some(true));
        assert_eq!(cfg.links, Some(false));
    }

    #[test]
    fn speed_parses_and_requires_positive_finite() {
        let cfg = parse(&["demo_showcase", "--speed", "1.5"]).expect("parse");
        assert_eq!(cfg.speed, 1.5);

        let err = parse(&["demo_showcase", "--speed", "0"]).expect_err("error");
        assert!(err.contains("> 0"));
    }

    #[test]
    fn seed_parses_as_u64() {
        let cfg = parse(&["demo_showcase", "--seed", "42"]).expect("parse");
        assert_eq!(cfg.seed, 42);

        let err = parse(&["demo_showcase", "--seed", "wat"]).expect_err("error");
        assert!(err.contains("Invalid --seed"));

        let err = parse(&["demo_showcase", "--seed", "-1"]).expect_err("error");
        assert!(err.contains("Invalid --seed"));
    }

    #[test]
    fn links_toggle_parses() {
        let cfg = parse(&["demo_showcase", "--links"]).expect("parse");
        assert_eq!(cfg.links, Some(true));

        let cfg = parse(&["demo_showcase", "--no-links"]).expect("parse");
        assert_eq!(cfg.links, Some(false));
    }

    #[test]
    fn width_height_require_positive_ints() {
        let cfg = parse(&["demo_showcase", "--width", "80", "--height", "24"]).expect("parse");
        assert_eq!(cfg.width, Some(80));
        assert_eq!(cfg.height, Some(24));

        let err = parse(&["demo_showcase", "--width", "0"]).expect_err("error");
        assert!(err.contains(">= 1"));
    }

    #[test]
    fn color_system_parses_known_values() {
        let cfg = parse(&["demo_showcase", "--color-system", "eight_bit"]).expect("parse");
        assert_eq!(cfg.color_system, ColorMode::EightBit);

        let err = parse(&["demo_showcase", "--color-system", "wat"]).expect_err("error");
        assert!(err.contains("Invalid --color-system"));
    }

    #[test]
    fn export_flags_are_mutually_exclusive() {
        let cfg = parse(&["demo_showcase", "--export"]).expect("parse");
        assert!(matches!(cfg.export, ExportMode::TempDir));

        let cfg = parse(&["demo_showcase", "--export-dir", "out"]).expect("parse");
        assert!(matches!(cfg.export, ExportMode::Dir(_)));

        let err = parse(&["demo_showcase", "--export", "--export-dir", "out"]).expect_err("error");
        assert!(err.contains("more than once"));
    }

    #[test]
    fn unknown_flags_error_is_friendly() {
        let err = parse(&["demo_showcase", "--wat"]).expect_err("error");
        assert!(err.contains("Unknown flag"));
        assert!(err.contains("--help"));
    }

    // ========== Additional CLI tests (bd-6tj5) ==========

    #[test]
    fn default_config_has_expected_values() {
        let cfg = parse(&["demo_showcase"]).expect("parse");
        // Default values from Config::with_defaults()
        assert_eq!(cfg.speed, 1.0);
        assert_eq!(cfg.seed, 0);
        assert!(!cfg.quick);
        assert!(!cfg.force_terminal);
        assert!(!cfg.help);
        assert!(!cfg.list_scenes);
        assert!(cfg.scene.is_none());
        assert!(cfg.width.is_none());
        assert!(cfg.height.is_none());
        assert!(cfg.interactive.is_none());
        assert!(cfg.live.is_none());
        assert!(cfg.screen.is_none());
        assert!(cfg.emoji.is_none());
        assert!(cfg.safe_box.is_none());
        assert!(cfg.links.is_none());
        assert!(matches!(cfg.color_system, ColorMode::Auto));
        assert!(matches!(cfg.export, ExportMode::Off));
    }

    #[test]
    fn quick_flag_parses() {
        let cfg = parse(&["demo_showcase", "--quick"]).expect("parse");
        assert!(cfg.quick);
    }

    #[test]
    fn force_terminal_flag_parses() {
        let cfg = parse(&["demo_showcase", "--force-terminal"]).expect("parse");
        assert!(cfg.force_terminal);
    }

    #[test]
    fn short_help_flag_works() {
        let cfg = parse(&["demo_showcase", "-h"]).expect("parse");
        assert!(cfg.help);
    }

    #[test]
    fn all_color_system_variants_parse() {
        let cases = [
            ("auto", ColorMode::Auto),
            ("none", ColorMode::None),
            ("no", ColorMode::None),
            ("off", ColorMode::None),
            ("standard", ColorMode::Standard),
            ("16", ColorMode::Standard),
            ("eight_bit", ColorMode::EightBit),
            ("eightbit", ColorMode::EightBit),
            ("256", ColorMode::EightBit),
            ("truecolor", ColorMode::TrueColor),
            ("true", ColorMode::TrueColor),
            ("24bit", ColorMode::TrueColor),
        ];

        for (input, expected) in cases {
            let cfg = parse(&["demo_showcase", "--color-system", input])
                .expect("parse color-system variant");
            assert_eq!(cfg.color_system, expected, "color-system {input}");
        }
    }

    #[test]
    fn missing_flag_value_gives_helpful_error() {
        let cases = [
            ("--speed", "Missing value for `--speed`"),
            ("--seed", "Missing value for `--seed`"),
            ("--width", "Missing value for `--width`"),
            ("--height", "Missing value for `--height`"),
            ("--color-system", "Missing value for `--color-system`"),
            ("--scene", "Missing value for `--scene`"),
            ("--export-dir", "Missing value for `--export-dir`"),
        ];

        for (flag, expected_msg) in cases {
            let err = parse(&["demo_showcase", flag]).expect_err("should error");
            assert!(
                err.contains(expected_msg),
                "Flag {flag} should report missing value, got: {err}"
            );
        }
    }

    #[test]
    fn speed_rejects_non_finite_values() {
        let err = parse(&["demo_showcase", "--speed", "inf"]).expect_err("error");
        assert!(err.contains("finite") || err.contains("> 0"));

        let err = parse(&["demo_showcase", "--speed", "nan"]).expect_err("error");
        assert!(err.contains("expected a number") || err.contains("Invalid --speed"));
    }

    #[test]
    fn speed_rejects_negative_values() {
        let err = parse(&["demo_showcase", "--speed", "-1.0"]).expect_err("error");
        assert!(err.contains("> 0"));
    }

    #[test]
    fn all_boolean_flag_pairs_parse() {
        // Test positive forms
        let cfg = parse(&["demo_showcase", "--interactive"]).expect("parse");
        assert_eq!(cfg.interactive, Some(true));

        let cfg = parse(&["demo_showcase", "--live"]).expect("parse");
        assert_eq!(cfg.live, Some(true));

        let cfg = parse(&["demo_showcase", "--screen"]).expect("parse");
        assert_eq!(cfg.screen, Some(true));

        let cfg = parse(&["demo_showcase", "--emoji"]).expect("parse");
        assert_eq!(cfg.emoji, Some(true));

        let cfg = parse(&["demo_showcase", "--safe-box"]).expect("parse");
        assert_eq!(cfg.safe_box, Some(true));

        let cfg = parse(&["demo_showcase", "--links"]).expect("parse");
        assert_eq!(cfg.links, Some(true));
    }

    #[test]
    fn width_height_reject_non_integer() {
        let err = parse(&["demo_showcase", "--width", "abc"]).expect_err("error");
        assert!(err.contains("Invalid --width"));

        let err = parse(&["demo_showcase", "--height", "1.5"]).expect_err("error");
        assert!(err.contains("Invalid --height"));
    }

    #[test]
    fn positional_args_rejected() {
        let err = parse(&["demo_showcase", "--"]).expect_err("error");
        assert!(err.contains("positional arguments"));
    }

    #[test]
    fn multiple_independent_flags_combine() {
        let cfg = parse(&[
            "demo_showcase",
            "--quick",
            "--force-terminal",
            "--width",
            "120",
            "--height",
            "40",
            "--seed",
            "12345",
            "--speed",
            "2.0",
            "--color-system",
            "truecolor",
            "--emoji",
            "--no-safe-box",
            "--links",
        ])
        .expect("parse");

        assert!(cfg.quick);
        assert!(cfg.force_terminal);
        assert_eq!(cfg.width, Some(120));
        assert_eq!(cfg.height, Some(40));
        assert_eq!(cfg.seed, 12345);
        assert_eq!(cfg.speed, 2.0);
        assert_eq!(cfg.color_system, ColorMode::TrueColor);
        assert_eq!(cfg.emoji, Some(true));
        assert_eq!(cfg.safe_box, Some(false));
        assert_eq!(cfg.links, Some(true));
    }

    // ========== Log level tests (bd-2rxj) ==========

    #[test]
    fn log_level_parses_all_variants() {
        let cases = [
            ("off", LogLevel::Off),
            ("none", LogLevel::Off),
            ("error", LogLevel::Error),
            ("warn", LogLevel::Warn),
            ("warning", LogLevel::Warn),
            ("info", LogLevel::Info),
            ("debug", LogLevel::Debug),
            ("trace", LogLevel::Trace),
        ];

        for (input, expected) in cases {
            let cfg = parse(&["demo_showcase", "--log-level", input]).expect("parse log-level");
            assert_eq!(cfg.log_level, expected, "log-level {input}");
        }
    }

    #[test]
    fn log_level_rejects_invalid() {
        let err = parse(&["demo_showcase", "--log-level", "wat"]).expect_err("error");
        assert!(err.contains("Invalid --log-level"));
    }

    #[test]
    fn log_level_defaults_to_off() {
        let cfg = parse(&["demo_showcase"]).expect("parse");
        assert_eq!(cfg.log_level, LogLevel::Off);
    }
}
