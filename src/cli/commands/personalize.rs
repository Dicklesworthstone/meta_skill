//! ms personalize - Adapt skills to user coding style
//!
//! Personalizes generic skills by adapting examples and terminology
//! to match user coding patterns extracted from CASS sessions.

use clap::{Args, Subcommand};
use colored::Colorize;

use crate::app::AppContext;
use crate::cass::CassClient;
use crate::dedup::{
    CaseStyle, CodePattern, CommentStyle, NamingConvention, Personalizer, PersonalizedSkill,
    StyleProfile,
};
use crate::error::Result;

#[derive(Args, Debug)]
pub struct PersonalizeArgs {
    #[command(subcommand)]
    pub command: PersonalizeCommand,
}

#[derive(Subcommand, Debug)]
pub enum PersonalizeCommand {
    /// Personalize a specific skill
    Skill(SkillArgs),
    /// Extract style profile from CASS sessions
    Extract(ExtractArgs),
    /// Show current style profile
    Show(ShowArgs),
}

#[derive(Args, Debug)]
pub struct SkillArgs {
    /// Skill ID to personalize
    pub skill_id: String,

    /// Output path for personalized skill (default: stdout)
    #[arg(long, short)]
    pub output: Option<String>,

    /// Style profile path (default: extract from recent sessions)
    #[arg(long)]
    pub style: Option<String>,
}

#[derive(Args, Debug)]
pub struct ExtractArgs {
    /// Number of recent CASS sessions to analyze
    #[arg(long, default_value = "10")]
    pub sessions: usize,

    /// CASS project filter
    #[arg(long)]
    pub project: Option<String>,

    /// Output path for style profile (default: ~/.config/ms/style.json)
    #[arg(long, short)]
    pub output: Option<String>,
}

#[derive(Args, Debug)]
pub struct ShowArgs {
    /// Style profile path (default: ~/.config/ms/style.json)
    #[arg(long)]
    pub style: Option<String>,
}

pub fn run(ctx: &AppContext, args: &PersonalizeArgs) -> Result<()> {
    match &args.command {
        PersonalizeCommand::Skill(skill_args) => run_skill(ctx, skill_args),
        PersonalizeCommand::Extract(extract_args) => run_extract(ctx, extract_args),
        PersonalizeCommand::Show(show_args) => run_show(ctx, show_args),
    }
}

