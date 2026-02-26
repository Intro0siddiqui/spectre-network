# Spectre Network - Context & Instructions

## Project Overview
Spectre Network is a high-performance, adversarial proxy mesh designed for multi-hop, encrypted anonymity. It employs a strict separation of concerns between Go and Rust.

### Core Architectural Mandate
- **Go (Networking & Orchestration)**: Handles 100% of network-facing operations. This includes proxy scraping, live connectivity validation, CLI orchestration, and the SOCKS5 server/tunneling layer.
- **Rust (System Processing)**: Strictly isolated from the network. Rust is responsible for high-performance system tasks: AES-256-GCM encryption/decryption primitives, proxy scoring logic, tiering, and complex chain topology calculations.

### Core Technologies
- **Go**: CLI orchestration, concurrent proxy scraping, connectivity validation, and network listeners.
- **Rust**: Memory-safe encryption, scoring algorithms, and topology generation.
- **FFI/CGO**: Go statically links the Rust engine (`librotator_rs.a`) to perform system-heavy calculations.

---

## Building and Running

### Build Commands
- **Quick Build**: `./build.sh` (or `./build.sh install` to place in `~/.local/bin`).
- **Manual Build**:
  1. `cargo build --release` (produces `librotator_rs.a`).
  2. `CGO_ENABLED=1 go build -ldflags="-s -w -extldflags '-static'" -o spectre orchestrator.go scraper.go`.

### Key Commands
- `spectre run`: Scrape, validate (Go), and build a chain (Rust).
- `spectre serve`: Start the SOCKS5 server (Go) using the encrypted relay logic.
- `spectre audit`: Run the containerized security leak test (requires Podman).
- `spectre stats`: View current proxy pool health.

---

## Development Conventions

### Strict Boundary Enforcement
- **No Networking in Rust**: Rust code must never import `std::net` or async networking crates (like `tokio::net`). All I/O must be passed through the FFI boundary from Go.
- **Validation**: All proxy health checks and pings must be performed in Go before passing data to Rust for scoring.
- **FFI Bridge**: `src/lib.rs` handles the conversion of Go-provided network data into Rust-internal system structures.

### Testing & Quality
- **Security Audit**: Use `spectre audit` to verify anonymity and leak protection.
- **Benchmarking**: Use `benchmark.sh` for latency and throughput testing.
- **Logging**: Ensure Rust scoring logic respects the Tier/Score thresholds defined by the user.

### Deployment
- Default deployment uses `Containerfile` with a non-root user (`spectre`, UID 2000).
- Encryption keys are **memory-only** and must never be persisted to disk.
