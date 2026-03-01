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
