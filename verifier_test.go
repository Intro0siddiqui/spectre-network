package main

import (
	"math"
	"net"
	"testing"
	"time"
)

func TestInternalVerifyProxy(t *testing.T) {
	// Test with a dead proxy (unlikely to have anything on port 1)
	p := &Proxy{
		IP:   "127.0.0.1",
		Port: 1,
	}
	
	internalVerifyProxy(p, 100*time.Millisecond)
	
	if p.Alive {
		t.Errorf("Proxy on port 1 should be dead")
	}
	
	if p.FailCount == 0 {
		t.Errorf("FailCount should be incremented for dead proxy")
	}
	
	if p.LastVerified == 0 {
		t.Errorf("LastVerified should be updated")
	}
}

func TestLatencySmoothing(t *testing.T) {
	// I'll create a local listener.
	l, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer l.Close()
	
	addr := l.Addr().(*net.TCPAddr)
	
	p := &Proxy{
		IP:      addr.IP.String(),
		Port:    uint16(addr.Port),
		Latency: 1.0, // Existing latency
		Score:   0.5,
	}
	
	internalVerifyProxy(p, 100*time.Millisecond)
	
	if !p.Alive {
		t.Errorf("Proxy should be alive")
	}
	
	// New latency should be around (1.0 * 0.6 + ~0.0 * 0.4) = ~0.6
	if p.Latency >= 1.0 {
		t.Errorf("Latency should have been smoothed down, got %f", p.Latency)
	}
	
	if p.Score <= 0.5 {
		t.Errorf("Score should have been boosted, got %f", p.Score)
	}
}

func TestScorePenalty(t *testing.T) {
	p := &Proxy{
		IP:    "192.0.2.1", // Non-routable
		Port:  1,
		Score: 0.8,
	}
	
	internalVerifyProxy(p, 10*time.Millisecond)
	
	if p.Alive {
		t.Errorf("Proxy should be dead")
	}
	
	expectedScore := 0.8 * 0.7
	if math.Abs(p.Score-expectedScore) > 0.001 {
		t.Errorf("Score should have been penalized to %f, got %f", expectedScore, p.Score)
	}
}

func TestInternalVerifyPool(t *testing.T) {
	proxies := []Proxy{
		{IP: "127.0.0.1", Port: 1},
		{IP: "127.0.0.1", Port: 2},
	}
	
	survivors := internalVerifyPool(proxies, 2)
	
	if len(survivors) != 2 {
		t.Errorf("Expected 2 survivors, got %d", len(survivors))
	}
	
	// If we set fail_count to 2, it should be pruned.
	proxies[0].FailCount = 2
	survivors = internalVerifyPool(proxies, 2)
	
	if len(survivors) != 1 {
		t.Errorf("Expected 1 survivor after pruning, got %d", len(survivors))
	}
}
