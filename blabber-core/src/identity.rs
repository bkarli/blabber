// Create store and manage Identities
use rand::prelude::*;
use anyhow::{anyhow, Context, Result};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use argon2::Argon2;

const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;


/// Hold the displayName as well as the
/// secret for generating the same node
/// every time. 
///
/// TODO: We should mlock this one and use Zeroing
pub struct Identity {
    pub displayName: String,
    pub secret: [u8; 32],
}

impl Identity {
    
    /// Create a new Identity from username and password
    pub fn new(displayName: impl Into<String>) -> Self {
        // create a random secret
        let secret: [u8; 32] = rand::rng().random();
        Self {
            displayName: displayName.into(),
            secret: secret,
        }

    }
    
    fn derive_key(password: &[u8], salt: &[u8]) -> Result<[u8; KEY_LEN]> {
        let mut key = [0u8; KEY_LEN];
        Argon2::default()
            .hash_password_into(password, salt, &mut key)
            .map_err(|e| anyhow!("Key derivation failed: {e}"))?;
        Ok(key)
    }

    /// store the identity and encrypt with password
    fn store(&self, password: impl Into<String> + Copy) -> Result<()> {
        let salt: [u8; SALT_LEN] = rand::rng().random();
        let nonce_bytes: [u8; NONCE_LEN] = rand::rng().random();

        let mut key = Self::derive_key(password.into().as_bytes(), &salt)?;
        let cipher = ChaCha20Poly1305::new((&key).into());
        let nonce = Nonce::from_slice(&nonce_bytes);
        
        let plaintext = format!("{}\n{}",self.displayName,password.into());
        let plaintext_bytes = plaintext.as_bytes();

        let ciphertext = cipher
            .encrypt(nonce, plaintext_bytes)
            .map_err(|e| anyhow!("encryption failed: {e}"))?;

        
        Ok(())
    }
    //
    // /// load the identity 
    // pub fn load_from_disk(path: PathBuf) {
    //
    // }

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
        assert_ne!(id.secret, [0u8; 32]);

    }

    #[test]
    fn store_identity_test() {
        let id = Identity::new("Alice").store("SecurePassword");
    }

    #[test]
    fn load_identity_test() {}
}
