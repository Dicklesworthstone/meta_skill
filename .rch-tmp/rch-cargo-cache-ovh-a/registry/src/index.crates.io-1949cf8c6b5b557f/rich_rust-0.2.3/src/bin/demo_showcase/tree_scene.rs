//! Tree showcase scene for demo_showcase.
//!
//! Demonstrates rich_rust Tree capabilities including:
//! - Hierarchical data display with guide lines
//! - Different guide styles (Unicode, Rounded, ASCII, Bold)
//! - Node icons and styling
//! - Collapsed nodes
//! - Service dependency graphs

use std::sync::Arc;

use rich_rust::console::Console;
use rich_rust::markup;
use rich_rust::renderables::tree::{Tree, TreeGuides, TreeNode};
use rich_rust::style::Style;

use crate::Config;
use crate::scenes::{Scene, SceneError};

/// Tree showcase scene: demonstrates Tree rendering capabilities.
pub struct TreeScene;

impl TreeScene {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Scene for TreeScene {
    fn name(&self) -> &'static str {
        "tree"
    }

    fn summary(&self) -> &'static str {
        "Tree showcase: guides, icons, collapsed nodes, dependency graphs."
    }

    fn run(&self, console: &Arc<Console>, _cfg: &Config) -> Result<(), SceneError> {
        console.print("[section.title]Trees: Hierarchical Data Display[/]");
        console.print("");
        console.print(
            "[dim]Trees visualize hierarchical relationships with configurable guide styles.[/]",
        );
        console.print("");

        // Demo 1: Deployment plan with icons
        render_deployment_plan(console);

        console.print("");

        // Demo 2: Service dependency graph
        render_service_dependencies(console);

        console.print("");

        // Demo 3: Guide styles comparison
        render_guide_styles(console);

        console.print("");

        // Demo 4: Collapsed nodes
        render_collapsed_tree(console);

        Ok(())
    }
}

/// Render a deployment plan tree with icons.
fn render_deployment_plan(console: &Console) {
    console.print("[brand.accent]Deployment Plan[/]");
    console.print("");

    // Build deployment plan tree with icons
    let root = TreeNode::with_icon(
        "📋",
        markup::render_or_plain("[bold]Nebula Deploy v2.4.1[/]"),
    )
    .child(
        TreeNode::with_icon("🔍", markup::render_or_plain("[cyan]Pre-flight Checks[/]"))
            .child(TreeNode::with_icon(
                "✓",
                markup::render_or_plain("[green]Health checks passed[/]"),
            ))
            .child(TreeNode::with_icon(
                "✓",
                markup::render_or_plain("[green]Dependencies verified[/]"),
            ))
            .child(TreeNode::with_icon(
                "✓",
                markup::render_or_plain("[green]Config validated[/]"),
            )),
    )
    .child(
        TreeNode::with_icon("📦", markup::render_or_plain("[cyan]Build Phase[/]"))
            .child(TreeNode::with_icon(
                "✓",
                markup::render_or_plain("[green]Compile artifacts[/]"),
            ))
            .child(TreeNode::with_icon(
                "✓",
                markup::render_or_plain("[green]Run test suite[/]"),
            ))
            .child(TreeNode::with_icon(
                "✓",
                markup::render_or_plain("[green]Create container image[/]"),
            )),
    )
    .child(
        TreeNode::with_icon("🚀", markup::render_or_plain("[cyan]Deploy Phase[/]"))
            .child(TreeNode::with_icon(
                "→",
                markup::render_or_plain("[yellow]Rolling update (in progress)[/]"),
            ))
            .child(TreeNode::with_icon(
                "○",
                markup::render_or_plain("[dim]Health verification[/]"),
            ))
            .child(TreeNode::with_icon(
                "○",
                markup::render_or_plain("[dim]Traffic migration[/]"),
            )),
    )
    .child(
        TreeNode::with_icon(
            "📊",
            markup::render_or_plain("[dim]Post-deploy Validation[/]"),
        )
        .child(TreeNode::with_icon(
            "○",
            markup::render_or_plain("[dim]Smoke tests[/]"),
        ))
        .child(TreeNode::with_icon(
            "○",
            markup::render_or_plain("[dim]Metric baseline[/]"),
        )),
    );

    let tree = Tree::new(root)
        .guides(TreeGuides::Rounded)
        .guide_style(Style::parse("dim cyan").unwrap_or_default());

    console.print_renderable(&tree);

    console.print("");
    console.print("[hint]Icons indicate status: ✓ complete, → in progress, ○ pending.[/]");
}

