//! Secret scanning and redaction for security.
//!
//! Provides detection and redaction of secrets in text content:
//! - Pattern-based detection for common secret formats (AWS keys, tokens, etc.)
//! - Entropy-based detection for high-entropy strings
//! - Unified redaction API for evidence, logs, and outputs
//!
//! # Example
//!
//! ```rust
//! use ms::security::secret_scanner::{scan_secrets, redact_secrets, SecretMatch};
//!
//! let content = "My AWS key is AKIAIOSFODNN7EXAMPLE";
//! let matches = scan_secrets(content);
//! assert!(!matches.is_empty());
//!
//! let redacted = redact_secrets(content);
//! assert!(!redacted.contains("AKIAIOSFODNN7EXAMPLE"));
//! ```

use once_cell::sync::Lazy;
use regex::Regex;

/// Types of secrets that can be detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecretType {
    /// AWS Access Key ID
    AwsAccessKey,
    /// AWS Secret Access Key
    AwsSecretKey,
    /// GitHub Personal Access Token
    GitHubToken,
    /// Generic API key pattern
    ApiKey,
    /// JWT token
    JwtToken,
    /// Private key (RSA, ECDSA, etc.)
    PrivateKey,
    /// Password in URL or config
    Password,
    /// Bearer token
    BearerToken,
    /// Slack webhook URL or token
    SlackToken,
    /// Generic high-entropy string
    HighEntropy,
    /// Base64-encoded secret (32+ chars)
    Base64Secret,
    /// Hex-encoded secret (32+ chars)
    HexSecret,
    /// SSH private key
    SshPrivateKey,
    /// PGP private key block
    PgpPrivateKey,
    /// Database connection string with credentials
    DatabaseUrl,
    /// Generic secret assignment pattern
    GenericSecret,
}

impl std::fmt::Display for SecretType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AwsAccessKey => write!(f, "AWS Access Key"),
            Self::AwsSecretKey => write!(f, "AWS Secret Key"),
            Self::GitHubToken => write!(f, "GitHub Token"),
            Self::ApiKey => write!(f, "API Key"),
            Self::JwtToken => write!(f, "JWT Token"),
            Self::PrivateKey => write!(f, "Private Key"),
            Self::Password => write!(f, "Password"),
            Self::BearerToken => write!(f, "Bearer Token"),
            Self::SlackToken => write!(f, "Slack Token"),
            Self::HighEntropy => write!(f, "High Entropy String"),
            Self::Base64Secret => write!(f, "Base64 Secret"),
            Self::HexSecret => write!(f, "Hex Secret"),
            Self::SshPrivateKey => write!(f, "SSH Private Key"),
            Self::PgpPrivateKey => write!(f, "PGP Private Key"),
            Self::DatabaseUrl => write!(f, "Database URL"),
            Self::GenericSecret => write!(f, "Generic Secret"),
        }
    }
}

/// A detected secret in content.
#[derive(Debug, Clone)]
pub struct SecretMatch {
    /// Type of secret detected
    pub secret_type: SecretType,
    /// Byte offset of the start of the match
    pub start: usize,
    /// Byte offset of the end of the match
    pub end: usize,
    /// The matched text (for internal use - be careful exposing this)
    matched_text: String,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f64,
}

impl SecretMatch {
    /// Get a masked version of the matched text for logging.
    pub fn masked_preview(&self) -> String {
        let len = self.matched_text.len();
        if len <= 8 {
            "[REDACTED]".to_string()
        } else {
            let prefix: String = self.matched_text.chars().take(4).collect();
            format!("{}...{} chars", prefix, len)
        }
    }

    /// Get the length of the matched secret.
    pub fn len(&self) -> usize {
        self.matched_text.len()
    }

    /// Check if the match is empty.
    pub fn is_empty(&self) -> bool {
        self.matched_text.is_empty()
    }
}

