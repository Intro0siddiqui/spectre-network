# Spectre Network

> **Non-Technical Summary:** Spectre Network is your own personal, high-speed privacy tunnel. It doesn't rely on any big VPN companies. Instead, it finds free public proxies across the internet, tests them to find the fastest ones, and strings them together into a "chain." Your internet traffic is then scrambled and sent through this chain, making it very difficult for anyone—including your ISP or the websites you visit—to see who you are or what you're doing. It's built to be tough, hiding its own tracks by making its traffic look like normal web browsing or video streaming.

---

A self-contained, adversarial proxy mesh. Farms its own proxy pool, scores and filters by tier (Dead/Bronze/Silver/Gold/Platinum), then builds multi-hop AES-256-GCM encrypted relay chains — no third-party VPN subscription required.

---

## Architecture

| Layer | Language | Role |
|---|---|---|
| Orchestrator | Go | CLI orchestration, state management, file I/O |
| Networking | Go | 100% of network I/O: Scraper, Verifier, SOCKS5 Server |
| VPN Manager | Go | User-space WireGuard tunnel (rootless netstack) |
| Engine | Rust | System processing: Scored Tiering, Topology calculations, AES-256-GCM |
| Audit | Go | Containerised adversarial leak testing (9-test suite) |

### The Go-Native Boundary
Spectre Network enforces a strict domain isolation model. **Go** manages the entire lifecycle of network packets, proxy validation, and SOCKS5 handshakes. **Rust** is strictly isolated from the network, providing high-performance cryptographic and mathematical primitives via CGO/FFI.

---

## Build

### Quick Build (Recommended)

```bash
# Build and install globally (~/.local/bin/spectre)
./build.sh install

# Or build only (binary in current directory)
./build.sh
```

### Manual Build

```bash
# 1. Build the Rust engine (creates static library librotator_rs.a)
cargo build --release

# 2. Build the spectre binary (Go + Rust FFI, fully static)
CGO_ENABLED=1 go build -ldflags="-s -w -extldflags '-static'" -o spectre .
```

> **Note:** The `spectre` binary produced by the Go build is the primary entry-point.
> A standalone Rust binary exists at `target/release/spectre` but is not required for normal usage.

---

## Usage

```
spectre <command> [flags]
```

| Command | What it does |
|---|---|
| `spectre run` | Scrape fresh proxies → polish → build chain |
| `spectre refresh` | Re-verify stored pool → fill gaps → build chain |
| `spectre rotate` | Build new chain from stored pool (no scrape, instant) |
| `spectre serve` | Build chain then start a live SOCKS5 proxy server |
| `spectre stats` | Show pool health without building a chain |
| `spectre audit` | Run containerised adversarial security test (needs Podman) |

### Flags

| Flag | Values | Default |
|---|---|---|
| `--mode` | `phantom` \| `high` \| `stealth` \| `lite` | `phantom` |
| `--limit` | integer (1–10 000) | `500` |
| `--protocol` | `all` \| `socks5` \| `https` \| `http` | `all` |
| `--port` | integer | `1080` |
| `--garlic` | boolean | `false` |
| `--obfuscation-mode` | `off` \| `simple` \| `advanced` \| `obfs4` | `off` |
| `--jitter-range` | integer (ms) | `3000` |
| --padding-range | `MIN-MAX` (bytes) | `512-1024` |
| `--mimic-protocol` | `https` \| `quic` \| `ssh` | `off` |
| `--mimic-fingerprint` | `chrome` \| `firefox` \| `edge` \| `youtube` | `chrome` |
| `--vpn-config` | path to `.conf` | `""` |
| `--vpn-position` | `entry` \| `intermediate` \| `exit` \| `any` | `any` |

### Examples

```bash
# First run — scrape fresh and build a phantom chain
spectre run --mode phantom --limit 1000

# Advanced obfuscation — randomized padding and jittered chaffing
spectre run --mode high --garlic --obfuscation-mode advanced --padding-range 256-512

# obfs4 Pluggable Transport (Protocol Morphing)
spectre run --mode lite --obfuscation-mode obfs4 --node-id <ID> --public-key <KEY>

# Protocol Mimicry — Disguise traffic as a Chrome TLS 1.3 handshake or QUIC stream
spectre run --mode phantom --mimic-protocol https --mimic-fingerprint chrome
spectre run --mode high --mimic-protocol quic

# Nexus Phase 4: WireGuard VPN Integration — Blend a native VPN into the mesh
spectre run --mode high --vpn-config /path/to/wg0.conf --vpn-position entry
```

---

## Modes

| Mode | Hops | Proxy Requirements | DNS | Encryption | Anonymity |
|---|---|---|---|---|---|
| `lite` | 1 | All proxies (Bronze+) | Local | AES-256-GCM | Low |
| `stealth` | 1–2 | HTTP/HTTPS only | Local | AES-256-GCM | Medium |
| `high` | 2–3 | SOCKS5/HTTPS preferred | Via chain | AES-256-GCM | High |
| `phantom` | 3–5 | Gold+ tier (score ≥ 0.7) SOCKS5/HTTPS | Via chain | AES-256-GCM | Maximum |

### Proxy Tier System

Proxies are automatically classified by quality during polish:

| Tier | Score Range | Description |
|---|---|---|
| **Platinum** | ≥ 0.85 | Premium (<0.1s latency, elite anonymity) |
| **Gold** | 0.70–0.85 | Fast and reliable (0.1–0.5s latency) |
| **Silver** | 0.50–0.70 | Good quality (0.5–1s latency) |
| **Bronze** | 0.30–0.50 | Working but slow (1–3s latency) |
| **Dead** | < 0.30 | Unusable (>3s latency, fails CONNECT) |

