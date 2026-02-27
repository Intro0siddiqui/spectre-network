package main

import (
	"bytes"
	"testing"
)

func TestLayeredEncryptionIntegrity(t *testing.T) {
	// Simulate 3-hop keys and nonces
	hop1Key := "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
	hop1Nonce := "000102030405060708090a0b"
	
	hop2Key := "101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f"
	hop2Nonce := "101112131415161718191a1b"
	
	hop3Key := "202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f"
	hop3Nonce := "202122232425262728292a2b"
	
	plaintext := []byte("Sensitive data through 3 hops")
	counter := uint64(42)

	// Outbound: Encrypt in REVERSE order (3, then 2, then 1)
	// Actually, the onion model means you encrypt for the LAST hop first, 
	// then the middle, then the first.
	
	// Layer 3 (Exit Hop)
	e3, err := encryptWithCounter(hop3Key, hop3Nonce, counter, plaintext)
	if err != nil {
		t.Fatalf("Hop 3 encryption failed: %v", err)
	}
	
	// Layer 2 (Middle Hop)
	e2, err := encryptWithCounter(hop2Key, hop2Nonce, counter, e3)
	if err != nil {
		t.Fatalf("Hop 2 encryption failed: %v", err)
	}
	
	// Layer 1 (Entry Hop)
	e1, err := encryptWithCounter(hop1Key, hop1Nonce, counter, e2)
	if err != nil {
		t.Fatalf("Hop 1 encryption failed: %v", err)
	}

	// Inbound: Decrypt in FORWARD order (1, then 2, then 3)
	
	// Layer 1
	d1, err := decryptWithCounter(hop1Key, hop1Nonce, counter, e1)
	if err != nil {
		t.Fatalf("Hop 1 decryption failed: %v", err)
	}
	if !bytes.Equal(d1, e2) {
		t.Errorf("Layer 1 decryption mismatch")
	}
	
	// Layer 2
	d2, err := decryptWithCounter(hop2Key, hop2Nonce, counter, d1)
	if err != nil {
		t.Fatalf("Hop 2 decryption failed: %v", err)
	}
	if !bytes.Equal(d2, e3) {
		t.Errorf("Layer 2 decryption mismatch")
	}
	
	// Layer 3
	d3, err := decryptWithCounter(hop3Key, hop3Nonce, counter, d2)
	if err != nil {
		t.Fatalf("Hop 3 decryption failed: %v", err)
	}
	if !bytes.Equal(d3, plaintext) {
		t.Errorf("Layer 3 decryption mismatch, expected %s, got %s", string(plaintext), string(d3))
	}
}
