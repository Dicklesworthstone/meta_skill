//! ms config - Manage configuration

use clap::Args;

use std::path::PathBuf;

use crate::app::AppContext;
use crate::cli::output;
use crate::cli::output::OutputFormat;
use crate::config::Config;
use crate::error::Result;

/// `ms config` accepts both the bare git/gh-style forms and explicit verbs:
///
/// - `ms config`                       -> list all configuration
/// - `ms config --list`                -> list all configuration
/// - `ms config show` / `list`         -> list all configuration (verb form)
/// - `ms config <key>`                 -> read a key
/// - `ms config <key> <value>`         -> write a key
/// - `ms config get <key>`             -> read a key (verb form)
/// - `ms config set <key> <value>`     -> write a key (verb form)
/// - `ms config unset <key>`           -> remove a key
/// - `ms config --unset <key>`         -> remove a key
///
/// The verb forms exist so the muscle-memory `ms config get <key>` no longer
/// silently writes a junk `get = "<key>"` top-level entry (issue #142). Writes
/// are validated against the known config schema and echo a confirmation.
#[derive(Args, Debug)]
pub struct ConfigArgs {
    /// Configuration key, or a `get`/`set`/`unset` verb
    pub key: Option<String>,

    /// Value to set (or the key, when the first token is a `get`/`set`/`unset` verb)
    pub value: Option<String>,

    /// Value to set, when the first token is the `set` verb
    pub extra: Option<String>,

    /// List all configuration
    #[arg(long)]
    pub list: bool,

    /// Unset a configuration key
    #[arg(long)]
    pub unset: bool,
}

/// A resolved `ms config` invocation after verb/positional disambiguation.
#[derive(Debug, PartialEq, Eq)]
enum ConfigAction {
    List,
    Get { key: String },
    Set { key: String, value: String },
    Unset { key: String },
}

fn resolve_action(args: &ConfigArgs) -> Result<ConfigAction> {
    let cfg_err = |msg: &str| crate::error::MsError::Config(msg.to_string());

    // `--unset <key>` (flag form). The key rides in the first positional; no
    // value is permitted.
    if args.unset {
        if args.value.is_some() || args.extra.is_some() {
            return Err(cfg_err("cannot combine --unset with a value"));
        }
        let key = args
            .key
            .clone()
            .ok_or_else(|| cfg_err("--unset requires a key"))?;
        return Ok(ConfigAction::Unset { key });
    }

    match args.key.as_deref() {
        None => Ok(ConfigAction::List),
        // `show`/`list` verbs render the whole configuration (git-style).
        Some("show" | "list") => {
            if args.value.is_some() || args.extra.is_some() {
                return Err(cfg_err("`config show` takes no arguments"));
            }
            Ok(ConfigAction::List)
        }
        // Verb forms: the first token is a verb, operands shift right by one.
        Some("get") => {
            let key = args
                .value
                .clone()
                .ok_or_else(|| cfg_err("`config get` requires a key"))?;
            if args.extra.is_some() {
                return Err(cfg_err("`config get` takes exactly one key"));
            }
            Ok(ConfigAction::Get { key })
        }
        Some("set") => {
            let key = args
                .value
                .clone()
                .ok_or_else(|| cfg_err("`config set` requires a key and a value"))?;
            let value = args
                .extra
                .clone()
                .ok_or_else(|| cfg_err("`config set` requires a value"))?;
            Ok(ConfigAction::Set { key, value })
        }
        Some("unset") => {
            let key = args
                .value
                .clone()
                .ok_or_else(|| cfg_err("`config unset` requires a key"))?;
            if args.extra.is_some() {
                return Err(cfg_err("`config unset` takes exactly one key"));
            }
            Ok(ConfigAction::Unset { key })
        }
        // Bare forms: <key> [value].
        Some(key) => {
            if args.extra.is_some() {
                return Err(cfg_err(
                    "too many arguments (use `config set <key> <value>`)",
                ));
            }
            match args.value.clone() {
                Some(value) => Ok(ConfigAction::Set {
                    key: key.to_string(),
                    value,
                }),
                None => Ok(ConfigAction::Get {
                    key: key.to_string(),
                }),
            }
        }
    }
}

