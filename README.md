# Spectre Network

A self-contained, adversarial proxy mesh. Farms its own proxy pool, scores and filters it, then builds multi-hop AES-256-GCM encrypted relay chains — no third-party VPN subscription required.

---

## Architecture

| Layer | Language | Role |
|---|---|---|
| Scraper | Go | Fetches proxies from 10+ sources concurrently |
| Engine | Rust | Polishes, scores, and builds encrypted chains |
| Orchestrator | Go + CGO | CLI that drives the full pipeline |
| Tunnel | Rust (tokio) | SOCKS5 server with per-chain AES-256-GCM encryption |
| Audit | Go | Containerised adversarial leak testing |

---

## Build

```bash
# 1. Build the Rust engine (shared library + binary)
PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1 cargo build --release

# 2. Build the Go orchestrator (links against the Rust lib via CGO)
CGO_ENABLED=1 go build -o spectre orchestrator.go

# 3. Build the Go scraper
go build -o go_scraper go_scraper.go
```

---

## Usage

```
spectre <command> [flags]
```

| Command | What it does |
|---|---|
| `spectre run` | Scrape fresh proxies → polish → build chain |
| `spectre refresh` | Re-verify stored pool → fill gaps → build chain |
| `spectre rotate` | Build new chain from stored pool (instant) |
| `spectre stats` | Show pool health without building a chain |
| `spectre audit` | Run containerised adversarial security test (needs Docker) |

### Flags

| Flag | Values | Default |
|---|---|---|
| `--mode` | `phantom` \| `high` \| `stealth` \| `lite` | `phantom` |
| `--limit` | integer | `500` |
| `--protocol` | `all` \| `socks5` \| `https` \| `http` | `all` |

### Examples

```bash
# First run — scrape fresh and build a phantom chain
spectre run --mode phantom --limit 1000

# Fast second run — re-verify pool, skip scraping if healthy
spectre refresh --mode phantom

# Instant rotation using whatever pool is on disk
spectre rotate --mode high

# Check pool stats
spectre stats

# Run the full security audit inside Docker
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
- ✅ Containerised adversarial security audit

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
# Builds + runs the audit container, prints a scored report
spectre audit
```

Tests performed: IP leak, DNS leak, header leak (X-Forwarded-For / Via), proxy reachability, latency budget.

---

## Roadmap

See [ROADMAP.md](ROADMAP.md) for planned phases: traffic obfuscation, P2P discovery, browser integration.
