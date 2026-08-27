//! Tree rendering example demonstrating hierarchical data display.
//!
//! Run with: `cargo run --example tree`

use rich_rust::prelude::*;

fn main() {
    let console = Console::new();
    let _width = console.width().min(80);

    // ========================================================================
    // Basic Tree
    // ========================================================================
    println!("\n=== Basic Tree ===\n");

    let tree = Tree::new(
        TreeNode::new("Root")
            .child(TreeNode::new("Child 1"))
            .child(TreeNode::new("Child 2"))
            .child(TreeNode::new("Child 3")),
    );

    console.print_renderable(&tree);

    // ========================================================================
    // Nested Tree
    // ========================================================================
    println!("\n=== Nested Tree ===\n");

    let nested = Tree::new(
        TreeNode::new("Project")
            .child(
                TreeNode::new("src")
                    .child(TreeNode::new("main.rs"))
                    .child(TreeNode::new("lib.rs"))
                    .child(
                        TreeNode::new("modules")
                            .child(TreeNode::new("parser.rs"))
                            .child(TreeNode::new("renderer.rs")),
                    ),
            )
            .child(
                TreeNode::new("tests")
                    .child(TreeNode::new("unit_tests.rs"))
                    .child(TreeNode::new("integration_tests.rs")),
            )
            .child(TreeNode::new("Cargo.toml"))
            .child(TreeNode::new("README.md")),
    );

    console.print_renderable(&nested);

    // ========================================================================
    // Tree with Icons
    // ========================================================================
    println!("\n=== Tree with Icons ===\n");

    let with_icons = Tree::new(
        TreeNode::with_icon("\u{1F4C1}", "Documents") // Folder emoji
            .child(
                TreeNode::with_icon("\u{1F4C2}", "Work") // Open folder
                    .child(TreeNode::with_icon("\u{1F4C4}", "report.docx")) // Document
                    .child(TreeNode::with_icon("\u{1F4CA}", "data.xlsx")), // Chart
            )
            .child(
                TreeNode::with_icon("\u{1F4C2}", "Personal")
                    .child(TreeNode::with_icon("\u{1F4F7}", "photos.zip")) // Camera
                    .child(TreeNode::with_icon("\u{1F3B5}", "music.mp3")), // Music note
            ),
    );

    console.print_renderable(&with_icons);

    // ========================================================================
    // Different Guide Styles
    // ========================================================================
    println!("\n=== ASCII Guides ===\n");

    let ascii_tree = Tree::new(
        TreeNode::new("Root")
            .child(TreeNode::new("Branch A").child(TreeNode::new("Leaf 1")))
            .child(TreeNode::new("Branch B").child(TreeNode::new("Leaf 2"))),
    )
    .guides(TreeGuides::Ascii);

    console.print_renderable(&ascii_tree);

    println!("\n=== Bold Guides ===\n");

    let bold_tree = Tree::new(
        TreeNode::new("Root")
            .child(TreeNode::new("Branch A").child(TreeNode::new("Leaf 1")))
            .child(TreeNode::new("Branch B").child(TreeNode::new("Leaf 2"))),
    )
    .guides(TreeGuides::Bold);

    console.print_renderable(&bold_tree);

    println!("\n=== Double Guides ===\n");

    let double_tree = Tree::new(
        TreeNode::new("Root")
            .child(TreeNode::new("Branch A").child(TreeNode::new("Leaf 1")))
            .child(TreeNode::new("Branch B").child(TreeNode::new("Leaf 2"))),
    )
    .guides(TreeGuides::Double);

    console.print_renderable(&double_tree);

    println!("\n=== Rounded Guides ===\n");

    let rounded_tree = Tree::new(
        TreeNode::new("Root")
            .child(TreeNode::new("Branch A").child(TreeNode::new("Leaf 1")))
            .child(TreeNode::new("Branch B").child(TreeNode::new("Leaf 2"))),
    )
    .guides(TreeGuides::Rounded);

    console.print_renderable(&rounded_tree);

    // ========================================================================
    // Styled Tree
    // ========================================================================
    println!("\n=== Styled Tree ===\n");

    let styled_tree = Tree::new(
        TreeNode::new("Configuration")
            .child(
                TreeNode::new("Database")
                    .child(TreeNode::new("host: localhost"))
                    .child(TreeNode::new("port: 5432")),
            )
            .child(
                TreeNode::new("Server")
                    .child(TreeNode::new("host: 0.0.0.0"))
                    .child(TreeNode::new("port: 8080")),
            ),
    )
    .guide_style(Style::parse("dim cyan").unwrap_or_default());

    console.print_renderable(&styled_tree);

    // ========================================================================
    // Hidden Root
    // ========================================================================
    println!("\n=== Hidden Root (show_root=false) ===\n");

    let hidden_root_tree = Tree::new(
        TreeNode::new("Menu")
            .child(TreeNode::new("File"))
            .child(TreeNode::new("Edit"))
            .child(TreeNode::new("View"))
            .child(TreeNode::new("Help")),
    )
    .show_root(false);

    console.print_renderable(&hidden_root_tree);

    // ========================================================================
    // Collapsed Nodes
    // ========================================================================
    println!("\n=== Tree with Collapsed Node ===\n");

    let collapsed_tree = Tree::new(
        TreeNode::new("Application")
            .child(
                TreeNode::new("Components")
                    .child(TreeNode::new("Button"))
                    .child(TreeNode::new("Input")),
            )
            .child(
                TreeNode::new("Utils (collapsed)")
                    .child(TreeNode::new("helpers.rs"))
                    .child(TreeNode::new("constants.rs"))
                    .collapsed(),
            )
            .child(TreeNode::new("Tests").child(TreeNode::new("component_tests.rs"))),
    );

    console.print_renderable(&collapsed_tree);

    // ========================================================================
    // Real-World Example: Directory Structure
    // ========================================================================
    println!("\n=== Real-World: Rust Project Structure ===\n");

    let project_tree = Tree::new(
        TreeNode::new("my-rust-project/")
            .child(
                TreeNode::new("src/")
                    .child(TreeNode::new("main.rs"))
                    .child(TreeNode::new("lib.rs"))
                    .child(
                        TreeNode::new("api/")
                            .child(TreeNode::new("mod.rs"))
                            .child(TreeNode::new("handlers.rs"))
                            .child(TreeNode::new("routes.rs")),
                    )
                    .child(
                        TreeNode::new("db/")
                            .child(TreeNode::new("mod.rs"))
                            .child(TreeNode::new("models.rs"))
                            .child(TreeNode::new("queries.rs")),
                    ),
            )
            .child(
                TreeNode::new("tests/")
                    .child(TreeNode::new("api_tests.rs"))
                    .child(TreeNode::new("db_tests.rs")),
            )
            .child(TreeNode::new("Cargo.toml"))
            .child(TreeNode::new("Cargo.lock"))
            .child(TreeNode::new("README.md"))
            .child(TreeNode::new(".gitignore")),
    )
    .guides(TreeGuides::Unicode);

    console.print_renderable(&project_tree);

    // ========================================================================
    // Section Divider
    // ========================================================================
    let rule =
        Rule::with_title("End of Tree Demo").style(Style::parse("bold green").unwrap_or_default());
    console.print_renderable(&rule);

    println!();
}
