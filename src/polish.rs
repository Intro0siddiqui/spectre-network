use crate::types::{Proxy, ProxyTier, ScoringWeights};
use std::collections::{HashMap, HashSet};

lazy_static::lazy_static! {
    static ref ANONYMITY_SCORES: HashMap<&'static str, f64> = {
        let mut m = HashMap::new();
        m.insert("elite", 1.0);
        m.insert("anonymous", 0.7);
        m.insert("transparent", 0.3);
        m.insert("", 0.1);
        m
    };
    static ref TYPE_SCORES: HashMap<&'static str, f64> = {
        let mut m = HashMap::new();
        m.insert("socks5", 1.0);
        m.insert("https", 0.9);
        m.insert("socks4", 0.6);
        m.insert("http", 0.5);
        m
    };
    static ref PREFERRED_COUNTRIES: HashSet<&'static str> = {
        let mut s = HashSet::new();
        s.insert("us");
        s.insert("de");
        s.insert("nl");
        s.insert("uk");
        s.insert("fr");
        s.insert("ca");
        s.insert("sg");
        s
    };
    static ref DNS_CAPABLE_TYPES: HashSet<&'static str> = {
        let mut s = HashSet::new();
        s.insert("https");
        s.insert("socks5");
        s
    };
    static ref CLOUD_IP_RANGES: HashSet<&'static str> = {
        let mut s = HashSet::new();
        s.insert("3.5."); // AWS
        s.insert("34.200."); // AWS
        s.insert("35.180."); // GCP
        s.insert("52.0."); // Azure
        s
    };
}

pub fn deduplicate_proxies(proxies: Vec<Proxy>) -> Vec<Proxy> {
    let mut seen: HashMap<String, Proxy> = HashMap::new();
    for p in proxies {
        let key = p.key();
        match seen.get(&key) {
            Some(existing) => {
                // If existing is standard and new is premium, replace it
                if existing.source_type == "standard" && p.source_type == "premium" {
                    seen.insert(key, p);
                }
            }
            None => {
                seen.insert(key, p);
            }
        }
    }
    seen.into_values().collect()
}

pub fn calculate_scores(mut proxies: Vec<Proxy>, weights: &ScoringWeights) -> Vec<Proxy> {
    if proxies.is_empty() {
        return proxies;
    }

    let max_latency = proxies
        .iter()
        .filter(|p| p.latency > 0.0)
        .map(|p| p.latency)
        .fold(0.0, f64::max)
        .max(1.0); // Avoid div by zero

    for p in &mut proxies {
        let mut score = 0.0;

        // Latency
        if p.latency > 0.0 {
            let latency_score = 1.0 - (p.latency / max_latency);
            score += latency_score * weights.latency;
        }

        // Anonymity
        let anon = p.anonymity.to_lowercase();
        let anon_score = ANONYMITY_SCORES.get(anon.as_str()).unwrap_or(&0.1);
        score += anon_score * weights.anonymity;

        // Country
        let country = p.country.to_lowercase();
        let country_score = if PREFERRED_COUNTRIES.contains(country.as_str()) {
            1.0
        } else {
            0.5
        };
        score += country_score * weights.country;

        // Protocol
        let proto = p.proto.to_lowercase();
        let type_score = TYPE_SCORES.get(proto.as_str()).unwrap_or(&0.3);
        score += type_score * weights.protocol;

        // Premium Bonus
        if p.source_type == "premium" {
            score += weights.premium;
        }

        // Cloud/Datacenter Penalty
        for range in CLOUD_IP_RANGES.iter() {
            if p.ip.starts_with(range) {
                score *= 0.5;
                break;
            }
        }

        // DNS Bonus
        if DNS_CAPABLE_TYPES.contains(proto.as_str()) {
            score *= 1.2;
        }

        p.score = score;
        
        // Assign tier based on final score
        p.tier = ProxyTier::from_score(score);
    }

    // Sort descending by score
    proxies.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    proxies
}

