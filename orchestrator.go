package main

/*
#cgo pkg-config: python3
#cgo LDFLAGS: -L./target/release -Wl,-rpath=./target/release -lrotator_rs -ldl -lm
#include <stdlib.h>

extern char* run_polish_c(const char* raw_json);
extern char* build_chain_decision_c(const char* mode, const char* dns_json, const char* non_dns_json, const char* combined_json);
extern void free_c_string(char* s);
*/
import "C"

import (
	"encoding/json"
	"fmt"
	"log"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"unsafe"
)

// ── ANSI colours ─────────────────────────────────────────────────────────────
const (
	reset  = "\033[0m"
	bold   = "\033[1m"
	cyan   = "\033[36m"
	green  = "\033[32m"
	yellow = "\033[33m"
	red    = "\033[31m"
	dim    = "\033[2m"
)

func col(c, s string) string { return c + s + reset }

// ── Data types ────────────────────────────────────────────────────────────────
type Proxy struct {
	IP        string  `json:"ip"`
	Port      uint16  `json:"port"`
	Proto     string  `json:"type"`
	Latency   float64 `json:"latency"`
	Country   string  `json:"country"`
	Anonymity string  `json:"anonymity"`
	Score     float64 `json:"score"`
}

type PolishResult struct {
	DNS      []Proxy `json:"dns"`
	NonDNS   []Proxy `json:"non_dns"`
	Combined []Proxy `json:"combined"`
}

type ChainHop struct {
	IP      string  `json:"ip"`
	Port    uint16  `json:"port"`
	Proto   string  `json:"proto"`
	Country string  `json:"country"`
	Latency float64 `json:"latency"`
	Score   float64 `json:"score"`
}

type CryptoHop struct {
	KeyHex   string `json:"key_hex"`
	NonceHex string `json:"nonce_hex"`
}

type RotationDecision struct {
	Mode       string      `json:"mode"`
	Timestamp  uint64      `json:"timestamp"`
	ChainID    string      `json:"chain_id"`
	Chain      []ChainHop  `json:"chain"`
	AvgLatency float64     `json:"avg_latency"`
	MinScore   float64     `json:"min_score"`
	MaxScore   float64     `json:"max_score"`
	Encryption []CryptoHop `json:"encryption"`
}

// ── CLI entry point ───────────────────────────────────────────────────────────
func main() {
	if len(os.Args) < 2 {
		printHelp()
		os.Exit(0)
	}

	cmd := os.Args[1]
	args := os.Args[2:]

	workspace, _ := os.Getwd()

	switch cmd {
	case "run":
		mode, limit, protocol := parseRunArgs(args, "phantom", 500, "all")
		cmdRun(workspace, mode, limit, protocol)

	case "refresh":
		mode, limit, protocol := parseRunArgs(args, "phantom", 500, "all")
		cmdRefresh(workspace, mode, limit, protocol)

	case "rotate":
		mode := flagStr(args, "--mode", "phantom")
		cmdRotate(workspace, mode)

	case "stats":
		cmdStats(workspace)

	case "audit":
		cmdAudit()

	case "help", "--help", "-h":
		printHelp()

	default:
		fmt.Printf("%s unknown command: %s\n\n", col(red, "✗"), cmd)
		printHelp()
		os.Exit(1)
	}
}

// ── Commands ──────────────────────────────────────────────────────────────────

// spectre run [--mode phantom|high|stealth|lite] [--limit N] [--protocol all|socks5|https]
// Full pipeline: scrape → polish → rotate → print chain
func cmdRun(workspace, mode string, limit int, protocol string) {
	printBanner()
	fmt.Printf("%s Scraping fresh proxies (limit=%d, protocol=%s)...\n", col(cyan, "◈"), limit, protocol)
	raw, err := runScraper(workspace, limit, protocol)
	if err != nil {
		log.Fatalf("%s %v", col(red, "✗ Scraper:"), err)
	}
	dns, nonDNS, combined, err := runPolish(workspace, raw)
	if err != nil {
		log.Fatalf("%s %v", col(red, "✗ Polish:"), err)
	}
	fmt.Printf("%s Pool: %s total | %s DNS-capable | %s non-DNS\n",
		col(green, "✓"),
		col(bold, fmt.Sprintf("%d", len(combined))),
		col(bold, fmt.Sprintf("%d", len(dns))),
		col(bold, fmt.Sprintf("%d", len(nonDNS))))

	decision, err := buildChainDecision(mode, dns, nonDNS, combined)
	if err != nil || decision == nil {
		log.Fatalf("%s no chain built — pool may be too small for mode %q", col(red, "✗"), mode)
	}
	printChain(decision)
}

