//! ms config - Manage configuration

use clap::Args;

use std::path::{Path, PathBuf};

use crate::cli::output;
use crate::app::AppContext;
use crate::config::Config;
use crate::error::Result;

#[derive(Args, Debug)]
pub struct ConfigArgs {
    /// Configuration key to get/set
    pub key: Option<String>,

    /// Value to set
    pub value: Option<String>,

    /// List all configuration
    #[arg(long)]
    pub list: bool,

    /// Unset a configuration key
    #[arg(long)]
    pub unset: bool,
}

pub fn run(ctx: &AppContext, args: &ConfigArgs) -> Result<()> {
    let ctx = ConfigContext {
        config: ctx.config.clone(),
        config_path: ctx.config_path.clone(),
        robot_mode: ctx.robot_mode,
    };

    if args.list || args.key.is_none() {
        return emit_config(&ctx);
    }

    if args.unset && args.value.is_some() {
        return Err(crate::error::MsError::Config(
            "cannot use --unset with a value".to_string(),
        ));
    }

    if args.unset {
        let key = args
            .key
            .as_ref()
            .ok_or_else(|| crate::error::MsError::Config("missing key".to_string()))?;
        return unset_key(&ctx, key);
    }

    if let (Some(key), Some(value)) = (args.key.as_ref(), args.value.as_ref()) {
        return set_key(&ctx, key, value);
    }

    let key = args
        .key
        .as_ref()
        .ok_or_else(|| crate::error::MsError::Config("missing key".to_string()))?;
    get_key(&ctx, key)
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
    set_path(&mut doc, key, value)?;
    write_config_doc(&ctx.config_path, &doc)?;
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


