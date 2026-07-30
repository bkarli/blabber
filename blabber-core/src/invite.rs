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

/// turns value into binary blob with one byte tag prefix.
/// Used for both types of invites.
fn tag_and_postcard<T: Serialize>(magic: u8, value: &T) -> Result<Zeroizing<Vec<u8>>> {
    let plaintext = postcard::to_allocvec(value)?;
    let mut tagged = Zeroizing::new(Vec::with_capacity(1 + plaintext.len()));
    tagged.push(magic);
    tagged.extend_from_slice(&plaintext);
    Ok(tagged)
}

/// reads invite and enforces srules for the prefix tags
fn untag_and_postcard<T: for<'de> Deserialize<'de>>(expected_magic: u8, tagged: &[u8]) -> Result<T> {
    let (magic, rest) = tagged.split_first().context("empty invite payload")?;
    ensure!(
        *magic == expected_magic,
        "wrong invite type for this operation (expected magic {expected_magic:#04x}, got {magic:#04x})"
    );
    Ok(postcard::from_bytes(rest)?)
}

/// produces ticket a user copies
fn serialize_tagged<T: Serialize>(magic: u8, value: &T) -> Result<String> {
    let tagged = tag_and_postcard(magic, value)?;
    Ok(BASE32_NOPAD.encode(&tagged))
}

/// turns pasted invites into raw bytes
fn deserialize_tagged<T: for<'de> Deserialize<'de>>(magic: u8, data: impl Into<String>) -> Result<T> {
    let data = data.into();
    let tagged: Zeroizing<Vec<u8>> = Zeroizing::new(
        BASE32_NOPAD
            .decode(data.as_bytes())
            .map_err(|e| anyhow::anyhow!("invalid base32 invite: {e}"))?,
    );
    untag_and_postcard(magic, &tagged)
}

/// storage path for on-disk invites. not for human sharing.
fn serialize_tagged_encrypted<T: Serialize>(magic: u8, value: &T, key: &[u8; 32]) -> Result<Vec<u8>> {
    let tagged = tag_and_postcard(magic, value)?;
    crypto::encrypt(key, &tagged, &[])
}

/// decrypts the bytes read from disk and passes invite on
fn deserialize_tagged_encrypted<T: for<'de> Deserialize<'de>>(magic: u8, data: &[u8], key: &[u8; 32]) -> Result<T> {
    let tagged = crypto::decrypt(key, data, &[])?;
    untag_and_postcard(magic, &tagged)
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
    /// builds the invite ticket carrying the decryption key
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

    /// human shareable path
    pub fn serialize_invite(&self) -> Result<String> {
        serialize_tagged(INVITE_MAGIC, self)
    }

    pub fn deserialize_invite(data: impl Into<String>) -> Result<Self> {
        deserialize_tagged(INVITE_MAGIC, data)
    }

    /// Encrypted for local on disk storage unlike `serialize_invite`, not meant to be human-shared.
    pub fn serialize_invite_encrypted(&self, key: &[u8; 32]) -> Result<Vec<u8>> {
        serialize_tagged_encrypted(INVITE_MAGIC, self, key)
    }

    pub fn deserialize_invite_encrypted(data: &[u8], key: &[u8; 32]) -> Result<Self> {
        deserialize_tagged_encrypted(INVITE_MAGIC, data, key)
    }
}

/// A read-only invite for a blind relay: no space_key field, so
/// one can't be mistaken for (or upgraded into) a full member Invite
/// tagged with a distinct magic byte so mixing the two up fails loudly.
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
        // Write, not Read: a relay has no space key and so can never write
        // a valid *encrypted* member/ record, but it does need write access
        // to publish its own cleartext relay/ presence entry (see
        // `Space::insert_self_as_relay`). This grants doc-level write, not
        // decryption - the relay still can't read or forge real content.
        let member_ticket = space.members.share(ShareMode::Write, AddrInfoOptions::RelayAndAddresses).await?;

        Ok(Self {
            space_id: space.id(),
            space_name: space.name().to_string(),
            info_ticket: info_ticket.to_string(),
            member_ticket: member_ticket.to_string(),
        })
    }

    pub fn serialize_invite(&self) -> Result<String> {
        serialize_tagged(RELAY_INVITE_MAGIC, self)
    }

    pub fn deserialize_invite(data: impl Into<String>) -> Result<Self> {
        deserialize_tagged(RELAY_INVITE_MAGIC, data)
    }

    pub fn serialize_invite_encrypted(&self, key: &[u8; 32]) -> Result<Vec<u8>> {
        serialize_tagged_encrypted(RELAY_INVITE_MAGIC, self, key)
    }

    pub fn deserialize_invite_encrypted(data: &[u8], key: &[u8; 32]) -> Result<Self> {
        deserialize_tagged_encrypted(RELAY_INVITE_MAGIC, data, key)
    }
}
