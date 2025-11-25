//! Spectre Network - Rust Rotator (Mojo replacement, pyo3 library)
//!
//! Responsibilities:
//! - Load processed proxy pools from python_polish.py outputs
//! - Build mode-specific pools (lite, stealth, high, phantom)
//! - Construct multi-hop chains for Phantom mode
//! - Generate cryptographic metadata for chains (IDs, keys, nonces)
//! - Expose a safe Python API via pyo3 (no subprocess / no Mojo)
//!
//! Python integration usage (example):
//!
//! from rotator_rs import build_chain
//!
//! decision = build_chain(mode="phantom", workspace="/workspace/spectre-network")
//! print(decision)
//!
//! This file is written as a combined binary+library for flexibility,
//! but primary use for this project is as a pyo3-powered library module.

use rand::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

// For robust crypto metadata use ring or similar. To keep this self-contained
// and buildable, we implement deterministic, secure random generation via rand.
// In a production environment, you can switch to ring/openssl as needed.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proxy {
    #[serde(alias = "ip")]
    pub ip: String,
    #[serde(alias = "port")]
    pub port: u16,
    #[serde(alias = "type")]
    pub proto: String,
    #[serde(default)]
    pub latency: f64,
    #[serde(default)]
    pub country: String,
    #[serde(default)]
    pub anonymity: String,
    #[serde(default)]
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainHop {
    pub ip: String,
    pub port: u16,
    pub proto: String,
    pub country: String,
    pub latency: f64,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryptoHop {
    pub key_hex: String,
    pub nonce_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationDecision {
    pub mode: String,
    pub timestamp: u64,
    pub chain_id: String,
    pub chain: Vec<ChainHop>,
    pub avg_latency: f64,
    pub min_score: f64,
    pub max_score: f64,
    // Encryption-centric metadata for each hop in order.
    pub encryption: Vec<CryptoHop>,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn load_json_array(path: &Path) -> io::Result<Vec<Proxy>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(path)?;
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }
    let proxies: Vec<Proxy> = serde_json::from_str(&raw)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{}: {}", path.display(), e)))?;
    Ok(proxies)
}

fn load_all_pools(workspace: &Path) -> io::Result<(Vec<Proxy>, Vec<Proxy>, Vec<Proxy>)> {
    let dns = load_json_array(&workspace.join("proxies_dns.json"))?;
    let non_dns = load_json_array(&workspace.join("proxies_non_dns.json"))?;
    let combined = load_json_array(&workspace.join("proxies_combined.json"))?;
    Ok((dns, non_dns, combined))
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

fn filter_mode_pool(mode: &str, dns: &[Proxy], non_dns: &[Proxy], combined: &[Proxy]) -> Vec<Proxy> {
    let mut pool = Vec::new();
    match mode {
        // Lite: anything reasonably fast
        "lite" => {
            pool.extend_from_slice(combined);
            pool.extend_from_slice(non_dns);
            pool.extend_from_slice(dns);
        }
        // Stealth: prefer http/https
        "stealth" => {
            for p in combined.iter().chain(dns).chain(non_dns) {
                let proto = normalize_proto(&p.proto);
                if proto == "http" || proto == "https" {
                    pool.push(p.clone());
                }
            }
        }
        // High: prefer DNS-safe + strong types
        "high" => {
            for p in dns {
                let proto = normalize_proto(&p.proto);
                if proto == "https" || proto == "socks5" {
                    pool.push(p.clone());
                }
            }
            // fallback: any high-score from combined
            if pool.is_empty() {
                for p in combined {
                    if p.score >= 0.5 {
                        pool.push(p.clone());
                    }
                }
            }
        }
        // Phantom: DNS-safe only, socks5/https, high scores
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

    // Deduplicate by ip:port
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
    // 16 random bytes -> 32 hex chars
    let mut bytes = [0u8; 16];
    rng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn generate_key_nonce<R: Rng + ?Sized>(rng: &mut R) -> (String, String) {
    // 32-byte key (256-bit), 12-byte nonce (96-bit, AEAD-ready)
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

        // For each hop, attach independent key/nonce for layered encryption.
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

/// Core Rust API (non-Python) for internal tests / binary.
pub fn build_chain_decision(mode: &str, workspace: &Path) -> io::Result<RotationDecision> {
    let (dns, non_dns, combined) = load_all_pools(workspace)?;
    let pool = filter_mode_pool(mode, &dns, &non_dns, &combined);
    if pool.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("No proxies available for mode '{}' in {}", mode, workspace.display()),
        ));
    }

    let mut rng = StdRng::from_entropy();
    match choose_chain_internal(mode, &pool, &mut rng) {
        Some(d) => Ok(d),
        None => Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Unable to construct chain for mode '{}'", mode),
        )),
    }
}

