# Spectre Network: Technology Stack

## Core Development
- **Networking & Orchestration**: **Go** (Standard Library, `colly` for scraping, `obfs4` for obfuscation, `utls` for protocol mimicry, `wireguard-go` & `netstack` for user-space VPN integration, `yaml.v3` for config). Responsible for 100% of network I/O, concurrent scraping, and the SOCKS5 server/tunneling layer.
- **System Processing & Cryptography**: **Rust** (Standard Library, `aes-gcm` for encryption, `serde` for JSON data exchange). Responsible for high-performance scoring, tiering, and topology calculations.
- **Bridge Logic**: **CGO (Foreign Function Interface)**. Rust is compiled as a static library (`librotator_rs.a`) and linked into the Go binary (`spectre`) via C pointers.

## Runtime & Deployment
- **Containerization**: **Podman** (Rootless, non-privileged). Used for both adversarial security audits (`spectre audit`) and the production runtime environment (`Containerfile`).
- **Target Platform**: **Linux (Ubuntu 24.04-based images)**.
- **Build System**: **Cargo** (Rust), **Go Compiler** (Go), and **Bash** (automation scripts).
