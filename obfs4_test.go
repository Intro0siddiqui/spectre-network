package main

import (
	"net"
	"testing"
	"time"
)

func TestObfs4HandshakeWrapper(t *testing.T) {
	// This test expects a wrapObfs4 function to exist in tunnel.go
	// For now, it will fail to compile or fail if we use a mock.
	
	server, client := net.Pipe()
	defer server.Close()
	defer client.Close()

	config := &ObfuscationConfig{
		Mode:      "obfs4",
		NodeID:    "1234567890abcdef1234567890abcdef12345678",
		PublicKey: "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
		Cert:      "cert",
		IATMode:   0,
	}

	// We'll implement this function in the Green Phase
	// obfsConn, err := wrapObfs4Client(client, "127.0.0.1:8080", config)
	
	// For Red Phase, we'll just check if we can call it (it should fail if not implemented)
	t.Run("ImplementationExists", func(t *testing.T) {
		// This will be implemented in tunnel.go
		_, err := wrapObfs4Client(client, "127.0.0.1:8080", config)
		if err == nil {
			t.Errorf("Expected wrapObfs4Client to fail (handshake timeout/mismatch), but it didn't")
		}
	})
}

func TestHandshakeProxyWithObfs4(t *testing.T) {
	server, client := net.Pipe()
	defer server.Close()
	defer client.Close()

	hop := ChainHop{
		IP:    "127.0.0.1",
		Port:  8080,
		Proto: "socks5",
		Obfuscation: &ObfuscationConfig{
			Mode:      "obfs4",
			NodeID:    "1234567890abcdef1234567890abcdef12345678",
			PublicKey: "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
			IATMode:   0,
		},
	}

	go func() {
		// This should initiate obfs4 handshake
		handshakeProxy(client, hop, "example.com:80", nil)	}()

	// Read first few bytes from server
	buf := make([]byte, 3)
	server.SetReadDeadline(time.Now().Add(100 * time.Millisecond))
	n, err := server.Read(buf)
	if err != nil {
		// It might time out because obfs4 waits for server response during handshake
		return 
	}

	// If it was plain SOCKS5, it would be 05 01 00
	if n >= 3 && buf[0] == 0x05 && buf[1] == 0x01 && buf[2] == 0x00 {
		t.Errorf("Handshake sent plain SOCKS5 data instead of obfs4")
	}
}
