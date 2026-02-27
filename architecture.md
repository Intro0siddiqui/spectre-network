# Spectre Network: Architectural Deep Dive

Spectre Network is an adversarial proxy mesh designed to farm its own proxy pool, score it, and assemble multi-hop AES-256-GCM encrypted relay chains. It operates entirely without relying on any third-party VPN subscriptions or centralized proxy networks, prioritizing security, anonymity, and deep traffic isolation.

This document serves as an exhaustive architectural blueprint detailing the cross-language design choices, the internal workflow, and an in-depth explanation of every file in the codebase.

---

## 1. High-Level Architecture Overview

The system is split into two co-dependent halves welded into a single binary (`spectre`):
1. **The Go Orchestrator (`orchestrator.go`, `scraper.go`, `verifier.go`, `tunnel.go`)**: Handles CLI argument parsing, concurrent web scraping, proxy validation, and the high-performance SOCKS5 proxy server. Go excels at concurrent network operations and I/O management.
2. **The Rust Engine (`rotator_rs` under `src/`)**: Handles memory-safe core logic, complex topology mathematics (scoring and generating proxy chains), and AES-256-GCM cryptographic primitives. Rust is chosen for its performance and memory safety in system-level processing.

The Go orchestrator uses **CGO (C bindings)** to call directly into the statically linked Rust library (`librotator_rs.a`).

### Strict Architectural Mandate
- **Go (Networking & Orchestration)**: Handles 100% of network-facing operations.
- **Rust (System Processing)**: Strictly isolated from the network. Handles encryption, scoring, and topology generation.

---

## 2. In-Depth File by File Breakdown

### The Go Front-End (Networking & Orchestration)

**`orchestrator.go`**
The **CLI Entrypoint and Lifecycle Manager**. 
- It parses CLI commands (`run`, `serve`, `refresh`, `rotate`, `stats`, `audit`).
- It manages file paths and state (saving `last_chain.json`, `proxies_combined.json`).
- It declares the `#cgo LDFLAGS` header that instructs the Go compiler to statically wrap the `librotator_rs.a` Rust archive directly into the `spectre` executable.

**`scraper.go`**
The **Concurrent Web Harvesting Module**. Spawns Goroutines to fetch proxies from 12+ sources concurrently using regex and HTML traversing.

**`verifier.go`**
The **Go-native Health Check System**. Performs live TCP reachability tests, measures latency, and updates proxy metrics (FailCount, LastVerified). Prunes dead proxies from the pool.

**`tunnel.go`**
The **Go-native SOCKS5 Server & Multi-hop Tunnel**.
- Implements the SOCKS5 interface for incoming client connections.
- Negotiates multi-hop proxy circuits (SOCKS5 or HTTP CONNECT) through chains of 1 to 5 proxies.
- Implements the `encryptedPipeGarlic` function, which pumps data with efficient multi-layered AES-256-GCM encryption.
- **Efficient Layered Encryption:** Uses a single FFI call to Rust to apply all encryption/decryption layers, minimizing CGO overhead.
- **Garlic Routing Features:** 
  - **Packet Padding:** Every packet is padded to a 512-byte multiple to hide traffic size.
  - **Chaffing:** Sends dummy packets during idle periods to obscure traffic timing.
  - **Dual-Path Routing:** Supports separate outbound and inbound circuits when the `--garlic` flag is active.
- Features automatic proxy rotation on connection failure.

**`security-audit/`**
An isolated Go application strictly built for adversarial leak testing.

---

### The Rust Engine (System Processing)

**`Cargo.toml`**
Defines dependencies (like `aes-gcm`, `serde`) and configures the static library output. Crucially, it contains **zero networking dependencies** (no `tokio::net` or `std::net`).

**`src/lib.rs`**
The **FFI Bridge**.Gateway between Go and Rust. Responsible for safely parsing data received from Go and exporting C-compatible functions for polishing, topology generation, and encryption.

**`src/crypto.rs`**
The cryptography module. Implements **AES-256-GCM** encryption primitives. Every packet is framed with a rotating counter to prevent replay attacks.

**`src/rotator.rs`**
The **Chain topology intelligence**. Randomly calculates multi-hop chain configurations and exit keys based on the selected mode.

**`src/polish.rs`**
The data scorer. Classifies proxies into tiers (Dead/Bronze/Silver/Gold/Platinum) based on metrics provided by the Go verifier.

---

## 3. Communication Workflow Summary (End-to-End)

When the user runs: `./spectre run --mode phantom`

1. **Go (orchestrator.go)** initiates `internalRunScraper`.
2. **Go (scraper.go)** concurrent workers fetch up to 10,000 proxies.
3. **Go (verifier.go)** performs TCP reachability and latency tests on all candidates.
4. **Go (orchestrator.go)** passes pre-validated proxy data to **Rust (run_polish_c)**.
5. **Rust (src/polish.rs)** scores and tiers the proxies.
6. **Rust (src/rotator.rs)** generates a 5-node circuit chain with random AES keys.
7. **Go** saves the chain topology and confirms success.

When the user then runs `./spectre serve --mode phantom`:

1. **Go (tunnel.go)** starts a SOCKS5 listener.
2. For each client, **Go** builds a multi-hop circuit through the chosen chain.
3. **Go** pumps data through an `encryptedPipe`, calling **Rust** FFI for per-packet AES-256-GCM encryption and decryption.
