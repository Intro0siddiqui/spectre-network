use rand::prelude::*;
use std::time::{SystemTime, UNIX_EPOCH};
use crate::types::{Proxy, ChainHop, CryptoHop, RotationDecision, ChainTopology};

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn normalize_proto(p: &str) -> String {
    match p.to_lowercase().as_str() {
        "http" => "http".into(),
        "https" => "https".into(),
        "socks" => "socks5".into(),
        "socks5" => "socks5".into(),
        "socks4" => "socks4".into(),
        other => other.to_string(),
    }
}

/// Derive a 32-byte AES-256 key from a master secret and chain-specific context.
/// Uses HKDF-SHA256 for secure key derivation.
///
/// `master_secret` — shared secret known only to authorized parties
/// `chain_id`      — unique identifier for this chain rotation
/// `hop_index`     — position in chain (0-based) for per-hop key separation
///
/// Returns a 64-character hex string representing the derived 32-byte key.
pub fn derive_key_from_secret(master_secret: &[u8], chain_id: &str, hop_index: usize) -> String {
    use hkdf::Hkdf;
    use sha2::Sha256;

    // Salt: use chain_id to ensure different chains get different keys even with same master secret
    let salt = chain_id.as_bytes();
    
    // Info: include hop index for per-hop key separation
    let info = format!("spectre-hop-{}", hop_index);
    
    // Derive 32 bytes (256 bits) for AES-256
    let hkdf = Hkdf::<Sha256>::new(Some(salt), master_secret);
    let mut okm = [0u8; 32];
    
    // Expand to get the output keying material
    hkdf.expand(info.as_bytes(), &mut okm)
        .expect("HKDF expand failed");
    
    hex::encode(okm)
}

/// Derive a 12-byte nonce from a master secret and chain-specific context.
/// Uses HKDF-SHA256 for secure nonce derivation.
///
/// `master_secret` — shared secret known only to authorized parties
/// `chain_id`      — unique identifier for this chain rotation
/// `hop_index`     — position in chain (0-based) for per-hop nonce separation
///
/// Returns a 24-character hex string representing the derived 12-byte nonce.
pub fn derive_nonce_from_secret(master_secret: &[u8], chain_id: &str, hop_index: usize) -> String {
    use hkdf::Hkdf;
    use sha2::Sha256;

    // Salt: use chain_id to ensure different chains get different nonces
    let salt = chain_id.as_bytes();
    
    // Info: include hop index for per-hop nonce separation
    let info = format!("spectre-nonce-{}", hop_index);
    
    // Derive 12 bytes for GCM nonce
    let hkdf = Hkdf::<Sha256>::new(Some(salt), master_secret);
    let mut okm = [0u8; 12];
    
    hkdf.expand(info.as_bytes(), &mut okm)
        .expect("HKDF expand failed");
    
    hex::encode(okm)
}

/// Generate encryption keys deterministically from a master secret.
/// This allows regenerating the same keys for a chain without storing them.
///
/// `master_secret` — shared secret (e.g., from environment variable or secure storage)
/// `chain_id`      — unique chain identifier
/// `num_hops`      — number of hops in the chain
///
/// Returns a vector of CryptoHop with derived keys and nonces.
pub fn generate_encryption_from_secret(master_secret: &[u8], chain_id: &str, num_hops: usize) -> Vec<CryptoHop> {
    (0..num_hops)
        .map(|i| {
            let key_hex = derive_key_from_secret(master_secret, chain_id, i);
            let nonce_hex = derive_nonce_from_secret(master_secret, chain_id, i);
            CryptoHop { key_hex, nonce_hex }
        })
        .collect()
}

