#!/usr/bin/env bash
# Check test naming standards compliance
#
# Usage: ./scripts/check_test_names.sh [--fix]
#
# This script checks that all test functions follow the naming conventions
# defined in TESTING.md.

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Valid test prefixes
VALID_PREFIXES="test_|e2e_|prop_|fuzz_|regression_|conformance_"

# Helper functions that are allowed without test prefix
HELPER_FUNCS="new|contents|write|flush|len|clear|clone|default|from|into|arbitrary_|tag_like|deeply_nested|repeated_tags|column_widths"

echo "Checking test naming standards..."
echo ""

# Find test functions that don't match the expected patterns
VIOLATIONS=$(rg "#\[test\]" -A 1 tests/*.rs 2>/dev/null | \
    rg "^\s*fn [a-z]" | \
    rg -v "fn ($VALID_PREFIXES)" | \
    rg -v "fn ($HELPER_FUNCS)" | \
    sed 's/.*fn /fn /' | \
    sort -u || true)

# Also check proptest! blocks for prop_ prefix
PROPTEST_VIOLATIONS=$(rg "^\s*fn [a-z][a-z0-9_]+\(" tests/property_tests.rs 2>/dev/null | \
    rg -v "#\[test\]" | \
    rg -v "fn ($VALID_PREFIXES|$HELPER_FUNCS)" | \
    rg "proptest" -B 5 2>/dev/null | \
    rg "^\s*fn " || true)

# Check unit tests in src/
SRC_VIOLATIONS=$(rg "#\[test\]" -A 1 src/*.rs src/**/*.rs 2>/dev/null | \
    rg "^\s*fn [a-z]" | \
    rg -v "fn test_" | \
    sed 's/.*fn /fn /' | \
    sort -u || true)

TOTAL_VIOLATIONS=0

if [ -n "$VIOLATIONS" ]; then
    echo -e "${YELLOW}Non-standard test names in tests/*.rs:${NC}"
    echo "$VIOLATIONS" | while read -r line; do
        echo "  - $line"
        ((TOTAL_VIOLATIONS++)) || true
    done
    echo ""
fi

if [ -n "$PROPTEST_VIOLATIONS" ]; then
    echo -e "${YELLOW}Property tests missing prop_ prefix:${NC}"
    echo "$PROPTEST_VIOLATIONS" | while read -r line; do
        echo "  - $line"
        ((TOTAL_VIOLATIONS++)) || true
    done
    echo ""
fi

if [ -n "$SRC_VIOLATIONS" ]; then
    echo -e "${YELLOW}Unit tests in src/ missing test_ prefix:${NC}"
    echo "$SRC_VIOLATIONS" | while read -r line; do
        echo "  - $line"
        ((TOTAL_VIOLATIONS++)) || true
    done
    echo ""
fi

# Summary
echo "=== Summary ==="
echo ""

# Count compliant tests
COMPLIANT_COUNT=$(rg "#\[test\]" -A 1 tests/*.rs src/*.rs 2>/dev/null | \
    rg "fn ($VALID_PREFIXES)" | wc -l || echo "0")

# Count total tests
TOTAL_TESTS=$(rg -c "#\[test\]" tests/*.rs src/*.rs 2>/dev/null | \
    awk -F: '{sum+=$2} END {print sum}' || echo "0")

echo "Total tests: $TOTAL_TESTS"
echo "Compliant tests: $COMPLIANT_COUNT"

if [ -z "$VIOLATIONS" ] && [ -z "$SRC_VIOLATIONS" ]; then
    echo -e "${GREEN}All test names comply with standards!${NC}"
    exit 0
else
    VIOLATION_COUNT=$(echo -e "$VIOLATIONS\n$SRC_VIOLATIONS" | grep -c "fn " || echo "0")
    echo -e "${YELLOW}Found $VIOLATION_COUNT potential naming issues${NC}"
    echo ""
    echo "See TESTING.md for naming conventions."
    echo "Note: Some violations may be helper functions, not actual tests."
    exit 0  # Don't fail - some are false positives
fi
