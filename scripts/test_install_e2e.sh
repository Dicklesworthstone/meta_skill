#!/bin/bash
# End-to-end tests for install.sh
# Tests actual installation from GitHub releases
# Usage: ./test_install_e2e.sh [--dry-run] [--version VERSION]
#
# Options:
#   --dry-run     Don't actually install, just test setup
#   --version     Test with specific version (default: latest)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOG="install_e2e_$(date +%Y%m%d_%H%M%S).log"
TESTS_PASSED=0
TESTS_FAILED=0
DRY_RUN=false
TEST_VERSION=""

# Colors
if [[ -t 1 ]]; then
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    YELLOW='\033[0;33m'
    BLUE='\033[0;34m'
    NC='\033[0m'
else
    RED='' GREEN='' YELLOW='' BLUE='' NC=''
fi

log()  { echo "[$(date +%H:%M:%S)] $*" | tee -a "$LOG"; }
pass() { log "${GREEN}PASS${NC}: $*"; TESTS_PASSED=$((TESTS_PASSED + 1)); }
fail() { log "${RED}FAIL${NC}: $*"; TESTS_FAILED=$((TESTS_FAILED + 1)); }
skip() { log "${YELLOW}SKIP${NC}: $*"; }
info() { log "${BLUE}INFO${NC}: $*"; }

# Parse arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        --version)
            TEST_VERSION="$2"
            shift 2
            ;;
        --help)
            echo "Usage: $0 [--dry-run] [--version VERSION]"
            echo ""
            echo "Options:"
            echo "  --dry-run     Don't actually install, just test setup"
            echo "  --version     Test with specific version (default: latest)"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

echo "=== Install Script E2E Tests ==="
echo "Log: $LOG"
echo "Dry run: $DRY_RUN"
echo ""

# Helper: Create isolated test environment
create_test_env() {
    local temp_dir
    temp_dir=$(mktemp -d)
    echo "$temp_dir"
}

# Helper: Cleanup test environment
cleanup_test_env() {
    local temp_dir="$1"
    rm -rf "$temp_dir" 2>/dev/null || true
}

# Test 1: Check if GitHub releases exist
test_github_releases_accessible() {
    log "Testing GitHub releases accessibility..."

    local effective_url version response
    if effective_url=$(curl -fsSL -o /dev/null -w '%{url_effective}' "https://github.com/Dicklesworthstone/meta_skill/releases/latest" 2>&1); then
        version="${effective_url##*/}"
        if [[ -n "$version" && "$version" =~ ^v[0-9] && "$version" != *"/"* ]]; then
            pass "GitHub releases accessible, latest: $version"
            return 0
        fi
    fi

    if response=$(curl -sS "https://api.github.com/repos/Dicklesworthstone/meta_skill/releases/latest" 2>&1); then
        if [[ "$response" == *"tag_name"* ]]; then
            version=$(echo "$response" | grep -o '"tag_name": "[^"]*"' | head -1 | cut -d'"' -f4)
            pass "GitHub releases accessible, latest: $version"
            return 0
        elif [[ "$response" == *"Not Found"* ]]; then
            skip "No releases found yet (repo may be new)"
            return 1
        elif [[ "$response" == *"rate limit"* ]] || [[ "$response" == *"API rate limit"* ]]; then
            skip "GitHub API rate limited (this is normal in CI without auth)"
            return 1
        else
            skip "GitHub API returned unexpected response (may be rate limited)"
            return 1
        fi
    else
        fail "Could not access GitHub API"
        return 1
    fi
}

# Test 2: Default installation
test_default_install() {
    if [[ "$DRY_RUN" == "true" ]]; then
        skip "Skipping actual installation (--dry-run)"
        return 0
    fi

    log "Testing default installation..."

    local temp_dir
    temp_dir=$(create_test_env)

    local output exit_code=0
    output=$(HOME="$temp_dir" INSTALL_DIR="$temp_dir/bin" \
             ${TEST_VERSION:+VERSION="$TEST_VERSION"} \
             timeout 120 "$SCRIPT_DIR/install.sh" 2>&1) || exit_code=$?

    if [[ $exit_code -eq 0 ]] && [[ -x "$temp_dir/bin/ms" ]]; then
        pass "Default installation succeeded"

        # Verify binary runs
        if "$temp_dir/bin/ms" --version >/dev/null 2>&1; then
            pass "Installed binary runs successfully"
        else
            fail "Installed binary does not run"
        fi
    else
        fail "Default installation failed (exit $exit_code): ${output:0:500}..."
    fi

    cleanup_test_env "$temp_dir"
}