fn run_skill(ctx: &AppContext, args: &SkillArgs) -> Result<()> {
    let db = ctx.db.as_ref();

    // Load the skill
    let skill = db.get_skill(&args.skill_id)?;
    let skill = skill.ok_or_else(|| {
        crate::error::MsError::SkillNotFound(format!("skill not found: {}", args.skill_id))
    })?;

    // Load style profile
    let style = load_style_profile(ctx, args.style.as_deref())?;

    // Create personalizer and adapt skill
    let personalizer = Personalizer::new(style);

    if !personalizer.should_personalize(&skill) {
        if ctx.robot_mode {
            let output = serde_json::json!({
                "status": "skipped",
                "skill_id": skill.id,
                "reason": "no style preferences to apply or skill not suitable for personalization"
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            println!(
                "{} Skill '{}' does not need personalization (no style preferences to apply)",
                "⚠".yellow(),
                skill.name
            );
        }
        return Ok(());
    }

    let personalized = personalizer.personalize(&skill);

    if ctx.robot_mode {
        output_personalized_robot(&personalized)?;
    } else {
        output_personalized_human(&personalized, args.output.as_deref())?;
    }

    Ok(())
}

fn run_extract(ctx: &AppContext, args: &ExtractArgs) -> Result<()> {
    // Check if CASS is available
    let cass = CassClient::new();

    if !cass.is_available() {
        if ctx.robot_mode {
            let output = serde_json::json!({
                "status": "error",
                "error": "CASS server is not available"
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            eprintln!("{} CASS server is not available", "✗".red());
            eprintln!("  Start CASS or configure CASS_URL in your config");
        }
        return Ok(());
    }

    if !ctx.robot_mode {
        println!("{}", "Extracting style profile from CASS sessions...".bold());
        println!();
    }

    // Query recent sessions (use project filter as search query if provided)
    let query = args.project.as_deref().unwrap_or("");
    let sessions = cass.search(query, args.sessions)?;

    if sessions.is_empty() {
        if ctx.robot_mode {
            let output = serde_json::json!({
                "status": "error",
                "error": "no sessions found"
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            eprintln!("{} No CASS sessions found to extract style from", "✗".red());
        }
        return Ok(());
    }

    // Extract style from sessions
    let style = extract_style_from_sessions(&cass, &sessions)?;

    // Save or output the style profile
    let output_path = args.output.clone().unwrap_or_else(|| {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("ms");
        config_dir.join("style.json").to_string_lossy().to_string()
    });

    if ctx.robot_mode {
        let output = serde_json::json!({
            "status": "ok",
            "sessions_analyzed": sessions.len(),
            "style": style,
            "output_path": output_path,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        // Save to file
        let json = serde_json::to_string_pretty(&style)?;
        if let Some(parent) = std::path::Path::new(&output_path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&output_path, &json)?;

        println!(
            "{} Style profile extracted from {} sessions",
            "✓".green(),
            sessions.len()
        );
        println!("  Saved to: {output_path}");
        println!();
        print_style_summary(&style);
    }

    Ok(())
}

fn run_show(ctx: &AppContext, args: &ShowArgs) -> Result<()> {
    let style = load_style_profile(ctx, args.style.as_deref())?;

    if ctx.robot_mode {
        let output = serde_json::json!({
            "status": "ok",
            "style": style,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", "Current Style Profile".bold());
        println!();
        print_style_summary(&style);
    }

    Ok(())
}

/// Load style profile from file or extract from sessions
fn load_style_profile(ctx: &AppContext, path: Option<&str>) -> Result<StyleProfile> {
    if let Some(path) = path {
        // Load from specified path
        let content = std::fs::read_to_string(path).map_err(|e| {
            crate::error::MsError::Config(format!("failed to read style profile: {e}"))
        })?;
        let style: StyleProfile = serde_json::from_str(&content).map_err(|e| {
            crate::error::MsError::Config(format!("failed to parse style profile: {e}"))
        })?;
        return Ok(style);
    }

    // Try default path
    let default_path = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("ms")
        .join("style.json");

    if default_path.exists() {
        let content = std::fs::read_to_string(&default_path).map_err(|e| {
            crate::error::MsError::Config(format!("failed to read style profile: {e}"))
        })?;
        let style: StyleProfile = serde_json::from_str(&content).map_err(|e| {
            crate::error::MsError::Config(format!("failed to parse style profile: {e}"))
        })?;
        return Ok(style);
    }

    // Return default style if no profile exists
    if !ctx.robot_mode {
        eprintln!(
            "{} No style profile found, using defaults",
            "⚠".yellow()
        );
        eprintln!(
            "  Run 'ms personalize extract' to create a profile from CASS sessions"
        );
    }

    Ok(StyleProfile::default())
}

/// Extract style profile from CASS sessions
fn extract_style_from_sessions(
    cass: &CassClient,
    sessions: &[crate::cass::SessionMatch],
) -> Result<StyleProfile> {
    let mut style = StyleProfile::default();
    let mut code_samples: Vec<String> = Vec::new();

    // Collect code samples from sessions
    for session_match in sessions.iter().take(10) {
        if let Ok(session) = cass.get_session(&session_match.session_id) {
            for msg in &session.messages {
                // Look for code blocks in assistant messages
                if msg.role == "assistant" {
                    code_samples.extend(extract_code_blocks(&msg.content));
                }
            }
        }
    }

    // Analyze code samples for patterns
    if !code_samples.is_empty() {
        style.naming = analyze_naming_conventions(&code_samples);
        style.patterns = analyze_code_patterns(&code_samples);
        style.comment_style = analyze_comment_style(&code_samples);
    }

    Ok(style)
}

/// Extract code blocks from markdown content
fn extract_code_blocks(content: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut in_block = false;
    let mut current_block = String::new();

    for line in content.lines() {
        if line.starts_with("```") {
            if in_block {
                blocks.push(current_block.clone());
                current_block.clear();
                in_block = false;
            } else {
                in_block = true;
            }
        } else if in_block {
            current_block.push_str(line);
            current_block.push('\n');
        }
    }

    blocks
}

/// Analyze code samples for naming conventions
fn analyze_naming_conventions(samples: &[String]) -> NamingConvention {
    let mut snake_count = 0;
    let mut camel_count = 0;

    // Simple heuristic: count underscore-separated vs camelCase identifiers
    for sample in samples {
        // Count snake_case patterns (word_word)
        snake_count += sample.matches('_').count();

        // Count camelCase patterns (lowercase followed by uppercase)
        let chars: Vec<char> = sample.chars().collect();
        for i in 1..chars.len() {
            if chars[i - 1].is_lowercase() && chars[i].is_uppercase() {
                camel_count += 1;
            }
        }
    }

    NamingConvention {
        variable_case: if snake_count > camel_count * 2 {
            CaseStyle::SnakeCase
        } else if camel_count > snake_count * 2 {
            CaseStyle::CamelCase
        } else {
            CaseStyle::SnakeCase // Default to snake_case
        },
        function_case: CaseStyle::SnakeCase,
        use_abbreviations: false,
        abbreviations: vec![],
    }
}

/// Analyze code samples for common patterns
fn analyze_code_patterns(samples: &[String]) -> Vec<CodePattern> {
    let mut patterns = Vec::new();

    let all_code = samples.join("\n");

    // Check for early return pattern
    let early_return_count = all_code.matches("return Err").count()
        + all_code.matches("return None").count()
        + all_code.matches("return;").count();
    if early_return_count > 2 {
        patterns.push(CodePattern {
            name: "early_return".to_string(),
            description: "Return early from functions on error/invalid cases".to_string(),
            example: Some("if !valid { return Err(...); }".to_string()),
            preference_strength: (early_return_count as f32 / samples.len() as f32).min(1.0),
        });
    }

    // Check for guard clause pattern
    let guard_count = all_code.matches("if !").count() + all_code.matches("if let Err").count();
    if guard_count > 2 {
        patterns.push(CodePattern {
            name: "guard_clause".to_string(),
            description: "Use guard clauses to handle edge cases first".to_string(),
            example: None,
            preference_strength: (guard_count as f32 / samples.len() as f32).min(1.0),
        });
    }

    // Check for Result/Option chaining
    let chain_count = all_code.matches(".map(").count()
        + all_code.matches(".and_then(").count()
        + all_code.matches(".ok_or").count();
    if chain_count > 3 {
        patterns.push(CodePattern {
            name: "method_chaining".to_string(),
            description: "Chain Result/Option methods instead of match expressions".to_string(),
            example: None,
            preference_strength: (chain_count as f32 / (samples.len() * 2) as f32).min(1.0),
        });
    }

    patterns
}

/// Analyze code samples for comment style
fn analyze_comment_style(samples: &[String]) -> CommentStyle {
    let all_code = samples.join("\n");

    let doc_comment_count = all_code.matches("///").count() + all_code.matches("//!").count();
    let todo_count = all_code.matches("TODO").count() + all_code.matches("FIXME").count();

    CommentStyle {
        use_doc_comments: doc_comment_count > 2,
        inline_style: crate::dedup::InlineCommentStyle::DoubleSlash,
        use_todo_markers: todo_count > 0,
    }
}

fn output_personalized_robot(personalized: &PersonalizedSkill) -> Result<()> {
    let output = serde_json::json!({
        "status": "ok",
        "original_id": personalized.original_id,
        "original_name": personalized.original_name,
        "adaptations_applied": personalized.adaptations_applied,
        "adapted_content": personalized.adapted_content,
    });
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

fn output_personalized_human(personalized: &PersonalizedSkill, output_path: Option<&str>) -> Result<()> {
    if personalized.adaptations_applied.is_empty() {
        println!(
            "{} No adaptations were applied to '{}'",
            "⚠".yellow(),
            personalized.original_name
        );
        return Ok(());
    }

    println!(
        "{} Personalized: {}",
        "✓".green(),
        personalized.original_name.bold()
    );
    println!();
    println!("{}", "Adaptations applied:".dimmed());
    for adaptation in &personalized.adaptations_applied {
        println!("  • {adaptation}");
    }
    println!();

    if let Some(path) = output_path {
        std::fs::write(path, &personalized.adapted_content)?;
        println!("Saved to: {path}");
    } else {
        println!("{}", "Adapted content:".dimmed());
        println!("{}", "-".repeat(40));
        println!("{}", personalized.adapted_content);
    }

    Ok(())
}

fn print_style_summary(style: &StyleProfile) {
    println!("{}", "Naming Conventions:".cyan());
    println!(
        "  Variable case: {:?}",
        style.naming.variable_case
    );
    println!(
        "  Function case: {:?}",
        style.naming.function_case
    );
    println!(
        "  Use abbreviations: {}",
        style.naming.use_abbreviations
    );
    println!();

    println!("{}", "Code Patterns:".cyan());
    if style.patterns.is_empty() {
        println!("  (none detected)");
    } else {
        for pattern in &style.patterns {
            println!(
                "  • {} (strength: {:.0}%)",
                pattern.name,
                pattern.preference_strength * 100.0
            );
            println!("    {}", pattern.description.dimmed());
        }
    }
    println!();

    println!("{}", "Comment Style:".cyan());
    println!(
        "  Doc comments: {}",
        if style.comment_style.use_doc_comments {
            "yes"
        } else {
            "no"
        }
    );
    println!("  Inline style: {:?}", style.comment_style.inline_style);
    println!(
        "  TODO markers: {}",
        if style.comment_style.use_todo_markers {
            "yes"
        } else {
            "no"
        }
    );
    println!();

    println!("{}", "Tech Preferences:".cyan());
    if style.tech_preferences.is_empty() {
        println!("  (none detected)");
    } else {
        for tech in &style.tech_preferences {
            println!("  • {tech}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct TestCli {
        #[command(subcommand)]
        command: PersonalizeCommand,
    }

    #[test]
    fn test_skill_args_parse() {
        let cli = TestCli::parse_from(["test", "skill", "my-skill"]);
        if let PersonalizeCommand::Skill(args) = cli.command {
            assert_eq!(args.skill_id, "my-skill");
        } else {
            panic!("Expected Skill command");
        }
    }

    #[test]
    fn test_extract_args_parse() {
        let cli = TestCli::parse_from(["test", "extract", "--sessions", "20"]);
        if let PersonalizeCommand::Extract(args) = cli.command {
            assert_eq!(args.sessions, 20);
        } else {
            panic!("Expected Extract command");
        }
    }

    #[test]
    fn test_extract_code_blocks() {
        let content = r#"Here is some code:

```rust
fn main() {
    println!("Hello");
}
```

And more text.

```python
def foo():
    pass
```
"#;
        let blocks = extract_code_blocks(content);
        assert_eq!(blocks.len(), 2);
        assert!(blocks[0].contains("fn main()"));
        assert!(blocks[1].contains("def foo()"));
    }

    #[test]
    fn test_analyze_naming_conventions_snake() {
        let samples = vec![
            "let user_name = get_user_name();".to_string(),
            "fn calculate_total_price() {}".to_string(),
        ];
        let naming = analyze_naming_conventions(&samples);
        assert_eq!(naming.variable_case, CaseStyle::SnakeCase);
    }

    #[test]
    fn test_analyze_naming_conventions_camel() {
        let samples = vec![
            "let userName = getUserName();".to_string(),
            "function calculateTotalPrice() {}".to_string(),
        ];
        let naming = analyze_naming_conventions(&samples);
        assert_eq!(naming.variable_case, CaseStyle::CamelCase);
    }
}
