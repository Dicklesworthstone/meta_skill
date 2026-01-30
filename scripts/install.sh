#!/bin/bash
# ms installer - https://github.com/Dicklesworthstone/meta_skill
# Usage: curl -sSL https://raw.githubusercontent.com/Dicklesworthstone/meta_skill/main/scripts/install.sh | bash
#
# Options:
#   --install-dir DIR  Install directory (default: ~/.local/bin)
#   --version VER      Version to install (default: latest)
#   --no-verify        Skip checksum verification
#   --help             Show this help message
#
# Environment variables:
#   INSTALL_DIR        Override install directory
#   VERSION            Override version to install
#   VERIFY             Set to "false" to skip checksum verification
#   NO_COLOR           Disable colored output

set -euo pipefail

# Configuration
REPO="Dicklesworthstone/meta_skill"
BINARY_NAME="ms"
DEFAULT_INSTALL_DIR="${HOME}/.local/bin"

# Allow sourcing without running (for testing)
if [[ "${1:-}" == "--source-only" ]]; then
    return 0 2>/dev/null || exit 0
fi

# Colors (respect NO_COLOR)
if [[ -z "${NO_COLOR:-}" ]] && [[ -t 1 ]]; then
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    YELLOW='\033[0;33m'
    BLUE='\033[0;34m'
    BOLD='\033[1m'
    NC='\033[0m'
else
    RED='' GREEN='' YELLOW='' BLUE='' BOLD='' NC=''
fi

log()  { echo -e "${BLUE}[ms]${NC} $*"; }
warn() { echo -e "${YELLOW}[ms]${NC} $*"; }
err()  { echo -e "${RED}[ms]${NC} $*" >&2; }
die()  { err "$*"; exit 1; }

usage() {
    cat << EOF
${BOLD}ms installer${NC}

Usage: $0 [OPTIONS]

Options:
  --install-dir DIR  Install directory (default: ~/.local/bin)
  --version VER      Version to install (default: latest)
  --no-verify        Skip checksum verification
  --help             Show this help message

Environment variables:
  INSTALL_DIR        Override install directory
  VERSION            Override version to install
  VERIFY             Set to "false" to skip checksum verification
  NO_COLOR           Disable colored output

Examples:
  # Install latest version
  curl -sSL https://raw.githubusercontent.com/Dicklesworthstone/meta_skill/main/scripts/install.sh | bash

  # Install specific version
  curl -sSL https://raw.githubusercontent.com/Dicklesworthstone/meta_skill/main/scripts/install.sh | VERSION=v0.1.5 bash

  # Install to custom directory
  curl -sSL https://raw.githubusercontent.com/Dicklesworthstone/meta_skill/main/scripts/install.sh | INSTALL_DIR=/usr/local/bin bash
EOF
}

# Detect platform
detect_platform() {
    local os arch
    os="$(uname -s | tr '[:upper:]' '[:lower:]')"
    arch="$(uname -m)"

    case "$os" in
        linux)
            os="unknown-linux-gnu"
            ;;
        darwin)
            os="apple-darwin"
            ;;
        mingw*|msys*|cygwin*)
            os="pc-windows-msvc"
            ;;
        *)
            die "Unsupported OS: $os"
            ;;
    esac

    case "$arch" in
        x86_64|amd64)
            arch="x86_64"
            ;;
        aarch64|arm64)
            arch="aarch64"
            ;;
        *)
            die "Unsupported architecture: $arch"
            ;;
    esac

    echo "${arch}-${os}"
}

# Fetch latest version from GitHub API
fetch_latest_version() {
    local response version

    if command -v curl >/dev/null 2>&1; then
        response=$(curl -sS "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null) || {
            die "Failed to fetch latest version. Check your internet connection."
        }
    elif command -v wget >/dev/null 2>&1; then
        response=$(wget -qO- "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null) || {
            die "Failed to fetch latest version. Check your internet connection."
        }
    else
        die "Neither curl nor wget found. Please install one of them."
    fi

    version=$(echo "$response" | grep -o '"tag_name": "[^"]*"' | head -1 | cut -d'"' -f4)

    if [[ -z "$version" ]]; then
        die "Could not determine latest version. The response was: $response"
    fi

    echo "$version"
}

# Download with progress
download() {
    local url="$1" dest="$2"

    log "Downloading from $url..."

    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$url" -o "$dest" || {
            die "Download failed: $url"
        }
    elif command -v wget >/dev/null 2>&1; then
        wget -q "$url" -O "$dest" || {
            die "Download failed: $url"
        }
    else
        die "Neither curl nor wget found"
    fi
}

