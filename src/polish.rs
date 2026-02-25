use crate::types::{Proxy, ProxyTier};
use std::collections::{HashMap, HashSet};

const LATENCY_WEIGHT: f64 = 0.4;
const ANONYMITY_WEIGHT: f64 = 0.3;
const COUNTRY_WEIGHT: f64 = 0.2;
const TYPE_WEIGHT: f64 = 0.1;

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
}

pub fn deduplicate_proxies(proxies: Vec<Proxy>) -> Vec<Proxy> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for p in proxies {
        let key = p.key();
        if !seen.contains(&key) {
            seen.insert(key);
            unique.push(p);
        }
    }
    unique
}

pub fn calculate_scores(mut proxies: Vec<Proxy>) -> Vec<Proxy> {
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
            score += latency_score * LATENCY_WEIGHT;
        }

        // Anonymity
        let anon = p.anonymity.to_lowercase();
        let anon_score = ANONYMITY_SCORES.get(anon.as_str()).unwrap_or(&0.1);
        score += anon_score * ANONYMITY_WEIGHT;

        // Country
        let country = p.country.to_lowercase();
        let country_score = if PREFERRED_COUNTRIES.contains(country.as_str()) {
            1.0
        } else {
            0.5
        };
        score += country_score * COUNTRY_WEIGHT;

        // Protocol
        let proto = p.proto.to_lowercase();
        let type_score = TYPE_SCORES.get(proto.as_str()).unwrap_or(&0.3);
        score += type_score * TYPE_WEIGHT;

        // Bonus
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

        // Verify which proxies remain (first occurrence of each IP:port)
        let keys: Vec<String> = deduplicated.iter().map(|p| p.key()).collect();
        assert!(keys.contains(&"192.168.1.1:8080".to_string()));
        assert!(keys.contains(&"192.168.1.2:8080".to_string()));
        assert!(keys.contains(&"192.168.1.3:9090".to_string()));
    }

    #[test]
    fn test_deduplicate_empty_list() {
        let proxies: Vec<Proxy> = vec![];
        let deduplicated = deduplicate_proxies(proxies);
        assert_eq!(deduplicated.len(), 0);
    }

    #[test]
    fn test_deduplicate_all_unique() {
        let proxies = vec![
            make_proxy("192.168.1.1", 8080, "http", 100.0, "us", "elite"),
            make_proxy("192.168.1.2", 8081, "https", 120.0, "de", "anonymous"),
            make_proxy("192.168.1.3", 9090, "socks5", 80.0, "nl", "elite"),
        ];

        let deduplicated = deduplicate_proxies(proxies);
        assert_eq!(deduplicated.len(), 3);
    }

    #[test]
    fn test_score_calculation_high_latency() {
        // High latency proxy should get low latency score
        let proxies = vec![
            make_proxy("192.168.1.1", 8080, "http", 1000.0, "us", "elite"), // High latency
            make_proxy("192.168.1.2", 8081, "http", 50.0, "us", "elite"),   // Low latency
        ];

        let scored = calculate_scores(proxies);

        // Find the high latency proxy
        let high_latency_proxy = scored.iter().find(|p| p.latency == 1000.0).unwrap();
        let low_latency_proxy = scored.iter().find(|p| p.latency == 50.0).unwrap();

        // Low latency should have higher score than high latency
        assert!(
            low_latency_proxy.score > high_latency_proxy.score,
            "Low latency proxy ({}) should have higher score than high latency proxy ({})",
            low_latency_proxy.score,
            high_latency_proxy.score
        );
    }

    #[test]
    fn test_score_calculation_low_latency() {
        // Low latency proxy should get high latency score
        let proxies = vec![
            make_proxy("192.168.1.1", 8080, "http", 10.0, "us", "elite"), // Very low latency
            make_proxy("192.168.1.2", 8081, "http", 500.0, "us", "elite"), // High latency
        ];

        let scored = calculate_scores(proxies);

        // The lowest latency proxy should be first (sorted by score descending)
        assert_eq!(
            scored[0].latency, 10.0,
            "Lowest latency proxy should have highest score"
        );
        assert!(scored[0].score > scored[1].score);
    }

    #[test]
    fn test_anonymity_scoring() {
        // Elite > Anonymous > Transparent
        let proxies = vec![
            make_proxy("192.168.1.1", 8080, "http", 100.0, "us", "elite"),
            make_proxy("192.168.1.2", 8081, "http", 100.0, "us", "anonymous"),
            make_proxy("192.168.1.3", 8082, "http", 100.0, "us", "transparent"),
            make_proxy("192.168.1.4", 8083, "http", 100.0, "us", ""), // Empty anonymity
        ];

        let scored = calculate_scores(proxies);

        let elite = scored.iter().find(|p| p.anonymity == "elite").unwrap();
        let anonymous = scored.iter().find(|p| p.anonymity == "anonymous").unwrap();
        let transparent = scored
            .iter()
            .find(|p| p.anonymity == "transparent")
            .unwrap();
        let empty = scored.iter().find(|p| p.anonymity == "").unwrap();

        // Verify anonymity scoring order (with same latency, elite should score highest)
        assert!(
            elite.score > anonymous.score,
            "Elite should score higher than anonymous"
        );
        assert!(
            anonymous.score > transparent.score,
            "Anonymous should score higher than transparent"
        );
        assert!(
            transparent.score > empty.score,
            "Transparent should score higher than empty"
        );
    }

    #[test]
    fn test_country_bonus() {
        // Preferred countries get bonus
        let proxies = vec![
            make_proxy("192.168.1.1", 8080, "http", 100.0, "us", "elite"), // Preferred
            make_proxy("192.168.1.2", 8081, "http", 100.0, "de", "elite"), // Preferred
            make_proxy("192.168.1.3", 8082, "http", 100.0, "nl", "elite"), // Preferred
            make_proxy("192.168.1.4", 8083, "http", 100.0, "xx", "elite"), // Not preferred
            make_proxy("192.168.1.5", 8084, "http", 100.0, "unknown", "elite"), // Not preferred
        ];

        let scored = calculate_scores(proxies);

        let us_proxy = scored.iter().find(|p| p.country == "us").unwrap();
        let xx_proxy = scored.iter().find(|p| p.country == "xx").unwrap();

        // Preferred country should have higher score
        assert!(
            us_proxy.score > xx_proxy.score,
            "Preferred country (US) should have higher score than non-preferred (XX): {} vs {}",
            us_proxy.score,
            xx_proxy.score
        );
    }

    #[test]
    fn test_preferred_countries_list() {
        // Verify all preferred countries are recognized
        let preferred = ["us", "de", "nl", "uk", "fr", "ca", "sg"];
        let non_preferred = ["xx", "yy", "zz", "unknown"];

        let mut proxies = Vec::new();
        for (i, country) in preferred.iter().enumerate() {
            proxies.push(make_proxy(
                &format!("192.168.1.{}", i + 1),
                8080,
                "http",
                100.0,
                country,
                "elite",
            ));
        }
        for (i, country) in non_preferred.iter().enumerate() {
            proxies.push(make_proxy(
                &format!("192.168.2.{}", i + 1),
                8080,
                "http",
                100.0,
                country,
                "elite",
            ));
        }

        let scored = calculate_scores(proxies);

        // All preferred countries should have higher scores than non-preferred
        let preferred_scores: Vec<f64> = scored
            .iter()
            .filter(|p| preferred.contains(&p.country.as_str()))
            .map(|p| p.score)
            .collect();
        let non_preferred_scores: Vec<f64> = scored
            .iter()
            .filter(|p| non_preferred.contains(&p.country.as_str()))
            .map(|p| p.score)
            .collect();

        let min_preferred = preferred_scores
            .iter()
            .cloned()
            .fold(f64::INFINITY, f64::min);
        let max_non_preferred = non_preferred_scores
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max);

        assert!(
            min_preferred > max_non_preferred,
            "All preferred countries should score higher"
        );
    }

    #[test]
    fn test_protocol_scoring() {
        // Different protocols have different base scores
        let proxies = vec![
            make_proxy("192.168.1.1", 8080, "socks5", 100.0, "us", "elite"),
            make_proxy("192.168.1.2", 8081, "https", 100.0, "us", "elite"),
            make_proxy("192.168.1.3", 8082, "socks4", 100.0, "us", "elite"),
            make_proxy("192.168.1.4", 8083, "http", 100.0, "us", "elite"),
        ];

        let scored = calculate_scores(proxies);

        let socks5 = scored.iter().find(|p| p.proto == "socks5").unwrap();
        let https = scored.iter().find(|p| p.proto == "https").unwrap();
        let _socks4 = scored.iter().find(|p| p.proto == "socks4").unwrap();
        let http = scored.iter().find(|p| p.proto == "http").unwrap();

        // socks5 and https get DNS bonus (1.2x), so should score higher
        assert!(
            socks5.score > http.score,
            "SOCKS5 should score higher than HTTP"
        );
        assert!(
            https.score > http.score,
            "HTTPS should score higher than HTTP"
        );
    }

    #[test]
    fn test_dns_capable_types() {
        // Verify DNS capable types get bonus
        let proxies = vec![
            make_proxy("192.168.1.1", 8080, "https", 100.0, "us", "elite"),
            make_proxy("192.168.1.2", 8081, "socks5", 100.0, "us", "elite"),
            make_proxy("192.168.1.3", 8082, "http", 100.0, "us", "elite"),
        ];

        let scored = calculate_scores(proxies);

        let https = scored.iter().find(|p| p.proto == "https").unwrap();
        let socks5 = scored.iter().find(|p| p.proto == "socks5").unwrap();
        let http = scored.iter().find(|p| p.proto == "http").unwrap();

        // DNS-capable types should have higher scores due to 1.2x bonus
        assert!(https.score > http.score);
        assert!(socks5.score > http.score);
    }

    #[test]
    fn test_weighted_selection_prefers_high_score() {
        // Higher score proxies should appear first after sorting
        let proxies = vec![
            make_proxy("192.168.1.1", 8080, "socks5", 10.0, "us", "elite"), // Best: low latency, elite, preferred country, DNS-capable
            make_proxy("192.168.1.2", 8081, "http", 500.0, "xx", "transparent"), // Worst: high latency, transparent, non-preferred
        ];

        let scored = calculate_scores(proxies);

        // After calculate_scores, proxies are sorted by score descending
        assert!(
            scored[0].score > scored[1].score,
            "Higher score proxy should be first"
        );
        assert_eq!(scored[0].ip, "192.168.1.1", "Best proxy should be first");
    }

    #[test]
    fn test_calculate_scores_empty() {
        let proxies: Vec<Proxy> = vec![];
        let scored = calculate_scores(proxies);
        assert_eq!(scored.len(), 0);
    }

    #[test]
    fn test_calculate_scores_single_proxy() {
        let proxies = vec![make_proxy(
            "192.168.1.1",
            8080,
            "http",
            100.0,
            "us",
            "elite",
        )];
        let scored = calculate_scores(proxies);
        assert_eq!(scored.len(), 1);
        assert!(scored[0].score > 0.0);
    }

    #[test]
    fn test_calculate_scores_zero_latency() {
        // Proxies with zero latency should still get scored
        let proxies = vec![
            make_proxy("192.168.1.1", 8080, "http", 0.0, "us", "elite"),
            make_proxy("192.168.1.2", 8081, "http", 100.0, "us", "elite"),
        ];

        let scored = calculate_scores(proxies);
        assert_eq!(scored.len(), 2);
        // Both should have valid scores
        assert!(scored[0].score >= 0.0);
        assert!(scored[1].score >= 0.0);
    }

    #[test]
    fn test_split_proxy_pools() {
        let proxies = vec![
            make_proxy("192.168.1.1", 8080, "https", 100.0, "us", "elite"),
            make_proxy("192.168.1.2", 8081, "socks5", 100.0, "us", "elite"),
            make_proxy("192.168.1.3", 8082, "http", 100.0, "us", "elite"),
            make_proxy("192.168.1.4", 8083, "socks4", 100.0, "us", "elite"),
        ];

        let (dns, non_dns) = split_proxy_pools(proxies);

        // DNS-capable: https, socks5
        assert_eq!(dns.len(), 2);
        assert!(dns.iter().any(|p| p.proto == "https"));
        assert!(dns.iter().any(|p| p.proto == "socks5"));

        // Non-DNS: http (socks4 is filtered out)
        assert_eq!(non_dns.len(), 1);
        assert!(non_dns.iter().any(|p| p.proto == "http"));
        assert!(!non_dns.iter().any(|p| p.proto == "socks4"));
    }

    #[test]
    fn test_split_proxy_pools_empty() {
        let proxies: Vec<Proxy> = vec![];
        let (dns, non_dns) = split_proxy_pools(proxies);
        assert_eq!(dns.len(), 0);
        assert_eq!(non_dns.len(), 0);
    }

    #[test]
    fn test_case_insensitive_country() {
        // Country matching should be case-insensitive
        let proxies = vec![
            make_proxy("192.168.1.1", 8080, "http", 100.0, "US", "elite"),
            make_proxy("192.168.1.2", 8081, "http", 100.0, "us", "elite"),
            make_proxy("192.168.1.3", 8082, "http", 100.0, "Us", "elite"),
            make_proxy("192.168.1.4", 8083, "http", 100.0, "XX", "elite"),
        ];

        let scored = calculate_scores(proxies);

        // All US variants should have similar scores (preferred country)
        let us_scores: Vec<f64> = scored
            .iter()
            .filter(|p| ["US", "us", "Us"].contains(&p.country.as_str()))
            .map(|p| p.score)
            .collect();

        let xx_score = scored.iter().find(|p| p.country == "XX").unwrap().score;

        // All US variants should score higher than XX
        for score in us_scores {
            assert!(
                score > xx_score,
                "US (any case) should score higher than XX"
            );
        }
    }

    #[test]
    fn test_case_insensitive_anonymity() {
        // Anonymity matching should be case-insensitive
        let proxies = vec![
            make_proxy("192.168.1.1", 8080, "http", 100.0, "us", "ELITE"),
            make_proxy("192.168.1.2", 8081, "http", 100.0, "us", "elite"),
            make_proxy("192.168.1.3", 8082, "http", 100.0, "us", "Elite"),
        ];

        let scored = calculate_scores(proxies);

        // All should have the same anonymity score component
        let scores: Vec<f64> = scored.iter().map(|p| p.score).collect();
        assert_eq!(scores[0], scores[1]);
        assert_eq!(scores[1], scores[2]);
    }

    #[test]
    fn test_case_insensitive_protocol() {
        // Protocol matching should be case-insensitive
        let proxies = vec![
            make_proxy("192.168.1.1", 8080, "HTTPS", 100.0, "us", "elite"),
            make_proxy("192.168.1.2", 8081, "https", 100.0, "us", "elite"),
            make_proxy("192.168.1.3", 8082, "Https", 100.0, "us", "elite"),
        ];

        let scored = calculate_scores(proxies);

        // All should have the same protocol score component (and DNS bonus)
        let scores: Vec<f64> = scored.iter().map(|p| p.score).collect();
        assert_eq!(scores[0], scores[1]);
        assert_eq!(scores[1], scores[2]);
    }

    #[test]
    fn test_latency_normalization_with_all_zero() {
        // When all latencies are zero, max_latency should default to 1.0
        let proxies = vec![
            make_proxy("192.168.1.1", 8080, "http", 0.0, "us", "elite"),
            make_proxy("192.168.1.2", 8081, "http", 0.0, "us", "elite"),
        ];

        let scored = calculate_scores(proxies);
        assert_eq!(scored.len(), 2);
        // Should not panic and should produce valid scores
        for p in &scored {
            assert!(p.score.is_finite());
        }
    }
}
