/// Per-hop AES-256-GCM encryption/decryption.
///
/// Every outbound payload is encrypted with the exit hop's key before entering
/// the proxy chain. Middle hops forward opaque ciphertext; only the exit hop
/// (which already sees cleartext in any proxy model) receives readable data.
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Key, Nonce,
};
use anyhow::{Context, Result};

/// Derive a unique 12-byte nonce from a base nonce and a packet counter.
///
/// This prevents nonce reuse in AES-GCM, which is critical for security.
/// The counter is XORed into the last 8 bytes of the base nonce.
///
/// `base_nonce` — 12-byte base nonce from the session
/// `counter`    — 64-bit packet counter (starts at 0, increments per packet)
///
/// Returns a derived 12-byte nonce that is unique for each counter value.
pub fn derive_nonce(base_nonce: &[u8], counter: u64) -> [u8; 12] {
    let mut derived = [0u8; 12];
    derived.copy_from_slice(base_nonce);

    // XOR the 64-bit counter into the last 8 bytes of the nonce
    // This ensures each packet gets a unique nonce as long as counter doesn't wrap
    let counter_bytes = counter.to_le_bytes();
    for i in 0..8 {
        derived[4 + i] ^= counter_bytes[i];
    }

    derived
}

/// Encrypt `plaintext` with AES-256-GCM using a counter-derived nonce.
///
/// `key_hex`   — 32-byte key encoded as 64 hex chars (from `CryptoHop`)
/// `nonce_hex` — 12-byte base nonce encoded as 24 hex chars (from `CryptoHop`)
/// `counter`   — 64-bit packet counter for nonce derivation
///
/// Returns `[ciphertext + tag]` — the nonce is derived from counter, not transmitted.
/// The receiver must use the same counter value to derive the same nonce for decryption.
pub fn encrypt_with_counter(
    key_hex: &str,
    nonce_hex: &str,
    counter: u64,
    plaintext: &[u8],
) -> Result<Vec<u8>> {
    let key_bytes = hex::decode(key_hex).context("bad key hex")?;
    let base_nonce_bytes = hex::decode(nonce_hex).context("bad nonce hex")?;

    let derived_nonce = derive_nonce(&base_nonce_bytes, counter);

    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(&derived_nonce);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| anyhow::anyhow!("AES-GCM encrypt error: {}", e))?;

    Ok(ciphertext)
}

/// Decrypt a ciphertext produced by `encrypt_with_counter()`.
///
/// `key_hex`   — 32-byte key encoded as 64 hex chars (from `CryptoHop`)
/// `nonce_hex` — 12-byte base nonce encoded as 24 hex chars (from `CryptoHop`)
/// `counter`   — 64-bit packet counter used for nonce derivation (must match encrypt side)
/// `data`      — ciphertext + tag (no nonce prefix, as it's derived from counter)
///
/// Returns the decrypted plaintext.
pub fn decrypt_with_counter(
    key_hex: &str,
    nonce_hex: &str,
    counter: u64,
    data: &[u8],
) -> Result<Vec<u8>> {
    let key_bytes = hex::decode(key_hex).context("bad key hex")?;
    let base_nonce_bytes = hex::decode(nonce_hex).context("bad nonce hex")?;

    let derived_nonce = derive_nonce(&base_nonce_bytes, counter);

    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(&derived_nonce);

    cipher
        .decrypt(nonce, data)
        .map_err(|e| anyhow::anyhow!("AES-GCM decrypt error: {}", e))
}

