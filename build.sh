#!/bin/bash
# Spectre Network — Build Script
# Builds the spectre binary and optionally installs it globally
#
# Usage:
#   ./build.sh          # Build only
#   ./build.sh install  # Build + install to ~/.local/bin
#   ./build.sh --help   # Show help

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INSTALL_DIR="$HOME/.local/bin"

print_help() {
    cat << EOF
Spectre Network Build Script

USAGE:
    ./build.sh [COMMAND]

COMMANDS:
    build       Build spectre binary (default)
    install     Build + install to ~/.local/bin
    uninstall   Remove from ~/.local/bin
    clean       Remove build artifacts
    --help      Show this help

EXAMPLES:
    ./build.sh                    # Build only
    ./build.sh install            # Build and install globally
    ./build.sh uninstall          # Remove installed binary

EOF
}

build() {
    echo "=== Building Spectre Network ==="
    cd "$SCRIPT_DIR"

    echo "[1/2] Building Rust engine..."
    cargo build --release

    echo "[2/2] Building spectre binary (static)..."
    CGO_ENABLED=1 go build -ldflags="-s -w -extldflags '-static'" -o spectre orchestrator.go scraper.go

    echo ""
    echo "✓ Build complete: $(ls -lh spectre | awk '{print $5, $9}')"
    echo ""
    echo "Run with: LD_LIBRARY_PATH=./target/release:$LD_LIBRARY_PATH ./spectre --help"
    echo "Or install globally with: ./build.sh install"
}

install() {
    build

    echo "[3/3] Installing to $INSTALL_DIR..."
    mkdir -p "$INSTALL_DIR"
    cp "$SCRIPT_DIR/spectre" "$INSTALL_DIR/spectre"
    chmod +x "$INSTALL_DIR/spectre"

    # Also build and install spectre-audit
    echo "Building spectre-audit..."
    cd "$SCRIPT_DIR/security-audit"
    go build -o "$INSTALL_DIR/spectre-audit" .
    chmod +x "$INSTALL_DIR/spectre-audit"

    echo ""
    echo "✓ Installed to: $INSTALL_DIR/"
    echo "  - spectre"
    echo "  - spectre-audit"
    echo ""

    # Check if INSTALL_DIR is in PATH
    if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
        echo "⚠ WARNING: $INSTALL_DIR is not in your PATH"
        echo ""
        echo "Add it to your shell config:"
        echo "  echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.bashrc"
        echo "  source ~/.bashrc"
        echo ""
        echo "Or run with full path: $INSTALL_DIR/spectre --help"
    else
        echo "✓ $INSTALL_DIR is in PATH"
        echo ""
        echo "Run: spectre --help"
    fi
}

uninstall() {
    echo "=== Uninstalling Spectre Network ==="

    if [ -f "$INSTALL_DIR/spectre" ]; then
        rm "$INSTALL_DIR/spectre"
        echo "✓ Removed: $INSTALL_DIR/spectre"
    else
        echo "⚠ Not installed at $INSTALL_DIR/spectre"
    fi

    if [ -f "$INSTALL_DIR/spectre-audit" ]; then
        rm "$INSTALL_DIR/spectre-audit"
        echo "✓ Removed: $INSTALL_DIR/spectre-audit"
    fi
}

clean() {
    echo "=== Cleaning Build Artifacts ==="
    cd "$SCRIPT_DIR"

    rm -f spectre spectre-audit go_scraper
    cargo clean

    echo "✓ Clean complete"
}

# Main
case "${1:-build}" in
    build)
        build
        ;;
    install)
        install
        ;;
    uninstall)
        uninstall
        ;;
    clean)
        clean
        ;;
    --help|-h)
        print_help
        ;;
    *)
        echo "Unknown command: $1"
        echo ""
        print_help
        exit 1
        ;;
esac
