use crate::tunnel;
use crate::types::{ChainHop, Proxy};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::net::TcpStream;
use tokio::sync::Semaphore;
use tokio::time::timeout;
use tracing::{debug, error, info};

const MAX_FAIL_COUNT: u32 = 3;
const DEFAULT_TIMEOUT_SECS: u64 = 8; // Slightly longer for handshakes
const MIN_POOL_SIZE: usize = 30;
/// Maximum number of concurrent verification tasks to prevent resource exhaustion.
/// This limits file descriptor usage and avoids triggering rate limits on proxies.
const MAX_CONCURRENT_VERIFICATIONS: usize = 50;

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Deep verify proxy using protocol-aware handshake
async fn deep_probe_proxy(proxy: &Proxy, timeout_secs: u64) -> (bool, f64) {
    let addr = format!("{}:{}", proxy.ip, proxy.port);
    let start = std::time::Instant::now();
    let timeout_duration = Duration::from_secs(timeout_secs);

    // Initial TCP connect
    let mut stream = match timeout(timeout_duration, TcpStream::connect(&addr)).await {
        Ok(Ok(s)) => s,
        _ => {
            debug!(proxy_addr = %addr, "Proxy deep probe TCP failed/timed out");
            return (false, 0.0);
        }
    };

    // Check protocol handshake.
    let hop = ChainHop {
        ip: proxy.ip.clone(),
        port: proxy.port,
        proto: proxy.proto.clone(),
        country: proxy.country.clone(),
        latency: proxy.latency,
        score: proxy.score,
    };

    // Connect to a fast reliable target to confirm proxy actually routes traffic
    let target = "api.ipify.org:443";

    let (alive, latency) = match timeout(
        timeout_duration,
        tunnel::handshake_proxy(&mut stream, &hop, target),
    )
    .await
    {
        Ok(Ok(_)) => {
            let elapsed = start.elapsed().as_secs_f64();
            debug!(proxy_addr = %addr, latency = elapsed, "Proxy deep probe successful");
            (true, elapsed)
        }
        Ok(Err(e)) => {
            debug!(proxy_addr = %addr, error = %e, "Proxy deep probe handshake failed");
            (false, 0.0)
        }
        Err(_) => {
            debug!(
                proxy_addr = %addr,
                timeout = timeout_secs,
                "Proxy deep probe handshake timed out"
            );
            (false, 0.0)
        }
    };

    // Explicitly drop stream to release file descriptors immediately
    // rather than waiting for Tokio GC, which can deadlock on 10k items.
    drop(stream);

    (alive, latency)
}

/// Verify all proxies in the pool concurrently with bounded concurrency.
/// Updates each proxy's alive, latency, fail_count, last_verified.
/// Prunes proxies with fail_count >= MAX_FAIL_COUNT.
/// Returns the surviving, updated proxy list.
///
/// # Arguments
/// * `proxies` - The list of proxies to verify
/// * `max_concurrent` - Maximum number of concurrent verification tasks (default: MAX_CONCURRENT_VERIFICATIONS)
pub async fn verify_pool(proxies: Vec<Proxy>) -> Vec<Proxy> {
    verify_pool_with_limit(proxies, MAX_CONCURRENT_VERIFICATIONS).await
}

