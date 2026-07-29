use crate::crypto;
use crate::space::Space;
use uuid::Uuid;
use serde::{Serialize, Deserialize};
use anyhow::{ensure, Context, Result};
use data_encoding::BASE32_NOPAD;
use iroh_docs::api::protocol::{AddrInfoOptions, ShareMode};
use zeroize::{Zeroize, Zeroizing};

const INVITE_MAGIC: u8 = 0x01;
const RELAY_INVITE_MAGIC: u8 = 0x02;

fn tag_and_postcard<T: Serialize>(magic: u8, value: &T) -> Result<Zeroizing<Vec<u8>>> {
    let plaintext = postcard::to_allocvec(value)?;
    let mut tagged = Zeroizing::new(Vec::with_capacity(1 + plaintext.len()));
    tagged.push(magic);
    tagged.extend_from_slice(&plaintext);
    Ok(tagged)
}

fn untag_and_postcard<T: for<'de> Deserialize<'de>>(expected_magic: u8, tagged: &[u8]) -> Result<T> {
    let (magic, rest) = tagged.split_first().context("empty invite payload")?;
    ensure!(
        *magic == expected_magic,
        "wrong invite type for this operation (expected magic {expected_magic:#04x}, got {magic:#04x})"
    );
    Ok(postcard::from_bytes(rest)?)
}

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
        let space_key = space
            .key()
            .context("cannot create a member invite for a space with no key (blind relay?)")?;

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
            space_key: **space_key,
        })
    }

    /// Serialize the invite struct
    pub fn serialize_invite(&self) -> Result<String> {
        let tagged = tag_and_postcard(INVITE_MAGIC, self)?;
        Ok(BASE32_NOPAD.encode(&tagged))
    }

    /// Deserialize the Invite from a string
    pub fn deserialize_invite(data: impl Into<String>) -> Result<Self> {
        let data = data.into();
        let tagged: Zeroizing<Vec<u8>> = Zeroizing::new(
            BASE32_NOPAD
                .decode(data.as_bytes())
                .map_err(|e| anyhow::anyhow!("invalid base32 invite: {e}"))?
        );

        untag_and_postcard(INVITE_MAGIC, &tagged)
    }

    /// Serialize and encrypt the invite for local, on-disk storage.
    /// Unlike `serialize_invite`, this is not meant to be human-shared.
    pub fn serialize_invite_encrypted(&self, key: &[u8; 32]) -> Result<Vec<u8>> {
        let tagged = tag_and_postcard(INVITE_MAGIC, self)?;
        crypto::encrypt(key, &tagged, &[])
    }

    /// Reverse of `serialize_invite_encrypted`.
    pub fn deserialize_invite_encrypted(data: &[u8], key: &[u8; 32]) -> Result<Self> {
        let tagged = crypto::decrypt(key, data, &[])?;
        untag_and_postcard(INVITE_MAGIC, &tagged)
    }
}

#[derive(Serialize, Deserialize)]
pub struct RelayInvite {
    pub space_id: Uuid,
    pub space_name: String,
    pub info_ticket: String,
    pub member_ticket: String,
}

impl RelayInvite {
    pub async fn from_space(space: &Space) -> Result<Self> {
        let info_ticket = space.info.share(ShareMode::Read, AddrInfoOptions::RelayAndAddresses).await?;
        let member_ticket = space.members.share(ShareMode::Read, AddrInfoOptions::RelayAndAddresses).await?;

        Ok(Self {
            space_id: space.id(),
            space_name: space.name().to_string(),
            info_ticket: info_ticket.to_string(),
            member_ticket: member_ticket.to_string(),
        })
    }

    pub fn serialize_invite(&self) -> Result<String> {
        let tagged = tag_and_postcard(RELAY_INVITE_MAGIC, self)?;
        Ok(BASE32_NOPAD.encode(&tagged))
    }

    pub fn deserialize_invite(data: impl Into<String>) -> Result<Self> {
        let data = data.into();
        let tagged: Zeroizing<Vec<u8>> = Zeroizing::new(
            BASE32_NOPAD
                .decode(data.as_bytes())
                .map_err(|e| anyhow::anyhow!("invalid base32 invite: {e}"))?
        );

        untag_and_postcard(RELAY_INVITE_MAGIC, &tagged)
    }

    pub fn serialize_invite_encrypted(&self, key: &[u8; 32]) -> Result<Vec<u8>> {
        let tagged = tag_and_postcard(RELAY_INVITE_MAGIC, self)?;
        crypto::encrypt(key, &tagged, &[])
    }

    pub fn deserialize_invite_encrypted(data: &[u8], key: &[u8; 32]) -> Result<Self> {
        let tagged = crypto::decrypt(key, data, &[])?;
        untag_and_postcard(RELAY_INVITE_MAGIC, &tagged)
    }
}