// spectre refresh [--mode ...] [--limit N] [--protocol ...]
// Re-verify stored pool → fill delta if needed → rotate
func cmdRefresh(workspace, mode string, limit int, protocol string) {
	printBanner()
	combinedPath := filepath.Join(workspace, "proxies_combined.json")
	if _, err := os.Stat(combinedPath); os.IsNotExist(err) {
		fmt.Printf("%s No stored pool found — running full scrape instead.\n", col(yellow, "⚠"))
		cmdRun(workspace, mode, limit, protocol)
		return
	}
	fmt.Printf("%s Loading stored pool...\n", col(cyan, "◈"))
	stored := loadProxies(combinedPath)
	fmt.Printf("%s Loaded %d stored proxies. Verifying liveness (this takes a moment)...\n", col(cyan, "◈"), len(stored))

	// Verification is done inside the Rust binary (--step refresh) for robustness
	// orchestrator.go triggers the Rust binary with --step refresh
	rustBin := filepath.Join(workspace, "target/release/spectre")
	c := exec.Command(rustBin, "--step", "refresh", "--mode", mode, "--limit", fmt.Sprintf("%d", limit), "--protocol", protocol)
	c.Stdout = os.Stdout
	c.Stderr = os.Stderr
	if err := c.Run(); err != nil {
		log.Fatalf("%s refresh failed: %v", col(red, "✗"), err)
	}
}

// spectre rotate [--mode ...]
// Use existing pool on disk to build a new chain
func cmdRotate(workspace, mode string) {
	printBanner()
	dns, nonDNS, combined := loadPools(workspace)
	if len(combined) == 0 {
		log.Fatalf("%s No proxy pool on disk. Run `spectre run` first.", col(red, "✗"))
	}
	decision, err := buildChainDecision(mode, dns, nonDNS, combined)
	if err != nil || decision == nil {
		log.Fatalf("%s Could not build chain for mode %q — try `spectre run` to refresh the pool.", col(red, "✗"), mode)
	}
	printChain(decision)
}

// spectre stats
// Show pool health without building a chain
func cmdStats(workspace string) {
	dns, nonDNS, combined := loadPools(workspace)
	fmt.Println(col(bold, "\n=== Spectre Pool Stats ==="))
	if len(combined) == 0 {
		fmt.Printf("%s No pool on disk. Run `spectre run` first.\n", col(yellow, "⚠"))
		return
	}
	var sumLat, sumScore float64
	for _, p := range combined {
		sumLat += p.Latency
		sumScore += p.Score
	}
	n := float64(len(combined))
	fmt.Printf("  Total proxies : %s\n", col(bold, fmt.Sprintf("%d", len(combined))))
	fmt.Printf("  DNS-capable   : %s\n", col(green, fmt.Sprintf("%d", len(dns))))
	fmt.Printf("  Non-DNS       : %s\n", fmt.Sprintf("%d", len(nonDNS)))
	fmt.Printf("  Avg latency   : %s\n", fmt.Sprintf("%.3fs", sumLat/n))
	fmt.Printf("  Avg score     : %s\n", fmt.Sprintf("%.3f", sumScore/n))
}

// spectre audit
// Launch the security audit container
func cmdAudit() {
	fmt.Println(col(bold, "\n=== Spectre Security Audit ==="))
	fmt.Printf("%s Building audit image...\n", col(cyan, "◈"))
	build := exec.Command("docker", "build", "-f", "Containerfile.audit", "-t", "spectre-audit", ".")
	build.Stdout = os.Stdout
	build.Stderr = os.Stderr
	if err := build.Run(); err != nil {
		log.Fatalf("%s docker build failed: %v", col(red, "✗"), err)
	}
	fmt.Printf("%s Running security audit...\n\n", col(cyan, "◈"))
	run := exec.Command("docker", "run", "--rm", "spectre-audit")
	run.Stdout = os.Stdout
	run.Stderr = os.Stderr
	if err := run.Run(); err != nil {
		os.Exit(1)
	}
}

