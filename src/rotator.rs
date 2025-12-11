use rand::prelude::*;
use std::time::{SystemTime, UNIX_EPOCH};
use crate::types::{Proxy, ChainHop, CryptoHop, RotationDecision};

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

    let mut indices: Vec<usize> = (0..pool.len()).collect();
    indices.shuffle(&mut rng);

    let selected = &indices[..hops];
    let mut chain = Vec::with_capacity(hops);
    let mut crypto = Vec::with_capacity(hops);
    let mut sum_latency = 0.0_f64;
    let mut min_score = f64::INFINITY;
    let mut max_score = f64::NEG_INFINITY;

    for &idx in selected {
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