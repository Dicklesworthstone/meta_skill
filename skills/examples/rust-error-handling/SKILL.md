---
id: rust-error-handling
name: Rust Error Handling
description: >-
  Rust-specific error handling patterns, building on the base error handling
  skill. Demonstrates the 'extends' composition feature.
tags: [error-handling, rust, example]
extends: error-handling-base
---

# Rust Error Handling

Rust-specific error handling patterns that extend the base error handling skill.

## Rules

- Use `thiserror` for defining library error types
- Use `anyhow` for application-level error handling
- Implement `std::error::Error` trait for custom error types
- Use the `?` operator for ergonomic error propagation
- Prefer `Result<T, E>` over panicking for recoverable errors
- Use `unwrap()` only in tests or when failure is impossible

## Examples

```rust
// Define library errors with thiserror
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("failed to read config file: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid config format: {0}")]
    Parse(#[from] toml::de::Error),

    #[error("missing required field: {field}")]
    MissingField { field: String },
}
```

```rust
// Application-level error handling with anyhow
use anyhow::{Context, Result};

fn load_config(path: &str) -> Result<Config> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read config from {}", path))?;

    let config: Config = toml::from_str(&content)
        .context("failed to parse config TOML")?;

    Ok(config)
}
```

## Checklist

- [ ] Custom error types use `thiserror` derive
- [ ] Error context is added with `anyhow::Context`
- [ ] No `unwrap()` calls in production code paths
- [ ] `Result` is used instead of `panic!` for recoverable errors
