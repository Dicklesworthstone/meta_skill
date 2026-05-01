#!/bin/bash
# Unit tests for install.sh functions
# Usage: ./test_install_functions.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TESTS_PASSED=0
TESTS_FAILED=0

# Colors
if [[ -t 1 ]]; then
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    YELLOW='\033[0;33m'
    NC='\033[0m'
else
    RED='' GREEN='' YELLOW='' NC=''
fi

pass() { echo -e "${GREEN}PASS${NC}: $*"; TESTS_PASSED=$((TESTS_PASSED + 1)); }
fail() { echo -e "${RED}FAIL${NC}: $*"; TESTS_FAILED=$((TESTS_FAILED + 1)); }
skip() { echo -e "${YELLOW}SKIP${NC}: $*"; }

# Create a test environment
TEMP_DIR=$(mktemp -d)
trap 'rm -rf "$TEMP_DIR"' EXIT
TEST_HOME="$TEMP_DIR/home"
mkdir -p "$TEST_HOME"
export HOME="$TEST_HOME"
ORIGINAL_PATH="$PATH"

# Source the real installer functions. Keeping this pointed at install.sh
# prevents unit tests from drifting behind installer changes.
# shellcheck source=scripts/install.sh
source "$SCRIPT_DIR/install.sh" --source-only

echo "=== Install Script Unit Tests ==="
echo ""

# Test 1: Platform detection format
test_platform_detection_format() {
    local result
    result=$(detect_platform 2>/dev/null) || {
        fail "detect_platform failed"
        return
    }

    # Platform should match pattern: (x86_64|aarch64)-(unknown-linux-gnu|apple-darwin|pc-windows-msvc)
    if [[ "$result" =~ ^(x86_64|aarch64)-(unknown-linux-gnu|apple-darwin|pc-windows-msvc)$ ]]; then
        pass "detect_platform returns valid format: $result"
    else
        fail "detect_platform returned invalid format: $result"
    fi
}

# Test 2: Platform detection produces output
test_platform_detection_not_empty() {
    local result
    result=$(detect_platform 2>/dev/null)

    if [[ -n "$result" ]]; then
        pass "detect_platform returns non-empty string"
    else
        fail "detect_platform returned empty string"
    fi
}

# Test 3: Log function works
test_log_function() {
    local output
    output=$(log "test message" 2>&1)

    if [[ "$output" == *"test message"* ]]; then
        pass "log function includes message"
    else
        fail "log function output unexpected: $output"
    fi
}

# Test 4: Die function returns error
test_die_function() {
    if (die "test error") 2>/dev/null; then
        fail "die function did not return error"
    else
        pass "die function returns non-zero"
    fi
}

# Test 5: Version normalization accepts release tags
test_normalize_version_prefixed() {
    local result
    result=$(normalize_version "v1.2.3")

    if [[ "$result" == "v1.2.3" ]]; then
        pass "normalize_version preserves v-prefixed releases"
    else
        fail "normalize_version returned unexpected prefixed result: $result"
    fi
}

# Test 6: Version normalization accepts bare SemVer
test_normalize_version_bare() {
    local result
    result=$(normalize_version "1.2.3")

    if [[ "$result" == "v1.2.3" ]]; then
        pass "normalize_version adds v prefix to bare releases"
    else
        fail "normalize_version returned unexpected bare result: $result"
    fi
}

# Test 7: Version normalization rejects unsafe input
test_normalize_version_rejects_invalid() {
    if (normalize_version "../v1.2.3") >/dev/null 2>&1; then
        fail "normalize_version accepted invalid release input"
    else
        pass "normalize_version rejects invalid release input"
    fi
}

# Test 8: Argument parser reports missing values cleanly
test_parse_args_requires_values() {
    if (parse_args --version) >/dev/null 2>&1; then
        fail "parse_args accepted --version without a value"
        return
    fi

    if (parse_args --install-dir) >/dev/null 2>&1; then
        fail "parse_args accepted --install-dir without a value"
        return
    fi

    if (parse_args --version --no-verify) >/dev/null 2>&1; then
        fail "parse_args accepted another option as the --version value"
        return
    fi

    pass "parse_args rejects options missing required values"
}

# Test 9: Default install dir is set
test_default_install_dir() {
    if [[ -n "$DEFAULT_INSTALL_DIR" ]]; then
        pass "DEFAULT_INSTALL_DIR is set: $DEFAULT_INSTALL_DIR"
    else
        fail "DEFAULT_INSTALL_DIR is not set"
    fi
}

# Test 10: Binary name is set
test_binary_name() {
    if [[ "$BINARY_NAME" == "ms" ]]; then
        pass "BINARY_NAME is 'ms'"
    else
        fail "BINARY_NAME is not 'ms': $BINARY_NAME"
    fi
}

# Test 11: Repo is correctly set
test_repo_setting() {
    if [[ "$REPO" == "Dicklesworthstone/meta_skill" ]]; then
        pass "REPO is correctly set"
    else
        fail "REPO is not correct: $REPO"
    fi
}

# Test 12: Architecture detection on current system
test_current_arch() {
    local arch
    arch=$(uname -m)

    case "$arch" in
        x86_64|amd64|aarch64|arm64)
            pass "Current architecture is supported: $arch"
            ;;
        *)
            skip "Current architecture may not be supported: $arch"
            ;;
    esac
}

# Test 13: OS detection on current system
test_current_os() {
    local os
    os=$(uname -s | tr '[:upper:]' '[:lower:]')

    case "$os" in
        linux|darwin)
            pass "Current OS is supported: $os"
            ;;
        mingw*|msys*|cygwin*)
            pass "Current OS (Windows-like) is supported: $os"
            ;;
        *)
            fail "Current OS may not be supported: $os"
            ;;
    esac
}

