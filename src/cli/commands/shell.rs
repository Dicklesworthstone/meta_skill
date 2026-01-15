//! ms shell - Print shell integration hooks for suggestions.

use clap::Args;
use colored::Colorize;

use crate::app::AppContext;
use crate::cli::output::{HumanLayout, emit_json};
use crate::error::{MsError, Result};

#[derive(Args, Debug)]
pub struct ShellArgs {
    /// Target shell: bash, zsh, fish
    #[arg(long)]
    pub shell: Option<String>,

    /// Minimum seconds between suggestions (rate limit)
    #[arg(long, default_value = "30")]
    pub interval_seconds: u64,
}

pub fn run(ctx: &AppContext, args: &ShellArgs) -> Result<()> {
    let shell = resolve_shell(args.shell.as_deref())?;
    let snippet = build_snippet(&shell, args.interval_seconds);

    if ctx.robot_mode {
        let payload = serde_json::json!({
            "status": "ok",
            "shell": shell,
            "interval_seconds": args.interval_seconds,
            "snippet": snippet,
        });
        emit_json(&payload)
    } else {
        let mut layout = HumanLayout::new();
        layout
            .title("Shell Integration")
            .section("Target")
            .kv("Shell", &shell)
            .kv("Interval", &format!("{}s", args.interval_seconds))
            .blank()
            .section("Install")
            .bullet("Add the snippet below to your shell rc file (e.g. ~/.bashrc, ~/.zshrc, ~/.config/fish/config.fish)")
            .bullet("Restart your shell after adding it")
            .blank()
            .section("Remove")
            .bullet("Delete the snippet from your rc file")
            .blank()
            .section("Snippet");
        crate::cli::output::emit_human(layout);
        println!();
        println!("{}", "# ---- ms shell hook ----".dimmed());
        println!("{snippet}");
        println!("{}", "# ---- end ms shell hook ----".dimmed());
        Ok(())
    }
}

fn resolve_shell(input: Option<&str>) -> Result<String> {
    if let Some(value) = input {
        return normalize_shell(value)
            .ok_or_else(|| MsError::ValidationFailed(format!("unsupported shell: {value}")));
    }

    let env_shell = std::env::var("SHELL").unwrap_or_default();
    let detected = env_shell.split('/').last().unwrap_or("").to_string();
    normalize_shell(&detected)
        .ok_or_else(|| MsError::ValidationFailed("unable to detect shell; use --shell".to_string()))
}

fn normalize_shell(value: &str) -> Option<String> {
    let lower = value.to_ascii_lowercase();
    match lower.as_str() {
        "bash" | "zsh" | "fish" => Some(lower),
        _ => None,
    }
}

fn build_snippet(shell: &str, interval_seconds: u64) -> String {
    match shell {
        "bash" => build_bash_snippet(interval_seconds),
        "zsh" => build_zsh_snippet(interval_seconds),
        "fish" => build_fish_snippet(interval_seconds),
        _ => String::new(),
    }
}

fn build_bash_snippet(interval_seconds: u64) -> String {
    format!(
        r#"MS_SUGGEST_INTERVAL=${{MS_SUGGEST_INTERVAL:-{interval_seconds}}}
ms_suggest_prompt() {{
  local now
  now=$(date +%s)
  if [[ -n "${{MS_SUGGEST_LAST:-}}" ]]; then
    if (( now - MS_SUGGEST_LAST < MS_SUGGEST_INTERVAL )); then
      return
    fi
  fi
  MS_SUGGEST_LAST=$now
  ms suggest --cwd "$PWD"
}}
PROMPT_COMMAND="ms_suggest_prompt${{PROMPT_COMMAND:+; $PROMPT_COMMAND}}"
"#
    )
}

fn build_zsh_snippet(interval_seconds: u64) -> String {
    format!(
        r#"typeset -g MS_SUGGEST_INTERVAL=${{MS_SUGGEST_INTERVAL:-{interval_seconds}}}
if [[ -z "${{MS_SUGGEST_LAST:-}}" ]]; then
  typeset -g MS_SUGGEST_LAST=0
fi
ms_suggest_precmd() {{
  local now
  now=$(date +%s)
  if (( now - MS_SUGGEST_LAST < MS_SUGGEST_INTERVAL )); then
    return
  fi
  MS_SUGGEST_LAST=$now
  ms suggest --cwd "$PWD"
}}
autoload -U add-zsh-hook
add-zsh-hook precmd ms_suggest_precmd
"#
    )
}

fn build_fish_snippet(interval_seconds: u64) -> String {
    format!(
        r#"set -g MS_SUGGEST_INTERVAL {interval_seconds}
if not set -q MS_SUGGEST_LAST
  set -g MS_SUGGEST_LAST 0
end
function __ms_suggest_prompt --on-event fish_prompt
  set -l now (date +%s)
  if test (math $now - $MS_SUGGEST_LAST) -lt $MS_SUGGEST_INTERVAL
    return
  end
  set -g MS_SUGGEST_LAST $now
  ms suggest --cwd "$PWD"
end
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_shell_accepts_known() {
        assert_eq!(normalize_shell("bash"), Some("bash".to_string()));
        assert_eq!(normalize_shell("zsh"), Some("zsh".to_string()));
        assert_eq!(normalize_shell("fish"), Some("fish".to_string()));
    }

    #[test]
    fn normalize_shell_rejects_unknown() {
        assert!(normalize_shell("nu").is_none());
    }

    #[test]
    fn snippet_contains_interval() {
        let snippet = build_bash_snippet(42);
        assert!(snippet.contains("MS_SUGGEST_INTERVAL"));
        assert!(snippet.contains("42"));
    }
}
