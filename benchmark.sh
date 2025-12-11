#!/bin/bash
set -e

echo "=== Spectre Benchmark ==="

# 1. Scrape Proxies (needed for pools)
echo "[1/4] Scraping proxies..."
./spectre --step scrape --limit 100 --protocol all
if [ $? -ne 0 ]; then
    echo "Scraping failed."
    exit 1
fi

# 2. Polish Proxies
echo "[2/4] Polishing proxies..."
./spectre --step polish
if [ $? -ne 0 ]; then
    echo "Polishing failed."
    exit 1
fi

# Function to run benchmark
run_test() {
    MODE=$1
    echo "------------------------------------------------"
    echo "Testing Mode: $MODE"
    
    # Start server in background
    ./spectre --step serve --mode "$MODE" --port 1080 > "spectre_${MODE}.log" 2>&1 &
    PID=$!
    
    # Wait for server to initialize (give it 5 seconds)
    sleep 5
    
    echo "Resolving IP..."
    # Measure time to fetch IP
    # We use -w to format output: time_total in seconds
    START=$(date +%s%N)
    
    # Using socks5h to resolve DNS remotely through proxy if supported/needed
    # The 'lite' mode might be socks4/http, but tunnel.rs seems to handle socks5 handshake.
    # We'll try curl. If it fails, we record that.
    
    HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" -x socks5h://127.0.0.1:1080 --max-time 20 https://api.ipify.org || echo "FAILED")
    
    END=$(date +%s%N)
    DURATION=$(( ($END - $START) / 1000000 ))
    
    if [ "$HTTP_CODE" == "200" ]; then
        echo "Success! Time taken: ${DURATION} ms"
    else
        echo "Request Failed (Code: $HTTP_CODE). Check log."
        cat "spectre_${MODE}.log" | tail -n 10
    fi
    
    # Cleanup
    kill $PID
    wait $PID 2>/dev/null || true
    sleep 2
}

# 3. Test Base Mode (Lite)
run_test "lite"

# 4. Test Different Mode (Phantom)
run_test "phantom"

echo "=== Benchmark Complete ==="