// ========================
// pyo3 Python Integration
// ========================

#[pyfunction]
fn build_chain(py: Python<'_>, mode: &str, workspace: Option<&str>) -> PyResult<PyObject> {
    let mode = mode.to_lowercase();
    let ws = workspace
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let decision = build_chain_decision(&mode, &ws).map_err(|e| {
        PyRuntimeError::new_err(format!(
            "Failed to build chain for mode='{}', workspace='{}': {}",
            mode,
            ws.display(),
            e
        ))
    })?;

    // Convert RotationDecision -> Python dict for ergonomic consumption.
    let result = PyDict::new(py);

    result.set_item("mode", decision.mode)?;
    result.set_item("timestamp", decision.timestamp)?;
    result.set_item("chain_id", decision.chain_id)?;
    result.set_item("avg_latency", decision.avg_latency)?;
    result.set_item("min_score", decision.min_score)?;
    result.set_item("max_score", decision.max_score)?;

    // Chain hops as list[dict]
    let chain_py = PyDict::new(py);
    let hops = decision
        .chain
        .iter()
        .enumerate()
        .map(|(i, hop)| {
            let d = PyDict::new(py);
            d.set_item("index", i + 1)?;
            d.set_item("ip", &hop.ip)?;
            d.set_item("port", hop.port)?;
            d.set_item("proto", &hop.proto)?;
            d.set_item("country", &hop.country)?;
            d.set_item("latency", hop.latency)?;
            d.set_item("score", hop.score)?;
            Ok(d.to_object(py))
        })
        .collect::<PyResult<Vec<_>>>()?;
    result.set_item("chain", hops)?;

    // Encryption metadata as list[dict], aligned with chain
    let enc = decision
        .encryption
        .iter()
        .enumerate()
        .map(|(i, ch)| {
            let d = PyDict::new(py);
            d.set_item("hop", i + 1)?;
            d.set_item("key_hex", &ch.key_hex)?;
            d.set_item("nonce_hex", &ch.nonce_hex)?;
            Ok(d.to_object(py))
        })
        .collect::<PyResult<Vec<_>>>()?;
    result.set_item("encryption", enc)?;

    Ok(result.to_object(py))
}

#[pyfunction]
fn validate_mode(mode: &str) -> PyResult<()> {
    let m = mode.to_lowercase();
    let allowed = ["lite", "stealth", "high", "phantom"];
    if allowed.contains(&m.as_str()) {
        Ok(())
    } else {
        Err(PyValueError::new_err(format!(
            "Invalid mode '{}'. Allowed: lite, stealth, high, phantom",
            mode
        )))
    }
}

#[pyfunction]
fn version() -> PyResult<String> {
    Ok("rotator_rs_pyo3_v1".to_string())
}

#[pymodule]
fn rotator_rs(m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(build_chain, m)?)?;
    m.add_function(wrap_pyfunction!(validate_mode, m)?)?;
    m.add_function(wrap_pyfunction!(version, m)?)?;
    Ok(())
}

// ========================
// Optional: CLI entrypoint
// ========================
//
// This keeps backward compatibility for manual debugging:
// `cargo run -- --mode phantom --workspace .`
//
#[cfg(feature = "cli")]
fn main() {
    use std::io::Write;

    let mut mode = "phantom".to_string();
    let mut workspace = std::env::current_dir().unwrap_or_else(|_| ".".into());

    {
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--mode" => {
                    if let Some(m) = args.next() {
                        mode = m;
                    }
                }
                "--workspace" => {
                    if let Some(w) = args.next() {
                        workspace = std::path::PathBuf::from(w);
                    }
                }
                _ => {}
            }
        }
    }

    let decision = match build_chain_decision(&mode, &workspace) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    println!(
        "{}",
        serde_json::to_string_pretty(&decision).unwrap_or_else(|_| "{}".to_string())
    );
    let _ = io::stdout().flush();
}
