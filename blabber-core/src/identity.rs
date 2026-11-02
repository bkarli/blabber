use rand::prelude::*;
use anyhow::{anyhow, ensure, Context, Result};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use argon2::Argon2;
use std::path::Path;
use zeroize::Zeroizing;

use crate::secret::LockedSecret;
use crate::storage::atomic_write;

const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;

/// A user's display name plus the secret their node identity and all
/// per-space authors are derived from.
pub struct Identity {
    pub display_name: String,
    pub secret: LockedSecret,
}

impl Identity {
    pub fn new(display_name: impl Into<String>) -> Self {
        Self {
            display_name: display_name.into(),
            secret: LockedSecret::generate_random(), // fresh crypto secure 32 byte value (page locked)
        }
    }

    /// helper function to turn user password into stronger actual password.
    fn derive_key(password: &[u8], salt: &[u8]) -> Result<Zeroizing<[u8; KEY_LEN]>> {
        let mut key = Zeroizing::new([0u8; KEY_LEN]);
        Argon2::default()
            .hash_password_into(password, salt, key.as_mut())
            .map_err(|e| anyhow!("key derivation failed: {e}"))?;
        Ok(key)
    }

    /// On-disk layout: `salt | nonce | encrypt(name_len:u8 | name | secret:32)`.
    /// The `u8` length prefix caps display names at 255 bytes.
    /// used to encrypt and persist the identity on disk. The parent directory
    /// must already exist.
    pub fn store(&self, password: &str, path: impl AsRef<Path>) -> Result<()> {
        atomic_write(path.as_ref(), &self.to_encrypted_bytes(password)?)
            .context("failed to write identity file")
    }

    fn to_encrypted_bytes(&self, password: &str) -> Result<Vec<u8>> {
        let salt: [u8; SALT_LEN] = rand::rng().random();
        let nonce_bytes: [u8; NONCE_LEN] = rand::rng().random();

        let key = Self::derive_key(password.as_bytes(), &salt)?;
        let cipher = ChaCha20Poly1305::new((&*key).into());
        let nonce = Nonce::from_slice(&nonce_bytes);

        let name_bytes = self.display_name.as_bytes();
        let mut plaintext = Zeroizing::new(Vec::new());
        plaintext.push(name_bytes.len() as u8);
        plaintext.extend_from_slice(name_bytes);
        plaintext.extend_from_slice(self.secret.as_bytes());

        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_slice())
            .map_err(|e| anyhow!("encryption failed: {e}"))?;

        let mut bytes = Vec::with_capacity(SALT_LEN + NONCE_LEN + ciphertext.len());
        bytes.extend_from_slice(&salt);
        bytes.extend_from_slice(&nonce_bytes);
        bytes.extend_from_slice(&ciphertext);
        Ok(bytes)
    }

    /// used to load the identity from disk
    pub fn load_from_disk(path: impl AsRef<Path>, password: &str) -> Result<Self> {
        let bytes = std::fs::read(path).context("failed to open file")?;
        Self::from_encrypted_bytes(&bytes, password)
    }

    fn from_encrypted_bytes(bytes: &[u8], password: &str) -> Result<Self> {
        ensure!(bytes.len() >= SALT_LEN + NONCE_LEN, "identity file is truncated");

        let (salt, rest) = bytes.split_at(SALT_LEN);
        let (nonce_bytes, ciphertext) = rest.split_at(NONCE_LEN);

        let key = Self::derive_key(password.as_bytes(), salt)?;
        let cipher = ChaCha20Poly1305::new((&*key).into());
        let nonce = Nonce::from_slice(nonce_bytes);
        let plaintext: Zeroizing<Vec<u8>> = Zeroizing::new(
            cipher
                .decrypt(nonce, ciphertext)
                .map_err(|e| anyhow!("decryption failed (wrong password?): {e}"))?,
        );

        let name_len = plaintext[0] as usize;
        let display_name = String::from_utf8(plaintext[1..1 + name_len].to_vec())
            .context("display name is not valid UTF-8")?;

        let offset = 1 + name_len;
        let mut secret_bytes = Zeroizing::new([0u8; 32]);
        secret_bytes.copy_from_slice(&plaintext[offset..offset + 32]);
        let secret = LockedSecret::from_bytes(&secret_bytes);

        Ok(Self { display_name, secret })
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_identity_test() {
        let id = Identity::new("Alice");

        assert_eq!(id.display_name, "Alice");
        assert_ne!(id.secret.as_bytes(), &[0u8; 32]);
    }

    #[test]
    fn store_identity_test() {
        let dir = tempfile::tempdir().expect("failed to create tempdir");
        let path = dir.path().join("test_identity.bin");
        Identity::new("Alice").store("SecurePassword", &path).unwrap();
    }

    #[test]
    fn truncated_identity_file_returns_an_error_instead_of_panicking() {
        let dir = tempfile::tempdir().expect("failed to create tempdir");
        let path = dir.path().join("test_identity.bin");
        std::fs::write(&path, [0u8; SALT_LEN - 1]).unwrap();
        assert!(Identity::load_from_disk(&path, "SecurePassword").is_err());
    }

    #[test]
    fn load_identity_test() {
        let dir = tempfile::tempdir().expect("failed to create tempdir");
        let path = dir.path().join("test_identity.bin");
        let id = Identity::new("Alice");
        id.store("SecurePassword", &path).unwrap();
        let loaded = Identity::load_from_disk(&path, "SecurePassword").unwrap();
        assert_eq!(id.display_name, loaded.display_name);
        assert_eq!(id.secret.as_bytes(), loaded.secret.as_bytes());
    }
}