// ── Helpers ───────────────────────────────────────────────────────────────────

func printBanner() {
	fmt.Printf("\n%s\n%s\n\n",
		col(bold+cyan, "    ░██████╗██████╗░███████╗░█████╗░████████╗██████╗░███████╗"),
		col(dim, "         Spectre Network — adversarial proxy mesh"),
	)
}

func printHelp() {
	fmt.Printf(`%s

  %s               Full pipeline: scrape → polish → build chain
  %s            Re-verify stored pool, fill gaps, build chain
  %s   [--mode M]   Build chain from stored pool (no scrape)
  %s               Show pool health stats
  %s               Run containerised security audit (needs Docker)

%s
  --mode      phantom | high | stealth | lite   (default: phantom)
  --limit     N proxies to scrape               (default: 500)
  --protocol  all | socks5 | https | http       (default: all)

%s
  spectre run --mode phantom --limit 1000
  spectre refresh --mode high
  spectre rotate --mode stealth
  spectre stats
  spectre audit

%s
  ✓  Multi-hop AES-256-GCM encrypted SOCKS5 tunnel (phantom: 3-5 hops)
  ✓  DNS through chain (SOCKS5h — no local DNS leaks)
  ✓  Pool persistence with health re-verification
  ✓  Randomised chain rotation on every run

%s
  ✗  Traffic shaping / timing obfuscation    → Phase 1 roadmap
  ✗  Protocol morphing (obfs4 / meek)        → Phase 1 roadmap
  ✗  P2P proxy discovery (DHT)               → Phase 2 roadmap

`,
		col(bold, "USAGE:  spectre <command> [flags]"),
		col(cyan+bold, "run"), col(cyan+bold, "refresh"), col(cyan+bold, "rotate"), col(cyan+bold, "stats"), col(cyan+bold, "audit"),
		col(bold, "FLAGS:"),
		col(bold, "EXAMPLES:"),
		col(bold, "WHAT IT DOES:"),
		col(bold, "WHAT IT DOESN'T DO YET:"),
	)
}

func printChain(d *RotationDecision) {
	fmt.Printf("\n%s %s | chain_id: %s\n",
		col(green, "✓ Chain built:"), col(bold, strings.ToUpper(d.Mode)), col(dim, d.ChainID[:12]+"…"))
	for i, h := range d.Chain {
		fmt.Printf("  %s hop %d: %s %-22s %s %s\n",
			col(cyan, "→"), i+1,
			col(bold, h.Proto),
			fmt.Sprintf("%s:%d", h.IP, h.Port),
			col(dim, h.Country),
			col(yellow, fmt.Sprintf("score=%.2f lat=%.3fs", h.Score, h.Latency)))
	}
	fmt.Printf("  %s avg_latency=%.3fs  min_score=%.2f  max_score=%.2f\n\n",
		col(dim, "chain:"), d.AvgLatency, d.MinScore, d.MaxScore)

	data, _ := json.MarshalIndent(d, "", "  ")
	saveJSON("last_chain.json", json.RawMessage(data))
	fmt.Printf("%s Chain saved to %s\n\n", col(dim, "ℹ"), col(bold, "last_chain.json"))
}

// ── Rust bridge ───────────────────────────────────────────────────────────────

func runScraper(workspace string, limit int, protocol string) ([]Proxy, error) {
	scraperPath := filepath.Join(workspace, "go_scraper")
	if _, err := os.Stat(scraperPath); os.IsNotExist(err) {
		return nil, fmt.Errorf("go_scraper binary not found — build with: go build -o go_scraper go_scraper.go")
	}
	cmd := exec.Command(scraperPath, "--limit", fmt.Sprintf("%d", limit), "--protocol", protocol)
	output, err := cmd.CombinedOutput()
	if err != nil {
		return nil, fmt.Errorf("scraper: %v — %s", err, string(output))
	}
	if strings.TrimSpace(string(output)) == "" {
		return []Proxy{}, nil
	}
	_ = os.WriteFile(filepath.Join(workspace, "raw_proxies.json"), output, 0644)
	var proxies []Proxy
	if err := json.Unmarshal(output, &proxies); err != nil {
		return nil, fmt.Errorf("parse scraper output: %v", err)
	}
	return proxies, nil
}

