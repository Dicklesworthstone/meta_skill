#!/bin/bash
# Coverage report and gap analysis for rich_rust
# Usage: ./scripts/coverage-report.sh [--json] [--threshold N]

set -e

THRESHOLD=80
FORMAT="text"
OUTPUT_DIR="target/coverage"

while [[ $# -gt 0 ]]; do
    case $1 in
        --json)
            FORMAT="json"
            shift
            ;;
        --threshold)
            THRESHOLD=$2
            shift 2
            ;;
        --output-dir)
            OUTPUT_DIR=$2
            shift 2
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

mkdir -p "$OUTPUT_DIR"

echo "Running coverage analysis..."
echo "Threshold: ${THRESHOLD}%"
echo ""

# Generate coverage data with JSON output for parsing
cargo llvm-cov --lib --all-features --json --output-path "$OUTPUT_DIR/coverage.json" 2>/dev/null

# Generate HTML report
cargo llvm-cov --lib --all-features --html --output-dir "$OUTPUT_DIR/html" 2>/dev/null

# Generate lcov for codecov
cargo llvm-cov --lib --all-features --lcov --output-path "$OUTPUT_DIR/lcov.info" 2>/dev/null

# Parse and display results
if [ "$FORMAT" = "json" ]; then
    cat "$OUTPUT_DIR/coverage.json"
else
    echo "=== Coverage Summary ==="
    echo ""

    # Extract totals from JSON
    TOTAL_LINES=$(jq -r '.data[0].totals.lines.count' "$OUTPUT_DIR/coverage.json")
    COVERED_LINES=$(jq -r '.data[0].totals.lines.covered' "$OUTPUT_DIR/coverage.json")
    LINE_PCT=$(jq -r '.data[0].totals.lines.percent' "$OUTPUT_DIR/coverage.json")

    TOTAL_FUNCS=$(jq -r '.data[0].totals.functions.count' "$OUTPUT_DIR/coverage.json")
    COVERED_FUNCS=$(jq -r '.data[0].totals.functions.covered' "$OUTPUT_DIR/coverage.json")
    FUNC_PCT=$(jq -r '.data[0].totals.functions.percent' "$OUTPUT_DIR/coverage.json")

    echo "Lines:     ${COVERED_LINES}/${TOTAL_LINES} (${LINE_PCT}%)"
    echo "Functions: ${COVERED_FUNCS}/${TOTAL_FUNCS} (${FUNC_PCT}%)"
    echo ""

    echo "=== Per-Module Coverage ==="
    echo ""
    printf "%-40s %8s %8s %8s\n" "Module" "Lines" "Funcs" "Status"
    printf "%-40s %8s %8s %8s\n" "------" "-----" "-----" "------"

    # Process each file
    jq -r '.data[0].files[] | "\(.filename)|\(.summary.lines.percent)|\(.summary.functions.percent)"' "$OUTPUT_DIR/coverage.json" | \
    while IFS='|' read -r filename line_pct func_pct; do
        # Extract just the module name
        module=$(echo "$filename" | sed 's|.*/src/||' | sed 's|\.rs$||')

        # Determine status
        line_int=${line_pct%.*}
        if [ "$line_int" -lt "$THRESHOLD" ]; then
            status="⚠️  LOW"
        elif [ "$line_int" -lt 90 ]; then
            status="📊 OK"
        else
            status="✅ GOOD"
        fi

        printf "%-40s %7.1f%% %7.1f%% %s\n" "$module" "$line_pct" "$func_pct" "$status"
    done | sort -t'%' -k2 -n

    echo ""
    echo "=== Gap Analysis ==="
    echo ""
    echo "Modules below ${THRESHOLD}% threshold:"

    jq -r '.data[0].files[] | select(.summary.lines.percent < '"$THRESHOLD"') | "\(.filename)|\(.summary.lines.percent)"' "$OUTPUT_DIR/coverage.json" | \
    while IFS='|' read -r filename line_pct; do
        module=$(echo "$filename" | sed 's|.*/src/||')
        printf "  - %-35s %6.1f%%\n" "$module" "$line_pct"
    done

    echo ""
    echo "=== Reports Generated ==="
    echo "  HTML: $OUTPUT_DIR/html/index.html"
    echo "  JSON: $OUTPUT_DIR/coverage.json"
    echo "  LCOV: $OUTPUT_DIR/lcov.info"

    # Check threshold
    LINE_INT=${LINE_PCT%.*}
    if [ "$LINE_INT" -lt "$THRESHOLD" ]; then
        echo ""
        echo "❌ FAILED: Overall coverage ${LINE_PCT}% is below threshold ${THRESHOLD}%"
        exit 1
    else
        echo ""
        echo "✅ PASSED: Overall coverage ${LINE_PCT}% meets threshold ${THRESHOLD}%"
    fi
fi
