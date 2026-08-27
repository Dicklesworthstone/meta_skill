//! Scene trait and registry for demo_showcase.
//!
//! Provides the `Scene` abstraction for individual showcase segments and a
//! registry for listing and selecting scenes by name.

use std::collections::HashMap;
use std::sync::Arc;

use rich_rust::console::Console;

use crate::Config;
use crate::dashboard_scene::DashboardScene;
use crate::debug_tools::DebugToolsScene;
use crate::emoji_links_scene::EmojiLinksScene;
use crate::export_scene::ExportScene;
use crate::hero::HeroScene;
use crate::json_scene::JsonScene;
use crate::layout_scene::LayoutScene;
use crate::markdown_scene::MarkdownScene;
use crate::outro_scene::OutroScene;
use crate::panel_scene::PanelScene;
use crate::syntax_scene::SyntaxScene;
use crate::table_scene::TableScene;
use crate::traceback_scene::TracebackScene;
use crate::tracing_scene::TracingScene;
use crate::tree_scene::TreeScene;

/// Error type for scene execution.
#[derive(Debug)]
pub enum SceneError {
    /// Scene execution failed with a message.
    Failed(String),
    /// An I/O error occurred during scene execution.
    Io(std::io::Error),
}

impl std::fmt::Display for SceneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Failed(msg) => write!(f, "{msg}"),
            Self::Io(err) => write!(f, "I/O error: {err}"),
        }
    }
}

impl std::error::Error for SceneError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for SceneError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

/// A single showcase scene.
///
/// Scenes are self-contained demonstrations of rich_rust features. Each scene
/// has a unique name, a short summary for display in `--list-scenes`, and a
/// `run` method that executes the demonstration.
pub trait Scene: Send + Sync {
    /// The unique identifier for this scene (used with `--scene <name>`).
    fn name(&self) -> &'static str;

    /// A short summary describing what this scene demonstrates.
    fn summary(&self) -> &'static str;

    /// Execute the scene.
    ///
    /// # Arguments
    /// * `console` - Shared console for output
    /// * `cfg` - The demo configuration (timing, export settings, etc.)
    ///
    /// # Returns
    /// `Ok(())` on success, or a `SceneError` on failure.
    fn run(&self, console: &Arc<Console>, cfg: &Config) -> Result<(), SceneError>;
}

/// Registry of available scenes.
///
/// The registry maintains the canonical ordering of scenes (matching the demo
/// storyboard) and provides lookup by name.
pub struct SceneRegistry {
    /// Ordered list of scenes (defines full-demo playback order).
    scenes: Vec<Box<dyn Scene>>,
    /// Name-to-index lookup for `--scene <name>`.
    by_name: HashMap<&'static str, usize>,
}

impl SceneRegistry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            scenes: Vec::new(),
            by_name: HashMap::new(),
        }
    }

    /// Register a scene. Scenes are run in registration order.
    pub fn register<S: Scene + 'static>(&mut self, scene: S) {
        let name = scene.name();
        let idx = self.scenes.len();
        self.scenes.push(Box::new(scene));
        self.by_name.insert(name, idx);
    }

    /// Get a scene by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&dyn Scene> {
        self.by_name
            .get(name)
            .and_then(|&idx| self.scenes.get(idx))
            .map(|s| s.as_ref())
    }

    /// Check if a scene exists by name.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.by_name.contains_key(name)
    }

    /// Get all scenes in registration order.
    pub fn all(&self) -> impl Iterator<Item = &dyn Scene> {
        self.scenes.iter().map(|s| s.as_ref())
    }

    /// Get the number of registered scenes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.scenes.len()
    }

    /// Check if the registry is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.scenes.is_empty()
    }
}

impl Default for SceneRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Build the default scene registry with all demo scenes.
///
/// Scene order matches the storyboard from bd-2b0s.
#[must_use]
pub fn build_registry() -> SceneRegistry {
    let mut registry = SceneRegistry::new();

    // Register scenes in storyboard order
    registry.register(HeroScene::new());
    registry.register(DashboardScene::new());
    registry.register(MarkdownScene::new());
    registry.register(SyntaxScene::new());
    registry.register(JsonScene::new());
    registry.register(TableScene::new());
    registry.register(PanelScene::new());
    registry.register(TreeScene::new());
    registry.register(LayoutScene::new());
    registry.register(EmojiLinksScene::new());
    registry.register(DebugToolsScene::new());
    registry.register(TracingScene::new());
    registry.register(TracebackScene::new());
    registry.register(ExportScene::new());
    registry.register(OutroScene::new());

    registry
}

/// Print the scene list as a formatted table.
///
/// This is itself a mini-showcase of rich_rust's Table rendering.
pub fn print_scene_list(console: &Console) {
    use rich_rust::renderables::table::{Column, Table};
    use rich_rust::style::Style;

    let registry = build_registry();
    let _scene_count = registry.len();
    if registry.is_empty() {
        console.print("[dim]No scenes registered.[/]");
        return;
    }

    let mut table = Table::new().title("Available Scenes");
    table.add_column(Column::new("Scene").style(Style::parse("bold cyan").unwrap_or_default()));
    table.add_column(Column::new("Description"));

    for scene in registry.all() {
        table.add_row_cells([scene.name(), scene.summary()]);
    }

    console.print_renderable(&table);
    console.print("");
    console.print("[dim]Run with [bold]--scene <name>[/bold] to run a single scene.[/]");
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyScene {
        name: &'static str,
        summary: &'static str,
    }

    impl DummyScene {
        const fn new(name: &'static str, summary: &'static str) -> Self {
            Self { name, summary }
        }
    }

    impl Scene for DummyScene {
        fn name(&self) -> &'static str {
            self.name
        }

        fn summary(&self) -> &'static str {
            self.summary
        }

        fn run(&self, _console: &Arc<Console>, _cfg: &Config) -> Result<(), SceneError> {
            Ok(())
        }
    }

    #[test]
    fn registry_registration_order_preserved() {
        let mut registry = SceneRegistry::new();
        registry.register(DummyScene::new("first", "First scene"));
        registry.register(DummyScene::new("second", "Second scene"));
        registry.register(DummyScene::new("third", "Third scene"));

        let names: Vec<_> = registry.all().map(|s| s.name()).collect();
        assert_eq!(names, vec!["first", "second", "third"]);
    }

    #[test]
    fn registry_lookup_by_name() {
        let mut registry = SceneRegistry::new();
        registry.register(DummyScene::new("hero", "Hero scene"));
        registry.register(DummyScene::new("outro", "Outro scene"));

        assert!(registry.contains("hero"));
        assert!(registry.contains("outro"));
        assert!(!registry.contains("unknown"));

        let scene = registry.get("hero").expect("hero scene");
        assert_eq!(scene.name(), "hero");
    }

    #[test]
    fn build_registry_contains_all_scenes() {
        let registry = build_registry();

        // Verify all expected scenes are present
        let expected = [
            "hero",
            "dashboard",
            "markdown",
            "syntax",
            "json",
            "table",
            "panels",
            "tree",
            "layout",
            "emoji_links",
            "debug_tools",
            "tracing",
            "traceback",
            "export",
            "outro",
        ];

        for name in expected {
            assert!(
                registry.contains(name),
                "Registry should contain scene '{name}'"
            );
        }

        assert_eq!(registry.len(), expected.len());
    }

    #[test]
    fn scene_error_display() {
        let err = SceneError::Failed("test error".to_string());
        assert_eq!(err.to_string(), "test error");

        let io_err = SceneError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "file not found",
        ));
        assert!(io_err.to_string().contains("I/O error"));
    }
}
