#!/bin/bash
# Spectre Network — end-to-end latency benchmark
# Runs a scrape + polish cycle, then measures request latency through a live
# SOCKS5 chain for each mode.
#
# Prerequisites: build the Go orchestrator first
#   cargo build --release
#   CGO_ENABLED=1 go build -ldflags="-s -w" -o spectre orchestrator.go scraper.go

set -e

echo "=== Spectre Benchmark ==="

# 1. Full scrape + polish (populates proxies_*.json)
echo "[1/2] Scraping + polishing proxies (limit=100)..."
./spectre run --mode phantom --limit 100 --protocol all

# Function to run a live serve benchmark for a given mode
run_test() {
    MODE=$1
    PORT=11080  # use a non-standard port to avoid conflict with any running server
    echo ""
    echo "------------------------------------------------"
    echo "Testing mode: $MODE"

    # Start server in background
    ./spectre serve --mode "$MODE" --port $PORT > "spectre_${MODE}.log" 2>&1 &
    PID=$!

    # Wait for SOCKS5 listener to come up (max 10 s)
    for i in $(seq 1 10); do
        if curl -s --socks5-hostname 127.0.0.1:$PORT --max-time 2 https://api.ipify.org > /dev/null 2>&1; then
            break
        fi
        sleep 1
    done

    echo "Measuring latency (socks5h → api.ipify.org)..."
    START=$(date +%s%N)
    HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
        -x socks5h://127.0.0.1:$PORT \
        --max-time 20 \
        https://api.ipify.org || echo "FAILED")
    END=$(date +%s%N)
    DURATION=$(( ($END - $START) / 1000000 ))

    if [ "$HTTP_CODE" == "200" ]; then
        echo "  ✓ Success — ${DURATION} ms"
    else
        echo "  ✗ Failed (HTTP $HTTP_CODE). Last log lines:"
        tail -n 5 "spectre_${MODE}.log"
    fi

    kill $PID 2>/dev/null || true
    wait $PID 2>/dev/null || true
    sleep 1
}

echo ""
echo "[2/2] Running per-mode benchmarks..."
run_test "lite"
run_test "phantom"

echo ""
echo "=== Benchmark Complete ==="
