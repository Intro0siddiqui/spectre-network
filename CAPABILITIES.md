# Spectre Network: Capabilities and Functionality

Spectre Network is an advanced, self-contained adversarial proxy mesh designed for maximum anonymity, security, and resilience. It operates by farming its own proxy pool, scoring them for quality, and assembling multi-hop AES-256-GCM encrypted relay chains.

---

## 1. Core Architecture & Design
*   **Hybrid Language Stack**: Combines **Go** for high-concurrency scraping, CLI orchestration, and containerized auditing with **Rust** for memory-safe core logic, complex topology mathematics, and high-performance async SOCKS5 serving via Tokio.
*   **Static Binary**: The system is compiled into a single, standalone binary where the Go frontend statically links against a Rust-compiled library (`librotator_rs.a`) via CGO/FFI.
*   **Stateless Security**: Encryption keys are derived at runtime and held strictly in memory. Only the network topology is persisted to disk, preventing retroactive decryption of traffic if the filesystem is compromised.

---

## 2. Proxy Harvesting & Intelligence
*   **Autonomous Scraping**: Concurrently harvests proxies from 9+ active sources, including APIs and GitHub repositories, without requiring external VPN or proxy subscriptions.
*   **Dynamic Tiering**: Automatically classifies proxies into five quality tiers based on a weighted scoring algorithm:
    *   **Platinum (≥ 0.85)**: Premium (<0.1s latency, elite anonymity).
    *   **Gold (0.70–0.85)**: Fast and reliable (0.1–0.5s latency).
    *   **Silver (0.50–0.70)**: Good quality (0.5–1s latency).
    *   **Bronze (0.30–0.50)**: Working but slow (1–3s latency).
    *   **Dead (< 0.30)**: Unusable (>3s latency, fails CONNECT).
*   **Intelligent Scoring**: Weighted scoring based on latency, anonymity level, geographic location (prefers specific countries), and protocol type.
*   **Deep Verification**: Performs protocol-aware SOCKS5/HTTP handshakes and routes test traffic to confirm a proxy actually routes data before it is used in a chain.

---

## 3. Advanced Tunneling & Encryption
*   **Multi-Hop Chaining**: Supports relay chains of 1 to 5 hops, providing deep traffic isolation and making traceback extremely difficult.
*   **AES-256-GCM Encryption**: Every connection in the chain is encrypted. Each hop has its own derived key and nonce.
*   **Nonce-Reuse Prevention**: Implements a per-packet nonce derivation system where a 64-bit packet counter is XORed into a base nonce, ensuring every packet uses a unique cryptographic nonce.
*   **DNS Leak Protection**: In `high` and `phantom` modes, DNS queries are routed through the proxy chain, preventing local DNS leaks to ISPs or local adversaries.
*   **Full-Duplex Encrypted Pipe**: High-throughput bidirectional piping of ciphertext using Tokio's async runtime.

---

## 4. Operation Modes
| Mode | Hops | Proxy Requirements | DNS Routing | Security Level |
|---|---|---|---|---|
| **Lite** | 1 | All proxies (Bronze+) | Local | Basic |
| **Stealth** | 1–2 | HTTP/HTTPS only | Local | Medium |
| **High** | 2–3 | SOCKS5/HTTPS preferred | Through Chain | High |
| **Phantom** | 3–5 | Gold+ tier SOCKS5/HTTPS | Through Chain | Maximum |

---

## 5. Persistence & Self-Healing
*   **Pool Persistence**: Scraped proxy pools are saved to disk (`proxies_combined.json`, etc.) to avoid frequent re-scraping.
*   **Live Re-verification**: The `refresh` command performs concurrent health checks on stored pools to prune dead nodes and fill gaps.
*   **Continuous Rotation**: During a persistent `serve` session, the system automatically rotates the entire chain topology every 5 minutes to prevent exit-IP fingerprinting.

---

## 6. Adversarial Security Auditing
*   **Containerized Audit Suite**: Includes a dedicated Go-based audit application (`spectre-audit`) that performs a 9-test suite inside a Podman container.
*   **Tests Performed**:
    *   IP & IPv6 Leak detection.
    *   DNS Leak verification.
    *   Header Leak checks (`X-Forwarded-For`, `Via`, etc.).
    *   TLS Stripping and Protocol Downgrade detection.
    *   Latency budget and Timing Variance analysis.
*   **Scored Reporting**: Provides a clear grade (A+ through F) based on the security posture of the constructed chain.

---

## 7. Deployment Flexibility
*   **CLI Mastery**: Full command-line control for scraping, rotating, serving, and checking stats.
*   **Containerized Deployment**: Minimal production runtime images based on Ubuntu 24.04, running as a non-root user for enhanced security.
*   **Benchmarking Tools**: Built-in scripts to measure end-to-end performance across all modes.
