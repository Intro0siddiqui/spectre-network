package main

import (
	"os"
	"testing"
)

func TestLoadSignaturesConfig(t *testing.T) {
	yamlContent := `
profiles:
  chrome:
    ja3: "771,4865-4866-4867-49195-49199-49196-49200-52393-52392-49171-49172-156-157-47-53,0-23-65281-10-11-35-16-5-13-18-51-45-43-27-21,29-23-24,0"
    alpn: "h2,http/1.1"
  firefox:
    ja3: "771,4865-4866-4867-49195-49199-49196-49200-52393-52392-49171-49172-156-157-47-53,0-23-65281-10-11-35-16-5-13-18-51-45-43-27-21,29-23-24,0"
    alpn: "h2,http/1.1"
`
	tmpFile := "signatures_test.yaml"
	err := os.WriteFile(tmpFile, []byte(yamlContent), 0644)
	if err != nil {
		t.Fatalf("Failed to create temp file: %v", err)
	}
	defer os.Remove(tmpFile)

	config := loadSignaturesConfig(tmpFile)
	if config == nil {
		t.Fatal("Expected config, got nil")
	}

	if _, ok := config.Profiles["chrome"]; !ok {
		t.Error("Expected chrome profile")
	}

	if config.Profiles["chrome"].JA3 != "771,4865-4866-4867-49195-49199-49196-49200-52393-52392-49171-49172-156-157-47-53,0-23-65281-10-11-35-16-5-13-18-51-45-43-27-21,29-23-24,0" {
		t.Errorf("Expected specific JA3, got %s", config.Profiles["chrome"].JA3)
	}
}

func TestParseMimicArgs(t *testing.T) {
	args := []string{"--mimic-protocol", "https", "--mimic-fingerprint", "chrome"}
	
	// Since parseRunArgs is already defined, I'll need to update it.
	// For now, this test will fail to compile until I update the orchestrator.
	_, _, _, _, _, mimic, _, _ := parseRunArgs(args, "phantom", 500, "all")
	
	if mimic == nil {
		t.Fatal("Expected mimic config, got nil")
	}
	if mimic.Protocol != "https" {
		t.Errorf("Expected https protocol, got %s", mimic.Protocol)
	}
}

func TestGenerateTLSClientHello(t *testing.T) {
	// Example JA3 fingerprint for Chrome 120
	ja3 := "771,4865-4866-4867-49195-49199-49196-49200-52393-52392-49171-49172-156-157-47-53,0-23-65281-10-11-35-16-5-13-18-51-45-43-27-21,29-23-24,0"
	
	// This function should be implemented in tunnel.go
	// Since we are using utls, we expect a utls.Config or similar.
	// For now we'll just test if it exists.
	_, err := parseJA3(ja3)
	if err != nil {
		t.Fatalf("Failed to parse JA3 (Red Phase): %v", err)
	}
}
