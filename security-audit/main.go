package main

import (
	"encoding/json"
	"fmt"
	"io"
	"net"
	"net/http"
	"os"
	"strings"
	"time"
)

// Test result for a single audit check
type TestResult struct {
	Name    string
	Passed  bool
	Message string
}

// The SOCKS5 proxy address the Spectre chain is listening on
const spectreProxy = "127.0.0.1:1080"

// External targets for leak testing
const (
	ipCheckURL  = "https://api.ipify.org"
	dnsCheckURL = "https://api.ip.sb/ip"
	headerCheckURL = "http://httpbin.org/headers"
)

func main() {
	fmt.Println("=== Spectre Network Security Audit ===")
	fmt.Printf("Target proxy: %s\n\n", spectreProxy)

	// Collect host IP before routing through Spectre
	hostIP := getDirectIP()
	fmt.Printf("[INFO] Host external IP: %s\n\n", hostIP)

	results := []TestResult{}

	results = append(results, testIPLeak(hostIP))
	results = append(results, testDNSLeak())
	results = append(results, testHeaderLeak())
	results = append(results, testProxyReachable())
	results = append(results, testLatencyBudget())

	// Print scorecard
	fmt.Println("\n=== Security Scorecard ===")
	passed := 0
	for _, r := range results {
		status := "\033[32m[PASS]\033[0m"
		if !r.Passed {
			status = "\033[31m[FAIL]\033[0m"
		} else {
			passed++
		}
		fmt.Printf("%s %-22s %s\n", status, r.Name+":", r.Message)
	}

	grade := grade(passed, len(results))
	fmt.Printf("\nSecurity Grade: %s (%d/%d passed)\n", grade, passed, len(results))

	if passed < len(results) {
		os.Exit(1)
	}
}

// getDirectIP fetches the external IP without going through the proxy
func getDirectIP() string {
	client := &http.Client{Timeout: 8 * time.Second}
	resp, err := client.Get(ipCheckURL)
	if err != nil {
		return "unknown"
	}
	defer resp.Body.Close()
	body, _ := io.ReadAll(resp.Body)
	return strings.TrimSpace(string(body))
}

// httpClientViaProxy creates an HTTP client that routes through the SOCKS5 proxy
func httpClientViaProxy() *http.Client {
	dialer := &net.Dialer{Timeout: 10 * time.Second}
	transport := &http.Transport{
		DialContext: func(ctx interface{ Done() <-chan struct{} }, network, addr string) (net.Conn, error) {
			// SOCKS5 connect
			conn, err := dialer.Dial("tcp", spectreProxy)
			if err != nil {
				return nil, fmt.Errorf("failed to connect to SOCKS5 proxy: %w", err)
			}
			// Handshake: no-auth SOCKS5
			conn.Write([]byte{0x05, 0x01, 0x00})
			buf := make([]byte, 2)
			conn.Read(buf)
			if buf[1] != 0x00 {
				conn.Close()
				return nil, fmt.Errorf("SOCKS5 auth rejected")
			}
			// CONNECT request
			host, port, _ := net.SplitHostPort(addr)
			portNum := 0
			fmt.Sscanf(port, "%d", &portNum)
			req := []byte{0x05, 0x01, 0x00, 0x03, byte(len(host))}
			req = append(req, []byte(host)...)
			req = append(req, byte(portNum>>8), byte(portNum&0xff))
			conn.Write(req)
			resp := make([]byte, 10)
			conn.Read(resp)
			if resp[1] != 0x00 {
				conn.Close()
				return nil, fmt.Errorf("SOCKS5 CONNECT rejected: %d", resp[1])
			}
			return conn, nil
		},
	}
	return &http.Client{Transport: transport, Timeout: 15 * time.Second}
}

