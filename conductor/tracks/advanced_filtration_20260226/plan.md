# Implementation Plan: Advanced Filtration & Layered Anonymity

## Goal
Optimize proxy validation speed, implement infrastructure diversity, and refine layered encryption to support high-quality third-party proxy sources.

## Phase 1: Go-Layer Validation & Source Enhancements [checkpoint: 3cfdf36]

- [x] **Task: Implement Validation Worker Pool** [a59828d]
  - [x] Create `internal/pool` package for generic worker management.
  - [x] Write failing tests for worker pool concurrency and error propagation.
  - [x] Refactor `verifier.go` to use the worker pool instead of direct goroutine spawning.
  - [x] Verify coverage (>80%) for the new pool logic.

- [x] **Task: Support Hybrid Proxy Sources** [f368e93]
  - [x] Update `Proxy` struct in `orchestrator.go` to include a `SourceType` (Standard/Premium).
  - [x] Implement a command to manually add "Premium" proxy endpoints (e.g., from Nord free tiers).
  - [x] Write tests ensuring Premium proxies are prioritized during chain selection.

- [x] **Task: Conductor - User Manual Verification 'Phase 1: Go-Layer Validation & Source Enhancements' (Protocol in workflow.md)**

## Phase 2: Rust-Layer Diversity & Advanced Scoring

- [x] **Task: Implement CIDR Diversity** [e703840]
  - [x] Update `src/rotator.rs` to include a subnet check during chain assembly.
  - [x] Write failing tests in Rust ensuring proxies in the same `/24` are rejected for the same chain.
  - [x] Implement `is_same_subnet` helper logic in Rust.

- [x] **Task: ASN Blacklisting & Dynamic Weights** [e703840]
  - [x] Add `asn_filter` module to `src/polish.rs`.
  - [x] Update FFI boundary in `src/lib.rs` to accept custom weight overrides from Go.
  - [x] Implement scoring penalty for known datacenter ASN ranges.
  - [x] Verify Rust tests pass for all new filtration logic.

- [ ] **Task: Conductor - User Manual Verification 'Phase 2: Rust-Layer Diversity & Advanced Scoring' (Protocol in workflow.md)**

## Phase 3: Encryption Layering & Final Integration

- [ ] **Task: Refine Multi-Layered Encryption**
  - [ ] Review `src/crypto.rs` to ensure strict encapsulation of encrypted payloads.
  - [ ] Write a Go integration test that simulates a 3-hop chain and verifies payload integrity.
  - [ ] Ensure `tunnel.go` correctly applies the layered keys derived by Rust.

- [ ] **Task: Experimental Garlic-Hybrid Implementation**
  - [ ] Add `--garlic` flag support to `orchestrator.go` CLI.
  - [ ] Update `tunnel.go` to support dual-path (split outbound/inbound) routing when the `--garlic` flag is active.
  - [ ] Implement basic packet padding/chaffing in `encryptedPipe`.

- [ ] **Task: Final Integration & Performance Audit**
  - [ ] Run `spectre audit` to confirm anonymity grades remain high.
  - [ ] Run `benchmark.sh` to ensure the worker pool improves scraping speed for large pools.
  - [ ] Update `architecture.md` with details on the new filtration and diversity logic.

- [ ] **Task: Conductor - User Manual Verification 'Phase 3: Encryption Layering & Final Integration' (Protocol in workflow.md)**
