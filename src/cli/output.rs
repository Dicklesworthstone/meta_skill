use chrono::{DateTime, Utc};
use console::style;
use serde::Serialize;

use crate::error::{MsError, Result};

#[derive(Debug, Clone, Copy)]
pub enum OutputMode {
    Human,
    Robot,
}

#[derive(Serialize)]
pub struct RobotResponse<T> {
    pub status: RobotStatus,
    pub timestamp: DateTime<Utc>,
    pub version: String,
    pub data: T,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RobotStatus {
    Ok,
    Error { code: String, message: String },
    Partial { completed: usize, failed: usize },
}

pub fn robot_ok<T: Serialize>(data: T) -> RobotResponse<T> {
    RobotResponse {
        status: RobotStatus::Ok,
        timestamp: Utc::now(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        data,
        warnings: Vec::new(),
    }
}

pub fn robot_error(
    code: impl Into<String>,
    message: impl Into<String>,
) -> RobotResponse<serde_json::Value> {
    RobotResponse {
        status: RobotStatus::Error {
            code: code.into(),
            message: message.into(),
        },
        timestamp: Utc::now(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        data: serde_json::Value::Null,
        warnings: Vec::new(),
    }
}

pub fn emit_robot<T: Serialize>(response: &RobotResponse<T>) -> Result<()> {
    emit_json(response)
}

pub fn emit_json<T: Serialize>(value: &T) -> Result<()> {
    let payload = serde_json::to_string_pretty(value)
        .map_err(|err| MsError::Config(format!("serialize output: {err}")))?;
    println!("{payload}");
    Ok(())
}

pub struct HumanLayout {
    lines: Vec<String>,
    key_width: usize,
}

impl HumanLayout {
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            key_width: 18,
        }
    }

    pub fn title(&mut self, text: &str) -> &mut Self {
        self.lines.push(style(text).bold().to_string());
        self.lines.push(String::new());
        self
    }

    pub fn section(&mut self, text: &str) -> &mut Self {
        self.lines.push(style(text).bold().to_string());
        self.lines.push("-".repeat(text.len().max(3)));
        self
    }

    pub fn kv(&mut self, key: &str, value: &str) -> &mut Self {
        let key_style = style(key).dim().to_string();
        self.lines.push(format!(
            "{key_style:width$} {value}",
            width = self.key_width
        ));
        self
    }

    pub fn bullet(&mut self, text: &str) -> &mut Self {
        self.lines.push(format!("- {text}"));
        self
    }

    pub fn blank(&mut self) -> &mut Self {
        self.lines.push(String::new());
        self
    }

    pub fn push_line(&mut self, line: impl Into<String>) -> &mut Self {
        self.lines.push(line.into());
        self
    }

    pub fn build(self) -> String {
        self.lines.join("\n")
    }
}

pub fn emit_human(layout: HumanLayout) {
    println!("{}", layout.build());
}
