# Implementation Plan: Phase 4: Nexus - WireGuard VPN Integration

**Phase 1: WireGuard Core & Configuration (Go)** [checkpoint: 4a71ac5]
- [x] **Task: Research and Integrate `wireguard-go` & `netstack`** (e7e106d)
    - [x] Add `golang.zx2c4.com/wireguard` and `golang.zx2c4.com/wireguard/tun/netstack` to `go.mod` (Go)
    - [x] Create `vpn_manager.go` to handle the user-space WireGuard interface (Go)
- [x] **Task: Implement Config Parser** (8068fd7)
    - [x] Add logic to parse standard WireGuard `.conf` files (PrivateKey, Endpoint, etc.) (Go)
    - [x] Extend `orchestrator.go` with `--vpn-config` and `--vpn-position` flags (Go)
- [x] **Task: Conductor - User Manual Verification 'Phase 1: WireGuard Core & Configuration' (Protocol in workflow.md)**

**Phase 2: Handshake & Tunneling (Go)**
- [x] **Task: Implement User-space Handshake** (9a334fc)
    - [x] **Write Tests (Red Phase):** Create unit tests in `nexus_test.go` to verify WireGuard configuration parsing and key handling (Go)
    - [x] **Implement (Green Phase):** Integrate the `wireguard-go` client to establish a tunnel to a specified endpoint (Go)
    - [ ] **Refactor:** Ensure the VPN connection is managed as a reusable `net.Conn` or `net.Dialer` (Go)
- [ ] **Task: Position-Aware Circuit Integration**
    - [ ] **Write Tests (Red Phase):** Create tests verifying that a circuit can be established through a VPN dialer (Go)
    - [ ] **Implement (Green Phase):** Modify `buildCircuitInternal` to use the VPN dialer when the current hop matches the VPN position (Go)
- [ ] **Task: Conductor - User Manual Verification 'Phase 2: Handshake & Tunneling' (Protocol in workflow.md)**

**Phase 3: Robustness & Final Integration (Go/Rust)**
- [ ] **Task: Failover & Health Monitoring**
    - [ ] **Write Tests (Red Phase):** Create tests for fallback behavior when the VPN endpoint is unreachable (Go)
    - [ ] **Implement (Green Phase):** Add logic to detect VPN tunnel failure and either attempt reconnect or fall back to standard proxies (Go)
- [ ] **Task: Documentation & Final Polish**
    - [ ] Update `README.md` and `ROADMAP.md` with Nexus Phase 4 details (Docs)
    - [ ] Final security review of the VPN credential handling (Audit)
- [ ] **Task: Conductor - User Manual Verification 'Phase 3: Integration & Final Polish' (Protocol in workflow.md)**
