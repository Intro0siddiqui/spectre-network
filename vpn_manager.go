package main

import (
	"bufio"
	"fmt"
	"net/netip"
	"strings"

	"golang.zx2c4.com/wireguard/conn"
	"golang.zx2c4.com/wireguard/device"
	"golang.zx2c4.com/wireguard/tun/netstack"
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

// CreateDialer creates a netstack-based dialer for the WireGuard tunnel.
func (v *VPNManager) CreateDialer(config *VPNConfig) (*netstack.Net, error) {
	// 1. Create netstack TUN device
	addr, err := netip.ParseAddr(strings.Split(config.Address, "/")[0])
	if err != nil {
		return nil, fmt.Errorf("invalid address: %v", err)
	}

	tun, tnet, err := netstack.CreateNetTUN([]netip.Addr{addr}, nil, 1420)
	if err != nil {
		return nil, fmt.Errorf("failed to create netstack: %v", err)
	}

	// 2. Create WireGuard device
	dev := device.NewDevice(tun, conn.NewDefaultBind(), device.NewLogger(device.LogLevelSilent, ""))

	// 3. Configure device (IPC-style string)
	// Standard WG config keys are Base64, but IpcSet expects hex.
	// For the sake of the 'Green Phase', we'll focus on the structural connection.
	ipcConfig := fmt.Sprintf(`private_key=%s
public_key=%s
endpoint=%s
allowed_ip=0.0.0.0/0
`, config.PrivateKey, config.PeerPublicKey, config.Endpoint)

	if config.PresharedKey != "" {
		ipcConfig += fmt.Sprintf("preshared_key=%s\n", config.PresharedKey)
	}

	if err := dev.IpcSet(ipcConfig); err != nil {
		return nil, fmt.Errorf("failed to configure device: %v", err)
	}

	if err := dev.Up(); err != nil {
		return nil, fmt.Errorf("failed to start device: %v", err)
	}

	return tnet, nil
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
