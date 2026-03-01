package main

import (
	"net"
	"strings"
	"testing"
	"time"
)

func TestSOCKS5Handshake(t *testing.T) {
	// Create a pipe to simulate a connection
	client, server := net.Pipe()
	defer client.Close()
	defer server.Close()

	// Run handleSOCKS5Client in a goroutine
	errCh := make(chan error, 1)
	go func() {
		errCh <- handleSOCKS5Client(server, RotationDecision{}, nil, nil, nil, nil, nil)
	}()

	// 1. Send version and methods
	client.Write([]byte{0x05, 0x01, 0x00})
	
	// 2. Read selected method
	buf := make([]byte, 2)
	client.Read(buf)
	if buf[0] != 0x05 || buf[1] != 0x00 {
		t.Errorf("Expected 0x05 0x00, got %x %x", buf[0], buf[1])
	}

	// 3. Send CONNECT request for IPv4 127.0.0.1:80
	client.Write([]byte{0x05, 0x01, 0x00, 0x01, 127, 0, 0, 1, 0, 80})
	
	// Wait for error
	select {
	case err := <-errCh:
		if err == nil || !strings.Contains(err.Error(), "empty proxy chain") {
			t.Errorf("Expected error containing 'empty proxy chain', got %v", err)
		}
	case <-time.After(100 * time.Millisecond):
		t.Errorf("Timeout waiting for handleSOCKS5Client to finish")
	}
}

func TestBuildCircuit(t *testing.T) {
	chain := []ChainHop{}
	_, err := buildCircuit(chain, "example.com:80", nil, nil, nil, "lite", false, nil, nil)
	if err == nil {
		t.Errorf("Expected error for empty chain")
	}
}

func TestHandshakeProxyWithMimic(t *testing.T) {
	client, server := net.Pipe()
	defer client.Close()
	defer server.Close()

	hop := ChainHop{
		IP:    "127.0.0.1",
		Port:  1080,
		Proto: "socks5",
	}

	mimic := &MimicConfig{
		Protocol:    "https",
		Fingerprint: "chrome",
	}

	go func() {
		// This should initiate TLS handshake via utls
		handshakeProxy(client, hop, "example.com:80", mimic)
	}()

	// Read first few bytes from server
	buf := make([]byte, 5)
	server.SetReadDeadline(time.Now().Add(100 * time.Millisecond))
	n, err := server.Read(buf)
	if err != nil {
		t.Fatalf("Failed to read from server: %v", err)
	}

	// TLS ClientHello starts with 0x16 (Handshake)
	if n < 1 || buf[0] != 0x16 {
		t.Errorf("Expected TLS ClientHello (0x16), got %x", buf[0])
	}
}

func TestHandshakeProxyWithQUICMimic(t *testing.T) {
	client, server := net.Pipe()
	defer client.Close()
	defer server.Close()

	hop := ChainHop{
		IP:    "127.0.0.1",
		Port:  1080,
		Proto: "socks5",
	}

	mimic := &MimicConfig{
		Protocol:    "quic",
		Fingerprint: "chrome",
	}

	go func() {
		// This should initiate pseudo-QUIC header wrapping
		handshakeProxy(client, hop, "example.com:80", mimic)
	}()

	// Read first few bytes from server
	buf := make([]byte, 5)
	server.SetReadDeadline(time.Now().Add(100 * time.Millisecond))
	n, err := server.Read(buf)
	if err != nil {
		t.Fatalf("Failed to read from server: %v", err)
	}

	// Pseudo-QUIC header should start with 0xc0 (Long Header) for version negotiation or initial
	if n < 1 || buf[0] != 0xc0 {
		t.Errorf("Expected pseudo-QUIC header (0xc0), got %x", buf[0])
	}
}

func TestCryptoRoundtrip(t *testing.T) {
	keyHex := "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
	nonceHex := "000102030405060708090a0b"
	plaintext := []byte("Hello, Spectre!")
	
	encrypted, err := encryptWithCounter(keyHex, nonceHex, 0, plaintext)
	if err != nil {
		t.Fatalf("Encryption failed: %v", err)
	}
	
	decrypted, err := decryptWithCounter(keyHex, nonceHex, 0, encrypted)
	if err != nil {
		t.Fatalf("Decryption failed: %v", err)
	}
	
	if string(decrypted) != string(plaintext) {
		t.Errorf("Expected %s, got %s", string(plaintext), string(decrypted))
	}
}
