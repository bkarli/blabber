use iroh_docs::{Author, AuthorId};
// Create store and manage Identities
use rand::prelude::*;
use anyhow::{anyhow, Context, Result};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use argon2::Argon2;
use std::{path::Path, str::FromStr};
use std::fs::File;
use std::io::Write;
use std::io::Read;
use zeroize::{Zeroize, Zeroizing};

use crate::secret::LockedSecret;

const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;


/// Hold the displayName as well as the
/// secret for generating the same node
/// every time.
pub struct Identity {
    pub displayName: String,
    pub secret: LockedSecret,
    pub author: Option<AuthorId>
}

pub fn sanitize_path_component(name: &str) -> Option<String> {
    let sanitized: String = name
        .chars()
        .filter(|character| character.is_alphanumeric() || *character == '-' || *character == '_')
        .collect();
    if sanitized.is_empty() {
        None
    } else {
        Some(sanitized)
    }
}

impl Identity {

    /// Create a new Identity from username and password
    pub fn new(displayName: impl Into<String>) -> Self {
        Self {
            displayName: displayName.into(),
            secret: LockedSecret::generate_random(),
            author: None,
        }

    }

    /// Rebuilds this identity with a newly-known author, reusing the same
    /// underlying secret bytes rather than regenerating them.
    pub fn with_author(&self, author: AuthorId) -> Self {
        Self {
            displayName: self.displayName.clone(),
            secret: LockedSecret::from_bytes(self.secret.as_bytes()),
            author: Some(author),
        }
    }

    fn derive_key(password: &[u8], salt: &[u8]) -> Result<Zeroizing<[u8; KEY_LEN]>> {
        let mut key = Zeroizing::new([0u8; KEY_LEN]);
        Argon2::default()
            .hash_password_into(password, salt, key.as_mut())
            .map_err(|e| anyhow!("Key derivation failed: {e}"))?;
        Ok(key)
    }

    /// store the identity and encrypt with password
    pub fn store(&self, password: &str, path: impl AsRef<Path>) -> Result<()> {
        let password_bytes = password.as_bytes();

        let salt: [u8; SALT_LEN] = rand::rng().random();
        let nonce_bytes: [u8; NONCE_LEN] = rand::rng().random();

        let key = Self::derive_key(password_bytes, &salt)?;
        let cipher = ChaCha20Poly1305::new((&*key).into());
        let nonce = Nonce::from_slice(&nonce_bytes);

        let name_bytes = self.displayName.as_bytes();
        let mut plaintext = Zeroizing::new(Vec::new());
        plaintext.push(name_bytes.len() as u8);
        plaintext.extend_from_slice(name_bytes);
        plaintext.extend_from_slice(self.secret.as_bytes());

        // Add author to the file
        match &self.author {
            Some(author) => {
                let author_bytes = author.to_bytes();
                plaintext.push(1u8); // signal that an author is present
                plaintext.extend_from_slice(&author_bytes);
            }
            None => {
                plaintext.push(0u8); // signal that no author present
            }

        }

        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_slice())
            .map_err(|e| anyhow!("encryption failed: {e}"))?;

        let mut file = File::create(path).context("failed to create file")?;
        file.write_all(&salt)?;
        file.write_all(&nonce_bytes)?;
        file.write_all(&ciphertext)?;
        Ok(())
    }


    //load the identity
    pub fn load_from_disk(path: impl AsRef<Path>, password: &str) -> Result<Self> {
        let password_bytes = password.as_bytes();
        let mut file = File::open(path).context("failed to open file")?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;

        let (salt, rest) = buffer.split_at(SALT_LEN);
        let (nonce_bytes, ciphertext) = rest.split_at(NONCE_LEN);

        let key = Self::derive_key(password_bytes, salt)?;
        let cipher = ChaCha20Poly1305::new((&*key).into());
        let nonce = Nonce::from_slice(nonce_bytes);
        let plaintext: Zeroizing<Vec<u8>> = Zeroizing::new(
            cipher.decrypt(nonce, ciphertext).map_err(|e| anyhow!("decryption failed->wrong password"))?
        );

        let name_len = plaintext[0] as usize;
        let displayName = String::from_utf8(plaintext[1..1 + name_len].to_vec())
        .context("display name is not valid UTF-8")?;



        let mut offset = 1 + name_len;
        let mut secret_bytes = [0u8; 32];
        secret_bytes.copy_from_slice(&plaintext[offset..offset + 32]);
        let secret = LockedSecret::from_bytes(&secret_bytes);
        secret_bytes.zeroize();

        offset += 32;

        let author = match plaintext.get(offset) {
            Some(1) => {
                offset += 1;
                let mut author_bytes = [0u8; 32];
                author_bytes.copy_from_slice(&plaintext[offset..offset + 32]);
                Some(AuthorId::from(author_bytes))
            }
            _ => None,
        };

        Ok( Self{
            displayName,
            secret,
            author: author
        })
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_identity_test() {
        let id = Identity::new("Alice");

        // check if the display Name matches
        assert_eq!(id.displayName, "Alice");

        // check if the secret generated is actually random byte array
        assert_ne!(id.secret.as_bytes(), &[0u8; 32]);

    }

    #[test]
    fn store_identity_test() {
        let dir = tempfile::tempdir().expect("failed to create tempdir");
        let path = dir.path().join("test_identity.bin");
        Identity::new("Alice").store("SecurePassword", &path).unwrap();
    }

    #[test]
    fn load_identity_test() {
        let dir = tempfile::tempdir().expect("failed to create tempdir");
        let path = dir.path().join("test_identity.bin");
        let id = Identity::new("Alice");
        id.store("SecurePassword", &path).unwrap();
        let loaded = Identity::load_from_disk(&path, "SecurePassword").unwrap();
        assert_eq!(id.displayName, loaded.displayName);
        assert_eq!(id.secret.as_bytes(), loaded.secret.as_bytes());
    }
}
