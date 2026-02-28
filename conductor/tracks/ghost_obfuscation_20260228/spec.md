# Specification: Phase 1: Ghost - Traffic Obfuscation

**Overview:**
Implement advanced traffic obfuscation to bypass Deep Packet Inspection (DPI) and break temporal fingerprinting. This track will refine existing chaffing/padding and introduce "Protocol Morphing" using the `obfs4` pluggable transport.

**Functional Requirements:**
1.  **Chaffing & Padding (Refinement):**
    -   Enhance the existing `encryptedPipeGarlic` function in `tunnel.go`.
    -   Implement **Randomized Padding**, where packets are padded to random sizes within a range (e.g., 256 to 1024 bytes) to avoid fixed-size signatures.
    -   Inject dummy "chaff" packets at randomized intervals to normalize traffic rates and volumes.
2.  **Protocol Morphing (`obfs4`):**
    -   Integrate `obfs4proxy` or a compatible `obfs4` library into the Go orchestrator.
    -   Modify `handshakeProxy` in `tunnel.go` to wrap initial SOCKS5/HTTP handshakes in an `obfs4` layer.
    -   Ensure `obfs4` metadata (e.g., node IDs, public keys) can be passed through the `RotationDecision`.
3.  **User Configuration:**
    -   Add new CLI flags (e.g., `--obfuscation-mode`, `--jitter-range`) to `orchestrator.go`.
    -   Support an optional `obfuscation.yaml` for advanced tuning of padding ranges and chaffing rates.

**Non-Functional Requirements:**
-   **Performance:** Padding and chaffing must not increase latency by more than 20% on a typical multi-hop circuit.
-   **Anonymity:** Obfuscated traffic must be indistinguishable from normal HTTPS/QUIC traffic under basic DPI.
-   **Stability:** The `obfs4` layer must gracefully handle connection resets and retries.

**Acceptance Criteria:**
-   [ ] `encryptedPipeGarlic` correctly applies randomized padding and injects chaff packets.
-   [ ] A circuit can be successfully established using `obfs4` protocol morphing.
-   [ ] CLI flags and `obfuscation.yaml` correctly override default obfuscation parameters.
-   [ ] Unit tests verify that padding and chaffing do not leak original packet sizes or timing.

**Out of Scope:**
-   `meek` (domain fronting) or custom QUIC-based morphing.
-   Integration with browser extensions (Hydra phase).
