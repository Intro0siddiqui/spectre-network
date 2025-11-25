# Spectre Network: A Polyglot Proxy Mesh for Evolved Anonymity

## Overview
Spectre Network is an open-source, modular proxy orchestration framework designed for anonymous web scraping, browsing, and data pipelines. Born from the need for a lightweight, high-performance alternative to traditional anonymity tools like Tor, Spectre evolves the core principles of multi-hop routing and encryption while addressing Tor's known limitations.

## Architecture
- **Go Core**: Concurrent proxy farming (10+ real sources, validation, deduplication)
- **Python Polish**: Asynchronous scoring and DNS/non-DNS pool generation
- **Rust Rotator (pyo3)**: In-process multi-mode rotator with encryption metadata, consumed directly by Python

## Components
1. `go_scraper.go` - Scrapes and validates proxies from real upstream lists/APIs
2. `python_polish.py` - Deduplicates, scores, classifies into DNS/non-DNS, writes:
   - `proxies_dns.json`
   - `proxies_non_dns.json`
   - `proxies_combined.json`
3. `rotator.rs` / `rotator_rs` (pyo3) - Rust rotation+encryption engine exposed as a Python module:
   - `build_chain(mode, workspace)` → JSON-like dict with:
     - mode, timestamp, chain_id
     - chain[]: per-hop proto/ip/port/country/latency/score
     - encryption[]: per-hop key_hex + nonce_hex (AEAD-ready)
4. `spectre.py` - Orchestrator:
   - Runs Go scraper → Python polish → Rust rotator (no Mojo subprocess)
   - Uses both direct `rotator_rs.build_chain` and a `SpectreRustRotator` wrapper

## Setup Requirements
- Go 1.21+ for concurrent scraping
- Python 3.8+ with aiohttp and standard JSON tooling
- Rust 1.74+ with `pyo3`, `serde`, `rand`, `serde_json` (for building the rotator)
- A supported Python/Rust toolchain for building a pyo3 extension (e.g. maturin)

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

### 3. Build the Rust rotator as a pyo3 module

Using maturin (recommended):

```bash
# Install maturin if needed
pip install maturin

# Build and develop-install rotator_rs into your current Python env
maturin develop --release
```

This produces an importable `rotator_rs` module exposing:
- `build_chain(mode: str, workspace: Optional[str]) -> dict`
- `validate_mode(mode: str)`
- `version()`

### 4. Run the orchestrated pipeline via spectre.py

```bash
# Full pipeline: scrape -> polish -> rust rotator (with encryption metadata)
python3 spectre.py --step full --mode phantom

# Or individual steps:
python3 spectre.py --step scrape   --limit 500 --protocol all
python3 spectre.py --step polish   --limit 500
python3 spectre.py --step rotate   --mode phantom
python3 spectre.py --step stats
```

SpectreOrchestrator now:
- Calls Go + Python exactly as before.
- Imports `rotator_rs` in-process (no subprocess / no Mojo).
- Uses a `SpectreRustRotator` wrapper plus direct `build_chain` integration.

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
MIT License - Open Source```

This produces an importable `rotator_rs` module exposing:
- `build_chain(mode: str, workspace: Optional[str]) -> dict`
- `validate_mode(mode: str)`
- `version()`

### 4. Run the orchestrated pipeline via spectre.py

```bash
# Full pipeline: scrape -> polish -> rust rotator (with encryption metadata)
python3 spectre.py --step full --mode phantom

# Or individual steps:
python3 spectre.py --step scrape   --limit 500 --protocol all
python3 spectre.py --step polish   --limit 500
python3 spectre.py --step rotate   --mode phantom
python3 spectre.py --step stats
```

SpectreOrchestrator now:
- Calls Go + Python exactly as before.
- Imports `rotator_rs` in-process (no subprocess / no Mojo).
- Uses a `SpectreRustRotator` wrapper plus direct `build_chain` integration.

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
