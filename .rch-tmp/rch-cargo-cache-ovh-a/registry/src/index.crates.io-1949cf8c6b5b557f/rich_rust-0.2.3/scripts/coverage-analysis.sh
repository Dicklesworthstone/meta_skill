#!/usr/bin/env bash
# Coverage analysis script for rich_rust
#
# This script generates detailed coverage reports including:
# - Per-module coverage breakdown
# - Uncovered functions detection
# - Coverage gaps identification
# - JSON summary for CI integration
#
# Usage:
#   ./scripts/coverage-analysis.sh          # Run with default settings
#   ./scripts/coverage-analysis.sh --json   # Output JSON summary
#   ./scripts/coverage-analysis.sh --html   # Generate HTML report
#   ./scripts/coverage-analysis.sh --check  # Check against thresholds

set -euo pipefail

# Configuration
THRESHOLD_OVERALL=${THRESHOLD_OVERALL:-70}
THRESHOLD_MODULE=${THRESHOLD_MODULE:-50}
OUTPUT_DIR="${OUTPUT_DIR:-target/coverage}"
REPORT_FILE="${OUTPUT_DIR}/coverage-report.txt"
JSON_FILE="${OUTPUT_DIR}/coverage-summary.json"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Ensure output directory exists
mkdir -p "${OUTPUT_DIR}"

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if cargo-llvm-cov is installed
check_tools() {
    if ! command -v cargo-llvm-cov &> /dev/null; then
        log_error "cargo-llvm-cov is not installed."
        log_info "Install with: cargo install cargo-llvm-cov"
        exit 1
    fi
}

# Generate coverage data
generate_coverage() {
    log_info "Generating coverage data..."

    # Clean previous coverage data
    cargo llvm-cov clean --workspace 2>/dev/null || true

    # Run tests with coverage
    cargo llvm-cov --all-features --workspace \
        --lcov --output-path "${OUTPUT_DIR}/lcov.info" \
        2>&1 | tee "${OUTPUT_DIR}/test-output.log"

    log_info "Coverage data generated"
}

# Generate per-module report
per_module_report() {
    log_info "Generating per-module coverage report..."

    # Get coverage for each module
    cargo llvm-cov report --all-features 2>/dev/null | tee "${REPORT_FILE}"

    echo ""
    echo "=== Per-Module Coverage Summary ==="
    echo ""

    # Parse and format the report
    # Look for source files and their coverage
    while IFS= read -r line; do
        # Skip header lines and empty lines
        if [[ "$line" == *"Filename"* ]] || [[ -z "$line" ]]; then
            continue
        fi

        # Extract coverage percentage from lines
        if [[ "$line" == *"src/"* ]]; then
            # Extract module name and coverage
            module=$(echo "$line" | awk '{print $1}' | sed 's|.*/||' | sed 's/\.rs$//')
            regions=$(echo "$line" | awk '{print $2}')
            missed=$(echo "$line" | awk '{print $3}')
            coverage=$(echo "$line" | awk '{print $4}' | tr -d '%')

            if [[ -n "$coverage" ]]; then
                # Color code based on threshold
                if (( $(echo "$coverage < $THRESHOLD_MODULE" | bc -l) )); then
                    echo -e "${RED}[LOW]${NC}  $module: ${coverage}%"
                elif (( $(echo "$coverage < $THRESHOLD_OVERALL" | bc -l) )); then
                    echo -e "${YELLOW}[MED]${NC}  $module: ${coverage}%"
                else
                    echo -e "${GREEN}[OK]${NC}   $module: ${coverage}%"
                fi
            fi
        fi
    done < "${REPORT_FILE}"
}

# Detect uncovered functions
uncovered_functions() {
    log_info "Detecting uncovered functions..."

    echo ""
    echo "=== Functions with 0% Coverage ==="
    echo ""

    # Use llvm-cov to show uncovered functions
    cargo llvm-cov report --all-features 2>/dev/null | \
        grep -E "^src/.*0\.00%" | \
        head -20 || echo "No completely uncovered functions found"
}

# Generate HTML report
html_report() {
    log_info "Generating HTML coverage report..."

    cargo llvm-cov report --html --all-features \
        --output-dir "${OUTPUT_DIR}/html" 2>/dev/null

    log_info "HTML report generated at: ${OUTPUT_DIR}/html/index.html"
}

# Generate JSON summary for CI
json_summary() {
    log_info "Generating JSON summary..."

    # Get overall coverage percentage
    overall=$(cargo llvm-cov report --all-features 2>/dev/null | \
        grep -E "^TOTAL" | awk '{print $4}' | tr -d '%' || echo "0")

    # Generate JSON
    cat > "${JSON_FILE}" << EOF
{
  "overall_coverage": ${overall:-0},
  "threshold_overall": ${THRESHOLD_OVERALL},
  "threshold_module": ${THRESHOLD_MODULE},
  "pass": $([ "${overall:-0}" -ge "${THRESHOLD_OVERALL}" ] && echo "true" || echo "false"),
  "generated_at": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "report_path": "${REPORT_FILE}",
  "html_path": "${OUTPUT_DIR}/html/index.html"
}
EOF

    log_info "JSON summary saved to: ${JSON_FILE}"
    cat "${JSON_FILE}"
}

# Check coverage against thresholds
check_thresholds() {
    log_info "Checking coverage against thresholds..."

    overall=$(cargo llvm-cov report --all-features 2>/dev/null | \
        grep -E "^TOTAL" | awk '{print $4}' | tr -d '%' || echo "0")

    echo ""
    echo "=== Coverage Check ==="
    echo "Overall Coverage: ${overall}%"
    echo "Threshold: ${THRESHOLD_OVERALL}%"
    echo ""

    if (( $(echo "${overall:-0} >= $THRESHOLD_OVERALL" | bc -l) )); then
        log_info "Coverage check PASSED"
        return 0
    else
        log_error "Coverage check FAILED"
        log_error "Coverage ${overall}% is below threshold ${THRESHOLD_OVERALL}%"
        return 1
    fi
}

# Show usage
usage() {
    echo "Usage: $0 [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  --json     Output JSON summary"
    echo "  --html     Generate HTML report"
    echo "  --check    Check against thresholds (exit 1 if below)"
    echo "  --full     Run full analysis (default)"
    echo "  --help     Show this help message"
    echo ""
    echo "Environment variables:"
    echo "  THRESHOLD_OVERALL  Overall coverage threshold (default: 70)"
    echo "  THRESHOLD_MODULE   Per-module threshold (default: 50)"
    echo "  OUTPUT_DIR         Output directory (default: target/coverage)"
}

# Main entry point
main() {
    local action="full"

    while [[ $# -gt 0 ]]; do
        case "$1" in
            --json)
                action="json"
                shift
                ;;
            --html)
                action="html"
                shift
                ;;
            --check)
                action="check"
                shift
                ;;
            --full)
                action="full"
                shift
                ;;
            --help|-h)
                usage
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                usage
                exit 1
                ;;
        esac
    done

    check_tools
    generate_coverage

    case "$action" in
        json)
            json_summary
            ;;
        html)
            html_report
            ;;
        check)
            per_module_report
            check_thresholds
            ;;
        full)
            per_module_report
            uncovered_functions
            html_report
            json_summary
            check_thresholds || true  # Don't fail the script
            ;;
    esac
}

main "$@"
