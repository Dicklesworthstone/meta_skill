//! Mock HTTP server for registry/GitHub API tests.
//!
//! Provides a test server that simulates GitHub API responses for bundle
//! publish/install operations without requiring network access.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;

/// Recorded HTTP request from the mock server.
#[derive(Debug, Clone)]
pub struct RecordedRequest {
    pub method: String,
    pub path: String,
    pub query: Option<String>,
    pub headers: HashMap<String, String>,
    pub body: Option<Vec<u8>>,
}

/// Response configuration for a mock endpoint.
#[derive(Debug, Clone)]
pub struct MockResponse {
    pub status: u16,
    pub body: Option<Vec<u8>>,
    pub headers: HashMap<String, String>,
    pub delay: Option<Duration>,
}

impl Default for MockResponse {
    fn default() -> Self {
        Self {
            status: 200,
            body: None,
            headers: HashMap::new(),
            delay: None,
        }
    }
}

impl MockResponse {
    /// Create a successful JSON response.
    pub fn json<T: Serialize>(data: T) -> Self {
        let mut headers = HashMap::new();
        headers.insert("content-type".to_string(), "application/json".to_string());
        Self {
            status: 200,
            body: Some(serde_json::to_vec(&data).unwrap_or_default()),
            headers,
            delay: None,
        }
    }

    /// Create an error response.
    pub fn error(status: u16, message: &str) -> Self {
        let body = json!({ "message": message, "status": status });
        let mut headers = HashMap::new();
        headers.insert("content-type".to_string(), "application/json".to_string());
        Self {
            status,
            body: Some(serde_json::to_vec(&body).unwrap_or_default()),
            headers,
            delay: None,
        }
    }

    /// Create a not found response.
    pub fn not_found() -> Self {
        Self::error(404, "Not Found")
    }

    /// Create an unauthorized response.
    pub fn unauthorized() -> Self {
        Self::error(401, "Unauthorized")
    }

    /// Add a delay before responding.
    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = Some(delay);
        self
    }
}

/// Mock GitHub release for API responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockRelease {
    pub id: u64,
    pub tag_name: String,
    pub html_url: String,
    pub upload_url: String,
    pub assets: Vec<MockAsset>,
}

impl MockRelease {
    pub fn new(id: u64, tag: &str, owner: &str, repo: &str) -> Self {
        Self {
            id,
            tag_name: tag.to_string(),
            html_url: format!("https://github.com/{owner}/{repo}/releases/tag/{tag}"),
            upload_url: format!(
                "https://uploads.github.com/repos/{owner}/{repo}/releases/{id}/assets{{?name,label}}"
            ),
            assets: Vec::new(),
        }
    }

    pub fn with_asset(mut self, asset: MockAsset) -> Self {
        self.assets.push(asset);
        self
    }
}

/// Mock GitHub release asset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockAsset {
    pub id: u64,
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}

impl MockAsset {
    pub fn new(id: u64, name: &str, download_url: &str) -> Self {
        Self {
            id,
            name: name.to_string(),
            browser_download_url: download_url.to_string(),
            size: 0,
        }
    }
}

/// Storage for mock server state and recordings.
#[derive(Default)]
pub struct MockServerState {
    pub requests: Vec<RecordedRequest>,
    pub releases: HashMap<String, MockRelease>,
    pub asset_data: HashMap<u64, Vec<u8>>,
}

impl MockServerState {
    /// Clear all recorded requests.
    pub fn clear_requests(&mut self) {
        self.requests.clear();
    }

    /// Add a release to the mock state.
    pub fn add_release(&mut self, owner: &str, repo: &str, release: MockRelease) {
        let key = format!("{}/{}/{}", owner, repo, release.tag_name);
        self.releases.insert(key, release);
    }

    /// Add asset data for download.
    pub fn add_asset_data(&mut self, asset_id: u64, data: Vec<u8>) {
        self.asset_data.insert(asset_id, data);
    }

    /// Get recorded requests matching a path pattern.
    pub fn requests_matching(&self, path_contains: &str) -> Vec<&RecordedRequest> {
        self.requests
            .iter()
            .filter(|r| r.path.contains(path_contains))
            .collect()
    }
}

/// Mock registry server for testing HTTP interactions.
///
/// This struct manages a mock HTTP server that simulates GitHub API responses
/// for bundle operations. Uses httpmock under the hood.
///
/// # Example
/// ```ignore
/// use ms::test_utils::mock_server::{MockRegistryServer, MockRelease};
///
/// let server = MockRegistryServer::start();
/// server.state().lock().unwrap().add_release(
///     "owner",
///     "repo",
///     MockRelease::new(1, "v1.0.0", "owner", "repo"),
/// );
///
/// // Use server.base_url() in tests
/// let url = format!("{}/repos/owner/repo/releases/tags/v1.0.0", server.base_url());
/// ```
pub struct MockRegistryServer {
    state: Arc<Mutex<MockServerState>>,
    base_url: String,
    // We don't store the server handle since httpmock manages lifecycle
}