/// Pattern definition for secret detection.
struct SecretPattern {
    secret_type: SecretType,
    regex: Regex,
    confidence: f64,
}

/// Static patterns for detecting secrets.
static SECRET_PATTERNS: Lazy<Vec<SecretPattern>> = Lazy::new(|| {
    vec![
        // AWS Access Key ID (starts with AKIA, ABIA, ACCA, ASIA)
        SecretPattern {
            secret_type: SecretType::AwsAccessKey,
            regex: Regex::new(r"(?:^|[^A-Z0-9])(A[KBS]IA[0-9A-Z]{16})(?:[^A-Z0-9]|$)")
                .expect("invalid AWS access key regex"),
            confidence: 0.95,
        },
        // AWS Secret Key (40 char base64-ish)
        SecretPattern {
            secret_type: SecretType::AwsSecretKey,
            regex: Regex::new(r#"(?i)(?:aws_?secret|secret_?key)['"]?\s*[:=]\s*['"]?([A-Za-z0-9/+=]{40})['"]?"#)
                .expect("invalid AWS secret key regex"),
            confidence: 0.90,
        },
        // GitHub Personal Access Token (classic: ghp_, fine-grained: github_pat_)
        SecretPattern {
            secret_type: SecretType::GitHubToken,
            regex: Regex::new(r"(?:ghp_[A-Za-z0-9]{36}|github_pat_[A-Za-z0-9]{22}_[A-Za-z0-9]{59}|gho_[A-Za-z0-9]{36}|ghs_[A-Za-z0-9]{36}|ghr_[A-Za-z0-9]{36})")
                .expect("invalid GitHub token regex"),
            confidence: 0.98,
        },
        // JWT tokens (three base64url segments)
        SecretPattern {
            secret_type: SecretType::JwtToken,
            regex: Regex::new(r"eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}")
                .expect("invalid JWT regex"),
            confidence: 0.95,
        },
        // Bearer token in authorization header
        SecretPattern {
            secret_type: SecretType::BearerToken,
            regex: Regex::new(r"(?i)bearer\s+([A-Za-z0-9_\-.~+/]+=*)")
                .expect("invalid bearer token regex"),
            confidence: 0.85,
        },
        // Generic API key patterns
        SecretPattern {
            secret_type: SecretType::ApiKey,
            regex: Regex::new(r#"(?i)(?:api[_-]?key|apikey)['"]?\s*[:=]\s*['"]?([A-Za-z0-9_\-]{16,64})['"]?"#)
                .expect("invalid API key regex"),
            confidence: 0.80,
        },
        // Password in URL (e.g., postgres://user:pass@host)
        SecretPattern {
            secret_type: SecretType::DatabaseUrl,
            regex: Regex::new(r"(?i)(?:postgres(?:ql)?|mysql|mongodb(?:\+srv)?|redis|amqp)://[^:]+:([^@]+)@")
                .expect("invalid database URL regex"),
            confidence: 0.90,
        },
        // SSH private key header
        SecretPattern {
            secret_type: SecretType::SshPrivateKey,
            regex: Regex::new(r"-----BEGIN (?:RSA |DSA |EC |OPENSSH )?PRIVATE KEY-----")
                .expect("invalid SSH key regex"),
            confidence: 0.99,
        },
        // PGP private key block
        SecretPattern {
            secret_type: SecretType::PgpPrivateKey,
            regex: Regex::new(r"-----BEGIN PGP PRIVATE KEY BLOCK-----")
                .expect("invalid PGP key regex"),
            confidence: 0.99,
        },
        // Generic private key
        SecretPattern {
            secret_type: SecretType::PrivateKey,
            regex: Regex::new(r"-----BEGIN (?:ENCRYPTED )?PRIVATE KEY-----")
                .expect("invalid private key regex"),
            confidence: 0.99,
        },
        // Slack token
        SecretPattern {
            secret_type: SecretType::SlackToken,
            regex: Regex::new(r"xox[baprs]-[0-9]{10,13}-[0-9]{10,13}[a-zA-Z0-9-]*")
                .expect("invalid Slack token regex"),
            confidence: 0.95,
        },
        // Generic password assignment
        SecretPattern {
            secret_type: SecretType::Password,
            regex: Regex::new(r#"(?i)(?:password|passwd|pwd)['\"]?\s*[:=]\s*['\"]([^'"\s]{8,64})['\"]"#)
                .expect("invalid password regex"),
            confidence: 0.70,
        },
        // Generic secret assignment
        SecretPattern {
            secret_type: SecretType::GenericSecret,
            regex: Regex::new(r#"(?i)(?:secret|token|credential)['\"]?\s*[:=]\s*['\"]([^'"\s]{16,64})['\"]"#)
                .expect("invalid generic secret regex"),
            confidence: 0.60,
        },
    ]
});

/// Scan content for secrets using pattern matching.
///
/// # Arguments
///
/// * `content` - The text content to scan
///
/// # Returns
///
/// Vector of `SecretMatch` for all detected secrets, sorted by position.
pub fn scan_secrets(content: &str) -> Vec<SecretMatch> {
    let mut matches = Vec::new();

    for pattern in SECRET_PATTERNS.iter() {
        for cap in pattern.regex.find_iter(content) {
            matches.push(SecretMatch {
                secret_type: pattern.secret_type,
                start: cap.start(),
                end: cap.end(),
                matched_text: cap.as_str().to_string(),
                confidence: pattern.confidence,
            });
        }
    }

    // Also check for high-entropy strings
    for entropy_match in scan_high_entropy(content) {
        // Don't duplicate if already matched by a pattern
        let overlaps = matches.iter().any(|m| {
            (entropy_match.start >= m.start && entropy_match.start < m.end)
                || (entropy_match.end > m.start && entropy_match.end <= m.end)
        });
        if !overlaps {
            matches.push(entropy_match);
        }
    }

    // Sort by position
    matches.sort_by_key(|m| m.start);
    matches
}

/// Scan for high-entropy strings that might be secrets.
fn scan_high_entropy(content: &str) -> Vec<SecretMatch> {
    static HIGH_ENTROPY_RE: Lazy<Regex> = Lazy::new(|| {
        // Look for long alphanumeric strings that could be secrets
        Regex::new(r"[A-Za-z0-9+/=_\-]{32,}")
            .expect("invalid high entropy regex")
    });

    let mut matches = Vec::new();

    for cap in HIGH_ENTROPY_RE.find_iter(content) {
        let text = cap.as_str();
        let entropy = calculate_entropy(text);

        // High entropy threshold (typically secrets have entropy > 4.5)
        if entropy > 4.5 {
            // Determine if it's base64 or hex
            let secret_type = if is_likely_base64(text) {
                SecretType::Base64Secret
            } else if is_likely_hex(text) {
                SecretType::HexSecret
            } else {
                SecretType::HighEntropy
            };

            // Confidence based on entropy
            let confidence = if entropy > 5.5 {
                0.85
            } else if entropy > 5.0 {
                0.70
            } else {
                0.55
            };

            matches.push(SecretMatch {
                secret_type,
                start: cap.start(),
                end: cap.end(),
                matched_text: text.to_string(),
                confidence,
            });
        }
    }

    matches
}

/// Calculate Shannon entropy of a string.
fn calculate_entropy(s: &str) -> f64 {
    if s.is_empty() {
        return 0.0;
    }

    let mut freq = std::collections::HashMap::new();
    for c in s.chars() {
        *freq.entry(c).or_insert(0usize) += 1;
    }

    let len = s.len() as f64;
    freq.values()
        .map(|&count| {
            let p = count as f64 / len;
            -p * p.log2()
        })
        .sum()
}

/// Check if a string looks like base64.
fn is_likely_base64(s: &str) -> bool {
    // Base64 characteristics: length divisible by 4, ends with 0-2 '='
    let clean = s.trim_end_matches('=');
    if s.len() % 4 != 0 && (s.len() - clean.len()) > 2 {
        return false;
    }
    // Check character set
    clean.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/')
}

/// Check if a string looks like hex.
fn is_likely_hex(s: &str) -> bool {
    s.len() % 2 == 0 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Check if content contains any secrets.
///
/// A quick check that returns true if any secrets are found.
pub fn contains_secrets(content: &str) -> bool {
    // Quick check with patterns first
    for pattern in SECRET_PATTERNS.iter() {
        if pattern.regex.is_match(content) {
            return true;
        }
    }

    // Check high entropy
    !scan_high_entropy(content).is_empty()
}

/// Redact all detected secrets in content.
///
/// # Arguments
///
/// * `content` - The text content to redact
///
/// # Returns
///
/// Content with all detected secrets replaced with `[REDACTED]`.
pub fn redact_secrets(content: &str) -> String {
    let matches = scan_secrets(content);
    if matches.is_empty() {
        return content.to_string();
    }

    // Build redacted string
    let mut result = String::with_capacity(content.len());
    let mut last_end = 0;

    for m in matches {
        // Add content before this match
        if m.start > last_end {
            result.push_str(&content[last_end..m.start]);
        }
        // Add redaction marker
        result.push_str("[REDACTED]");
        last_end = m.end;
    }

    // Add remaining content
    if last_end < content.len() {
        result.push_str(&content[last_end..]);
    }

    result
}

/// Redact secrets with type-specific markers.
///
/// # Arguments
///
/// * `content` - The text content to redact
///
/// # Returns
///
/// Content with secrets replaced with type-specific markers like `[REDACTED:AWS_KEY]`.
pub fn redact_secrets_typed(content: &str) -> String {
    let matches = scan_secrets(content);
    if matches.is_empty() {
        return content.to_string();
    }

    let mut result = String::with_capacity(content.len());
    let mut last_end = 0;

    for m in matches {
        if m.start > last_end {
            result.push_str(&content[last_end..m.start]);
        }
        // Type-specific marker
        let marker = match m.secret_type {
            SecretType::AwsAccessKey => "[REDACTED:AWS_ACCESS_KEY]",
            SecretType::AwsSecretKey => "[REDACTED:AWS_SECRET_KEY]",
            SecretType::GitHubToken => "[REDACTED:GITHUB_TOKEN]",
            SecretType::ApiKey => "[REDACTED:API_KEY]",
            SecretType::JwtToken => "[REDACTED:JWT]",
            SecretType::PrivateKey => "[REDACTED:PRIVATE_KEY]",
            SecretType::Password => "[REDACTED:PASSWORD]",
            SecretType::BearerToken => "[REDACTED:BEARER_TOKEN]",
            SecretType::SlackToken => "[REDACTED:SLACK_TOKEN]",
            SecretType::HighEntropy => "[REDACTED:HIGH_ENTROPY]",
            SecretType::Base64Secret => "[REDACTED:BASE64_SECRET]",
            SecretType::HexSecret => "[REDACTED:HEX_SECRET]",
            SecretType::SshPrivateKey => "[REDACTED:SSH_KEY]",
            SecretType::PgpPrivateKey => "[REDACTED:PGP_KEY]",
            SecretType::DatabaseUrl => "[REDACTED:DB_CREDENTIAL]",
            SecretType::GenericSecret => "[REDACTED:SECRET]",
        };
        result.push_str(marker);
        last_end = m.end;
    }

    if last_end < content.len() {
        result.push_str(&content[last_end..]);
    }

    result
}

/// Summary of secrets found in content.
#[derive(Debug, Clone, Default)]
pub struct SecretScanSummary {
    /// Total number of secrets found
    pub total_count: usize,
    /// Count by secret type
    pub by_type: std::collections::HashMap<SecretType, usize>,
    /// Highest confidence match
    pub max_confidence: f64,
}

/// Scan content and return a summary without exposing secret values.
pub fn scan_secrets_summary(content: &str) -> SecretScanSummary {
    let matches = scan_secrets(content);

    let mut summary = SecretScanSummary::default();
    summary.total_count = matches.len();

    for m in &matches {
        *summary.by_type.entry(m.secret_type).or_insert(0) += 1;
        if m.confidence > summary.max_confidence {
            summary.max_confidence = m.confidence;
        }
    }

    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_aws_access_key() {
        let content = "My key is AKIAIOSFODNN7EXAMPLE";
        let matches = scan_secrets(content);
        assert!(!matches.is_empty());
        assert!(matches.iter().any(|m| m.secret_type == SecretType::AwsAccessKey));
    }

    #[test]
    fn test_detect_github_token() {
        let content = "token: ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
        let matches = scan_secrets(content);
        assert!(!matches.is_empty());
        assert!(matches.iter().any(|m| m.secret_type == SecretType::GitHubToken));
    }

    #[test]
    fn test_detect_jwt() {
        let content = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
        let matches = scan_secrets(content);
        assert!(!matches.is_empty());
        assert!(matches.iter().any(|m| m.secret_type == SecretType::JwtToken));
    }

    #[test]
    fn test_detect_private_key() {
        let content = "-----BEGIN RSA PRIVATE KEY-----\nMIIEow...";
        let matches = scan_secrets(content);
        assert!(!matches.is_empty());
        assert!(matches.iter().any(|m| m.secret_type == SecretType::SshPrivateKey));
    }

    #[test]
    fn test_detect_database_url() {
        let content = "DATABASE_URL=postgres://admin:supersecret@localhost:5432/mydb";
        let matches = scan_secrets(content);
        assert!(!matches.is_empty());
        assert!(matches.iter().any(|m| m.secret_type == SecretType::DatabaseUrl));
    }

    #[test]
    fn test_redact_secrets() {
        let content = "My AWS key is AKIAIOSFODNN7EXAMPLE and token is ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
        let redacted = redact_secrets(content);
        assert!(!redacted.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(!redacted.contains("ghp_"));
        assert!(redacted.contains("[REDACTED]"));
    }

    #[test]
    fn test_redact_secrets_typed() {
        let content = "api_key = 'sk_live_12345678901234567890'";
        let redacted = redact_secrets_typed(content);
        assert!(redacted.contains("[REDACTED:"));
    }

    #[test]
    fn test_no_false_positives_on_normal_text() {
        let content = "This is just normal text without any secrets.";
        let matches = scan_secrets(content);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_entropy_calculation() {
        // Low entropy (repeated chars)
        assert!(calculate_entropy("aaaaaaaaaa") < 1.0);

        // High entropy (random-looking)
        assert!(calculate_entropy("aB3dE5fG7hI9jK1lM3nO5p") > 4.0);
    }

    #[test]
    fn test_summary() {
        let content = "key1: AKIAIOSFODNN7EXAMPLE key2: ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
        let summary = scan_secrets_summary(content);
        assert!(summary.total_count >= 2);
        assert!(summary.max_confidence > 0.9);
    }

    #[test]
    fn test_slack_token() {
        // Build a test token dynamically to avoid triggering secret scanners
        let prefix = "xoxb";
        let sep = "-";
        let nums = "0".repeat(13);
        let suffix = "TestTokenValue24Ch";
        let content = format!("SLACK_TOKEN={}{sep}{nums}{sep}{nums}{sep}{suffix}", prefix);
        let matches = scan_secrets(&content);
        assert!(!matches.is_empty());
        assert!(matches.iter().any(|m| m.secret_type == SecretType::SlackToken));
    }

    #[test]
    fn test_contains_secrets() {
        assert!(contains_secrets("token: ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"));
        assert!(!contains_secrets("just some normal text"));
    }
}
