# Specification: Phase 4: Nexus - WireGuard VPN Integration

**Overview:**
Integrate WireGuard into the Spectre Network as a specialized "Trusted Hop." This allows users to mix high-performance commercial VPN connections with ephemeral public proxies in their multi-hop circuits.

**Functional Requirements:**
1.  **WireGuard User-space Client (Go):**
    -   Integrate `wireguard-go` and `netstack` to handle WireGuard connections without requiring root privileges or system-level network interface changes.
    -   Support standard WireGuard configuration parameters: `PrivateKey`, `PublicKey`, `Endpoint`, `AllowedIPs`, and `PresharedKey`.
2.  **Flexible Circuit Integration:**
    -   Allow users to specify the position of the VPN hop within the circuit (e.g., `First`, `Intermediate`, `Exit`) via a new `vpn_config.toml` or by flagging entries in `premium_proxies.json`.
    -   Spectre should be able to tunnel SOCKS5/HTTPS proxy traffic *through* the WireGuard tunnel or vice versa.
3.  **Dynamic Failover:**
    -   If the WireGuard endpoint is unreachable, Spectre should gracefully fall back to a pure public-proxy circuit (if configured by the user).
4.  **CLI & Configuration:**
    -   Add `--vpn-config` flag to point to a WireGuard `.conf` or `.toml` file.
    -   Add `--vpn-position [entry|intermediate|exit|any]` to control circuit placement.

**Non-Functional Requirements:**
-   **Security:** Ensure WireGuard private keys are handled securely in memory and never logged.
-   **Performance:** The user-space WireGuard implementation should maintain at least 80% of the raw endpoint throughput.
-   **Portability:** The implementation must work across Linux without external kernel dependencies.

**Acceptance Criteria:**
-   [ ] A multi-hop circuit can be established where the first hop is a WireGuard tunnel.
-   [ ] A multi-hop circuit can be established where the exit hop is a WireGuard tunnel.
-   [ ] Spectre can successfully parse and connect using a standard `.conf` WireGuard file.
-   [ ] Unit tests in `nexus_test.go` verify the user-space handshake and data encapsulation.

**Out of Scope:**
-   OpenVPN integration (deferred to a follow-up Nexus sub-track).
-   Automated scraping of public WireGuard endpoints (manual config only).
