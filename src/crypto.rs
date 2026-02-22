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

/// Encrypt `plaintext` with AES-256-GCM.
///
/// `key_hex`   — 32-byte key encoded as 64 hex chars (from `CryptoHop`)
/// `nonce_hex` — 12-byte nonce encoded as 24 hex chars (from `CryptoHop`)
///
/// Returns `[nonce (12 bytes) || ciphertext + tag]` so the receiver can
/// always find the nonce even if it rotates later.
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