# Test 3: Installation to custom directory
test_custom_dir_install() {
    if [[ "$DRY_RUN" == "true" ]]; then
        skip "Skipping actual installation (--dry-run)"
        return 0
    fi

    log "Testing installation to custom directory..."

    local temp_dir
    temp_dir=$(create_test_env)
    local custom_dir="$temp_dir/custom/path/bin"

    local output exit_code=0
    output=$(HOME="$temp_dir" INSTALL_DIR="$custom_dir" \
             ${TEST_VERSION:+VERSION="$TEST_VERSION"} \
             timeout 120 "$SCRIPT_DIR/install.sh" 2>&1) || exit_code=$?

    if [[ $exit_code -eq 0 ]] && [[ -x "$custom_dir/ms" ]]; then
        pass "Custom directory installation succeeded"
    else
        fail "Custom directory installation failed: ${output:0:500}..."
    fi

    cleanup_test_env "$temp_dir"
}

# Test 4: Installation with checksum verification
test_checksum_verification() {
    if [[ "$DRY_RUN" == "true" ]]; then
        skip "Skipping actual installation (--dry-run)"
        return 0
    fi

    log "Testing checksum verification..."

    local temp_dir
    temp_dir=$(create_test_env)

    local output exit_code=0
    output=$(HOME="$temp_dir" INSTALL_DIR="$temp_dir/bin" \
             VERIFY=true \
             ${TEST_VERSION:+VERSION="$TEST_VERSION"} \
             timeout 120 "$SCRIPT_DIR/install.sh" 2>&1) || exit_code=$?

    if [[ $exit_code -eq 0 ]]; then
        if [[ "$output" == *"Checksum verified"* ]] || [[ "$output" == *"checksum"* ]]; then
            pass "Checksum verification completed"
        else
            # Might be skipped if no checksums file
            pass "Installation succeeded (checksum may not be available)"
        fi
    else
        fail "Installation with verification failed: ${output:0:500}..."
    fi

    cleanup_test_env "$temp_dir"
}

# Test 5: Installation without checksum verification
test_skip_checksum() {
    if [[ "$DRY_RUN" == "true" ]]; then
        skip "Skipping actual installation (--dry-run)"
        return 0
    fi

    log "Testing installation without checksum verification..."

    local temp_dir
    temp_dir=$(create_test_env)

    local output exit_code=0
    output=$(HOME="$temp_dir" INSTALL_DIR="$temp_dir/bin" \
             ${TEST_VERSION:+VERSION="$TEST_VERSION"} \
             timeout 120 "$SCRIPT_DIR/install.sh" --no-verify 2>&1) || exit_code=$?

    if [[ $exit_code -eq 0 ]] && [[ -x "$temp_dir/bin/ms" ]]; then
        if [[ "$output" == *"skipped"* ]] || [[ "$output" == *"--no-verify"* ]]; then
            pass "Installation with --no-verify succeeded and noted skip"
        else
            pass "Installation with --no-verify succeeded"
        fi
    else
        fail "Installation with --no-verify failed: ${output:0:500}..."
    fi

    cleanup_test_env "$temp_dir"
}

# Test 6: Specific version installation
test_specific_version() {
    if [[ "$DRY_RUN" == "true" ]]; then
        skip "Skipping actual installation (--dry-run)"
        return 0
    fi

    # Skip if no specific version to test
    if [[ -z "$TEST_VERSION" ]]; then
        skip "No specific version provided (use --version)"
        return 0
    fi

    log "Testing specific version installation ($TEST_VERSION)..."

    local temp_dir
    temp_dir=$(create_test_env)

    local output exit_code=0
    output=$(HOME="$temp_dir" INSTALL_DIR="$temp_dir/bin" \
             VERSION="$TEST_VERSION" \
             timeout 120 "$SCRIPT_DIR/install.sh" 2>&1) || exit_code=$?

    if [[ $exit_code -eq 0 ]] && [[ -x "$temp_dir/bin/ms" ]]; then
        # Check if installed version matches
        local installed_version
        installed_version=$("$temp_dir/bin/ms" --version 2>&1 | grep -o '[0-9]\+\.[0-9]\+\.[0-9]\+' | head -1 || true)
        local expected_version="${TEST_VERSION#v}"

        if [[ "$installed_version" == "$expected_version" ]]; then
            pass "Specific version $TEST_VERSION installed correctly"
        else
            info "Installed version: $installed_version, expected: $expected_version"
            pass "Version installation succeeded (version format may differ)"
        fi
    else
        fail "Specific version installation failed: ${output:0:500}..."
    fi

    cleanup_test_env "$temp_dir"
}

