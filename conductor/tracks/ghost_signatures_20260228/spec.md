# Specification: Ghost Phase 1 - Protocol Signature Mimicry (Signatures)

**Overview:**
Implement protocol signature mimicry to disguise Spectre Network traffic as standard HTTPS/QUIC or other common protocols. This ensures traffic bypasses Deep Packet Inspection (DPI) signatures that typically flag or block SOCKS5/VPN traffic.

**Functional Requirements:**
1.  **Protocol Mirroring (Signatures):**
    -   Implement **TLS 1.3 Handshake Mimicry** to make the initial connection appear as a standard HTTPS request.
    -   Provide support for **JA3/JA4 Fingerprint Resistance**, allowing the client to mimic common browser fingerprints (e.g., Chrome, Firefox).
    -   Support **QUIC (HTTP/3) Signature Wrapping** to emulate YouTube-like or video streaming traffic patterns.
2.  **Mimicry Implementation (Go):**
    -   Modify the `handshakeProxy` in `tunnel.go` to wrap the handshake in a pseudo-TLS or QUIC header.
    -   Utilize a Go library (e.g., `utls`) for advanced TLS fingerprinting to match standard browser signatures.
3.  **User Configuration:**
    -   Add CLI flags (e.g., `--mimic-protocol [https|quic|ssh]`) to `orchestrator.go`.
    -   Support `signatures.yaml` for advanced users to define custom JA3 fingerprints and ALPN strings.
    -   Default to "Standard HTTPS" mimicry if no protocol is specified.

**Non-Functional Requirements:**
-   **Anonymity:** Mimicked traffic must match the entropy and header patterns of the target protocol.
-   **Performance:** Mimicry logic should not introduce more than 50ms of additional latency to the initial handshake.
-   **Stealth:** The mimicry must be robust against active probing by DPI middleboxes.

**Acceptance Criteria:**
-   [ ] A circuit can be established that presents a valid-looking TLS 1.3 handshake to an external observer.
-   [ ] The `spectre` client successfully mimics a specific JA3 fingerprint as verified by a TLS fingerprinting service.
-   [ ] CLI flags and `signatures.yaml` correctly override default mimicry parameters.
-   [ ] Unit tests in `tunnel_test.go` verify that signatures are correctly applied before transmission.

**Out of Scope:**
-   Full implementation of `meek` (domain fronting).
-   Real-time payload shaping (packet-by-packet) beyond the handshake.
-   Rust-side encryption changes (encryption remains AES-256-GCM as per mandates).
