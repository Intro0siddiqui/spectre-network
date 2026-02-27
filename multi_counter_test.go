package main

import (
	"bytes"
	"encoding/binary"
	"fmt"
	"testing"
)

func TestMultiLayerCounterRoundtrip(t *testing.T) {
	// Keys and nonces for a 3-hop chain
	hops := []CryptoHop{
		{KeyHex: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef", NonceHex: "0123456789abcdef01234567"},
		{KeyHex: "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210", NonceHex: "fedcba9876543210fedcba98"},
		{KeyHex: "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff", NonceHex: "00112233445566778899aabb"},
	}

	// Test multiple sequential packets (simulating a session)
	session, err := NewCryptoSession(hops)
	if err != nil {
		t.Fatalf("Failed to create crypto session: %v", err)
	}

	for counter := uint64(0); counter < 10; counter++ {
		plaintext := []byte(fmt.Sprintf("Packet data with counter %d", counter))
		
		// 1. Outbound: Encrypt all layers in one call
		payload, err := encryptLayered(session, counter, plaintext)
		if err != nil {
			t.Fatalf("Outbound encryption failed at counter %d: %v", counter, err)
		}

		// 2. Inbound: Decrypt all layers in one call
		payload, err = decryptLayered(session, counter, payload)
		if err != nil {
			t.Fatalf("Inbound decryption failed at counter %d: %v", counter, err)
		}

		if !bytes.Equal(payload, plaintext) {
			t.Errorf("Counter %d mismatch, expected %s, got %s", counter, string(plaintext), string(payload))
		}
	}
}

func TestCounterMismatchFails(t *testing.T) {
	// Wrong counter should fail decryption (GCM auth check)
	key := "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
	nonce := "000102030405060708090a0b"
	plaintext := []byte("Sensitive data")
	
	// Encrypt with counter 0
	ciphertext, err := encryptWithCounter(key, nonce, 0, plaintext)
	if err != nil {
		t.Fatalf("Encryption failed: %v", err)
	}
	
	// Decrypt with counter 1 (SHOULD FAIL)
	_, err = decryptWithCounter(key, nonce, 1, ciphertext)
	if err == nil {
		t.Errorf("Decryption with wrong counter should have failed")
	}
}

func TestChaffingPacket(t *testing.T) {
	hops := []CryptoHop{
		{KeyHex: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef", NonceHex: "0123456789abcdef01234567"},
	}
	session, _ := NewCryptoSession(hops)
	
	// Dummy packet (origLen = 0)
	payload := make([]byte, 512)
	binary.LittleEndian.PutUint16(payload[0:2], 0)
	
	encrypted, err := encryptLayered(session, 123, payload)
	if err != nil {
		t.Fatalf("Encryption failed: %v", err)
	}
	
	decrypted, err := decryptLayered(session, 123, encrypted)
	if err != nil {
		t.Fatalf("Decryption failed: %v", err)
	}
	
	// Check origLen
	origLen := binary.LittleEndian.Uint16(decrypted[0:2])
	if origLen != 0 {
		t.Errorf("Expected origLen 0 for chaff packet, got %d", origLen)
	}
}