func runPolish(workspace string, proxies []Proxy) (dns, nonDNS, combined []Proxy, err error) {
	proxiesJSON, err := json.Marshal(proxies)
	if err != nil {
		return nil, nil, nil, err
	}
	cRaw := C.CString(string(proxiesJSON))
	defer C.free(unsafe.Pointer(cRaw))

	cOut := C.run_polish_c(cRaw)
	if cOut == nil {
		return nil, nil, nil, fmt.Errorf("rust polish returned null")
	}
	defer C.free_c_string(cOut)

	var result PolishResult
	if err := json.Unmarshal([]byte(C.GoString(cOut)), &result); err != nil {
		return nil, nil, nil, fmt.Errorf("parse polish result: %v", err)
	}

	saveJSON(filepath.Join(workspace, "proxies_dns.json"), result.DNS)
	saveJSON(filepath.Join(workspace, "proxies_non_dns.json"), result.NonDNS)
	saveJSON(filepath.Join(workspace, "proxies_combined.json"), result.Combined)
	return result.DNS, result.NonDNS, result.Combined, nil
}

func buildChainDecision(mode string, dns, nonDNS, combined []Proxy) (*RotationDecision, error) {
	cMode := C.CString(mode)
	defer C.free(unsafe.Pointer(cMode))

	dnsJSON, _ := json.Marshal(dns)
	cDNS := C.CString(string(dnsJSON))
	defer C.free(unsafe.Pointer(cDNS))

	nonDNSJSON, _ := json.Marshal(nonDNS)
	cNonDNS := C.CString(string(nonDNSJSON))
	defer C.free(unsafe.Pointer(cNonDNS))

	combinedJSON, _ := json.Marshal(combined)
	cCombined := C.CString(string(combinedJSON))
	defer C.free(unsafe.Pointer(cCombined))

	cOut := C.build_chain_decision_c(cMode, cDNS, cNonDNS, cCombined)
	if cOut == nil {
		return nil, nil
	}
	defer C.free_c_string(cOut)

	var d RotationDecision
	if err := json.Unmarshal([]byte(C.GoString(cOut)), &d); err != nil {
		return nil, err
	}
	return &d, nil
}

// ── IO helpers ────────────────────────────────────────────────────────────────

func loadProxies(path string) []Proxy {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil
	}
	var p []Proxy
	_ = json.Unmarshal(data, &p)
	return p
}

func saveJSON(path string, v interface{}) {
	data, _ := json.MarshalIndent(v, "", "  ")
	_ = os.WriteFile(path, data, 0644)
}

func loadPools(workspace string) (dns, nonDNS, combined []Proxy) {
	return loadProxies(filepath.Join(workspace, "proxies_dns.json")),
		loadProxies(filepath.Join(workspace, "proxies_non_dns.json")),
		loadProxies(filepath.Join(workspace, "proxies_combined.json"))
}

// ── Flag parsing ──────────────────────────────────────────────────────────────

func flagStr(args []string, name, def string) string {
	for i, a := range args {
		if a == name && i+1 < len(args) {
			return args[i+1]
		}
	}
	return def
}

func flagInt(args []string, name string, def int) int {
	v := flagStr(args, name, "")
	if v == "" {
		return def
	}
	var n int
	fmt.Sscanf(v, "%d", &n)
	if n == 0 {
		return def
	}
	return n
}

func parseRunArgs(args []string, defaultMode string, defaultLimit int, defaultProto string) (mode string, limit int, protocol string) {
	return flagStr(args, "--mode", defaultMode),
		flagInt(args, "--limit", defaultLimit),
		flagStr(args, "--protocol", defaultProto)
}
