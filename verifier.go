package main

import (
	"fmt"
	"math"
	"net"
	"spectre-network/internal/pool"
	"time"
)

const (
	MaxFailCount               = 3
	DefaultVerifyTimeout       = 8 * time.Second
	MinPoolSize                = 30
	MaxConcurrentVerifications = 50
)

func nowUnix() uint64 {
	return uint64(time.Now().Unix())
}

// internalVerifyProxy performs a TCP connection test and updates proxy metrics.
// This ports the logic from Rust's deep_probe_proxy (TCP part) and verify_pool.
func internalVerifyProxy(p *Proxy, timeout time.Duration) {
	addr := fmt.Sprintf("%s:%d", p.IP, p.Port)
	start := time.Now()
	
	conn, err := net.DialTimeout("tcp", addr, timeout)
	latency := time.Since(start).Seconds()
	
	p.LastVerified = nowUnix()
	
	if err != nil {
		p.Alive = false
		p.FailCount++
		// Penalize score on failure: Reduce score by 30% on each failed attempt.
		// Proxies with FailCount >= MaxFailCount are pruned from the pool.
		p.Score = math.Max(p.Score*0.7, 0.0)
		return
	}
	conn.Close()
	
	p.Alive = true
	p.FailCount = 0
	
	// Update latency with recent measurement (weighted average to smooth: 0.6 old + 0.4 new)
	// This prevents a single outlier measurement from radically changing the proxy's rank.
	if p.Latency > 0 {
		p.Latency = p.Latency*0.6 + latency*0.4
	} else {
		p.Latency = latency
	}
	
	// Slight score boost for surviving proxies: Incremental improvement for stable proxies.
	p.Score = math.Min(p.Score*0.95+0.05, 1.0)
}

// internalVerifyPool verifies a slice of proxies concurrently with bounded concurrency.
func internalVerifyPool(proxies []Proxy, maxConcurrent int) []Proxy {
	if maxConcurrent <= 0 {
		maxConcurrent = MaxConcurrentVerifications
	}
	
	p := pool.NewPool(maxConcurrent)
	
	// Results will be updated in-place on the slice elements
	for i := range proxies {
		idx := i
		p.Submit(func() error {
			internalVerifyProxy(&proxies[idx], DefaultVerifyTimeout)
			return nil
		})
	}
	
	p.Wait()
	
	// Prune proxies with fail_count >= MaxFailCount
	survivors := []Proxy{}
	for _, p := range proxies {
		if p.FailCount < MaxFailCount {
			survivors = append(survivors, p)
		}
	}
	
	return survivors
}
