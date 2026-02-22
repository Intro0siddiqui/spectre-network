use crate::types::Proxy;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::net::TcpStream;
use tokio::time::timeout;

const MAX_FAIL_COUNT: u32 = 3;
const DEFAULT_TIMEOUT_SECS: u64 = 5;
const MIN_POOL_SIZE: usize = 30;

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Attempt a TCP connection to the proxy's ip:port and measure latency.
/// Returns (alive, latency_secs).
async fn probe_proxy(ip: &str, port: u16, timeout_secs: u64) -> (bool, f64) {
    let addr = format!("{}:{}", ip, port);
    let start = std::time::Instant::now();
    let result = timeout(Duration::from_secs(timeout_secs), TcpStream::connect(&addr)).await;

    match result {
        Ok(Ok(_)) => {
            let elapsed = start.elapsed().as_secs_f64();
            (true, elapsed)
        }
        _ => (false, 0.0),
    }
}

/// Verify all proxies in the pool concurrently.
/// Updates each proxy's alive, latency, fail_count, last_verified.
/// Prunes proxies with fail_count >= MAX_FAIL_COUNT.
/// Returns the surviving, updated proxy list.
pub async fn verify_pool(mut proxies: Vec<Proxy>) -> Vec<Proxy> {
    let timeout_secs = DEFAULT_TIMEOUT_SECS;
    let total = proxies.len();
    log::info!("Re-verifying pool of {} proxies...", total);

    // Run probes concurrently — produce (index, alive, latency)
    let mut handles = Vec::with_capacity(total);
    for (i, proxy) in proxies.iter().enumerate() {
        let ip = proxy.ip.clone();
        let port = proxy.port;
        handles.push(tokio::spawn(async move {
            let (alive, latency) = probe_proxy(&ip, port, timeout_secs).await;
            (i, alive, latency)
        }));
    }

    let mut results = vec![(false, 0.0_f64); total];
    for handle in handles {
        if let Ok((i, alive, latency)) = handle.await {
            results[i] = (alive, latency);
        }
    }

    // Apply results
    let ts = now_unix();
    for (i, proxy) in proxies.iter_mut().enumerate() {
        let (alive, latency) = results[i];
        proxy.last_verified = ts;
        proxy.alive = alive;
        if alive {
            proxy.fail_count = 0;
            // Update latency with recent measurement (weighted average to smooth)
            if proxy.latency > 0.0 {
                proxy.latency = proxy.latency * 0.6 + latency * 0.4;
            } else {
                proxy.latency = latency;
            }
            // Slight score boost for surviving proxies
            proxy.score = (proxy.score * 0.95 + 0.05).min(1.0);
        } else {
            proxy.fail_count += 1;
            // Penalize score on failure
            proxy.score = (proxy.score * 0.7).max(0.0);
        }
    }

    let before = proxies.len();
    proxies.retain(|p| p.fail_count < MAX_FAIL_COUNT);
    let pruned = before - proxies.len();
    let alive_count = proxies.iter().filter(|p| p.alive).count();

    log::info!(
        "Verification done: {}/{} alive, {} pruned",
        alive_count,
        proxies.len(),
        pruned
    );

    proxies
}

/// Returns true if the pool is large enough and fresh enough to skip scraping.
/// "Fresh" means last_verified within stale_secs of now.
pub fn is_pool_healthy(proxies: &[Proxy], stale_secs: u64) -> bool {
    let alive: Vec<&Proxy> = proxies.iter().filter(|p| p.alive).collect();
    if alive.len() < MIN_POOL_SIZE {
        return false;
    }
    let now = now_unix();
    let freshest = alive.iter().map(|p| p.last_verified).max().unwrap_or(0);
    now.saturating_sub(freshest) < stale_secs
}