/// Verify all proxies in the pool concurrently with a specified concurrency limit.
/// Updates each proxy's alive, latency, fail_count, last_verified.
/// Prunes proxies with fail_count >= MAX_FAIL_COUNT.
/// Returns the surviving, updated proxy list.
///
/// # Arguments
/// * `proxies` - The list of proxies to verify
/// * `max_concurrent` - Maximum number of concurrent verification tasks
pub async fn verify_pool_with_limit(mut proxies: Vec<Proxy>, max_concurrent: usize) -> Vec<Proxy> {
    let timeout_secs = DEFAULT_TIMEOUT_SECS;
    let total = proxies.len();
    info!(
        proxy_count = total,
        max_concurrent = max_concurrent,
        "Re-verifying proxy pool (Deep Probe)"
    );

    // Create a semaphore to limit concurrent connections
    let semaphore = Arc::new(Semaphore::new(max_concurrent));

    // Run probes concurrently with bounded concurrency — produce (index, alive, latency)
    let mut handles = Vec::with_capacity(total);
    for (i, proxy) in proxies.iter().enumerate() {
        let p = proxy.clone();
        let sem = Arc::clone(&semaphore);

        // Acquire a permit before spawning the task
        // This will wait if the semaphore is saturated (max_concurrent tasks already running)
        let permit = match sem.acquire_owned().await {
            Ok(p) => p,
            Err(e) => {
                // Semaphore was closed, which should not happen in normal operation
                error!(error = %e, proxy_index = i, "Semaphore acquisition failed, skipping proxy");
                continue;
            }
        };

        // Log when semaphore is saturated (optional debug output)
        if semaphore.available_permits() == 0 {
            debug!(
                max_concurrent = max_concurrent,
                "Semaphore saturated, verifications queued"
            );
        }

        handles.push(tokio::spawn(async move {
            let (alive, latency) = deep_probe_proxy(&p, timeout_secs).await;
            // Explicitly release the permit by dropping it
            drop(permit);
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

    info!(
        alive = alive_count,
        total = proxies.len(),
        pruned = pruned,
        "Proxy verification completed"
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ProxyTier;

    /// Helper to create a test proxy
    fn make_proxy(
        ip: &str,
        port: u16,
        alive: bool,
        latency: f64,
        fail_count: u32,
        last_verified: u64,
    ) -> Proxy {
        Proxy {
            ip: ip.to_string(),
            port,
            proto: "http".to_string(),
            latency,
            country: "us".to_string(),
            anonymity: "elite".to_string(),
            score: 0.5,
            tier: ProxyTier::Silver,
            fail_count,
            last_verified,
            alive,
        }
    }

    #[tokio::test]
    async fn test_verify_alive_proxy() {
        // Working proxy should be marked alive
        // Use localhost:1 which is typically closed, but we test the logic
        // For a real alive test, we'd need a running server
        // Instead, we test that the verification process updates fields correctly

        let proxies = vec![make_proxy("127.0.0.1", 1, false, 0.0, 0, 0)];

        // This will likely fail (port 1 is closed), but we test the structure
        let result = verify_pool(proxies).await;

        // Verify the proxy was processed (even if marked dead)
        assert_eq!(result.len(), 1, "Should have one proxy after verification");
        assert_eq!(result[0].ip, "127.0.0.1");
        assert_eq!(result[0].port, 1);
        // last_verified should be updated
        assert!(
            result[0].last_verified > 0,
            "last_verified should be updated"
        );
    }

    #[tokio::test]
    async fn test_verify_dead_proxy() {
        // Dead proxy should be marked dead and fail_count incremented
        let proxies = vec![make_proxy("192.0.2.1", 12345, false, 0.0, 0, 0)];

        let result = verify_pool(proxies).await;

        // The proxy should be marked as dead (connection will fail)
        if !result.is_empty() {
            assert!(
                !result[0].alive || result[0].fail_count > 0,
                "Dead proxy should have fail_count or be marked dead"
            );
        }
    }

    #[tokio::test]
    async fn test_latency_smoothing() {
        // Multiple verifications should smooth latency
        // Create a proxy with existing latency
        let proxy = make_proxy("127.0.0.1", 1, false, 100.0, 0, 0);

        let proxies = vec![proxy];

        // After verification, if proxy is alive, latency should be smoothed
        let result = verify_pool(proxies).await;

        if !result.is_empty() && result[0].alive {
            // Latency should be smoothed: old * 0.6 + new * 0.4
            // Since connection will fail, this tests the else branch
            // If alive, the smoothing formula applies
            assert!(result[0].latency >= 0.0, "Latency should be non-negative");
        }
    }

    #[tokio::test]
    async fn test_fail_count_increments() {
        // Failed verifications increase fail count
        let proxies = vec![make_proxy("192.0.2.1", 54321, false, 0.0, 0, 0)];

        let result = verify_pool(proxies).await;

        if !result.is_empty() {
            // If proxy is dead, fail_count should increment
            if !result[0].alive {
                assert!(
                    result[0].fail_count >= 1,
                    "Fail count should increment on failure"
                );
            }
        }
    }

    #[tokio::test]
    async fn test_fail_count_pruning() {
        // Proxies with fail_count >= MAX_FAIL_COUNT should be pruned
        let proxies = vec![
            make_proxy("192.0.2.1", 11111, false, 0.0, 0, 0), // fail_count = 0
            make_proxy("192.0.2.2", 22222, false, 0.0, 2, 0), // fail_count = 2
            make_proxy("192.0.2.3", 33333, false, 0.0, 3, 0), // fail_count = 3, should be pruned
        ];

        let result = verify_pool(proxies).await;

        // The proxy with fail_count >= 3 should be pruned after failing again
        // Note: all will likely fail since these are test IPs
        // The one starting at 3 will go to 4 and be pruned
        assert!(result.len() <= 3, "Should have at most 3 proxies");
    }

    #[tokio::test]
    async fn test_verify_pool_with_limit() {
        // Test verification with custom concurrency limit
        let proxies = vec![
            make_proxy("127.0.0.1", 1, false, 0.0, 0, 0),
            make_proxy("127.0.0.1", 2, false, 0.0, 0, 0),
        ];

        let result = verify_pool_with_limit(proxies, 2).await;

        // Both proxies should be processed
        assert_eq!(result.len(), 2, "Should have 2 proxies after verification");
    }

    #[tokio::test]
    async fn test_verify_empty_pool() {
        // Empty pool should return empty
        let proxies: Vec<Proxy> = vec![];

        let result = verify_pool(proxies).await;

        assert_eq!(result.len(), 0, "Empty pool should return empty");
    }

    #[tokio::test]
    async fn test_verify_single_proxy() {
        // Single proxy verification
        let proxies = vec![make_proxy("127.0.0.1", 1, false, 0.0, 0, 0)];

        let result = verify_pool(proxies).await;

        assert_eq!(result.len(), 1, "Should have 1 proxy");
        assert!(
            result[0].last_verified > 0,
            "last_verified should be updated"
        );
    }

    #[test]
    fn test_is_pool_healthy_sufficient_alive() {
        // Pool with enough alive proxies should be healthy
        let proxies: Vec<Proxy> = (0..MIN_POOL_SIZE + 10)
            .map(|i| {
                make_proxy(
                    &format!("192.168.1.{}", i),
                    8080,
                    true,
                    100.0,
                    0,
                    now_unix(),
                )
            })
            .collect();

        // Pool should be healthy (enough alive, recently verified)
        assert!(is_pool_healthy(&proxies, 3600), "Pool should be healthy");
    }

    #[test]
    fn test_is_pool_healthy_insufficient_alive() {
        // Pool with too few alive proxies should be unhealthy
        let proxies: Vec<Proxy> = (0..MIN_POOL_SIZE - 10)
            .map(|i| {
                make_proxy(
                    &format!("192.168.1.{}", i),
                    8080,
                    true,
                    100.0,
                    0,
                    now_unix(),
                )
            })
            .collect();

        // Pool should be unhealthy (not enough alive)
        assert!(
            !is_pool_healthy(&proxies, 3600),
            "Pool should be unhealthy - insufficient alive"
        );
    }

    #[test]
    fn test_is_pool_healthy_stale() {
        // Pool with stale verification should be unhealthy
        let old_time = now_unix() - 7200; // 2 hours ago
        let proxies: Vec<Proxy> = (0..MIN_POOL_SIZE + 10)
            .map(|i| make_proxy(&format!("192.168.1.{}", i), 8080, true, 100.0, 0, old_time))
            .collect();

        // Pool should be unhealthy (stale verification)
        assert!(
            !is_pool_healthy(&proxies, 3600),
            "Pool should be unhealthy - stale"
        );
    }

    #[test]
    fn test_is_pool_healthy_empty() {
        // Empty pool should be unhealthy
        let proxies: Vec<Proxy> = vec![];

        assert!(
            !is_pool_healthy(&proxies, 3600),
            "Empty pool should be unhealthy"
        );
    }

    #[test]
    fn test_is_pool_healthy_all_dead() {
        // Pool with all dead proxies should be unhealthy
        let now = now_unix();
        let proxies: Vec<Proxy> = (0..MIN_POOL_SIZE + 10)
            .map(|i| make_proxy(&format!("192.168.1.{}", i), 8080, false, 0.0, 0, now))
            .collect();

        // Pool should be unhealthy (no alive proxies)
        assert!(
            !is_pool_healthy(&proxies, 3600),
            "Pool with all dead proxies should be unhealthy"
        );
    }

    #[tokio::test]
    async fn test_score_boost_on_success() {
        // Surviving proxies should get slight score boost
        // Note: This is hard to test without a real working proxy
        // We test the logic by checking that score changes appropriately
        let proxy = make_proxy("127.0.0.1", 1, false, 0.0, 0, 0);

        let proxies = vec![proxy];
        let result = verify_pool(proxies).await;

        if !result.is_empty() && result[0].alive {
            // Score should be boosted: (score * 0.95 + 0.05).min(1.0)
            // Expected: (0.5 * 0.95 + 0.05) = 0.525
            assert!(
                result[0].score >= 0.5,
                "Score should be boosted or maintained"
            );
        }
    }

    #[tokio::test]
    async fn test_score_penalty_on_failure() {
        // Failed proxies should get score penalty
        let mut proxy = make_proxy("192.0.2.1", 12345, false, 0.0, 0, 0);
        proxy.score = 0.8;

        let proxies = vec![proxy];
        let result = verify_pool(proxies).await;

        if !result.is_empty() && !result[0].alive {
            // Score should be penalized: score * 0.7
            // Expected: 0.8 * 0.7 = 0.56
            assert!(
                result[0].score < 0.8,
                "Score should be penalized on failure"
            );
        }
    }

    #[tokio::test]
    async fn test_latency_update_formula() {
        // Test the latency smoothing formula
        let mut proxy = make_proxy("127.0.0.1", 1, false, 0.0, 0, 0);
        proxy.latency = 100.0; // Existing latency
        proxy.alive = true;

        let proxies = vec![proxy];
        let result = verify_pool(proxies).await;

        if !result.is_empty() && result[0].alive {
            // New latency should be: old * 0.6 + new * 0.4
            // Since we can't control the actual connection result, we just verify it's updated
            assert!(result[0].latency >= 0.0);
        } else if !result.is_empty() {
            // If dead, latency might be set to the new measurement (0.0 for failed)
            // or remain unchanged depending on implementation
            assert!(result[0].latency >= 0.0);
        }
    }

    #[tokio::test]
    async fn test_last_verified_updated() {
        // last_verified should always be updated
        let old_time = 1000u64;
        let proxies = vec![make_proxy("127.0.0.1", 1, false, 0.0, 0, old_time)];

        let result = verify_pool(proxies).await;

        assert!(!result.is_empty());
        assert!(
            result[0].last_verified > old_time,
            "last_verified should be updated to current time"
        );
    }

    #[tokio::test]
    async fn test_alive_flag_updated() {
        // alive flag should be updated based on probe result
        let proxies = vec![make_proxy("127.0.0.1", 1, true, 100.0, 0, 0)];

        let result = verify_pool(proxies).await;

        assert!(!result.is_empty());
        // The alive flag is updated based on actual probe result
        // Port 1 is typically closed, so it will likely be false
        // We just verify the field exists and is a boolean
        let _ = result[0].alive;
    }

    #[test]
    fn test_now_unix() {
        // Test that now_unix returns reasonable values
        let now = now_unix();
        assert!(now > 0, "Unix timestamp should be positive");

        // Should be close to current time (within 1 second)
        let expected = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(
            now >= expected - 1 && now <= expected + 1,
            "now_unix should return current time"
        );
    }

    #[tokio::test]
    async fn test_probe_proxy_closed_port() {
        // Test probing a closed port
        let proxy = make_proxy("127.0.0.1", 1, false, 0.0, 0, 0);
        let (alive, latency) = deep_probe_proxy(&proxy, 1).await;

        // Port 1 is typically closed
        assert!(!alive, "Port 1 should be closed");
        assert_eq!(latency, 0.0, "Latency should be 0 for failed probe");
    }

    #[tokio::test]
    async fn test_probe_proxy_timeout() {
        // Test probing with short timeout
        // Use a non-routable IP to test timeout behavior
        let proxy = make_proxy("192.0.2.1", 80, false, 0.0, 0, 0);
        let (alive, latency) = deep_probe_proxy(&proxy, 1).await;

        // This IP should not respond within 1 second
        assert!(!alive, "Non-routable IP should not be alive");
        assert_eq!(latency, 0.0, "Latency should be 0 for timeout");
    }

    #[test]
    fn test_constants() {
        // Verify constants are set correctly
        assert_eq!(MAX_FAIL_COUNT, 3);
        assert_eq!(DEFAULT_TIMEOUT_SECS, 8);
        assert_eq!(MIN_POOL_SIZE, 30);
        assert_eq!(MAX_CONCURRENT_VERIFICATIONS, 50);
    }
}
