# Spectre Network Roadmap

> **Non-Technical Summary:** This roadmap shows the journey of Spectre Network from a powerful privacy tool to an unblockable, community-driven network. We've already completed the first major phase: "Ghost," which makes your traffic nearly impossible to distinguish from regular internet use. Next, we're working on "Hive" to make the network even more resilient by letting nodes help each other find proxies without a central server. Future steps include "Hydra" for maximum invisibility and "Nexus" to let you blend your own professional VPNs into the Spectre mesh.

---

> Spectre is fully operational for everyday use. The phases below are **extreme-measure upgrades** for adversarial environments — censorship regimes, targeted surveillance, or nation-state-level threats. They are not needed for normal usage.

---

## Phase 1: Ghost — Traffic Obfuscation
**When you need it:** You are in a country with Deep Packet Inspection (China, Iran, Russia) that blocks non-HTTPS traffic, OR you believe a targeted surveillance operation is timing your connections.

- [x] **Traffic Shaping (Chaffing)**
    - Inject dummy packets to normalise traffic rates and volume
    - *Why:* Defeats timing correlation where an adversary matches your outbound bursts with the exit proxy's bursts
- [x] **Packet Padding**
    - Pad all frames to fixed sizes (e.g. 512 bytes)
    - Randomised micro-delays (jitter) to break temporal fingerprinting
- [x] **Protocol Morphing (Pluggable Transports)**
    - Wrap SOCKS5 traffic in HTTPS or QUIC so it looks like a YouTube stream
    - Support `obfs4` or `meek` (domain fronting) to bypass DPI blocklists
- [x] **Protocol Signature Mimicry (Signatures)**
    - Mimic specific browser JA3/JA4 TLS fingerprints and ALPN strings using `utls`
    - Disguise handshakes as standard HTTPS or QUIC streams to evade DPI signatures

---

## Phase 2: Hive — Decentralisation
**When you need it:** The proxy scraper sources get blocked or poisoned, or you need the network to self-heal without any central dependency.

- [ ] **Gossip / DHT Proxy Discovery**
    - Replace the centralised scraper with a libp2p DHT — nodes share healthy proxy candidates with neighbours
    - Network heals itself if a source dies
- [ ] **Reputation System**
    - Nodes score proxies locally based on uptime and honesty (EigenTrust or similar)
    - Prevents malicious nodes from poisoning the pool with honeypot proxies

---

## Phase 3: Hydra — Unstoppability
**When you need it:** A well-funded adversary is actively trying to block or de-anonymise all Spectre traffic specifically.

- [ ] **Poly-Hop Chameleon Chaining**
    - Rotate protocols per hop: `User → SSH → SOCKS5 → HTTPS → Target`
    - Each hop looks like a different type of traffic
- [ ] **Browser Integration**
    - WebAssembly build to run directly inside a browser extension
    - One-click activation for non-technical users

---

## Phase 4: Nexus — Commercial & Native Integration
**When you need it:** You want to combine the speed and reliability of commercial VPN providers with the anonymity of an adversarial multi-hop circuit.

- [~] **Direct VPN Protocol Support (WireGuard/OpenVPN)**
    - Implement native client logic for WireGuard and OpenVPN to use existing commercial VPN accounts as internal mesh hops.
    - *Why:* Allows the inclusion of high-performance, trusted commercial servers alongside ephemeral public proxies.
