package main

/*
#cgo LDFLAGS: -L./target/release -lrotator_rs -ldl -lm -lpthread
#include <stdlib.h>

// run_polish_c: Scores and tiers proxies based on weighted metrics.
extern char* run_polish_c(const char* raw_json, const char* weights_json);

// build_chain_decision_c: Selects proxies and generates a multi-hop chain with encryption keys.
extern char* build_chain_decision_c(const char* mode, const char* dns_json, const char* non_dns_json, const char* combined_json);

// build_chain_topology_c: Same as above but returns only network info (no keys).
extern char* build_chain_topology_c(const char* mode, const char* dns_json, const char* non_dns_json, const char* combined_json);

// derive_keys_from_secret_c: Derives per-hop AES keys from a master secret.
extern char* derive_keys_from_secret_c(const char* master_secret, const char* chain_id, int num_hops);

// encrypt_with_counter_c: Single-layer AES-256-GCM encryption.
extern unsigned char* encrypt_with_counter_c(const char* key_hex, const char* nonce_hex, unsigned long long counter, const unsigned char* plaintext, size_t plaintext_len, size_t* out_len);

// decrypt_with_counter_c: Single-layer AES-256-GCM decryption.
extern unsigned char* decrypt_with_counter_c(const char* key_hex, const char* nonce_hex, unsigned long long counter, const unsigned char* ciphertext, size_t ciphertext_len, size_t* out_len);

// FFI Cleanup helpers
extern void free_byte_array(unsigned char* ptr, size_t len);
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
	"strconv"
	"strings"
	"unsafe"

	"gopkg.in/yaml.v3"
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
// Proxy represents a single proxy candidate with its metrics.
type Proxy struct {
	IP           string  `json:"ip,omitempty"`
	Port         uint16  `json:"port,omitempty"`
	Proto        string  `json:"type,omitempty"`
	Latency      float64 `json:"latency,omitempty"`
	Country      string  `json:"country,omitempty"`
	Anonymity    string  `json:"anonymity,omitempty"`
	Score        float64 `json:"score,omitempty"`
	Tier         string  `json:"tier"` // Assigned by Rust polish - preserved to identify quality levels (Platinum, Gold, etc.)
	FailCount    uint32  `json:"fail_count"`
	LastVerified uint64  `json:"last_verified"`
	Alive        bool    `json:"alive"`
	SourceType   string  `json:"source_type"` // "standard" or "premium"
}

// ScoringWeights defines the priority of various proxy attributes during scoring.
type ScoringWeights struct {
	Latency   float64 `json:"latency"`
	Anonymity float64 `json:"anonymity"`
	Country   float64 `json:"country"`
	Protocol  float64 `json:"protocol"`
	Premium   float64 `json:"premium"`
}

func defaultWeights() ScoringWeights {
	return ScoringWeights{
		Latency:   0.4,
		Anonymity: 0.3,
		Country:   0.2,
		Protocol:  0.1,
		Premium:   0.5,
	}
}

type PolishResult struct {
	DNS      []Proxy `json:"dns"`
	NonDNS   []Proxy `json:"non_dns"`
	Combined []Proxy `json:"combined"`
}

type SignatureProfile struct {
	JA3  string `json:"ja3" yaml:"ja3"`
	JA4  string `json:"ja4,omitempty" yaml:"ja4,omitempty"`
	ALPN string `json:"alpn" yaml:"alpn"`
}

type SignatureConfig struct {
	Profiles map[string]SignatureProfile `json:"profiles" yaml:"profiles"`
}

// MimicConfig defines how the tunnel should disguise its handshakes.
type MimicConfig struct {
	Protocol    string `json:"protocol" yaml:"protocol"` // "https", "quic", "ssh"
	Fingerprint string `json:"fingerprint" yaml:"fingerprint"` // "chrome", "firefox", "edge", etc.
}

// ChainHop is a single link in the multi-hop proxy chain.
type ChainHop struct {
	IP          string             `json:"ip"`
	Port        uint16             `json:"port"`
	Proto       string             `json:"proto"`
	Country     string             `json:"country"`
	Latency     float64            `json:"latency"`
	Score       float64            `json:"score"`
	Obfuscation *ObfuscationConfig `json:"obfuscation,omitempty"`
	Mimic       *MimicConfig       `json:"mimic,omitempty"`
}

type ObfuscationConfig struct {
	Mode         string `json:"mode" yaml:"mode"`                   // "off", "simple", "advanced", "obfs4"
	JitterRange  int    `json:"jitter_range" yaml:"jitter_range"`   // ms
	PaddingRange [2]int `json:"padding_range" yaml:"padding_range"` // [min, max] bytes
	NodeID       string `json:"node_id,omitempty" yaml:"node_id,omitempty"`
	PublicKey    string `json:"public_key,omitempty" yaml:"public_key,omitempty"`
	Cert         string `json:"cert,omitempty" yaml:"cert,omitempty"`
	IATMode      int    `json:"iat_mode,omitempty" yaml:"iat_mode,omitempty"`
}

type CryptoHop struct {
	KeyHex   string `json:"key_hex"`
	NonceHex string `json:"nonce_hex"`
}

// RotationDecision contains the complete blueprint for a proxy chain session.
type RotationDecision struct {
	Mode       string      `json:"mode"`
	Timestamp  uint64      `json:"timestamp"`
	ChainID    string      `json:"chain_id"`
	Chain      []ChainHop  `json:"chain"`
	AvgLatency float64     `json:"avg_latency"`
	MinScore   float64     `json:"min_score"`
	MaxScore   float64     `json:"max_score"`
	Encryption []CryptoHop `json:"encryption"` // Contains AES-256-GCM keys; kept in memory only.
	Garlic     bool        `json:"garlic"`
}

// ChainTopology contains only the chain structure without cryptographic material.
// This struct is safe to persist to disk as it excludes encryption keys and nonces.
// SECURITY: Using this for last_chain.json prevents plaintext key storage.
type ChainTopology struct {
	ChainID    string    `json:"chain_id"`
	Hops       []HopInfo `json:"hops"`
	CreatedAt  uint64    `json:"created_at"`
	Mode       string    `json:"mode"`
	AvgLatency float64   `json:"avg_latency"`
	MinScore   float64   `json:"min_score"`
	MaxScore   float64   `json:"max_score"`
}

// HopInfo contains only the network topology information for a chain hop.
// Excludes all cryptographic material (keys, nonces, country, latency, score).
type HopInfo struct {
	IP   string `json:"ip"`
	Port uint16 `json:"port"`
	Type string `json:"type"`
}

// toChainTopology converts a RotationDecision to ChainTopology, stripping all encryption keys.
// This is the safe version to persist to disk.
func (d *RotationDecision) toChainTopology() ChainTopology {
	hops := make([]HopInfo, len(d.Chain))
	for i, h := range d.Chain {
		hops[i] = HopInfo{
			IP:   h.IP,
			Port: h.Port,
			Type: h.Proto,
		}
	}
	return ChainTopology{
		ChainID:    d.ChainID,
		Hops:       hops,
		CreatedAt:  d.Timestamp,
		Mode:       d.Mode,
		AvgLatency: d.AvgLatency,
		MinScore:   d.MinScore,
		MaxScore:   d.MaxScore,
	}
}

// ── Input validation ──────────────────────────────────────────────────────────

// validateMode checks if the mode parameter is one of the allowed values
func validateMode(mode string) bool {
	validModes := map[string]bool{
		"lite":    true,
		"stealth": true,
		"high":    true,
		"phantom": true,
	}
	return validModes[mode]
}

// validateLimit checks if the limit parameter is within acceptable bounds
// Prevents resource exhaustion from excessively large values
func validateLimit(limit int) bool {
	return limit > 0 && limit <= 10000
}

// validateProtocol checks if the protocol parameter is valid
func validateProtocol(protocol string) bool {
	validProtocols := map[string]bool{
		"all":    true,
		"socks5": true,
		"https":  true,
		"http":   true,
	}
	return validProtocols[protocol]
}

// sanitizeMode normalizes and validates the mode string
// Returns the normalized mode and a boolean indicating validity
func sanitizeMode(mode string) (string, bool) {
	normalized := strings.ToLower(strings.TrimSpace(mode))
	if !validateMode(normalized) {
		return "", false
	}
	return normalized, true
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
		mode, limit, protocol, garlic, obfuscation, mimic, vpnConfig, vpnPos := parseRunArgs(args, "phantom", 500, "all")
		weights := parseWeightArgs(args)
		// Validate inputs before proceeding
		if sanitizedMode, ok := sanitizeMode(mode); !ok {
			fmt.Printf("%s Invalid mode: %s. Allowed: lite, stealth, high, phantom\n", col(red, "✗"), mode)
			os.Exit(1)
		} else {
			mode = sanitizedMode
		}
		if !validateLimit(limit) {
			fmt.Printf("%s Invalid limit: %d. Must be between 1 and 10000\n", col(red, "✗"), limit)
			os.Exit(1)
		}
		if !validateProtocol(protocol) {
			fmt.Printf("%s Invalid protocol: %s. Allowed: all, socks5, https, http\n", col(red, "✗"), protocol)
			os.Exit(1)
		}
		cmdRun(workspace, mode, limit, protocol, weights, garlic, obfuscation, mimic, vpnConfig, vpnPos)

	case "refresh":
		mode, limit, protocol, garlic, obfuscation, mimic, vpnConfig, vpnPos := parseRunArgs(args, "phantom", 500, "all")
		weights := parseWeightArgs(args)
		// Validate inputs before proceeding
		if sanitizedMode, ok := sanitizeMode(mode); !ok {
			fmt.Printf("%s Invalid mode: %s. Allowed: lite, stealth, high, phantom\n", col(red, "✗"), mode)
			os.Exit(1)
		} else {
			mode = sanitizedMode
		}
		if !validateLimit(limit) {
			fmt.Printf("%s Invalid limit: %d. Must be between 1 and 10000\n", col(red, "✗"), limit)
			os.Exit(1)
		}
		if !validateProtocol(protocol) {
			fmt.Printf("%s Invalid protocol: %s. Allowed: all, socks5, https, http\n", col(red, "✗"), protocol)
			os.Exit(1)
		}
		cmdRefresh(workspace, mode, limit, protocol, weights, garlic, obfuscation, mimic, vpnConfig, vpnPos)

	case "rotate":
		mode, _, _, garlic, obfuscation, mimic, vpnConfig, vpnPos := parseRunArgs(args, "phantom", 0, "")
		// Validate mode before proceeding
		if sanitizedMode, ok := sanitizeMode(mode); !ok {
			fmt.Printf("%s Invalid mode: %s. Allowed: lite, stealth, high, phantom\n", col(red, "✗"), mode)
			os.Exit(1)
		} else {
			mode = sanitizedMode
		}
		cmdRotate(workspace, mode, garlic, obfuscation, mimic, vpnConfig, vpnPos)

	case "stats":
		cmdStats(workspace)

	case "add":
		ip := flagStr(args, "--ip", "")
		port := flagInt(args, "--port", 0)
		proto := flagStr(args, "--proto", "socks5")
		country := flagStr(args, "--country", "xx")
		anonymity := flagStr(args, "--anonymity", "elite")
		
		if ip == "" || port == 0 {
			fmt.Printf("%s IP and Port are required. Usage: spectre add --ip IP --port PORT [--proto PROTO] [--country CC] [--anonymity ANON]\n", col(red, "✗"))
			os.Exit(1)
		}
		cmdAdd(workspace, ip, uint16(port), proto, country, anonymity)

	case "audit":
		cmdAudit()

	case "serve":
		mode, _, _, garlic, obfuscation, mimic, vpnConfig, vpnPos := parseRunArgs(args, "phantom", 0, "")
		portStr := flagStr(args, "--port", "1080")
		port, _ := strconv.Atoi(portStr)
		if sanitizedMode, ok := sanitizeMode(mode); !ok {
			fmt.Printf("%s Invalid mode: %s. Allowed: lite, stealth, high, phantom\n", col(red, "✗"), mode)
			os.Exit(1)
		} else {
			mode = sanitizedMode
		}
		cmdServe(workspace, mode, port, garlic, obfuscation, mimic, vpnConfig, vpnPos)

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
func cmdRun(workspace, mode string, limit int, protocol string, weights ScoringWeights, garlic bool, obfuscation *ObfuscationConfig, mimic *MimicConfig, vpnConfig, vpnPos string) {
	printBanner()
	fmt.Printf("%s Scraping fresh proxies (limit=%d, protocol=%s)...\n", col(cyan, "◈"), limit, protocol)
	raw, err := runScraper(workspace, limit, protocol)
	if err != nil {
		log.Fatalf("%s %v", col(red, "✗ Scraper:"), err)
	}
	dns, nonDNS, combined, err := runPolish(workspace, raw, weights)
	if err != nil {
		log.Fatalf("%s %v", col(red, "✗ Polish:"), err)
	}
	fmt.Printf("%s Pool: %s total | %s DNS-capable | %s non-DNS\n",
		col(green, "✓"),
		col(bold, fmt.Sprintf("%d", len(combined))),
		col(bold, fmt.Sprintf("%d", len(dns))),
		col(bold, fmt.Sprintf("%d", len(nonDNS))))

	decision, err := buildChainDecision(mode, dns, nonDNS, combined, garlic, obfuscation, mimic)
	if err != nil || decision == nil {
		log.Fatalf("%s no chain built — pool may be too small for mode %q", col(red, "✗"), mode)
	}
	printChain(decision)
}

// spectre refresh [--mode ...] [--limit N] [--protocol ...]
// Re-verify stored pool → fill delta if needed → rotate
func cmdRefresh(workspace, mode string, limit int, protocol string, weights ScoringWeights, garlic bool, obfuscation *ObfuscationConfig, mimic *MimicConfig, vpnConfig, vpnPos string) {
	printBanner()
	combinedPath := filepath.Join(workspace, "proxies_combined.json")
	if _, err := os.Stat(combinedPath); os.IsNotExist(err) {
		fmt.Printf("%s No stored pool found — running full scrape instead.\n", col(yellow, "⚠"))
		cmdRun(workspace, mode, limit, protocol, weights, garlic, obfuscation, mimic, vpnConfig, vpnPos)
		return
	}
	fmt.Printf("%s Loading stored pool...\n", col(cyan, "◈"))
	stored := loadProxies(combinedPath)
	fmt.Printf("%s Loaded %d stored proxies. Verifying liveness (this takes a moment)...\n", col(cyan, "◈"), len(stored))

	dns, nonDNS, combined, err := runVerify(workspace, stored, weights)
	if err != nil {
		log.Fatalf("%s Verify failed: %v", col(red, "✗"), err)
	}

	fmt.Printf("%s Pool: %s total | %s DNS-capable | %s non-DNS\n",
		col(green, "✓"),
		col(bold, fmt.Sprintf("%d", len(combined))),
		col(bold, fmt.Sprintf("%d", len(dns))),
		col(bold, fmt.Sprintf("%d", len(nonDNS))))

	decision, err := buildChainDecision(mode, dns, nonDNS, combined, garlic, obfuscation, mimic)
	if err != nil || decision == nil {
		log.Fatalf("%s Could not rebuild chain for mode %q", col(red, "✗"), mode)
	}
	printChain(decision)
}

func runVerify(workspace string, proxies []Proxy, weights ScoringWeights) (dns, nonDNS, combined []Proxy, err error) {
	fmt.Printf("  %s Verifying pool of %d proxies...\n", col(dim, "→"), len(proxies))
	verified := internalVerifyPool(proxies, MaxConcurrentVerifications)
	// Re-run polish on verified proxies to update pools and scores
	return runPolish(workspace, verified, weights)
}

// spectre rotate [--mode ...]
// Use existing pool on disk to build a new chain
func cmdRotate(workspace, mode string, garlic bool, obfuscation *ObfuscationConfig, mimic *MimicConfig, vpnConfig, vpnPos string) {
	printBanner()
	dns, nonDNS, combined := loadPools(workspace)
	if len(combined) == 0 {
		log.Fatalf("%s No proxy pool on disk. Run `spectre run` first.", col(red, "✗"))
	}
	decision, err := buildChainDecision(mode, dns, nonDNS, combined, garlic, obfuscation, mimic)
	if err != nil || decision == nil {
		log.Fatalf("%s Could not build chain for mode %q — try `spectre run` to refresh the pool.", col(red, "✗"), mode)
	}
	printChain(decision)
}

// spectre serve [--mode M] [--port P]
func cmdServe(workspace, mode string, port int, garlic bool, obfuscation *ObfuscationConfig, mimic *MimicConfig, vpnConfig, vpnPos string) {
	printBanner()
	dns, nonDNS, combined := loadPools(workspace)
	if len(combined) == 0 {
		log.Fatalf("%s No proxy pool on disk. Run `spectre run` first.", col(red, "✗"))
	}
	decision, err := buildChainDecision(mode, dns, nonDNS, combined, garlic, obfuscation, mimic)
	if err != nil || decision == nil {
		log.Fatalf("%s Could not build chain for mode %q", col(red, "✗"), mode)
	}
	printChain(decision)

	fmt.Printf("%s Starting SOCKS5 server on port %d with live rotation...\n", col(green, "✓"), port)

	if err := startSOCKS5Server(port, *decision, dns, nonDNS, combined, obfuscation, mimic); err != nil {
		log.Fatalf("%s Server failed: %v", col(red, "✗"), err)
	}
}

// spectre add --ip ... --port ... --proto ...
func cmdAdd(workspace, ip string, port uint16, proto, country, anonymity string) {
	premiumPath := filepath.Join(workspace, "premium_proxies.json")
	premium := loadProxies(premiumPath)
	
	newProxy := Proxy{
		IP:         ip,
		Port:       port,
		Proto:      proto,
		Country:    country,
		Anonymity:  anonymity,
		SourceType: "premium",
		Alive:      true,
	}
	
	// Avoid duplicates in premium_proxies.json
	exists := false
	for i, p := range premium {
		if p.IP == ip && p.Port == port {
			premium[i] = newProxy
			exists = true
			break
		}
	}
	if !exists {
		premium = append(premium, newProxy)
	}
	
	saveJSON(premiumPath, premium)
	fmt.Printf("%s Added premium proxy: %s:%d [%s] (%s)\n", col(green, "✓"), ip, port, proto, country)
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
// Runs a self-contained Podman security probe:
//  1. Starts the SOCKS5 chain inside the container
//  2. Runs spectre-audit to test IP/DNS/header/IPv6/TLS leaks
//  3. Prints a security scorecard (A+ → F)
func cmdAudit() {
	fmt.Println(col(bold, "\n=== Spectre Security Audit ==="))

	// Auto-build spectre-audit probe binary if missing
	if _, err := os.Stat("spectre-audit"); os.IsNotExist(err) {
		fmt.Printf("%s Building spectre-audit probe...\n", col(cyan, "◈"))
		auditBuild := exec.Command("go", "build", "-o", "../spectre-audit", ".")
		auditBuild.Dir = "./security-audit/"
		auditBuild.Stdout = os.Stdout
		auditBuild.Stderr = os.Stderr
		if err := auditBuild.Run(); err != nil {
			log.Fatalf("%s Failed to build spectre-audit: %v", col(red, "✗"), err)
		}
	}

	fmt.Printf("%s Building audit image with Podman...\n", col(cyan, "◈"))
	build := exec.Command("podman", "build", "-f", "Containerfile.audit", "-t", "spectre-audit", ".")
	build.Stdout = os.Stdout
	build.Stderr = os.Stderr
	if err := build.Run(); err != nil {
		log.Fatalf("%s podman build failed: %v", col(red, "✗"), err)
	}

	fmt.Printf("%s Running security probe...\n\n", col(cyan, "◈"))
	run := exec.Command("podman", "run", "--rm", "spectre-audit")
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
  %s  [--mode M]            Build chain from stored pool (no scrape)
  spectre serve   [--mode M] [--port P]  Start SOCKS5 proxy server (default port: 1080)
  spectre add     --ip IP --port PORT    Add a premium manual proxy
  %s                          Show pool health stats
  %s                          Run containerised security audit (needs Podman)

%s
  --mode      phantom | high | stealth | lite   (default: phantom)
  --limit     N proxies to scrape               (default: 500)
  --protocol  all | socks5 | https | http       (default: all)
  --garlic    Enable multi-hop chaffing/padding
  --obfuscation-mode    off | simple | advanced
  --jitter-range        N (ms)
  --padding-range       MIN-MAX (bytes)
  --obfuscation-config  path/to/yaml
  --mimic-protocol      https | quic | ssh
  --mimic-fingerprint   chrome | firefox | edge | youtube (default: chrome)
  --signatures-config   path/to/yaml (default: signatures.yaml)
  --vpn-config          path/to/wg.conf
  --vpn-position        entry | intermediate | exit | any (default: any)

%s
  spectre run --mode phantom --limit 1000
  spectre refresh --mode high
  spectre rotate --mode stealth
  spectre stats
  spectre audit

%s
  ✓  Multi-hop AES-256-GCM encrypted SOCKS5 tunnel (phantom: 3-5 hops)
  ✓  DNS through chain — no local DNS leaks
  ✓  Pool persistence with health re-verification
  ✓  Randomised chain rotation on every run

`,
		col(bold, "USAGE:  spectre <command> [flags]"),
		col(cyan+bold, "run"), col(cyan+bold, "refresh"),
		col(cyan+bold, "rotate"), col(cyan+bold, "stats"), col(cyan+bold, "audit"),
		col(bold, "FLAGS:"),
		col(bold, "EXAMPLES:"),
		col(bold, "FEATURES:"),
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

	// SECURITY: Save only chain topology to disk, NOT the encryption keys.
	// Keys remain only in memory for the duration of this session.
	// This prevents anyone with file access from retroactively decrypting traffic.
	topology := d.toChainTopology()
	data, _ := json.MarshalIndent(topology, "", "  ")
	saveJSON("last_chain.json", json.RawMessage(data))
	fmt.Printf("%s Chain topology saved to %s (encryption keys kept in memory only)\n\n", col(dim, "ℹ"), col(bold, "last_chain.json"))
}

// ── Rust bridge ───────────────────────────────────────────────────────────────

func runScraper(workspace string, limit int, protocol string) ([]Proxy, error) {
	proxies := internalRunScraper(limit, protocol)

	if len(proxies) == 0 {
		return []Proxy{}, nil
	}

	data, err := json.MarshalIndent(proxies, "", "  ")
	if err == nil {
		_ = os.WriteFile(filepath.Join(workspace, "raw_proxies.json"), data, 0644)
	}

	return proxies, nil
}

func runPolish(workspace string, proxies []Proxy, weights ScoringWeights) (dns, nonDNS, combined []Proxy, err error) {
	// Load and merge premium proxies
	premiumPath := filepath.Join(workspace, "premium_proxies.json")
	if _, err := os.Stat(premiumPath); err == nil {
		premium := loadProxies(premiumPath)
		if len(premium) > 0 {
			fmt.Printf("  %s Merging %d premium proxies...\n", col(dim, "→"), len(premium))
			proxies = append(proxies, premium...)
		}
	}

	proxiesJSON, err := json.Marshal(proxies)
	if err != nil {
		return nil, nil, nil, err
	}
	cRaw := C.CString(string(proxiesJSON))
	defer C.free(unsafe.Pointer(cRaw))

	weightsJSON, _ := json.Marshal(weights)
	cWeights := C.CString(string(weightsJSON))
	defer C.free(unsafe.Pointer(cWeights))

	cOut := C.run_polish_c(cRaw, cWeights)
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

func buildChainDecision(mode string, dns, nonDNS, combined []Proxy, garlic bool, obfuscation *ObfuscationConfig, mimic *MimicConfig) (*RotationDecision, error) {
	// Validate mode before passing to Rust FFI
	if !validateMode(mode) {
		return nil, fmt.Errorf("invalid mode: %s (allowed: lite, stealth, high, phantom)", mode)
	}

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
		return nil, fmt.Errorf("build_chain_decision_c returned null for mode: %s", mode)
	}
	defer C.free_c_string(cOut)

	var d RotationDecision
	if err := json.Unmarshal([]byte(C.GoString(cOut)), &d); err != nil {
		return nil, err
	}
	d.Garlic = garlic

	// Inject obfuscation config if active
	if obfuscation != nil && obfuscation.Mode != "off" {
		for i := range d.Chain {
			d.Chain[i].Obfuscation = obfuscation
		}
	}

	// Inject mimic config if active
	if mimic != nil && mimic.Protocol != "" {
		for i := range d.Chain {
			d.Chain[i].Mimic = mimic
		}
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

func loadSignaturesConfig(path string) *SignatureConfig {
	config := &SignatureConfig{
		Profiles: make(map[string]SignatureProfile),
	}
	data, err := os.ReadFile(path)
	if err != nil {
		return config
	}
	err = yaml.Unmarshal(data, config)
	if err != nil {
		log.Printf("%s Error parsing signatures config: %v\n", col(red, "✗"), err)
	}
	return config
}

func loadObfuscationConfig(path string) *ObfuscationConfig {
	data, err := os.ReadFile(path)
	if err != nil {
		return &ObfuscationConfig{Mode: "off"}
	}
	var config ObfuscationConfig
	if err := yaml.Unmarshal(data, &config); err != nil {
		fmt.Printf("%s Error parsing obfuscation.yaml: %v\n", col(red, "✗"), err)
		return &ObfuscationConfig{Mode: "off"}
	}
	return &config
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

func flagBool(args []string, name string) bool {
	for _, a := range args {
		if a == name {
			return true
		}
	}
	return false
}

func parseRunArgs(args []string, defaultMode string, defaultLimit int, defaultProto string) (mode string, limit int, protocol string, garlic bool, obfuscation *ObfuscationConfig, mimic *MimicConfig, vpnConfig string, vpnPosition string) {
	mode = flagStr(args, "--mode", defaultMode)
	limit = flagInt(args, "--limit", defaultLimit)
	protocol = flagStr(args, "--protocol", defaultProto)
	garlic = flagBool(args, "--garlic")

	vpnConfig = flagStr(args, "--vpn-config", "")
	vpnPosition = flagStr(args, "--vpn-position", "any")

	obfFile := flagStr(args, "--obfuscation-config", "obfuscation.yaml")
	obfuscation = loadObfuscationConfig(obfFile)

	sigFile := flagStr(args, "--signatures-config", "signatures.yaml")
	_ = loadSignaturesConfig(sigFile) // We load it to ensure it's valid if user provided it

	mimic = &MimicConfig{
		Protocol:    flagStr(args, "--mimic-protocol", ""),
		Fingerprint: flagStr(args, "--mimic-fingerprint", "chrome"),
	}

	if m := flagStr(args, "--obfuscation-mode", ""); m != "" {
		obfuscation.Mode = m
	}
	if j := flagInt(args, "--jitter-range", -1); j != -1 {
		obfuscation.JitterRange = j
	}
	padding := flagStr(args, "--padding-range", "")
	if padding != "" {
		var min, max int
		fmt.Sscanf(padding, "%d-%d", &min, &max)
		if max > 0 {
			obfuscation.PaddingRange = [2]int{min, max}
		}
	}
	if nodeID := flagStr(args, "--node-id", ""); nodeID != "" {
		obfuscation.NodeID = nodeID
	}
	if pubKey := flagStr(args, "--public-key", ""); pubKey != "" {
		obfuscation.PublicKey = pubKey
	}
	if cert := flagStr(args, "--cert", ""); cert != "" {
		obfuscation.Cert = cert
	}
	if iat := flagInt(args, "--iat-mode", -1); iat != -1 {
		obfuscation.IATMode = iat
	}
	return
}

func flagFloat(args []string, name string, def float64) float64 {
	v := flagStr(args, name, "")
	if v == "" {
		return def
	}
	var n float64
	fmt.Sscanf(v, "%f", &n)
	return n
}

func parseWeightArgs(args []string) ScoringWeights {
	w := defaultWeights()
	w.Latency = flagFloat(args, "--lat-weight", w.Latency)
	w.Anonymity = flagFloat(args, "--anon-weight", w.Anonymity)
	w.Country = flagFloat(args, "--country-weight", w.Country)
	w.Protocol = flagFloat(args, "--proto-weight", w.Protocol)
	w.Premium = flagFloat(args, "--premium-weight", w.Premium)
	return w
}
