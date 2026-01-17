#!/usr/bin/env bash
# test-homebrew.sh - E2E test for Homebrew tap installation
#
# This script tests the complete Homebrew installation flow for ms.
# It should be run on macOS or Linux with Homebrew installed.
#
# Usage:
#   ./scripts/test-homebrew.sh           # Full test with cleanup
#   ./scripts/test-homebrew.sh --skip-cleanup  # Keep ms installed after test
#   ./scripts/test-homebrew.sh --local   # Test local formula (for development)

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
TAP_NAME="dicklesworthstone/tap"
FORMULA_NAME="ms"
LOG_FILE="/tmp/ms-homebrew-test-$(date +%Y%m%d-%H%M%S).log"

# Parse arguments
SKIP_CLEANUP=false
USE_LOCAL_FORMULA=false
LOCAL_FORMULA_PATH=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --skip-cleanup)
            SKIP_CLEANUP=true
            shift
            ;;
        --local)
            USE_LOCAL_FORMULA=true
            shift
            if [[ $# -gt 0 && ! "$1" =~ ^-- ]]; then
                LOCAL_FORMULA_PATH="$1"
                shift
            fi
            ;;
        -h|--help)
            echo "Usage: $0 [--skip-cleanup] [--local [formula-path]]"
            echo ""
            echo "Options:"
            echo "  --skip-cleanup    Don't uninstall ms after testing"
            echo "  --local [path]    Test local formula file instead of tap"
            echo "  -h, --help        Show this help message"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Ensure tee captures all output
exec 1> >(tee -a "$LOG_FILE") 2>&1

log() {
    echo -e "${BLUE}[$(date +%H:%M:%S)]${NC} $*"
}

success() {
    echo -e "${GREEN}[$(date +%H:%M:%S)] ✓${NC} $*"
}

warn() {
    echo -e "${YELLOW}[$(date +%H:%M:%S)] ⚠${NC} $*"
}

error() {
    echo -e "${RED}[$(date +%H:%M:%S)] ✗${NC} $*"
}

# Check prerequisites
check_prerequisites() {
    log "Checking prerequisites..."

    if ! command -v brew &> /dev/null; then
        error "Homebrew is not installed. Please install it first:"
        echo "  /bin/bash -c \"\$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)\""
        exit 1
    fi
    success "Homebrew is installed: $(brew --version | head -1)"

    # Check if ms is already installed
    if brew list --formula | grep -q "^${FORMULA_NAME}$"; then
        warn "ms is already installed via Homebrew"
        if [[ "$USE_LOCAL_FORMULA" == "true" ]]; then
            log "Uninstalling existing ms for local formula test..."
            brew uninstall "$FORMULA_NAME" || true
        fi
    fi
}

# Test tap addition
test_tap_add() {
    log "Testing tap addition..."

    # Remove tap if it exists (for clean test)
    if brew tap | grep -q "^${TAP_NAME}$"; then
        log "Removing existing tap for clean test..."
        brew untap "$TAP_NAME" || true
    fi

    # Add tap
    log "Adding tap: $TAP_NAME"
    if brew tap "$TAP_NAME"; then
        success "Tap added successfully"
    else
        error "Failed to add tap"
        exit 1
    fi

    # Verify tap is listed
    if brew tap | grep -q "^${TAP_NAME}$"; then
        success "Tap is listed in brew tap"
    else
        error "Tap not found in brew tap list"
        exit 1
    fi
}

# Test installation
test_install() {
    log "Testing installation..."

    if [[ "$USE_LOCAL_FORMULA" == "true" ]]; then
        if [[ -n "$LOCAL_FORMULA_PATH" && -f "$LOCAL_FORMULA_PATH" ]]; then
            log "Installing from local formula: $LOCAL_FORMULA_PATH"
            brew install "$LOCAL_FORMULA_PATH"
        else
            error "Local formula path not specified or file not found"
            exit 1
        fi
    else
        log "Installing ${TAP_NAME}/${FORMULA_NAME}..."
        if brew install "${TAP_NAME}/${FORMULA_NAME}"; then
            success "ms installed successfully"
        else
            error "Failed to install ms"
            exit 1
        fi
    fi

    # Verify installation
    if command -v ms &> /dev/null; then
        success "ms binary is available in PATH"
    else
        error "ms binary not found in PATH"
        exit 1
    fi
}

# Test basic functionality
test_basic_commands() {
    log "Testing basic commands..."

    # Version
    local version
    version=$(ms --version 2>&1)
    log "Version output: $version"
    if [[ "$version" =~ ^ms[[:space:]][0-9]+\.[0-9]+ ]]; then
        success "--version works"
    else
        warn "--version output format unexpected (may still be valid)"
    fi

    # Help
    if ms --help > /dev/null 2>&1; then
        success "--help works"
    else
        error "--help failed"
        exit 1
    fi

    # Doctor (quick check)
    log "Running ms doctor..."
    if ms doctor 2>&1; then
        success "doctor command works"
    else
        warn "doctor command had warnings (this may be expected if not initialized)"
    fi

    # List (may fail if not initialized, that's OK)
    log "Testing ms list..."
    if ms list --limit=1 2>&1; then
        success "list command works"
    else
        warn "list returned no skills (expected if not initialized)"
    fi
}

# Test shell completions (if installed)
test_completions() {
    log "Testing shell completions..."

    # Check if completions were installed
    local completions_dir
    completions_dir="$(brew --prefix)/share/bash-completion/completions"

    if [[ -f "${completions_dir}/ms" ]]; then
        success "Bash completions installed"
    else
        warn "Bash completions not found (may not be included in formula)"
    fi

    # Check zsh completions
    completions_dir="$(brew --prefix)/share/zsh/site-functions"
    if [[ -f "${completions_dir}/_ms" ]]; then
        success "Zsh completions installed"
    else
        warn "Zsh completions not found (may not be included in formula)"
    fi
}

# Test upgrade path
test_upgrade() {
    log "Testing upgrade..."

    if [[ "$USE_LOCAL_FORMULA" == "true" ]]; then
        log "Skipping upgrade test for local formula"
        return
    fi

    if brew upgrade "${TAP_NAME}/${FORMULA_NAME}" 2>&1; then
        success "Upgrade command works (already at latest or upgraded)"
    else
        warn "Upgrade command had issues (may be expected)"
    fi
}

# Cleanup
cleanup() {
    log "Cleaning up..."

    if [[ "$SKIP_CLEANUP" == "true" ]]; then
        warn "Skipping cleanup (--skip-cleanup specified)"
        log "To manually cleanup later, run:"
        echo "  brew uninstall ms"
        echo "  brew untap $TAP_NAME"
        return
    fi

    # Uninstall ms
    if brew list --formula | grep -q "^${FORMULA_NAME}$"; then
        log "Uninstalling ms..."
        brew uninstall "$FORMULA_NAME"
        success "ms uninstalled"
    fi

    # Remove tap
    if brew tap | grep -q "^${TAP_NAME}$"; then
        log "Removing tap..."
        brew untap "$TAP_NAME"
        success "Tap removed"
    fi

    success "Cleanup complete"
}

# Main test flow
main() {
    echo ""
    echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
    echo -e "${BLUE}        ms Homebrew Tap E2E Test                           ${NC}"
    echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
    echo ""

    log "Log file: $LOG_FILE"
    log "Skip cleanup: $SKIP_CLEANUP"
    log "Use local formula: $USE_LOCAL_FORMULA"
    echo ""

    # Run tests
    check_prerequisites
    echo ""

    if [[ "$USE_LOCAL_FORMULA" != "true" ]]; then
        test_tap_add
        echo ""
    fi

    test_install
    echo ""

    test_basic_commands
    echo ""

    test_completions
    echo ""

    test_upgrade
    echo ""

    cleanup
    echo ""

    echo -e "${GREEN}═══════════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN}        All tests passed!                                  ${NC}"
    echo -e "${GREEN}═══════════════════════════════════════════════════════════${NC}"
    echo ""
    log "Log saved to: $LOG_FILE"
}

# Run main with error handling
trap 'error "Test failed! Check log: $LOG_FILE"; exit 1' ERR
main "$@"
