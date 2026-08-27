//! Interactive start wizard for demo_showcase.
//!
//! Provides an optional interactive setup experience when running the demo
//! in a TTY without CLI arguments. Falls back to defaults otherwise.

use std::io;
use std::sync::Arc;

use rich_rust::console::Console;
use rich_rust::interactive::Prompt;

/// Wizard configuration choices.
#[derive(Debug, Clone, Default)]
pub struct WizardChoices {
    /// Run a specific scene instead of the full demo.
    pub scene: Option<String>,
    /// Use quick mode (reduced sleeps).
    pub quick: bool,
    /// Export to files after running.
    pub export: bool,
}

/// Run the interactive start wizard.
///
/// Returns `Some(choices)` if the wizard ran successfully, or `None` if the
/// wizard should be skipped (non-interactive mode, or user declined).
///
/// # Arguments
/// * `console` - Console for prompts
/// * `interactive_allowed` - Whether interactive features are enabled
/// * `scene_names` - Available scene names for selection
pub fn run_wizard(
    console: &Arc<Console>,
    interactive_allowed: bool,
    scene_names: &[&str],
) -> Option<WizardChoices> {
    // Skip wizard if interactive mode is disabled
    if !interactive_allowed {
        return None;
    }

    // Skip wizard if not a TTY
    if !console.is_terminal() {
        return None;
    }

    // Show wizard header
    console.print("");
    console.print("[bold brand.accent]Welcome to Nebula Deploy![/]");
    console.print("[dim]Let's configure your demo experience.[/]");
    console.print("");

    let mut choices = WizardChoices::default();

    // Question 1: Run mode (full demo or specific scene)
    match ask_run_mode(console, scene_names) {
        Ok(Some(scene)) => choices.scene = Some(scene),
        Ok(None) => {}         // Full demo
        Err(_) => return None, // User cancelled or error
    }

    // Question 2: Speed
    match ask_speed(console) {
        Ok(quick) => choices.quick = quick,
        Err(_) => return None,
    }

    // Question 3: Export
    match ask_export(console) {
        Ok(export) => choices.export = export,
        Err(_) => return None,
    }

    console.print("");
    console.print("[dim]Starting demo...[/]");
    console.print("");

    Some(choices)
}

/// Ask user whether to run full demo or a specific scene.
fn ask_run_mode(console: &Console, scene_names: &[&str]) -> io::Result<Option<String>> {
    let prompt = Prompt::new("[bold]Run mode[/]")
        .default("full")
        .markup(true);

    console.print("[dim]Options: 'full' for complete demo, or a scene name[/]");

    // Show available scenes hint
    if !scene_names.is_empty() {
        let preview: Vec<_> = scene_names.iter().take(5).copied().collect();
        let hint = if scene_names.len() > 5 {
            format!(
                "{}, ... ({} more)",
                preview.join(", "),
                scene_names.len() - 5
            )
        } else {
            preview.join(", ")
        };
        console.print(&format!("[dim]Scenes: {hint}[/]"));
    }

    let answer = prompt
        .ask(console)
        .map_err(|e| io::Error::other(e.to_string()))?;

    let answer = answer.trim().to_lowercase();
    if answer.is_empty() || answer == "full" || answer == "all" {
        Ok(None)
    } else if scene_names.contains(&answer.as_str()) {
        Ok(Some(answer))
    } else {
        // Unknown scene, fall back to full
        console.print(&format!(
            "[dim]Unknown scene '{answer}', running full demo.[/]"
        ));
        Ok(None)
    }
}

/// Ask user about demo speed.
fn ask_speed(console: &Console) -> io::Result<bool> {
    let prompt = Prompt::new("[bold]Speed[/]").default("normal").markup(true);

    console.print("[dim]Options: 'normal' for full experience, 'quick' for faster run[/]");

    let answer = prompt
        .ask(console)
        .map_err(|e| io::Error::other(e.to_string()))?;

    let answer = answer.trim().to_lowercase();
    Ok(answer == "quick" || answer == "fast" || answer == "q")
}

/// Ask user about export.
fn ask_export(console: &Console) -> io::Result<bool> {
    let prompt = Prompt::new("[bold]Export to HTML/SVG?[/]")
        .default("no")
        .markup(true);

    console.print("[dim]Options: 'yes' to save output files, 'no' to skip[/]");

    let answer = prompt
        .ask(console)
        .map_err(|e| io::Error::other(e.to_string()))?;

    let answer = answer.trim().to_lowercase();
    Ok(answer == "yes" || answer == "y" || answer == "true")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wizard_choices_default() {
        let choices = WizardChoices::default();
        assert!(choices.scene.is_none());
        assert!(!choices.quick);
        assert!(!choices.export);
    }

    #[test]
    fn wizard_skips_when_not_interactive() {
        let console = Console::builder().force_terminal(false).build().shared();

        let result = run_wizard(&console, false, &["hero", "outro"]);
        assert!(result.is_none());
    }

    #[test]
    fn wizard_skips_when_not_tty() {
        let console = Console::builder().force_terminal(false).build().shared();

        let result = run_wizard(&console, true, &["hero", "outro"]);
        assert!(result.is_none());
    }
}