/// Render a service dependency graph.
fn render_service_dependencies(console: &Console) {
    console.print("[brand.accent]Service Dependency Graph[/]");
    console.print("");

    let root = TreeNode::with_icon("🌐", markup::render_or_plain("[bold]api-gateway[/]"))
        .child(
            TreeNode::with_icon("🔐", markup::render_or_plain("[cyan]auth-service[/]"))
                .child(TreeNode::with_icon(
                    "🗄️",
                    markup::render_or_plain("postgres-primary"),
                ))
                .child(TreeNode::with_icon(
                    "📮",
                    markup::render_or_plain("redis-sessions"),
                )),
        )
        .child(
            TreeNode::with_icon("👤", markup::render_or_plain("[cyan]user-service[/]"))
                .child(TreeNode::with_icon(
                    "🗄️",
                    markup::render_or_plain("postgres-primary"),
                ))
                .child(TreeNode::with_icon(
                    "📮",
                    markup::render_or_plain("redis-cache"),
                )),
        )
        .child(
            TreeNode::with_icon("📊", markup::render_or_plain("[cyan]analytics-service[/]"))
                .child(TreeNode::with_icon(
                    "🔍",
                    markup::render_or_plain("elasticsearch"),
                ))
                .child(TreeNode::with_icon(
                    "📨",
                    markup::render_or_plain("kafka-cluster"),
                )),
        )
        .child(
            TreeNode::with_icon("💳", markup::render_or_plain("[cyan]billing-service[/]"))
                .child(TreeNode::with_icon(
                    "🗄️",
                    markup::render_or_plain("postgres-billing"),
                ))
                .child(TreeNode::with_icon(
                    "🔒",
                    markup::render_or_plain("vault-secrets"),
                )),
        );

    let tree = Tree::new(root)
        .guides(TreeGuides::Rounded)
        .guide_style(Style::parse("dim").unwrap_or_default());

    console.print_renderable(&tree);

    console.print("");
    console.print("[hint]Dependency trees help visualize service relationships.[/]");
}

/// Demonstrate different guide styles.
fn render_guide_styles(console: &Console) {
    console.print("[brand.accent]Guide Style Comparison[/]");
    console.print("");

    let styles = [
        ("Unicode (default)", TreeGuides::Unicode),
        ("Rounded", TreeGuides::Rounded),
        ("Bold", TreeGuides::Bold),
        ("ASCII", TreeGuides::Ascii),
    ];

    for (name, guides) in styles {
        console.print(&format!("[dim]{name}:[/]"));

        let root = TreeNode::new(markup::render_or_plain("[bold]root[/]"))
            .child(
                TreeNode::new("branch-a")
                    .child(TreeNode::new("leaf-1"))
                    .child(TreeNode::new("leaf-2")),
            )
            .child(TreeNode::new("branch-b"));

        let tree = Tree::new(root)
            .guides(guides)
            .guide_style(Style::parse("cyan").unwrap_or_default());

        console.print_renderable(&tree);
        console.print("");
    }

    console.print("[hint]Choose guides based on terminal capabilities and aesthetics.[/]");
}

/// Demonstrate collapsed nodes.
fn render_collapsed_tree(console: &Console) {
    console.print("[brand.accent]Collapsed Nodes[/]");
    console.print("");
    console.print("[dim]Collapsed nodes hide their children for compact display:[/]");
    console.print("");

    let root = TreeNode::with_icon("📁", markup::render_or_plain("[bold]release-artifacts/[/]"))
        .child(
            TreeNode::with_icon("📁", markup::render_or_plain("[cyan]binaries/[/]"))
                .child(TreeNode::with_icon(
                    "📄",
                    markup::render_or_plain("nebula-linux-x86_64"),
                ))
                .child(TreeNode::with_icon(
                    "📄",
                    markup::render_or_plain("nebula-darwin-arm64"),
                ))
                .child(TreeNode::with_icon(
                    "📄",
                    markup::render_or_plain("nebula-windows-x86_64.exe"),
                )),
        )
        .child(
            TreeNode::with_icon(
                "📁",
                markup::render_or_plain("[dim]checksums/[/] [dim italic](3 items)[/]"),
            )
            .collapsed()
            .child(TreeNode::new("SHA256SUMS"))
            .child(TreeNode::new("SHA256SUMS.sig"))
            .child(TreeNode::new("SHA512SUMS")),
        )
        .child(
            TreeNode::with_icon(
                "📁",
                markup::render_or_plain("[dim]docs/[/] [dim italic](5 items)[/]"),
            )
            .collapsed()
            .child(TreeNode::new("README.md"))
            .child(TreeNode::new("CHANGELOG.md"))
            .child(TreeNode::new("LICENSE"))
            .child(TreeNode::new("MIGRATION.md"))
            .child(TreeNode::new("API.md")),
        )
        .child(TreeNode::with_icon(
            "📄",
            markup::render_or_plain("manifest.json"),
        ));

    let tree = Tree::new(root)
        .guides(TreeGuides::Rounded)
        .guide_style(Style::parse("dim").unwrap_or_default());

    console.print_renderable(&tree);

    console.print("");
    console.print("[hint]Collapsed nodes show item counts; expand for details.[/]");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_scene_has_correct_name() {
        let scene = TreeScene::new();
        assert_eq!(scene.name(), "tree");
    }

    #[test]
    fn tree_scene_runs_without_error() {
        let scene = TreeScene::new();
        let console = Console::builder()
            .force_terminal(false)
            .markup(true)
            .build()
            .shared();
        let cfg = Config::with_defaults();

        let result = scene.run(&console, &cfg);
        assert!(result.is_ok());
    }
}