pub fn run(ctx: &AppContext, args: &ConfigArgs) -> Result<()> {
    let ctx = ConfigContext {
        config: ctx.config.clone(),
        config_path: ctx.config_path.clone(),
        robot_mode: ctx.output_format != OutputFormat::Human,
    };

    if args.list {
        return emit_config(&ctx);
    }

    match resolve_action(args)? {
        ConfigAction::List => emit_config(&ctx),
        ConfigAction::Get { key } => get_key(&ctx, &key),
        ConfigAction::Set { key, value } => set_key(&ctx, &key, &value),
        ConfigAction::Unset { key } => unset_key(&ctx, &key),
    }
}

struct ConfigContext {
    config: Config,
    config_path: PathBuf,
    robot_mode: bool,
}

fn emit_config(ctx: &ConfigContext) -> Result<()> {
    if ctx.robot_mode {
        return output::emit_json(&ctx.config);
    }

    let rendered = toml::to_string_pretty(&ctx.config)
        .map_err(|err| crate::error::MsError::Config(format!("render config: {err}")))?;
    println!("{rendered}");
    Ok(())
}

fn get_key(ctx: &ConfigContext, key: &str) -> Result<()> {
    let value = config_value_at(&ctx.config, key)?;
    if ctx.robot_mode {
        return output::emit_json(&value);
    }
    println!("{}", format_value(&value));
    Ok(())
}

fn set_key(ctx: &ConfigContext, key: &str, raw_value: &str) -> Result<()> {
    let mut doc = load_config_doc(&ctx.config_path)?;
    let value = parse_value(raw_value)?;
    set_path(&mut doc, key, value.clone())?;
    // Reject unknown/typo keys BEFORE persisting so a mistyped key (or a stray
    // `get`/`set` verb that slipped through) can never silently land as a junk
    // top-level entry (issue #142).
    validate_key_known(&doc, key)?;
    write_config_doc(&ctx.config_path, &doc)?;

    if ctx.robot_mode {
        let confirmation = serde_json::json!({
            "status": "ok",
            "action": "set",
            "key": key,
            "value": value,
        });
        return output::emit_json(&confirmation);
    }
    println!("Set {key} = {}", format_value(&value));
    Ok(())
}

/// Validate that `key` is a real configuration key.
///
/// The document (with the new value applied) is round-tripped through the typed
/// [`Config`]: because the config sub-tables ignore unknown fields on
/// deserialization, a typo'd or otherwise unknown key is dropped and will not
/// reappear when the typed config is re-serialized — so its path lookup fails
/// and we reject the write. Known keys (including optional ones like
/// `output.theme` that are absent from a default config) survive the round-trip
/// and are accepted. A type mismatch surfaces here as a parse error instead of
/// corrupting the config file.
fn validate_key_known(doc: &toml::Value, key: &str) -> Result<()> {
    let typed: Config = doc.clone().try_into().map_err(|err| {
        crate::error::MsError::Config(format!("invalid value for `{key}`: {err}"))
    })?;
    let normalized = toml::Value::try_from(&typed)
        .map_err(|err| crate::error::MsError::Config(format!("serialize config: {err}")))?;
    if get_path(&normalized, key).is_err() {
        return Err(crate::error::MsError::Config(format!(
            "unknown config key: {key} (run `ms config --list` to see available keys)"
        )));
    }
    Ok(())
}

fn unset_key(ctx: &ConfigContext, key: &str) -> Result<()> {
    let mut doc = load_config_doc(&ctx.config_path)?;
    unset_path(&mut doc, key)?;
    write_config_doc(&ctx.config_path, &doc)?;
    Ok(())
}

fn load_config_doc(path: &std::path::Path) -> Result<toml::Value> {
    if path.exists() {
        let raw = std::fs::read_to_string(path)
            .map_err(|err| crate::error::MsError::Config(format!("read config: {err}")))?;
        let doc = toml::from_str(&raw)
            .map_err(|err| crate::error::MsError::Config(format!("parse config: {err}")))?;
        Ok(doc)
    } else {
        Ok(toml::Value::Table(toml::map::Map::new()))
    }
}

fn write_config_doc(path: &std::path::Path, doc: &toml::Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| crate::error::MsError::Config(format!("create config dir: {err}")))?;
    }
    let rendered = toml::to_string_pretty(doc)
        .map_err(|err| crate::error::MsError::Config(format!("render config: {err}")))?;
    std::fs::write(path, rendered)
        .map_err(|err| crate::error::MsError::Config(format!("write config: {err}")))?;
    Ok(())
}

