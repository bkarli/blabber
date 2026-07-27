use anyhow::{anyhow, ensure, Result};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    ChaCha20Poly1305, Nonce,
};
use rand::prelude::*;

const NONCE_LEN: usize = 12;

pub fn encrypt(key: &[u8; 32], plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new(key.into());
    let nonce_bytes: [u8; NONCE_LEN] = rand::rng().random();
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, Payload { msg: plaintext, aad })
        .map_err(|e| anyhow!("encryption failed: {e}"))?;

    let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

pub fn decrypt(key: &[u8; 32], data: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
    ensure!(data.len() > NONCE_LEN, "ciphertext too short");
    let (nonce_bytes, ciphertext) = data.split_at(NONCE_LEN);

    let cipher = ChaCha20Poly1305::new(key.into());
    let nonce = Nonce::from_slice(nonce_bytes);

    cipher
        .decrypt(nonce, Payload { msg: ciphertext, aad })
        .map_err(|e| anyhow!("decryption failed: {e}"))
}