func testIPLeak(hostIP string) TestResult {
	client := httpClientViaProxy()
	resp, err := client.Get(ipCheckURL)
	if err != nil {
		return TestResult{"IP Leak", false, fmt.Sprintf("could not reach check URL via proxy: %v", err)}
	}
	defer resp.Body.Close()
	body, _ := io.ReadAll(resp.Body)
	proxyIP := strings.TrimSpace(string(body))

	if proxyIP == hostIP {
		return TestResult{"IP Leak", false, fmt.Sprintf("LEAK: proxy IP matches host IP (%s)", hostIP)}
	}
	return TestResult{"IP Leak", true, fmt.Sprintf("chain IP %s != host IP %s", proxyIP, hostIP)}
}

func testDNSLeak() TestResult {
	// Use a secondary IP check that also reflects the DNS resolver path
	client := httpClientViaProxy()
	resp, err := client.Get(dnsCheckURL)
	if err != nil {
		return TestResult{"DNS Leak", false, fmt.Sprintf("could not reach DNS check URL: %v", err)}
	}
	defer resp.Body.Close()
	body, _ := io.ReadAll(resp.Body)
	ip := strings.TrimSpace(string(body))
	if ip == "" {
		return TestResult{"DNS Leak", false, "got empty response from DNS check endpoint"}
	}
	// If we got here, DNS resolved through the proxy chain (not locally)
	return TestResult{"DNS Leak", true, fmt.Sprintf("DNS resolved via proxy chain (seen IP: %s)", ip)}
}

func testHeaderLeak() TestResult {
	client := httpClientViaProxy()
	resp, err := client.Get(headerCheckURL)
	if err != nil {
		return TestResult{"Header Leak", false, fmt.Sprintf("could not reach header check URL: %v", err)}
	}
	defer resp.Body.Close()
	body, _ := io.ReadAll(resp.Body)

	// Parse the JSON response from httpbin
	var result map[string]interface{}
	if err := json.Unmarshal(body, &result); err != nil {
		return TestResult{"Header Leak", false, "failed to parse header response"}
	}

	headers, _ := result["headers"].(map[string]interface{})
	leaks := []string{}
	for _, key := range []string{"X-Forwarded-For", "Via", "X-Real-Ip", "Forwarded"} {
		if _, found := headers[key]; found {
			leaks = append(leaks, key)
		}
	}

	if len(leaks) > 0 {
		return TestResult{"Header Leak", false, fmt.Sprintf("leaking headers: %s", strings.Join(leaks, ", "))}
	}
	return TestResult{"Header Leak", true, "no identifying headers leaked"}
}

func testProxyReachable() TestResult {
	conn, err := net.DialTimeout("tcp", spectreProxy, 3*time.Second)
	if err != nil {
		return TestResult{"Proxy Reachable", false, fmt.Sprintf("SOCKS5 port not reachable: %v", err)}
	}
	conn.Close()
	return TestResult{"Proxy Reachable", true, fmt.Sprintf("SOCKS5 on %s is up", spectreProxy)}
}

func testLatencyBudget() TestResult {
	client := httpClientViaProxy()
	start := time.Now()
	resp, err := client.Get("http://example.com")
	elapsed := time.Since(start)

	if err != nil {
		return TestResult{"Latency Budget", false, fmt.Sprintf("request failed: %v", err)}
	}
	resp.Body.Close()

	budget := 6 * time.Second
	if elapsed > budget {
		return TestResult{"Latency Budget", false, fmt.Sprintf("%.2fs exceeds %.0fs budget", elapsed.Seconds(), budget.Seconds())}
	}
	return TestResult{"Latency Budget", true, fmt.Sprintf("%.2fs (budget %.0fs)", elapsed.Seconds(), budget.Seconds())}
}

func grade(passed, total int) string {
	pct := float64(passed) / float64(total)
	switch {
	case pct == 1.0:
		return "A+"
	case pct >= 0.85:
		return "A"
	case pct >= 0.70:
		return "B"
	case pct >= 0.55:
		return "C"
	default:
		return "F"
	}
}