fn parse_value(raw: &str) -> Result<toml::Value> {
    let direct = format!("value = {raw}");
    if let Ok(value) = toml::from_str::<toml::Value>(&direct) {
        if let Some(parsed) = value.get("value") {
            return Ok(parsed.clone());
        }
    }

    let quoted = format!("value = {}", toml::Value::String(raw.to_string()));
    let parsed = toml::from_str::<toml::Value>(&quoted)
        .map_err(|err| crate::error::MsError::Config(format!("parse value: {err}")))?;
    parsed
        .get("value")
        .cloned()
        .ok_or_else(|| crate::error::MsError::Config("parse value: missing".to_string()))
}

fn config_value_at(config: &Config, key: &str) -> Result<toml::Value> {
    let doc = toml::Value::try_from(config)
        .map_err(|err| crate::error::MsError::Config(format!("serialize config: {err}")))?;
    get_path(&doc, key)
}

fn get_path(doc: &toml::Value, key: &str) -> Result<toml::Value> {
    let mut current = doc;
    for part in key.split('.') {
        current = current
            .get(part)
            .ok_or_else(|| crate::error::MsError::Config(format!("unknown key: {key}")))?;
    }
    Ok(current.clone())
}

fn set_path(doc: &mut toml::Value, key: &str, value: toml::Value) -> Result<()> {
    let parts: Vec<&str> = key.split('.').collect();
    if parts.is_empty() {
        return Err(crate::error::MsError::Config("empty key".to_string()));
    }

    ensure_table(doc)?;
    let mut current = doc;
    for part in &parts[..parts.len() - 1] {
        let table = current
            .as_table_mut()
            .ok_or_else(|| crate::error::MsError::Config("invalid config table".to_string()))?;
        current = table
            .entry((*part).to_string())
            .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
        ensure_table(current)?;
    }

    let table = current
        .as_table_mut()
        .ok_or_else(|| crate::error::MsError::Config("invalid config table".to_string()))?;
    table.insert(parts[parts.len() - 1].to_string(), value);
    Ok(())
}

fn unset_path(doc: &mut toml::Value, key: &str) -> Result<()> {
    let parts: Vec<&str> = key.split('.').collect();
    if parts.is_empty() {
        return Err(crate::error::MsError::Config("empty key".to_string()));
    }

    ensure_table(doc)?;
    let mut current = doc;
    for part in &parts[..parts.len() - 1] {
        let table = current
            .as_table_mut()
            .ok_or_else(|| crate::error::MsError::Config("invalid config table".to_string()))?;
        current = table
            .get_mut(*part)
            .ok_or_else(|| crate::error::MsError::Config(format!("unknown key: {key}")))?;
        ensure_table(current)?;
    }

    let table = current
        .as_table_mut()
        .ok_or_else(|| crate::error::MsError::Config("invalid config table".to_string()))?;
    table.remove(parts[parts.len() - 1]);
    Ok(())
}

fn ensure_table(value: &mut toml::Value) -> Result<()> {
    if value.is_table() {
        Ok(())
    } else {
        Err(crate::error::MsError::Config(
            "config path is not a table".to_string(),
        ))
    }
}

