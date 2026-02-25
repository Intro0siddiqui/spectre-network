use serde::{Deserialize, Deserializer, Serialize};

/// Proxy quality tier based on real connectivity testing
/// Higher tiers = better quality, faster, more reliable
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ProxyTier {
    /// Dead or very slow (>3s latency, fails CONNECT)
    #[serde(rename = "dead")]
    Dead = 0,
    /// Working but slow (1-3s latency, some failures)
    #[serde(rename = "bronze")]
    Bronze = 1,
    /// Good quality (0.5-1s latency, reliable)
    #[serde(rename = "silver")]
    Silver = 2,
    /// Fast and reliable (0.1-0.5s latency, 200 OK responses)
    #[serde(rename = "gold")]
    Gold = 3,
    /// Premium ( <0.1s latency, never blocked, elite anonymity)
    #[serde(rename = "platinum")]
    Platinum = 4,
}

impl Default for ProxyTier {
    fn default() -> Self {
        ProxyTier::Bronze
    }
}

/// Custom deserializer for ProxyTier that handles empty strings, missing values, and Option types
fn deserialize_tier<'de, D>(deserializer: D) -> Result<ProxyTier, D::Error>
where
    D: Deserializer<'de>,
{
    // Handle both string and Option<string> cases
    let opt = Option::<String>::deserialize(deserializer)?;
    Ok(match opt.as_deref() {
        Some("platinum") => ProxyTier::Platinum,
        Some("gold") => ProxyTier::Gold,
        Some("silver") => ProxyTier::Silver,
        Some("bronze") => ProxyTier::Bronze,
        Some("dead") => ProxyTier::Dead,
        Some("") | None => ProxyTier::Bronze, // Default for empty or missing values
        Some(unknown) => {
            // Log unknown tier and default to Bronze
            log::warn!("Unknown proxy tier '{}', defaulting to bronze", unknown);
            ProxyTier::Bronze
        }
    })
}

impl ProxyTier {
    pub fn from_score(score: f64) -> Self {
        if score >= 0.85 {
            ProxyTier::Platinum
        } else if score >= 0.70 {
            ProxyTier::Gold
        } else if score >= 0.50 {
            ProxyTier::Silver
        } else if score >= 0.30 {
            ProxyTier::Bronze
        } else {
            ProxyTier::Dead
        }
    }

    pub fn min_score(&self) -> f64 {
        match self {
            ProxyTier::Platinum => 0.85,
            ProxyTier::Gold => 0.70,
            ProxyTier::Silver => 0.50,
            ProxyTier::Bronze => 0.30,
            ProxyTier::Dead => 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proxy {
    #[serde(rename = "ip", alias = "IP")]
    pub ip: String,
    #[serde(rename = "port", alias = "Port")]
    pub port: u16,
    #[serde(rename = "type", alias = "protocol")]
    pub proto: String,
    #[serde(default)]
    pub latency: f64,
    #[serde(default)]
    pub country: String,
    #[serde(default)]
    pub anonymity: String,
    #[serde(default)]
    pub score: f64,
    /// Quality tier based on real connectivity testing (assigned by Rust polish)
    #[serde(default, deserialize_with = "deserialize_tier")]
    pub tier: ProxyTier,
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

/// ChainTopology contains only the chain structure without cryptographic material.
/// This struct is safe to persist to disk as it excludes encryption keys and nonces.
/// Use this for storing chain decisions in last_chain.json to prevent key leakage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainTopology {
    pub chain_id: String,
    pub hops: Vec<HopInfo>,
    pub created_at: u64,
    pub mode: String,
    pub avg_latency: f64,
    pub min_score: f64,
    pub max_score: f64,
}

/// HopInfo contains only the network topology information for a chain hop.
/// Excludes all cryptographic material (keys, nonces).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HopInfo {
    pub ip: String,
    pub port: u16,
    #[serde(rename = "type")]
    pub proto: String,
}

impl RotationDecision {
    /// Converts a RotationDecision to ChainTopology, stripping all encryption keys.
    /// This is the safe version to persist to disk.
    pub fn to_chain_topology(&self) -> ChainTopology {
        ChainTopology {
            chain_id: self.chain_id.clone(),
            hops: self
                .chain
                .iter()
                .map(|h| HopInfo {
                    ip: h.ip.clone(),
                    port: h.port,
                    proto: h.proto.clone(),
                })
                .collect(),
            created_at: self.timestamp,
            mode: self.mode.clone(),
            avg_latency: self.avg_latency,
            min_score: self.min_score,
            max_score: self.max_score,
        }
    }
}