# Verify checksum
verify_checksum() {
    local binary="$1" checksums="$2"
    local expected actual binary_name

    if [[ "${VERIFY:-true}" != "true" ]]; then
        warn "Checksum verification skipped (--no-verify)"
        return 0
    fi

    if [[ ! -f "$checksums" ]]; then
        warn "Checksums file not found, skipping verification"
        return 0
    fi

    binary_name=$(basename "$binary")
    expected=$(grep -E "\s${binary_name}$" "$checksums" 2>/dev/null | awk '{print $1}' || true)

    if [[ -z "$expected" ]]; then
        warn "No checksum found for $binary_name, skipping verification"
        return 0
    fi

    # Use sha256sum on Linux, shasum on macOS
    if command -v sha256sum >/dev/null 2>&1; then
        actual=$(sha256sum "$binary" | awk '{print $1}')
    elif command -v shasum >/dev/null 2>&1; then
        actual=$(shasum -a 256 "$binary" | awk '{print $1}')
    else
        warn "No SHA256 tool found, skipping verification"
        return 0
    fi

    if [[ "$expected" != "$actual" ]]; then
        die "Checksum mismatch! Expected: $expected, Got: $actual"
    fi

    log "Checksum verified ${GREEN}✓${NC}"
}

# Parse arguments
parse_args() {
    INSTALL_DIR="${INSTALL_DIR:-$DEFAULT_INSTALL_DIR}"
    VERSION="${VERSION:-latest}"
    VERIFY="${VERIFY:-true}"

    while [[ $# -gt 0 ]]; do
        case "$1" in
            --install-dir)
                INSTALL_DIR="$2"
                shift 2
                ;;
            --version)
                VERSION="$2"
                shift 2
                ;;
            --no-verify)
                VERIFY="false"
                shift
                ;;
            --help|-h)
                usage
                exit 0
                ;;
            *)
                die "Unknown option: $1. Use --help for usage."
                ;;
        esac
    done
}

# Main installation
main() {
    parse_args "$@"

    log "${BOLD}Installing ms...${NC}"

    # Detect platform
    local platform
    platform=$(detect_platform)
    log "Detected platform: ${GREEN}$platform${NC}"

    # Get version
    if [[ "$VERSION" == "latest" ]]; then
        log "Fetching latest version..."
        VERSION=$(fetch_latest_version)
    fi
    log "Installing version: ${GREEN}$VERSION${NC}"

    # Create temp directory
    local temp_dir
    temp_dir=$(mktemp -d)
    trap 'rm -rf "${temp_dir:-}"' EXIT

    # Build download URLs
    # Adjust version for URL (strip 'v' prefix if present)
    local version_for_url="${VERSION#v}"
    local base_url="https://github.com/${REPO}/releases/download/${VERSION}"
    local archive_name="ms-${version_for_url}-${platform}.tar.gz"
    local archive_url="${base_url}/${archive_name}"
    local checksums_url="${base_url}/SHA256SUMS.txt"

    # Download archive
    download "$archive_url" "${temp_dir}/${archive_name}"

    # Download checksums (optional)
    download "$checksums_url" "${temp_dir}/SHA256SUMS.txt" 2>/dev/null || {
        warn "Could not download checksums file"
        touch "${temp_dir}/SHA256SUMS.txt"
    }

    # Extract
    log "Extracting..."
    tar -xzf "${temp_dir}/${archive_name}" -C "$temp_dir" || {
        die "Failed to extract archive"
    }

    # Find the binary (might be at root or in a subdirectory)
    local binary_path
    binary_path=$(find "$temp_dir" -name "$BINARY_NAME" -type f -executable 2>/dev/null | head -1)
    if [[ -z "$binary_path" ]]; then
        binary_path=$(find "$temp_dir" -name "$BINARY_NAME" -type f 2>/dev/null | head -1)
    fi

    if [[ -z "$binary_path" ]]; then
        die "Could not find $BINARY_NAME in archive"
    fi

    # Verify checksum
    verify_checksum "$binary_path" "${temp_dir}/SHA256SUMS.txt"

    # Install
    mkdir -p "$INSTALL_DIR"
    mv "$binary_path" "${INSTALL_DIR}/${BINARY_NAME}"
    chmod +x "${INSTALL_DIR}/${BINARY_NAME}"

    log "${GREEN}${BOLD}Successfully installed ms ${VERSION} to ${INSTALL_DIR}/${BINARY_NAME}${NC}"

    # Check PATH
    if ! echo "$PATH" | tr ':' '\n' | grep -q "^${INSTALL_DIR}$"; then
        echo ""
        warn "Add ${INSTALL_DIR} to your PATH:"
        echo ""
        echo "  For bash (add to ~/.bashrc):"
        echo "    export PATH=\"\$PATH:${INSTALL_DIR}\""
        echo ""
        echo "  For zsh (add to ~/.zshrc):"
        echo "    export PATH=\"\$PATH:${INSTALL_DIR}\""
        echo ""
        echo "  For fish (run once):"
        echo "    fish_add_path ${INSTALL_DIR}"
        echo ""
    fi

    # Run version check
    echo ""
    log "Verifying installation..."
    if "${INSTALL_DIR}/${BINARY_NAME}" --version; then
        echo ""
        log "${GREEN}Installation complete! Run 'ms --help' to get started.${NC}"
    else
        warn "Binary installed but failed to run. Please check the logs."
    fi
}

main "$@"