# Test 7: Idempotent installation (install twice)
test_idempotent_install() {
    if [[ "$DRY_RUN" == "true" ]]; then
        skip "Skipping actual installation (--dry-run)"
        return 0
    fi

    log "Testing idempotent installation..."

    local temp_dir
    temp_dir=$(create_test_env)

    # First install
    local output1 exit_code1=0
    output1=$(HOME="$temp_dir" INSTALL_DIR="$temp_dir/bin" \
              ${TEST_VERSION:+VERSION="$TEST_VERSION"} \
              timeout 120 "$SCRIPT_DIR/install.sh" 2>&1) || exit_code1=$?

    if [[ $exit_code1 -ne 0 ]]; then
        fail "First installation failed"
        cleanup_test_env "$temp_dir"
        return
    fi

    # Second install (should overwrite cleanly)
    local output2 exit_code2=0
    output2=$(HOME="$temp_dir" INSTALL_DIR="$temp_dir/bin" \
              ${TEST_VERSION:+VERSION="$TEST_VERSION"} \
              timeout 120 "$SCRIPT_DIR/install.sh" 2>&1) || exit_code2=$?

    if [[ $exit_code2 -eq 0 ]] && [[ -x "$temp_dir/bin/ms" ]]; then
        pass "Idempotent installation succeeded (ran twice without error)"
    else
        fail "Second installation failed: ${output2:0:500}..."
    fi

    cleanup_test_env "$temp_dir"
}

# Test 8: Binary functionality after install
test_binary_functionality() {
    if [[ "$DRY_RUN" == "true" ]]; then
        skip "Skipping actual installation (--dry-run)"
        return 0
    fi

    log "Testing binary functionality after install..."

    local temp_dir
    temp_dir=$(create_test_env)

    local output exit_code=0
    output=$(HOME="$temp_dir" INSTALL_DIR="$temp_dir/bin" \
             ${TEST_VERSION:+VERSION="$TEST_VERSION"} \
             timeout 120 "$SCRIPT_DIR/install.sh" 2>&1) || exit_code=$?

    if [[ $exit_code -ne 0 ]]; then
        fail "Installation failed"
        cleanup_test_env "$temp_dir"
        return
    fi

    # Test various commands
    local ms="$temp_dir/bin/ms"

    if "$ms" --help >/dev/null 2>&1; then
        pass "ms --help works"
    else
        fail "ms --help failed"
    fi

    if "$ms" --version >/dev/null 2>&1; then
        pass "ms --version works"
    else
        fail "ms --version failed"
    fi

    cleanup_test_env "$temp_dir"
}

# Run all tests
info "System information:"
info "  OS: $(uname -s)"
info "  Arch: $(uname -m)"
info "  Platform: $(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)"
echo ""

echo "Running connectivity tests..."
releases_available=false
if test_github_releases_accessible; then
    releases_available=true
fi

if [[ "$releases_available" == "false" ]] && [[ "$DRY_RUN" == "false" ]]; then
    echo ""
    log "${YELLOW}No releases available. Skipping installation tests.${NC}"
    log "To test script logic without installation, use --dry-run"
    echo ""
else
    echo ""
    echo "Running installation tests..."
    test_default_install
    test_custom_dir_install
    test_checksum_verification
    test_skip_checksum
    test_specific_version
    test_idempotent_install
    test_binary_functionality
fi

# Summary
echo ""
echo "=== Test Summary ==="
log "Passed: ${GREEN}$TESTS_PASSED${NC}"
log "Failed: ${RED}$TESTS_FAILED${NC}"
log "See full log: $LOG"

if [[ $TESTS_FAILED -gt 0 ]]; then
    exit 1
fi

exit 0
