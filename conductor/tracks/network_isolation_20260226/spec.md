# Track Specification: Isolate Networking Logic into Go Layer

## Goal
The primary objective of this track is to strictly enforce the architectural boundary between Go and Rust. Rust must be isolated from all network operations, and its responsibility must be limited to system-level processing (cryptography, scoring, topology).

## Background
Currently, the Rust engine (`rotator_rs`) contains networking logic in `verifier.rs` and `tunnel.rs`. These modules perform live pings and manage SOCKS5/TCP connections. To improve maintainability and follow the project's core mandate, these responsibilities must migrate to the Go orchestration layer.

## Requirements

### Go Layer (Networking & Orchestration)
- Implement all proxy validation/verifier logic in Go.
- Implement a high-performance SOCKS5 server and multi-hop tunneling logic in Go.
- Manage all TCP/UDP connections and timeouts.
- Provide a clean FFI interface to pass validated network data into Rust.

### Rust Engine (System Processing)
- Refine the `lib.rs` FFI boundary to receive raw data from Go.
- Focus strictly on AES-256-GCM encryption/decryption.
- Implement scoring and tiering logic without direct network interaction.
- Generate chain topologies based on Go-provided validation metrics.
- Remove `tokio::net` and `std::net` dependencies from the Rust project.

## Acceptance Criteria
- [ ] All proxy validation is performed in Go before scoring.
- [ ] The SOCKS5 server and multi-hop tunnel operate entirely within the Go runtime.
- [ ] Rust project contains zero networking imports (e.g., `tokio::net`, `std::net`).
- [ ] `spectre audit` confirms that the new Go-based tunnel provides the same (or better) anonymity and security as the previous implementation.
- [ ] System remains statically linked and builds with a single binary.
