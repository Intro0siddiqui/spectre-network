# Spectre Network

A self-contained, adversarial proxy mesh. Farms its own proxy pool, scores and filters it, then builds multi-hop AES-256-GCM encrypted relay chains — no third-party VPN subscription required.

---

## Architecture

| Layer | Language | Role |
|---|---|---|
| Scraper + Orchestrator | Go | Single binary: fetches proxies from 12+ sources + CLI orchestration |
| Engine | Rust (`rotator_rs`) | Polishes, scores, and builds encrypted chains (called via FFI) |
| Tunnel | Rust (tokio) | SOCKS5 server with per-connection AES-256-GCM encryption |
| Audit | Go | Containerised adversarial leak testing (9-test suite) |

The `spectre` binary is a single standalone executable built from `orchestrator.go` and `scraper.go`, statically linked against the compiled Rust library (`librotator_rs.a`) via CGO/FFI. All core logic (polishing, chain building, serving) is handled by Rust, while Go handles scraping and CLI orchestration.

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

# 2. Build the spectre binary (Go orchestrator + scraper + Rust FFI, fully static)
CGO_ENABLED=1 go build -ldflags="-s -w -extldflags '-static'" -o spectre orchestrator.go scraper.go
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

### Examples

```bash
# First run — scrape fresh and build a phantom chain
spectre run --mode phantom --limit 1000

# Fast second run — re-verify pool, skip scraping if healthy
spectre refresh --mode phantom

# Instant rotation using whatever pool is on disk
spectre rotate --mode high

# Start a persistent SOCKS5 server on port 1080
spectre serve --mode phantom --port 1080

# Check pool stats
spectre stats

# Run the full security audit inside Podman
spectre audit
```

---

## Modes

| Mode | Hops | DNS | Encryption | Anonymity |
|---|---|---|---|---|
| `lite` | 1 | Local | AES-256-GCM | Low |
| `stealth` | 1–2 | Local | AES-256-GCM | Medium |
| `high` | 2–3 | Via chain | AES-256-GCM | High |
| `phantom` | 3–5 | Via chain | AES-256-GCM | Maximum |

---

## What It Does

- ✅ Multi-hop SOCKS5 tunnel with AES-256-GCM encryption on every connection
- ✅ DNS routed through chain in `high`/`phantom` modes (no local DNS leaks)
- ✅ Proxy pool persistence with live health re-verification
- ✅ Randomised chain assembly on every rotation — no fixed exit IP
- ✅ Weighted scoring (latency, anonymity, country, protocol)
- ✅ Containerised adversarial security audit (9-test suite)
- ✅ Encryption keys kept in memory only — only chain topology is saved to `last_chain.json`

## What It Doesn't Do Yet

- ❌ **Traffic shaping** — no packet padding or jitter injection. A global passive adversary can correlate traffic by timing. *(Phase 1 roadmap)*
- ❌ **Protocol morphing** — traffic looks like SOCKS5, not HTTPS/QUIC. Deep Packet Inspection can detect it. *(Phase 1 roadmap)*
- ❌ **P2P proxy discovery** — still uses a centralised scraper. *(Phase 2 roadmap)*

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

The Go scraper fans out concurrently across up to 12 sources:

- ProxyScrape API (HTTP + SOCKS5)
- TheSpeedX GitHub proxy lists
- monosans GitHub proxy lists
- vakhov/fresh-proxy-list
- hookzof/socks5_list
- iplocate/free-proxy-list
- komutan234/Proxy-List-Free
- Proxifly free-proxy-list
- GeoNode API (HTTP + SOCKS5)
- FreeProxyList.net (HTML scrape via Colly)

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
