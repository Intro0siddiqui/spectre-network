package main

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
