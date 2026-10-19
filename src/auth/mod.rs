//! JFP Cloud Authentication Module
//!
//! Handles secure authentication with JeffreysPrompts Premium Cloud:
//! - Device code flow (RFC 8628) for headless/SSH environments
//! - Token storage via OS keychain with secure file fallback
//! - Token refresh and revocation
//!
//! ## Usage
//!
//! ```ignore
//! // Login (starts device code flow)
//! ms auth login
//!
//! // Check auth status
//! ms auth status
//!
//! // Logout (clears stored credentials)
//! ms auth logout
//!
//! // Revoke token on server
//! ms auth revoke
//! ```

pub mod device_code;
pub mod token_storage;

use serde::{Deserialize, Serialize};

use crate::error::{MsError, Result};

/// JFP Cloud authentication configuration.
#[derive(Debug, Clone)]
pub struct JfpAuthConfig {
    /// Base URL for the JFP API
    pub base_url: String,
    /// Client ID for authentication
    pub client_id: String,
}

impl Default for JfpAuthConfig {
    fn default() -> Self {
        Self {
            base_url: "https://pro.jeffreysprompts.com".to_string(),
            client_id: "ms-cli".to_string(),
        }
    }
}

impl JfpAuthConfig {
    /// Create config with custom base URL (for testing/staging).
    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            ..Default::default()
        }
    }
}

/// Stored authentication credentials.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JfpCredentials {
    /// JWT access token (short-lived, ~24h)
    pub access_token: String,
    /// JWT refresh token (long-lived, ~30d)
    pub refresh_token: String,
    /// Unix timestamp when access token expires
    pub expires_at: i64,
    /// User's email address
    pub email: String,
    /// User's subscription tier
    pub tier: String,
    /// User ID from JFP
    pub user_id: String,
}

impl JfpCredentials {
    /// Check if the access token is expired (with 5-minute buffer).
    pub fn is_expired(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        // Consider expired if within 5 minutes of expiry
        self.expires_at - 300 <= now
    }

    /// Check if the access token is about to expire (within 1 hour).
    pub fn needs_refresh(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        self.expires_at - 3600 <= now
    }

    /// Get remaining time until expiry in seconds.
    pub fn time_remaining(&self) -> i64 {
        let now = chrono::Utc::now().timestamp();
        (self.expires_at - now).max(0)
    }
}

/// Authentication status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthStatus {
    /// Whether the user is authenticated
    pub authenticated: bool,
    /// User email if authenticated
    pub email: Option<String>,
    /// Subscription tier if authenticated
    pub tier: Option<String>,
    /// Seconds until token expires (if authenticated)
    pub expires_in: Option<i64>,
    /// Storage method used (keychain or file)
    pub storage_method: Option<String>,
}

/// Main authentication client.
pub struct JfpAuthClient {
    config: JfpAuthConfig,
    http_client: reqwest::blocking::Client,
}

impl JfpAuthClient {
    /// Create a new auth client with default config.
    pub fn new() -> Result<Self> {
        Self::with_config(JfpAuthConfig::default())
    }

    /// Create a new auth client with custom config.
    pub fn with_config(config: JfpAuthConfig) -> Result<Self> {
        let http_client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| MsError::Config(format!("HTTP client error: {e}")))?;

