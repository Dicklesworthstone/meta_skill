//! Device Code Flow (RFC 8628)
//!
//! Implements the OAuth 2.0 Device Authorization Grant for CLI authentication.
//! This flow is ideal for headless/SSH environments where browser login isn't practical.
//!
//! Flow:
//! 1. CLI requests device code via POST /api/cli/device-code
//! 2. API returns device_code (secret) and user_code (shown to user)
//! 3. User visits verification URL and enters user_code
//! 4. CLI polls POST /api/cli/device-token with device_code
//! 5. After user verifies, CLI receives access + refresh tokens

use serde::{Deserialize, Serialize};
use std::io::IsTerminal;
use std::time::{Duration, Instant};
use tracing::{debug, info};

use super::{JfpAuthConfig, JfpCredentials};
use crate::error::{MsError, Result};

/// Response from the device code request endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCodeResponse {
    /// Secret code used for polling (never shown to user)
    pub device_code: String,
    /// User-friendly code to enter on verification page
    pub user_code: String,
    /// URL where user should authenticate
    pub verification_url: String,
    /// URL with code pre-filled (for convenience)
    pub verification_url_complete: String,
    /// How long the codes are valid (seconds)
    pub expires_in: u64,
    /// Minimum polling interval (seconds)
    pub interval: u64,
}

/// Result of polling for device code verification.
#[derive(Debug)]
pub enum PollResult {
    /// User has verified, credentials are ready
    Success(JfpCredentials),
    /// User hasn't verified yet, continue polling
    Pending,
    /// Device code has expired
    Expired,
    /// User denied the request
    AccessDenied,
    /// Polling too fast
    SlowDown(u64),
}

/// Request a new device code from the JFP API.
pub fn request_device_code(
    client: &reqwest::blocking::Client,
    config: &JfpAuthConfig,
) -> Result<DeviceCodeResponse> {
    let url = format!("{}/api/cli/device-code", config.base_url);

    info!(url = %url, client_id = %config.client_id, "Requesting device code");

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "client_id": config.client_id
        }))
        .send()
        .map_err(|e| MsError::AuthError(format!("Device code request failed: {e}")))?;

    let status = response.status();
    let body = response
        .text()
        .map_err(|e| MsError::AuthError(format!("Failed to read response: {e}")))?;

    if !status.is_success() {
        // Try to parse error response
        if let Ok(error) = serde_json::from_str::<serde_json::Value>(&body) {
            let error_msg = error["error_description"]
                .as_str()
                .or_else(|| error["error"].as_str())
                .unwrap_or("Unknown error");
            return Err(MsError::AuthError(format!(
                "Device code request failed: {error_msg}"
            )));
        }
        return Err(MsError::AuthError(format!(
            "Device code request failed ({status}): {body}"
        )));
    }

    let device_code: DeviceCodeResponse = serde_json::from_str(&body)
        .map_err(|e| MsError::AuthError(format!("Invalid device code response: {e}")))?;

    debug!(
        user_code = %device_code.user_code,
        expires_in = device_code.expires_in,
        "Device code received"
    );

    Ok(device_code)
}

