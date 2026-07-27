use crate::crypto;
use crate::space::Space;
use uuid::Uuid;
use serde::{Serialize, Deserialize};
use anyhow::Result;
use data_encoding::BASE32_NOPAD;
use iroh_docs::api::protocol::{AddrInfoOptions, ShareMode};
use zeroize::{Zeroize, Zeroizing};


#[derive(Serialize, Deserialize)]
pub struct Invite {
    pub space_id: Uuid,
    pub space_name: String,
    pub info_ticket: String,
    pub member_ticket: String,
    pub space_key: [u8; 32],
}

impl Drop for Invite {
    fn drop(&mut self) {
        self.space_key.zeroize();
    }
}

impl Invite {
    /// Create a invite string from a space
    pub async fn from_space(space: &Space) -> Result<Self> {
        let info_ticket = space.info.share(ShareMode::Read, AddrInfoOptions::RelayAndAddresses).await?;
        let member_ticket = space.members.share(ShareMode::Write, AddrInfoOptions::RelayAndAddresses).await?;

        Ok(Self {
            space_id: space.id(),
            space_name: space.name().to_string(),
            info_ticket: info_ticket.to_string(),
            member_ticket: member_ticket.to_string(),
            // Deliberate, minimal boundary copy: `Space` holds the key behind a
            // shared `Arc<Zeroizing<...>>`, but `Invite` needs an owned plain
            // array to be postcard-serializable into the shareable ticket.
            space_key: **space.key(),
        })
    }

    /// Serialize the invite struct
    pub fn serialize_invite(&self) -> Result<String> {
        let bytes: Zeroizing<Vec<u8>> = Zeroizing::new(postcard::to_allocvec(self)?);

        Ok(BASE32_NOPAD.encode(&bytes))
    }

    /// Deserialize the Invite from a string
    pub fn deserialize_invite(data: impl Into<String>) -> Result<Self> {
        let data = data.into();
        let bytes: Zeroizing<Vec<u8>> = Zeroizing::new(
            BASE32_NOPAD
                .decode(data.as_bytes())
                .map_err(|e| anyhow::anyhow!("invalid base32 invite: {e}"))?
        );

        let invite = postcard::from_bytes(&bytes)?;
        Ok(invite)
    }

    /// Serialize and encrypt the invite for local, on-disk storage.
    /// Unlike `serialize_invite`, this is not meant to be human-shared.
    pub fn serialize_invite_encrypted(&self, key: &[u8; 32]) -> Result<Vec<u8>> {
        let bytes: Zeroizing<Vec<u8>> = Zeroizing::new(postcard::to_allocvec(self)?);
        crypto::encrypt(key, &bytes, &[])
    }

    /// Reverse of `serialize_invite_encrypted`.
    pub fn deserialize_invite_encrypted(data: &[u8], key: &[u8; 32]) -> Result<Self> {
        let bytes = crypto::decrypt(key, data, &[])?;
        let invite = postcard::from_bytes(&bytes)?;
        Ok(invite)
    }
}
