package main

/*
#cgo pkg-config: python3
#cgo LDFLAGS: -L./target/release -Wl,-rpath=./target/release -lrotator_rs -ldl -lm
#include <stdlib.h>

// Forward declarations of C functions from Rust C API
extern char* run_polish_c(const char* raw_json);
extern char* build_chain_decision_c(const char* mode, const char* dns_json, const char* non_dns_json, const char* combined_json);
extern void free_c_string(char* s);
*/
import "C"

import (
	"encoding/json"
	"flag"
	"fmt"
	"io/ioutil"
	"log"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"unsafe"
)

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

func main() {
	mode := flag.String("mode", "phantom", "Operating mode")
	limit := flag.Int("limit", 500, "Proxy limit")
	protocol := flag.String("protocol", "all", "Proxy protocol")
	step := flag.String("step", "full", "Pipeline step")
	stats := flag.Bool("stats", false, "Print stats")
	_ = flag.Int("port", 1080, "SOCKS port")
	flag.Parse()

	workspace, err := os.Getwd()
	if err != nil {
		log.Fatal(err)
	}

	if *stats {
		printStats(workspace)
		return
	}

	switch *step {
	case "scrape":
		_, err := runScraper(workspace, *limit, *protocol)
		if err != nil {
			log.Fatalf("Scraper failed: %v", err)
		}
	case "polish":
		raw := loadProxies(filepath.Join(workspace, "raw_proxies.json"))
		_, _, _, err := runPolish(workspace, raw)
		if err != nil {
			log.Fatalf("Polish failed: %v", err)
		}
	case "rotate":
		dns, nonDNS, combined := loadPools(workspace)
		decision, err := buildChainDecision(*mode, dns, nonDNS, combined)
		if err != nil {
			log.Fatalf("Rotate failed: %v", err)
		}
		if decision != nil {
			printDecision(decision)
		} else {
			log.Println("Failed to build chain")
		}
	case "serve":
		// Serve needs port logic, mostly stubbed here as per Rust impl
		dns, nonDNS, combined := loadPools(workspace)
		decision, err := buildChainDecision(*mode, dns, nonDNS, combined)
		if err != nil {
			log.Fatalf("Rotate serve failed: %v", err)
		}
		if decision != nil {
			printDecision(decision)
			log.Println("Serve not fully implemented in Go wrapper (tunnel port mapping needed)")
		} else {
			log.Println("Failed to build chain. Run 'full' or 'scrape' first to populate pools.")
		}
	case "full":
		raw, err := runScraper(workspace, *limit, *protocol)
		if err != nil {
			log.Fatalf("Scraper failed: %v", err)
		}
		dns, nonDNS, combined, err := runPolish(workspace, raw)
		if err != nil {
			log.Fatalf("Polish failed: %v", err)
		}
		decision, err := buildChainDecision(*mode, dns, nonDNS, combined)
		if err != nil {
			log.Fatalf("Rotate failed: %v", err)
		}
		if decision != nil {
			printDecision(decision)
		} else {
			log.Println("Failed to build chain")
		}
		printSummary(len(combined), len(dns), len(nonDNS))
	default:
		log.Fatalf("Unknown step: %s", *step)
	}
}

func runScraper(workspace string, limit int, protocol string) ([]Proxy, error) {
	log.Println("Starting Go scraper...")
	scraperPath := filepath.Join(workspace, "go_scraper")
	if _, err := os.Stat(scraperPath); os.IsNotExist(err) {
		return nil, fmt.Errorf("go_scraper binary not found at %s", scraperPath)
	}

	cmd := exec.Command(scraperPath, "--limit", fmt.Sprintf("%d", limit), "--protocol", protocol)
	output, err := cmd.CombinedOutput()
	if err != nil {
		return nil, fmt.Errorf("go scraper err: %v, output: %s", err, string(output))
	}

	rawJSON := string(output)
	if strings.TrimSpace(rawJSON) == "" {
		log.Println("Go scraper returned empty output")
		return []Proxy{}, nil
	}

	err = ioutil.WriteFile(filepath.Join(workspace, "raw_proxies.json"), output, 0644)
	if err != nil {
		return nil, err
	}

	var proxies []Proxy
	err = json.Unmarshal(output, &proxies)
	if err != nil {
		return nil, fmt.Errorf("failed to parse go_scraper output: %v", err)
	}
	log.Printf("Scraped %d proxies\n", len(proxies))
	return proxies, nil
}

