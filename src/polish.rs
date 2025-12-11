use crate::types::Proxy;
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
    }

    // Sort descending
    proxies.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    proxies
}

pub fn split_proxy_pools(proxies: Vec<Proxy>) -> (Vec<Proxy>, Vec<Proxy>) {
    let mut dns = Vec::new();
    let mut non_dns = Vec::new();

    for p in proxies {
        if DNS_CAPABLE_TYPES.contains(p.proto.to_lowercase().as_str()) {
            dns.push(p);
        } else {
            non_dns.push(p);
        }
    }
    (dns, non_dns)
}
