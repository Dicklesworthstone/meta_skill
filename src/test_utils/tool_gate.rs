//! Shared gate for tests that require external CLI tools (`br` / `bv`).
//!
//! Tool-gated tests are marked `#[ignore = "needs br"]` or `#[ignore = "needs bv"]`
//! so that a default `cargo test` run reports them as `ignored` rather than silently
//! returning early and being counted as `ok`. When run explicitly via `--ignored`,
//! these helpers verify the tool is present, record the exact `--version` string
//! they ran against, and panic loudly if the tool is missing or unusable.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// Resolved binary path and recorded `--version` output for a verified external tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolInfo {
    pub binary: PathBuf,
    pub version: String,
}

/// Probe a specific binary with `--version` and return its trimmed version string.
pub fn probe_tool_binary(binary: &Path) -> Result<ToolInfo, String> {
    let output = Command::new(binary)
        .arg("--version")
        .output()
        .map_err(|err| format!("failed to execute `{}`: {err}", binary.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!(
            "`{} --version` exited with {}: {}",
            binary.display(),
            output.status,
            if stderr.is_empty() {
                "<no stderr>"
            } else {
                &stderr
            }
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let version = if stdout.is_empty() {
        String::from_utf8_lossy(&output.stderr).trim().to_string()
    } else {
        stdout
    };

    if version.is_empty() {
        return Err(format!(
            "`{} --version` succeeded but produced no version output",
            binary.display()
        ));
    }

    Ok(ToolInfo {
        binary: binary.to_path_buf(),
        version,
    })
}

/// Require a specific binary to be executable and report a non-empty `--version`.
///
/// Panics loudly if the binary is unavailable so tool-gated tests never pass silently.
pub fn require_tool_binary(tool_label: &str, binary: &Path, test_name: &str) -> ToolInfo {
    match probe_tool_binary(binary) {
        Ok(info) => info,
        Err(detail) => {
            panic!(
                "[{test_name}] required external tool `{tool_label}` is unavailable ({detail}). \
                 Install `{tool_label}` on PATH or run without `--ignored`."
            );
        }
    }
}

/// Require the beads CLI (`br`, falling back to legacy `bd`) and record its version.
///
/// Panics loudly if unavailable; never returns early.
pub fn require_br(test_name: &str) -> ToolInfo {
    static LOGGED: OnceLock<String> = OnceLock::new();
    let bin = crate::beads::resolve_beads_binary();
    let info = require_tool_binary("br", &bin, test_name);
    let _ = LOGGED.get_or_init(|| {
        eprintln!(
            "[tool-gate] beads CLI verified: {} ({})",
            info.binary.display(),
            info.version
        );
        info.version.clone()
    });
    info
}

/// Require the `bv` (beads_viewer) CLI and record its version.
///
/// Panics loudly if unavailable; never returns early.
pub fn require_bv(test_name: &str) -> ToolInfo {
    static LOGGED: OnceLock<String> = OnceLock::new();
    let bin = PathBuf::from("bv");
    let info = require_tool_binary("bv", &bin, test_name);
    let _ = LOGGED.get_or_init(|| {
        eprintln!(
            "[tool-gate] bv CLI verified: {} ({})",
            info.binary.display(),
            info.version
        );
        info.version.clone()
    });
    info
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_nonexistent_tool_returns_err() {
        let err = probe_tool_binary(Path::new("/nonexistent/ms-tool-gate-missing-bin"))
            .expect_err("missing binary must return Err");
        assert!(
            err.contains("failed to execute"),
            "unexpected error detail: {err}"
        );
    }

    #[test]
    #[should_panic(expected = "required external tool `br` is unavailable")]
    fn require_missing_tool_panics_loudly() {
        let _ = require_tool_binary(
            "br",
            Path::new("/nonexistent/ms-tool-gate-missing-br"),
            "planted_negative_missing_br",
        );
    }

    #[cfg(unix)]
    #[test]
    fn require_tool_records_exact_version_output() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let bin = dir.path().join("fake-tool");
        std::fs::write(&bin, "#!/bin/sh\necho 'fake-tool 9.8.7'\n").expect("write script");
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755))
            .expect("chmod script");

        let info = require_tool_binary("fake-tool", &bin, "test_records_version");
        assert_eq!(info.binary, bin);
        assert_eq!(info.version, "fake-tool 9.8.7");
    }
}