/// Poll for device code verification.
///
/// Returns `PollResult::Pending` if the user hasn't verified yet,
/// or `PollResult::Success` with credentials once verified.
pub fn poll_for_token(
    client: &reqwest::blocking::Client,
    config: &JfpAuthConfig,
    device_code: &str,
) -> Result<PollResult> {
    let url = format!("{}/api/cli/device-token", config.base_url);

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "device_code": device_code,
            "client_id": config.client_id
        }))
        .send()
        .map_err(|e| MsError::AuthError(format!("Token poll failed: {e}")))?;

    let status = response.status();
    let body = response
        .text()
        .map_err(|e| MsError::AuthError(format!("Failed to read response: {e}")))?;

    // Handle error responses
    if !status.is_success() {
        let error: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
        let error_code = error["error"].as_str().unwrap_or("unknown");

        return match error_code {
            "authorization_pending" => Ok(PollResult::Pending),
            "slow_down" => {
                let interval = error["interval"].as_u64().unwrap_or(10);
                Ok(PollResult::SlowDown(interval))
            }
            "expired_token" | "invalid_grant" => Ok(PollResult::Expired),
            "access_denied" => Ok(PollResult::AccessDenied),
            _ => Err(MsError::AuthError(format!(
                "Token poll error: {error_code}"
            ))),
        };
    }

    // Success - parse credentials
    let token_response: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| MsError::AuthError(format!("Invalid token response: {e}")))?;

    let creds = JfpCredentials {
        access_token: token_response["access_token"]
            .as_str()
            .ok_or_else(|| MsError::AuthError("Missing access_token".to_string()))?
            .to_string(),
        refresh_token: token_response["refresh_token"]
            .as_str()
            .ok_or_else(|| MsError::AuthError("Missing refresh_token".to_string()))?
            .to_string(),
        expires_at: chrono::Utc::now().timestamp()
            + token_response["expires_in"].as_i64().unwrap_or(86400),
        email: token_response["email"]
            .as_str()
            .ok_or_else(|| MsError::AuthError("Missing email".to_string()))?
            .to_string(),
        tier: token_response["tier"]
            .as_str()
            .unwrap_or("free")
            .to_string(),
        user_id: token_response["user_id"]
            .as_str()
            .ok_or_else(|| MsError::AuthError("Missing user_id".to_string()))?
            .to_string(),
    };

    info!(email = %creds.email, tier = %creds.tier, "Authentication successful");

    Ok(PollResult::Success(creds))
}

/// Run the complete device code authentication flow.
///
/// This function:
/// 1. Requests a device code
/// 2. Displays instructions to the user
/// 3. Polls until the user verifies or the code expires
/// 4. Returns credentials on success
pub fn run_device_code_flow(
    client: &reqwest::blocking::Client,
    config: &JfpAuthConfig,
    display_callback: impl Fn(&DeviceCodeResponse),
) -> Result<JfpCredentials> {
    // Request device code
    let device_code = request_device_code(client, config)?;

    // Display instructions to user
    display_callback(&device_code);

    // Calculate expiry time
    let expires_at = Instant::now() + Duration::from_secs(device_code.expires_in);
    let mut poll_interval = Duration::from_secs(device_code.interval);

    // Poll for verification
    loop {
        // Check if expired
        if Instant::now() >= expires_at {
            return Err(MsError::AuthError(
                "Device code expired. Please run 'ms auth login' again.".to_string(),
            ));
        }

        // Wait before polling
        std::thread::sleep(poll_interval);

        // Poll for token
        match poll_for_token(client, config, &device_code.device_code)? {
            PollResult::Success(creds) => return Ok(creds),
            PollResult::Pending => {
                debug!("Authorization pending, continuing to poll");
                continue;
            }
            PollResult::SlowDown(new_interval) => {
                debug!(interval = new_interval, "Slowing down poll rate");
                poll_interval = Duration::from_secs(new_interval);
                continue;
            }
            PollResult::Expired => {
                return Err(MsError::AuthError(
                    "Device code expired. Please run 'ms auth login' again.".to_string(),
                ));
            }
            PollResult::AccessDenied => {
                return Err(MsError::AuthError(
                    "Access denied. The user rejected the authorization request.".to_string(),
                ));
            }
        }
    }
}

/// Check if we're running in a TTY (interactive terminal).
pub fn is_tty() -> bool {
    std::io::stdout().is_terminal() && std::io::stdin().is_terminal()
}

/// Try to open the verification URL in the user's browser.
pub fn open_browser(url: &str) -> bool {
    match open::that(url) {
        Ok(()) => {
            debug!(url = %url, "Opened browser");
            true
        }
        Err(e) => {
            debug!(url = %url, error = %e, "Failed to open browser");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_tty_check() {
        // Just verify the function doesn't panic
        let _ = is_tty();
    }

    #[test]
    fn test_device_code_response_serialization() {
        let json = r#"{
            "device_code": "abc123",
            "user_code": "XYZW-1234",
            "verification_url": "https://example.com/verify",
            "verification_url_complete": "https://example.com/verify?code=XYZW-1234",
            "expires_in": 900,
            "interval": 5
        }"#;

        let response: DeviceCodeResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.device_code, "abc123");
        assert_eq!(response.user_code, "XYZW-1234");
        assert_eq!(response.expires_in, 900);
        assert_eq!(response.interval, 5);
    }
}