impl MockRegistryServer {
    /// Start a new mock server on a random available port.
    #[cfg(test)]
    pub fn start() -> Self {
        use httpmock::prelude::*;

        let server = MockServer::start();
        let state = Arc::new(Mutex::new(MockServerState::default()));
        let base_url = server.base_url();

        // Set up default GitHub API mock handlers
        let state_clone = Arc::clone(&state);

        // Mock GET /repos/{owner}/{repo}/releases/tags/{tag}
        server.mock(|when, then| {
            when.method(GET)
                .path_matches(regex::Regex::new(r"^/repos/[^/]+/[^/]+/releases/tags/[^/]+$").unwrap());
            then.status(200).body("{}"); // Will be overridden by specific mocks
        });

        // Mock GET /repos/{owner}/{repo}/releases/latest
        server.mock(|when, then| {
            when.method(GET)
                .path_matches(regex::Regex::new(r"^/repos/[^/]+/[^/]+/releases/latest$").unwrap());
            then.status(200).body("{}");
        });

        // Mock POST /repos/{owner}/{repo}/releases (create release)
        server.mock(|when, then| {
            when.method(POST)
                .path_matches(regex::Regex::new(r"^/repos/[^/]+/[^/]+/releases$").unwrap());
            then.status(201).body("{}");
        });

        Self {
            state,
            base_url,
        }
    }

    /// Get the base URL of the mock server.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Get access to the mock server state.
    pub fn state(&self) -> &Arc<Mutex<MockServerState>> {
        &self.state
    }

    /// Record a request (called by mock handlers).
    pub fn record_request(&self, request: RecordedRequest) {
        if let Ok(mut state) = self.state.lock() {
            state.requests.push(request);
        }
    }

    /// Get the GitHub API base URL (for test configuration).
    pub fn github_api_url(&self) -> String {
        self.base_url.clone()
    }

    /// Get the GitHub uploads URL (for test configuration).
    pub fn github_uploads_url(&self) -> String {
        self.base_url.clone()
    }
}

/// Builder for creating mock server configurations.
#[derive(Default)]
pub struct MockServerBuilder {
    releases: Vec<(String, String, MockRelease)>,
    assets: Vec<(u64, Vec<u8>)>,
}

impl MockServerBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a release to the mock server.
    pub fn with_release(mut self, owner: &str, repo: &str, release: MockRelease) -> Self {
        self.releases.push((owner.to_string(), repo.to_string(), release));
        self
    }

    /// Add asset data for a specific asset ID.
    pub fn with_asset_data(mut self, asset_id: u64, data: Vec<u8>) -> Self {
        self.assets.push((asset_id, data));
        self
    }

    /// Build and start the mock server.
    #[cfg(test)]
    pub fn build(self) -> MockRegistryServer {
        let server = MockRegistryServer::start();
        {
            let mut state = server.state.lock().unwrap();
            for (owner, repo, release) in self.releases {
                state.add_release(&owner, &repo, release);
            }
            for (asset_id, data) in self.assets {
                state.add_asset_data(asset_id, data);
            }
        }
        server
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_response_json() {
        let data = json!({"key": "value"});
        let response = MockResponse::json(&data);
        assert_eq!(response.status, 200);
        assert!(response.body.is_some());
        assert_eq!(
            response.headers.get("content-type"),
            Some(&"application/json".to_string())
        );
    }

    #[test]
    fn test_mock_response_error() {
        let response = MockResponse::error(500, "Internal Server Error");
        assert_eq!(response.status, 500);
        assert!(response.body.is_some());
    }

    #[test]
    fn test_mock_release_creation() {
        let release = MockRelease::new(1, "v1.0.0", "owner", "repo");
        assert_eq!(release.id, 1);
        assert_eq!(release.tag_name, "v1.0.0");
        assert!(release.html_url.contains("owner/repo"));
    }

    #[test]
    fn test_mock_release_with_asset() {
        let asset = MockAsset::new(1, "bundle.msb", "https://example.com/bundle.msb");
        let release = MockRelease::new(1, "v1.0.0", "owner", "repo").with_asset(asset);
        assert_eq!(release.assets.len(), 1);
        assert_eq!(release.assets[0].name, "bundle.msb");
    }

    #[test]
    fn test_mock_server_state() {
        let mut state = MockServerState::default();
        state.add_release(
            "owner",
            "repo",
            MockRelease::new(1, "v1.0.0", "owner", "repo"),
        );
        assert!(state.releases.contains_key("owner/repo/v1.0.0"));
    }

    #[test]
    fn test_mock_server_state_asset_data() {
        let mut state = MockServerState::default();
        state.add_asset_data(1, vec![1, 2, 3, 4]);
        assert_eq!(state.asset_data.get(&1), Some(&vec![1, 2, 3, 4]));
    }

    #[test]
    fn test_mock_server_starts() {
        let server = MockRegistryServer::start();
        assert!(!server.base_url().is_empty());
        assert!(server.base_url().starts_with("http://"));
    }

    #[test]
    fn test_mock_server_builder() {
        let release = MockRelease::new(1, "v1.0.0", "test", "repo");
        let server = MockServerBuilder::new()
            .with_release("test", "repo", release)
            .with_asset_data(1, vec![1, 2, 3])
            .build();

        let state = server.state().lock().unwrap();
        assert!(state.releases.contains_key("test/repo/v1.0.0"));
        assert!(state.asset_data.contains_key(&1));
    }
}