func runPolish(workspace string, proxies []Proxy) (dns, nonDNS, combined []Proxy, err error) {
	log.Printf("Polishing %d proxies...\n", len(proxies))

	proxiesJSON, err := json.Marshal(proxies)
	if err != nil {
		return nil, nil, nil, err
	}

	cRawJSON := C.CString(string(proxiesJSON))
	defer C.free(unsafe.Pointer(cRawJSON))

	cOutJSON := C.run_polish_c(cRawJSON)
	if cOutJSON == nil {
		return nil, nil, nil, fmt.Errorf("rust polish returned null")
	}
	defer C.free_c_string(cOutJSON)

	outStr := C.GoString(cOutJSON)

	var result PolishResult
	err = json.Unmarshal([]byte(outStr), &result)
	if err != nil {
		return nil, nil, nil, fmt.Errorf("failed to parse polish result: %v", err)
	}

	// Save pools
	saveJSON(filepath.Join(workspace, "proxies_dns.json"), result.DNS)
	saveJSON(filepath.Join(workspace, "proxies_non_dns.json"), result.NonDNS)
	saveJSON(filepath.Join(workspace, "proxies_combined.json"), result.Combined)

	return result.DNS, result.NonDNS, result.Combined, nil
}

func loadProxies(path string) []Proxy {
	var proxies []Proxy
	data, err := ioutil.ReadFile(path)
	if err != nil {
		return proxies
	}
	_ = json.Unmarshal(data, &proxies)
	return proxies
}

func saveJSON(path string, v interface{}) {
	data, _ := json.MarshalIndent(v, "", "  ")
	_ = ioutil.WriteFile(path, data, 0644)
}

func loadPools(workspace string) (dns, nonDNS, combined []Proxy) {
	return loadProxies(filepath.Join(workspace, "proxies_dns.json")),
		loadProxies(filepath.Join(workspace, "proxies_non_dns.json")),
		loadProxies(filepath.Join(workspace, "proxies_combined.json"))
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

	cOutJSON := C.build_chain_decision_c(cMode, cDNS, cNonDNS, cCombined)
	if cOutJSON == nil {
		return nil, nil // Null decision implies failure to build
	}
	defer C.free_c_string(cOutJSON)

	outStr := C.GoString(cOutJSON)
	var decision RotationDecision
	err := json.Unmarshal([]byte(outStr), &decision)
	if err != nil {
		return nil, err
	}
	return &decision, nil
}

func printDecision(d *RotationDecision) {
	data, _ := json.MarshalIndent(d, "", "  ")
	fmt.Println(string(data))
}

func printStats(workspace string) {
	dns, nonDNS, combined := loadPools(workspace)
	fmt.Println("\n=== Spectre Network Stats ===")
	fmt.Printf("Total proxies (Combined): %d\n", len(combined))
	fmt.Printf("DNS-Capable: %d\n", len(dns))
	fmt.Printf("Non-DNS: %d\n", len(nonDNS))

	if len(combined) > 0 {
		var sumLat, sumScore float64
		for _, p := range combined {
			sumLat += p.Latency
			sumScore += p.Score
		}
		fmt.Printf("Average Latency: %.3fs\n", sumLat/float64(len(combined)))
		fmt.Printf("Average Score: %.3f\n", sumScore/float64(len(combined)))
	}
}

func printSummary(total, dns, nonDNS int) {
	fmt.Println("\n=== Spectre Polish Summary ===")
	fmt.Printf("Total proxies: %d\n", total)
	fmt.Printf("DNS-capable: %d\n", dns)
	fmt.Printf("Non-DNS: %d\n", nonDNS)
}