Tier assignment is automatic based on weighted scoring (latency, anonymity, country, protocol).

---

## What It Does

- ✅ Multi-hop SOCKS5 tunnel with AES-256-GCM encryption on every connection
- ✅ **Traffic shaping (Garlic Mode)** — randomized packet padding and jitter injection to defeat timing correlation.
- ✅ **Protocol morphing (obfs4)** — supports pluggable transports to bypass Deep Packet Inspection.
- ✅ **Protocol signature mimicry** — disguise handshakes as TLS 1.3 (Chrome/Firefox JA3) or QUIC streams to evade DPI.
- ✅ **Nexus Phase 4: WireGuard Integration** — supports blending professional/commercial VPNs as hops in the mesh via user-space WireGuard.
- ✅ DNS routed through chain in `high`/`phantom` modes (no local DNS leaks)
- ✅ Proxy pool persistence with live health re-verification
- ✅ Randomised chain assembly on every rotation — no fixed exit IP
- ✅ Weighted scoring (latency, anonymity, country, protocol)
- ✅ Containerised adversarial security audit (9-test suite)
- ✅ Encryption keys kept in memory only — only chain topology is saved to `last_chain.json`

## What It Doesn't Do Yet

- ❌ **P2P proxy discovery** — still uses a centralised scraper. *(Phase 2 roadmap)*
- ❌ **Browser Integration** — WebAssembly build for browser extensions. *(Phase 3 roadmap)*

### Is the missing traffic shaping a problem?

For **99% of use cases** (hiding from websites, ISPs, corporate surveillance, casual tracking) — no. The multi-hop chain with AES-GCM is more than sufficient.

For a **targeted nation-state adversary** with infrastructure access to your ISP and the exit proxy simultaneously — yes, timing correlation is possible without traffic shaping. This is the same limitation Tor partially has and is a known hard problem.

---

## Security Audit

```bash
# Auto-builds + runs the audit container, prints a scored report
spectre audit
```

Requires **Podman** (not Docker). The command auto-builds the `spectre-audit` probe binary if missing, then builds and runs the `Containerfile.audit` image.

Tests performed:

| Test | What it checks |
|---|---|
| IP Leak | Chain exit IP differs from host IP |
| DNS Leak | DNS resolves via chain, not local resolver |
| Header Leak | No `X-Forwarded-For`, `Via`, `X-Real-Ip`, `Forwarded` headers |
| Additional Headers | No `X-Client-IP`, `CF-Connecting-IP`, `True-Client-IP`, etc. |
| Proxy Reachable | SOCKS5 port is accepting connections |
| Latency Budget | End-to-end request completes within 6 s |
| IPv6 Leak | IPv6 address not exposed through chain |
| TLS Stripping | HTTPS connections are not downgraded |
| Timing Analysis | Timing variance is within acceptable bounds |

Grading: **A+** (9/9) → **A** (≥8/9) → **B** (≥7/9) → **C** (≥6/9) → **F** (<6/9).

---

## Proxy Sources

The Go scraper fans out concurrently across 9 active sources (using a 12-worker pool):

**Working Sources:**
- ProxyScrape API (HTTP)
- GeoNode API (HTTP + SOCKS5)
- TheSpeedX GitHub proxy lists (HTTP + SOCKS4 + SOCKS5)
- monosans GitHub proxy lists (HTTP + SOCKS5)
- vakhov/fresh-proxy-list (HTTP + SOCKS5)
- ProxySpace / ShiftyTR GitHub (HTTP + SOCKS5)
- FreeProxyList.net (HTML scrape via Colly)
- hookzof/socks5_list (SOCKS5, high quality)
- clarketm GitHub proxy list

**Removed (dead/unreliable):**
- Proxifly, Iplocate, Komutan234 (consistently returned 0 proxies)
- ProxyScrape SOCKS5 endpoint (depleted)

**Typical pool size:** 600–800 proxies per scrape at `--limit 200`

---

## Disk Layout

After a successful run the following files are written to the working directory:

| File | Contents |
|---|---|
| `raw_proxies.json` | Raw scraped proxies (pre-polish) |
| `proxies_combined.json` | All scored proxies |
| `proxies_dns.json` | SOCKS5 proxies suitable for DNS-through-chain |
| `proxies_non_dns.json` | HTTP/HTTPS proxies |
| `last_chain.json` | **Topology only** — chain IPs/ports, no encryption keys |

> **Security note:** Encryption keys (AES-256-GCM key + nonce per hop) are derived at chain-build time and held in process memory only. They are never written to disk. `last_chain.json` contains only the network topology, so anyone with filesystem access cannot retroactively decrypt captured traffic.

---

## Container Deployment

### Runtime image (`Containerfile`)

Build the binaries and pool on the host first, then:

```bash
# Pre-populate pool
./spectre run --mode phantom --limit 500

# Build and run the container
podman build -t spectre-preloaded -f Containerfile .
podman run -d --name spectre-node -p 1080:1080 spectre-preloaded
```

The container runs `spectre serve --mode phantom --port 1080` as its default command.
Base image: **ubuntu:24.04**. Runs as non-root user `spectre` (UID 2000).

---

## Roadmap

See [ROADMAP.md](ROADMAP.md) for planned phases: traffic obfuscation, P2P discovery, browser integration.