# Test 14: Curl or wget is available
test_download_tool_available() {
    if command -v curl >/dev/null 2>&1; then
        pass "curl is available"
    elif command -v wget >/dev/null 2>&1; then
        pass "wget is available"
    else
        fail "Neither curl nor wget is available"
    fi
}

# Test 15: SHA256 tool is available
test_sha_tool_available() {
    if command -v sha256sum >/dev/null 2>&1; then
        pass "sha256sum is available"
    elif command -v shasum >/dev/null 2>&1; then
        pass "shasum is available"
    else
        fail "No SHA256 tool is available"
    fi
}

# Test 16: Latest-version redirect parsing handles GitHub release redirects
test_latest_redirect_curl() {
    local stub_dir result
    stub_dir="$TEMP_DIR/curl-redirect-bin"
    mkdir -p "$stub_dir"
    cat > "$stub_dir/curl" <<'EOF'
#!/bin/bash
printf '%s' 'https://github.com/Dicklesworthstone/meta_skill/releases/tag/v9.8.7'
EOF
    chmod +x "$stub_dir/curl"

    PATH="$stub_dir:$ORIGINAL_PATH"
    if result=$(fetch_latest_version_from_redirect); then
        PATH="$ORIGINAL_PATH"
    else
        PATH="$ORIGINAL_PATH"
        fail "fetch_latest_version_from_redirect failed for valid curl redirect"
        return
    fi

    if [[ "$result" == "v9.8.7" ]]; then
        pass "fetch_latest_version_from_redirect parses curl release redirect"
    else
        fail "fetch_latest_version_from_redirect returned unexpected result: $result"
    fi
}

# Test 17: Latest-version redirect parsing rejects malformed tags
test_latest_redirect_rejects_invalid() {
    local stub_dir
    stub_dir="$TEMP_DIR/curl-invalid-bin"
    mkdir -p "$stub_dir"
    cat > "$stub_dir/curl" <<'EOF'
#!/bin/bash
printf '%s' 'https://github.com/Dicklesworthstone/meta_skill/releases/tag/not-a-version'
EOF
    chmod +x "$stub_dir/curl"

    PATH="$stub_dir:$ORIGINAL_PATH"
    if fetch_latest_version_from_redirect >/dev/null 2>&1; then
        PATH="$ORIGINAL_PATH"
        fail "fetch_latest_version_from_redirect accepted malformed tag"
    else
        PATH="$ORIGINAL_PATH"
        pass "fetch_latest_version_from_redirect rejects malformed tag"
    fi
}

# Test 18: Checksum verification accepts matching release entries
test_verify_checksum_accepts_match() {
    local artifact checksum_file artifact_hash
    artifact="$TEMP_DIR/ms-1.2.3-x86_64-unknown-linux-gnu.tar.gz"
    checksum_file="$TEMP_DIR/SHA256SUMS.txt"
    printf '%s' 'release payload' > "$artifact"

    if command -v sha256sum >/dev/null 2>&1; then
        artifact_hash=$(sha256sum "$artifact" | awk '{print $1}')
    elif command -v shasum >/dev/null 2>&1; then
        artifact_hash=$(shasum -a 256 "$artifact" | awk '{print $1}')
    else
        skip "No SHA256 tool is available for checksum test"
        return
    fi

    printf '%s  %s\r\n' "$artifact_hash" "$(basename "$artifact")" > "$checksum_file"

    if verify_checksum "$artifact" "$checksum_file" >/dev/null 2>&1; then
        pass "verify_checksum accepts matching artifact entry"
    else
        fail "verify_checksum rejected matching artifact entry"
    fi
}

# Test 19: Checksum verification rejects missing release entries
test_verify_checksum_rejects_missing_entry() {
    local artifact checksum_file
    artifact="$TEMP_DIR/ms-1.2.3-x86_64-unknown-linux-gnu.tar.gz"
    checksum_file="$TEMP_DIR/empty-SHA256SUMS.txt"
    printf '%s' 'release payload' > "$artifact"
    printf '%s  %s\n' "$(printf '%064d' 0)" "different-artifact.tar.gz" > "$checksum_file"

    if (verify_checksum "$artifact" "$checksum_file") >/dev/null 2>&1; then
        fail "verify_checksum accepted checksum file without artifact entry"
    else
        pass "verify_checksum rejects checksum file without artifact entry"
    fi
}

# Run all tests
echo "Running platform detection tests..."
test_platform_detection_format
test_platform_detection_not_empty
test_current_arch
test_current_os

echo ""
echo "Running function tests..."
test_log_function
test_die_function
test_normalize_version_prefixed
test_normalize_version_bare
test_normalize_version_rejects_invalid
test_parse_args_requires_values
test_latest_redirect_curl
test_latest_redirect_rejects_invalid
test_verify_checksum_accepts_match
test_verify_checksum_rejects_missing_entry

echo ""
echo "Running configuration tests..."
test_default_install_dir
test_binary_name
test_repo_setting

echo ""
echo "Running dependency tests..."
test_download_tool_available
test_sha_tool_available

# Summary
echo ""
echo "=== Test Summary ==="
echo -e "Passed: ${GREEN}$TESTS_PASSED${NC}"
echo -e "Failed: ${RED}$TESTS_FAILED${NC}"

if [[ $TESTS_FAILED -gt 0 ]]; then
    exit 1
fi

exit 0