/// Reconstruct a full RotationDecision from a ChainTopology and master secret.
/// This allows restoring encryption keys when needed without persisting them.
///
/// `topology`      — chain topology loaded from disk (no keys)
/// `master_secret` — shared secret for key derivation
///
/// Returns a RotationDecision with regenerated encryption keys.
pub fn reconstruct_decision_from_topology(topology: &ChainTopology, master_secret: &[u8]) -> RotationDecision {
    let encryption = generate_encryption_from_secret(master_secret, &topology.chain_id, topology.hops.len());
    
    // Reconstruct ChainHops from HopInfo (with placeholder values for non-topology fields)
    let chain = topology.hops.iter().map(|h| ChainHop {
        ip: h.ip.clone(),
        port: h.port,
        proto: h.proto.clone(),
        country: String::new(),      // Not stored in topology
        latency: 0.0,                // Not stored in topology
        score: 0.0,                  // Not stored in topology
    }).collect();
    
    RotationDecision {
        mode: topology.mode.clone(),
        timestamp: topology.created_at,
        chain_id: topology.chain_id.clone(),
        chain,
        avg_latency: topology.avg_latency,
        min_score: topology.min_score,
        max_score: topology.max_score,
        encryption,
    }
}

pub fn filter_mode_pool(mode: &str, dns: &[Proxy], non_dns: &[Proxy], combined: &[Proxy]) -> Vec<Proxy> {
    let mut pool = Vec::new();
    match mode {
        "lite" => {
            pool.extend_from_slice(combined);
            pool.extend_from_slice(non_dns);
            pool.extend_from_slice(dns);
        }
        "stealth" => {
            for p in combined.iter().chain(dns).chain(non_dns) {
                let proto = normalize_proto(&p.proto);
                if proto == "http" || proto == "https" {
                    pool.push(p.clone());
                }
            }
        }
        "high" => {
            for p in dns {
                let proto = normalize_proto(&p.proto);
                if proto == "https" || proto == "socks5" {
                    pool.push(p.clone());
                }
            }
            if pool.is_empty() {
                for p in combined {
                    if p.score >= 0.5 {
                        pool.push(p.clone());
                    }
                }
            }
        }
        "phantom" => {
            for p in dns {
                let proto = normalize_proto(&p.proto);
                if (proto == "socks5" || proto == "https") && p.score >= 0.4 {
                    pool.push(p.clone());
                }
            }
        }
        _ => {
            pool.extend_from_slice(combined);
            pool.extend_from_slice(dns);
            pool.extend_from_slice(non_dns);
        }
    }

    let mut seen = std::collections::HashSet::new();
    pool.retain(|p| {
        let key = format!("{}:{}", p.ip, p.port);
        if seen.contains(&key) {
            false
        } else {
            seen.insert(key);
            true
        }
    });

    pool
}

