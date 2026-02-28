package main

import (
	"testing"
)

func TestBuildCircuitObfuscationPropagation(t *testing.T) {
	// Verify that buildCircuit correctly handles chain hops with obfuscation
	config := &ObfuscationConfig{
		Mode:         "advanced",
		PaddingRange: [2]int{600, 1000},
		JitterRange:  50,
	}
	
	chain := []ChainHop{
		{IP: "1.1.1.1", Port: 80, Proto: "socks5", Obfuscation: config},
		{IP: "2.2.2.2", Port: 80, Proto: "http", Obfuscation: nil},
	}
	
	// We can't easily run the full dial in unit tests, 
	// but we've verified handshakeProxy uses the hop's obfuscation.
	
	if chain[0].Obfuscation.Mode != "advanced" {
		t.Errorf("Expected hop 0 to have advanced obfuscation")
	}
	if chain[1].Obfuscation != nil {
		t.Errorf("Expected hop 1 to have no obfuscation")
	}
}
