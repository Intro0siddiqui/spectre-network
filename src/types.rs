use serde::{Deserialize, Serialize};

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
    /// Consecutive verification failures (prune at >= 3)
    #[serde(default)]
    pub fail_count: u32,
    /// Unix timestamp of last successful or attempted verify
    #[serde(default)]
    pub last_verified: u64,
    /// Whether the last verification probe succeeded
    #[serde(default = "default_alive")]
    pub alive: bool,
}

fn default_alive() -> bool {
    true
}

impl Proxy {
    pub fn key(&self) -> String {
        format!("{}:{}", self.ip, self.port)
    }
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
    pub encryption: Vec<CryptoHop>,
}
