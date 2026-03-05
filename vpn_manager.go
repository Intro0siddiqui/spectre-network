package main

import (
	"bufio"
	"fmt"
	"strings"
)

// VPNConfig stores parsed WireGuard configuration.
type VPNConfig struct {
	PrivateKey     string
	Address        string
	PeerPublicKey  string
	Endpoint       string
	AllowedIPs     string
	PresharedKey   string
}

// VPNManager handles the user-space WireGuard interface.
type VPNManager struct {
	ConfigPath string
}

// NewVPNManager creates a new VPNManager.
func NewVPNManager(configPath string) *VPNManager {
	return &VPNManager{
		ConfigPath: configPath,
	}
}

// ParseConfig parses a standard WireGuard .conf file string.
func (v *VPNManager) ParseConfig(configStr string) (*VPNConfig, error) {
	config := &VPNConfig{}
	scanner := bufio.NewScanner(strings.NewReader(configStr))
	var section string

	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}

		if strings.HasPrefix(line, "[") && strings.HasSuffix(line, "]") {
			section = strings.ToLower(line[1 : len(line)-1])
			continue
		}

		parts := strings.SplitN(line, "=", 2)
		if len(parts) != 2 {
			continue
		}

		key := strings.ToLower(strings.TrimSpace(parts[0]))
		val := strings.TrimSpace(parts[1])

		switch section {
		case "interface":
			switch key {
			case "privatekey":
				config.PrivateKey = val
			case "address":
				config.Address = val
			}
		case "peer":
			switch key {
			case "publickey":
				config.PeerPublicKey = val
			case "endpoint":
				config.Endpoint = val
			case "allowedips":
				config.AllowedIPs = val
			case "presharedkey":
				config.PresharedKey = val
			}
		}
	}

	if err := scanner.Err(); err != nil {
		return nil, err
	}

	if config.PrivateKey == "" {
		return nil, fmt.Errorf("missing PrivateKey in [Interface]")
	}
	if config.PeerPublicKey == "" {
		return nil, fmt.Errorf("missing PublicKey in [Peer]")
	}

	return config, nil
}
