# Track Specification: Advanced Filtration, Diversity & Layered Anonymity

## Overview
This track optimizes proxy filtration while introducing layered anonymity concepts. It aims to integrate high-quality third-party proxy sources and ensure multi-layered encryption across the mesh.

## Functional Requirements

### Go Layer (Networking)
- **Implement a Dedicated Worker Pool:** Manage high-concurrency validation tasks for 10k+ proxies.
- **Hybrid Source Support:** Add a new "Premium" source category to prioritize proxies manually added from high-quality free tiers (Nord, etc.).
- **Protocol Deep Probes:** Verify that these premium hops actually support encrypted payloads.

### Rust Layer (System)
- **Multi-Layered Encryption Refinement:** Ensure the "Onion" model is strictly followed where the user's data is recursively encrypted for each hop in the chain.
- **CIDR Diversity Logic:** Prevent multi-hop chains from using proxies in the same `/24` subnet to ensure infrastructure spread.
- **Dynamic Scoring & Blacklisting:** Implement ASN filtering and allow Go to pass priority weights for "Premium" vs "Free" proxies.

## Non-Functional Requirements
- **Deep Anonymity:** Ensure the "fake IP" (exit node) has zero correlation with the user's "main IP".
- **Resilience:** The system must gracefully fall back to the standard mesh if "Premium" hops are unavailable.

## Acceptance Criteria
- [ ] Chains can successfully mix "Premium" (manual/API) and "Standard" (scraped) proxies.
- [ ] Multi-hop chains never contain two proxies from the same `/24` subnet.
- [ ] Cryptographic material is derived uniquely per-hop and properly layered.
- [ ] Scraper handles blacklisted ASNs (datacenter ranges) to prioritize residential-like IPs.

## Out of Scope
- Automated account creation for third-party VPN free tiers.
- Implementing a custom VPN protocol (remains SOCKS5/HTTP based).