pub fn split_proxy_pools(proxies: Vec<Proxy>) -> (Vec<Proxy>, Vec<Proxy>) {
    let mut dns = Vec::new();
    let mut non_dns = Vec::new();

    for p in proxies {
        let proto = p.proto.to_lowercase();
        // Skip SOCKS4 as it's outdated and doesn't support DNS resolution via proxy
        if proto == "socks4" {
            continue;
        }

        if DNS_CAPABLE_TYPES.contains(proto.as_str()) {
            dns.push(p);
        } else {
            non_dns.push(p);
        }
    }
    (dns, non_dns)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ProxyTier;

    /// Helper to create a test proxy
    fn make_proxy(
        ip: &str,
        port: u16,
        proto: &str,
        latency: f64,
        country: &str,
        anonymity: &str,
    ) -> Proxy {
        Proxy {
            ip: ip.to_string(),
            port,
            proto: proto.to_string(),
            latency,
            country: country.to_string(),
            anonymity: anonymity.to_string(),
            score: 0.0,
            tier: ProxyTier::Bronze,
            fail_count: 0,
            last_verified: 0,
            alive: true,
            source_type: "standard".to_string(),
        }
    }

    #[test]
    fn test_deduplicate_removes_duplicates() {
        // Same IP:port should be deduplicated
        let proxies = vec![
            make_proxy("192.168.1.1", 8080, "http", 100.0, "us", "elite"),
            make_proxy("192.168.1.1", 8080, "http", 150.0, "us", "elite"), // Duplicate
            make_proxy("192.168.1.2", 8080, "https", 120.0, "de", "anonymous"),
            make_proxy("192.168.1.1", 8080, "socks5", 200.0, "us", "elite"), // Another duplicate
            make_proxy("192.168.1.3", 9090, "socks5", 80.0, "nl", "elite"),
        ];

        let deduplicated = deduplicate_proxies(proxies);

        // Should have 3 unique proxies (by IP:port)
        assert_eq!(deduplicated.len(), 3, "Should have 3 unique proxies");
    }

    #[test]
    fn test_deduplicate_empty_list() {
        let proxies: Vec<Proxy> = vec![];
        let deduplicated = deduplicate_proxies(proxies);
        assert_eq!(deduplicated.len(), 0);
    }

    #[test]
    fn test_score_calculation_high_latency() {
        let proxies = vec![
            make_proxy("192.168.1.1", 8080, "http", 1000.0, "us", "elite"), // High latency
            make_proxy("192.168.1.2", 8081, "http", 50.0, "us", "elite"),   // Low latency
        ];

        let weights = ScoringWeights::default();
        let scored = calculate_scores(proxies, &weights);

        let high_latency_proxy = scored.iter().find(|p| p.latency == 1000.0).unwrap();
        let low_latency_proxy = scored.iter().find(|p| p.latency == 50.0).unwrap();

        assert!(low_latency_proxy.score > high_latency_proxy.score);
    }

    #[test]
    fn test_asn_filter() {
        let p1 = make_proxy("3.5.1.1", 80, "http", 100.0, "us", "elite"); // AWS range
        let p2 = make_proxy("1.1.1.1", 80, "http", 100.0, "us", "elite");
        
        let weights = ScoringWeights::default();
        let scored = calculate_scores(vec![p1, p2], &weights);
        
        let aws = scored.iter().find(|p| p.ip == "3.5.1.1").unwrap();
        let normal = scored.iter().find(|p| p.ip == "1.1.1.1").unwrap();
        
        assert!(normal.score > aws.score, "Cloud IP should be penalized");
    }

    #[test]
    fn test_score_calculation_low_latency() {
        let proxies = vec![
            make_proxy("192.168.1.1", 8080, "http", 10.0, "us", "elite"),
            make_proxy("192.168.1.2", 8081, "http", 500.0, "us", "elite"),
        ];

        let weights = ScoringWeights::default();
        let scored = calculate_scores(proxies, &weights);

        assert_eq!(scored[0].latency, 10.0);
        assert!(scored[0].score > scored[1].score);
    }

    #[test]
    fn test_anonymity_scoring() {
        let proxies = vec![
            make_proxy("192.168.1.1", 8080, "http", 100.0, "us", "elite"),
            make_proxy("192.168.1.2", 8081, "http", 100.0, "us", "anonymous"),
            make_proxy("192.168.1.3", 8082, "http", 100.0, "us", "transparent"),
        ];

        let weights = ScoringWeights::default();
        let scored = calculate_scores(proxies, &weights);

        let elite = scored.iter().find(|p| p.anonymity == "elite").unwrap();
        let anonymous = scored.iter().find(|p| p.anonymity == "anonymous").unwrap();
        let transparent = scored.iter().find(|p| p.anonymity == "transparent").unwrap();

        assert!(elite.score > anonymous.score);
        assert!(anonymous.score > transparent.score);
    }

    #[test]
    fn test_country_bonus() {
        let proxies = vec![
            make_proxy("192.168.1.1", 8080, "http", 100.0, "us", "elite"), // Preferred
            make_proxy("192.168.1.2", 8081, "http", 100.0, "xx", "elite"), // Not preferred
        ];

        let weights = ScoringWeights::default();
        let scored = calculate_scores(proxies, &weights);

        let us_proxy = scored.iter().find(|p| p.country == "us").unwrap();
        let xx_proxy = scored.iter().find(|p| p.country == "xx").unwrap();

        assert!(us_proxy.score > xx_proxy.score);
    }

    #[test]
    fn test_protocol_scoring() {
        let proxies = vec![
            make_proxy("192.168.1.1", 8080, "socks5", 100.0, "us", "elite"),
            make_proxy("192.168.1.2", 8081, "http", 100.0, "us", "elite"),
        ];

        let weights = ScoringWeights::default();
        let scored = calculate_scores(proxies, &weights);

        let socks5 = scored.iter().find(|p| p.proto == "socks5").unwrap();
        let http = scored.iter().find(|p| p.proto == "http").unwrap();

        assert!(socks5.score > http.score);
    }

    #[test]
    fn test_premium_bonus() {
        let mut p1 = make_proxy("1.1.1.1", 80, "http", 100.0, "us", "elite");
        p1.source_type = "standard".to_string();
        
        let mut p2 = make_proxy("2.2.2.2", 80, "http", 100.0, "us", "elite");
        p2.source_type = "premium".to_string();
        
        let weights = ScoringWeights::default();
        let scored = calculate_scores(vec![p1, p2], &weights);
        
        let standard = scored.iter().find(|p| p.ip == "1.1.1.1").unwrap();
        let premium = scored.iter().find(|p| p.ip == "2.2.2.2").unwrap();
        
        assert!(premium.score > standard.score);
    }

    #[test]
    fn test_calculate_scores_empty_weights() {
        let proxies: Vec<Proxy> = vec![];
        let weights = ScoringWeights::default();
        let scored = calculate_scores(proxies, &weights);
        assert_eq!(scored.len(), 0);
    }

    #[test]
    fn test_split_proxy_pools() {
        let proxies = vec![
            make_proxy("192.168.1.1", 8080, "https", 100.0, "us", "elite"),
            make_proxy("192.168.1.2", 8081, "socks5", 100.0, "us", "elite"),
            make_proxy("192.168.1.3", 8082, "http", 100.0, "us", "elite"),
        ];

        let (dns, non_dns) = split_proxy_pools(proxies);

        assert_eq!(dns.len(), 2);
        assert_eq!(non_dns.len(), 1);
    }
}
