package main

import (
	"os"
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

func TestVPNManagerConnect(t *testing.T) {
	configContent := `[Interface]
PrivateKey = 000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f
Address = 10.0.0.1/32

[Peer]
PublicKey = 202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f
Endpoint = 1.2.3.4:51820
`
	tmpFile := "test_wg.conf"
	os.WriteFile(tmpFile, []byte(configContent), 0644)
	defer os.Remove(tmpFile)

	manager := NewVPNManager(tmpFile)
	err := manager.Connect()
	if err != nil {
		t.Fatalf("Connect failed: %v", err)
	}
	if manager.Dialer == nil {
		t.Fatal("Expected dialer to be set after Connect")
	}
}

func TestVPNFailover(t *testing.T) {
	// Mock a failing VPNManager (config file doesn't exist)
	manager := NewVPNManager("nonexistent.conf")
	err := manager.Connect()
	if err == nil {
		t.Fatal("Expected Connect to fail for nonexistent config")
	}
	
	// Ensure that even if VPN is nil, circuit building still works (using standard proxies)
	chain := []ChainHop{
		{IP: "1.2.3.4", Port: 1080, Proto: "socks5"},
	}
	_, err = buildCircuit(chain, "example.com:80", nil, nil, nil, "lite", false, nil, nil, nil, "entry")
	// It will still fail because 1.2.3.4:1080 is unreachable, but it shouldn't crash
	if err == nil {
		t.Errorf("Expected error for unreachable proxy")
	}
}
