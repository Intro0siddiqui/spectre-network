# Spectre Network: Architectural Deep Dive

Spectre Network is an adversarial proxy mesh designed to farm its own proxy pool, score it, and assemble multi-hop AES-256-GCM encrypted relay chains. It operates entirely without relying on any third-party VPN subscriptions or centralized proxy networks, prioritizing security, anonymity, and deep traffic isolation.

This document serves as an exhaustive architectural blueprint detailing the cross-language design choices, the internal workflow, and an in-depth explanation of every file in the codebase.

---

## 1. High-Level Architecture Overview

The system is split into two co-dependent halves welded into a single binary (`spectre`):
1. **The Go Orchestrator (`orchestrator.go` & `scraper.go`)**: Handles CLI argument parsing, heavy concurrent web scraping, file I/O operations, and containerized auditing. Go excels at concurrent network fetching and building cross-platform CLI tool chains.
2. **The Rust Engine (`rotator_rs` under `src/`)**: Handles memory-safe core logic, complex topology mathematics (scoring and generating proxy chains), and the high-performance async SOCKS5 proxy server. Rust is chosen here to prevent memory leaks and handle high-throughput encrypted packet piping with Tokio.

The Go orchestrator uses **CGO (C bindings)** to call directly into the statically linked Rust library (`librotator_rs.a`). This allows Go to pass raw JSON proxy lists to Rust via C-strings, where Rust unmarshals them, processes them, and returns JSON decisions back to Go seamlessly.

---

## 2. In-Depth File by File Breakdown

### The Rust Engine (Proxy & Tunnel Logic)

**`Cargo.toml`**
The Rust project manager. It defines the dependencies (like `tokio`, `aes-gcm`, `pyo3`, and `serde`) and configures the build output. Crucially, we configure `crate-type = ["staticlib", "cdylib", "rlib"]` so Rust outputs a standalone archive file (`librotator_rs.a`) that Go statically links.

**`src/lib.rs`**
The **Foreign Function Interface (FFI) Bridge**. This file is the gateway between Go and Rust. It marks functions with `#[no_mangle] pub extern "C"` to export them to C. It is responsible for safely parsing raw JSON pointers received from Go, passing them to internal Rust modules (`polish`, `verifier`, `rotator`, `tunnel`), trapping Rust thread panics so they don't crash the Go runtime, and memory-managing the C-strings returning to Go.

**`src/crypto.rs`**
The cryptography module. It implements the **AES-256-GCM** encryption primitives used across the multi-hop SOCKS5 pipe. It handles deriving safe nonces and keys per hop. Every packet is framed with a rotating counter to prevent replay attacks and cryptographic nonce reuse.

**`src/rotator.rs`**
The **Chain topology intelligence**. This is the brain that determines how traffic will flow. Based on the selected mode (`lite`, `stealth`, `high`, `phantom`), it randomly calculates how many proxy hops are necessary. It penalizes/rewards proxy chains based on geographic diversity, latency, and proxy response reliability.

**`src/polish.rs`**
The initial data cleaner. It takes thousands of raw, unverified proxies from the scraper and filters out duplicates, invalid IP forms, and drops proxies with zero anonymity guarantees. 

**`src/verifier.rs`**
The secondary health check system. It spawns hundreds of internal async tasks to perform live ping and TCP handshake checks against the proxy pools concurrently to ensure that stored topologies have not gone offline or degraded before traffic is routed.

**`src/tunnel.rs`**
The **highest-throughput and most complex file** in the project. Powered by Tokio, it implements:
1. The SOCKS5 interface for incoming client connections.
2. The logic to sequentially negotiate SOCKS5 or HTTP CONNECT tunnels sequentially through a chain of 1 to 5 proxies (building the circuit).
3. The `encrypted_pipe` function, which acts as a bidirectional, full-duplex pump feeding ciphertext back and forth through the constructed chain with strict timeouts to prevent hangs.

---

### The Go Front-End (Scraping, Orchestration, Auditing)

**`orchestrator.go`**
The **CLI Entrypoint and Lifecycle Manager**. 
- It parses CLI commands (`run`, `serve`, `refresh`, `rotate`, `stats`, `audit`).
- It manages file paths and state (saving `last_chain.json`, `proxies_combined.json`).
- It declares the `#cgo LDFLAGS` header that instructs the Go compiler to statically wrap the `librotator_rs.a` Rust archive directly into the `spectre` executable.
- It dynamically orchestrates the workflow: invoking the scraper, passing the proxies via CGO to the Rust engine for polishing, generating the proxy chain, and eventually commanding Rust to spin up the SOCKS5 server listener.

**`scraper.go`**
The **Concurrent Web Harvesting Module**. Originally designed as a standalone binary to prevent web parsing crashes from bringing down the main orchestrator, it has been cleanly merged for ease-of-use. It spawns hundreds of lightweight Goroutines that perform concurrent scraping from 12+ APIs and GitHub repositories using regex pattern matching and HTML traversing (via `colly`). It features aggressive timeouts and validates the TCP reachability of proxies before pushing them to the orchestrator.

**`security-audit/ (Directory)`**
An isolated Go application strictly built for adversarial leak testing. During a `spectre audit`, this tool interacts strictly as a client and targets the SOCKS5 proxy to measure its security:
- Probing if the exit IP corresponds to the host IP.
- Checking for TLS downgrades or protocol stripping.
- Firing test packets to expose un-routed IPv6 packets.
- Testing proxy latency budgets.

---

### Containerization & Build Files

**`Containerfile`**
A Podman deployment script for production runtime. It constructs a secure, minimal Ubuntu container specifically to run `spectre serve`. It enforces non-root usage (UID 2000), copies pre-scraped local JSON pools, and runs the SOCKS5 proxy entirely isolated from host networking.

**`Containerfile.audit`**
Similar to the main containerfile but designed strictly for running the `. /spectre audit` sequence. It brings up a background `spectre serve` task *inside* the container, actively waits for it to become responsive, and then blasts the proxy with the `security-audit` tools securely inside an isolated Podman network namespace.

**`benchmark.sh`**
A bash utility for developers. It automates end-to-end load tests across every Spectre Network mode (`lite` through `phantom`), scraping proxies, starting servers, sending test cURL requests with strict timeout monitoring, and outputting latency metric reports.

---

## 3. Communication Workflow Summary (End-to-End)

When the user runs: `./spectre run --mode phantom`

1. **Go (orchestrator.go)** initiates `internalRunScraper`.
2. **Go (scraper.go)** concurrent workers fetch up to 10,000 proxies and perform basic TCP connectivity tests. Returning valid IP:Port combinations.
3. **Go (orchestrator.go)** converts the collected pool into a JSON string and passes a C-string pointer `run_polish_c()` across the FFI boundary.
4. **Rust (src/lib.rs)** receives the pointer, parses JSON, and passes it to `polish::polish_pool`.
5. **Rust (src/rotator.rs)** generates 5 random AES exit keys and creates an intelligent 5-node circuit chain.
6. **Rust** converts the complex Rust struct decision back into a JSON C-string pointer and hands it back to Go.
7. **Go** saves the chain topology to disk (without writing encryption keys) and prints the chain success.

When the user then runs `./spectre serve --mode phantom`, Go hands that decision mapping directly back to `src/tunnel.rs` via FFI, spinning up Tokio threads that perform SOCKS5 handshakes down the chain topology, maintaining the tunnel securely.