        Ok(Self {
            config,
            http_client,
        })
    }

    /// Get the current authentication status.
    pub fn status(&self) -> Result<AuthStatus> {
        match token_storage::load_credentials() {
            Ok(Some(creds)) => {
                let storage_method = token_storage::current_storage_method();
                let time_remaining = creds.time_remaining();
                let authenticated = !creds.is_expired();
                Ok(AuthStatus {
                    authenticated,
                    email: Some(creds.email),
                    tier: Some(creds.tier),
                    expires_in: Some(time_remaining),
                    storage_method: Some(storage_method),
                })
            }
            Ok(None) => Ok(AuthStatus {
                authenticated: false,
                email: None,
                tier: None,
                expires_in: None,
                storage_method: None,
            }),
            Err(e) => {
                tracing::warn!(error = %e, "Failed to load credentials");
                Ok(AuthStatus {
                    authenticated: false,
                    email: None,
                    tier: None,
                    expires_in: None,
                    storage_method: None,
                })
            }
        }
    }

    /// Start the device code authentication flow.
    ///
    /// Returns the device code response for the user to verify.
    pub fn start_device_code_flow(&self) -> Result<device_code::DeviceCodeResponse> {
        device_code::request_device_code(&self.http_client, &self.config)
    }

    /// Poll for device code verification.
    ///
    /// Returns credentials once the user completes verification.
    pub fn poll_device_code(&self, device_code: &str) -> Result<device_code::PollResult> {
        device_code::poll_for_token(&self.http_client, &self.config, device_code)
    }

    /// Complete the login flow by storing credentials.
    pub fn complete_login(&self, creds: JfpCredentials) -> Result<()> {
        token_storage::save_credentials(&creds)?;
        tracing::info!(email = %creds.email, tier = %creds.tier, "Authentication successful");
        Ok(())
    }

    /// Refresh the access token using the refresh token.
    pub fn refresh_token(&self) -> Result<JfpCredentials> {
        let creds = token_storage::load_credentials()?
            .ok_or_else(|| MsError::AuthError("Not authenticated".to_string()))?;

        let url = format!("{}/api/cli/token/refresh", self.config.base_url);

        let response = self
            .http_client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "refresh_token": creds.refresh_token,
                "client_id": self.config.client_id,
            }))
            .send()
            .map_err(|e| MsError::AuthError(format!("Refresh request failed: {e}")))?;

        if !response.status().is_success() {
            let body = response.text().unwrap_or_default();
            return Err(MsError::AuthError(format!("Token refresh failed: {body}")));
        }

        let body: serde_json::Value = response
            .json()
            .map_err(|e| MsError::AuthError(format!("Invalid refresh response: {e}")))?;

        let new_creds = JfpCredentials {
            access_token: body["access_token"]
                .as_str()
                .ok_or_else(|| MsError::AuthError("Missing access_token".to_string()))?
                .to_string(),
            refresh_token: body["refresh_token"]
                .as_str()
                .unwrap_or(&creds.refresh_token)
                .to_string(),
            expires_at: chrono::Utc::now().timestamp()
                + body["expires_in"].as_i64().unwrap_or(86400),
            email: creds.email,
            tier: body["tier"].as_str().unwrap_or(&creds.tier).to_string(),
            user_id: creds.user_id,
        };

        token_storage::save_credentials(&new_creds)?;
        tracing::info!("Token refreshed successfully");

        Ok(new_creds)
    }

    /// Get valid credentials, refreshing if necessary.
    pub fn get_valid_credentials(&self) -> Result<JfpCredentials> {
        let creds = token_storage::load_credentials()?.ok_or_else(|| {
            MsError::AuthError("Not authenticated. Run 'ms auth login' first.".to_string())
        })?;

        if creds.needs_refresh() {
            tracing::debug!("Access token needs refresh");
            return self.refresh_token();
        }

        Ok(creds)
    }

    /// Revoke the current token on the server.
    pub fn revoke_token(&self) -> Result<()> {
        let creds = token_storage::load_credentials()?
            .ok_or_else(|| MsError::AuthError("Not authenticated".to_string()))?;

        let url = format!("{}/api/cli/token/revoke", self.config.base_url);

        let response = self
            .http_client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", creds.access_token))
            .json(&serde_json::json!({
                "token": creds.refresh_token,
                "token_type_hint": "refresh_token",
            }))
            .send()
            .map_err(|e| MsError::AuthError(format!("Revoke request failed: {e}")))?;

        // RFC 7009: revocation endpoint returns 200 even if token was already revoked
        if !response.status().is_success() {
            let body = response.text().unwrap_or_default();
            tracing::warn!(body = %body, "Token revocation returned non-success status");
        }

        // Clear local credentials regardless of server response
        token_storage::clear_credentials()?;
        tracing::info!("Token revoked and local credentials cleared");

        Ok(())
    }

    /// Clear local credentials without server revocation.
    pub fn logout(&self) -> Result<()> {
        token_storage::clear_credentials()?;
        tracing::info!("Logged out (local credentials cleared)");
        Ok(())
    }
}

impl Default for JfpAuthClient {
    fn default() -> Self {
        Self::new().expect("Failed to create default auth client")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_credentials_expiry() {
        let future = chrono::Utc::now().timestamp() + 7200; // 2 hours
        let creds = JfpCredentials {
            access_token: "test".to_string(),
            refresh_token: "test".to_string(),
            expires_at: future,
            email: "test@example.com".to_string(),
            tier: "free".to_string(),
            user_id: "123".to_string(),
        };

        assert!(!creds.is_expired());
        assert!(!creds.needs_refresh()); // >1 hour remaining
    }

    #[test]
    fn test_credentials_needs_refresh() {
        let soon = chrono::Utc::now().timestamp() + 1800; // 30 minutes
        let creds = JfpCredentials {
            access_token: "test".to_string(),
            refresh_token: "test".to_string(),
            expires_at: soon,
            email: "test@example.com".to_string(),
            tier: "free".to_string(),
            user_id: "123".to_string(),
        };

        assert!(!creds.is_expired());
        assert!(creds.needs_refresh()); // <1 hour remaining
    }

    #[test]
    fn test_credentials_expired() {
        let past = chrono::Utc::now().timestamp() - 100;
        let creds = JfpCredentials {
            access_token: "test".to_string(),
            refresh_token: "test".to_string(),
            expires_at: past,
            email: "test@example.com".to_string(),
            tier: "free".to_string(),
            user_id: "123".to_string(),
        };

        assert!(creds.is_expired());
        assert!(creds.needs_refresh());
    }

    #[test]
    fn test_default_config() {
        let config = JfpAuthConfig::default();
        assert_eq!(config.base_url, "https://pro.jeffreysprompts.com");
        assert_eq!(config.client_id, "ms-cli");
    }
}