fn format_value(value: &toml::Value) -> String {
    match value {
        toml::Value::String(s) => s.clone(),
        _ => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{ConfigAction, ConfigArgs, ConfigContext, get_key, set_key, unset_key};
    use crate::config::Config;

    fn args(key: Option<&str>, value: Option<&str>, extra: Option<&str>) -> ConfigArgs {
        ConfigArgs {
            key: key.map(str::to_string),
            value: value.map(str::to_string),
            extra: extra.map(str::to_string),
            list: false,
            unset: false,
        }
    }

    fn ctx(dir: &std::path::Path) -> ConfigContext {
        ConfigContext {
            config: Config::default(),
            config_path: dir.join("config.toml"),
            robot_mode: false,
        }
    }

    // --- Verb / positional disambiguation ---

    #[test]
    fn bare_key_only_is_a_get() {
        let action =
            super::resolve_action(&args(Some("search.use_embeddings"), None, None)).unwrap();
        assert_eq!(
            action,
            ConfigAction::Get {
                key: "search.use_embeddings".to_string()
            }
        );
    }

    #[test]
    fn bare_key_value_is_a_set() {
        let action =
            super::resolve_action(&args(Some("search.use_embeddings"), Some("false"), None))
                .unwrap();
        assert_eq!(
            action,
            ConfigAction::Set {
                key: "search.use_embeddings".to_string(),
                value: "false".to_string()
            }
        );
    }

    #[test]
    fn get_verb_reads_the_following_key() {
        // The footgun form: `ms config get search.use_embeddings` must resolve to
        // a READ of `search.use_embeddings`, NOT a write of `get = "..."`.
        let action =
            super::resolve_action(&args(Some("get"), Some("search.use_embeddings"), None)).unwrap();
        assert_eq!(
            action,
            ConfigAction::Get {
                key: "search.use_embeddings".to_string()
            }
        );
    }

    #[test]
    fn set_verb_writes_the_following_key_value() {
        let action = super::resolve_action(&args(
            Some("set"),
            Some("search.use_embeddings"),
            Some("false"),
        ))
        .unwrap();
        assert_eq!(
            action,
            ConfigAction::Set {
                key: "search.use_embeddings".to_string(),
                value: "false".to_string()
            }
        );
    }

    #[test]
    fn get_verb_without_key_errors() {
        assert!(super::resolve_action(&args(Some("get"), None, None)).is_err());
    }

    #[test]
    fn set_verb_without_value_errors() {
        assert!(
            super::resolve_action(&args(Some("set"), Some("search.use_embeddings"), None)).is_err()
        );
    }

    #[test]
    fn show_and_list_verbs_resolve_to_list() {
        assert_eq!(
            super::resolve_action(&args(Some("show"), None, None)).unwrap(),
            ConfigAction::List
        );
        assert_eq!(
            super::resolve_action(&args(Some("list"), None, None)).unwrap(),
            ConfigAction::List
        );
    }

    #[test]
    fn unset_verb_resolves() {
        let action =
            super::resolve_action(&args(Some("unset"), Some("output.theme"), None)).unwrap();
        assert_eq!(
            action,
            ConfigAction::Unset {
                key: "output.theme".to_string()
            }
        );
    }

    // --- Behavior: read, set-with-confirmation, unknown-key rejection ---

    #[test]
    fn read_form_returns_known_value() {
        let dir = tempfile::tempdir().unwrap();
        // get_key reads from the in-memory config; a known key must succeed.
        get_key(&ctx(dir.path()), "search.use_embeddings").expect("read known key");
    }

    #[test]
    fn set_form_persists_known_key() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        set_key(&c, "search.use_embeddings", "false").expect("set known key");
        let written = std::fs::read_to_string(&c.config_path).unwrap();
        assert!(
            written.contains("use_embeddings = false"),
            "config file should record the set value, got:\n{written}"
        );
        // The mistaken `get`/`set` verbs must never appear as top-level keys.
        assert!(
            !written.contains("get ="),
            "junk `get` key leaked:\n{written}"
        );
        assert!(
            !written.contains("set ="),
            "junk `set` key leaked:\n{written}"
        );
    }

    #[test]
    fn set_optional_absent_key_is_accepted() {
        // `output.theme` is Option<String> and absent from a default config; it
        // must still be settable (the round-trip validation must not reject it).
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        set_key(&c, "output.theme", "dark").expect("set optional key");
        let written = std::fs::read_to_string(&c.config_path).unwrap();
        assert!(written.contains("theme = \"dark\""), "got:\n{written}");
    }

    #[test]
    fn set_unknown_key_is_rejected_and_not_persisted() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let err = set_key(&c, "search.bogus_typo", "false").unwrap_err();
        assert!(
            err.to_string().contains("unknown config key"),
            "expected unknown-key error, got: {err}"
        );
        // Nothing must have been written to disk.
        assert!(
            !c.config_path.exists()
                || !std::fs::read_to_string(&c.config_path)
                    .unwrap()
                    .contains("bogus_typo"),
            "typo key must not persist"
        );
    }

    #[test]
    fn set_top_level_junk_key_is_rejected() {
        // Directly attempting to write a `get` top-level key (the old silent
        // footgun) must be rejected as an unknown key.
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        assert!(set_key(&c, "get", "search.use_embeddings").is_err());
    }

    #[test]
    fn unset_removes_key() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        set_key(&c, "output.theme", "dark").expect("set");
        unset_key(&c, "output.theme").expect("unset");
        let written = std::fs::read_to_string(&c.config_path).unwrap();
        assert!(!written.contains("theme = \"dark\""), "got:\n{written}");
    }
}