/// Encrypt `plaintext` with AES-256-GCM (legacy function, kept for compatibility).
///
/// `key_hex`   — 32-byte key encoded as 64 hex chars (from `CryptoHop`)
/// `nonce_hex` — 12-byte nonce encoded as 24 hex chars (from `CryptoHop`)
///
/// Returns `[nonce (12 bytes) || ciphertext + tag]` so the receiver can
/// always find the nonce even if it rotates later.
///
/// WARNING: This function does NOT use counter-based nonce derivation.
/// Use `encrypt_with_counter` for new code to prevent nonce reuse vulnerabilities.
pub fn encrypt(key_hex: &str, nonce_hex: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
    let key_bytes = hex::decode(key_hex).context("bad key hex")?;
    let nonce_bytes = hex::decode(nonce_hex).context("bad nonce hex")?;

    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| anyhow::anyhow!("AES-GCM encrypt error: {}", e))?;

    // Prepend nonce so decrypt() is self-contained
    let mut out = Vec::with_capacity(12 + ciphertext.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Decrypt a blob produced by `encrypt()`.
///
/// Expects `[nonce (12 bytes) || ciphertext + tag]`.
pub fn decrypt(key_hex: &str, data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < 12 {
        anyhow::bail!("ciphertext too short");
    }
    let key_bytes = hex::decode(key_hex).context("bad key hex")?;
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);

    let nonce = Nonce::from_slice(&data[..12]);
    let ciphertext = &data[12..];

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("AES-GCM decrypt error: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to generate a random 32-byte key as hex
    fn generate_test_key() -> String {
        let mut key = [0u8; 32];
        getrandom::getrandom(&mut key).unwrap();
        hex::encode(key)
    }

    /// Helper to generate a random 12-byte nonce as hex
    fn generate_test_nonce() -> String {
        let mut nonce = [0u8; 12];
        getrandom::getrandom(&mut nonce).unwrap();
        hex::encode(nonce)
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        // Test that decrypt(encrypt(data)) == data
        let key = generate_test_key();
        let nonce = generate_test_nonce();
        let plaintext = b"Hello, Spectre Network!";

        let encrypted = encrypt(&key, &nonce, plaintext).expect("Encryption should succeed");
        let decrypted = decrypt(&key, &encrypted).expect("Decryption should succeed");

        assert_eq!(decrypted, plaintext.as_slice());
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip_with_counter() {
        // Test that decrypt_with_counter(encrypt_with_counter(data)) == data
        let key = generate_test_key();
        let nonce = generate_test_nonce();
        let plaintext = b"Counter-mode encryption test";

        for counter in 0..5u64 {
            let encrypted = encrypt_with_counter(&key, &nonce, counter, plaintext)
                .expect("Encryption should succeed");
            let decrypted = decrypt_with_counter(&key, &nonce, counter, &encrypted)
                .expect("Decryption should succeed");

            assert_eq!(
                decrypted,
                plaintext.as_slice(),
                "Roundtrip failed for counter {}",
                counter
            );
        }
    }

    #[test]
    fn test_different_nonces_produce_different_ciphertext() {
        // Same plaintext + key, different nonce = different ciphertext
        let key = generate_test_key();
        let plaintext = b"Same plaintext, different nonces";

        let nonce1 = generate_test_nonce();
        let nonce2 = generate_test_nonce();

        // Ensure nonces are actually different
        assert_ne!(nonce1, nonce2, "Test setup: nonces should be different");

        let encrypted1 = encrypt(&key, &nonce1, plaintext).expect("Encryption 1 should succeed");
        let encrypted2 = encrypt(&key, &nonce2, plaintext).expect("Encryption 2 should succeed");

        // Ciphertexts should be different (including the prepended nonce)
        assert_ne!(
            encrypted1, encrypted2,
            "Different nonces should produce different ciphertexts"
        );

        // Verify both can be decrypted correctly
        let decrypted1 = decrypt(&key, &encrypted1).expect("Decryption 1 should succeed");
        let decrypted2 = decrypt(&key, &encrypted2).expect("Decryption 2 should succeed");

        assert_eq!(decrypted1, plaintext.as_slice());
        assert_eq!(decrypted2, plaintext.as_slice());
    }

    #[test]
    fn test_nonce_derivation_with_counter() {
        // Test the counter-based nonce derivation
        let base_nonce = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C,
        ];

        // Counter 0 should XOR with zeros (no change to first 4 bytes, last 8 XORed with 0)
        let derived_0 = derive_nonce(&base_nonce, 0);
        assert_eq!(derived_0, base_nonce, "Counter 0 should produce same nonce");

        // Counter 1 should change the last bytes
        let derived_1 = derive_nonce(&base_nonce, 1);
        assert_ne!(
            derived_0, derived_1,
            "Counter 1 should produce different nonce"
        );

        // Counter 2 should be different from both
        let derived_2 = derive_nonce(&base_nonce, 2);
        assert_ne!(
            derived_0, derived_2,
            "Counter 2 should produce different nonce"
        );
        assert_ne!(
            derived_1, derived_2,
            "Counter 2 should differ from counter 1"
        );

        // Verify first 4 bytes remain unchanged (counter XORed into last 8)
        assert_eq!(derived_0[..4], base_nonce[..4]);
        assert_eq!(derived_1[..4], base_nonce[..4]);
        assert_eq!(derived_2[..4], base_nonce[..4]);

        // Test counter wrapping behavior - counter 256 should affect byte at position 4+0
        let derived_256 = derive_nonce(&base_nonce, 256);
        // 256 in little-endian is [0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
        // So byte at position 4+1 should be XORed with 0x01
        assert_ne!(
            derived_256, derived_0,
            "Counter 256 should produce different nonce"
        );
    }

    #[test]
    fn test_invalid_key_fails() {
        // Wrong key should fail to decrypt
        let key1 = generate_test_key();
        let key2 = generate_test_key();
        let nonce = generate_test_nonce();
        let plaintext = b"This should fail with wrong key";

        assert_ne!(key1, key2, "Test setup: keys should be different");

        let encrypted = encrypt(&key1, &nonce, plaintext).expect("Encryption should succeed");

        // Decrypting with wrong key should fail (GCM authentication check)
        let decrypt_result = decrypt(&key2, &encrypted);
        assert!(
            decrypt_result.is_err(),
            "Decryption with wrong key should fail"
        );

        // Also test with counter mode
        let encrypted_counter = encrypt_with_counter(&key1, &nonce, 0, plaintext)
            .expect("Counter encryption should succeed");
        let decrypt_counter_result = decrypt_with_counter(&key2, &nonce, 0, &encrypted_counter);
        assert!(
            decrypt_counter_result.is_err(),
            "Counter decryption with wrong key should fail"
        );
    }

    #[test]
    fn test_tampered_ciphertext_fails() {
        // Modified ciphertext should fail authentication
        let key = generate_test_key();
        let nonce = generate_test_nonce();
        let plaintext = b"Tamper detection test";

        let encrypted = encrypt(&key, &nonce, plaintext).expect("Encryption should succeed");

        // Tamper with the ciphertext (skip first 12 bytes which is the nonce)
        let mut tampered = encrypted.clone();
        if tampered.len() > 13 {
            tampered[13] ^= 0xFF; // Flip bits in the ciphertext
        }

        let decrypt_result = decrypt(&key, &tampered);
        assert!(
            decrypt_result.is_err(),
            "Tampered ciphertext should fail authentication"
        );

        // Also test with counter mode
        let encrypted_counter = encrypt_with_counter(&key, &nonce, 0, plaintext)
            .expect("Counter encryption should succeed");

        let mut tampered_counter = encrypted_counter.clone();
        if tampered_counter.len() > 0 {
            tampered_counter[0] ^= 0xFF; // Flip bits in the ciphertext
        }

        let decrypt_counter_result = decrypt_with_counter(&key, &nonce, 0, &tampered_counter);
        assert!(
            decrypt_counter_result.is_err(),
            "Tampered counter ciphertext should fail"
        );
    }

    #[test]
    fn test_empty_plaintext() {
        // Test encryption/decryption of empty data
        let key = generate_test_key();
        let nonce = generate_test_nonce();
        let plaintext = b"";

        let encrypted = encrypt(&key, &nonce, plaintext).expect("Empty encryption should succeed");
        let decrypted = decrypt(&key, &encrypted).expect("Empty decryption should succeed");

        assert_eq!(decrypted, plaintext.as_slice());
    }

    #[test]
    fn test_large_plaintext() {
        // Test with larger data
        let key = generate_test_key();
        let nonce = generate_test_nonce();
        let plaintext = vec![0x42u8; 10000]; // 10KB of data

        let encrypted = encrypt(&key, &nonce, &plaintext).expect("Large encryption should succeed");
        let decrypted = decrypt(&key, &encrypted).expect("Large decryption should succeed");

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_counter_sequence_uniqueness() {
        // Verify that sequential counters produce unique nonces
        let base_nonce = [0xAA; 12];
        let mut derived_nonces = std::collections::HashSet::new();

        for counter in 0..1000u64 {
            let derived = derive_nonce(&base_nonce, counter);
            assert!(
                derived_nonces.insert(derived),
                "Nonce collision at counter {}",
                counter
            );
        }
    }

    #[test]
    fn test_decrypt_too_short() {
        // Test that very short data fails gracefully
        let key = generate_test_key();
        let short_data = vec![0x00u8; 5]; // Less than 12 bytes (nonce size)

        let result = decrypt(&key, &short_data);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too short"));
    }

    #[test]
    fn test_invalid_hex_key() {
        // Test that invalid hex in key fails gracefully
        let invalid_key = "not_valid_hex!@#$";
        let nonce = generate_test_nonce();
        let plaintext = b"Test";

        let result = encrypt(invalid_key, &nonce, plaintext);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("bad key hex"));
    }

    #[test]
    fn test_invalid_hex_nonce() {
        // Test that invalid hex in nonce fails gracefully
        let key = generate_test_key();
        let invalid_nonce = "not_valid_hex!@#$";
        let plaintext = b"Test";

        let result = encrypt(&key, invalid_nonce, plaintext);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("bad nonce hex"));
    }
}
