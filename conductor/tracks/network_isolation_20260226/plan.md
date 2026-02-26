# Implementation Plan: Isolate Networking Logic into Go Layer

## Goal
Enforce the architectural boundary between Go and Rust. Rust must be isolated from all network operations, and its responsibility must be limited to system-level processing.

## Phase 1: Migrate Verifier Logic to Go

- [x] **Task: Implement Go-based Verifier**
  - [ ] Write failing unit tests in Go for proxy reachability and validation.
  - [ ] Implement `internalVerifyProxy` in Go with TCP connection tests and timeouts.
  - [ ] Port the scoring weights and latency measurement logic from Rust's `verifier.rs` to Go.
  - [ ] Verify coverage (>80%) for the new Go verifier.

- [x] **Task: Refactor Orchestrator to use Go Verifier**
  - [ ] Modify `internalRunScraper` to invoke the Go verifier.
  - [ ] Ensure the Go verifier correctly populates the `Proxy` struct with latency, country, and status before FFI calls.
  - [ ] Commit changes with task summary.

- [x] **Task: Remove Rust Verifier Logic**
  - [ ] Delete `src/verifier.rs` and its module declaration in `src/lib.rs`.
  - [ ] Remove `run_verify_c` and other validation-related FFI exports.
  - [ ] Ensure Rust's `polish` logic now accepts pre-validated proxies.
  - [ ] Commit changes with task summary.

- [ ] **Task: Conductor - User Manual Verification 'Phase 1: Migrate Verifier Logic to Go' (Protocol in workflow.md)**

## Phase 2: Migrate Tunneling & SOCKS5 Server to Go

- [ ] **Task: Implement Go-based SOCKS5 Server**
  - [ ] Write failing unit tests in Go for a SOCKS5 server.
  - [ ] Implement the core SOCKS5 listener and handshake logic in Go.
  - [ ] Verify coverage (>80%) for the SOCKS5 implementation.

- [ ] **Task: Implement Go-based Multi-hop Tunneling**
  - [ ] Write failing unit tests for a multi-hop proxy circuit (Go layer).
  - [ ] Implement the logic to build a circuit through a chain of 1 to 5 proxies (SOCKS5/HTTP/HTTPS).
  - [ ] Integrate Rust's AES-256-GCM encryption primitives into the Go tunnel pipe.
  - [ ] Verify coverage (>80%) for the tunneling logic.

- [ ] **Task: Refactor Orchestrator to use Go Tunnel**
  - [ ] Replace `start_spectre_server_c` with the new Go-native server implementation.
  - [ ] Ensure the Go tunnel correctly routes DNS through the chain when required by the mode.
  - [ ] Commit changes with task summary.

- [ ] **Task: Remove Rust Tunnel Logic**
  - [ ] Delete `src/tunnel.rs` and its module declaration in `src/lib.rs`.
  - [ ] Remove `start_spectre_server_c` and other network-related FFI exports.
  - [ ] Ensure Rust's `lib.rs` and `src/` directory are free from `tokio::net` and `std::net` imports.
  - [ ] Commit changes with task summary.

- [ ] **Task: Conductor - User Manual Verification 'Phase 2: Migrate Tunneling & SOCKS5 Server to Go' (Protocol in workflow.md)**

## Phase 3: Rust Engine Refactoring & Clean-up

- [ ] **Task: Strip Networking Dependencies from Rust**
  - [ ] Update `Cargo.toml` to remove `tokio` (with network features) and other network-specific crates.
  - [ ] Refactor `src/rotator.rs` to ensure it only performs scoring and topology generation based on provided metrics.
  - [ ] Commit changes with task summary.

- [ ] **Task: Final Integration & Audit**
  - [ ] Execute `spectre audit` to confirm anonymity and leak protection.
  - [ ] Run `benchmark.sh` to compare performance with the previous implementation.
  - [ ] Finalize documentation in `README.md` and `architecture.md` to reflect the new architecture.
  - [ ] Commit changes with task summary.

- [ ] **Task: Conductor - User Manual Verification 'Phase 3: Rust Engine Refactoring & Clean-up' (Protocol in workflow.md)**
