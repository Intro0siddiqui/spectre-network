package main

import (
	"testing"
)

func TestVPNManagerInitialization(t *testing.T) {
	// This test will fail because VPNManager is not yet defined.
	manager := NewVPNManager("test.conf")
	if manager == nil {
		t.Fatal("Failed to create VPNManager")
	}
}

func TestParseConfig(t *testing.T) {
	configStr := `[Interface]
PrivateKey = ABCDEFG
Address = 10.0.0.1/32

[Peer]
PublicKey = HIJKLMN
Endpoint = 1.2.3.4:51820
AllowedIPs = 0.0.0.0/0
`
	manager := NewVPNManager("")
	config, err := manager.ParseConfig(configStr)
	if err != nil {
		t.Fatalf("Failed to parse config: %v", err)
	}

	if config.PrivateKey != "ABCDEFG" {
		t.Errorf("Expected PrivateKey ABCDEFG, got %s", config.PrivateKey)
	}
	if config.PeerPublicKey != "HIJKLMN" {
		t.Errorf("Expected PeerPublicKey HIJKLMN, got %s", config.PeerPublicKey)
	}
	if config.Endpoint != "1.2.3.4:51820" {
		t.Errorf("Expected Endpoint 1.2.3.4:51820, got %s", config.Endpoint)
	}
}