fn generate_chain_id<R: Rng + ?Sized>(rng: &mut R) -> String {
    let mut bytes = [0u8; 16];
    rng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn generate_key_nonce<R: Rng + ?Sized>(rng: &mut R) -> (String, String) {
    let mut key = [0u8; 32];
    let mut nonce = [0u8; 12];
    rng.fill_bytes(&mut key);
    rng.fill_bytes(&mut nonce);
    (hex::encode(key), hex::encode(nonce))
}

/// Weighted random selection of proxy indices based on their scores.
/// Higher score proxies are selected more often, but with diversity control.
///
/// `pool` - the proxy pool to select from
/// `rng` - random number generator
/// `num_to_select` - number of proxies to select
/// `diversity_exponent` - controls selection diversity:
///   - 1.0 = pure score weighting (highest scores strongly preferred)
///   - >1.0 = more diversity (flattens the weight distribution)
///   - <1.0 = even stronger preference for top scores
///
/// Returns indices of selected proxies (no duplicates).
fn weighted_random_choice<R: Rng>(
    pool: &[Proxy],
    mut rng: R,
    num_to_select: usize,
    diversity_exponent: f64,
) -> Vec<usize> {
    let mut selected_indices = Vec::with_capacity(num_to_select);
    let mut available: Vec<usize> = (0..pool.len()).collect();

    for _ in 0..num_to_select {
        if available.is_empty() {
            break;
        }

        // Calculate weights for available proxies using diversity exponent
        // weight = score^(1/exponent)
        // This prevents always selecting the same top proxies
        let weights: Vec<f64> = available
            .iter()
            .map(|&idx| {
                let score = if pool[idx].score > 0.0 { pool[idx].score } else { 0.5 };
                // Apply diversity exponent: higher exponent = flatter distribution
                score.powf(1.0 / diversity_exponent)
            })
            .collect();

        let total_weight: f64 = weights.iter().sum();

        if total_weight <= 0.0 {
            // Fallback to uniform random if all weights are zero
            let random_idx = rng.gen_range(0..available.len());
            selected_indices.push(available.remove(random_idx));
            continue;
        }

        // Weighted random selection using cumulative distribution
        let random_value = rng.gen_range(0.0..total_weight);
        let mut cumulative = 0.0;
        let mut chosen_position = 0;

        for (i, &weight) in weights.iter().enumerate() {
            cumulative += weight;
            if random_value <= cumulative {
                chosen_position = i;
                break;
            }
        }

        // Select and remove the chosen proxy from available pool
        selected_indices.push(available.remove(chosen_position));
    }

    selected_indices
}

fn choose_chain_internal<R: Rng>(
    mode: &str,
    pool: &[Proxy],
    mut rng: R,
) -> Option<RotationDecision> {
    if pool.is_empty() {
        return None;
    }

    let (hops_min, hops_max) = match mode {
        "phantom" => (3_usize, 5_usize),
        "high" => (2, 3),
        "stealth" => (1, 2),
        _ => (1, 1),
    };

    let hops = rng
        .gen_range(hops_min..=hops_max)
        .min(pool.len())
        .max(1);

    // Use weighted selection based on proxy scores
    // Diversity exponent of 1.5 provides a balance between preferring high scores
    // and maintaining diversity in chain selection
    let diversity_exponent = 1.5;
    let selected = weighted_random_choice(pool, &mut rng, hops, diversity_exponent);
    let mut chain = Vec::with_capacity(hops);
    let mut crypto = Vec::with_capacity(hops);
    let mut sum_latency = 0.0_f64;
    let mut min_score = f64::INFINITY;
    let mut max_score = f64::NEG_INFINITY;

    for idx in selected {
        let p = &pool[idx];
        let hop = ChainHop {
            ip: p.ip.clone(),
            port: p.port,
            proto: normalize_proto(&p.proto),
            country: p.country.clone(),
            latency: if p.latency > 0.0 { p.latency } else { 1.0 },
            score: if p.score > 0.0 { p.score } else { 0.5 },
        };
        sum_latency += hop.latency;
        if hop.score < min_score {
            min_score = hop.score;
        }
        if hop.score > max_score {
            max_score = hop.score;
        }

        let (key_hex, nonce_hex) = generate_key_nonce(&mut rng);
        crypto.push(CryptoHop { key_hex, nonce_hex });

        chain.push(hop);
    }

    let avg_latency = sum_latency / chain.len() as f64;

    let mut outer_rng = rng;
    let chain_id = generate_chain_id(&mut outer_rng);

    Some(RotationDecision {
        mode: mode.to_string(),
        timestamp: now_unix(),
        chain_id,
        chain,
        avg_latency,
        min_score: if min_score.is_finite() { min_score } else { 0.0 },
        max_score: if max_score.is_finite() { max_score } else { 0.0 },
        encryption: crypto,
    })
}

pub fn build_chain_decision(mode: &str, dns: &[Proxy], non_dns: &[Proxy], combined: &[Proxy]) -> Option<RotationDecision> {
    let pool = filter_mode_pool(mode, dns, non_dns, combined);
    if pool.is_empty() {
        return None;
    }

    let mut rng = StdRng::from_entropy();
    choose_chain_internal(mode, &pool, &mut rng)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::HopInfo;
    use rand::SeedableRng;

    /// Helper to create a test proxy
    fn make_proxy(ip: &str, port: u16, proto: &str, latency: f64, country: &str, anonymity: &str, score: f64) -> Proxy {
        Proxy {
            ip: ip.to_string(),
            port,
            proto: proto.to_string(),
            latency,
            country: country.to_string(),
            anonymity: anonymity.to_string(),
            score,
            fail_count: 0,
            last_verified: 0,
            alive: true,
        }
    }

    /// Helper to create DNS-capable proxy
    fn make_dns_proxy(ip: &str, port: u16, proto: &str, score: f64) -> Proxy {
        make_proxy(ip, port, proto, 100.0, "us", "elite", score)
    }

    /// Helper to create non-DNS proxy
    fn make_non_dns_proxy(ip: &str, port: u16, proto: &str, score: f64) -> Proxy {
        make_proxy(ip, port, proto, 100.0, "us", "elite", score)
    }

    #[test]
    fn test_filter_mode_lite() {
        // Lite mode includes all proxies
        let dns = vec![make_dns_proxy("192.168.1.1", 8080, "https", 0.8)];
        let non_dns = vec![make_non_dns_proxy("192.168.1.2", 8081, "http", 0.6)];
        let combined = vec![make_proxy("192.168.1.3", 8082, "socks5", 100.0, "us", "elite", 0.7)];

        let pool = filter_mode_pool("lite", &dns, &non_dns, &combined);

        // Lite mode should include all proxies (with deduplication)
        assert!(!pool.is_empty(), "Lite mode should have proxies");
        // Should have at least the combined proxies
        assert!(pool.len() >= combined.len());
    }

    #[test]
    fn test_filter_mode_lite_empty_pools() {
        // Lite mode with empty pools
        let dns: Vec<Proxy> = vec![];
        let non_dns: Vec<Proxy> = vec![];
        let combined: Vec<Proxy> = vec![];

        let pool = filter_mode_pool("lite", &dns, &non_dns, &combined);
        assert_eq!(pool.len(), 0);
    }

    #[test]
    fn test_filter_mode_phantom_requires_minimum_score() {
        // Phantom mode filters low-score proxies
        let dns = vec![
            make_dns_proxy("192.168.1.1", 8080, "https", 0.8),  // Above threshold
            make_dns_proxy("192.168.1.2", 8081, "socks5", 0.3), // Below threshold (0.4)
            make_dns_proxy("192.168.1.3", 8082, "https", 0.5),  // Above threshold
        ];
        let non_dns: Vec<Proxy> = vec![];
        let combined: Vec<Proxy> = vec![];

        let pool = filter_mode_pool("phantom", &dns, &non_dns, &combined);

        // Phantom mode requires score >= 0.4 and socks5/https
        assert_eq!(pool.len(), 2, "Should filter out low-score proxy");
        assert!(pool.iter().all(|p| p.score >= 0.4));
    }

    #[test]
    fn test_filter_mode_stealth() {
        // Stealth mode only includes HTTP/HTTPS proxies
        let dns = vec![
            make_dns_proxy("192.168.1.1", 8080, "https", 0.8),
            make_dns_proxy("192.168.1.2", 8081, "socks5", 0.8),
        ];
        let non_dns = vec![
            make_non_dns_proxy("192.168.1.3", 8082, "http", 0.6),
            make_non_dns_proxy("192.168.1.4", 8083, "socks4", 0.6),
        ];
        let combined: Vec<Proxy> = vec![];

        let pool = filter_mode_pool("stealth", &dns, &non_dns, &combined);

        // Stealth mode should only have http/https
        assert!(!pool.is_empty());
        for p in &pool {
            assert!(
                p.proto.to_lowercase() == "http" || p.proto.to_lowercase() == "https",
                "Stealth mode should only include HTTP/HTTPS, got: {}",
                p.proto
            );
        }
    }

    #[test]
    fn test_filter_mode_high() {
        // High mode prefers DNS-capable high-score proxies
        let dns = vec![
            make_dns_proxy("192.168.1.1", 8080, "https", 0.8),
            make_dns_proxy("192.168.1.2", 8081, "socks5", 0.7),
            make_dns_proxy("192.168.1.3", 8082, "http", 0.9), // Not DNS-capable
        ];
        let non_dns: Vec<Proxy> = vec![];
        let combined: Vec<Proxy> = vec![];

        let pool = filter_mode_pool("high", &dns, &non_dns, &combined);

        // High mode prefers https/socks5 from DNS pool
        assert!(!pool.is_empty());
        // Should prefer DNS-capable types
        for p in &pool {
            assert!(
                p.proto.to_lowercase() == "https" || p.proto.to_lowercase() == "socks5",
                "High mode should prefer DNS-capable types"
            );
        }
    }

    #[test]
    fn test_filter_mode_unknown_defaults_to_all() {
        // Unknown mode should default to including all proxies
        let dns = vec![make_dns_proxy("192.168.1.1", 8080, "https", 0.8)];
        let non_dns = vec![make_non_dns_proxy("192.168.1.2", 8081, "http", 0.6)];
        let combined = vec![make_proxy("192.168.1.3", 8082, "socks5", 100.0, "us", "elite", 0.7)];

        let pool = filter_mode_pool("unknown_mode", &dns, &non_dns, &combined);

        // Unknown mode should include all
        assert!(pool.len() >= dns.len() + non_dns.len() + combined.len() - 2); // Some dedup may occur
    }

    #[test]
    fn test_chain_length_by_mode() {
        // Different modes produce different chain lengths
        let dns = vec![
            make_dns_proxy("192.168.1.1", 8080, "https", 0.8),
            make_dns_proxy("192.168.1.2", 8081, "socks5", 0.7),
            make_dns_proxy("192.168.1.3", 8082, "https", 0.9),
            make_dns_proxy("192.168.1.4", 8083, "socks5", 0.6),
            make_dns_proxy("192.168.1.5", 8084, "https", 0.85),
        ];
        let non_dns: Vec<Proxy> = vec![];
        let combined: Vec<Proxy> = vec![];

        // Test lite mode (1 hop)
        let lite_decision = build_chain_decision("lite", &dns, &non_dns, &combined);
        assert!(lite_decision.is_some());
        let lite = lite_decision.unwrap();
        assert!(lite.chain.len() >= 1 && lite.chain.len() <= 1, "Lite mode should have 1 hop");

        // Test stealth mode (1-2 hops)
        let stealth_decision = build_chain_decision("stealth", &dns, &non_dns, &combined);
        assert!(stealth_decision.is_some());
        let stealth = stealth_decision.unwrap();
        assert!(stealth.chain.len() >= 1 && stealth.chain.len() <= 2, "Stealth mode should have 1-2 hops");

        // Test high mode (2-3 hops)
        let high_decision = build_chain_decision("high", &dns, &non_dns, &combined);
        assert!(high_decision.is_some());
        let high = high_decision.unwrap();
        assert!(high.chain.len() >= 2 && high.chain.len() <= 3, "High mode should have 2-3 hops");

        // Test phantom mode (3-5 hops)
        let phantom_decision = build_chain_decision("phantom", &dns, &non_dns, &combined);
        assert!(phantom_decision.is_some());
        let phantom = phantom_decision.unwrap();
        assert!(phantom.chain.len() >= 3 && phantom.chain.len() <= 5, "Phantom mode should have 3-5 hops");
    }

    #[test]
    fn test_chain_has_unique_hops() {
        // No duplicate proxies in a chain
        let dns = vec![
            make_dns_proxy("192.168.1.1", 8080, "https", 0.8),
            make_dns_proxy("192.168.1.2", 8081, "socks5", 0.7),
            make_dns_proxy("192.168.1.3", 8082, "https", 0.9),
            make_dns_proxy("192.168.1.4", 8083, "socks5", 0.6),
            make_dns_proxy("192.168.1.5", 8084, "https", 0.85),
        ];
        let non_dns: Vec<Proxy> = vec![];
        let combined: Vec<Proxy> = vec![];

        // Run multiple times to check for uniqueness
        for _ in 0..10 {
            let decision = build_chain_decision("phantom", &dns, &non_dns, &combined);
            assert!(decision.is_some());
            let decision = decision.unwrap();

            // Check that all hops have unique IP:port combinations
            let mut seen = std::collections::HashSet::new();
            for hop in &decision.chain {
                let key = format!("{}:{}", hop.ip, hop.port);
                assert!(
                    seen.insert(key.clone()),
                    "Duplicate hop found in chain: {}",
                    key
                );
            }
        }
    }

    #[test]
    fn test_chain_keys_are_valid_hex() {
        // Generated keys are valid hex strings
        let dns = vec![make_dns_proxy("192.168.1.1", 8080, "https", 0.8)];
        let non_dns: Vec<Proxy> = vec![];
        let combined: Vec<Proxy> = vec![];

        let decision = build_chain_decision("lite", &dns, &non_dns, &combined);
        assert!(decision.is_some());
        let decision = decision.unwrap();

        // Check that all encryption keys are valid 64-char hex (32 bytes)
        for crypto in &decision.encryption {
            assert_eq!(crypto.key_hex.len(), 64, "Key should be 64 hex chars (32 bytes)");
            assert!(
                crypto.key_hex.chars().all(|c| c.is_ascii_hexdigit()),
                "Key should be valid hex: {}",
                crypto.key_hex
            );
        }

        // Check that all nonces are valid 24-char hex (12 bytes)
        for crypto in &decision.encryption {
            assert_eq!(crypto.nonce_hex.len(), 24, "Nonce should be 24 hex chars (12 bytes)");
            assert!(
                crypto.nonce_hex.chars().all(|c| c.is_ascii_hexdigit()),
                "Nonce should be valid hex: {}",
                crypto.nonce_hex
            );
        }
    }

    #[test]
    fn test_chain_id_is_valid_hex() {
        // Chain ID should be valid hex
        let dns = vec![make_dns_proxy("192.168.1.1", 8080, "https", 0.8)];
        let non_dns: Vec<Proxy> = vec![];
        let combined: Vec<Proxy> = vec![];

        let decision = build_chain_decision("lite", &dns, &non_dns, &combined);
        assert!(decision.is_some());
        let decision = decision.unwrap();

        // Chain ID should be 32 hex chars (16 bytes)
        assert_eq!(decision.chain_id.len(), 32, "Chain ID should be 32 hex chars");
        assert!(
            decision.chain_id.chars().all(|c| c.is_ascii_hexdigit()),
            "Chain ID should be valid hex"
        );
    }

    #[test]
    fn test_build_chain_decision_empty_pool() {
        // Should return None when pool is empty
        let dns: Vec<Proxy> = vec![];
        let non_dns: Vec<Proxy> = vec![];
        let combined: Vec<Proxy> = vec![];

        let decision = build_chain_decision("lite", &dns, &non_dns, &combined);
        assert!(decision.is_none(), "Should return None for empty pool");
    }

    #[test]
    fn test_derive_key_from_secret_deterministic() {
        // Same inputs should produce same key
        let master_secret = b"test-secret-key";
        let chain_id = "test-chain-123";
        let hop_index = 0;

        let key1 = derive_key_from_secret(master_secret, chain_id, hop_index);
        let key2 = derive_key_from_secret(master_secret, chain_id, hop_index);

        assert_eq!(key1, key2, "Key derivation should be deterministic");
        assert_eq!(key1.len(), 64, "Key should be 64 hex chars");
    }

    #[test]
    fn test_derive_key_from_secret_different_hops() {
        // Different hop indices should produce different keys
        let master_secret = b"test-secret-key";
        let chain_id = "test-chain-123";

        let key0 = derive_key_from_secret(master_secret, chain_id, 0);
        let key1 = derive_key_from_secret(master_secret, chain_id, 1);
        let key2 = derive_key_from_secret(master_secret, chain_id, 2);

        assert_ne!(key0, key1, "Different hops should have different keys");
        assert_ne!(key1, key2, "Different hops should have different keys");
        assert_ne!(key0, key2, "Different hops should have different keys");
    }

    #[test]
    fn test_derive_key_from_secret_different_chains() {
        // Different chain IDs should produce different keys
        let master_secret = b"test-secret-key";
        let hop_index = 0;

        let key1 = derive_key_from_secret(master_secret, "chain-1", hop_index);
        let key2 = derive_key_from_secret(master_secret, "chain-2", hop_index);

        assert_ne!(key1, key2, "Different chains should have different keys");
    }

    #[test]
    fn test_derive_nonce_from_secret_deterministic() {
        // Same inputs should produce same nonce
        let master_secret = b"test-secret-key";
        let chain_id = "test-chain-123";
        let hop_index = 0;

        let nonce1 = derive_nonce_from_secret(master_secret, chain_id, hop_index);
        let nonce2 = derive_nonce_from_secret(master_secret, chain_id, hop_index);

        assert_eq!(nonce1, nonce2, "Nonce derivation should be deterministic");
        assert_eq!(nonce1.len(), 24, "Nonce should be 24 hex chars");
    }

    #[test]
    fn test_derive_nonce_from_secret_different_hops() {
        // Different hop indices should produce different nonces
        let master_secret = b"test-secret-key";
        let chain_id = "test-chain-123";

        let nonce0 = derive_nonce_from_secret(master_secret, chain_id, 0);
        let nonce1 = derive_nonce_from_secret(master_secret, chain_id, 1);
        let nonce2 = derive_nonce_from_secret(master_secret, chain_id, 2);

        assert_ne!(nonce0, nonce1, "Different hops should have different nonces");
        assert_ne!(nonce1, nonce2, "Different hops should have different nonces");
    }

    #[test]
    fn test_generate_encryption_from_secret() {
        // Should generate correct number of encryption hops
        let master_secret = b"test-secret-key";
        let chain_id = "test-chain-123";
        let num_hops = 3;

        let encryption = generate_encryption_from_secret(master_secret, chain_id, num_hops);

        assert_eq!(encryption.len(), num_hops);

        for (i, crypto) in encryption.iter().enumerate() {
            assert_eq!(crypto.key_hex.len(), 64, "Hop {} key should be 64 hex chars", i);
            assert_eq!(crypto.nonce_hex.len(), 24, "Hop {} nonce should be 24 hex chars", i);
        }
    }

    #[test]
    fn test_reconstruct_decision_from_topology() {
        // Should reconstruct a valid decision from topology
        let master_secret = b"test-secret-key";

        let topology = ChainTopology {
            chain_id: "test-chain-123".to_string(),
            hops: vec![
                HopInfo {
                    ip: "192.168.1.1".to_string(),
                    port: 8080,
                    proto: "https".to_string(),
                },
                HopInfo {
                    ip: "192.168.1.2".to_string(),
                    port: 8081,
                    proto: "socks5".to_string(),
                },
            ],
            created_at: 1234567890,
            mode: "lite".to_string(),
            avg_latency: 100.0,
            min_score: 0.5,
            max_score: 0.9,
        };

        let decision = reconstruct_decision_from_topology(&topology, master_secret);

        assert_eq!(decision.chain_id, topology.chain_id);
        assert_eq!(decision.mode, topology.mode);
        assert_eq!(decision.chain.len(), topology.hops.len());
        assert_eq!(decision.encryption.len(), topology.hops.len());

        // Verify encryption keys were regenerated
        for crypto in &decision.encryption {
            assert_eq!(crypto.key_hex.len(), 64);
            assert_eq!(crypto.nonce_hex.len(), 24);
        }
    }

    #[test]
    fn test_normalize_proto() {
        // Test protocol normalization (internal function via filter_mode_pool)
        let dns = vec![make_dns_proxy("192.168.1.1", 8080, "HTTPS", 0.8)];
        let non_dns: Vec<Proxy> = vec![];
        let combined: Vec<Proxy> = vec![];

        let decision = build_chain_decision("lite", &dns, &non_dns, &combined);
        assert!(decision.is_some());
        let decision = decision.unwrap();

        // Protocol should be normalized to lowercase
        for hop in &decision.chain {
            assert_eq!(hop.proto, hop.proto.to_lowercase());
        }
    }

    #[test]
    fn test_chain_metrics() {
        // Verify chain metrics are calculated correctly
        let dns = vec![
            make_dns_proxy("192.168.1.1", 8080, "https", 0.8),
            make_dns_proxy("192.168.1.2", 8081, "socks5", 0.7),
            make_dns_proxy("192.168.1.3", 8082, "https", 0.9),
        ];
        let non_dns: Vec<Proxy> = vec![];
        let combined: Vec<Proxy> = vec![];

        let decision = build_chain_decision("high", &dns, &non_dns, &combined);
        assert!(decision.is_some());
        let decision = decision.unwrap();

        // Verify avg_latency is reasonable
        assert!(decision.avg_latency > 0.0, "Average latency should be positive");

        // Verify min/max scores are within expected range
        assert!(decision.min_score >= 0.0 && decision.min_score <= 1.0);
        assert!(decision.max_score >= 0.0 && decision.max_score <= 1.0);
        assert!(decision.min_score <= decision.max_score);
    }

    #[test]
    fn test_weighted_random_choice_diversity() {
        // Test that weighted selection provides diversity
        let pool = vec![
            make_proxy("192.168.1.1", 8080, "https", 100.0, "us", "elite", 0.9),
            make_proxy("192.168.1.2", 8081, "socks5", 100.0, "us", "elite", 0.8),
            make_proxy("192.168.1.3", 8082, "https", 100.0, "us", "elite", 0.7),
            make_proxy("192.168.1.4", 8083, "socks5", 100.0, "us", "elite", 0.6),
            make_proxy("192.168.1.5", 8084, "https", 100.0, "us", "elite", 0.5),
        ];

        // Run multiple selections to verify diversity
        let mut all_selected_indices = std::collections::HashSet::new();

        for seed in 0..20u64 {
            let rng = StdRng::seed_from_u64(seed);
            let selected = weighted_random_choice(&pool, rng, 3, 1.5);
            assert_eq!(selected.len(), 3, "Should select 3 proxies");

            // Verify no duplicates in single selection
            let unique: std::collections::HashSet<_> = selected.iter().collect();
            assert_eq!(unique.len(), 3, "Should have no duplicates");

            for &idx in &selected {
                all_selected_indices.insert(idx);
            }
        }

        // Over many iterations, should have selected from multiple proxies
        assert!(all_selected_indices.len() >= 3, "Should show diversity in selection");
    }

    #[test]
    fn test_filter_deduplicates() {
        // Filter mode should deduplicate proxies
        let dns = vec![
            make_dns_proxy("192.168.1.1", 8080, "https", 0.8),
            make_dns_proxy("192.168.1.1", 8080, "socks5", 0.7), // Same IP:port
        ];
        let non_dns: Vec<Proxy> = vec![];
        let combined: Vec<Proxy> = vec![];

        let pool = filter_mode_pool("lite", &dns, &non_dns, &combined);

        // Should have deduplicated
        let mut seen = std::collections::HashSet::new();
        for p in &pool {
            let key = format!("{}:{}", p.ip, p.port);
            assert!(seen.insert(key), "Pool should be deduplicated");
        }
    }
}