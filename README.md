# Spectre Network: A Polyglot Proxy Mesh for Evolved Anonymity

## Overview
Spectre Network is an open-source, modular proxy orchestration framework designed for anonymous web scraping, browsing, and data pipelines. Born from the need for a lightweight, high-performance alternative to traditional anonymity tools like Tor, Spectre evolves the core principles of multi-hop routing and encryption while addressing Tor's known limitations.

## Architecture
- **Go Core**: Concurrent proxy farming (10+ real sources, validation, deduplication)
- **Python Polish**: Asynchronous scoring and DNS/non-DNS pool generation
- **Rust Rotator (pyo3)**: In-process multi-mode rotator with encryption metadata, consumed directly by Python

## Components
1. `go_scraper.go` - Scrapes and validates proxies from real upstream lists/APIs
2. `python_polish.py` - Deduplicates, scores, classifies into DNS/non-DNS (Legacy, now partially managed by Rust engine)
3. `src/` (Rust) - Rotation+encryption engine exposing a C API for Go.
4. `orchestrator.go` - The primary orchestrator:
   - Uses CGO to natively invoke the Rust engine `rotator_rs` in-process.
   - Replaces the legacy Java/GraalVM orchestrator.

## Setup Requirements
- Go 1.21+ for the Scraper & Orchestrator compilation
- Rust 1.74+ for building the backend engine (`librotator_rs.so`)
- `python3-dev` / `pkg-config` for CGO Python linking dependencies

## Usage

### 1. Build and run Go scraper
```bash
go build -o go_scraper go_scraper.go
./go_scraper --limit 500 --protocol all > raw_proxies.json
```

### 2. Polish proxies with Python
```bash
python3 python_polish.py --input raw_proxies.json
# generates: proxies_dns.json, proxies_non_dns.json, proxies_combined.json
```

### 3. Build the Core Ecosystem
Compile the Rust engine and the Go orchestrator:

```bash
# Build the Rust C-API engine
export PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1
cargo build --release

# Build the main orchestrator (automatically links the Rust engine via CGO)
go build -o spectre orchestrator.go
```

### 4. Run the Orchestrated Pipeline

```bash
# Full pipeline: scrape -> polish -> rust rotator (with encryption metadata)
./spectre --step full --mode phantom

# Or individual steps:
./spectre --step scrape   --limit 500 --protocol all
./spectre --step polish
./spectre --step rotate   --mode phantom
./spectre --step stats
```

The `orchestrator.go` engine natively interfaces with the Rust compiled shared library (`librotator_rs.so`) over CGO boundaries, without any Java Virtual Machine overhead or subprocess startup times.

## Operational Modes
- **Lite**: HTTP/SOCKS4 pool, local DNS (0.5s latency)
- **Stealth**: TLS-wrapped HTTP (0.8s latency)
- **High**: HTTPS/SOCKS5h pool, remote DNS (1.2s latency)
- **Phantom**: Multi-hop crypto chains with forward secrecy (2-4s latency)

## Performance Targets
- 500-2000 raw proxies/hour
- 12% live rate from free pools
- 95%+ leak resistance in phantom mode
- 1.5x faster than Tor Browser

## Security & Encryption Features

- Rust-native chain builder with:
  - Mode-aware selection:
    - Lite / Stealth / High / Phantom routing semantics
  - Unique `chain_id` per rotation
  - Per-hop random `key_hex` (32 bytes) and `nonce_hex` (12 bytes) for AEAD-ready encryption
- Designed to support:
  - Layered per-hop encryption (Onion-style) in Phantom mode
  - Correlation resistance via randomized chains and metadata
  - Future upgrades to ECDH/AES-GCM or post-quantum schemes using the provided key/nonce scaffolding

## License
MIT License - Open Source
