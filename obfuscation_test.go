package main

import (
	"encoding/binary"
	"io"
	"net"
	"testing"
	"time"
)

func TestEncryptedPipeGarlicPadding(t *testing.T) {
	client, serverClient := net.Pipe()
	serverOut, serverOutRemote := net.Pipe()
	serverIn, _ := net.Pipe()
	defer client.Close()
	defer serverClient.Close()
	defer serverOut.Close()
	defer serverOutRemote.Close()

	cryptoHops := []CryptoHop{
		{
			KeyHex:   "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
			NonceHex: "000102030405060708090a0b",
		},
	}

	// Currently encryptedPipeGarlic doesn't take ObfuscationConfig in its signature
	// This test will fail to compile if I add it now, so I'll first update the signature in tunnel.go
	// but leave implementation as is (fixed padding).
	
	go func() {
		// Advanced mode with custom padding range
		config := &ObfuscationConfig{
			Mode:         "advanced",
			PaddingRange: [2]int{600, 1000},
			JitterRange:  10,
		}
		encryptedPipeGarlic(client, serverOut, serverIn, cryptoHops, true, config)
	}()

	msg := []byte("test message")
	go client.Write(msg)

	// Read from serverOutRemote
	head := make([]byte, 12)
	serverOutRemote.SetReadDeadline(time.Now().Add(5 * time.Second))
	if _, err := io.ReadFull(serverOutRemote, head); err != nil {
		t.Fatalf("Failed to read frame head: %v", err)
	}
	
	length := binary.LittleEndian.Uint32(head[8:12])
	
	// If it's still fixed 512, this will pass the check length > 512 if I set range higher.
	// But it SHOULD be within the range [600, 1000].
	
	if length < 600 || length > 1050 {
		t.Errorf("Expected length in range [600, 1050], got %d", length)
	}
}

func TestEncryptedPipeGarlicChaffing(t *testing.T) {
	client, _ := net.Pipe()
	serverOut, serverOutRemote := net.Pipe()
	serverIn, _ := net.Pipe()
	defer client.Close()
	defer serverOut.Close()
	defer serverOutRemote.Close()

	cryptoHops := []CryptoHop{
		{
			KeyHex:   "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
			NonceHex: "000102030405060708090a0b",
		},
	}

	go func() {
		// Advanced mode with very frequent chaffing
		config := &ObfuscationConfig{
			Mode:         "advanced",
			PaddingRange: [2]int{100, 200},
			JitterRange:  50, // 50ms interval
		}
		encryptedPipeGarlic(client, serverOut, serverIn, cryptoHops, true, config)
	}()

	// Don't write anything to client, wait for chaffing
	
	// Read multiple frames from serverOutRemote
	for i := 0; i < 3; i++ {
		head := make([]byte, 12)
		serverOutRemote.SetReadDeadline(time.Now().Add(1 * time.Second))
		if _, err := io.ReadFull(serverOutRemote, head); err != nil {
			t.Fatalf("Failed to read chaff head %d: %v", i, err)
		}
		
		length := binary.LittleEndian.Uint32(head[8:12])
		
		// 100-200 payload + 16 overhead = 116-216
		if length < 116 || length > 216 {
			t.Errorf("Expected chaff length in range [116, 216], got %d", length)
		}
		
		// Consume payload
		payload := make([]byte, length)
		io.ReadFull(serverOutRemote, payload)
	}
}
